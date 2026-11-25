//! The window's half of the PRs destination
//! (`ferrite_prs::prs::PrsView`): the root makes it over itself, with the
//! window's PR reader as its child and Code's body, Changes' pane of the PR
//! whose detail shows. What it shows of the window it reads through the
//! `Source` made here (`prs_inputs`); what it asks (a list read, a PR
//! read, the board back, a Project added) it says as a `PrsEvent`,
//! answered in `on_prs_event` once the view's own update has ended.

use ferrite_core::protocol::WirePrFilter;
use ferrite_core::ProjectKey;
use ferrite_prs::prs::{PrsEvent, PrsInputs, PrsView};
use ferrite_prs::pull_request::{PrStop, PullRequestView};
use ferrite_ui::source::Source;

use super::destination::{Destination, Shows};
use super::panel::PanelControl;
use super::*;

impl CockpitView {
    /// Show the PRs destination in the board's place: on `project` when
    /// told, else where it was left, else the shown Thread's Project, else
    /// the first one.
    pub(crate) fn open_prs(&mut self, project: Option<ProjectKey>, cx: &mut Context<Self>) {
        let focused = self.focused_thread(cx);
        let from_thread =
            focused.and_then(|thread| self.facts.get(thread).and_then(|facts| facts.project));
        let first = self.every_project(cx).first().map(|(key, _, _)| *key);
        let known = |project: &ProjectKey| self.cockpit.read(cx).project(*project).is_some();
        let chosen = project
            .or(self.prs.read(cx).project().filter(known))
            .or(from_thread)
            .or(first);
        self.destination = Some(Destination {
            shows: Shows::Prs,
            focus_at_open: self.cockpit.read(cx).roster().focused(),
        });
        self.prs.update(cx, |prs, cx| prs.show(chosen, cx));
        cx.notify();
    }

    /// A checkout's PR opens beside its conversation. Links to another
    /// repository use the browser rather than the checkout's forge reader.
    pub(crate) fn open_named_pr(
        &mut self,
        thread: ThreadKey,
        number: u64,
        url: String,
        cx: &mut Context<Self>,
    ) {
        let origin = |path: &std::path::Path| ferrite_pane::file_links::origin_base(path);
        let checkout = self
            .thread_path(thread, cx)
            .and_then(|path| origin(&path))
            .or_else(|| {
                self.facts
                    .pr(thread)
                    .and_then(|pr| pr.url.as_ref())
                    .and_then(|url| {
                        url.rsplit_once("/pull/")
                            .map(|(repository, _)| repository.to_owned())
                    })
            });
        let same_repository = checkout
            .as_ref()
            .is_some_and(|repository| {
                ["pull", "issues"].iter().any(|kind| url.starts_with(&format!("{repository}/{kind}/")))
            });
        if same_repository {
            self.upsert_surface(thread, surfaces::SurfaceId::PullRequest(number), cx);
        } else {
            self.browser_action(thread, ferrite_core::browser::Action::Open { url }, cx);
        }
    }

    /// The footer's Pull requests: the shown Thread's own PR in its Panel
    /// when its branch has one, else the destination.
    pub(crate) fn open_prs_from_foot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .focused_thread(cx)
            .filter(|thread| self.facts.pr(*thread).is_some())
        {
            Some(thread) => {
                self.open_pr_as(thread, ferrite_pane::link_target::Open::Primary, window, cx)
            }
            None => self.open_prs(None, cx),
        }
    }

    pub(super) fn on_prs_event(
        &mut self,
        _: Entity<PrsView>,
        event: &PrsEvent,
        cx: &mut Context<Self>,
    ) {
        match *event {
            PrsEvent::Read(project, filter) => self.read_prs_list(project, filter, cx),
            PrsEvent::Opened(target) => {
                self.read_pr_snapshot(target, true, cx);
                cx.notify();
            }
            PrsEvent::Leave => {
                self.leave_destination();
                cx.notify();
            }
            PrsEvent::AddProject => self.open_project_creator(cx),
        }
    }

    /// Ask the forge for `project`'s list as `filter` shows it, and hand
    /// the answer to the PRs to wait on.
    fn read_prs_list(
        &mut self,
        project: ProjectKey,
        filter: WirePrFilter,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .cockpit
            .read(cx)
            .project(project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        if !super::reads_forge(self.cockpit.read(cx), &path) {
            return;
        }
        let answer = self.cockpit.update(cx, |cockpit, _| {
            cockpit.list_pull_requests(project.machine(), &path, filter)
        });
        self.prs
            .update(cx, |prs, cx| prs.reading((project, filter), answer, cx));
    }

    /// The window's PRs, reading it through a `Source` over the root, with
    /// the window's PR reader as its child and Code's body, Changes' pane
    /// of the PR whose detail shows.
    pub(super) fn new_prs(
        changes: &Entity<ferrite_tools::changes::ChangesView>,
        reader: &Entity<PullRequestView>,
        cx: &mut Context<Self>,
    ) -> Entity<PrsView> {
        let root = cx.entity();
        let inputs = Source::of(&root, |root: &Self, _: &(), cx| Some(root.prs_inputs(cx)));
        let shown = Source::of(&root, |root: &Self, _: &(), cx| {
            let (target, tab) = root.prs.read(cx).open()?;
            root.pr_code(root.pr_shown(target, tab))
        });
        let code = cx.new(|_| ferrite_tools::changes::ChangesPane::new(changes.clone(), shown));
        let reader = reader.clone();
        cx.new(|cx| PrsView::new(inputs, reader, code.into(), cx))
    }

    /// Everything the PRs draw from.
    fn prs_inputs(&self, cx: &gpui::App) -> PrsInputs {
        let projects = self
            .every_project(cx)
            .into_iter()
            .map(|(key, _, title)| (key, SharedString::from(title)))
            .collect();
        let stops = self.panel.focus.handles(|control| match control {
            PanelControl::PrView(tab) => Some(PrStop::View(*tab)),
            _ => None,
        });
        PrsInputs { projects, stops }
    }

    /// The open PR's views, as the PRs declare them, enter the window's
    /// row of stops before the PRs draw them, so the arrows walk them as
    /// they walk a Thread's PR tab's.
    pub(super) fn register_prs_stops(&self, cx: &mut Context<Self>) {
        if !self.prs_shown() {
            return;
        }
        for (stop, semantics) in self.prs.read(cx).tab_stops() {
            if let PrStop::View(tab) = stop {
                self.panel.focus.register(PanelControl::PrView(tab), semantics, cx);
            }
        }
    }
}
