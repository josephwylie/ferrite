//! The window's half of the pull request Surface: the PR reader
//! (`ferrite_prs::pull_request::PullRequestView`) is the window's one, and
//! the root makes it over itself. What the reader draws of the window it
//! reads through the `Source`s made here (`pr_inputs`, whether a PR is
//! still wanted); what it asks (a read, a view kept, Code's diff, a
//! hand-off, the `⋯`) it says as a `PrEvent`, answered in `on_pr_event`.
//! The forge and the Thread's agent are the window's, so the root asks
//! them and hands the answers to the reader. Its controls are stops of the
//! Panel's row: the reader declares them (`tab_stops`), the root enters
//! them as it draws the Surface (`register_pr_stops`) and hands each one's
//! handle back through the inputs.

use ferrite_core::protocol::WirePr;
use ferrite_core::pull_request::{PrSource, PrTarget};
use ferrite_prs::pull_request::{PrEvent, PrInputs, PrShown, PrStop, PrTone, PullRequestView};
use ferrite_prs::PrTab;
use ferrite_ui::source::Source;

use super::*;
use crate::cockpit::menu_button::{Door, Edge};

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
            .update(cx, |reader, _| reader.begin_read(target, again))
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
            .update(cx, |reader, _| reader.install(target, pr));
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
        self.pull_request.read(cx).snapshot(self.pr_tab_target(thread)?)
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

/// The PR header's `⋯` menu, hung from its right edge.
pub(super) fn pr_more_door(target: PrTarget) -> Door {
    Door::new(MenuTarget::PrMore(target), Edge::Right)
}

impl CockpitView {
    /// Open the Thread's PR in its browser preview. An explicit open: the
    /// Thread comes forward first, so the page is the one shown.
    pub(crate) fn open_pr(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let url = self.facts.pr(thread).and_then(|pr| pr.url.clone());
        if let Some(url) = url {
            self.show_thread(thread, cx);
            self.browser_action(thread, ferrite_core::browser::Action::Open { url }, cx);
        }
    }

    /// The Thread's PR as a link target: the one every door to it opens.
    pub(crate) fn pr_target(&self, thread: ThreadKey) -> Option<LinkTarget> {
        self.facts.pr(thread).map(|pr| LinkTarget::PullRequest {
            thread: thread.thread(),
            number: pr.number,
            url: pr.url.clone(),
        })
    }

    /// An explicit open of the Thread's PR, by the door `how` names: its
    /// native detail, the integrated browser, or the system's. What cannot
    /// open says why.
    pub(crate) fn open_pr_as(
        &mut self,
        thread: ThreadKey,
        how: Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.pr_target(thread) else {
            return;
        };
        let address = target.address();
        match (how, address) {
            (Open::Primary, _) => {
                self.show_thread(thread, cx);
                self.open_pr_surface(thread, cx);
            }
            (Open::Integrated, Ok(_)) => self.open_pr(thread, cx),
            (Open::System, Ok(url)) => cx.open_url(&url),
            (_, Err(why)) => ferrite_pane::link_target::notify_unavailable(&why, window, cx),
        }
    }
}
