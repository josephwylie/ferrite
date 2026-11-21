//! The skills a Step's Provider has installed in the Project, discovered
//! once per Provider, for the picker and the check.
//!
//! Only the root and its pump write them, so they are a plain struct
//! beside the editor's view (ADR 0041), shaped like the import picker's
//! listing: the root asks (`listen`) and its pump takes each answer
//! (`listed`).

use ferrite_core::providers::commands::Discovery;
use ferrite_core::SessionCommand;

use super::*;

/// What one Provider's discovery says.
pub(crate) enum Found {
    /// Asked, not yet answered.
    Looking(Discovery),
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

    /// Wait for `provider`'s discovery; none, this window cannot list.
    fn listen(&mut self, provider: Provider, reply: Option<Discovery>) {
        let found = match reply {
            Some(reply) => Found::Looking(reply),
            None => Found::Failed("This window cannot list skills.".into()),
        };
        self.0.push((provider, found));
    }

    /// Take each discovery that answered; whether any did.
    fn listed(&mut self) -> bool {
        let mut heard = false;
        for (_, found) in &mut self.0 {
            let Found::Looking(reply) = found else {
                continue;
            };
            let answer = match reply.try_recv() {
                Ok(answer) => answer,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                Err(error) => Err(std::io::Error::other(error)),
            };
            heard = true;
            *found = match answer {
                Ok(commands) => Found::Listed(
                    commands
                        .into_iter()
                        .filter(SessionCommand::is_skill)
                        .collect(),
                ),
                Err(error) => Found::Failed(error.to_string()),
            };
        }
        heard
    }

    /// What `provider`'s discovery says, once asked.
    pub(super) fn found(&self, provider: Provider) -> Option<&Found> {
        self.0
            .iter()
            .find(|(each, _)| *each == provider)
            .map(|(_, found)| found)
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
    pub(super) fn discover_skills(&mut self, provider: Provider) {
        let Some(view) = self.factory() else {
            return;
        };
        if view.workflows.skills.asked(provider) {
            return;
        }
        let Some(cwd) = self
            .cockpit
            .registry()
            .project(view.project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        let reply = self.cockpit.discover_commands(provider, &cwd);
        if let Some(view) = self.factory_mut() {
            view.workflows.skills.listen(provider, reply);
        }
    }

    /// A discovery that answered: its skills, for the picker and the check.
    pub(in crate::cockpit) fn poll_factory_skills(&mut self, cx: &mut Context<Self>) {
        self.poll_start_lists(cx);
        if self
            .factory_mut()
            .is_some_and(|view| view.workflows.skills.listed())
        {
            cx.notify();
        }
    }

    /// What is known installed: models as the Providers announced them,
    /// skills as discovered.
    pub(crate) fn factory_catalog(&self) -> Catalog {
        Catalog {
            models: Provider::ALL
                .into_iter()
                .map(|provider| (provider, self.cockpit.announced_models(provider)))
                .collect(),
            skills: Provider::ALL
                .into_iter()
                .filter_map(|provider| {
                    let skills = self.factory()?.workflows.skills.installed(provider)?;
                    Some((provider, skills.iter().map(|s| s.name.clone()).collect()))
                })
                .collect(),
        }
    }

    /// The shown draft's problems.
    pub(crate) fn draft_problems(&self, cx: &gpui::App) -> Vec<Problem> {
        self.shown_workflow(cx)
            .map(|workflow| workflow.draft.problems(&self.factory_catalog()))
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

    pub(crate) fn reveal_skills(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_mut() {
            view.workflows.skills_open = open;
            view.editor
                .update(cx, |editor, _| editor.focused = open.then_some(Field::SkillSearch));
        }
        cx.notify();
    }
}
