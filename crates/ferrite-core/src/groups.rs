//! Durable project-scoped Thread groups.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::Store;
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

const SCHEMA_VERSION: u32 = 1;
const FILE_NAME: &str = "groups.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(u64);

impl GroupId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: GroupId,
    pub title: String,
    pub members: Vec<ThreadId>,
}

impl Group {
    pub fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            format!("group-{:02}", self.id.0)
        } else {
            self.title.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupChange {
    Create {
        first: ThreadId,
        second: ThreadId,
    },
    Join {
        thread: ThreadId,
        group: GroupId,
        index: Option<usize>,
    },
    Leave {
        thread: ThreadId,
    },
    ReorderMember {
        group: GroupId,
        thread: ThreadId,
        index: usize,
    },
    MoveGroup {
        group: GroupId,
        index: usize,
    },
    Rename {
        group: GroupId,
        title: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    Thread {
        thread: ThreadId,
        group: Option<GroupId>,
    },
    Group(GroupId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    GroupHeader(GroupId),
    ThreadRow {
        thread: ThreadId,
        group: Option<GroupId>,
        index: usize,
    },
    GroupGap(usize),
    LooseZone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Nothing,
    Change(GroupChange),
    Refused(String),
}

/// Translate pointer geometry into the same mutation language as keys.
pub fn plan_drop(drag: Drag, target: DropTarget) -> Plan {
    match (drag, target) {
        (Drag::Thread { thread, .. }, DropTarget::ThreadRow { thread: target, .. })
            if thread == target =>
        {
            Plan::Nothing
        }
        (
            Drag::Thread {
                group: Some(source),
                ..
            },
            DropTarget::GroupHeader(target),
        ) if source == target => Plan::Nothing,
        (
            Drag::Thread { thread, .. },
            DropTarget::ThreadRow {
                thread: target,
                group: None,
                ..
            },
        ) => Plan::Change(GroupChange::Create {
            first: target,
            second: thread,
        }),
        (Drag::Thread { thread, .. }, DropTarget::GroupHeader(group)) => {
            Plan::Change(GroupChange::Join {
                thread,
                group,
                index: None,
            })
        }
        (
            Drag::Thread {
                thread,
                group: Some(source),
            },
            DropTarget::ThreadRow {
                group: Some(target),
                index,
                ..
            },
        ) if source == target => Plan::Change(GroupChange::ReorderMember {
            group: target,
            thread,
            index,
        }),
        (
            Drag::Thread { thread, .. },
            DropTarget::ThreadRow {
                group: Some(group),
                index,
                ..
            },
        ) => Plan::Change(GroupChange::Join {
            thread,
            group,
            index: Some(index),
        }),
        (Drag::Thread { thread, .. }, DropTarget::LooseZone) => {
            Plan::Change(GroupChange::Leave { thread })
        }
        (Drag::Group(group), DropTarget::GroupGap(index)) => {
            Plan::Change(GroupChange::MoveGroup { group, index })
        }
        (Drag::Group(_), _) | (Drag::Thread { .. }, DropTarget::GroupGap(_)) => {
            Plan::Refused("not a valid drop target".into())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dissolved {
    pub group: GroupId,
    pub survivor: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    pub group: Option<GroupId>,
    pub dissolved: Vec<Dissolved>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    Io(String),
    MissingGroup,
    CrossProject,
    MissingProject,
    SameThread,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::MissingGroup => write!(f, "group no longer exists"),
            Self::CrossProject => write!(f, "Threads from different projects cannot be grouped"),
            Self::MissingProject => write!(f, "Thread project metadata is missing"),
            Self::SameThread => write!(f, "a Thread cannot be grouped with itself"),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<io::Error> for ApplyError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedGroups {
    version: u32,
    groups: Vec<PersistedGroup>,
}

#[derive(Serialize, Deserialize)]
struct PersistedGroup {
    id: GroupId,
    title: String,
    members: Vec<u64>,
}

/// The one owning module for membership and its durable order.
#[derive(Clone)]
pub struct Groups {
    dir: PathBuf,
    groups: Vec<Group>,
    next_id: u64,
}

impl Groups {
    pub fn load(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(FILE_NAME);
        let persisted = match fs::read(&path) {
            Ok(bytes) => Some(
                serde_json::from_slice::<PersistedGroups>(&bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if let Some(persisted) = &persisted {
            if persisted.version != SCHEMA_VERSION {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("groups version {} is not supported", persisted.version),
                ));
            }
        }
        let store = Store::open(dir)?;
        let known: BTreeSet<ThreadId> = store.thread_ids()?.into_iter().collect();
        let mut claimed = BTreeSet::new();
        let mut healed = false;
        let mut groups = Vec::new();
        for stored in persisted
            .as_ref()
            .into_iter()
            .flat_map(|value| &value.groups)
        {
            let before = stored.members.len();
            let members: Vec<_> = stored
                .members
                .iter()
                .copied()
                .map(ThreadId::new)
                .filter_map(|thread| {
                    if !known.contains(&thread) {
                        return None;
                    }
                    if let Err(error) = store.peek(thread) {
                        return Some(Err(io::Error::new(io::ErrorKind::InvalidData, error)));
                    }
                    Some(Ok(thread))
                })
                .collect::<io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|thread| claimed.insert(*thread))
                .collect();
            healed |= members.len() != before;
            if members.len() < 2 {
                healed = true;
                continue;
            }
            groups.push(Group {
                id: stored.id,
                title: stored.title.clone(),
                members,
            });
        }
        let next_id = groups.iter().map(|group| group.id.0).max().unwrap_or(0) + 1;
        let this = Self {
            dir: dir.to_path_buf(),
            groups,
            next_id,
        };
        if healed {
            this.persist()?;
        }
        Ok(this)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Group> {
        self.groups.iter()
    }

    pub fn get(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }

    pub fn of(&self, thread: ThreadId) -> Option<&Group> {
        self.groups
            .iter()
            .find(|group| group.members.contains(&thread))
    }

    pub fn apply(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        self.validate_change(&change)?;
        let before = self.groups.clone();
        let before_id = self.next_id;
        let result = self.apply_in_memory(change);
        if result.is_err() {
            self.groups = before;
            self.next_id = before_id;
            return result;
        }
        if let Err(error) = self.persist() {
            self.groups = before;
            self.next_id = before_id;
            return Err(error.into());
        }
        result
    }

    pub(crate) fn restore_snapshot(&mut self, snapshot: Self) -> Result<(), ApplyError> {
        snapshot.persist()?;
        *self = snapshot;
        Ok(())
    }

    pub fn preview_drop(&self, drag: Drag, target: DropTarget) -> Plan {
        match plan_drop(drag, target) {
            Plan::Change(change) => {
                let mut preview = self.clone();
                if let Err(error) = preview
                    .validate_change(&change)
                    .and_then(|_| preview.apply_in_memory(change.clone()))
                {
                    Plan::Refused(error.to_string())
                } else {
                    Plan::Change(change)
                }
            }
            plan => plan,
        }
    }

    fn validate_change(&self, change: &GroupChange) -> Result<(), ApplyError> {
        self.validate_all()?;
        match &change {
            GroupChange::Create { first, second } => {
                self.require_thread_project(*first)?;
                self.require_thread_project(*second)?;
            }
            GroupChange::Join { thread, .. } => {
                self.require_thread_project(*thread)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Validate a draft's project against a group before its Thread exists.
    pub(crate) fn validate_join_project(
        &self,
        group: GroupId,
        project: Option<ProjectId>,
    ) -> Result<(), ApplyError> {
        self.validate_all()?;
        let Some(project) = project else {
            return Err(ApplyError::MissingProject);
        };
        let target = self.get(group).ok_or(ApplyError::MissingGroup)?;
        if let Some(first) = target.members.first().copied() {
            match self.thread_project(first)? {
                Some(group_project) if group_project == project => {}
                Some(_) => return Err(ApplyError::CrossProject),
                None => return Err(ApplyError::MissingProject),
            }
        }
        Ok(())
    }

    fn apply_in_memory(&mut self, change: GroupChange) -> Result<Applied, ApplyError> {
        match change {
            GroupChange::Create { first, second } => {
                if first == second {
                    return Err(ApplyError::SameThread);
                }
                self.ensure_same_project(first, second)?;
                let mut dissolved = self.detach(first).into_iter().collect::<Vec<_>>();
                dissolved.extend(self.detach(second));
                let id = GroupId(self.next_id);
                self.next_id += 1;
                self.groups.push(Group {
                    id,
                    title: String::new(),
                    members: vec![first, second],
                });
                Ok(Applied {
                    group: Some(id),
                    dissolved,
                })
            }
            GroupChange::Join {
                thread,
                group,
                index,
            } => {
                let target = self.get(group).ok_or(ApplyError::MissingGroup)?;
                if let Some(first) = target.members.first().copied() {
                    self.ensure_same_project(thread, first)?;
                }
                let dissolved = self.detach(thread).into_iter().collect();
                let target = self
                    .groups
                    .iter_mut()
                    .find(|item| item.id == group)
                    .ok_or(ApplyError::MissingGroup)?;
                let at = index
                    .unwrap_or(target.members.len())
                    .min(target.members.len());
                target.members.insert(at, thread);
                Ok(Applied {
                    group: Some(group),
                    dissolved,
                })
            }
            GroupChange::Leave { thread } => Ok(Applied {
                group: None,
                dissolved: self.detach(thread).into_iter().collect(),
            }),
            GroupChange::ReorderMember {
                group,
                thread,
                index,
            } => {
                let target = self
                    .groups
                    .iter_mut()
                    .find(|item| item.id == group)
                    .ok_or(ApplyError::MissingGroup)?;
                let old = target
                    .members
                    .iter()
                    .position(|item| *item == thread)
                    .ok_or(ApplyError::MissingGroup)?;
                target.members.remove(old);
                let at = index.min(target.members.len());
                target.members.insert(at, thread);
                Ok(Applied {
                    group: Some(group),
                    dissolved: Vec::new(),
                })
            }
            GroupChange::MoveGroup { group, index } => {
                let old = self
                    .groups
                    .iter()
                    .position(|item| item.id == group)
                    .ok_or(ApplyError::MissingGroup)?;
                let item = self.groups.remove(old);
                let at = group_gap_after_removal(old, index, self.groups.len());
                self.groups.insert(at, item);
                Ok(Applied {
                    group: Some(group),
                    dissolved: Vec::new(),
                })
            }
            GroupChange::Rename { group, title } => {
                let target = self
                    .groups
                    .iter_mut()
                    .find(|item| item.id == group)
                    .ok_or(ApplyError::MissingGroup)?;
                target.title = title.trim().to_string();
                Ok(Applied {
                    group: Some(group),
                    dissolved: Vec::new(),
                })
            }
        }
    }

    fn detach(&mut self, thread: ThreadId) -> Option<Dissolved> {
        let index = self
            .groups
            .iter()
            .position(|group| group.members.contains(&thread))?;
        self.groups[index]
            .members
            .retain(|member| *member != thread);
        if self.groups[index].members.len() < 2 {
            let group = self.groups.remove(index);
            Some(Dissolved {
                group: group.id,
                survivor: group.members[0],
            })
        } else {
            None
        }
    }

    fn ensure_same_project(&self, a: ThreadId, b: ThreadId) -> Result<(), ApplyError> {
        match (self.thread_project(a)?, self.thread_project(b)?) {
            (Some(a), Some(b)) if a == b => Ok(()),
            (None, _) | (_, None) => Err(ApplyError::MissingProject),
            _ => Err(ApplyError::CrossProject),
        }
    }

    fn thread_project(&self, thread: ThreadId) -> Result<Option<ProjectId>, ApplyError> {
        let store = Store::open(&self.dir)?;
        store
            .peek(thread)
            .map(|meta| meta.project_id)
            .map_err(|error| ApplyError::Io(error.to_string()))
    }

    fn require_thread_project(&self, thread: ThreadId) -> Result<ProjectId, ApplyError> {
        self.thread_project(thread)?
            .ok_or(ApplyError::MissingProject)
    }

    fn validate_all(&self) -> Result<(), ApplyError> {
        let store = Store::open(&self.dir)?;
        let known: BTreeSet<_> = store.thread_ids()?.into_iter().collect();
        for thread in self.groups.iter().flat_map(|group| &group.members) {
            if !known.contains(thread) {
                return Err(ApplyError::Io(format!("Thread {thread} no longer exists")));
            }
            store
                .peek(*thread)
                .map_err(|error| ApplyError::Io(error.to_string()))?;
        }
        Ok(())
    }

    fn persist(&self) -> io::Result<()> {
        let value = PersistedGroups {
            version: SCHEMA_VERSION,
            groups: self
                .groups
                .iter()
                .map(|group| PersistedGroup {
                    id: group.id,
                    title: group.title.clone(),
                    members: group.members.iter().map(|thread| thread.get()).collect(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
        let temporary = self.dir.join(format!(".{FILE_NAME}.tmp"));
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, self.dir.join(FILE_NAME))
    }
}

fn group_gap_after_removal(old: usize, gap: usize, remaining: usize) -> usize {
    gap.saturating_sub(usize::from(gap > old)).min(remaining)
}

/// Near-square row × column packing, with the long edge horizontal.
pub fn grid(count: usize) -> (usize, usize) {
    if count == 0 {
        return (0, 0);
    }
    let floor = (count as f64).sqrt() as usize;
    let columns = if floor < count.div_ceil(floor.max(1)) {
        floor + 1
    } else {
        floor.max(1)
    };
    (count.div_ceil(columns), columns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Provider;
    use crate::workspace::{registry::Registry, WorkspaceBinding};

    #[test]
    fn moving_from_a_pair_dissolves_it_and_reports_the_survivor() {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-groups-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (threads, _writers) = stored_threads(&dir, 4);
        let mut groups = Groups::load(&dir).unwrap();
        let first = groups
            .apply(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let second = groups
            .apply(GroupChange::Create {
                first: threads[2],
                second: threads[3],
            })
            .unwrap()
            .group
            .unwrap();

        let applied = groups
            .apply(GroupChange::Join {
                thread: threads[0],
                group: second,
                index: None,
            })
            .unwrap();

        assert_eq!(
            applied.dissolved,
            vec![Dissolved {
                group: first,
                survivor: threads[1],
            }]
        );
        assert_eq!(groups.of(threads[0]).unwrap().id, second);
        assert_eq!(
            groups.iter().next().unwrap().members,
            [threads[2], threads[3], threads[0]]
        );
    }

    #[test]
    fn drop_plans_create_target_first_join_leave_reorder_and_group_move() {
        let one = ThreadId::new(1);
        let two = ThreadId::new(2);
        let group = GroupId::new(7);
        assert_eq!(
            plan_drop(
                Drag::Thread {
                    thread: one,
                    group: None
                },
                DropTarget::ThreadRow {
                    thread: two,
                    group: None,
                    index: 0
                }
            ),
            Plan::Change(GroupChange::Create {
                first: two,
                second: one
            })
        );
        assert_eq!(
            plan_drop(
                Drag::Thread {
                    thread: one,
                    group: None
                },
                DropTarget::GroupHeader(group)
            ),
            Plan::Change(GroupChange::Join {
                thread: one,
                group,
                index: None
            })
        );
        assert_eq!(
            plan_drop(
                Drag::Thread {
                    thread: one,
                    group: Some(group)
                },
                DropTarget::LooseZone
            ),
            Plan::Change(GroupChange::Leave { thread: one })
        );
        assert_eq!(
            plan_drop(
                Drag::Thread {
                    thread: one,
                    group: Some(group)
                },
                DropTarget::ThreadRow {
                    thread: two,
                    group: Some(group),
                    index: 0
                }
            ),
            Plan::Change(GroupChange::ReorderMember {
                group,
                thread: one,
                index: 0
            })
        );
        assert_eq!(
            plan_drop(Drag::Group(group), DropTarget::GroupGap(2)),
            Plan::Change(GroupChange::MoveGroup { group, index: 2 })
        );
    }

    #[test]
    fn persisted_groups_heal_deleted_thread_ids() {
        let dir = scratch("heal");
        let store = Store::open(&dir).unwrap();
        let checkout = dir.join("project");
        std::fs::create_dir_all(&checkout).unwrap();
        let project = Registry::open(&dir).unwrap().register(&checkout).unwrap();
        let make = || {
            store
                .create(
                    Provider::Claude,
                    Some(project),
                    WorkspaceBinding::Main {
                        checkout: checkout.clone(),
                    },
                )
                .unwrap()
        };
        let (one, writer_one) = make();
        let (two, writer_two) = make();
        let mut groups = Groups::load(&dir).unwrap();
        groups
            .apply(GroupChange::Create {
                first: one,
                second: two,
            })
            .unwrap();
        drop((writer_one, writer_two));
        store.delete(one).unwrap();

        let healed = Groups::load(&dir).unwrap();
        assert_eq!(healed.iter().count(), 0, "a healed singleton dissolves");
        store.delete(two).unwrap();
        assert_eq!(Groups::load(&dir).unwrap().iter().count(), 0);
    }

    #[test]
    fn unreadable_thread_metadata_refuses_load_without_overwriting_groups() {
        let dir = scratch("load-refuses-corrupt-thread");
        let store = Store::open(&dir).unwrap();
        let checkout = dir.join("project");
        std::fs::create_dir_all(&checkout).unwrap();
        let project = Registry::open(&dir).unwrap().register(&checkout).unwrap();
        let (thread, writer) = store
            .create(
                Provider::Claude,
                Some(project),
                WorkspaceBinding::Main { checkout },
            )
            .unwrap();
        let (other, other_writer) = store
            .create(
                Provider::Claude,
                Some(project),
                WorkspaceBinding::Main {
                    checkout: dir.join("project"),
                },
            )
            .unwrap();
        let mut groups = Groups::load(&dir).unwrap();
        groups
            .apply(GroupChange::Create {
                first: thread,
                second: other,
            })
            .unwrap();
        drop((writer, other_writer));
        let groups_path = dir.join(FILE_NAME);
        let before = std::fs::read(&groups_path).unwrap();
        std::fs::write(
            dir.join(thread.to_string()).join("log.jsonl"),
            b"not a header\n",
        )
        .unwrap();

        assert!(Groups::load(&dir).is_err());
        assert_eq!(std::fs::read(groups_path).unwrap(), before);
    }

    #[test]
    fn one_hundred_members_persist_in_order_while_cross_project_moves_refuse() {
        let dir = scratch("project-unbounded");
        let store = Store::open(&dir).unwrap();
        let roots = [dir.join("one"), dir.join("two")];
        for root in &roots {
            std::fs::create_dir_all(root).unwrap();
        }
        let mut registry = Registry::open(&dir).unwrap();
        let projects = [
            registry.register(&roots[0]).unwrap(),
            registry.register(&roots[1]).unwrap(),
        ];
        let mut ids = Vec::new();
        let mut writers = Vec::new();
        for index in 0..103 {
            let project = if index == 102 {
                projects[1]
            } else {
                projects[0]
            };
            let (id, writer) = store
                .create(
                    Provider::Claude,
                    Some(project),
                    WorkspaceBinding::Main {
                        checkout: roots[usize::from(index == 102)].clone(),
                    },
                )
                .unwrap();
            ids.push(id);
            writers.push(writer);
        }
        let mut groups = Groups::load(&dir).unwrap();
        let group = groups
            .apply(GroupChange::Create {
                first: ids[0],
                second: ids[1],
            })
            .unwrap()
            .group
            .unwrap();
        for thread in &ids[2..100] {
            groups
                .apply(GroupChange::Join {
                    thread: *thread,
                    group,
                    index: None,
                })
                .unwrap();
        }
        assert_eq!(groups.get(group).unwrap().members, ids[..100]);
        assert_eq!(grid(100), (10, 10));
        let join = groups.preview_drop(
            Drag::Thread {
                thread: ids[100],
                group: None,
            },
            DropTarget::GroupHeader(group),
        );
        assert!(matches!(join, Plan::Change(GroupChange::Join { .. })));
        let applied = match join {
            Plan::Change(change) => groups.apply(change).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(applied.group, Some(group));

        let downward = groups.preview_drop(
            Drag::Thread {
                thread: ids[0],
                group: Some(group),
            },
            DropTarget::ThreadRow {
                thread: ids[100],
                group: Some(group),
                index: 100,
            },
        );
        match downward {
            Plan::Change(change) => groups.apply(change).unwrap(),
            _ => panic!("same-project downward reorder was refused"),
        };
        let mut expected = ids[1..101].to_vec();
        expected.push(ids[0]);
        assert_eq!(groups.get(group).unwrap().members, expected);
        drop(groups);
        let groups = Groups::load(&dir).unwrap();
        assert_eq!(groups.get(group).unwrap().members, expected);
        assert!(matches!(
            groups.preview_drop(
                Drag::Thread {
                    thread: ids[102],
                    group: None,
                },
                DropTarget::GroupHeader(group),
            ),
            Plan::Refused(reason) if reason.contains("different projects")
        ));
        assert_eq!(groups.get(group).unwrap().members, expected);
    }

    #[test]
    fn missing_project_identity_is_not_treated_as_a_shared_project() {
        let dir = scratch("missing-project");
        let store = Store::open(&dir).unwrap();
        let checkout = dir.join("legacy");
        std::fs::create_dir_all(&checkout).unwrap();
        let make = || {
            store
                .create(
                    Provider::Claude,
                    None,
                    WorkspaceBinding::Main {
                        checkout: checkout.clone(),
                    },
                )
                .unwrap()
        };
        let (one, _one_writer) = make();
        let (two, _two_writer) = make();
        let mut groups = Groups::load(&dir).unwrap();

        assert!(matches!(
            groups.apply(GroupChange::Create {
                first: one,
                second: two,
            }),
            Err(ApplyError::MissingProject)
        ));
        assert_eq!(groups.iter().count(), 0);
    }

    #[test]
    fn member_reorders_use_final_positions_while_group_moves_use_gaps() {
        let dir = scratch("reorder-positions");
        let (threads, _writers) = stored_threads(&dir, 7);
        let mut groups = Groups::load(&dir).unwrap();
        let first = groups
            .apply(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        groups
            .apply(GroupChange::Join {
                thread: threads[2],
                group: first,
                index: None,
            })
            .unwrap();
        let second = groups
            .apply(GroupChange::Create {
                first: threads[3],
                second: threads[4],
            })
            .unwrap()
            .group
            .unwrap();
        let third = groups
            .apply(GroupChange::Create {
                first: threads[5],
                second: threads[6],
            })
            .unwrap()
            .group
            .unwrap();

        groups
            .apply(GroupChange::ReorderMember {
                group: first,
                thread: threads[0],
                index: 3,
            })
            .unwrap();
        assert_eq!(
            groups.get(first).unwrap().members,
            [threads[1], threads[2], threads[0]]
        );
        groups
            .apply(GroupChange::MoveGroup {
                group: first,
                index: 3,
            })
            .unwrap();
        assert_eq!(
            groups.iter().map(|group| group.id).collect::<Vec<_>>(),
            [second, third, first]
        );
        groups
            .apply(GroupChange::MoveGroup {
                group: first,
                index: 0,
            })
            .unwrap();
        assert_eq!(
            groups.iter().map(|group| group.id).collect::<Vec<_>>(),
            [first, second, third]
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-groups-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stored_threads(
        dir: &Path,
        count: usize,
    ) -> (Vec<ThreadId>, Vec<crate::store::ThreadWriter>) {
        let store = Store::open(dir).unwrap();
        let checkout = dir.join("project");
        std::fs::create_dir_all(&checkout).unwrap();
        let project = Registry::open(dir).unwrap().register(&checkout).unwrap();
        (0..count)
            .map(|_| {
                store
                    .create(
                        Provider::Claude,
                        Some(project),
                        WorkspaceBinding::Main {
                            checkout: checkout.clone(),
                        },
                    )
                    .unwrap()
            })
            .unzip()
    }
}
