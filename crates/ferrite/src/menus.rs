//! The application menu bar: every cockpit verb the keys already run,
//! spelled out where a person looks for it, with the platform's own edit
//! menu so the standard cut/copy/paste/select-all reach the Composer. The
//! frame sets it again whenever the keys are bound, so its key
//! equivalents follow the table.

use gpui::{Menu, MenuItem, OsAction, SystemMenuType};

use ferrite_cockpit::{cockpit, palette, shortcuts};
use ferrite_ui::keymap::Quit;

pub fn app_menus() -> Vec<Menu> {
    use cockpit::{
        CloseThread, NewGroup, NewThread, NewWorktreeThread, NextPane, OpenSettings, PreviousPane,
        ReopenThread, ToggleFullscreen, ToggleNav, ToggleNotifications, TogglePanel,
    };
    use ferrite_pane::pane::NextDecision;
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
                MenuItem::os_action("Undo", ferrite_pane::composer::Undo, OsAction::Undo),
                MenuItem::os_action("Redo", ferrite_pane::composer::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", ferrite_pane::composer::Cut, OsAction::Cut),
                MenuItem::os_action("Copy", ferrite_pane::composer::Copy, OsAction::Copy),
                MenuItem::os_action("Paste", ferrite_pane::composer::Paste, OsAction::Paste),
                MenuItem::os_action(
                    "Select All",
                    ferrite_pane::composer::SelectAll,
                    OsAction::SelectAll,
                ),
                MenuItem::separator(),
                MenuItem::action("Find\u{2026}", ferrite_pane::find::Open),
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
    use super::{app_menus, shortcuts};
    use ferrite_frame::keys::{bind, load, KEY_TABLE};
    use ferrite_pane::find;
    use ferrite_ui::keymap;

    /// The keymap's action names are strings (the ones with no default key
    /// too); startup rebuilds real actions from them. Without this, a renamed action would only fail at launch.
    #[gpui::test]
    fn every_table_entry_builds_for_both_platforms(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for platform in [keymap::Platform::Mac, keymap::Platform::Windows] {
                assert_eq!(
                    load(keymap::bindings(platform), cx).len(),
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
            ferrite_pane::pane::init_components(cx);
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
            bind(keymap::bindings(keymap::PLATFORM), cx);
            bind(keymap::bindings(keymap::PLATFORM), cx);
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
                if name == "Toggle Panel"
                    && action.partial_eq(&ferrite_cockpit::cockpit::TogglePanel)
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
}
