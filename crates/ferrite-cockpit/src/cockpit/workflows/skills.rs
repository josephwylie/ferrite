//! The skills a Step's Provider has installed in the Project, as the
//! window asks for them and takes each answer: what is found lives on the
//! Factory (`ferrite_factory::workflows::Skills`).

use super::*;

impl CockpitView {
    /// Ask the Provider which skills it has installed in the Project, once.
    pub(super) fn discover_skills(&mut self, provider: Provider, cx: &mut gpui::App) {
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return;
        };
        if view.read(cx).skills().asked(provider) {
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
        self.update_factory(cx, |view, _| view.skills_mut().listen(provider, reply, place));
    }

    /// A discovery that answered: its skills, for the picker and the check.
    pub(in crate::cockpit) fn poll_factory_skills(&mut self, cx: &mut Context<Self>) {
        self.poll_start_lists(cx);
        if self
            .update_factory(cx, |view, _| view.skills_mut().listed(crate::cockpit::machines::said))
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
                    let skills = self.factory()?.read(cx).skills().installed(provider)?;
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
