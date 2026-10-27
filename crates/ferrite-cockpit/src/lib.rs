// The Cockpit: the grid, the nav, the Roster glue and the window's
// content, with the key table and menu bar its verbs fill. The app
// (`ferrite`) opens the window on it.
mod background_chips;
#[cfg(unix)]
mod board;
mod cli_updates;
pub mod cockpit;
pub mod cockpit_window;
pub mod demo;
mod facts;
pub mod ferrite_update;
mod float;
mod fuzzy;
pub mod machines;
mod nav;
mod notifications;
mod palette;
mod pr;
mod prefs;
mod project_editor;
mod projects;
mod review;
mod shortcuts;
#[cfg(unix)]
mod sidebar;
mod status;
mod system_notifications;
pub mod titlebar;

// The shared UI, under the names the app has always used for it.
use ferrite_ui::{components, icons, keymap, loops_overlay, menu, motion, pointer, theme};
#[cfg(feature = "visual-reference")]
use ferrite_ui::{platform_text, register_fonts};
// The Pane, likewise.
#[cfg(test)]
use ferrite_pane::hover_card;
use ferrite_pane::{
    composer, decision, file_links, find, image_viewer, pane, preview, prompt_drop, rich, select,
    transcript, visual,
};

use ::gpui;
use ::gpui::*;

actions!(ferrite, [Quit]);

/// Marks the bindings Ferrite's key table made, apart from the ones the
/// kit (its inputs, text views, dialogs) and `rich` bind for themselves.
const KEY_TABLE: KeyBindingMetaIndex = KeyBindingMetaIndex(0);

/// Bind `table` (the defaults with the operator's keys laid over them) in
/// place of the key table bound before, and make it the table every drawn
/// chord reads; the menu bar is set again so its key equivalents follow.
///
/// gpui removes no single binding, so the keymap is rebuilt: everyone
/// else's bindings stay, in order, and the new table takes the old one's
/// place (the end, the first time), keeping same-depth precedence.
pub fn bind_keys(table: Vec<keymap::Binding>, cx: &mut App) {
    keymap::install(table.clone());
    let mut table = Some(load_bindings(table, cx));
    let keymap = cx.key_bindings();
    let mut rebuilt = Vec::new();
    for binding in keymap.borrow().bindings() {
        if binding.meta() == Some(KEY_TABLE) {
            rebuilt.extend(table.take().into_iter().flatten());
        } else {
            rebuilt.push(binding.clone());
        }
    }
    rebuilt.extend(table.into_iter().flatten());
    cx.clear_key_bindings();
    cx.bind_keys(rebuilt);
    cx.set_menus(app_menus());
}

/// A key table (crate::keymap), built into live bindings. The table is
/// data so both platforms' spellings stay testable; actions are rebuilt
/// from their registered names here, and the test below keeps a renamed
/// action from surviving to a launch panic.
fn load_bindings(table: Vec<keymap::Binding>, cx: &mut App) -> Vec<KeyBinding> {
    table
        .into_iter()
        .map(|(keystroke, action, context)| {
            let action = cx
                .build_action(action, None)
                .expect("the keymap names registered actions");
            let context = context.map(|context| {
                KeyBindingContextPredicate::parse(context)
                    .expect("the keymap contexts parse")
                    .into()
            });
            KeyBinding::load(
                &keystroke,
                action,
                context,
                false,
                None,
                &DummyKeyboardMapper,
            )
            .expect("the keymap keystrokes parse")
            .with_meta(KEY_TABLE)
        })
        .collect()
}

/// The application menu bar: every cockpit verb the keys already run,
/// spelled out where a person looks for it, with the platform's own edit
/// menu so the standard cut/copy/paste/select-all reach the Composer.
fn app_menus() -> Vec<Menu> {
    use cockpit::{
        CloseThread, NewGroup, NewThread, NewWorktreeThread, NextDecision, NextPane, OpenSettings,
        PreviousPane, ReopenThread, ToggleFullscreen, ToggleNav, ToggleNotifications, TogglePanel,
    };
    vec![
        Menu {
            disabled: false,
            name: "Ferrite".into(),
            items: vec![
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Ferrite", Quit),
            ],
        },
        Menu {
            disabled: false,
            name: "File".into(),
            items: vec![
                MenuItem::action("New Thread", NewThread),
                MenuItem::action("New Thread in a Worktree", NewWorktreeThread),
                // ⌘⇧G: the focused Thread and a new one, founding a Group.
                MenuItem::action("New Group", NewGroup),
                // ⌘G: the palette scoped to Groups.
                MenuItem::action("Open Group\u{2026}", palette::OpenGroups),
                MenuItem::separator(),
                MenuItem::action("Park Thread", CloseThread),
                MenuItem::action("Reopen Parked Thread", ReopenThread),
                MenuItem::action("Show Parked", palette::ShowParked),
            ],
        },
        Menu {
            disabled: false,
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", composer::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", composer::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", composer::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", composer::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", composer::Paste, OsAction::Paste),
                MenuItem::os_action("Select All", composer::SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Find\u{2026}", find::Open),
            ],
        },
        Menu {
            disabled: false,
            name: "View".into(),
            items: vec![
                MenuItem::action("Commands\u{2026}", palette::Toggle),
                MenuItem::action("Review Branch", palette::ReviewBranch),
                MenuItem::separator(),
                MenuItem::action("Toggle Sidebar", ToggleNav),
                MenuItem::action("Toggle Panel", TogglePanel),
                MenuItem::action("Needs You", ToggleNotifications),
                MenuItem::action("Toggle Pane Fullscreen", ToggleFullscreen),
                MenuItem::separator(),
                MenuItem::action("Next Pane", NextPane),
                MenuItem::action("Previous Pane", PreviousPane),
                MenuItem::action("Next Decision", NextDecision),
            ],
        },
        Menu {
            disabled: false,
            name: "Window".into(),
            items: vec![],
        },
        // The keys' door off the bar (SPEC §5.7), beside the palette's
        // `keyboard shortcuts`.
        Menu {
            disabled: false,
            name: "Help".into(),
            items: vec![MenuItem::action("Keyboard Shortcuts", shortcuts::Toggle)],
        },
    ]
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the crate root globs `gpui::*`, whose `test` macro
    // would capture the `#[test]` this macro expands to and recurse.
    use super::{app_menus, bind_keys, find, keymap, load_bindings, shortcuts, KEY_TABLE};

    /// The keymap's action names are strings (the ones with no default key
    /// too); startup rebuilds real actions from them. Without this, a renamed action would only fail at launch.
    #[gpui::test]
    fn every_table_entry_builds_for_both_platforms(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for platform in [keymap::Platform::Mac, keymap::Platform::Windows] {
                assert_eq!(
                    load_bindings(keymap::bindings(platform), cx).len(),
                    keymap::bindings(platform).len()
                );
            }
            for (action, _) in keymap::UNBOUND {
                assert!(cx.build_action(action, None).is_ok(), "{action}");
            }
        });
    }

    /// Binding the key table, at launch or on `reset keys`, replaces only
    /// the table: the kit's own keys (an input's backspace, rich text's
    /// copy) stay bound, and the table is never bound twice.
    #[gpui::test]
    fn binding_the_key_table_keeps_the_kits_keys(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::pane::init_components(cx);
            let kit = |cx: &mut gpui::App| {
                cx.key_bindings()
                    .borrow()
                    .bindings()
                    .filter(|binding| binding.meta() != Some(KEY_TABLE))
                    .count()
            };
            let ours = |cx: &mut gpui::App| {
                cx.key_bindings()
                    .borrow()
                    .bindings()
                    .filter(|binding| binding.meta() == Some(KEY_TABLE))
                    .count()
            };
            let before = kit(cx);
            // The defaults, as other tests read the installed table too.
            bind_keys(keymap::bindings(keymap::PLATFORM), cx);
            bind_keys(keymap::bindings(keymap::PLATFORM), cx);
            assert_eq!(kit(cx), before);
            assert_eq!(ours(cx), keymap::bindings(keymap::PLATFORM).len());
            assert!(cx
                .key_bindings()
                .borrow()
                .bindings()
                .any(|binding| binding.action().name() == "input::Backspace"));
        });
    }

    /// The Edit menu finds, as ⌘F does: the menu bar spells the key from
    /// the same action's binding.
    #[test]
    fn the_edit_menu_finds() {
        let edit = app_menus()
            .into_iter()
            .find(|menu| menu.name == "Edit")
            .expect("an Edit menu");
        assert!(edit.items.iter().any(|item| matches!(
            item,
            gpui::MenuItem::Action { name, action, .. }
                if name == "Find\u{2026}" && action.partial_eq(&find::Open)
        )));
    }

    /// The View menu toggles the panel beside the sidebar, its chord read
    /// from the same action's binding.
    #[test]
    fn the_view_menu_toggles_the_panel() {
        let view = app_menus()
            .into_iter()
            .find(|menu| menu.name == "View")
            .expect("a View menu");
        let names: Vec<&str> = view
            .items
            .iter()
            .filter_map(|item| match item {
                gpui::MenuItem::Action { name, .. } => Some(name.as_ref()),
                _ => None,
            })
            .collect();
        let sidebar = names.iter().position(|name| *name == "Toggle Sidebar");
        let panel = names.iter().position(|name| *name == "Toggle Panel");
        assert_eq!(panel, sidebar.map(|at| at + 1), "beside the sidebar's");
        assert!(view.items.iter().any(|item| matches!(
            item,
            gpui::MenuItem::Action { name, action, .. }
                if name == "Toggle Panel" && action.partial_eq(&crate::cockpit::TogglePanel)
        )));
    }

    /// `?` left the bar (SPEC §5.7): the Help menu opens the keys.
    #[test]
    fn the_help_menu_opens_the_keys() {
        let help = app_menus()
            .into_iter()
            .find(|menu| menu.name == "Help")
            .expect("a Help menu");
        assert!(help.items.iter().any(|item| matches!(
            item,
            gpui::MenuItem::Action { name, action, .. }
                if name == "Keyboard Shortcuts" && action.partial_eq(&shortcuts::Toggle)
        )));
    }

    /// Rule 10 over the Cockpit's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("lib.rs", include_str!("lib.rs")),
            ("cockpit.rs", include_str!("cockpit.rs")),
            ("cockpit/subagents.rs", include_str!("cockpit/subagents.rs")),
            ("cockpit/panel/preview.rs", include_str!("cockpit/panel/preview.rs")),
            ("nav.rs", include_str!("nav.rs")),
            ("titlebar.rs", include_str!("titlebar.rs")),
            ("prefs.rs", include_str!("prefs.rs")),
            ("project_editor.rs", include_str!("project_editor.rs")),
            ("notifications.rs", include_str!("notifications.rs")),
            ("background_chips.rs", include_str!("background_chips.rs")),
        ]);
    }
}
