//! What a draft's workspace chip and its menu read of its Project's repo:
//! the branch each Project's checkout is on, and what the repo lists for
//! the menu (its other worktrees and its local branches), as git here or
//! the machine's Server last answered.
//!
//! Only the root asks, off the UI thread or of the machine's Server, and
//! only the answers and its pump write them, so they are a plain struct on
//! the root (ADR 0041). Never read from a frame: the chip asks about a
//! Project it has not named yet, and the last answer stands until the new
//! one lands.

use super::*;

/// A machine's answer to `list_repo`, on its way.
type Reply = std::sync::mpsc::Receiver<std::io::Result<RepoListing>>;

#[derive(Default)]
pub(super) struct Checkouts {
    /// The branch each Project's checkout is on, as last answered.
    branches: std::cell::RefCell<std::collections::HashMap<ProjectKey, SharedString>>,
    /// Projects whose checkout is being asked about: one lookup each at a
    /// time, however many frames name it meanwhile.
    asking: std::cell::RefCell<std::collections::HashSet<ProjectKey>>,
    /// What git lists for a draft's chosen repo, kept by Project: the
    /// rows show the last answer at once and take the fresh one when it
    /// lands.
    listing: Option<(ProjectKey, RepoListing)>,
    /// On a machine, the listing its Server is reading for the menu or a
    /// draft's chip.
    asking_machine: Option<(ProjectKey, Reply)>,
}

impl Checkouts {
    /// The branch `project`'s checkout is on, as last answered.
    pub(super) fn branch(&self, project: ProjectKey) -> Option<SharedString> {
        self.branches.borrow().get(&project).cloned()
    }

    /// Whether `project`'s checkout is being asked about.
    pub(super) fn asking(&self, project: ProjectKey) -> bool {
        self.asking.borrow().contains(&project)
    }

    /// Whether any checkout is being asked about.
    #[cfg(test)]
    pub(super) fn asking_any(&self) -> bool {
        !self.asking.borrow().is_empty()
    }

    /// `projects`' checkouts are being asked about.
    pub(super) fn listen(&self, projects: impl IntoIterator<Item = ProjectKey>) {
        self.asking.borrow_mut().extend(projects);
    }

    /// Take git's answers, a branch or none for each Project asked about
    /// (`main`, the branch a fresh clone would be sitting on); whether
    /// any changed.
    pub(super) fn listed(&mut self, answers: Vec<(ProjectKey, Option<String>)>) -> bool {
        let mut changed = false;
        let asking = self.asking.get_mut();
        let known = self.branches.get_mut();
        for (project, branch) in answers {
            asking.remove(&project);
            let branch = branch.map_or_else(|| SharedString::from(MAIN_BRANCH), SharedString::from);
            if known.get(&project) != Some(&branch) {
                known.insert(project, branch);
                changed = true;
            }
        }
        changed
    }

    /// What `project`'s repo lists, as last answered.
    pub(super) fn listing(&self, project: ProjectKey) -> Option<&RepoListing> {
        self.listing
            .as_ref()
            .filter(|(listed, _)| *listed == project)
            .map(|(_, listing)| listing)
    }

    /// Git here listed `project`'s repo.
    pub(super) fn set_listing(&mut self, project: ProjectKey, listing: RepoListing) {
        self.listing = Some((project, listing));
    }

    /// Wait for the machine's Server to list `project`'s repo; none, it
    /// cannot say.
    pub(super) fn listen_machine(&mut self, project: ProjectKey, reply: Option<Reply>) {
        self.asking_machine = reply.map(|reply| (project, reply));
    }

    /// Take the machine's listing, once it lands, with the branch its
    /// checkout is on (`main` when it cannot say). `None` while nothing
    /// was asked; whether it landed.
    pub(super) fn machine_listed(&mut self) -> Option<bool> {
        let (project, answer) = self.asking_machine.as_ref()?;
        let project = *project;
        let listing = match answer.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => return Some(false),
            Ok(listing) => listing.unwrap_or_default(),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => RepoListing::default(),
        };
        let branch = listing.checked_out.as_deref().unwrap_or(MAIN_BRANCH);
        self.branches
            .get_mut()
            .insert(project, SharedString::from(branch.to_string()));
        self.listing = Some((project, listing));
        self.asking_machine = None;
        Some(true)
    }
}
