//! The status line's own keys (FL-9): ⇧⇥ cycles the focused Thread's
//! permission mode — the `⇧⇥ mode` the Solo status line names at its right.
//! The cockpit handles the action (`cockpit::palette`); this module names it
//! and holds the pure step.

gpui::actions!(status, [CycleMode]);

/// Whether ⇧⇥ steps onto `mode`. Claude Code's own cycle is `default` →
/// `acceptEdits` → `plan` (→ bypass, where the Session was launched in it):
/// `dontAsk` stays off it, and `auto` too — the CLI refuses it for most
/// models ("auto mode unavailable for this model", 2.1.289). Both stay
/// choosable from the palette's `permission mode`.
pub(crate) fn cycles(mode: &str) -> bool {
    !matches!(mode, "dontAsk" | "auto")
}

/// The mode after `current` in the Session's own list (`default` → `accept
/// edits` → `plan` → …, wrapping): the first when `current` is not in it.
/// `None` when the Session offers no modes.
pub(crate) fn next_mode<'a>(current: Option<&str>, modes: &'a [String]) -> Option<&'a str> {
    if modes.is_empty() {
        return None;
    }
    let at = current
        .and_then(|current| modes.iter().position(|mode| mode == current))
        .map_or(0, |at| (at + 1) % modes.len());
    Some(modes[at].as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_steps_through_the_list_and_wraps() {
        let modes: Vec<String> = ["default", "acceptEdits", "plan"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(next_mode(Some("default"), &modes), Some("acceptEdits"));
        assert_eq!(next_mode(Some("acceptEdits"), &modes), Some("plan"));
        assert_eq!(next_mode(Some("plan"), &modes), Some("default"));
        assert_eq!(next_mode(Some("unheard"), &modes), Some("default"));
        assert_eq!(next_mode(None, &modes), Some("default"));
        assert_eq!(next_mode(Some("plan"), &[]), None);
    }

    /// The cycle is Claude Code's: what the CLI refuses or keeps off its
    /// own ⇧⇥ is not stepped onto.
    #[test]
    fn the_cycle_skips_the_modes_the_cli_keeps_off_it() {
        let cycle: Vec<&str> = ["default", "acceptEdits", "plan", "dontAsk", "auto"]
            .into_iter()
            .filter(|mode| cycles(mode))
            .collect();
        assert_eq!(cycle, ["default", "acceptEdits", "plan"]);
        assert!(cycles("on-request"), "Codex's policies all cycle");
    }
}
