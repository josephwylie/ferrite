use super::*;

/// A `settings.json` the window could not read is never saved over: a
/// change applies for this run and the file stays exactly as it was.
#[gpui::test]
fn an_unreadable_settings_file_is_never_saved_over(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("settings-unreadable", 1);
    let prefs = Preferences {
        unreadable: Some("expected value at line 1 column 1".into()),
        ..Preferences::ephemeral()
    };
    let path = prefs.dir.join(ferrite_core::settings::Settings::FILE);
    std::fs::create_dir_all(&prefs.dir).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.unreadable_told, "the window said so")
    });

    view.update(cx, |view, cx| {
        view.change_settings(|settings| settings.confirm_delete = false, cx)
    });

    view.read_with(cx, |view, _| assert!(!view.prefs.settings.confirm_delete));
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
}

/// Settings › About's Diagnostics puts the facts and the log's end on the
/// clipboard.
#[gpui::test]
fn copy_diagnostics_puts_the_report_on_the_clipboard(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("copy-diagnostics", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

    view.update(cx, |view, cx| view.copy_diagnostics(cx));

    let report = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap();
    assert!(report.starts_with("Ferrite diagnostics\n"), "{report}");
    assert!(report.contains("Open Threads: 1\n"), "{report}");
    assert!(report.contains("--- log, last"), "{report}");
    view.read_with(cx, |view, _| {
        assert_eq!(view.settings_copied, Some("Diagnostics"))
    });
}
