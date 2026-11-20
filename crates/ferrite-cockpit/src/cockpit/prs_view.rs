//! The PRs destination as a view of its own (ADR 0041): one Project's pull
//! requests, listed and read whole, in the board's place (as the Factory
//! board is), whatever is checked out. The root holds it for the window's
//! life, so what it holds outlives it being hidden. What it shows of the
//! window it reads through `prs_inputs`; what it asks of the window (a
//! list read, a PR read, the board back, a Project added) it says as a
//! `PrsEvent`, which the Cockpit answers once the view's own update has
//! ended.
//!
//! The shape it holds:
//!
//! - **Scope:** a Project (the repository its root belongs to) and a state
//!   `filter`. Each (Project, filter) keeps the last list read beside the
//!   state of its newest read (`ListStatus`), so a refresh or a failure
//!   never takes the rows away, and an answer lands only if it carries the
//!   request still out for its list.
//! - **Selection:** the PR whose row is marked, and whether its detail
//!   shows. Leaving the detail returns to the list as it was left: its
//!   scroll, its mark, its filter and its scope.
//! - **Detail:** the PR as a Thread's PR tab shows it (`pr_content`), read
//!   by number in the Project's repository (`PrTarget::project`), with no
//!   actions: they belong to a Thread's own PR. The PR reader
//!   (`PullRequestView`) draws that body, and the Cockpit hands it over
//!   with the inputs.
//! - **Freshness:** the view, a scope or filter chosen, and a detail opened
//!   each read again, over the content the last read left.
//! - **Keys:** the view owns its keys under the `PullRequests` context:
//!   ↑ ↓ move the marked row, ⏎ opens it, esc goes back to the list, then
//!   closes the view.

use std::collections::HashMap;

use gpui::{actions, WeakEntity};

use super::panel::pr_read::{PrFailure, PrTarget};
use super::panel::pull_request::PrTone;
use super::*;
use super::destination::{Destination, Shows};
use crate::cockpit::surfaces::PrTab;
use ferrite_core::protocol::{ReviewPullRequests, WirePrFilter, WirePrList, WirePrRow, WirePrUnread};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_ui::icons::{self, icon};

actions!(pull_requests, [Next, Previous, Open, Back]);

/// The key context the destination holds focus under.
const KEY_CONTEXT: &str = "PullRequests";

#[derive(Clone, Debug, PartialEq, Eq)]
enum ListStatus {
    Reading { request: u64 },
    Settled,
    Failed(PrFailure),
}

struct ListRead {
    list: Option<WirePrList>,
    status: ListStatus,
}

pub(crate) struct PrsView {
    cockpit: WeakEntity<CockpitView>,
    pub(super) project: Option<ProjectId>,
    pub(super) filter: WirePrFilter,
    lists: HashMap<(ProjectId, WirePrFilter), ListRead>,
    pub(super) selected: Option<u64>,
    detail: bool,
    tab: PrTab,
    pub(super) scroll: ScrollHandle,
    issued: u64,
    pub(super) focus: FocusHandle,
}

/// What the PRs draw from, as the Cockpit has it this frame.
pub(crate) struct PrsInputs {
    /// The Projects to scope to, in the registry's order, by title.
    projects: Vec<(ProjectId, SharedString)>,
    /// The open PR, when its detail shows.
    detail: Option<PrsDetail>,
}

/// The open PR, as the Panel's reader has it.
pub(crate) struct PrsDetail {
    number: u64,
    url: Option<String>,
    /// The state the last read gave it; the list's row stands in before.
    tone: Option<PrTone>,
    /// The PR tab's body, drawn by the reader (`PullRequestView`).
    content: Div,
}

/// What the PRs ask of the window.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PrsEvent {
    /// Read this Project's list in this state.
    Read(ProjectId, WirePrFilter),
    /// A PR's detail opened: read it again over what the last read left.
    Opened(PrTarget),
    /// Esc on the list: the board comes back.
    Leave,
    AddProject,
}

impl EventEmitter<PrsEvent> for PrsView {}

const FILTERS: [(WirePrFilter, &str); 3] = [
    (WirePrFilter::Open, "Open"),
    (WirePrFilter::Closed, "Closed"),
    (WirePrFilter::All, "All"),
];

/// The repository's PR index, from the address of one of its PRs.
fn index_of(url: &str) -> Option<String> {
    let (repo, _) = url.rsplit_once("/pull/")?;
    Some(format!("{repo}/pulls"))
}

impl CockpitView {
    /// Show the PRs destination in the board's place: on `project` when
    /// told, else where it was left, else the shown Thread's Project, else
    /// the first one.
    pub(crate) fn open_prs(&mut self, project: Option<ProjectId>, cx: &mut Context<Self>) {
        let focused = self.focused_thread();
        let from_thread =
            focused.and_then(|thread| self.facts.get(thread).and_then(|facts| facts.project));
        let first = self
            .cockpit
            .registry()
            .projects()
            .first()
            .map(|project| project.id);
        let known = |project: &ProjectId| self.cockpit.registry().project(*project).is_some();
        let chosen = project
            .or(self.prs.read(cx).project.filter(known))
            .or(from_thread)
            .or(first);
        self.destination = Some(Destination {
            shows: Shows::Prs,
            focus_at_open: self.cockpit.roster().focused(),
        });
        self.prs.update(cx, |prs, cx| prs.show(chosen, cx));
        cx.notify();
    }

    /// A checkout's PR opens beside its conversation. Links to another
    /// repository use the browser rather than the checkout's forge reader.
    pub(crate) fn open_named_pr(
        &mut self,
        thread: ThreadId,
        number: u64,
        url: String,
        cx: &mut Context<Self>,
    ) {
        let origin = |path: &std::path::Path| ferrite_pane::file_links::origin_base(path);
        let checkout = self
            .thread_path(thread)
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
            .focused_thread()
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
        project: ProjectId,
        filter: WirePrFilter,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        let answer = self.cockpit.list_pull_requests(&path, filter);
        self.prs
            .update(cx, |prs, cx| prs.reading((project, filter), answer, cx));
    }

    /// Everything the PRs draw from. `open` is the PR whose detail shows,
    /// and the view of it chosen.
    fn prs_inputs(&self, open: Option<(PrTarget, PrTab)>, cx: &mut Context<Self>) -> PrsInputs {
        let projects = self
            .cockpit
            .registry()
            .projects()
            .iter()
            .map(|project| (project.id, SharedString::from(project.title.clone())))
            .collect();
        let detail = open.map(|(target, tab)| {
            let shown = self.pr_shown(target, tab, cx);
            self.pull_request.update(cx, |reader, cx| {
                let snapshot = reader.snapshot(target);
                PrsDetail {
                    number: target.number,
                    url: snapshot.map(|pr| pr.url.clone()),
                    tone: snapshot.map(PrTone::of_snapshot),
                    content: reader.pr_content(shown, cx),
                }
            })
        });
        PrsInputs { projects, detail }
    }
}

impl PrsView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            project: None,
            filter: WirePrFilter::Open,
            lists: HashMap::new(),
            selected: None,
            detail: false,
            tab: PrTab::Summary,
            scroll: ScrollHandle::new(),
            issued: 0,
            focus: cx.focus_handle(),
        }
    }

    fn rows(&self) -> &[WirePrRow] {
        self.project
            .and_then(|project| self.lists.get(&(project, self.filter)))
            .and_then(|read| read.list.as_ref())
            .map_or(&[], |list| list.rows.as_slice())
    }

    pub(super) fn set_tab(&mut self, tab: PrTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    /// Shown on `project`, its list read again.
    fn show(&mut self, project: Option<ProjectId>, cx: &mut Context<Self>) {
        self.project = project;
        if let Some(project) = project {
            self.read_list(project, cx);
        }
    }

    /// Ask for `project`'s list as the filter shows it, over the rows the
    /// last read left: they stay until the new ones land.
    fn read_list(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        cx.emit(PrsEvent::Read(project, self.filter));
    }

    /// The forge's answer to a list read, on its way.
    fn reading(
        &mut self,
        key: (ProjectId, WirePrFilter),
        answer: std::io::Result<std::sync::mpsc::Receiver<std::io::Result<ReviewPullRequests>>>,
        cx: &mut Context<Self>,
    ) {
        self.issued += 1;
        let request = self.issued;
        let status = ListStatus::Reading { request };
        match self.lists.get_mut(&key) {
            Some(read) => read.status = status,
            None => {
                self.lists.insert(key, ListRead { list: None, status });
            }
        }
        cx.spawn(async move |this, cx| {
            let read = crate::cockpit::beside::answered(answer, cx).await;
            this.update(cx, |view, cx| view.landed(key, request, read, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn landed(
        &mut self,
        key: (ProjectId, WirePrFilter),
        request: u64,
        read: std::io::Result<ReviewPullRequests>,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = self.lists.get_mut(&key) else {
            return;
        };
        if slot.status != (ListStatus::Reading { request }) {
            return;
        }
        match read {
            Ok(read) => match read.list {
                Ok(list) => {
                    slot.list = Some(list);
                    slot.status = ListStatus::Settled;
                }
                Err(note) => {
                    let kind = read.unread.unwrap_or(WirePrUnread::Failed);
                    slot.status = ListStatus::Failed(PrFailure { kind, note });
                }
            },
            Err(error) => {
                slot.status = ListStatus::Failed(PrFailure {
                    kind: WirePrUnread::Failed,
                    note: error.to_string(),
                })
            }
        }
        cx.notify();
    }

    fn choose_project(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if self.project != Some(project) {
            self.project = Some(project);
            self.selected = None;
            self.detail = false;
        }
        self.read_list(project, cx);
        cx.notify();
    }

    fn choose_filter(&mut self, filter: WirePrFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        if let Some(project) = self.project {
            self.read_list(project, cx);
        }
        cx.notify();
    }

    /// A row pressed: its PR's detail shows, read again over what the last
    /// read left.
    pub(super) fn open_detail(&mut self, number: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project else {
            return;
        };
        self.selected = Some(number);
        self.detail = true;
        cx.emit(PrsEvent::Opened(PrTarget::project(project, number)));
        cx.notify();
    }

    fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.detail = false;
        cx.notify();
    }

    /// The marked row moves `delta` rows, holding at either end, and
    /// scrolls into view. With none marked it starts at the first.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.detail {
            return;
        }
        let rows = self.rows();
        let Some(last) = rows.len().checked_sub(1) else {
            return;
        };
        let at = rows.iter().position(|row| Some(row.number) == self.selected);
        let next = match at {
            Some(at) => at.saturating_add_signed(delta).min(last),
            None => 0,
        };
        self.selected = Some(rows[next].number);
        self.scroll.scroll_to_item(next);
        cx.notify();
    }

    /// The destination's crumbs: its Project, `Pull requests`, then the PR
    /// whose detail shows.
    pub(super) fn head(&self, project: Option<SharedString>) -> Vec<AnyElement> {
        use ferrite_ui::theme as t;
        let crumb = |text: SharedString, current: bool| {
            div()
                .flex_shrink_0()
                .text_size(px(t::FS_TITLE))
                .when(current, |crumb| crumb.font_weight(t::W_LABEL))
                .text_color(rgb(if current {
                    t::INK_STRONG
                } else {
                    t::INK_SUBTLE
                }))
                .child(text)
        };
        let detail = self.detail.then_some(self.selected).flatten();
        let separator = || {
            div()
                .flex_shrink_0()
                .text_color(rgb(t::INK_SUBTLE))
                .child("/")
                .into_any_element()
        };
        let mut crumbs = Vec::new();
        if let Some(project) = project {
            crumbs.push(crumb(project, false).into_any_element());
            crumbs.push(separator());
        }
        crumbs.push(
            crumb("Pull requests".into(), detail.is_none())
                .debug_selector(|| "view-head-prs".into())
                .into_any_element(),
        );
        if let Some(number) = detail {
            crumbs.push(separator());
            crumbs.push(crumb(format!("#{number}").into(), true).into_any_element());
        }
        vec![div()
            .debug_selector(|| "view-breadcrumb".into())
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(t::HEAD_CRUMB_GAP))
            .children(crumbs)
            .into_any_element()]
    }

    /// The destination: its scope and filter over the list, or the open PR.
    fn board(&self, inputs: PrsInputs, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let body = match (self.project, inputs.detail) {
            (None, _) => self.no_project(cx),
            (Some(_), Some(detail)) => self.detail(detail, cx),
            (Some(project), None) => self.list(project, &inputs.projects, cx),
        };
        div()
            .debug_selector(|| "prs-board".into())
            .track_focus(&self.focus)
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(|view, _: &Next, _, cx| view.step(1, cx)))
            .on_action(cx.listener(|view, _: &Previous, _, cx| view.step(-1, cx)))
            .on_action(cx.listener(|view, _: &Open, _, cx| {
                if let Some(number) = view.selected.filter(|_| !view.detail) {
                    view.open_detail(number, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &Back, _, cx| {
                if view.detail {
                    view.close_detail(cx);
                } else {
                    cx.emit(PrsEvent::Leave);
                }
            }))
            .flex_1()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .rounded(px(R_CARD))
            .bg(paint::CARD)
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .child(body)
    }

    fn no_project(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        div()
            .debug_selector(|| "prs-no-project".into())
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .child(icon(icons::GIT_PULL_REQUEST, SPACE_6, INK_SUBTLE))
            .child(
                div()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("No project to read pull requests from"),
            )
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Add a project, and its repository's pull requests list here."),
            )
            .child(
                ferrite_ui::components::secondary_button(
                    "prs-add-project",
                    Some(icons::FOLDER_PLUS),
                    "Add project",
                    cx,
                )
                .debug_selector(|| "prs-add-project".into())
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(PrsEvent::AddProject);
                })),
            )
    }

    /// A pressable segment of a group: `label`, pressed while `on`.
    fn segment(
        &self,
        id: SharedString,
        label: SharedString,
        on: bool,
        press: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let selector = id.to_string();
        let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(SURFACE_TAB_H))
            .px(px(PR_SEGMENT_PAD_X))
            .rounded(px(R_CONTROL))
            .text_color(rgb(if on { INK_STRONG } else { INK_SUBTLE }))
            .map(|segment| {
                if on {
                    segment.bg(rgba(PR_SEGMENT_PRESSED))
                } else {
                    segment.hover_raised(key)
                }
            })
            .child(label)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                press(view, cx);
            }))
            .into_any_element()
    }

    /// The scope and the state filter, over the list.
    fn scope_bar(
        &self,
        project: ProjectId,
        projects: &[(ProjectId, SharedString)],
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let group = |segments: Vec<AnyElement>, selector: &'static str| {
            div()
                .debug_selector(move || selector.into())
                .flex()
                .flex_wrap()
                .gap(px(SPACE_0_5))
                .p(px(SPACE_0_5))
                .rounded(px(PR_GROUP_R))
                .bg(rgba(PR_GROUP_GROUND))
                .children(segments)
        };
        let scope = projects
            .iter()
            .map(|(id, title)| {
                let id = *id;
                self.segment(
                    format!("prs-project-{}", id.get()).into(),
                    title.clone(),
                    id == project,
                    move |view, cx| view.choose_project(id, cx),
                    cx,
                )
            })
            .collect();
        let filters = FILTERS
            .into_iter()
            .map(|(filter, label)| {
                self.segment(
                    format!("prs-filter-{label}").into(),
                    label.into(),
                    filter == self.filter,
                    move |view, cx| view.choose_filter(filter, cx),
                    cx,
                )
            })
            .collect();
        div()
            .debug_selector(|| "prs-scope".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_3))
            .h_auto()
            .min_h(px(CONTROL_H + SPACE_4))
            .px(px(SPACE_4))
            .py(px(SPACE_2))
            .child(div().flex_1().min_w_0().child(group(scope, "prs-projects")))
            .child(group(filters, "prs-filters"))
    }

    fn list(
        &self,
        project: ProjectId,
        projects: &[(ProjectId, SharedString)],
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let read = self.lists.get(&(project, self.filter));
        let list = read.and_then(|read| read.list.as_ref());
        let failure = read.and_then(|read| match &read.status {
            ListStatus::Failed(failure) => Some(failure),
            _ => None,
        });
        let reading = read.is_none_or(|read| matches!(read.status, ListStatus::Reading { .. }));
        let rows: Vec<AnyElement> = list
            .map(|list| list.rows.iter().map(|row| self.row(row, cx)).collect())
            .unwrap_or_default();
        let note = |id: &'static str, text: &'static str| {
            div()
                .debug_selector(move || id.into())
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(FS_LABEL))
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let content = match (list, failure) {
            (None, Some(failure)) => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .debug_selector(|| "prs-failed".into())
                        .text_color(rgb(RED))
                        .child(SharedString::from(format!(
                            "{}. {}",
                            failure.list_headline(),
                            failure.note
                        ))),
                )
                .child(self.retry(project, cx))
                .into_any_element(),
            (None, None) if reading => note("prs-reading", "Reading\u{2026}").into_any_element(),
            (None, None) => note("prs-none", "No pull requests").into_any_element(),
            (Some(list), _) if list.rows.is_empty() => note(
                "prs-empty",
                match self.filter {
                    WirePrFilter::Open => "No open pull requests",
                    WirePrFilter::Closed => "No closed pull requests",
                    WirePrFilter::All => "No pull requests",
                },
            )
            .into_any_element(),
            (Some(list), _) => {
                let more = list.more.then(|| {
                    let index = list.rows.first().and_then(|row| index_of(&row.url));
                    pr_more_link(
                        "prs-more",
                        format!(
                            "Showing the {} most recently updated. See all on the forge",
                            list.rows.len()
                        ),
                        index,
                        cx,
                    )
                });
                div()
                    .id("prs-rows")
                    .debug_selector(|| "prs-rows".into())
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(SPACE_2))
                    .pb(px(SPACE_3))
                    .children(rows)
                    .children(more)
                    .into_any_element()
            }
        };
        let stale = list.is_some().then(|| failure).flatten().map(|failure| {
            div()
                .debug_selector(|| "prs-stale".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_4))
                .py(px(SPACE_1))
                .text_color(rgb(RED))
                .child(SharedString::from(format!(
                    "{}. Showing the last read. {}",
                    failure.list_headline(),
                    failure.note
                )))
                .child(self.retry(project, cx))
        });
        let refreshing = (list.is_some() && reading).then(|| {
            div()
                .debug_selector(|| "prs-refreshing".into())
                .px(px(SPACE_4))
                .py(px(SPACE_1))
                .text_color(rgb(INK_SUBTLE))
                .child("Refreshing\u{2026}")
        });
        div()
            .debug_selector(|| "prs-list".into())
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(self.scope_bar(project, projects, cx))
            .children(stale)
            .children(refreshing)
            .child(content)
    }

    fn retry(&self, project: ProjectId, cx: &mut Context<Self>) -> AnyElement {
        ferrite_ui::components::secondary_button("prs-retry", None, "Retry", cx)
            .debug_selector(|| "prs-retry".into())
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.read_list(project, cx);
            }))
            .into_any_element()
    }

    /// A PR of the list: its state's glyph, its number and title, then who
    /// opened it, between which branches, and when it was last touched.
    fn row(&self, row: &WirePrRow, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let number = row.number;
        let tone = panel::pull_request::PrTone::of_wire(row.state, row.draft);
        let marked = self.selected == Some(number);
        let id = SharedString::from(format!("prs-row-{number}"));
        let selector = id.to_string();
        let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
        let age = panel::pr_read::stamp_age(&row.updated_at);
        let meta = format!(
            "{} \u{b7} {} \u{2192} {}{}{age}",
            row.author,
            row.head,
            row.base,
            if age.is_empty() { "" } else { " \u{b7} " },
        );
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_start()
            .gap(px(SPACE_3))
            .px(px(SPACE_2))
            .py(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .map(|line| {
                if marked {
                    line.hover_carried(key)
                } else {
                    line.hover_raised(key)
                }
            })
            .child(div().flex_shrink_0().mt(px(SPACE_0_5)).child(icon(
                tone.glyph(),
                ICON_CHEVRON,
                tone.ink(),
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(W_LABEL)
                            .text_color(rgb(INK_STRONG))
                            .child(SharedString::from(row.title.clone())),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(format!("#{number} \u{b7} {meta}"))),
                    ),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_detail(number, cx);
            }))
            .into_any_element()
    }

    /// The open PR: a way back to the list, its number on the forge, then
    /// the views a Thread's PR tab has but Code.
    fn detail(&self, detail: PrsDetail, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let PrsDetail {
            number,
            url,
            tone,
            content,
        } = detail;
        let tone = tone.or_else(|| {
            let row = self.rows().iter().find(|row| row.number == number)?;
            Some(panel::pull_request::PrTone::of_wire(row.state, row.draft))
        });
        let back = div()
            .id("prs-back")
            .debug_selector(|| "prs-back".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(SURFACE_TAB_H))
            .px(px(PR_SEGMENT_PAD_X))
            .rounded(px(R_CONTROL))
            .text_color(rgb(INK_SUBTLE))
            .hover_raised(ferrite_ui::pointer::hover_key(&gpui::ElementId::from(
                "prs-back",
            )))
            .child(icon(icons::BACK, ICON_CHEVRON, INK_SUBTLE))
            .child("Pull requests")
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_detail(cx);
            }));
        let open = url.map(|url| {
            div()
                .id("prs-open-external")
                .debug_selector(|| "prs-open-external".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1))
                .cursor_pointer()
                .text_color(rgb(INK_SUBTLE))
                .hover(|link| link.underline())
                .child("Open on the forge")
                .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)))
        });
        div()
            .debug_selector(|| "prs-detail".into())
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .text_size(px(FS_BADGE))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(CONTROL_H + SPACE_4))
                    .px(px(SPACE_2))
                    .border_b(px(HAIRLINE_W))
                    .border_color(panel::pull_request::soft_line())
                    .child(back)
                    .child(div().flex_1())
                    .children(open)
                    .children(tone.map(panel::pull_request::pr_pill)),
            )
            .child(content)
    }
}

impl PrsView {
    /// The PR whose detail shows, and the view of it chosen.
    pub(super) fn open(&self) -> Option<(PrTarget, PrTab)> {
        self.project
            .zip(self.detail.then_some(self.selected).flatten())
            .map(|(project, number)| (PrTarget::project(project, number), self.tab))
    }
}

impl Render for PrsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open();
        match self
            .cockpit
            .update(cx, |cockpit, cx| cockpit.prs_inputs(open, cx))
        {
            Ok(inputs) => self.board(inputs, cx),
            Err(_) => div(),
        }
    }
}

/// The way past a list's cap: the repository's PR index, when one of the
/// rows names it.
fn pr_more_link(
    id: &'static str,
    said: String,
    index: Option<String>,
    cx: &mut Context<PrsView>,
) -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(SPACE_1))
        .px(px(SPACE_2))
        .py(px(SPACE_2))
        .text_color(rgb(INK_SUBTLE))
        .when_some(index, |link, index| {
            link.cursor_pointer()
                .hover(|link| link.underline())
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&index)))
        })
        .child(SharedString::from(said))
        .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
        .into_any_element()
}
