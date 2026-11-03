//! How a provider is named and marked: branding only, in one place. What a
//! provider can do is its bridge's and the shared view's (ferrite-core),
//! never decided here by its name.

use ferrite_core::store::Provider;

use crate::{icons, theme};

/// Its name as a person says it (`Allow Claude to run this command?`).
pub fn title(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

/// Its name lowercase, as a head or a picker section prints it (`· codex ·`):
/// the store's own spelling.
pub fn word(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}

/// Its CLI's name (`Claude CLI`), for the updater.
pub fn cli(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude CLI",
        Provider::Codex => "Codex CLI",
    }
}

/// Its logomark and brand ink: the one place brand colour is allowed.
pub fn mark(provider: Provider) -> (&'static str, u32) {
    match provider {
        Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE),
        Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX),
    }
}
