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

/// The key context of a keystroke no text field takes: the cockpit's own
/// root holds the keyboard (the transcript, the board, a wall), and neither
/// a Composer nor a kit input is on the focus path.
pub const NO_TEXT_FIELD: &str = "Ferrite && !Composer && !Input";

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
        // ⌘V with the keyboard anywhere but a Composer still pastes into
        // the focused Pane's Composer, and moves the keyboard there.
        (with_primary("v"), "cockpit::Paste", None),
        // Word-wise editing, the basic text-field grammar: one word at a
        // time backwards and forwards, the line halves, word steps, and
        // shift-selection — all scoped to the Composer so nothing else
        // ever sees them.
        (
            with_word("backspace"),
            "composer::DeleteWordLeft",
            Some("Composer"),
        ),
        (
            with_word("delete"),
            "composer::DeleteWordRight",
            Some("Composer"),
        ),
        (
            with_edge("backspace"),
            "composer::DeleteToStart",
            Some("Composer"),
        ),
        // ⌘⌫ on an empty line parks the Thread (the palette's `park
        // thread`): bound after DeleteToStart, so the empty line's own
        // context wins the same-depth tie and a line with text still
        // deletes to its start. With no text field holding the keyboard
        // (the transcript, the board) it parks too.
        (
            with_primary("backspace"),
            "cockpit::CloseThread",
            Some("ComposerEmpty"),
        ),
        (
            with_primary("backspace"),
            "cockpit::CloseThread",
            Some(NO_TEXT_FIELD),
        ),
        (
            with_edge("delete"),
            "composer::DeleteToEnd",
            Some("Composer"),
        ),
        (with_word("left"), "composer::WordLeft", Some("Composer")),
        (with_word("right"), "composer::WordRight", Some("Composer")),
        (with_edge("left"), "composer::Home", Some("Composer")),
        (with_edge("right"), "composer::End", Some("Composer")),
        (
            "shift-left".into(),
            "composer::SelectLeft",
            Some("Composer"),
        ),
        (
            "shift-right".into(),
            "composer::SelectRight",
            Some("Composer"),
        ),
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
        (
            "shift-home".into(),
            "composer::SelectHome",
            Some("Composer"),
        ),
        ("shift-end".into(), "composer::SelectEnd", Some("Composer")),
        (with_primary("a"), "composer::SelectAll", Some("Composer")),
        // The Composer's own copy and cut. Bound BEFORE the cockpit's cmd-c
        // below and scoped to the Composer: the deeper context wins while
        // the line has a selection, and an empty selection propagates so
        // the transcript's copy still answers.
        (with_primary("c"), "composer::Copy", Some("Composer")),
        (with_primary("x"), "composer::Cut", Some("Composer")),
        (with_primary("z"), "composer::Undo", Some("Composer")),
        (with_primary("shift-z"), "composer::Redo", Some("Composer")),
        // Emacs muscle memory every shell honours: ctrl-a / ctrl-e to the
        // line's ends, ctrl-w kills the word before the caret.
        // A hard line break in the draft: the box grows a row and enter
        // still sends the whole thing.
        ("shift-enter".into(), "composer::Newline", Some("Composer")),
        ("ctrl-a".into(), "composer::Home", Some("Composer")),
        ("ctrl-e".into(), "composer::End", Some("Composer")),
        (
            "ctrl-w".into(),
            "composer::DeleteWordLeft",
            Some("Composer"),
        ),
        // Copy the transcript selection a drag made; with nothing selected
        // the key does nothing (the Composer has no selection of its own).
        (with_primary("c"), "cockpit::CopySelection", None),
        ("enter".into(), "cockpit::Submit", None),
        // Kit buttons synthesize clicks from Enter/Space; do not submit a prompt.
        ("enter".into(), "zed::NoAction", Some("PromptAttachment")),
        ("escape".into(), "cockpit::Interrupt", None),
        // Only while a Decision holds the keyboard: elsewhere these are
        // just letters going into the Composer.
        ("y".into(), "cockpit::Allow", Some("Decision")),
        ("n".into(), "cockpit::Deny", Some("Decision")),
        ("a".into(), "cockpit::Always", Some("Decision")),
        // A question Decision's options, by number — digits again with
        // text on the line, the y/n/a rule.
        ("1".into(), "cockpit::PickOption1", Some("Decision")),
        ("2".into(), "cockpit::PickOption2", Some("Decision")),
        ("3".into(), "cockpit::PickOption3", Some("Decision")),
        ("4".into(), "cockpit::PickOption4", Some("Decision")),
        // At wall range no Pane holds a Composer, so the same keys answer
        // whichever Thread is flagged without focusing it first.
        ("y".into(), "cockpit::Allow", Some("Wall")),
        ("n".into(), "cockpit::Deny", Some("Wall")),
        ("a".into(), "cockpit::Always", Some("Wall")),
        // The wall tile's boxed answers by their digits (`1 allow`,
        // `2 always`, `3 deny`; a question's options by number).
        ("1".into(), "cockpit::PickOption1", Some("Wall")),
        ("2".into(), "cockpit::PickOption2", Some("Wall")),
        ("3".into(), "cockpit::PickOption3", Some("Wall")),
        // The cockpit: walk the grid, and jump to whoever needs answering.
        (with_primary("]"), "cockpit::NextPane", None),
        (with_primary("["), "cockpit::PreviousPane", None),
        (with_primary("d"), "cockpit::NextDecision", None),
        // The board's ordinals: ⌘1…⌘9 focus the shown board's Panes in head
        // order (Solo: ⌘1 is the Solo Pane).
        (with_primary("1"), "cockpit::FocusThread1", None),
        (with_primary("2"), "cockpit::FocusThread2", None),
        (with_primary("3"), "cockpit::FocusThread3", None),
        (with_primary("4"), "cockpit::FocusThread4", None),
        (with_primary("5"), "cockpit::FocusThread5", None),
        (with_primary("6"), "cockpit::FocusThread6", None),
        (with_primary("7"), "cockpit::FocusThread7", None),
        (with_primary("8"), "cockpit::FocusThread8", None),
        (with_primary("9"), "cockpit::FocusThread9", None),
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
        // The bell: what finished while the operator looked elsewhere.
        (with_primary("i"), "cockpit::ToggleNotifications", None),
        // Shift: the same draft, aimed straight at "new worktree" instead
        // of the checkout the operator is sitting in.
        (with_primary("shift-n"), "cockpit::NewWorktreeThread", None),
        // ⌘G opens a Group: the palette scoped to Groups. New Group (the
        // focused solo Thread plus a new one) moves to ⌘⇧G.
        (with_primary("g"), "palette::OpenGroups", None),
        (with_primary("shift-g"), "cockpit::NewGroup", None),
        // ⌘K: the command palette — every Thread, then every command.
        (with_primary("k"), "palette::Toggle", None),
        // The palette's two keyed commands: the parked Threads, and the
        // focused Thread's diff against main in a reader beside it.
        (with_primary("shift-p"), "palette::ShowParked", None),
        (with_primary("shift-d"), "palette::CompareWithMain", None),
        // The transcript's reading size, the browser's zoom keys: `=` (and
        // `+`, its shifted face) steps up, `-` down, `0` back to Standard.
        (with_primary("="), "cockpit::TextLarger", None),
        (with_primary("+"), "cockpit::TextLarger", None),
        (with_primary("-"), "cockpit::TextSmaller", None),
        (with_primary("0"), "cockpit::TextReset", None),
        // Tab accepts a highlighted command first, then keeps #29's draft-band
        // walk or L1 tool disclosure walk. Shift-Tab cycles the permission
        // mode (the status line's `⇧⇥ mode`); inside a tool disclosure walk
        // it stays the walk's reverse step.
        ("tab".into(), "cockpit::BandCycle", Some("Ferrite")),
        ("shift-tab".into(), "status::CycleMode", Some("Ferrite")),
        (
            "shift-tab".into(),
            "cockpit::ToolCyclePrevious",
            Some("ToolDisclosure"),
        ),
        // `?`: the shortcuts sheet, read from this table. Only on an empty
        // Composer line or with no text field holding the keyboard; a `?`
        // typed into text is just a character.
        ("?".into(), "shortcuts::Toggle", Some("ComposerEmpty")),
        ("?".into(), "shortcuts::Toggle", Some(NO_TEXT_FIELD)),
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
        // On a draft of more than one visual row the arrows walk the
        // caret between rows. Bound AFTER the history rows: gpui tries
        // the later same-depth binding first, so the Composer sees the
        // key and hands it on (propagates) only from the first or last
        // row — where history recall keeps its single-line meaning.
        ("up".into(), "composer::Up", Some("Composer")),
        ("down".into(), "composer::Down", Some("Composer")),
        // A Decision row under an empty line: ↑↓ walk its options, ⏎ picks
        // the selected one, esc denies, tab amends (a note sent with the
        // answer). Bound after the history and row walks and after the bare
        // enter and escape rows, so the empty line's own context wins the
        // same-depth tie; with text on the line every key edits it.
        (
            "up".into(),
            "decision::SelectPrevious",
            Some("Decision > ComposerEmpty"),
        ),
        (
            "down".into(),
            "decision::SelectNext",
            Some("Decision > ComposerEmpty"),
        ),
        (
            "enter".into(),
            "decision::Confirm",
            Some("Decision > ComposerEmpty"),
        ),
        (
            "escape".into(),
            "decision::Dismiss",
            Some("Decision > ComposerEmpty"),
        ),
        (
            "tab".into(),
            "decision::Amend",
            Some("Decision > ComposerEmpty"),
        ),
        // The empty board's key list (`new thread ⌘N`, …): ↑↓ walk it, ⏎
        // runs the selected row.
        ("up".into(), "empty_board::Previous", Some("EmptyBoard")),
        ("down".into(), "empty_board::Next", Some("EmptyBoard")),
        ("enter".into(), "empty_board::Run", Some("EmptyBoard")),
        // The floats' own keys, each in its float's context and bound after
        // the bare rows they shadow: the palette (↑↓ select, ⏎ open, ⇥
        // preview in a pane, esc), the notifications list (↑↓ select, ⏎
        // open, ⌫ dismiss, esc) and the shortcuts sheet (↑↓ scroll, esc).
        ("up".into(), "palette::SelectPrevious", Some("Palette")),
        ("down".into(), "palette::SelectNext", Some("Palette")),
        ("enter".into(), "palette::Confirm", Some("Palette")),
        ("tab".into(), "palette::Preview", Some("Palette")),
        ("escape".into(), "palette::Dismiss", Some("Palette")),
        (
            "up".into(),
            "notifications::SelectPrevious",
            Some("Notifications"),
        ),
        (
            "down".into(),
            "notifications::SelectNext",
            Some("Notifications"),
        ),
        ("enter".into(), "notifications::Open", Some("Notifications")),
        (
            "backspace".into(),
            "notifications::Dismiss",
            Some("Notifications"),
        ),
        (
            "escape".into(),
            "notifications::Close",
            Some("Notifications"),
        ),
        ("up".into(), "shortcuts::ScrollUp", Some("Shortcuts")),
        ("down".into(), "shortcuts::ScrollDown", Some("Shortcuts")),
        ("escape".into(), "shortcuts::Dismiss", Some("Shortcuts")),
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
            ("composer::Undo", "z"),
            ("composer::Redo", "shift-z"),
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
            // ⌘G opens a Group; New Group moves to ⌘⇧G.
            ("palette::OpenGroups", "g"),
            ("cockpit::NewGroup", "shift-g"),
            ("palette::Toggle", "k"),
            ("palette::ShowParked", "shift-p"),
            ("palette::CompareWithMain", "shift-d"),
            ("cockpit::TextLarger", "="),
            ("cockpit::TextSmaller", "-"),
            ("cockpit::TextReset", "0"),
            ("cockpit::CloseThread", "w"),
            ("cockpit::CloseThread", "backspace"),
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
    /// disclosure walk; Shift-Tab cycles the permission mode, and stays the
    /// disclosure walk's reverse step only inside that walk.
    #[test]
    fn tab_cycles_the_band_and_shift_tab_cycles_the_mode() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            assert!(
                table.contains(&("tab".into(), "cockpit::BandCycle", Some("Ferrite"))),
                "{platform:?} is missing tab for cockpit::BandCycle"
            );
            assert!(table.contains(&("shift-tab".into(), "status::CycleMode", Some("Ferrite"))));
            assert!(table.contains(&(
                "shift-tab".into(),
                "cockpit::ToolCyclePrevious",
                Some("ToolDisclosure")
            )));
            assert!(
                !table.contains(&(
                    "shift-tab".into(),
                    "cockpit::ToolCyclePrevious",
                    Some("Ferrite")
                )),
                "the mode owns shift-tab outside a disclosure walk"
            );
        }
    }

    /// The position of the first row binding `key` to `action`.
    fn at(
        table: &[(String, &'static str, Option<&'static str>)],
        key: &str,
        action: &str,
    ) -> usize {
        table
            .iter()
            .position(|(keys, bound, _)| keys == key && *bound == action)
            .unwrap_or_else(|| panic!("{key} → {action} is not in the table"))
    }

    /// A Decision row's keys live under an empty Composer line only, and
    /// sit after every bare and Composer row they shadow, so gpui's
    /// same-depth tie-break hands them the key: ↑↓ past history and the row
    /// walk, ⏎ past Submit, esc past Interrupt. The ComposerMenu rows stay
    /// later still. The number keys answer by number in the Decision
    /// context and on the wall.
    #[test]
    fn decision_keys_win_on_an_empty_line_and_numbers_pick() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            for (key, action) in [
                ("up", "decision::SelectPrevious"),
                ("down", "decision::SelectNext"),
                ("enter", "decision::Confirm"),
                ("escape", "decision::Dismiss"),
                ("tab", "decision::Amend"),
            ] {
                assert!(
                    table.contains(&(key.into(), action, Some("Decision > ComposerEmpty"))),
                    "{platform:?} is missing {key} for {action}"
                );
            }
            assert!(
                at(&table, "up", "cockpit::HistoryOlder")
                    < at(&table, "up", "decision::SelectPrevious")
            );
            assert!(
                at(&table, "up", "composer::Up") < at(&table, "up", "decision::SelectPrevious")
            );
            assert!(
                at(&table, "down", "composer::Down") < at(&table, "down", "decision::SelectNext")
            );
            assert!(
                at(&table, "enter", "cockpit::Submit") < at(&table, "enter", "decision::Confirm")
            );
            assert!(
                at(&table, "escape", "cockpit::Interrupt")
                    < at(&table, "escape", "decision::Dismiss")
            );
            assert!(
                at(&table, "up", "decision::SelectPrevious")
                    < at(&table, "up", "cockpit::MenuPrevious")
            );
            for (key, action) in [
                ("1", "cockpit::PickOption1"),
                ("2", "cockpit::PickOption2"),
                ("3", "cockpit::PickOption3"),
            ] {
                assert!(table.contains(&(key.into(), action, Some("Decision"))));
                assert!(table.contains(&(key.into(), action, Some("Wall"))));
            }
            for (key, action) in [
                ("y", "cockpit::Allow"),
                ("n", "cockpit::Deny"),
                ("a", "cockpit::Always"),
            ] {
                assert!(table.contains(&(key.into(), action, Some("Decision"))));
                assert!(table.contains(&(key.into(), action, Some("Wall"))));
            }
        }
    }

    /// Every float's keys sit in its own context and after the bare rows
    /// they shadow; `?` opens the shortcuts sheet only where no text is
    /// being typed; ⌘⌫ parks from an empty line or with no text field.
    #[test]
    fn the_floats_and_the_empty_board_have_their_keys() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            for (key, action, context) in [
                ("up", "palette::SelectPrevious", "Palette"),
                ("down", "palette::SelectNext", "Palette"),
                ("enter", "palette::Confirm", "Palette"),
                ("tab", "palette::Preview", "Palette"),
                ("escape", "palette::Dismiss", "Palette"),
                ("up", "notifications::SelectPrevious", "Notifications"),
                ("down", "notifications::SelectNext", "Notifications"),
                ("enter", "notifications::Open", "Notifications"),
                ("backspace", "notifications::Dismiss", "Notifications"),
                ("escape", "notifications::Close", "Notifications"),
                ("up", "shortcuts::ScrollUp", "Shortcuts"),
                ("down", "shortcuts::ScrollDown", "Shortcuts"),
                ("escape", "shortcuts::Dismiss", "Shortcuts"),
                ("up", "empty_board::Previous", "EmptyBoard"),
                ("down", "empty_board::Next", "EmptyBoard"),
                ("enter", "empty_board::Run", "EmptyBoard"),
                ("?", "shortcuts::Toggle", "ComposerEmpty"),
                ("?", "shortcuts::Toggle", NO_TEXT_FIELD),
            ] {
                assert!(
                    table.contains(&(key.into(), action, Some(context))),
                    "{platform:?} is missing {key} for {action} in {context}"
                );
            }
            assert!(
                at(&table, "enter", "cockpit::Submit") < at(&table, "enter", "palette::Confirm")
            );
            assert!(
                at(&table, "escape", "cockpit::Interrupt")
                    < at(&table, "escape", "palette::Dismiss")
            );
            assert!(
                at(&table, "backspace", "composer::Backspace")
                    < at(&table, "backspace", "notifications::Dismiss")
            );
            assert!(
                at(&table, "escape", "cockpit::Interrupt")
                    < at(&table, "escape", "shortcuts::Dismiss")
            );
            assert!(
                at(&table, "enter", "cockpit::Submit") < at(&table, "enter", "empty_board::Run")
            );
            let park = match platform {
                Platform::Mac => "cmd-backspace",
                Platform::Windows => "ctrl-backspace",
            };
            assert!(table.contains(&(park.into(), "cockpit::CloseThread", Some("ComposerEmpty"))));
            assert!(table.contains(&(park.into(), "cockpit::CloseThread", Some(NO_TEXT_FIELD))));
            let delete = match platform {
                Platform::Mac => "cmd-backspace",
                Platform::Windows => "ctrl-shift-backspace",
            };
            assert!(
                at(&table, delete, "composer::DeleteToStart")
                    < at(&table, park, "cockpit::CloseThread"),
                "an empty line's park beats the delete; text still deletes"
            );
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

    /// The Composer's own row walk sits between history and the menu:
    /// the menu still owns the arrows while it is up, and a row step that
    /// propagates from the first or last row reaches history next.
    #[test]
    fn row_arrows_sit_between_history_and_the_menu() {
        for platform in [Platform::Mac, Platform::Windows] {
            let table = bindings(platform);
            let at = |wanted: &str| {
                table
                    .iter()
                    .position(|(_, action, _)| *action == wanted)
                    .unwrap_or_else(|| panic!("{wanted} is not in the table"))
            };
            for (key, action) in [("up", "composer::Up"), ("down", "composer::Down")] {
                assert!(
                    table.contains(&(key.into(), action, Some("Composer"))),
                    "{platform:?} is missing {key} for {action}"
                );
            }
            assert!(at("cockpit::HistoryOlder") < at("composer::Up"));
            assert!(at("composer::Up") < at("cockpit::MenuPrevious"));
            assert!(at("cockpit::HistoryNewer") < at("composer::Down"));
            assert!(at("composer::Down") < at("cockpit::MenuNext"));
            assert!(table.contains(&("shift-enter".into(), "composer::Newline", Some("Composer"))));
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
