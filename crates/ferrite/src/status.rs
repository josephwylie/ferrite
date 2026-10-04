//! The status line's own keys (FL-9): ⇧⇥ cycles the focused Thread's
//! permission mode — the `⇧⇥ mode` the Solo status line names at its right.
//! The cockpit handles the action (`cockpit::palette`); this module names it
//! and holds the pure step.

gpui::actions!(status, [CycleMode]);

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
}
