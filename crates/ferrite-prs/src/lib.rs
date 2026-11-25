//! Pull requests and the side chat, out of the Cockpit: the PRs destination
//! (`prs`), the PR reader that draws a PR in a Thread's Panel and in the
//! PRs' detail (`pull_request`), and the side chat's conversations
//! (`side_chat`). Each owns its own state, reads what it draws of the
//! window through a `Source` (`ferrite_ui::source`) and says what the
//! operator asks as an event (ADR 0041). Nothing here names the Cockpit, so
//! an edit to one builds and tests without it.

use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "../../ferrite-core/src/test_temp.rs"]
mod test_temp;

pub mod prs;
pub mod pull_request;
pub mod side_chat;

/// Which of a pull request's views its Surface shows (T3's `detailTab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrTab {
    #[default]
    Summary,
    Guide,
    Timeline,
    Code,
}

impl PrTab {
    pub const ALL: [Self; 4] = [Self::Summary, Self::Guide, Self::Timeline, Self::Code];

    pub fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Guide => "Guide",
            Self::Timeline => "Timeline",
            Self::Code => "Code",
        }
    }
}

#[cfg(test)]
mod tests {
    /// Rule 10 over this crate's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("prs.rs", include_str!("prs.rs")),
            ("pull_request.rs", include_str!("pull_request.rs")),
            ("side_chat.rs", include_str!("side_chat.rs")),
        ]);
    }
}
