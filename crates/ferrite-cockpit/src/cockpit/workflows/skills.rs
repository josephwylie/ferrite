//! The skills a Step's Provider has installed in the Project, discovered
//! once per Provider, for the picker and the check.

use super::*;

impl CockpitView {
    /// Ask the Provider which skills it has installed in the Project, once.
    pub(super) fn discover_skills(&mut self, provider: Provider) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view
            .workflows
            .skills
            .iter()
            .any(|found| found.provider == provider)
        {
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
        let pending = self.cockpit.discover_commands(provider, &cwd);
        let error = pending
            .is_none()
            .then(|| "This window cannot list skills.".to_string());
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.skills.push(SkillsFound {
                provider,
                pending,
                skills: Vec::new(),
                error,
            });
        }
    }

    /// A discovery that answered: its skills, for the picker and the check.
    pub(in crate::cockpit) fn poll_factory_skills(&mut self, cx: &mut Context<Self>) {
        self.poll_start_lists(cx);
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        let mut heard = false;
        for found in &mut view.workflows.skills {
            let Some(pending) = &found.pending else {
                continue;
            };
            let answer = match pending.try_recv() {
                Ok(answer) => answer,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                Err(error) => Err(std::io::Error::other(error)),
            };
            found.pending = None;
            heard = true;
            match answer {
                Ok(commands) => {
                    found.skills = commands
                        .into_iter()
                        .filter(ferrite_core::SessionCommand::is_skill)
                        .collect()
                }
                Err(error) => found.error = Some(error.to_string()),
            }
        }
        if heard {
            cx.notify();
        }
    }

    /// The skills found for `provider`, once its discovery answered.
    pub(super) fn skills_for(&self, provider: Provider) -> Option<&[ferrite_core::SessionCommand]> {
        self.factory_view
            .as_ref()?
            .workflows
            .skills
            .iter()
            .find(|found| found.provider == provider && found.pending.is_none())
            .filter(|found| found.error.is_none())
            .map(|found| found.skills.as_slice())
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
                    let skills = self.skills_for(provider)?;
                    Some((provider, skills.iter().map(|s| s.name.clone()).collect()))
                })
                .collect(),
        }
    }

    /// The shown draft's problems.
    pub(crate) fn draft_problems(&self) -> Vec<Problem> {
        self.shown_workflow()
            .map(|workflow| workflow.draft.problems(&self.factory_catalog()))
            .unwrap_or_default()
    }

    pub(crate) fn add_skill(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(step) = self.shown_step() else {
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
        let Some(step) = self.shown_step() else {
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
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.skills_open = open;
            view.workflows.focused = open.then_some(Field::SkillSearch);
        }
        cx.notify();
    }
}
