use super::*;
use crate::cockpit::surfaces::{CloseSelection, SurfaceAction, SurfaceId, Surfaces};
use crate::cockpit::tools::FilesVerb;

fn preferences(name: &str) -> Preferences {
    let mut prefs = Preferences::ephemeral();
    prefs.dir = scratch(name);
    prefs
}

#[gpui::test]
fn rendering_panels_does_not_write_tabs_or_working_sets(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit("panel-no-render-save", Provider::Claude);
    let prefs = preferences("panel-no-render-save-settings");
    let dir = prefs.dir.clone();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    view.update_in(cx, |view, window, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        for _ in 0..3 {
            let _ = view.render_cockpit(window, cx);
        }
    });
    assert!(!dir.join("panel.json").exists());
    assert!(!dir.join("files.json").exists());
    view.update(cx, |view, cx| view.flush_panel_persistence(cx));
    assert!(dir.join("panel.json").exists(), "the edit needs a save");
    assert!(
        dir.join("files.json").exists(),
        "the working set needs a save"
    );
}

#[gpui::test]
fn idle_pump_saves_panel_and_working_sets_without_a_window(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit("panel-hidden-save", Provider::Claude);
    let prefs = preferences("panel-hidden-save-settings");
    let dir = prefs.dir.clone();
    let view = cx.new(|cx| CockpitView::new_with_settings(core, Provider::Claude, prefs, cx));
    let thread = view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        thread
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !dir.join("files.json").exists() {
        view.update(cx, |view, cx| view.pump(cx));
        assert!(
            std::time::Instant::now() < deadline,
            "an idle pump must save without rendering"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        Surfaces::saved_in(&dir, thread.thread()).unwrap().active,
        Some(SurfaceId::Files)
    );
    let files: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("files.json")).unwrap()).unwrap();
    let saved = &files["threads"][0];
    assert_eq!(saved["machine"], serde_json::Value::Null, "this computer's");
    assert_eq!(saved["thread"], thread.thread().get());
    assert_eq!(saved["files"][0]["path"], checkout.join("src/lib.rs").to_str().unwrap());
}

#[gpui::test]
fn close_reopen_and_files_preferences_restore_with_the_saved_panel(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit("panel-restore-first", Provider::Claude);
    let prefs = preferences("panel-restore-settings");
    let dir = prefs.dir.clone();
    let view = cx.new(|cx| CockpitView::new_with_settings(core, Provider::Claude, prefs, cx));
    let file = checkout.join("src/lib.rs");
    let thread = view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.open_in_files(thread, file.clone(), None, true, cx);
        view.files.update(cx, |files, cx| files.run_verb(thread, FilesVerb::TogglePreviewTabs, cx));
        view.close_surfaces(thread, CloseSelection::One(SurfaceId::Files), cx);
        view.apply_surface(thread, SurfaceAction::Reopen);
        view.apply_surface(thread, SurfaceAction::ToggleDetails);
        view.flush_panel_persistence(cx);
        thread
    });
    let (core, _fake, _checkout) = bound_cockpit("panel-restore-second", Provider::Claude);
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir;
    let restored = cx.new(|cx| CockpitView::new_with_settings(core, Provider::Claude, prefs, cx));
    restored.update(cx, |view, cx| {
        assert_eq!(view.panes[0].thread(), Some(thread));
        assert_eq!(view.active_surface(thread), Some(SurfaceId::Files));
        assert!(!view.panel.surfaces.get(thread).unwrap().details_inline_open);
        view.ensure_files(thread, cx);
        assert_eq!(
            view.files.read(cx).tab_names(thread),
            [("lib.rs".to_string(), false)]
        );
        view.open_context_menu(
            MenuTarget::FilesMore(thread),
            gpui::point(px(0.), px(0.)),
            cx,
        );
        let preview_tabs = view
            .context_menu()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .find(|(item, _)| item.label == "Preview tabs")
            .unwrap();
        assert!(!preview_tabs.0.checked);
    });
}

/// A deleted Thread's working set leaves `files.json` with the next save,
/// and once saved, pumps with nothing new write nothing.
#[gpui::test]
fn a_deleted_threads_working_set_is_saved_away_once(cx: &mut TestAppContext) {
    let (mut core, _fake, checkout) = bound_cockpit("panel-deleted-save", Provider::Claude);
    let other = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: checkout.clone() })
        .unwrap();
    let prefs = preferences("panel-deleted-save-settings");
    let dir = prefs.dir.clone();
    let view = cx.new(|cx| CockpitView::new_with_settings(core, Provider::Claude, prefs, cx));
    let thread = view.update(cx, |view, cx| {
        let thread = view.cockpit.read(cx).threads().into_iter().find(|&thread| thread != other).unwrap();
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        view.flush_panel_persistence(cx);
        view.delete_thread(thread, cx).unwrap();
        view.pump(cx);
        view.open_in_files(other, checkout.join("README.md"), None, true, cx);
        view.pump(cx);
        view.flush_panel_persistence(cx);
        thread
    });
    let files: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("files.json")).unwrap()).unwrap();
    let saved: Vec<_> = files["threads"].as_array().unwrap().iter().map(|set| set["thread"].as_u64()).collect();
    assert_eq!(saved, [Some(other.thread().get())], "its working set is gone");
    std::fs::remove_file(dir.join("files.json")).unwrap();
    view.update(cx, |view, cx| {
        for _ in 0..3 {
            view.pump(cx);
            view.flush_panel_persistence(cx);
        }
    });
    assert!(!dir.join("files.json").exists(), "nothing changed, so nothing is written");
}
