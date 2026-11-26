//! The skills a Step's Provider has installed in the Project, asked of the
//! Project's machine once per Provider, for the picker and the check.
//!
//! Only the window writes them, so they are a plain struct on the Factory
//! (ADR 0041), shaped like the import picker's listing: the window asks
//! (`listen`) and its pump takes each answer (`listed`).

use ferrite_client::cockpit::Ask;
use ferrite_core::provider::Provider;
use ferrite_core::SessionCommand;
use gpui::SharedString;

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
pub struct Skills(Vec<(Provider, Found)>);

impl Skills {
    /// Whether `provider` was asked already.
    pub fn asked(&self, provider: Provider) -> bool {
        self.0.iter().any(|(each, _)| *each == provider)
    }

    /// Wait for `provider`'s discovery on the machine `place` names.
    pub fn listen(&mut self, provider: Provider, reply: Ask<Vec<SessionCommand>>, place: SharedString) {
        self.0.push((provider, Found::Looking(reply, place)));
    }

    /// Take each discovery that answered, a failure in the words `said`
    /// gives it on the machine it names; whether any did.
    pub fn listed(&mut self, said: impl Fn(&std::io::Error, &str) -> String) -> bool {
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
                Err(error) => Found::Failed(said(&error, &place)),
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
    pub fn note(&self, provider: Provider) -> String {
        match self.found(provider) {
            Some(Found::Looking(..)) => "Looking for installed skills\u{2026}".into(),
            Some(Found::Failed(error)) => format!("Could not list skills: {error}"),
            Some(Found::Listed(skills)) => format!("{} installed for {provider}", skills.len()),
            None => String::new(),
        }
    }

    /// The skills found for `provider`, once its discovery answered.
    pub fn installed(&self, provider: Provider) -> Option<&[SessionCommand]> {
        match self.found(provider)? {
            Found::Listed(skills) => Some(skills),
            _ => None,
        }
    }
}
