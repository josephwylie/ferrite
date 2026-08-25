//! The cockpit's whole key table, spelled per platform.
//!
//! Windows has no cmd key, so every primary shortcut is ctrl there and cmd
//! on macOS. The table is data — plain strings, no gpui dispatch — so both
//! platforms' bindings are asserted by tests that run on any host. gpui
//! 0.2.2 does offer a `secondary-` token with the same mapping, but a token
//! resolved inside gpui's platform cfg cannot be checked for the other
//! platform from one machine; explicit strings can.

/// Which convention the primary modifier follows. Linux, when it arrives,
/// sits on the ctrl side.
// One variant is always the other target's: each build constructs only its
// own PLATFORM, and the cross-platform tests live behind cfg(test).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Mac,
    Windows,
}

/// The convention this build follows.
#[cfg(target_os = "macos")]
pub const PLATFORM: Platform = Platform::Mac;
#[cfg(not(target_os = "macos"))]
pub const PLATFORM: Platform = Platform::Windows;

/// Every key the cockpit binds: (keystroke, action name, key context).
/// Action names are the registered `namespace::Action` strings, so the
/// table stays buildable without touching gpui.
pub fn bindings(platform: Platform) -> Vec<(String, &'static str, Option<&'static str>)> {
    let primary = match platform {
        Platform::Mac => "cmd",
        Platform::Windows => "ctrl",
    };
    let with_primary = |key: &str| format!("{primary}-{key}");
    vec![
        ("backspace".into(), "composer::Backspace", None),
        ("delete".into(), "composer::Delete", None),
        ("left".into(), "composer::Left", None),
        ("right".into(), "composer::Right", None),
        ("home".into(), "composer::Home", None),
        ("end".into(), "composer::End", None),
        (with_primary("v"), "composer::Paste", None),
        ("enter".into(), "cockpit::Submit", None),
        ("escape".into(), "cockpit::Interrupt", None),
        // Only while a Decision holds the keyboard: elsewhere these are
        // just letters going into the Composer.
        ("y".into(), "cockpit::Allow", Some("Decision")),
        ("n".into(), "cockpit::Deny", Some("Decision")),
        ("a".into(), "cockpit::Always", Some("Decision")),
        // At wall range no Pane holds a Composer, so the same keys answer
        // whichever Thread is flagged without focusing it first.
        ("y".into(), "cockpit::Allow", Some("Wall")),
        ("n".into(), "cockpit::Deny", Some("Wall")),
        ("a".into(), "cockpit::Always", Some("Wall")),
        // The cockpit: walk the grid, and jump to whoever needs answering.
        (with_primary("]"), "cockpit::NextPane", None),
        (with_primary("["), "cockpit::PreviousPane", None),
        (with_primary("d"), "cockpit::NextDecision", None),
        (with_primary("n"), "cockpit::NewThread", None),
        // Shift: the same new Thread, in its own worktree instead of the
        // checkout the operator is sitting in.
        (with_primary("shift-n"), "cockpit::NewWorktreeThread", None),
        // Close parks the Thread; it is still there, and reopening revives it.
        (with_primary("w"), "cockpit::CloseThread", None),
        // And back again: the most recently parked Thread, revived.
        (with_primary("o"), "cockpit::ReopenThread", None),
        // cmd-q is the macOS convention; Windows has no cmd, so ctrl-q there
        // (alt-f4 comes free from the OS).
        (with_primary("q"), "ferrite::Quit", None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Keystroke;

    /// Windows has no cmd key: a binding spelled cmd-* is dead there.
    #[test]
    fn windows_never_binds_the_cmd_modifier() {
        for (keystroke, action, _) in bindings(Platform::Windows) {
            assert!(!keystroke.contains("cmd"), "{action} bound to {keystroke}");
        }
    }

    #[test]
    fn primary_shortcuts_are_cmd_on_mac_and_ctrl_on_windows() {
        let expected = [
            ("composer::Paste", "v"),
            ("cockpit::NextPane", "]"),
            ("cockpit::PreviousPane", "["),
            ("cockpit::NextDecision", "d"),
            ("cockpit::NewThread", "n"),
            ("cockpit::NewWorktreeThread", "shift-n"),
            ("cockpit::CloseThread", "w"),
            ("cockpit::ReopenThread", "o"),
            ("ferrite::Quit", "q"),
        ];
        let strokes = |platform: Platform| -> Vec<(String, &'static str)> {
            bindings(platform)
                .into_iter()
                .map(|(keystroke, action, _)| (keystroke, action))
                .collect()
        };
        let mac = strokes(Platform::Mac);
        let windows = strokes(Platform::Windows);
        for (action, key) in expected {
            assert!(
                mac.contains(&(format!("cmd-{key}"), action)),
                "mac is missing cmd-{key} for {action}"
            );
            assert!(
                windows.contains(&(format!("ctrl-{key}"), action)),
                "windows is missing ctrl-{key} for {action}"
            );
        }
    }

    /// The two platforms differ in spelling only: same actions, same order,
    /// same key contexts.
    #[test]
    fn both_platforms_bind_the_same_actions_in_the_same_contexts() {
        let shape = |platform: Platform| -> Vec<(&'static str, Option<&'static str>)> {
            bindings(platform)
                .into_iter()
                .map(|(_, action, context)| (action, context))
                .collect()
        };
        assert_eq!(shape(Platform::Mac), shape(Platform::Windows));
    }

    /// A keystroke gpui cannot parse would panic at startup on one platform
    /// only; parse is pure, so both spellings are checked from here.
    #[test]
    fn every_keystroke_in_the_table_parses() {
        for platform in [Platform::Mac, Platform::Windows] {
            for (keystroke, action, _) in bindings(platform) {
                if let Err(e) = Keystroke::parse(&keystroke) {
                    panic!("{action} ({platform:?}): {e:?}");
                }
            }
        }
    }
}
