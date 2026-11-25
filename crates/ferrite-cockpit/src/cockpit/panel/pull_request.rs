//! The pull request Surface (ADR 0039; T3's `PullRequestDetailPanel`):
//! the PR of the Thread's branch as a tab of its own, opened from the PR
//! row in Overview's Changes and the `+` menu's Pull request (P).
//!
//! - **Header:** the repo and `#N` on the left, `#N` in its state's ink
//!   and opening the PR on the forge; on the right the primary action
//!   (Merge, armed on the first press: "Confirm merge?") or, once the PR is
//!   merged or closed, its state pill, then `⋯`.
//! - **`⋯`:** hand-offs (Ask a question prepares the current composer; Explain, Fix
//!   findings and Write guide ask the Thread's agent), the PR's state
//!   (Ready for review or Convert to draft, Merge now, Enable auto-merge,
//!   Allow edits from maintainers), the destructive rows (Close, Revert,
//!   Delete branch), armed as every destructive row is, and copies. Ferrite
//!   runs no forge writes of its own: the PR's state changes are asked of
//!   the agent, which has `gh`, as `Create PR` is.
//! - **Views:** Summary, Guide, Timeline and Code as a segmented group,
//!   the checks and comment counts after it; the one chosen is kept on the
//!   Surface (`ThreadSurfaces::pr_tabs`). Code reads the requested PR's published
//!   files when its tab is hovered or shown.
//!
//! The PR's facts (its number, state, checks) are the nav's reading
//! (`facts.pr`); its whole snapshot (body, checks, conversation) is read
//! when the Surface opens (`Cockpit::review_pull_request`).
//!
//! The reader is a view of its own (ADR 0041), `PullRequestView`: it owns
//! every read, every action's course and the Merge armed. What it draws
//! of the window it reads through a `Source` the root makes over itself
//! (`pr_inputs`); what it asks of the window (a read, a view kept, Code's
//! diff, a hand-off, the `⋯`) it says as a `PrEvent`. The forge and the
//! Thread's agent are the window's, so the window asks them and hands the
//! answers to the view. Its controls are stops of the Panel's row: the
//! view declares them (`tab_stops`), the root enters them as it draws the
//! Surface and hands each one's handle back through the inputs. Code's
//! body is Changes', a view the root hands it (`ChangesPane`).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{AnyView, Bounds, FocusHandle, Pixels, WeakEntity};

use super::pr_action::{ActionState, PrActions, Stage};
use super::pr_read::{Answer, PrFailure, PrRead, PrReads, stamp_age};
use ferrite_core::pull_request::{PrSource, PrTarget};
use super::*;
use crate::cockpit::menu_button::{Door, Edge};
use crate::cockpit::surfaces::PrTab;
use ferrite_ui::components::{IconSize, MenuDoor};
use ferrite_ui::source::Source;
use ferrite_core::protocol::{
    WireCheckStatus, WirePr, WirePrCommentKind, WirePrState, WirePrUnread,
};
use ferrite_core::pull_request::PrState;

/// A pull request's state, as its pill and its tab's icon show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrTone {
    Open,
    Draft,
    Merged,
    Closed,
}

impl PrTone {
    fn of_reading(state: PrState, draft: bool) -> Self {
        match (state, draft) {
            (PrState::Merged, _) => Self::Merged,
            (PrState::Closed, _) => Self::Closed,
            (PrState::Open, true) => Self::Draft,
            (PrState::Open, false) => Self::Open,
        }
    }

    pub(in crate::cockpit) fn of_snapshot(pr: &WirePr) -> Self {
        Self::of_wire(pr.state, pr.draft)
    }

    pub(in crate::cockpit) fn of_wire(state: WirePrState, draft: bool) -> Self {
        match (state, draft) {
            (WirePrState::Merged, _) => Self::Merged,
            (WirePrState::Closed, _) => Self::Closed,
            (_, true) => Self::Draft,
            _ => Self::Open,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Draft => "Draft",
            Self::Merged => "Merged",
            Self::Closed => "Closed",
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Self::Merged => icons::GIT_MERGE,
            Self::Draft => icons::GIT_PULL_REQUEST_DRAFT,
            Self::Open | Self::Closed => icons::GIT_PULL_REQUEST,
        }
    }

    /// T3's state tones on Ferrite's hues (`pullRequestIcons`).
    pub(crate) fn ink(self) -> u32 {
        use ferrite_ui::theme::*;
        match self {
            Self::Open => GREEN,
            Self::Draft => INK_SUBTLE,
            Self::Merged => MAGENTA,
            Self::Closed => RED,
        }
    }

    pub(in crate::cockpit) fn settled(self) -> bool {
        matches!(self, Self::Merged | Self::Closed)
    }
}

/// One thing the PR Surface's `⋯` or its primary action does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrAction {
    Ask,
    Explain,
    FixFindings,
    WriteGuide,
    ReadyForReview,
    ConvertToDraft,
    MergeNow,
    AutoMerge,
    AllowEdits,
    Close,
    Revert,
    DeleteBranch,
    CopyLink,
    CopyBranch,
}

impl PrAction {
    fn label(self, guide: bool) -> &'static str {
        match self {
            Self::Ask => "Ask a question",
            Self::Explain => "Explain this PR",
            Self::FixFindings => "Fix findings",
            Self::WriteGuide if guide => "Rewrite guide",
            Self::WriteGuide => "Write guide",
            Self::ReadyForReview => "Ready for review",
            Self::ConvertToDraft => "Convert to draft",
            Self::MergeNow => "Merge now",
            Self::AutoMerge => "Enable auto-merge",
            Self::AllowEdits => "Allow edits from maintainers",
            Self::Close => "Close PR",
            Self::Revert => "Revert changes",
            Self::DeleteBranch => "Delete branch",
            Self::CopyLink => "Copy link",
            Self::CopyBranch => "Copy branch name",
        }
    }

    /// Whether the row asks the Thread's agent (and so is one action in
    /// flight at a time), rather than copying or opening a side chat.
    pub(crate) fn hands_off(self) -> bool {
        !matches!(self, Self::Ask | Self::CopyLink | Self::CopyBranch)
    }

    /// Whether the row arms before it runs.
    fn destructive(self) -> bool {
        matches!(
            self,
            Self::MergeNow | Self::AutoMerge | Self::Close | Self::Revert | Self::DeleteBranch
        )
    }

    /// Whether the row applies to a PR in `tone`.
    fn applies(self, tone: PrTone) -> bool {
        match self {
            Self::ReadyForReview => tone == PrTone::Draft,
            Self::ConvertToDraft | Self::MergeNow => tone == PrTone::Open,
            Self::AutoMerge | Self::AllowEdits | Self::Close | Self::FixFindings => !tone.settled(),
            Self::Revert => tone == PrTone::Merged,
            Self::DeleteBranch => tone.settled(),
            Self::Ask | Self::Explain | Self::WriteGuide | Self::CopyLink | Self::CopyBranch => {
                true
            }
        }
    }

    /// What the Thread's agent is asked, for a row that hands off.
    pub(crate) fn prompt(self, pr: &WirePr) -> Option<String> {
        let n = pr.number;
        let head = &pr.head;
        Some(match self {
            Self::Ask | Self::CopyLink | Self::CopyBranch => return None,
            Self::Explain => format!(
                "Explain pull request #{n} ({url}): what it changes and why, in the order a \
                 reviewer should read it, and anything that deserves a close look. Read it \
                 with `gh pr view {n}` and `gh pr diff {n}`.",
                url = pr.url
            ),
            Self::FixFindings => format!(
                "Address the open review findings on pull request #{n}. Read the unresolved \
                 review comments with `gh pr view {n} --comments`, fix each one on this \
                 branch, commit and push, and say what changed for each."
            ),
            Self::WriteGuide => format!(
                "Write a review guide for pull request #{n}: walk a reviewer through the \
                 change in the order to read it, file by file, with what to check in each. \
                 Post it with `gh pr comment {n}`, its first line `{GUIDE_HEAD}`."
            ),
            Self::ReadyForReview => {
                format!("Mark pull request #{n} ready for review with `gh pr ready {n}`.")
            }
            Self::ConvertToDraft => {
                format!("Convert pull request #{n} to a draft with `gh pr ready {n} --undo`.")
            }
            Self::MergeNow => format!(
                "Merge pull request #{n} now with `gh pr merge {n}`, using the repository's \
                 default merge method, and report the result."
            ),
            Self::AutoMerge => format!(
                "Enable auto-merge on pull request #{n} with `gh pr merge {n} --auto`, using \
                 the repository's default merge method."
            ),
            Self::AllowEdits => format!(
                "Turn on \"Allow edits from maintainers\" for pull request #{n} \
                 (`gh api -X PATCH repos/{{owner}}/{{repo}}/pulls/{n} -F maintainer_can_modify=true`)."
            ),
            Self::Close => format!("Close pull request #{n} with `gh pr close {n}`."),
            Self::Revert => format!(
                "Revert pull request #{n}: open a pull request that reverts its changes \
                 (`gh pr revert {n}`), and report its URL."
            ),
            Self::DeleteBranch => format!(
                "Delete the branch `{head}` of pull request #{n} on the remote with \
                 `git push origin --delete {head}`."
            ),
        })
    }
}

/// The `⋯`'s rows, in T3's groups.
const MENU: [&[PrAction]; 4] = [
    &[
        PrAction::Ask,
        PrAction::Explain,
        PrAction::FixFindings,
        PrAction::WriteGuide,
    ],
    &[
        PrAction::ReadyForReview,
        PrAction::ConvertToDraft,
        PrAction::MergeNow,
        PrAction::AutoMerge,
        PrAction::AllowEdits,
    ],
    &[PrAction::Close, PrAction::Revert, PrAction::DeleteBranch],
    &[PrAction::CopyLink, PrAction::CopyBranch],
];

/// How a review guide's comment begins (`PrAction::WriteGuide`): the Guide
/// view shows the latest comment that does.
const GUIDE_HEAD: &str = "## Review guide";

/// The latest review guide among the PR's comments, which the forge's read
/// lists newest first.
fn guide(pr: &WirePr) -> Option<&str> {
    pr.comments
        .iter()
        .map(|comment| comment.body.as_str())
        .find(|body| body.trim_start().starts_with(GUIDE_HEAD))
}

/// `owner/repo`, out of the PR's address on the forge.
fn repo_of(url: &str) -> Option<String> {
    let path = url
        .split("://")
        .nth(1)?
        .split('/')
        .skip(1)
        .collect::<Vec<_>>();
    match path.as_slice() {
        [owner, repo, ..] if !owner.is_empty() && !repo.is_empty() => {
            Some(format!("{owner}/{repo}"))
        }
        _ => None,
    }
}

/// The checks as the views' trailing slot says them: failures first.
fn checks_line(pr: &WirePr) -> Option<(String, u32)> {
    use ferrite_ui::theme::*;
    if pr.checks.is_empty() {
        return None;
    }
    let count = |status: &[WireCheckStatus]| {
        pr.checks
            .iter()
            .filter(|check| status.contains(&check.status))
            .count()
    };
    let failing = count(&[WireCheckStatus::Failure]);
    let running = count(&[WireCheckStatus::Running, WireCheckStatus::Pending]);
    Some(if failing > 0 {
        (format!("{failing} failing"), RED)
    } else if running > 0 {
        (format!("{running} running"), INK_SUBTLE)
    } else {
        ("Checks passing".into(), INK_SUBTLE)
    })
}

/// A hairline at 60% (T3's `border-border/60`).
pub(in crate::cockpit) fn soft_line() -> gpui::Rgba {
    rgba((ferrite_ui::theme::LINE << 8) | 0x99)
}

/// The pull request reader (ADR 0041): what each PR it shows knows of its
/// pull request, what each action on one became, the Merge waiting for
/// its second press, and the prose it renders. One view for the window:
/// the Panel's PR Surface is its render, and the PRs destination, which
/// holds it as its child, draws its detail with it (`pr_content`), so the
/// two may stand in one frame while the Panel slides shut.
pub(crate) struct PullRequestView {
    /// The PR the Panel shows, as the window has it.
    inputs: Source<PrInputs>,
    /// Whether an answer for a PR is still wanted: a Thread's tab closed
    /// meanwhile wants none.
    wanted: Source<bool, PrTarget>,
    /// Code's body in the Panel: Changes' pane of the PR, the root's view.
    code: AnyView,
    /// Where the PR's number was last drawn, for the menu ⇧F10 opens from
    /// it to hang from.
    number_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Each PR read whole, a Thread's tab's or a Project's.
    pub(super) reads: PrReads,
    /// What a PR's prose (its body, guide and comments) renders through.
    rich: ferrite_pane::rich::TextCache,
    /// What each PR's action in flight has become.
    pub(super) actions: PrActions,
    /// The PR whose Merge waits for its second press.
    armed: Option<PrTarget>,
    /// When the operator last chose another view of a pull request
    /// (Summary, Guide, Timeline, Code): its body fades in from then.
    picked: Option<std::time::Instant>,
}

/// A PR as the window shows it this frame, in the Panel or the PRs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PrShown {
    pub target: PrTarget,
    /// The view chosen, as its Surface or the PRs keep it.
    pub tab: PrTab,
    /// Whether it is the PR of its Thread's branch: only that one can be
    /// asked to change (Merge, Write guide).
    pub branch: bool,
}

/// One of the PR's controls that is a stop of the Panel's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PrStop {
    Number,
    Primary,
    More,
    View(PrTab),
}

/// What the Panel's PR Surface draws from, as the Cockpit has it.
pub(crate) struct PrInputs {
    pub shown: PrShown,
    /// The nav's reading of the Thread's PR, its state and address: the
    /// header's until the PR is read.
    pub reading: Option<(PrTone, Option<String>)>,
    /// The `⋯` button's door.
    pub more: MenuDoor,
    /// The handle of each stop `tab_stops` declared, as the window entered
    /// them in the Panel's row.
    pub stops: HashMap<PrStop, FocusHandle>,
}

/// What the reader asks of the window.
pub(crate) enum PrEvent {
    /// Ask the forge for the PR again: a failed read's Retry, or the read
    /// a failed action's Retry waits on.
    Read(PrTarget),
    /// A view chosen: shown, and kept where the PR is shown.
    SelectTab(PrTarget, PrTab),
    /// Code came near: read the PR's published files.
    EnsureCode(PrTarget),
    /// Merge confirmed, Ready for review, Write guide, or a failed
    /// hand-off's Retry: ask the Thread's agent.
    Run(PrTarget, PrAction),
    /// The `⋯` pressed: its menu, or to shut it.
    More(PrTarget),
}

impl EventEmitter<PrEvent> for PullRequestView {}

impl CockpitView {
    /// The PR row and the `+` menu's Pull request: the branch's PR as a
    /// Surface of `thread`'s Panel. Nothing with no PR on the branch.
    pub(crate) fn open_pr_surface(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if let Some(number) = self.facts.pr(thread).map(|pr| pr.number) {
            self.upsert_surface(thread, SurfaceId::PullRequest(number), cx);
        }
    }

    /// The Thread's PR tab: the PR it shows, else the one its branch has.
    fn pr_tab_target(&self, thread: ThreadKey) -> Option<PrTarget> {
        let tab = self.panel.surfaces.get(thread).and_then(|state| {
            state
                .active
                .iter()
                .chain(state.tabs.iter())
                .find_map(|id| match id {
                    SurfaceId::PullRequest(number) => Some(*number),
                    _ => None,
                })
        });
        let number = tab.or_else(|| self.facts.pr(thread).map(|pr| pr.number))?;
        Some(PrTarget::thread(thread, number))
    }

    /// The checkout `source` names the repository of.
    pub(in crate::cockpit) fn pr_source_path(
        &self,
        source: PrSource,
        cx: &gpui::App,
    ) -> Option<std::path::PathBuf> {
        match source {
            PrSource::Thread(thread) => self.thread_path(thread, cx),
            PrSource::Project(project) => self
                .cockpit
                .read(cx)
                .project(project)
                .map(|project| project.root.clone()),
        }
    }

    /// Read `target`'s PR whole for its Surface, unless a read is out, or
    /// (without `again`) it has been read. Tests and captures never run the
    /// forge's CLI: only a fixture world's forge answers them.
    pub(in crate::cockpit) fn read_pr_snapshot(
        &mut self,
        target: PrTarget,
        again: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.pr_source_path(target.source, cx) else {
            return;
        };
        if !crate::cockpit::reads_forge(self.cockpit.read(cx), &path) {
            return;
        }
        let Some(request) = self
            .pull_request
            .update(cx, |reader, _| reader.reads.begin(target, again))
        else {
            return;
        };
        let answer = self.cockpit.update(cx, |cockpit, _| {
            cockpit.review_pull_request(target.source.machine(), &path, Some(target.number))
        });
        self.pull_request
            .update(cx, |reader, cx| reader.wait(target, request, answer, cx));
    }

    /// Whether an answer for `target` is still wanted: a Thread's tab
    /// closed meanwhile wants none.
    fn pr_tab_open(&self, target: PrTarget) -> bool {
        match target.source {
            PrSource::Thread(thread) => self
                .panel
                .surfaces
                .get(thread)
                .is_some_and(|state| state.tabs.contains(&SurfaceId::PullRequest(target.number))),
            PrSource::Project(_) => true,
        }
    }

    /// A fixture world's snapshot of `thread`'s PR, as if read.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn fixture_pr_snapshot(&mut self, thread: ThreadKey, pr: WirePr, cx: &mut Context<Self>) {
        let target = PrTarget::thread(thread, pr.number);
        self.pull_request
            .update(cx, |reader, _| reader.reads.install(target, pr));
    }

    /// Ask the forge again for `thread`'s open PR tab: its checks and
    /// conversation move without the PR's number or state moving.
    pub(crate) fn refresh_pr_tab(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let open = self.panel.surfaces.get(thread).is_some_and(|state| {
            state
                .tabs
                .iter()
                .any(|id| matches!(id, SurfaceId::PullRequest(_)))
        });
        if let Some(target) = self.pr_tab_target(thread).filter(|_| open) {
            self.read_pr_snapshot(target, true, cx);
        }
    }

    /// The nav's reading of `thread`'s PR changed. A tab for another PR
    /// than the branch's goes, and a snapshot the reading has overtaken (a
    /// merge the agent ran) is let go, until its read lands: the reading
    /// speaks. Any other change asks the forge again, as checks do.
    pub(crate) fn pr_reading_changed(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(reading) = self.facts.pr(thread) else {
            return;
        };
        let target = PrTarget::thread(thread, reading.number);
        let tone = PrTone::of_reading(reading.state, reading.draft);
        self.pull_request
            .update(cx, |reader, cx| reader.overtaken(target, tone, cx));
        self.refresh_pr_tab(thread, cx);
    }

    /// The PR the Panel shows this frame, if it shows one.
    pub(super) fn shown_pr(&self, cx: &gpui::App) -> Option<PrTarget> {
        let index = self.panel_pane(cx)?;
        match (self.panes[index].thread()?, self.shown_surface(index, cx)?) {
            (thread, SurfaceId::PullRequest(number)) => Some(PrTarget::thread(thread, number)),
            _ => None,
        }
    }

    /// The snapshot of `thread`'s PR, once read.
    pub(crate) fn pr_snapshot<'a>(&self, thread: ThreadKey, cx: &'a gpui::App) -> Option<&'a WirePr> {
        self.pull_request.read(cx).reads.snapshot(self.pr_tab_target(thread)?)
    }

    /// The PR's state: its snapshot's, else the nav's reading's.
    pub(crate) fn pr_tone(&self, thread: ThreadKey, cx: &gpui::App) -> Option<PrTone> {
        self.pr_snapshot(thread, cx)
            .map(PrTone::of_snapshot)
            .or_else(|| {
                self.facts
                    .pr(thread)
                    .map(|pr| PrTone::of_reading(pr.state, pr.draft))
            })
    }

    /// Whether `target` is the PR of its Thread's branch.
    pub(in crate::cockpit) fn branch_pr(&self, target: PrTarget) -> bool {
        match target.source {
            PrSource::Thread(thread) => self
                .facts
                .pr(thread)
                .is_some_and(|pr| pr.number == target.number),
            PrSource::Project(_) => false,
        }
    }

    /// The window's PR reader, reading it through `Source`s over the root,
    /// and Code's body in the Panel, Changes' pane of the PR it shows.
    pub(in crate::cockpit) fn new_pull_request(
        changes: &Entity<ferrite_tools::changes::ChangesView>,
        cx: &mut Context<Self>,
    ) -> Entity<PullRequestView> {
        let root = cx.entity();
        let inputs = Source::of(&root, |root: &Self, _: &(), cx| root.pr_inputs(cx));
        let wanted = Source::of(&root, |root: &Self, target: &PrTarget, _| Some(root.pr_tab_open(*target)));
        let shown = Source::of(&root, |root: &Self, _: &(), cx| {
            let (shown, _) = root.panel_pr(cx)?;
            root.pr_code(shown)
        });
        let code = cx.new(|_| ferrite_tools::changes::ChangesPane::new(changes.clone(), shown));
        cx.new(|_| PullRequestView::new(inputs, wanted, code.into()))
    }

    /// Code's body for `shown`, while Code is its view: the PR's published
    /// files, as Changes draws them.
    pub(in crate::cockpit) fn pr_code(
        &self,
        shown: PrShown,
    ) -> Option<(ferrite_tools::changes::Diff, ferrite_tools::changes::ChangesInputs)> {
        let diff = ferrite_tools::changes::Diff::PullRequest(shown.target);
        (shown.tab == PrTab::Code).then(|| (diff, self.changes_inputs(diff)))
    }

    /// `target` in `tab`, as its reader draws it.
    pub(in crate::cockpit) fn pr_shown(&self, target: PrTarget, tab: PrTab) -> PrShown {
        PrShown {
            target,
            tab,
            branch: self.branch_pr(target),
        }
    }

    /// The PR the Panel draws this frame, and the nav's reading of it.
    fn panel_pr(&self, cx: &gpui::App) -> Option<(PrShown, Option<(PrTone, Option<String>)>)> {
        let thread = self.panes[self.panel_drawn_pane(cx)?].thread()?;
        let Some(SurfaceId::PullRequest(number)) = self.active_surface(thread) else {
            return None;
        };
        let tab = self
            .panel
            .surfaces
            .get(thread)
            .map_or(PrTab::Summary, |state| state.pr_tab(number));
        let reading = self
            .facts
            .pr(thread)
            .map(|pr| (PrTone::of_reading(pr.state, pr.draft), pr.url.clone()));
        Some((self.pr_shown(PrTarget::thread(thread, number), tab), reading))
    }

    /// Everything the Panel's PR Surface draws from: the PR the Panel shows.
    fn pr_inputs(&self, cx: &gpui::App) -> Option<PrInputs> {
        let (shown, reading) = self.panel_pr(cx)?;
        Some(PrInputs {
            shown,
            reading,
            more: self.doors().door(MenuTarget::PrMore(shown.target)),
            stops: self.panel.focus.handles(|control| match control {
                PanelControl::PrNumber => Some(PrStop::Number),
                PanelControl::PrPrimary => Some(PrStop::Primary),
                PanelControl::PrMore => Some(PrStop::More),
                PanelControl::PrView(tab) => Some(PrStop::View(*tab)),
                _ => None,
            }),
        })
    }

    /// The PR Surface's stops, as the reader declares them, enter the
    /// Panel's row after the head's, before the reader draws them.
    pub(in crate::cockpit) fn register_pr_stops(&self, cx: &mut Context<Self>) {
        let Some((shown, reading)) = self.panel_pr(cx) else {
            return;
        };
        let stops = self
            .pull_request
            .read(cx)
            .tab_stops(shown, reading.map(|(tone, _)| tone));
        for (stop, semantics) in stops {
            let control = match stop {
                PrStop::Number => PanelControl::PrNumber,
                PrStop::Primary => PanelControl::PrPrimary,
                PrStop::More => PanelControl::PrMore,
                PrStop::View(tab) => PanelControl::PrView(tab),
            };
            self.panel.focus.register(control, semantics, cx);
        }
    }

    pub(in crate::cockpit) fn on_pr_event(
        &mut self,
        _: Entity<PullRequestView>,
        event: &PrEvent,
        cx: &mut Context<Self>,
    ) {
        match *event {
            PrEvent::Read(target) => self.read_pr_snapshot(target, true, cx),
            PrEvent::SelectTab(target, tab) => {
                if tab == PrTab::Code {
                    self.ensure_diff(ferrite_tools::changes::Diff::PullRequest(target), cx);
                }
                match target.source {
                    PrSource::Thread(thread) => {
                        let number = target.number;
                        self.apply_surface(thread, SurfaceAction::SelectPrTab { number, tab });
                    }
                    PrSource::Project(_) => self.prs.update(cx, |prs, cx| prs.set_tab(tab, cx)),
                }
                cx.notify();
            }
            PrEvent::EnsureCode(target) => self.ensure_diff(ferrite_tools::changes::Diff::PullRequest(target), cx),
            PrEvent::Run(target, action) => self.run_pr_action(target, action, cx),
            PrEvent::More(target) => self.toggle_door(pr_more_door(target), cx),
        }
    }
}

impl PullRequestView {
    pub(in crate::cockpit) fn new(
        inputs: Source<PrInputs>,
        wanted: Source<bool, PrTarget>,
        code: AnyView,
    ) -> Self {
        Self {
            inputs,
            wanted,
            code,
            number_at: Rc::default(),
            reads: PrReads::default(),
            rich: Default::default(),
            actions: PrActions::default(),
            armed: None,
            picked: None,
        }
    }

    /// When the operator last chose another view, which the body fades from.
    #[cfg(test)]
    pub(in crate::cockpit) fn picked(&self) -> Option<std::time::Instant> {
        self.picked
    }

    /// The snapshot of `target`, once read.
    pub(in crate::cockpit) fn snapshot(&self, target: PrTarget) -> Option<&WirePr> {
        self.reads.snapshot(target)
    }

    /// Wait on the forge's `answer` to `request` for `target`.
    fn wait(
        &mut self,
        target: PrTarget,
        request: u64,
        answer: std::io::Result<
            std::sync::mpsc::Receiver<std::io::Result<ferrite_core::protocol::ReviewPullRequest>>,
        >,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let read = ferrite_ui::answer::answered(answer, cx).await;
            this.update(cx, |view, cx| view.landed(target, request, read, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    /// The forge answered `request` for `target`. Only the request the
    /// target still waits on, for a tab still open, changes anything: a
    /// tab closed or a read asked again meanwhile leaves the answer unheard.
    fn landed(
        &mut self,
        target: PrTarget,
        request: u64,
        read: std::io::Result<ferrite_core::protocol::ReviewPullRequest>,
        cx: &mut Context<Self>,
    ) {
        if !self.wanted.read(&target, cx).unwrap_or(false) {
            self.reads.forget(target);
            return;
        }
        let answer = match read {
            Ok(read) => match read.pr {
                Ok(pr) if pr.number == target.number => Answer::Read(Box::new(pr)),
                Ok(pr) => Answer::Failed(PrFailure {
                    kind: WirePrUnread::Failed,
                    note: format!(
                        "Requested pull request #{}, but received #{}",
                        target.number, pr.number
                    ),
                }),
                Err(note) => match read.unread {
                    Some(kind) => Answer::Failed(PrFailure { kind, note }),
                    None => Answer::Gone(PrFailure {
                        kind: WirePrUnread::Failed,
                        note,
                    }),
                },
            },
            Err(error) => Answer::Failed(PrFailure {
                kind: WirePrUnread::Failed,
                note: error.to_string(),
            }),
        };
        if self.reads.finish(target, request, answer) {
            self.action_checked(target, cx);
            cx.notify();
        }
    }

    /// The nav reads `target` as `tone`: a snapshot it has overtaken goes.
    fn overtaken(&mut self, target: PrTarget, tone: PrTone, cx: &mut Context<Self>) {
        if self
            .reads
            .snapshot(target)
            .is_some_and(|pr| PrTone::of_snapshot(pr) != tone)
        {
            self.reads.forget(target);
            cx.notify();
        }
    }

    /// Merge armed on a PR Surface no longer `shown` (another tab, the
    /// Panel hidden, another Thread or PR) is disarmed: one press, much
    /// later, must not merge. The root asks every frame, the Surface
    /// drawn or not.
    pub(in crate::cockpit) fn disarm_unless(&mut self, shown: Option<PrTarget>) {
        if self.armed.is_some() && self.armed != shown {
            self.armed = None;
        }
    }

    /// Forget what was read of, and done to, the PRs of Threads that are gone.
    pub(in crate::cockpit) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        let kept = |target: &PrTarget| match target.source {
            PrSource::Thread(thread) => keep(thread),
            PrSource::Project(_) => true,
        };
        self.reads.retain(kept);
        self.actions.retain(kept);
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.reads.held_for_test(thread)
    }

    /// A view of `target`'s PR chosen; `pressed` when it is the one shown.
    fn select(&mut self, target: PrTarget, tab: PrTab, pressed: bool, cx: &mut Context<Self>) {
        if !pressed {
            self.picked = Some(cx.background_executor().now());
        }
        if let PrSource::Thread(_) = target.source {
            self.armed = None;
            self.actions.clear_done(target);
        }
        cx.emit(PrEvent::SelectTab(target, tab));
        cx.notify();
    }

    /// The `⋯`'s rows for `target`'s PR as it stands: a row the forge's
    /// reading rules out, or one an action in flight holds, says why.
    /// `branch_pr`: whether it is its Thread's branch's.
    pub(in crate::cockpit) fn menu_rows(
        &self,
        target: PrTarget,
        branch_pr: bool,
    ) -> Vec<Option<(menu::Item, PrAction)>> {
        let PrSource::Thread(_) = target.source else {
            return Vec::new();
        };
        let snapshot = self.reads.snapshot(target);
        let Some(tone) = snapshot.map(PrTone::of_snapshot) else {
            return Vec::new();
        };
        let guide = snapshot.and_then(guide).is_some();
        let busy = self.actions.pending(target);
        let mut rows = Vec::new();
        for group in MENU {
            let group: Vec<_> = group
                .iter()
                .filter(|action| {
                    action.applies(tone)
                        && (branch_pr
                            || matches!(
                                action,
                                PrAction::Ask | PrAction::CopyLink | PrAction::CopyBranch
                            ))
                })
                .collect();
            if group.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(None);
            }
            for action in group {
                let mut item = menu::Item::new(action.label(guide));
                if action.destructive() {
                    item = item.destructive();
                }
                if action.hands_off() {
                    if let Some(why) = snapshot.and_then(|pr| action.blocked_by(pr)) {
                        item = item.disabled_because(why);
                    } else if busy {
                        item = item.disabled_because("Another action on this PR is in flight");
                    }
                }
                rows.push(Some((item, *action)));
            }
        }
        rows
    }

    /// The PR's state as its header shows it: its snapshot's, else the
    /// nav's `reading`, else open.
    fn tone(&self, target: PrTarget, reading: Option<PrTone>) -> PrTone {
        self.reads
            .snapshot(target)
            .map(PrTone::of_snapshot)
            .or(reading)
            .unwrap_or(PrTone::Open)
    }

    /// An open PR's primary action and what it says: Merge, armed on the
    /// first press; a draft's, Ready for review.
    fn primary(&self, target: PrTarget, tone: PrTone) -> (PrAction, &'static str) {
        match tone {
            PrTone::Draft => (PrAction::ReadyForReview, "Ready for review"),
            _ if self.armed == Some(target) => (PrAction::MergeNow, "Confirm merge?"),
            _ => (PrAction::MergeNow, "Merge"),
        }
    }

    /// The PR Surface's controls as stops of the Panel's row, in the order
    /// it draws them: the number, the primary action while it shows, the
    /// `⋯`, then the views. `reading` is the nav's reading of its state.
    pub(in crate::cockpit) fn tab_stops(
        &self,
        shown: PrShown,
        reading: Option<PrTone>,
    ) -> Vec<(PrStop, Semantics)> {
        let target = shown.target;
        let tone = self.tone(target, reading);
        let mut stops = vec![(
            PrStop::Number,
            Semantics::button(format!("Pull request #{}, open on GitHub", target.number)),
        )];
        if !tone.settled() && shown.branch {
            stops.push((PrStop::Primary, Semantics::button(self.primary(target, tone).1)));
        }
        stops.push((PrStop::More, Semantics::button("More pull request actions")));
        stops.extend(
            PrTab::ALL.map(|tab| (PrStop::View(tab), Semantics::tab(tab.label(), tab == shown.tab))),
        );
        stops
    }

    /// Where the PR's number was last drawn: the menu ⇧F10 opens from it
    /// hangs from there.
    pub(in crate::cockpit) fn number_bounds(&self) -> Option<Bounds<Pixels>> {
        self.number_at.get()
    }

    /// The PR Surface: header, then its content.
    fn surface(&self, inputs: PrInputs, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let PrInputs {
            shown,
            reading,
            more,
            stops,
        } = inputs;
        let target = shown.target;
        let code = (shown.tab == PrTab::Code).then(|| self.code.clone());
        let this = cx.weak_entity();
        div()
            .debug_selector(|| "pr".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .text_size(px(FS_BADGE))
            .child(self.pr_header(target, shown.branch, reading, more, &stops, cx))
            .children(self.pr_action_notice(target, cx))
            .child(self.pr_content(shown, code, &stops, &this))
            .into_any_element()
    }

    /// A PR's views, the notice of its newest read, and the view shown,
    /// whichever surface asks: the Panel's, or the PRs' detail, which
    /// holds this reader as its child and draws it with a read. `code` is
    /// Code's body, `stops` the views' handles, `this` the reader their
    /// presses go to.
    pub(in crate::cockpit) fn pr_content(
        &self,
        shown: PrShown,
        code: Option<AnyView>,
        stops: &HashMap<PrStop, FocusHandle>,
        this: &WeakEntity<Self>,
    ) -> Div {
        let PrShown { target, tab, branch } = shown;
        let read = self.reads.get(target);
        let snapshot = read.and_then(|read| read.snapshot.as_ref());
        let body = match (snapshot, read.and_then(PrRead::failure)) {
            (Some(pr), _) => match tab {
                PrTab::Summary => self.pr_summary(target, pr),
                PrTab::Guide => self.pr_guide(target, branch, pr, this),
                PrTab::Timeline => self.pr_timeline(target, pr),
                PrTab::Code => code.map_or_else(|| div().into_any_element(), AnyView::into_any_element),
            },
            (None, Some(failure)) => pr_note(failure.note.clone().into()),
            (None, None) => pr_note("Reading\u{2026}".into()),
        };
        let notice = read.and_then(|read| match (read.reading(), read.failure()) {
            (true, _) if snapshot.is_some() => Some(pr_refreshing()),
            (_, Some(failure)) => Some(pr_failed(target, failure, snapshot.is_some(), this)),
            _ => None,
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(rgb(ferrite_ui::theme::TX_GROUND))
            .children(snapshot.map(pr_identity))
            .child(self.pr_views(target, tab, stops, this))
            .children(notice)
            .child(ferrite_ui::motion::swap_since(
                self.picked,
                div().flex().flex_col().flex_1().min_h_0().child(body),
            ))
    }

    /// T3's header grid: the crumbs, then the actions.
    fn pr_header(
        &self,
        target: PrTarget,
        branch_pr: bool,
        reading: Option<(PrTone, Option<String>)>,
        more: MenuDoor,
        stops: &HashMap<PrStop, FocusHandle>,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let number = target.number;
        let (reading_tone, reading_url) = reading.unzip();
        let tone = self.tone(target, reading_tone);
        let repo = self
            .reads
            .snapshot(target)
            .map(|pr| pr.url.clone())
            .or(reading_url.flatten())
            .and_then(|url| repo_of(&url));
        let number_semantics = Semantics::button(format!("Pull request #{number}, open on GitHub"));
        let url = self.reads.snapshot(target).map(|pr| pr.url.clone());
        let number_at = self.number_at.clone();
        let crumbs = div()
            .debug_selector(|| "pr-crumbs".into())
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(PR_ROW_H))
            .pl(px(SPACE_4))
            .children(repo.map(|repo| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(format!("{repo} /")))
            }))
            .child(
                div()
                    .id("pr-number")
                    .debug_selector(|| "pr-number".into())
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .font_weight(W_LABEL)
                    .text_color(rgb(tone.ink()))
                    .cursor_pointer()
                    .hover(|link| link.underline())
                    .tooltip(ferrite_ui::menu::tooltip("Open on GitHub"))
                    .map(|link| match stops.get(&PrStop::Number) {
                        Some(handle) => controls::focusable(link, handle, &number_semantics),
                        None => link,
                    })
                    .child(
                        gpui::canvas(move |at, _, _| number_at.set(Some(at)), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(SharedString::from(format!("#{number}")))
                    .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        if let Some(url) = &url {
                            cx.open_url(url);
                        }
                    }),
            );
        let primary = if tone.settled() || !branch_pr {
            pr_pill(tone).into_any_element()
        } else {
            self.pr_primary(target, tone, stops, cx)
        };
        let more_semantics = Semantics::button("More pull request actions");
        let face = match stops.get(&PrStop::More) {
            Some(handle) => controls::icon_stop(
                "pr-more",
                icons::ELLIPSIS,
                IconSize::Standard,
                handle,
                &more_semantics,
                cx,
            ),
            None => ferrite_ui::components::icon_button(
                "pr-more",
                icons::ELLIPSIS,
                IconSize::Standard,
                more_semantics.name.clone(),
                cx,
            ),
        };
        let press_more = cx.listener(move |_, _: &(), _, cx| cx.emit(PrEvent::More(target)));
        let more = more.button(face, "More", move |window, cx| press_more(&(), window, cx));
        div()
            .debug_selector(|| "pr-header".into())
            .bg(rgb(ferrite_ui::theme::TX_GROUND))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .border_b(px(HAIRLINE_W))
            .border_color(soft_line())
            .child(crumbs)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .h(px(PR_ROW_H))
                    .mr(px(SPACE_4))
                    .child(primary)
                    .child(more),
            )
    }

    /// An open PR's primary action: Merge, which asks once more before it
    /// hands off; a draft's, Ready for review. While an action is in
    /// flight on the PR, or the forge's reading rules Merge out, it is
    /// there but cannot be pressed (`pr_action_notice` says why).
    fn pr_primary(
        &self,
        target: PrTarget,
        tone: PrTone,
        stops: &HashMap<PrStop, FocusHandle>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let (action, label) = self.primary(target, tone);
        let held = self.actions.pending(target)
            || self
                .reads
                .snapshot(target)
                .is_some_and(|pr| action.blocked_by(pr).is_some());
        let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from("pr-primary"));
        let semantics = Semantics::button(label);
        div()
            .id("pr-primary")
            .debug_selector(|| "pr-primary".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(SURFACE_TAB_H))
            .px(px(PR_SEGMENT_PAD_X))
            .rounded(px(R_CONTROL))
            .bg(paint::RAISED)
            .font_weight(W_LABEL)
            .text_color(rgb(if held { INK_SUBTLE } else { INK_STRONG }))
            .when(!held, |button| button.hover_control(key).press_control())
            .map(|primary| match stops.get(&PrStop::Primary) {
                Some(handle) => controls::focusable(primary, handle, &semantics),
                None => primary,
            })
            .child(label)
            .when(!held, |button| {
                button.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if action == PrAction::MergeNow && view.armed != Some(target) {
                        view.armed = Some(target);
                        cx.notify();
                        return;
                    }
                    view.armed = None;
                    cx.emit(PrEvent::Run(target, action));
                    cx.notify();
                }))
            })
            .into_any_element()
    }

    /// What an action on `target`'s PR stands at, or what keeps Merge from
    /// it, in a line under the header: queued, running, failed with its
    /// retry, done. Merge armed says what it will merge with and what the
    /// panel does not know of it.
    pub(in crate::cockpit) fn pr_action_words(&self, target: PrTarget) -> Option<PrNotice> {
        use ferrite_ui::theme::*;
        let number = target.number;
        let notice = match self.actions.get(target) {
            Some(ActionState::Pending { action, stage, .. }) => {
                let what = match stage {
                    Stage::Handing => "handing it to the agent",
                    Stage::Queued(_) => "queued behind the agent's running turn",
                    Stage::Running => "the agent is on it",
                    Stage::Checking => "checking the PR",
                };
                (
                    format!("{} #{number}: {what}.", action.working()),
                    INK_SUBTLE,
                    vec![("pr-action-dismiss", "Stop waiting", false)],
                )
            }
            Some(ActionState::Failed { action, error, .. }) => (
                format!("{} #{number} failed. {error}", action.working()),
                RED,
                vec![
                    ("pr-action-retry", "Retry", true),
                    ("pr-action-dismiss", "Dismiss", false),
                ],
            ),
            Some(ActionState::Done { action }) => (
                format!("{} #{number}.", action.done()),
                INK_SUBTLE,
                vec![("pr-action-dismiss", "Dismiss", false)],
            ),
            None => {
                let pr = self.reads.snapshot(target)?;
                let (said, ink) = match (PrTone::of_snapshot(pr), pr.merge) {
                    (PrTone::Open, _) if self.armed == Some(target) => (
                        "Merges with the repository's default method. Your permission and the methods it allows are not read here.".to_string(),
                        INK_SUBTLE,
                    ),
                    (PrTone::Open, _) => (PrAction::MergeNow.blocked_by(pr).map(|why| format!("Merge unavailable. {why}"))?, RED),
                    _ => return None,
                };
                (said, ink, Vec::new())
            }
        };
        Some(notice)
    }

    /// `pr_action_words`, drawn.
    fn pr_action_notice(&self, target: PrTarget, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let (said, ink, buttons) = self.pr_action_words(target)?;
        let buttons = buttons.into_iter().map(|(id, label, retry)| {
            let chip =
                pr_chip(id, label).on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if retry {
                        view.retry(target, cx);
                    } else {
                        view.dismiss(target, cx);
                    }
                }));
            // The leaving chip holds one width and the right edge, in
            // every state, so a second click on Retry never lands on the
            // Stop waiting that replaces it.
            if retry {
                chip
            } else {
                chip.w(px(PR_LEAVE_CHIP_W)).justify_center()
            }
        });
        Some(
            div()
                .debug_selector(|| "pr-action".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_4))
                .py(px(SPACE_2))
                .border_b(px(HAIRLINE_W))
                .border_color(soft_line())
                .text_color(rgb(ink))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .child(SharedString::from(said)),
                )
                .children(buttons)
                .into_any_element(),
        )
    }

    /// The views as a segmented group, the checks and comments after it.
    fn pr_views(
        &self,
        target: PrTarget,
        shown: PrTab,
        stops: &HashMap<PrStop, FocusHandle>,
        this: &WeakEntity<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let tabs = PrTab::ALL.into_iter();
        let segments = tabs.map(|tab| {
            let id = SharedString::from(format!("pr-tab-{}", tab.label()));
            let selector = id.to_string();
            let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
            let pressed = tab == shown;
            let semantics = Semantics::tab(tab.label(), pressed);
            let segment = div()
                .id(id)
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .h(px(SURFACE_TAB_H))
                .px(px(PR_SEGMENT_PAD_X))
                .rounded(px(R_CONTROL))
                .text_color(rgb(if pressed { INK_STRONG } else { INK_SUBTLE }))
                .map(|segment| {
                    if pressed {
                        segment.bg(rgba(PR_SEGMENT_PRESSED))
                    } else {
                        segment.hover_raised(key)
                    }
                })
                .map(|segment| match stops.get(&PrStop::View(tab)) {
                    Some(handle) => controls::focusable(segment, handle, &semantics),
                    None => segment,
                })
                .child(tab.label())
                .on_click(on(this, move |view, _: &ClickEvent, _, cx| {
                    view.select(target, tab, pressed, cx);
                }));
            if tab != PrTab::Code {
                return segment.into_any_element();
            };
            // Code's diff is read as it comes near, not after the press:
            // the segment's own hover is its wash's, so a wrapper listens.
            div()
                .id("pr-tab-Code-near")
                .on_hover(on(this, move |_, hovered: &bool, _, cx| {
                    if *hovered {
                        cx.emit(PrEvent::EnsureCode(target));
                    }
                }))
                .child(segment)
                .into_any_element()
        });
        let snapshot = self.reads.snapshot(target);
        let checks = snapshot.and_then(checks_line).map(|(line, ink)| {
            div()
                .debug_selector(|| "pr-checks".into())
                .text_color(rgb(ink))
                .child(SharedString::from(line))
        });
        let comments = snapshot.filter(|pr| !pr.comments.is_empty()).map(|pr| {
            let count = pr.comments.len();
            div()
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(format!(
                    "{count} comment{}",
                    if count == 1 { "" } else { "s" }
                )))
        });
        div()
            .debug_selector(|| "pr-views".into())
            .border_t(px(HAIRLINE_W))
            .border_color(soft_line())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .py(px(SPACE_2))
            .child(
                div()
                    .debug_selector(|| "pr-segments".into())
                    .flex()
                    .flex_shrink_0()
                    .gap(px(SPACE_0_5))
                    .p(px(SPACE_0_5))
                    .rounded(px(PR_GROUP_R))
                    .bg(rgba(PR_GROUP_GROUND))
                    .children(segments),
            )
            .child(div().flex_1())
            .children(checks)
            .children(comments)
    }

    /// `text` as the Markdown it is, in the reading size of a document.
    fn pr_prose(&self, id: String, text: &str) -> AnyElement {
        use ferrite_ui::theme::*;
        div()
            .font_family(FONT_UI)
            .text_size(px(FS_PROSE))
            .line_height(px(LH_PROSE))
            .child(ferrite_pane::rich::Markdown::new(
                id,
                text.to_string(),
                self.rich.clone(),
            ))
            .into_any_element()
    }

    /// Summary: the description and checks below the persistent PR identity.
    fn pr_summary(&self, target: PrTarget, pr: &WirePr) -> AnyElement {
        use ferrite_ui::theme::*;
        let checks = pr
            .checks
            .iter()
            .enumerate()
            .map(|(index, check)| {
                let (glyph, ink) = match check.status {
                    WireCheckStatus::Success => (icons::CHECK_CIRCLE, GREEN),
                    WireCheckStatus::Failure => (icons::X_CIRCLE, RED),
                    WireCheckStatus::Running | WireCheckStatus::Pending => {
                        (icons::CIRCLE_DASHED, YELLOW)
                    }
                    WireCheckStatus::Skipped | WireCheckStatus::Unknown => {
                        (icons::CIRCLE_MINUS, INK_SUBTLE)
                    }
                };
                let row = div()
                    .id(("pr-check", index))
                    .debug_selector(move || format!("pr-check-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(PR_ROW_H))
                    .child(icon(glyph, ICON_CHEVRON, ink))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(check.name.clone())),
                    );
                match check.url.clone() {
                    Some(url) => row
                        .cursor_pointer()
                        .hover(|row| row.underline())
                        .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                        .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            })
            .collect::<Vec<_>>();
        pr_scroll("pr-summary")
            .when(!pr.body.is_empty(), |summary| {
                summary.child(
                    div()
                        .debug_selector(|| "pr-body".into())
                        .mt(px(SPACE_3))
                        .child(self.pr_prose(format!("pr-body-{target:?}"), &pr.body)),
                )
            })
            .when(!pr.checks.is_empty(), |summary| {
                summary
                    .child(
                        div()
                            .mt(px(SPACE_3))
                            .font_weight(W_LABEL)
                            .text_color(rgb(INK_SUBTLE))
                            .child("Checks"),
                    )
                    .children(checks)
            })
            .when(pr.more_checks(), |summary| {
                summary.child(pr_more(
                    "pr-more-checks",
                    "More checks than are shown. See all on the forge",
                    &pr.url,
                ))
            })
            .into_any_element()
    }

    /// Guide: the PR's latest review guide, or the way to ask for one (a
    /// Thread's own PR only: the PRs destination reads and does not ask).
    fn pr_guide(
        &self,
        target: PrTarget,
        branch: bool,
        pr: &WirePr,
        this: &WeakEntity<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        if let Some(text) = guide(pr) {
            return pr_scroll("pr-guide")
                .child(self.pr_prose(format!("pr-guide-{target:?}"), text))
                .into_any_element();
        }
        div()
            .debug_selector(|| "pr-guide-empty".into())
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .text_color(rgb(INK_SUBTLE))
            .child("No review guide yet")
            .when(branch, |empty| {
                let working = matches!(
                    self.actions.get(target),
                    Some(ActionState::Pending {
                        action: PrAction::WriteGuide,
                        ..
                    })
                );
                empty.child(
                    div()
                        .id("pr-write-guide")
                        .debug_selector(|| "pr-write-guide".into())
                        .flex()
                        .items_center()
                        .h(px(SURFACE_TAB_H))
                        .px(px(PR_SEGMENT_PAD_X))
                        .rounded(px(R_CONTROL))
                        .bg(paint::RAISED)
                        .text_color(rgb(if working { INK_SUBTLE } else { INK_STRONG }))
                        .when(!working, |button| {
                            button
                                .hover_control("pr-write-guide")
                                .press_control()
                                .on_click(on(this, move |_, _: &ClickEvent, _, cx| {
                                    cx.emit(PrEvent::Run(target, PrAction::WriteGuide));
                                }))
                        })
                        .child(if working {
                            "Writing guide\u{2026}"
                        } else {
                            "Write guide"
                        }),
                )
            })
            .into_any_element()
    }

    /// Timeline: the conversation as the forge's read lists it, newest
    /// first (T3's default order): each entry's code context, its
    /// resolved or outdated mark, and its replies beneath.
    fn pr_timeline(&self, target: PrTarget, pr: &WirePr) -> AnyElement {
        use ferrite_ui::theme::*;
        if pr.comments.is_empty() {
            return pr_note("No comments yet".into());
        }
        let entries = pr.comments.iter().enumerate().map(|(index, comment)| {
            let kind = match comment.kind {
                WirePrCommentKind::Review => "reviewed",
                WirePrCommentKind::Finding => "commented on",
                WirePrCommentKind::Comment | WirePrCommentKind::Unknown => "commented",
            };
            let what = match comment.kind {
                WirePrCommentKind::Finding => {
                    format!("{} {kind} {}", comment.author, comment.anchor)
                }
                _ => format!("{} {kind}", comment.author),
            };
            let verdict = comment.verdict.and_then(ferrite_ui::pull_request::review_verdict);
            let mark = |text: &'static str, ink: u32| {
                div()
                    .debug_selector(move || format!("pr-{}-{index}", text.to_lowercase()))
                    .flex_shrink_0()
                    .text_color(rgb(ink))
                    .child(text)
            };
            div()
                .id(SharedString::from(format!(
                    "pr-activity-{target:?}-{}",
                    comment
                        .id
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_else(|| index.to_string())
                )))
                .debug_selector(move || format!("pr-comment-{index}"))
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .py(px(SPACE_2))
                .border_b(px(HAIRLINE_W))
                .border_color(soft_line())
                .child(
                    div()
                        .flex()
                        .gap(px(SPACE_2))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(W_LABEL)
                                .text_color(rgb(INK_STRONG))
                                .child(SharedString::from(what)),
                        )
                        .when(comment.author_is_bot, |row| {
                            row.child(
                                div()
                                    .debug_selector(move || format!("pr-bot-{index}"))
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child("bot"),
                            )
                        })
                        .children(verdict.map(|(text, ink)| {
                            div()
                                .debug_selector(move || format!("pr-verdict-{index}"))
                                .flex_shrink_0()
                                .text_color(rgb(ink))
                                .child(text)
                        }))
                        .when(comment.resolved, |head| head.child(mark("Resolved", GREEN)))
                        .when(comment.outdated, |head| {
                            head.child(mark("Outdated", INK_SUBTLE))
                        })
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(INK_SUBTLE))
                                .child(stamp_age(&comment.created_at)),
                        ),
                )
                .children(comment.snippet.as_ref().map(|snippet| {
                    div()
                        .debug_selector(move || format!("pr-snippet-{index}"))
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(R_CONTROL))
                        .bg(paint::RAISED)
                        .font_family(FONT_CODE)
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .overflow_x_hidden()
                        .child(SharedString::from(snippet.clone()))
                }))
                .when(!comment.body.is_empty(), |entry| {
                    entry.child(
                        self.pr_prose(format!("pr-comment-{target:?}-{index}"), &comment.body),
                    )
                })
                .children(comment.replies.iter().enumerate().map(|(reply, said)| {
                    div()
                        .id(SharedString::from(format!(
                            "pr-reply-{target:?}-{}-{}",
                            comment.id.as_deref().unwrap_or("legacy"),
                            said.id
                                .as_deref()
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("{index}-{reply}"))
                        )))
                        .debug_selector(move || format!("pr-reply-{index}-{reply}"))
                        .flex()
                        .flex_col()
                        .ml(px(SPACE_3))
                        .pl(px(SPACE_3))
                        .border_l(px(HAIRLINE_W))
                        .border_color(soft_line())
                        .child(
                            div()
                                .flex()
                                .gap(px(SPACE_2))
                                .text_color(rgb(INK_SUBTLE))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(W_LABEL)
                                        .child(SharedString::from(format!(
                                            "{} replied",
                                            said.author
                                        ))),
                                )
                                .child(div().flex_shrink_0().child(stamp_age(&said.created_at))),
                        )
                        .child(
                            self.pr_prose(
                                format!("pr-reply-{target:?}-{index}-{reply}"),
                                &said.body,
                            ),
                        )
                }))
        });
        pr_scroll("pr-timeline")
            .children(entries)
            .when(pr.more_comments(), |timeline| {
                timeline.child(pr_more(
                    "pr-more-comments",
                    "Older comments are not shown. See all on the forge",
                    &pr.url,
                ))
            })
            .into_any_element()
    }
}

impl Render for PullRequestView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.inputs.read(&(), cx) {
            Some(inputs) => self.surface(inputs, cx),
            None => div().into_any_element(),
        }
    }
}

/// A read that failed: what went wrong and its way back, over the last
/// good content when there is any.
fn pr_failed(
    target: PrTarget,
    failure: &PrFailure,
    stale: bool,
    this: &WeakEntity<PullRequestView>,
) -> AnyElement {
    use ferrite_ui::theme::*;
    let said = if stale {
        format!(
            "{}. Showing the last read. {}",
            failure.headline(),
            failure.note
        )
    } else {
        failure.note.clone()
    };
    let retry = pr_chip("pr-retry", "Retry").on_click(on(this, move |_, _: &ClickEvent, _, cx| {
        cx.stop_propagation();
        cx.emit(PrEvent::Read(target));
    }));
    div()
        .debug_selector(|| "pr-failed".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_2))
        .px(px(SPACE_4))
        .py(px(SPACE_2))
        .border_b(px(HAIRLINE_W))
        .border_color(soft_line())
        .text_color(rgb(RED))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_normal()
                .child(SharedString::from(said)),
        )
        .child(retry)
        .into_any_element()
}

/// A press or hover on the reader, made outside its own update, as
/// `cx.listener` makes one inside it: the PRs draw their detail with the
/// reader, and its presses still go to it.
fn on<E: ?Sized + 'static>(
    this: &WeakEntity<PullRequestView>,
    act: impl Fn(&mut PullRequestView, &E, &mut Window, &mut Context<PullRequestView>) + 'static,
) -> impl Fn(&E, &mut Window, &mut gpui::App) + 'static {
    let this = this.clone();
    move |event, window, cx| {
        this.update(cx, |view, cx| act(view, event, window, cx)).ok();
    }
}

/// The way past a read's cap: `url`, the PR where the forge holds the rest.
fn pr_more(id: &'static str, said: &'static str, url: &str) -> AnyElement {
    use ferrite_ui::theme::*;
    let url = url.to_string();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(SPACE_1))
        .mt(px(SPACE_2))
        .cursor_pointer()
        .text_color(rgb(INK_SUBTLE))
        .hover(|link| link.underline())
        .child(said)
        .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
        .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url))
        .into_any_element()
}

/// A merged or closed PR's pill (T3's outline Badge): its state's icon and
/// word in its ink.
pub(in crate::cockpit) fn pr_pill(tone: PrTone) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-pill".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(SURFACE_TAB_H))
        .min_w(px(SURFACE_TAB_H))
        .px(px(PR_PILL_PAD_X))
        .rounded(px(PR_GROUP_R))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgba(PR_PILL_GROUND))
        .font_weight(W_LABEL)
        .text_color(rgb(tone.ink()))
        .child(icon(tone.glyph(), SURFACE_PLUS_GLYPH, tone.ink()))
        .child(tone.label())
}

/// A refresh over the last good content: it stays, a mark turns above it.
fn pr_refreshing() -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-refreshing".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_2))
        .px(px(SPACE_4))
        .py(px(SPACE_2))
        .border_b(px(HAIRLINE_W))
        .border_color(soft_line())
        .text_color(rgb(INK_SUBTLE))
        .child(ferrite_ui::components::working_mark(
            PR_LINK_GLYPH,
            INK_SUBTLE,
        ))
        .child("Refreshing\u{2026}")
        .into_any_element()
}

/// What a PR's notice strip says: its words, their ink, and its chips as
/// (selector, label, whether it retries).
type PrNotice = (String, u32, Vec<(&'static str, &'static str, bool)>);

/// The width of the chip that dismisses a notice, whatever it is called.
const PR_LEAVE_CHIP_W: f32 = 112.;

fn pr_identity(pr: &WirePr) -> Div {
    use ferrite_ui::theme::*;
    let author = pr.author.as_ref().map(|author| {
        div()
            .debug_selector(|| "pr-author".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .bg(paint::RAISED)
                    .children(pr.author_avatar_url.as_ref().map(|url| {
                        gpui::img(SharedString::from(url.clone()))
                            .size(px(16.))
                            .rounded_full()
                    })),
            )
            .child(SharedString::from(author.clone()))
    });
    let updated = pr.updated_at.as_ref().map(|stamp| {
        let age = stamp_age(stamp);
        let label = if !age.is_empty() {
            format!("{age} ago")
        } else if ferrite_core::clock::parse_stamp(stamp).is_some() {
            "just now".into()
        } else {
            stamp.clone()
        };
        div()
            .debug_selector(|| "pr-updated".into())
            .child(SharedString::from(format!("updated {label}")))
    });
    let files = pr.changed_files.map(|count| {
        div()
            .debug_selector(|| "pr-files".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon(icons::DIFF, 14., INK_SUBTLE))
            .child(SharedString::from(format!(
                "{count} {}",
                if count == 1 { "file" } else { "files" }
            )))
    });
    div()
        .debug_selector(|| "pr-identity".into())
        .flex_shrink_0()
        .min_w_0()
        .px(px(16.))
        .pt(px(4.))
        .pb(px(16.))
        .text_size(px(12.))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .debug_selector(|| "pr-title".into())
                .truncate()
                .text_size(px(16.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(INK_STRONG))
                .child(SharedString::from(pr.title.clone())),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(20.))
                .mt(px(8.))
                .children(author)
                .when(pr.author.is_some() && pr.updated_at.is_some(), |row| {
                    row.child("·")
                })
                .children(updated)
                .child(div().flex_1())
                .children(pr.url.contains("/pull/").then(|| {
                    pr_copyable(
                        "pr-copy-checkout",
                        format!("gh pr checkout {}", pr.number),
                        "Copy checkout command",
                    )
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(20.))
                .mt(px(16.))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.))
                        .font_family(FONT_CODE)
                        .child(div().truncate().child(SharedString::from(pr.base.clone())))
                        .child("←")
                        .child(pr_copyable(
                            "pr-copy-head",
                            pr.head.clone(),
                            "Copy branch name",
                        )),
                )
                .children(files)
                .children(pr.additions.map(|count| {
                    div()
                        .debug_selector(|| "pr-additions".into())
                        .font_family(FONT_CODE)
                        .text_color(rgb(GREEN))
                        .child(SharedString::from(format!("+{count}")))
                }))
                .children(pr.deletions.map(|count| {
                    div()
                        .debug_selector(|| "pr-deletions".into())
                        .font_family(FONT_CODE)
                        .text_color(rgb(RED))
                        .child(SharedString::from(format!("−{count}")))
                })),
        )
}

fn pr_copyable(id: &'static str, text: String, tooltip: &'static str) -> gpui::Stateful<Div> {
    let value = text.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .min_w_0()
        .truncate()
        .font_family(ferrite_ui::theme::FONT_CODE)
        .cursor_pointer()
        .tooltip(ferrite_ui::menu::tooltip(tooltip))
        .hover(|copy| copy.text_color(rgb(ferrite_ui::theme::INK_STRONG)))
        .child(SharedString::from(text))
        .on_click(move |_: &ClickEvent, _, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(value.clone()))
        })
}

fn pr_chip(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(id)
        .debug_selector(|| id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(SURFACE_TAB_H))
        .px(px(PR_SEGMENT_PAD_X))
        .rounded(px(R_CONTROL))
        .bg(paint::RAISED)
        .font_weight(W_LABEL)
        .text_color(rgb(INK_STRONG))
        .hover_control(ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id)))
        .press_control()
        .child(label)
}

fn pr_note(note: SharedString) -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-note".into())
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(px(FS_LABEL))
        .text_color(rgb(INK_SUBTLE))
        .child(note)
        .into_any_element()
}

/// A view's own scroll, wrapping its prose.
fn pr_scroll(selector: &'static str) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(SPACE_4))
        .py(px(SPACE_3))
        .whitespace_normal()
        .text_color(rgb(INK_STRONG))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_guide_wins_among_a_newest_first_read() {
        use ferrite_core::protocol::WirePrComment;
        let said = |body: &str| WirePrComment {
            id: None,
            author_is_bot: false,
            place: None,
            kind: WirePrCommentKind::Comment,
            author: "bot".into(),
            anchor: "comment".into(),
            verdict: None,
            body: body.into(),
            snippet: None,
            created_at: String::new(),
            resolved: false,
            outdated: false,
            replies: Vec::new(),
        };
        use ferrite_core::protocol::{WireMerge, WireSync};
        let pr = WirePr {
            number: 1,
            title: String::new(),
            url: String::new(),
            body: String::new(),
            author: None,
            author_avatar_url: None,
            updated_at: None,
            changed_files: None,
            additions: None,
            deletions: None,
            state: WirePrState::Open,
            draft: false,
            head: String::new(),
            base: String::new(),
            merge: WireMerge::Unknown,
            sync: WireSync::InSync,
            checks: Vec::new(),
            comments: vec![
                said("## Review guide\nnewest"),
                said("thanks"),
                said("## Review guide\noldest"),
            ],
            truncated: false,
            checks_truncated: false,
            comments_truncated: false,
        };
        assert_eq!(guide(&pr), Some("## Review guide\nnewest"));
    }

    #[test]
    fn the_repo_comes_from_the_prs_address() {
        assert_eq!(
            repo_of("https://github.com/o/r/pull/48").as_deref(),
            Some("o/r")
        );
        assert_eq!(repo_of("not a url"), None);
    }

    #[test]
    fn each_state_offers_only_what_applies() {
        let offered = |tone| -> Vec<PrAction> {
            MENU.iter()
                .flat_map(|group| group.iter().copied())
                .filter(|action| action.applies(tone))
                .collect()
        };
        assert!(offered(PrTone::Open).contains(&PrAction::MergeNow));
        assert!(!offered(PrTone::Open).contains(&PrAction::Revert));
        assert!(offered(PrTone::Draft).contains(&PrAction::ReadyForReview));
        assert!(!offered(PrTone::Draft).contains(&PrAction::MergeNow));
        let merged = offered(PrTone::Merged);
        assert!(merged.contains(&PrAction::Revert) && merged.contains(&PrAction::DeleteBranch));
        assert!(!merged.contains(&PrAction::Close));
    }
}

/// The PR header's `⋯` menu, hung from its right edge.
pub(super) fn pr_more_door(target: PrTarget) -> Door {
    Door::new(MenuTarget::PrMore(target), Edge::Right)
}
