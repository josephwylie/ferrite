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
    // The word modifier: alt on macOS (alt-backspace, alt-left), ctrl on
    // Windows (ctrl-backspace, ctrl-left) — each platform's own text-field
    // grammar. On Windows ctrl-left is a word step and home is the line
    // edge; on macOS cmd-left is the line edge and there is no other.
    let word = match platform {
        Platform::Mac => "alt",
        Platform::Windows => "ctrl",
    };
    let with_word = |key: &str| format!("{word}-{key}");
    // The line-edge modifier for deleting and jumping: cmd on macOS. Windows
    // has no native spelling for "delete to line start", so ctrl-shift
    // carries it there.
    let edge = match platform {
        Platform::Mac => "cmd",
        Platform::Windows => "ctrl-shift",
    };
    let with_edge = |key: &str| format!("{edge}-{key}");
    let with_primary = |key: &str| format!("{primary}-{key}");
    vec![
        ("backspace".into(), "composer::Backspace", None),
        ("delete".into(), "composer::Delete", None),
        ("left".into(), "composer::Left", None),
        ("right".into(), "composer::Right", None),
        ("home".into(), "composer::Home", None),
        ("end".into(), "composer::End", None),
        (with_primary("v"), "composer::Paste", None),
        // Word-wise editing, the basic text-field grammar: one word at a
        // time backwards and forwards, the line halves, word steps, and
        // shift-selection — all scoped to the Composer so nothing else
        // ever sees them.
        (with_word("backspace"), "composer::DeleteWordLeft", Some("Composer")),
        (with_word("delete"), "composer::DeleteWordRight", Some("Composer")),
        (with_edge("backspace"), "composer::DeleteToStart", Some("Composer")),
        (with_edge("delete"), "composer::DeleteToEnd", Some("Composer")),
        (with_word("left"), "composer::WordLeft", Some("Composer")),
        (with_word("right"), "composer::WordRight", Some("Composer")),
        (with_edge("left"), "composer::Home", Some("Composer")),
        (with_edge("right"), "composer::End", Some("Composer")),
        ("shift-left".into(), "composer::SelectLeft", Some("Composer")),
        ("shift-right".into(), "composer::SelectRight", Some("Composer")),
        (
            format!("shift-{word}-left"),
            "composer::SelectWordLeft",
            Some("Composer"),
        ),
        (
            format!("shift-{word}-right"),
            "composer::SelectWordRight",
            Some("Composer"),
        ),
        (
            format!("shift-{edge}-left"),
            "composer::SelectHome",
            Some("Composer"),
        ),
        (
            format!("shift-{edge}-right"),
            "composer::SelectEnd",
            Some("Composer"),
        ),
        ("shift-home".into(), "composer::SelectHome", Some("Composer")),
        ("shift-end".into(), "composer::SelectEnd", Some("Composer")),
        (with_primary("a"), "composer::SelectAll", Some("Composer")),
        // The Composer's own copy and cut. Bound BEFORE the cockpit's cmd-c
        // below and scoped to the Composer: the deeper context wins while
        // the line has a selection, and an empty selection propagates so
        // the transcript's copy still answers.
        (with_primary("c"), "composer::Copy", Some("Composer")),
        (with_primary("x"), "composer::Cut", Some("Composer")),
        // Emacs muscle memory every shell honours: ctrl-a / ctrl-e to the
        // line's ends, ctrl-w kills the word before the caret.
        ("ctrl-a".into(), "composer::Home", Some("Composer")),
        ("ctrl-e".into(), "composer::End", Some("Composer")),
        ("ctrl-w".into(), "composer::DeleteWordLeft", Some("Composer")),
        // Copy the transcript selection a drag made; with nothing selected
        // the key does nothing (the Composer has no selection of its own).
        (with_primary("c"), "cockpit::CopySelection", None),
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
        // #20: browser-tab muscle memory — cmd-t is the same new Thread,
        // and cmd-n stays as an alias beside it.
        (with_primary("t"), "cockpit::NewThread", None),
        // The focused Pane takes the whole cockpit at L1; cmd-f again
        // restores the grid. Escape stays Interrupt (#20 design): stealing
        // the panic key for "exit fullscreen" would make it ambiguous.
        (with_primary("f"), "cockpit::ToggleFullscreen", None),
        // #21: fold the nav to its LED rail and back — the VS Code sidebar
        // muscle memory (cmd-t/w/f are spoken for by #20).
        (with_primary("b"), "cockpit::ToggleNav", None),
        // The platform's own Settings chord.
        (with_primary(","), "cockpit::OpenSettings", None),
        // Shift: the same draft, aimed straight at "new worktree" instead
        // of the checkout the operator is sitting in.
        (with_primary("shift-n"), "cockpit::NewWorktreeThread", None),
        // Tab keeps #29's draft-band walk; on a Thread Pane the same action
        // walks L1 tool disclosures. Shift-Tab is the reverse Thread walk.
        ("tab".into(), "cockpit::BandCycle", None),
        ("shift-tab".into(), "cockpit::ToolCyclePrevious", None),
        // Close parks the Thread; it is still there, and reopening revives it.
        (with_primary("w"), "cockpit::CloseThread", None),
        // And back again: the most recently parked Thread, revived.
        (with_primary("o"), "cockpit::ReopenThread", None),
        // cmd-q is the macOS convention; Windows has no cmd, so ctrl-q there
        // (alt-f4 comes free from the OS).
        (with_primary("q"), "ferrite::Quit", None),
        (
            "up".into(),
            "cockpit::HistoryOlder",
            Some("ComposerHistory"),
        ),
        (
            "down".into(),
            "cockpit::HistoryNewer",
            Some("ComposerHistory"),
        ),
        // #23: the Composer's `/` and `@` popovers (and #29's band
        // popovers, which ride the same keys). These sit BELOW the bare
        // enter and escape rows, because gpui breaks a same-depth tie
        // toward the later binding: it hands the keys to the open menu —
        // enter picks instead of submitting, escape dismisses instead of
        // interrupting — and escape with no popover keeps its existing
        // meaning.
        ("up".into(), "cockpit::MenuPrevious", Some("ComposerMenu")),
        ("down".into(), "cockpit::MenuNext", Some("ComposerMenu")),
        ("enter".into(), "cockpit::MenuPick", Some("ComposerMenu")),
        (
            "enter".into(),
            "cockpit::ToggleTool",
            Some("ToolDisclosure"),
        ),
        (
            "escape".into(),
            "cockpit::MenuDismiss",
            Some("ComposerMenu"),
        ),
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
            ("composer::SelectAll", "a"),
            ("composer::Copy", "c"),
            ("composer::Cut", "x"),
            ("cockpit::CopySelection", "c"),
            ("cockpit::NextPane", "]"),
            ("cockpit::PreviousPane", "["),
            ("cockpit::NextDecision", "d"),
            ("cockpit::NewThread", "n"),
            // #20: cmd-t is the browser-tab spelling of the same new Thread.
            ("cockpit::NewThread", "t"),
            ("cockpit::ToggleFullscreen", "f"),
            // #21: the nav collapses to its rail on both platforms.
            ("cockpit::ToggleNav", "b"),
            ("cockpit::OpenSettings", ","),
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

    #[test]
    fn close_remains_and_the_invented_group_shortcuts_are_absent() {
        let removed = [
            "cockpit::ToggleGroup",
            "cockpit::MoveToGroup",
            "cockpit::RenameGroup",
            "cockpit::MoveGroupUp",
            "cockpit::MoveGroupDown",
        ];
        for platform in [Platform::Mac, Platform::Windows] {
            let actions: Vec<_> = bindings(platform)
                .into_iter()
                .map(|(_, action, _)| action)
                .collect();
            assert!(actions.contains(&"cockpit::CloseThread"));
            assert!(removed.iter().all(|action| !actions.contains(action)));
        }
    }

    /// Tab stays the draft band's chip walk and doubles as the forward L1
    /// disclosure walk; Shift-Tab is its reverse on Thread Panes.
    #[test]
    fn tab_cycles_the_band_on_both_platforms() {
        for platform in [Platform::Mac, Platform::Windows] {
            assert!(
                bindings(platform).contains(&("tab".into(), "cockpit::BandCycle", None)),
                "{platform:?} is missing tab for cockpit::BandCycle"
            );
            assert!(bindings(platform).contains(&(
                "shift-tab".into(),
                "cockpit::ToolCyclePrevious",
                None
            )));
        }
    }

    /// #23: the Composer menus' keys exist on both platforms, only inside
    /// the ComposerMenu key context — a bare arrow key must never steal
    /// from anything else — and their enter/escape rows sit after the bare
    /// Submit/Interrupt rows so gpui's same-depth tie-break picks them
    /// while a popover is up. Escape with no popover keeps its meaning.
    #[test]
    fn the_composer_menu_keys_are_scoped_to_its_context_on_both_platforms() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            for (key, action) in [
                ("up", "cockpit::MenuPrevious"),
                ("down", "cockpit::MenuNext"),
                ("enter", "cockpit::MenuPick"),
                ("escape", "cockpit::MenuDismiss"),
            ] {
                assert!(
                    table.contains(&(key.into(), action, Some("ComposerMenu"))),
                    "{platform:?} is missing {key} for {action} in ComposerMenu"
                );
            }
            for (bare, scoped) in [
                ("cockpit::Submit", "cockpit::MenuPick"),
                ("cockpit::Interrupt", "cockpit::MenuDismiss"),
            ] {
                let at = |wanted: &str| {
                    table
                        .iter()
                        .position(|(_, action, _)| *action == wanted)
                        .unwrap_or_else(|| panic!("{wanted} is not in the table"))
                };
                assert!(
                    at(bare) < at(scoped),
                    "{scoped} must be bound after {bare} ({platform:?})"
                );
            }
            assert!(table.contains(&(
                "enter".into(),
                "cockpit::ToggleTool",
                Some("ToolDisclosure")
            )));
            let submit = table
                .iter()
                .position(|(_, action, _)| *action == "cockpit::Submit")
                .unwrap();
            let toggle = table
                .iter()
                .position(|(_, action, _)| *action == "cockpit::ToggleTool")
                .unwrap();
            assert!(submit < toggle, "tool Enter must beat bare Submit");
        }
    }

    #[test]
    fn prompt_history_arrows_are_scoped_and_menu_arrows_stay_later() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            for (key, action) in [
                ("up", "cockpit::HistoryOlder"),
                ("down", "cockpit::HistoryNewer"),
            ] {
                assert!(
                    table.contains(&(key.into(), action, Some("ComposerHistory"))),
                    "{platform:?} is missing {key} for {action}"
                );
            }
            let history = table
                .iter()
                .position(|(_, action, _)| *action == "cockpit::HistoryOlder")
                .unwrap();
            let menu = table
                .iter()
                .position(|(_, action, _)| *action == "cockpit::MenuPrevious")
                .unwrap();
            assert!(history < menu, "menu arrows must retain precedence");
        }
    }

    /// The word grammar follows each platform's own text fields: alt on
    /// macOS, ctrl on Windows; the line halves are cmd on macOS.
    #[test]
    fn word_editing_follows_each_platforms_text_field_grammar() {
        let table = bindings(Platform::Mac);
        for (key, action) in [
            ("alt-backspace", "composer::DeleteWordLeft"),
            ("alt-delete", "composer::DeleteWordRight"),
            ("cmd-backspace", "composer::DeleteToStart"),
            ("cmd-delete", "composer::DeleteToEnd"),
            ("alt-left", "composer::WordLeft"),
            ("alt-right", "composer::WordRight"),
            ("cmd-left", "composer::Home"),
            ("cmd-right", "composer::End"),
            ("shift-alt-left", "composer::SelectWordLeft"),
            ("shift-cmd-right", "composer::SelectEnd"),
            ("shift-left", "composer::SelectLeft"),
        ] {
            assert!(
                table.contains(&(key.into(), action, Some("Composer"))),
                "mac is missing {key} for {action}"
            );
        }
        let table = bindings(Platform::Windows);
        for (key, action) in [
            ("ctrl-backspace", "composer::DeleteWordLeft"),
            ("ctrl-delete", "composer::DeleteWordRight"),
            ("ctrl-left", "composer::WordLeft"),
            ("ctrl-right", "composer::WordRight"),
            ("shift-ctrl-left", "composer::SelectWordLeft"),
        ] {
            assert!(
                table.contains(&(key.into(), action, Some("Composer"))),
                "windows is missing {key} for {action}"
            );
        }
        // The Composer's copy sits before the cockpit's, so the tie inside
        // the deeper context resolves toward the line's own selection.
        let mac = bindings(Platform::Mac);
        let at = |wanted: &str| {
            mac.iter()
                .position(|(_, action, _)| *action == wanted)
                .unwrap()
        };
        assert!(at("composer::Copy") < at("cockpit::CopySelection"));
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
