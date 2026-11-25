//! The window's machines (ADR 0026, ADR 0045): what their Threads' rows
//! say of them in the nav's one list, the notice of a machine the window
//! cannot reach, what a machine that went looks like, and Settings ›
//! Machines. The window holds every enabled machine at once; a press on
//! any machine's Thread opens it in place.

use super::*;
use ferrite_nav::notifications::Alert;
use ferrite_nav::view::MachineNotice;
use ferrite_client::cockpit::ServerState;
use ferrite_core::machine::Failure;
use gpui::App;

/// What a machine's failure reads as, `place` naming the machine: a Server
/// from before the ask is told to update, not shown its refusal.
pub(super) fn said(error: &std::io::Error, place: &str) -> String {
    match error.kind() {
        std::io::ErrorKind::Unsupported => format!("update ferrited on {place}"),
        _ => error.to_string(),
    }
}

/// What a lost machine still shows (ADR 0026): `element` dimmed under a
/// cover, `id`, that takes every press until the machine is back.
pub(super) fn held(element: Div, id: impl Into<SharedString>) -> Div {
    let id: SharedString = id.into();
    let selector = id.clone();
    element
        .relative()
        .opacity(ferrite_ui::theme::MACHINE_LOST_OPACITY)
        .child(
            div()
                .id(gpui::ElementId::Name(id))
                .debug_selector(move || selector.to_string())
                .absolute()
                .inset_0()
                .occlude(),
        )
}

impl CockpitView {
    /// Where an act with no Thread or Project of its own goes
    /// (`Cockpit::start_machine`).
    pub(super) fn start_machine(&self, cx: &App) -> Option<ferrite_core::MachineId> {
        self.cockpit.read(cx).start_machine()
    }

    /// Every Project of every machine the window holds, this computer's
    /// first, each with its title as a row says it: after its machine's
    /// name when that is not this computer.
    pub(super) fn every_project<'a>(
        &self,
        cx: &'a App,
    ) -> Vec<(ProjectKey, &'a ferrite_core::project::Project, String)> {
        let cockpit = self.cockpit.read(cx);
        let mut machines: Vec<ferrite_core::MachineId> = cockpit.machines().collect();
        machines.sort_by_key(|machine| cockpit.this_computer() != Some(*machine));
        machines
            .into_iter()
            .flat_map(|machine| {
                let registry = cockpit.registry(machine);
                let label = self.machine_label(machine, cx);
                registry.reading().projects().iter().map(move |project| {
                    let title = match &label {
                        Some(machine) => format!("{machine} \u{203a} {}", project.title),
                        None => project.title.clone(),
                    };
                    (registry.project_key(project.id), project, title)
                })
            })
            .collect()
    }

    /// Where `machine`'s Sessions run, in a word: its name, or this
    /// computer.
    pub(super) fn place_of(&self, machine: ferrite_core::MachineId, cx: &App) -> SharedString {
        match self.cockpit.read(cx).bridge(machine) {
            Some(bridge) => bridge.name().to_string().into(),
            None => "this computer".into(),
        }
    }

    /// `machine`'s name, when it is not this computer: what a row of it
    /// says beside its own words.
    pub(super) fn machine_label(&self, machine: ferrite_core::MachineId, cx: &App) -> Option<String> {
        if self.cockpit.read(cx).this_computer() == Some(machine) {
            return None;
        }
        self.machines.as_ref()?.name(machine)
    }

    /// Whether `machine` is another machine, or none reached: what it
    /// names on disk is never this computer's, so the window stands in no
    /// local files or defaults for its own (ADR 0026).
    pub(super) fn elsewhere(&self, machine: ferrite_core::MachineId, cx: &App) -> bool {
        !self.cockpit.read(cx).local(machine)
    }

    /// What a row of `thread` says of its machine, when that is not this
    /// computer: its name, and whether the window has lost it.
    pub(super) fn elsewhere_row(&self, thread: ThreadKey, cx: &App) -> Option<nav::Elsewhere> {
        let machine = thread.machine();
        Some(nav::Elsewhere {
            machine: self.machine_label(machine, cx)?.into(),
            lost: self.machine_lost(machine, cx),
        })
    }

    /// Every machine the window holds but cannot reach, other than this
    /// computer, in the order Settings lists them: what the notice of
    /// each says.
    pub(super) fn machine_notices(&self, cx: &App) -> Vec<MachineNotice> {
        let cockpit = self.cockpit.read(cx);
        let mut machines: Vec<ferrite_core::MachineId> = cockpit.machines().collect();
        let listed = |machine| {
            self.machines
                .as_ref()
                .and_then(|machines| machines.listed_as(machine))
        };
        machines.sort_by_key(|machine| listed(*machine));
        machines
            .into_iter()
            .filter_map(|machine| {
                let name = self.machine_label(machine, cx)?;
                let Some(ServerState::Detached { attention, .. }) = cockpit.server(machine)
                else {
                    return None;
                };
                let (words, sign_in, reconnecting) = match attention {
                    Some(failure) if failure.needs_sign_in() => {
                        (format!("Sign in to {name}"), true, false)
                    }
                    Some(_) => (format!("{name} is offline"), false, false),
                    None => (format!("{name} is reconnecting"), false, true),
                };
                Some(MachineNotice {
                    machine,
                    words: words.into(),
                    sign_in,
                    reconnecting,
                })
            })
            .collect()
    }

    /// Whether `machine` is another machine whose Server is not there:
    /// its Panes stay, dimmed, and take no input until it is back.
    pub(super) fn machine_lost(&self, machine: ferrite_core::MachineId, cx: &App) -> bool {
        self.elsewhere(machine, cx)
            && matches!(
                self.cockpit.read(cx).server(machine),
                Some(ServerState::Detached { .. })
            )
    }

    /// Whether `machine`'s Server answers (or it runs an Engine of its
    /// own): what the window mirrors of it is its Server's.
    pub(super) fn attached(&self, machine: ferrite_core::MachineId, cx: &App) -> bool {
        self.cockpit
            .read(cx)
            .server(machine)
            .is_none_or(|server| matches!(server, ServerState::Attached { .. }))
    }

    /// Settings, on Connections.
    pub(super) fn open_connections(&mut self, cx: &mut Context<Self>) {
        self.show_settings(cx).update(cx, |settings, cx| {
            settings.show(settings::PageKey::Connections, cx);
        });
    }

    /// Sign in to `machine`: over ssh in a terminal; a paired link whose
    /// session ended, by pairing it again in Connections.
    pub(super) fn sign_in(
        &mut self,
        machine: ferrite_core::MachineId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.cockpit.read(cx).server(machine),
            Some(ServerState::Detached { attention: Some(Failure::Revoked), .. })
        ) {
            self.pair_again(machine, window, cx);
            return;
        }
        let Some(bridge) = self.cockpit.read(cx).bridge(machine) else {
            return;
        };
        if let Err(error) = bridge.sign_in() {
            self.nav_notice.problem(format!("cannot sign in: {error}"));
            cx.notify();
        }
    }

    /// Connections, its editor open on `machine` for a fresh pairing code.
    fn pair_again(
        &mut self,
        machine: ferrite_core::MachineId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let saved = self.machines.as_ref().and_then(|machines| {
            machines.machine(machines.listed_as(machine)?).cloned()
        });
        let settings = self.show_settings(cx);
        settings.update(cx, |settings, cx| settings.show(settings::PageKey::Connections, cx));
        if let Some(saved) = saved {
            let connections = settings.read(cx).connections().clone();
            let dir = self.prefs.dir.clone();
            connections.update(cx, |connections, cx| connections.edit(Some(saved), &dir, window, cx));
        }
    }

    /// Follow Settings' machines: hold each one turned on, let go of each
    /// one turned off or removed, and forget what the Inbox heard of it.
    pub(super) fn machines_changed(&mut self, cx: &mut Context<Self>) {
        let saved = self.prefs.settings.machines.clone();
        let local_enabled = self.prefs.settings.local_environment_enabled;
        if self.machines.is_none() {
            return;
        }
        // What the Files and the new Panes of a machine about to go hold
        // is saved first.
        self.sync_working_sets(cx);
        self.save_drafts(cx);
        let before: Vec<ferrite_core::MachineId> = self.cockpit.read(cx).machines().collect();
        let machines = self.machines.as_mut().expect("checked");
        self.cockpit.update(cx, |cockpit, _| {
            machines.set_local_enabled(local_enabled, cockpit);
            machines.settings_changed(&saved, cockpit);
        });
        self.machines_let_go(&before, cx);
    }

    /// The machines held in `before` that the window let go of since, and
    /// those it took: the Panels of each are put away or held, a shell of
    /// each one gone ends, what the window keeps by the Threads of each one
    /// held again follows them to their new keys, the Inbox forgets each
    /// one gone, and the Panes and the nav redraw.
    pub(super) fn machines_let_go(
        &mut self,
        before: &[ferrite_core::MachineId],
        cx: &mut Context<Self>,
    ) {
        let now: Vec<ferrite_core::MachineId> = self.cockpit.read(cx).machines().collect();
        let gone: Vec<ferrite_core::MachineId> = before
            .iter()
            .copied()
            .filter(|machine| !now.contains(machine))
            .collect();
        // A Machine removed while off holds nothing, yet its copies go
        // with it.
        self.stop_copies_to(&gone, cx);
        if gone.is_empty() && before.len() == now.len() {
            return;
        }
        for machine in &gone {
            self.terminals.update(cx, |terminals, cx| terminals.let_go(*machine, cx));
        }
        self.forget_projects_of(&gone, cx);
        let reheld = self.panel.hold(self.cockpit.read(cx));
        for rehold in &reheld {
            self.terminals.update(cx, |terminals, cx| terminals.held_again(*rehold, cx));
            self.side_chat.update(cx, |chat, cx| chat.held_again(*rehold, cx));
        }
        if !reheld.is_empty() {
            self.reopen_put_away_drafts(cx);
        }
        self.toasts.update(cx, |toasts, cx| toasts.forget(&gone, cx));
        self.sync_panes(cx);
        cx.notify();
    }

    /// The window let go of `gone`: each copy to one stops at once, saying
    /// whether its Machine was turned off, for its Resume, or removed, for
    /// good; a copy to a Machine removed while off says so too.
    fn stop_copies_to(&mut self, gone: &[ferrite_core::MachineId], cx: &mut Context<Self>) {
        let saved = &self.prefs.settings.machines;
        let removed = |key: &ferrite_core::MachineKey| {
            key.0
                .as_ref()
                .is_some_and(|target| !saved.iter().any(|machine| machine.target == *target))
        };
        let stopped = self.copies.update(cx, |copies, cx| copies.machines_went(gone, removed, cx));
        for key in &stopped {
            self.tell_copy(key, cx);
        }
    }

    /// The window let go of `gone`: a Project of theirs names nothing it
    /// holds, so no guess of where a new Thread starts (the nav's filter,
    /// the last Project worked in) names one any longer, and each new Pane
    /// aimed at one goes with its machine. A new Thread starts on a machine
    /// the window holds: before any Project there, the new-Project card on
    /// this computer (`start_machine`).
    fn forget_projects_of(&mut self, gone: &[ferrite_core::MachineId], cx: &mut Context<Self>) {
        let gone_with = |project: Option<ProjectKey>| project.is_some_and(|project| gone.contains(&project.machine()));
        if gone_with(self.nav_filter) {
            self.nav_filter = None;
        }
        if gone_with(self.launch_project) {
            self.launch_project = super::startup_project(self.cockpit.read(cx));
        }
        self.put_away_drafts(gone, cx);
    }

    /// `thread`'s title as the Inbox and a toast say it: after its
    /// machine's name when that is not this computer.
    pub(super) fn row_title(&self, thread: ThreadKey, cx: &App) -> SharedString {
        let title = self.facts.name(thread);
        match self.machine_label(thread.machine(), cx) {
            Some(machine) => ferrite_nav::notifications::machine_title(&machine, &title),
            None => title,
        }
    }
}

impl CockpitView {
    /// Ask `project`'s machine's Server what its repo holds for the
    /// workspace popover (`list_repo`): git there, never here (ADR 0026).
    /// An older Server cannot say, and the popover offers the fresh starts
    /// alone.
    pub(super) fn ask_machine_repo(&mut self, project: ProjectKey, cx: &mut App) {
        let reply = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.list_repo(project).ok());
        self.checkouts.listen_machine(project, reply);
    }

    /// Take the machine's answer to `ask_machine_repo`, once it lands: the
    /// popover's rows, and the branch its checkout is on, which no git
    /// here could name — `main` when it cannot say, as for a local Project.
    /// With nothing on its way, ask about a draft's Project whose branch
    /// the chip has no answer for yet.
    pub(super) fn poll_machine_repo(&mut self, cx: &mut Context<Self>) {
        match self.checkouts.machine_listed() {
            None => self.ask_drafted_checkout(cx),
            Some(false) => {}
            Some(true) => {
                self.sync_workspace_rows(cx);
                cx.notify();
            }
        }
    }

    /// A draft's workspace chip names its checkout's branch, which on a
    /// machine only its Server can read: ask about the first drafted
    /// Project, on a machine there, it has no answer for.
    fn ask_drafted_checkout(&mut self, cx: &mut App) {
        let unknown = self
            .panes
            .iter()
            .filter_map(|pane| pane.draft())
            .map(|draft| draft.binding.project())
            .find(|project| {
                let machine = project.machine();
                self.elsewhere(machine, cx)
                    && !self.machine_lost(machine, cx)
                    && self.checkouts.branch(*project).is_none()
            });
        if let Some(project) = unknown {
            self.ask_machine_repo(project, cx);
        }
    }
}

/// Whether an alert's record still wants the operator, on its machine.
/// `None` while that machine has not answered (`NeedsYou::ring`).
pub(super) fn wanted(alert: &Alert, view: &CockpitView, cx: &App) -> Option<bool> {
    let machine = alert.thread.machine();
    if !view.attached(machine, cx) {
        return None;
    }
    let notifications = view.cockpit.read(cx).notifications(machine);
    Some(alert.target.wants(notifications.reading()))
}

/// What a folder of `machine` holds, asked of its Server through `model`:
/// a folder browser's `List` (ADR 0026).
pub(super) fn folders_on(model: Entity<Cockpit>, machine: MachineId) -> ferrite_settings::folder_browser::List {
    std::rc::Rc::new(move |path, cx| model.update(cx, |cockpit, _| cockpit.list_dir(machine, path)))
}
