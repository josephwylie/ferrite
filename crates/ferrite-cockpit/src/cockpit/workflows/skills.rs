//! The skills a Step's Provider has installed in the Project, asked of the
//! Project's machine once per Provider, for the picker and the check.
//!
//! Only the root and its pump write them, so they are a plain struct
//! beside the editor's view (ADR 0041), shaped like the import picker's
//! listing: the root asks (`listen`) and its pump takes each answer
//! (`listed`).

use ferrite_client::cockpit::Ask;
use ferrite_core::SessionCommand;

use super::*;

/// What one Provider's discovery says.
enum Found {
    /// Asked of the machine `place` names, not yet answered.
    Looking(Ask<Vec<SessionCommand>>, SharedString),
    /// The skills among the commands it listed.
    Listed(Vec<SessionCommand>),
    Failed(String),
}

/// The skills each Provider asked has installed in the Project.
#[derive(Default)]
pub(crate) struct Skills(Vec<(Provider, Found)>);

impl Skills {
    /// Whether `provider` was asked already.
    fn asked(&self, provider: Provider) -> bool {
        self.0.iter().any(|(each, _)| *each == provider)
    }

    /// Wait for `provider`'s discovery on the machine `place` names.
    fn listen(&mut self, provider: Provider, reply: Ask<Vec<SessionCommand>>, place: SharedString) {
        self.0.push((provider, Found::Looking(reply, place)));
    }

    /// Take each discovery that answered; whether any did.
    fn listed(&mut self) -> bool {
        let mut heard = false;
        for (_, found) in &mut self.0 {
            let Found::Looking(reply, place) = found else {
                continue;
            };
            let Some(answer) = reply.answer() else {
                continue;
            };
            let place = place.clone();
            heard = true;
            *found = match answer {
                Ok(commands) => Found::Listed(
                    commands
                        .into_iter()
                        .filter(SessionCommand::is_skill)
                        .collect(),
                ),
                Err(error) => Found::Failed(crate::cockpit::machines::said(&error, &place)),
            };
        }
        heard
    }

    /// What `provider`'s discovery says, once asked.
    fn found(&self, provider: Provider) -> Option<&Found> {
        self.0
            .iter()
            .find(|(each, _)| *each == provider)
            .map(|(_, found)| found)
    }

    /// What the inspector says of `provider`'s skills.
    pub(super) fn note(&self, provider: Provider) -> String {
        match self.found(provider) {
            Some(Found::Looking(..)) => "Looking for installed skills\u{2026}".into(),
            Some(Found::Failed(error)) => format!("Could not list skills: {error}"),
            Some(Found::Listed(skills)) => format!("{} installed for {provider}", skills.len()),
            None => String::new(),
        }
    }

    /// The skills found for `provider`, once its discovery answered.
    pub(super) fn installed(&self, provider: Provider) -> Option<&[SessionCommand]> {
        match self.found(provider)? {
            Found::Listed(skills) => Some(skills),
            _ => None,
        }
    }
}

impl CockpitView {
    /// Ask the Provider which skills it has installed in the Project, once.
    pub(super) fn discover_skills(&mut self, provider: Provider, cx: &mut gpui::App) {
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return;
        };
        if view.read(cx).skills.asked(provider) {
            return;
        }
        let Some(cwd) = self
            .cockpit
            .read(cx)
            .project(project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        let reply = self.cockpit.update(cx, |cockpit, _| {
            cockpit.list_commands(project.machine(), provider, &cwd)
        });
        let place = self.place_of(project.machine(), cx);
        self.update_factory(cx, |view, _| view.skills.listen(provider, reply, place));
    }

    /// A discovery that answered: its skills, for the picker and the check.
    pub(in crate::cockpit) fn poll_factory_skills(&mut self, cx: &mut Context<Self>) {
        self.poll_start_lists(cx);
        if self
            .update_factory(cx, |view, _| view.skills.listed())
            .unwrap_or(false)
        {
            cx.notify();
        }
    }

    /// What is known installed: models as the Providers announced them,
    /// skills as discovered.
    pub(crate) fn factory_catalog(&self, cx: &gpui::App) -> Catalog {
        Catalog {
            models: Provider::ALL
                .into_iter()
                .map(|provider| {
                    let announced = self
                        .factory_machine()
                        .map(|machine| self.cockpit.read(cx).announced_models(machine, provider))
                        .unwrap_or_default();
                    (provider, announced)
                })
                .collect(),
            skills: Provider::ALL
                .into_iter()
                .filter_map(|provider| {
                    let skills = self.factory()?.read(cx).skills.installed(provider)?;
                    Some((provider, skills.iter().map(|s| s.name.clone()).collect()))
                })
                .collect(),
        }
    }

    /// The shown draft's problems.
    pub(crate) fn draft_problems(&self, cx: &gpui::App) -> Vec<Problem> {
        self.shown_workflow(cx)
            .map(|workflow| workflow.draft.problems(&self.factory_catalog(cx)))
            .unwrap_or_default()
    }

    pub(crate) fn add_skill(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(step) = self.shown_step(cx) else {
            return;
        };
        if step.skills.contains(&name) {
            return;
        }
        let mut skills = step.skills.clone();
        skills.push(name);
        let id = step.id.clone();
        self.edit_step(
            id,
            StepChange {
                skills: Some(skills),
                ..Default::default()
            },
            cx,
        );
    }

    pub(crate) fn remove_skill(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(step) = self.shown_step(cx) else {
            return;
        };
        let skills: Vec<String> = step
            .skills
            .iter()
            .filter(|each| *each != name)
            .cloned()
            .collect();
        let id = step.id.clone();
        self.edit_step(
            id,
            StepChange {
                skills: Some(skills),
                ..Default::default()
            },
            cx,
        );
    }
}
