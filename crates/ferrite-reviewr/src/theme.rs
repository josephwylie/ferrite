//! Ferrite's stand-in for upstream's `theme` (the terminal palettes, dropped): the syntax theme
//! a [`crate::highlight::Highlighter`] is built from. Ferrite paints syntax itself
//! ([`crate::highlight::Highlighter::plain`]); the bundled themes remain for the ported tests.

use two_face::theme::EmbeddedThemeName;

/// The syntax theme paired with a palette: a bundled `.tmTheme`'s vendored bytes (for themes
/// `two-face` lacks, and for Catppuccin Mocha), or a theme from the `two-face` embedded set.
#[derive(Clone, Copy, Debug)]
pub enum SyntaxChoice {
    Bundled(&'static [u8]),
    Embedded(EmbeddedThemeName),
}

/// A theme as the tests resolve it: only its syntax half.
#[cfg(test)]
pub(crate) struct Theme {
    pub(crate) syntax: SyntaxChoice,
}

/// The bundled syntax theme `name` names; anything else is two-face's Catppuccin Mocha.
#[cfg(test)]
pub(crate) fn resolve(name: Option<&str>) -> Theme {
    let syntax = match name.unwrap_or("catppuccin") {
        "catppuccin" => SyntaxChoice::Bundled(include_bytes!("../assets/Catppuccin Mocha.tmTheme")),
        "tokyo-night" => SyntaxChoice::Bundled(include_bytes!("../assets/tokyo-night.tmTheme")),
        "tokyo-night-day" => {
            SyntaxChoice::Bundled(include_bytes!("../assets/tokyo-night-day.tmTheme"))
        }
        "rose-pine" => SyntaxChoice::Bundled(include_bytes!("../assets/rose-pine.tmTheme")),
        "rose-pine-dawn" => {
            SyntaxChoice::Bundled(include_bytes!("../assets/rose-pine-dawn.tmTheme"))
        }
        "ayu" => SyntaxChoice::Bundled(include_bytes!("../assets/ayu-dark.tmTheme")),
        "everforest" => SyntaxChoice::Bundled(include_bytes!("../assets/everforest.tmTheme")),
        _ => SyntaxChoice::Embedded(EmbeddedThemeName::CatppuccinMocha),
    };
    Theme { syntax }
}
