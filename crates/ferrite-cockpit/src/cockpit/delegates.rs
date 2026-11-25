//! The operator's hand on a Thread's Delegates (ADR 0047), as T3 Code's
//! Lineage and timeline have it for a task in a linked environment: its
//! rows open the Thread there, and the row of a Thread another machine
//! delegated opens that one. Each names its Thread by the Server there and
//! its id for it, so the window finds the machine that answers as that
//! Server. Cancelling one is its Server's: the window asks, and the
//! Thread's digest says what became of it (`ThreadFacts::delegating`).

use super::*;
use crate::notifications::{Alert, RowTarget};
use ferrite_client::cockpit::ServerState;
use ferrite_core::delegate::{self, DelegateId, Delegated};
use ferrite_core::notifications::{DecisionNoticeId, RequestKind};
use ferrite_core::server::ServerId;
use ferrite_core::transcript::{Body, Transcript};
use ferrite_core::{DelegatedFrom, MachineId};
use ferrite_pane::pane::{AgentClock, AgentFace, AgentRun, Elsewhere};
use ferrite_pane::transcript::{ParentLink, TranscriptTail};
use gpui::App;

/// A machine of this window whose Server answers as a given one.
pub(super) struct Serving {
    /// The name this window saved it by; `None` for this computer, which
    /// the nav calls `local` and its Server calls by its own label.
    pub(super) name: Option<String>,
    /// The machine, as the window holds it.
    pub(super) machine: MachineId,
}

/// The Thread on another machine that delegated one of this window's (T3's
/// `parentThreadLink`), as its divider and its Lineage row name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Delegator {
    pub(super) server: ServerId,
    pub(super) thread: ThreadId,
    /// Its title as its machine says it now, else as it was when it
    /// delegated (T3's `parentThreadTitle ?? delegatedFrom.title`).
    pub(super) title: String,
    /// Its machine by the name this window saved it by, else by the label
    /// its Server sent (T3's `environment?.label ?? fallbackLabel`).
    pub(super) machine: String,
    /// A machine of this window answers as its Server, so it opens.
    pub(super) reached: bool,
}

impl Delegator {
    pub(super) fn of(from: &DelegatedFrom, serving: Option<Serving>, title: Option<&str>) -> Self {
        Self {
            server: from.server.clone(),
            thread: from.thread,
            title: title.map_or_else(|| from.title.clone(), str::to_owned),
            reached: serving.is_some(),
            machine: serving
                .and_then(|serving| serving.name)
                .unwrap_or_else(|| from.label.clone()),
        }
    }
}

impl CockpitView {
    /// The machine of this window whose Server answers as `server` now.
    pub(super) fn serving(&self, server: &ServerId, cx: &App) -> Option<Serving> {
        let cockpit = self.cockpit.read(cx);
        let machine = cockpit.machines().find(|machine| {
            matches!(
                cockpit.server(*machine),
                Some(ServerState::Attached { id: Some(id), .. }) if id == *server
            )
        })?;
        Some(Serving {
            name: self.machine_label(machine, cx),
            machine,
        })
    }

    /// Thread `thread` of the machine whose Server answers as `server`.
    fn key_on(&self, server: &ServerId, thread: ThreadId, cx: &App) -> Option<ThreadKey> {
        let machine = self.serving(server, cx)?.machine;
        Some(self.cockpit.read(cx).registry(machine).key(thread))
    }

    /// The Thread that delegated one of this window's, from what it was
    /// delegated with (`ThreadFacts::delegated_from`) and what its machine
    /// says of it now.
    pub(super) fn delegator(&self, from: &DelegatedFrom, cx: &App) -> Delegator {
        let serving = self.serving(&from.server, cx);
        let cockpit = self.cockpit.read(cx);
        let title = serving.as_ref().and_then(|serving| {
            cockpit
                .thread(cockpit.registry(serving.machine).key(from.thread))?
                .title()
        });
        Delegator::of(from, serving, title)
    }

    /// What Pane `index`'s divider says of the Thread that delegated its
    /// own (T3's `Subagent of`, over Main's rows alone).
    pub(super) fn delegator_link(&self, index: usize, cx: &App) -> Option<ParentLink> {
        let pane = self.panes.get(index).filter(|pane| pane.is_main())?;
        let from = self.facts.get(pane.thread()?)?.delegated_from.as_ref()?;
        let delegator = self.delegator(from, cx);
        Some(ParentLink {
            title: delegator.title,
            machine: delegator.machine,
            opens: delegator.reached,
        })
    }

    /// A press on the row of the Thread that delegated this one (T3's
    /// `DelegatedFromRow`): it opens when its machine still has it, and
    /// says in T3's words when not, or when that machine is gone.
    pub(super) fn open_delegator(&mut self, server: &ServerId, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(key) = self.key_on(server, thread, cx) else {
            self.nav_notice.problem("Could not open the parent thread");
            cx.notify();
            return;
        };
        // Until its Server says what it parked, the open tries.
        if self.cockpit.read(cx).holds(key).unwrap_or(true) {
            self.notice_verb(Verb::OpenThread(key), cx);
        } else {
            self.nav_notice.hint("The parent thread was deleted");
            cx.notify();
        }
    }

    /// The rows of the `delegate_task` calls in `transcript` (T3's
    /// `SubagentTimelineLink` for a task in a linked environment): each
    /// call's Delegate as `thread`'s digest says it, by the call, `on` the
    /// machine it went to and opening its Thread there while this window
    /// reaches it. A call that started none draws as a call.
    pub(super) fn delegate_faces(
        &self,
        thread: ThreadKey,
        transcript: &Transcript,
        cx: &App,
    ) -> Vec<(String, AgentFace)> {
        let Some(delegates) = self
            .facts
            .get(thread)
            .map(|facts| &facts.delegating.delegates)
            .filter(|delegates| !delegates.is_empty())
        else {
            return Vec::new();
        };
        let Some(provider) = self.cockpit.read(cx).thread(thread).map(|open| open.provider())
        else {
            return Vec::new();
        };
        transcript
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Tool(tool) => Some(tool),
                _ => None,
            })
            .filter_map(|tool| {
                let id = delegate::started_by(tool)?;
                let delegated = delegates.iter().find(|delegated| delegated.id == id)?;
                let reached = self.serving(&delegated.server, cx).is_some();
                Some((tool.call.clone(), delegate_face(delegated, reached, provider)))
            })
            .collect()
    }

    /// The Delegate call `call` of `thread`'s `subject` started, by its
    /// Server and its Thread there.
    pub(super) fn delegate_of_call(
        &self,
        thread: ThreadKey,
        subject: &ferrite_core::activity::Subject,
        call: &str,
        cx: &App,
    ) -> Option<(ServerId, ThreadId)> {
        let open = self.cockpit.read(cx).thread(thread)?;
        let id = delegate::started_by(open.activity().subject(subject)?.transcript().tool(call)?)?;
        let delegated = self
            .facts
            .get(thread)?
            .delegating
            .delegates
            .iter()
            .find(|delegated| delegated.id == id)?;
        Some((delegated.server.clone(), delegated.thread))
    }

    /// Open Thread `thread` of the machine whose Server is `server`, in
    /// place. A machine no longer reached opens nothing.
    pub(super) fn open_elsewhere(&mut self, server: &ServerId, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(key) = self.key_on(server, thread, cx) {
            self.notice_verb(Verb::OpenThread(key), cx);
        }
    }

    /// Cancel `thread`'s Delegate `delegate`, as its agent's `task_cancel`
    /// does. Its row moves when the digest says so; a refusal is said in
    /// the nav's notice.
    pub(super) fn cancel_delegate(&mut self, thread: ThreadKey, delegate: DelegateId, cx: &mut Context<Self>) {
        let answer = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.cancel_delegate(thread, delegate));
        cx.spawn(async move |this, cx| {
            if let Err(error) = ferrite_ui::answer::answered(answer, cx).await {
                this.update(cx, |view, cx| {
                    view.nav_notice.problem(error.to_string());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

/// A Delegate, or a Delegate's own, that waits on the operator on its
/// machine (T3's `useChildThreadInputs`, a child in a linked environment
/// with a request pending).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WaitingDelegate {
    pub(super) title: String,
    pub(super) kind: RequestKind,
    /// Its oldest request, on the machine that has it.
    pub(super) alert: Alert,
}

/// What this window reads of one Delegate's Thread on its machine.
pub(super) struct Seen {
    /// Its Thread, on its machine.
    pub(super) thread: ThreadKey,
    /// Its oldest request there.
    pub(super) request: Option<(DecisionNoticeId, RequestKind)>,
    /// Its title as its machine says it now.
    pub(super) title: Option<String>,
    /// Its own Delegates, as its digest names them.
    pub(super) delegates: Vec<Delegated>,
}

/// The open Delegates under `roots` that wait on the operator, down each
/// one's own as its machine's digest names them (T3 walks a Thread's
/// descendants), in the order they were started and each once. `seen`
/// reads a Delegate's Thread on its machine; one no machine of this window
/// answers for is not seen.
pub(super) fn waiting_delegates(
    roots: &[Delegated],
    seen: impl Fn(&ServerId, ThreadId) -> Option<Seen>,
) -> Vec<WaitingDelegate> {
    let mut queue: std::collections::VecDeque<Delegated> = roots.iter().cloned().collect();
    let mut walked = Vec::new();
    let mut waiting = Vec::new();
    while let Some(delegate) = queue.pop_front() {
        let key = (delegate.server.clone(), delegate.thread);
        if delegate.status.ended() || walked.contains(&key) {
            continue;
        }
        walked.push(key);
        let Some(there) = seen(&delegate.server, delegate.thread) else {
            continue;
        };
        if let Some((id, kind)) = there.request {
            waiting.push(WaitingDelegate {
                title: there
                    .title
                    .or(delegate.title)
                    .unwrap_or_else(|| "Delegate".to_owned()),
                kind,
                alert: Alert {
                    target: RowTarget::Decision(id),
                    thread: there.thread,
                },
            });
        }
        queue.extend(there.delegates);
    }
    waiting
}

/// T3's `childThreadInputsTitle`, Delegate where T3 says Subagent (ADR
/// 0047): `Delegate needs approval` when every one waits on a permission,
/// `input` when none does, `you` when some do; `2 delegates need …` for
/// several.
pub(super) fn waiting_title(waiting: &[WaitingDelegate]) -> String {
    let approvals = waiting
        .iter()
        .filter(|delegate| delegate.kind == RequestKind::Permission)
        .count();
    let need = match approvals {
        0 => "input",
        all if all == waiting.len() => "approval",
        _ => "you",
    };
    match waiting.len() {
        1 => format!("Delegate needs {need}"),
        count => format!("{count} delegates need {need}"),
    }
}

impl CockpitView {
    /// What `waiting_delegates` reads of `thread` on the machine whose
    /// Server is `server`: its Notices, its title and its digest's
    /// Delegates.
    fn seen(&self, server: &ServerId, thread: ThreadId, cx: &App) -> Option<Seen> {
        let key = self.key_on(server, thread, cx)?;
        let cockpit = self.cockpit.read(cx);
        Some(Seen {
            thread: key,
            request: cockpit
                .notifications(key.machine())
                .first_request(thread)
                .map(|notice| (notice.id.clone(), notice.kind)),
            title: cockpit
                .thread(key)
                .and_then(|open| open.title().map(str::to_owned)),
            delegates: self
                .facts
                .get(key)
                .map(|facts| facts.delegating.delegates.clone())
                .unwrap_or_default(),
        })
    }

    /// The line over Main's background-work line while one of its
    /// Delegates waits on the operator (T3's `childInputNowItem`): who
    /// waits, each name opening it, and `Open approval` (else `Open
    /// question`) opening the first at its request. It only opens: a
    /// Delegate's permission is answered on its own Thread, never from its
    /// parent's (ADR 0047). None while Main itself waits on the operator.
    pub(super) fn delegate_waiting_tail(&self, index: usize, cx: &Context<Self>) -> Option<TranscriptTail> {
        let pane = self.panes.get(index).filter(|pane| pane.is_main())?;
        let thread = pane.thread()?;
        if self.cockpit.read(cx).thread(thread)?.pending().is_some() {
            return None;
        }
        let roots = &self.facts.get(thread)?.delegating.delegates;
        let waiting = waiting_delegates(roots, |server, there| self.seen(server, there, cx));
        let first = waiting.first()?.clone();
        let title = waiting_title(&waiting);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            ("delegate-waiting", thread, &title).hash(&mut hasher);
            for delegate in &waiting {
                (&delegate.title, delegate.alert.thread).hash(&mut hasher);
            }
            hasher.finish()
        };
        let owner = cx.weak_entity();
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                let open = |alert: &Alert| {
                    let (owner, alert) = (owner.clone(), alert.clone());
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let _ = owner.update(cx, |view, cx| view.notice_verb(alert.open_verb(), cx));
                    }
                };
                let last = waiting.len() - 1;
                let names = div().flex().min_w_0().gap(px(ferrite_ui::theme::SPACE_1)).children(
                    waiting.iter().enumerate().map(|(at, delegate)| {
                        let name = ferrite_pane::pane::inline_action(
                            SharedString::from(format!("delegate-needs-you-open-{at}")),
                            delegate.title.clone(),
                            ferrite_ui::theme::INK_SUBTLE,
                            cx,
                        )
                        .debug_selector(move || format!("delegate-needs-you-open-{at}"))
                        .on_click(open(&delegate.alert));
                        div()
                            .flex()
                            .whitespace_nowrap()
                            .child(name)
                            .when(at < last, |name| name.child(","))
                            .into_any_element()
                    }),
                );
                let action = ferrite_pane::pane::inline_action(
                    "delegate-needs-you-action",
                    match first.kind {
                        RequestKind::Permission => "Open approval",
                        RequestKind::Question => "Open question",
                    },
                    ferrite_ui::theme::INK_STRONG,
                    cx,
                )
                .debug_selector(|| "delegate-needs-you-action".into())
                .on_click(open(&first.alert))
                .into_any_element();
                ferrite_pane::pane::now_line(
                    "delegate-needs-you",
                    key,
                    ferrite_pane::pane::NowLine {
                        icon: match first.kind {
                            RequestKind::Permission => ferrite_ui::icons::SHIELD_QUESTION,
                            RequestKind::Question => ferrite_ui::icons::MESSAGE_CIRCLE,
                        },
                        needs_you: true,
                        title: title.clone(),
                        description: Some(names.into_any_element()),
                    },
                    vec![action],
                )
                .into_any_element()
            }),
        })
    }
}

/// How a Delegate's row stands (T3's statuses for a task): running while
/// it starts or works, waiting while a Decision waits there, then done,
/// failed, or stopped (cancelled, or stopped with its parent); once ended,
/// its result (its problem when it has none); and its clock from the
/// stamps its Server wrote, counting while it runs. Its provider is the one
/// it was started on, the delegating Thread's (`provider`) when none was named.
fn delegate_face(
    delegated: &Delegated,
    reached: bool,
    provider: ferrite_core::provider::Provider,
) -> AgentFace {
    use ferrite_core::clock;
    use ferrite_core::delegate::Status;
    let run = match delegated.status {
        Status::Starting | Status::Running => AgentRun::Running,
        Status::Waiting => AgentRun::Waiting,
        Status::Completed => AgentRun::Done,
        Status::Failed => AgentRun::Failed,
        Status::Cancelled | Status::Interrupted => AgentRun::Stopped,
    };
    let ended = delegated.status.ended();
    let stamp = |stamp: &Option<String>| stamp.as_deref().and_then(clock::parse_stamp);
    let clock = match (stamp(&delegated.started_at), stamp(&delegated.ended_at)) {
        (Some(from), Some(to)) if ended => (from <= to).then_some(AgentClock::Ran { from, to }),
        (Some(started), None) if !ended => Some(AgentClock::Since(clock::instant_at(started))),
        _ => None,
    };
    AgentFace {
        provider: delegated.provider.unwrap_or(provider),
        run,
        summary: None,
        result: ended
            .then(|| delegated.result.clone().or_else(|| delegated.problem.clone()))
            .flatten(),
        clock,
        on: Some(Elsewhere {
            machine: delegated.machine.clone(),
            reached,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::delegate::Status;
    use ferrite_core::provider::Provider;

    /// A Delegate's avatar is the provider it was started on, else the
    /// delegating Thread's.
    #[test]
    fn a_delegates_face_names_its_provider() {
        let mut named = delegated(1, "s-pc", None, Status::Running);
        assert_eq!(delegate_face(&named, true, Provider::Claude).provider, Provider::Claude);
        named.provider = Some(Provider::Codex);
        assert_eq!(delegate_face(&named, true, Provider::Claude).provider, Provider::Codex);
    }

    fn delegated(id: u32, server: &str, title: Option<&str>, status: Status) -> Delegated {
        Delegated {
            id: DelegateId::new(id),
            server: ServerId::new(server),
            machine: server.trim_start_matches("s-").into(),
            thread: ThreadId::new(u64::from(id)),
            title: title.map(str::to_owned),
            provider: None,
            status,
            problem: None,
            stop_owed: false,
            result: None,
            started_at: None,
            ended_at: None,
        }
    }

    fn request(thread: u64, serial: u64) -> DecisionNoticeId {
        DecisionNoticeId {
            thread: ThreadId::new(thread),
            handle: (&ferrite_core::protocol::WireHandle {
                generation: 1,
                serial,
                request_id: format!("r{serial}"),
            })
                .into(),
        }
    }

    /// What `pc` says of its Threads: 1 asks a permission and delegated 3
    /// back to this computer, which asks a question; 2 waits on nothing.
    fn seen(server: &ServerId, thread: ThreadId) -> Option<Seen> {
        let pc = |thread| ThreadKey::fixture(1, thread);
        match (server.as_str(), thread.get()) {
            ("s-pc", 1) => Some(Seen {
                thread: pc(1),
                request: Some((request(1, 4), RequestKind::Permission)),
                title: Some("Fix the tests today".into()),
                delegates: vec![delegated(3, "s-here", None, Status::Waiting)],
            }),
            ("s-pc", 2) => Some(Seen {
                thread: pc(2),
                request: None,
                title: None,
                delegates: vec![],
            }),
            ("s-here", 3) => Some(Seen {
                thread: ThreadKey::fixture(0, 3),
                request: Some((request(3, 9), RequestKind::Question)),
                title: None,
                delegates: vec![],
            }),
            _ => None,
        }
    }

    /// T3 walks every descendant with a request pending: an open
    /// Delegate's, then its own Delegates', by the title its machine says
    /// now (else the one it was sent with, else `Delegate`), each opening
    /// at its oldest request on its machine. An ended one, or one on a
    /// machine this window does not reach, is not walked.
    #[test]
    fn the_delegates_that_wait_are_found_down_their_own() {
        let roots = [
            delegated(1, "s-pc", Some("Fix the tests"), Status::Waiting),
            delegated(2, "s-pc", Some("Lint"), Status::Running),
            delegated(4, "s-pc", Some("Docs"), Status::Completed),
            delegated(5, "s-gone", Some("Bench"), Status::Running),
        ];
        let waiting = waiting_delegates(&roots, seen);
        assert_eq!(
            waiting,
            [
                WaitingDelegate {
                    title: "Fix the tests today".into(),
                    kind: RequestKind::Permission,
                    alert: Alert {
                        target: RowTarget::Decision(request(1, 4)),
                        thread: ThreadKey::fixture(1, 1),
                    },
                },
                WaitingDelegate {
                    title: "Delegate".into(),
                    kind: RequestKind::Question,
                    alert: Alert {
                        target: RowTarget::Decision(request(3, 9)),
                        thread: ThreadKey::fixture(0, 3),
                    },
                },
            ]
        );
        assert_eq!(waiting_title(&waiting), "2 delegates need you");
        assert_eq!(waiting_title(&waiting[..1]), "Delegate needs approval");
        assert_eq!(waiting_title(&waiting[1..]), "Delegate needs input");
        assert_eq!(waiting_title(&[waiting[0].clone(), waiting[0].clone()]), "2 delegates need approval");
    }

    /// T3's task row: its status, its result (else its problem) once it
    /// ended, how long it ran from the stamps its Server wrote, `on` its
    /// machine; it opens only while this window reaches that machine.
    #[test]
    fn a_delegates_row_reads_its_digest() {
        let mut running = delegated(1, "s-pc", Some("Fix the tests"), Status::Running);
        running.started_at = Some("2026-11-23T09:00:00+10:30".into());
        let face = delegate_face(&running, true, Provider::Claude);
        assert_eq!(face.run, AgentRun::Running);
        assert!(matches!(face.clock, Some(AgentClock::Since(_))));
        assert_eq!((face.opens(), face.result), (true, None));
        assert_eq!(
            face.on,
            Some(Elsewhere {
                machine: "pc".into(),
                reached: true
            })
        );

        let mut done = running.clone();
        done.status = Status::Completed;
        done.result = Some("All green.".into());
        done.ended_at = Some("2026-11-23T09:04:10+10:30".into());
        let face = delegate_face(&done, false, Provider::Claude);
        assert_eq!(face.run, AgentRun::Done);
        assert_eq!(face.clock.map(AgentClock::elapsed), Some(std::time::Duration::from_secs(250)));
        assert_eq!(face.result.as_deref(), Some("All green."));
        assert!(!face.opens(), "a machine this window does not reach opens nothing");

        let mut failed = done.clone();
        (failed.status, failed.result) = (Status::Failed, None);
        failed.problem = Some("codex exited".into());
        assert_eq!(delegate_face(&failed, true, Provider::Claude).run, AgentRun::Failed);
        assert_eq!(delegate_face(&failed, true, Provider::Claude).result.as_deref(), Some("codex exited"));
        for stopped in [Status::Cancelled, Status::Interrupted] {
            let face = delegate_face(&Delegated { status: stopped, ..done.clone() }, true, Provider::Claude);
            assert_eq!(face.run, AgentRun::Stopped);
        }
        let waiting = Delegated { status: Status::Waiting, ..running };
        assert_eq!(delegate_face(&waiting, true, Provider::Claude).run, AgentRun::Waiting);
    }
}
