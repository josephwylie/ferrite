//! The setup sheet (ADR 0048): a draft's Project of this computer copied
//! to the Machine it picked, registered there and chosen on the draft. The
//! copy is the window's, followed by the card, a toast and the draft's
//! chip; only Cancel stops it. A copy stopped part way is unfinished, and
//! only its Resume changes what it left there.

use super::*;
use ferrite_core::sound::Sound;

/// The draft's composer text.
fn draft_text(view: &Entity<CockpitView>, draft: PaneIdentity, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.index_of(draft).unwrap()];
        pane.composer.read(cx).text().to_string()
    })
}

/// Where box's own files are kept on this computer.
fn box_dir(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> PathBuf {
    view.read_with(cx, |view, _| ferrite_core::machine::dir(&view.prefs.dir, &box_machine().target))
}

fn card_size(cx: &mut gpui::VisualTestContext) -> gpui::Size<gpui::Pixels> {
    debug_bounds(cx, "placement-card".into()).expect("the card").size
}

/// A first copy under way once the copy has made its folder there and box
/// is slow: what is left of it takes long enough to stop part way. The
/// card shows how far it is.
fn copying_part_way(dest: &Path, cx: &mut gpui::VisualTestContext) {
    BOX_SLOW.store(true, Ordering::SeqCst);
    press(cx, "placement-copy");
    until(cx, |cx| {
        dest.join(".gitignore").is_file()
            && text_of(cx, "placement-sent").is_some_and(|said| said.contains(" of "))
    });
    assert!(debug_bounds(cx, "placement-cancel".into()).is_some());
    assert!(debug_bounds(cx, "placement-resume".into()).is_none(), "a first copy is no Resume");
}

/// Whether a copy runs, whatever shows it.
fn copy_runs(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| view.copies.read(cx).iter().any(|(_, copy)| copy.phase.live()))
}

/// The tag of the system's notification of the one copy.
fn copy_tag(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| view.copies.read(cx).iter().next().expect("a copy").0.tag())
}

/// The kit's toasts at rest, as the rest of its motion is in unit tests
/// (`ferrite_ui::motion::live`): they slide on the wall clock, which a
/// test does not step, and a press on one still sliding in misses.
fn toasts_rest(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

/// `selector` on a copy's toast, its stack settled: a toast it replaced
/// leaves on the stack's clock, and until then is read as well.
fn on_toast(cx: &mut gpui::VisualTestContext, selector: &str) {
    drawn(cx, selector);
    settles(cx);
}

/// The copy's toast closed as the operator closes it: the pointer over it
/// shows its close button.
fn close_toast(cx: &mut gpui::VisualTestContext) {
    let card = debug_bounds(cx, "notification-card".into()).expect("a toast");
    cx.simulate_mouse_move(card.center(), None::<gpui::MouseButton>, gpui::Modifiers::none());
    tick(cx);
    press(cx, "notification-close");
    settles(cx);
}

/// `path` as the sheet shows box's paths: under `~/` where it is in the
/// home.
fn shown(path: &Path) -> String {
    let home = ferrite_core::folder::expand_home(Path::new("~"));
    let home = dunce::canonicalize(&home).unwrap_or(home);
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()).replace('\\', "/"),
        Err(_) => path.display().to_string().replace('\\', "/"),
    }
}

/// Every file under `dir`, with its size.
fn files_in(dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                dirs.push(entry.path());
            } else {
                found.push((entry.path(), meta.len()));
            }
        }
    }
    found.sort();
    found
}

/// With none on box, the setup sheet copies the draft's Project there:
/// its browser opens in box's projects folder, the copy named for the
/// Project's folder, and can be aimed elsewhere; what git ignores is listed
/// unticked, and what is ticked goes too. The copy is registered on box
/// and chosen on the draft, its git reads it as unchanged, and from then on
/// each environment's Project is the other's counterpart.
#[gpui::test]
fn a_project_with_none_there_is_copied_by_the_sheet_and_known_there_after(cx: &mut TestAppContext) {
    let base = resolved_dir("env-copy");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-copy", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    view.update(cx, |view, cx| {
        view.panes[view.index_of(draft).unwrap()]
            .composer
            .update(cx, |composer, cx| composer.set("hello".into(), cx));
    });

    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });
    assert_eq!(
        text_in(&sheet_line(&view, cx), cx),
        format!("{}/", projects.display()),
        "box's projects folder"
    );
    assert_eq!(
        destination(&view, cx),
        format!("{}/app", projects.display()),
        "the Project's folder name in it"
    );
    assert!(debug_bounds(cx, "placement-ignored:target".into()).is_some());
    assert!(debug_bounds(cx, "folder-browser-rows".into()).is_none(), "the browser opens only when asked");
    let dest = projects.join("copied");
    aim_sheet_at(&view, &dest, cx);
    press(cx, "placement-ignored:.env");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    drawn(cx, "placement-copied");
    assert!(
        text_of(cx, "placement-copied").is_some_and(|said| said.contains("B in ")),
        "what it copied, and in how long: {:?}",
        text_of(cx, "placement-copied")
    );
    press(cx, "placement-done");
    assert!(sheet(&view, cx).is_none(), "Done closes it");
    let copy = draft_project(&view, draft, cx);
    assert!(
        roots_on(&view, on_box, cx).contains(&dest),
        "registered on box"
    );
    assert!(dest.join("main.rs").is_file());
    assert!(dest.join(".git").is_dir());
    assert!(dest.join(".env").is_file(), "the ticked entry went");
    assert!(!dest.join("target").exists(), "the unticked one did not");
    assert_eq!(
        git(&dest, &["status", "--porcelain"]),
        "",
        "git there reads the copy as unchanged"
    );
    let title = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.cockpit
                .read(cx)
                .project(copy)
                .map(|project| project.title.clone())
        })
    };
    until(cx, |cx| title(cx).as_deref() == Some("app"));
    assert_eq!(
        text_of(cx, "band-chip-0").as_deref(),
        Some("app"),
        "named as the Project it copies, not as its folder"
    );
    assert_eq!(text_of(cx, ENVIRONMENT).as_deref(), Some("box"));
    assert_eq!(draft_text(&view, draft, cx), "hello");

    pick_on(&view, ENVIRONMENT, "Local", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == mine);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == copy);
    assert!(
        sheet(&view, cx).is_none(),
        "the copy is the counterpart now"
    );
}

/// A Project that git says is a repository is copied all the same, and
/// where it went is kept as for any other: the copy is its counterpart on
/// box whatever its remote says later.
#[gpui::test]
fn where_a_copy_went_is_kept_whatever_git_says(cx: &mut TestAppContext) {
    let base = resolved_dir("env-kept");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-kept", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), Some("https://github.com/acme/app.git"));
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let kept = box_dir(&view, cx);
    assert_eq!(
        ferrite_core::placement::placements(&kept),
        [ferrite_core::placement::Placement {
            local_root: source.clone(),
            remote_root: projects.join("app"),
        }]
    );
}

/// A copy stopped part way, by Cancel, is unfinished: the sheet stays up
/// to say so, lists the folder the copy made, adds nothing, and leaves the
/// draft where it was.
/// Closed and opened again, the sheet opens on it, under `~/`, and says
/// what it is; its Resume sends the rest. A file put there in between
/// stays, and one of the Project's paths someone changed there stays as
/// they left it and is listed.
#[gpui::test]
fn a_copy_cancelled_part_way_is_resumed_and_keeps_what_was_put_there_since(cx: &mut TestAppContext) {
    let base = resolved_dir("env-cancel");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-cancel", &projects);
    let on_box = machines.held(1).unwrap();
    std::fs::create_dir_all(projects.join("elsewhere")).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let big = vec![9u8; 4 << 20];
    std::fs::write(source.join("big.bin"), &big).unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");

    copying_part_way(&dest, cx);
    press(cx, "placement-cancel");
    BOX_SLOW.store(false, Ordering::SeqCst);
    assert!(sheet(&view, cx).is_some(), "Cancel stops the copy, not the sheet");
    assert!(!copy_runs(&view, cx));
    assert_eq!(
        text_of(cx, "placement-cancelled").as_deref(),
        Some("Copy cancelled. Resume finishes it from where it stopped.")
    );
    assert!(debug_bounds(cx, "placement-resume".into()).is_some());
    assert_eq!(
        ferrite_core::placement::unfinished(&box_dir(&view, cx), &source),
        [dest.clone()],
        "what it sent is kept for its Resume"
    );
    browse(cx);
    until(cx, |cx| {
        text_of(cx, "folder-browser-rows").is_some_and(|rows| rows.split(' ').any(|row| row == "app"))
    });
    for _ in 0..4 {
        tick(cx);
    }
    assert_eq!(draft_project(&view, draft, cx), mine, "the draft stays");
    assert!(roots_on(&view, on_box, cx).is_empty(), "nothing registered");

    press(cx, "placement-close");
    assert!(sheet(&view, cx).is_none(), "once stopped, it closes");
    std::fs::write(dest.join("theirs.txt"), "theirs").unwrap();
    std::fs::write(dest.join("main.rs"), "their main").unwrap();
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-unfinished");
    assert_eq!(
        text_of(cx, "placement-unfinished").as_deref(),
        Some("An unfinished copy of app is here.")
    );
    // Under `~/` once box has said where its home is.
    until(cx, |cx| destination(&view, cx).replace('\\', "/") == shown(&dest));
    assert!(debug_bounds(cx, "placement-change".into()).is_some(), "on it, the browser shut");
    assert!(debug_bounds(cx, "placement-use".into()).is_none());
    drawn(cx, "placement-ignored:.env");
    press(cx, "placement-resume");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(debug_bounds(cx, "placement-use".into()).is_none(), "never used as it is");
    drawn(cx, "placement-skipped");
    assert_eq!(
        text_of(cx, "placement-skipped").as_deref(),
        Some("main.rs \u{b7} changed on box")
    );
    assert_eq!(std::fs::read(dest.join("big.bin")).unwrap(), big);
    assert_eq!(std::fs::read_to_string(dest.join("theirs.txt")).unwrap(), "theirs");
    assert_eq!(std::fs::read_to_string(dest.join("main.rs")).unwrap(), "their main");
    assert_eq!(roots_on(&view, on_box, cx), [dest.clone()]);
    assert_eq!(
        ferrite_core::placement::unfinished(&box_dir(&view, cx), &source),
        Vec::<PathBuf>::new(),
        "finished"
    );
}

/// While a copy runs the card shows how far it is, at the compact width,
/// clear of the draft's band, and holds the keys. A press outside the
/// card, esc, the close button and Continue in background each let the
/// card go and the copy go on, followed by a toast and by the draft's
/// chip, a ring and how far; the toast's Show and the chip bring the card
/// back. Only Cancel stops the copy, and its Resume finishes it.
#[gpui::test]
fn the_card_goes_and_the_copy_goes_on_until_cancel(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    toasts_rest(cx);
    let base = resolved_dir("env-held");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-held", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("big.bin"), vec![9u8; 4 << 20]).unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    settles(cx);
    let choosing = card_size(cx);
    let band = debug_bounds(cx, "composer-env".into()).expect("the draft's band");
    let bottom = debug_bounds(cx, "placement-card".into()).unwrap().bottom();
    assert!(bottom < band.top(), "the card ends at {bottom:?}, over the band at {:?}", band.top());

    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    settles(cx);
    let copying = card_size(cx);
    assert!(copying.width < choosing.width, "compact: {copying:?} from {choosing:?}");
    assert!(copying.height < choosing.height, "compact: {copying:?} from {choosing:?}");
    assert!(
        text_of(cx, "placement-route").is_some_and(|said| said.starts_with("app \u{2192} ")),
        "{:?}",
        text_of(cx, "placement-route")
    );
    assert!(debug_bounds(cx, "placement-bar".into()).is_some());
    cx.simulate_input("zz");
    assert_eq!(draft_text(&view, draft, cx), "", "the keys reached no draft");

    let outside = gpui::point(px(8.), px(8.));
    cx.simulate_mouse_down(outside, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    assert!(sheet(&view, cx).is_none(), "a press outside lets the card go");
    assert!(copy_runs(&view, cx), "and the copy go on");
    on_toast(cx, "copy-toast-show");
    assert!(debug_bounds(cx, "notification-progress".into()).is_some(), "the toast follows it");
    drawn(cx, "band-chip-ring");
    assert!(
        text_of(cx, ENVIRONMENT).is_some_and(|said| said.starts_with("box ") && said.ends_with('%')),
        "the chip says how far: {:?}",
        text_of(cx, ENVIRONMENT)
    );

    press(cx, "copy-toast-show");
    assert!(sheet(&view, cx).is_some(), "Show brings the card back");
    drawn(cx, "placement-sent");
    settles(cx);
    assert!(debug_bounds(cx, "copy-toast-show".into()).is_none(), "the card, not the toast, shows it");
    cx.simulate_keystrokes("escape");
    assert!(sheet(&view, cx).is_none() && copy_runs(&view, cx), "esc lets the card go");

    press(cx, ENVIRONMENT);
    assert!(sheet(&view, cx).is_some(), "the chip brings the card back");
    press(cx, "placement-close");
    assert!(sheet(&view, cx).is_none() && copy_runs(&view, cx), "the close button lets it go");

    press(cx, ENVIRONMENT);
    press(cx, "placement-background");
    assert!(sheet(&view, cx).is_none() && copy_runs(&view, cx), "Continue in background lets it go");

    press(cx, ENVIRONMENT);
    press(cx, "placement-cancel");
    BOX_SLOW.store(false, Ordering::SeqCst);
    assert!(!copy_runs(&view, cx), "Cancel stops it");
    assert_eq!(
        text_of(cx, "placement-cancelled").as_deref(),
        Some("Copy cancelled. Resume finishes it from where it stopped.")
    );
    assert_eq!(draft_project(&view, draft, cx), mine, "the draft stays");
    // Back on choosing, the card reads the Project again before Resume.
    drawn(cx, "placement-ignored:.env");
    press(cx, "placement-resume");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(dest.join("big.bin").is_file());
}

/// The card is as tall as what it shows: a Project with nothing git
/// ignores draws a shorter card than one with a list. Change… opens box's
/// folders in it, the list as tall as the folder's rows: an almost empty
/// folder leaves no well under them, and one of many lists eight before
/// it scrolls.
#[gpui::test]
fn the_card_is_as_tall_as_what_it_shows(cx: &mut TestAppContext) {
    let base = resolved_dir("env-one-size");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-one-size", &projects);
    for at in 0..20 {
        std::fs::create_dir_all(projects.join(format!("many/f{at:02}"))).unwrap();
    }
    std::fs::create_dir_all(projects.join("few/only")).unwrap();
    let listing = project_folder(&base.join("here/app"), None);
    let bare = base.join("here/bare");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(bare.join("notes.txt"), "notes").unwrap();
    let listing = registered_here(&mut core, &listing);
    let bare = registered_here(&mut core, &bare);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(listing, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    settles(cx);
    let with_list = card_size(cx);
    press(cx, "placement-close");

    view.update(cx, |view, cx| view.open_draft_in_project(bare, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-copy".into()).is_some()
            && debug_bounds(cx, "placement-listing".into()).is_none()
    });
    settles(cx);
    let without = card_size(cx);
    assert_eq!(without.width, with_list.width);
    assert!(without.height < with_list.height, "{without:?} under {with_list:?}");
    assert!(debug_bounds(cx, "placement-ignored-all".into()).is_none(), "no list at all");

    browse(cx);
    let row = ferrite_ui::theme::FILES_ROW_H;
    let line = sheet_line(&view, cx);
    let folders_in = |folder: &str, cx: &mut gpui::VisualTestContext| {
        line.update(cx, |line, cx| line.set(format!("{}/", projects.join(folder).display()), cx));
        until(cx, |cx| debug_bounds(cx, "folder-browser-reading".into()).is_none());
        settles(cx);
        (debug_bounds(cx, "folder-browser-rows".into()).expect("the folders").size.height, card_size(cx).height)
    };
    let (few, card_on_few) = folders_in("few", cx);
    assert_eq!(few, px(2.0 * row), "`..` and its one folder, no well under them");
    assert!(card_on_few > without.height, "the browser opens in the card");
    let (many, card_on_many) = folders_in("many", cx);
    assert_eq!(many, px(8.0 * row), "eight, the rest scrolling");
    let grew = f32::from(card_on_many - card_on_few);
    assert!((grew - 6.0 * row).abs() < 1.0, "the card as tall as the list: it grew {grew}");
}

/// The tick over what git ignores is all of them, none or some: a press
/// from none or some ticks every one, from all none, and Copy says how
/// much goes.
#[gpui::test]
fn one_tick_takes_all_that_git_ignores_or_none(cx: &mut TestAppContext) {
    let base = resolved_dir("env-tick-all");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-tick-all", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("target/big.bin"), vec![1u8; 4 << 20]).unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        text_of(cx, "placement-ignored-total").is_some_and(|said| said.starts_with("2 items \u{b7} "))
            && text_of(cx, "placement-copy").is_some_and(|said| said.starts_with("Copy ") && said.ends_with('B'))
    });
    assert_eq!(text_of(cx, "placement-ignored-total").as_deref(), Some("2 items \u{b7} 4.2 MB"));
    let copy = |cx: &mut gpui::VisualTestContext| text_of(cx, "placement-copy").unwrap();
    let none = copy(cx);
    assert!(!none.ends_with(" MB"), "what git ignores stays: {none}");
    let mixed = |cx: &mut gpui::VisualTestContext| debug_bounds(cx, "checkbox-dash".into()).is_some();
    assert!(!mixed(cx), "none ticked");

    press(cx, "placement-ignored:.env");
    assert!(mixed(cx), "some ticked");
    press(cx, "placement-ignored-all");
    assert!(!mixed(cx));
    let all = copy(cx);
    assert!(all.ends_with(" MB"), "every one ticked: {all}");
    press(cx, "placement-ignored-all");
    assert_eq!(copy(cx), none, "from all, none");
    press(cx, "placement-ignored-all");
    assert_eq!(copy(cx), all, "from none, all");

    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let dest = projects.join("app");
    assert!(dest.join(".env").is_file() && dest.join("target/big.bin").is_file(), "all went");
}

/// A destination that holds something takes no first copy: the sheet says
/// so and nothing there changes. Another path copies; `Use what is there`
/// registers it as it is.
#[gpui::test]
fn a_destination_that_holds_something_is_never_copied_over(cx: &mut TestAppContext) {
    let base = resolved_dir("env-occupied");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-occupied", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let there = projects.join("app");
    std::fs::create_dir_all(&there).unwrap();
    std::fs::write(there.join("keep.txt"), "mine").unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });

    press(cx, "placement-copy");
    until(cx, |cx| {
        debug_bounds(cx, "placement-occupied".into()).is_some()
    });
    assert!(debug_bounds(cx, "placement-use".into()).is_some());
    let listed = |dir: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(listed(&there), ["keep.txt"], "nothing there is touched");

    aim_sheet_at(&view, &projects.join("other"), cx);
    assert!(
        debug_bounds(cx, "placement-occupied".into()).is_none(),
        "another path is chosen afresh"
    );
    assert!(debug_bounds(cx, "placement-copy".into()).is_some());
    aim_sheet_at(&view, &there, cx);
    press(cx, "placement-copy");
    until(cx, |cx| debug_bounds(cx, "placement-use".into()).is_some());
    press(cx, "placement-use");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert_eq!(roots_on(&view, on_box, cx), [there.clone()]);
    assert_eq!(listed(&there), ["keep.txt"], "used as it is");
}

/// `there` spelled another way: through a link on unix, in other letters
/// where names are the same in either case.
fn another_spelling(there: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let link = there.with_file_name("linked");
        std::os::unix::fs::symlink(there, &link).unwrap();
        link
    }
    #[cfg(not(unix))]
    {
        let leaf = there.file_name().unwrap().to_string_lossy().to_uppercase();
        there.with_file_name(leaf)
    }
}

/// `Use what is there` on a Project box already has, its folder spelled
/// another way, chooses that Project as it is: it keeps its own name.
#[gpui::test]
fn a_project_used_as_it_is_keeps_its_name(cx: &mut TestAppContext) {
    let base = resolved_dir("env-use-named");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-use-named", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/mine"), None);
    let there = projects.join("theirs");
    std::fs::create_dir_all(&there).unwrap();
    std::fs::write(there.join("keep.txt"), "theirs").unwrap();
    let theirs = registered(&mut core, on_box, &there);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    aim_sheet_at(&view, &another_spelling(&there), cx);
    press(cx, "placement-copy");
    until(cx, |cx| debug_bounds(cx, "placement-use".into()).is_some());
    press(cx, "placement-use");
    until(cx, |cx| draft_project(&view, draft, cx) == theirs);
    for _ in 0..8 {
        tick(cx);
    }
    let title = view.read_with(cx, |view, cx| view.cockpit.read(cx).project(theirs).unwrap().title.clone());
    assert_eq!(title, "theirs");
}

/// A file another program holds while the copy runs is left behind. The
/// card gone, the rest is chosen on the draft and the toast says how many
/// were left, its Show the card that lists them.
#[gpui::test]
fn a_file_in_use_while_copying_is_left_behind_and_listed(cx: &mut TestAppContext) {
    toasts_rest(cx);
    let base = resolved_dir("env-busy");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-busy", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("big.bin"), vec![9u8; 4 << 20]).unwrap();
    let busy = source.join("server.log");
    std::fs::write(&busy, "running").unwrap();
    #[cfg(windows)]
    let _held = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&busy)
            .unwrap()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&busy, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&busy).is_ok() {
            // Root reads it anyway: nothing here is busy.
            return;
        }
    }
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });
    copying_part_way(&projects.join("app"), cx);
    press(cx, "placement-background");
    BOX_SLOW.store(false, Ordering::SeqCst);
    until(cx, |cx| text_of(cx, "notification-title").is_some_and(|said| said.contains("Copied to box")));
    assert_eq!(text_of(cx, "notification-message").as_deref(), Some("app \u{b7} 1 item not copied"));
    assert_eq!(
        draft_project(&view, draft, cx).machine(),
        on_box,
        "the rest is chosen"
    );
    assert!(projects.join("app/main.rs").is_file());
    on_toast(cx, "copy-toast-show");
    press(cx, "copy-toast-show");
    assert_eq!(
        text_of(cx, "placement-skipped").as_deref(),
        Some("server.log \u{b7} in use here")
    );
    press(cx, "placement-done");
    assert!(sheet(&view, cx).is_none());
}

/// A copy box cannot take says why in the operator's words, and once box
/// is back its retry copies.
#[gpui::test]
fn a_copy_that_fails_says_why_and_its_retry_copies(cx: &mut TestAppContext) {
    let base = resolved_dir("env-failed");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-failed", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });

    box_down(true);
    until(cx, |cx| {
        view.read_with(cx, |view, cx| !attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "placement-copy");
    until(cx, |cx| {
        debug_bounds(cx, "placement-failed".into()).is_some()
    });
    assert_eq!(
        text_of(cx, "placement-failed").as_deref(),
        Some("Can't reach box. Try again once it is back.")
    );
    assert_eq!(draft_project(&view, draft, cx), mine);

    box_down(false);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.retry_server(on_box))
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "placement-retry");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(projects.join("app/main.rs").is_file());
}

/// A copy that stops before it lands anything leaves no unfinished copy:
/// the sheet says what it could not write and why, in plain words, and
/// offers to try again, not to resume; opened again, it claims no
/// unfinished copy either.
#[gpui::test]
fn a_copy_that_lands_nothing_leaves_no_unfinished_copy(cx: &mut TestAppContext) {
    let base = resolved_dir("env-landed-nothing");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-landed-nothing", &projects);
    // A file where the copy's folder must go: nothing can be made there.
    std::fs::write(projects.join("blocker"), "a file").unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let name = sheet(&view, cx).unwrap().read_with(cx, |sheet, _| sheet.name.clone());
    name.update(cx, |name, cx| name.set("blocker/app".into(), cx));
    tick(cx);
    press(cx, "placement-copy");
    drawn(cx, "placement-failed");
    let said = text_of(cx, "placement-failed").unwrap();
    assert!(said.starts_with("Can't write ") && said.contains("blocker"), "{said}");
    assert!(!said.contains("os error") && said.ends_with('.'), "{said}");
    assert!(debug_bounds(cx, "placement-retry".into()).is_some(), "tried again, not resumed");
    assert!(debug_bounds(cx, "placement-resume".into()).is_none());
    let kept = box_dir(&view, cx);
    assert_eq!(ferrite_core::placement::unfinished(&kept, &source), Vec::<PathBuf>::new());
    assert_eq!(draft_project(&view, draft, cx), mine);

    press(cx, "placement-close");
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    settles(cx);
    assert!(debug_bounds(cx, "placement-unfinished".into()).is_none(), "no unfinished copy is claimed");
    assert_eq!(destination(&view, cx).replace('\\', "/"), format!("{}/app", projects.display()).replace('\\', "/"));
}

/// A destination holding another Project's unfinished copy is that one's:
/// the sheet says whose, offers nothing there to use as it is, and nothing
/// there changes.
#[gpui::test]
fn another_projects_unfinished_copy_is_left_to_it(cx: &mut TestAppContext) {
    let base = resolved_dir("env-another-copy");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-another-copy", &projects);
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("big.bin"), vec![9u8; 4 << 20]).unwrap();
    let other = project_folder(&base.join("here/web"), None);
    let app = registered_here(&mut core, &source);
    let web = registered_here(&mut core, &other);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(app, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    press(cx, "placement-cancel");
    BOX_SLOW.store(false, Ordering::SeqCst);
    press(cx, "placement-close");
    // The write the cancel cut off is dropped there once its connection ends.
    let part = |path: &PathBuf| path.extension().is_some_and(|ext| ext == "part");
    until(cx, |_| !files_in(&dest).iter().any(|(path, _)| part(path)));
    let held = files_in(&dest);

    view.update(cx, |view, cx| view.open_draft_in_project(web, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    aim_sheet_at(&view, &dest, cx);
    press(cx, "placement-copy");
    drawn(cx, "placement-occupied");
    assert_eq!(
        text_of(cx, "placement-occupied").as_deref(),
        Some("app holds an unfinished copy of app. Nothing there is changed.")
    );
    assert!(debug_bounds(cx, "placement-use".into()).is_none(), "not offered as it is");
    assert_eq!(files_in(&dest), held, "nothing there changed");
    let kept = box_dir(&view, cx);
    assert_eq!(ferrite_core::placement::unfinished(&kept, &source), [dest.clone()], "still app's");
}

/// box lost part way through a copy: the sheet says so in the operator's
/// words, whole, and Resume finishes the copy once box is back.
#[gpui::test]
fn a_machine_lost_part_way_is_said_so_and_resume_finishes_once_it_is_back(cx: &mut TestAppContext) {
    let base = resolved_dir("env-lost");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-lost", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let big = vec![5u8; 4 << 20];
    std::fs::write(source.join("big.bin"), &big).unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    box_down(true);
    BOX_SLOW.store(false, Ordering::SeqCst);
    drawn(cx, "placement-failed");
    let said = "The connection to box was lost. Resume finishes the copy once it is back.";
    assert_eq!(text_of(cx, "placement-failed").as_deref(), Some(said));
    let line = debug_bounds(cx, "placement-failed".into()).unwrap();
    let card = debug_bounds(cx, "placement-card".into()).unwrap();
    assert!(line.right() <= card.right(), "said inside the card");
    let slot = debug_bounds(cx, "placement-stage".into()).unwrap();
    assert!(slot.size.height <= px(2.0 * ferrite_ui::theme::LH_UI), "in two lines at most");

    box_down(false);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.retry_server(on_box))
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "placement-resume");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert_eq!(std::fs::read(dest.join("big.bin")).unwrap(), big);
}

/// A worktree inside the Project stays here, and the sheet says so; a
/// Project that is itself a linked worktree is refused, the sheet naming
/// its main checkout by its folder.
#[gpui::test]
fn worktrees_are_never_copied(cx: &mut TestAppContext) {
    let base = resolved_dir("env-worktrees");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-worktrees", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join(".gitignore"), "target/\n.env\n.worktrees/\n").unwrap();
    git(&source, &["worktree", "add", "-q", "-b", "feature", ".worktrees/feature"]);
    let beside = base.join("here/app-beside");
    git(&source, &["worktree", "add", "-q", "-b", "beside", &beside.display().to_string()]);
    let mine = registered_here(&mut core, &source);
    let beside = registered_here(&mut core, &beside);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-worktrees");
    settles(cx);
    assert_eq!(
        text_of(cx, "placement-worktrees").as_deref(),
        Some("Worktrees are not copied.")
    );
    assert!(debug_bounds(cx, "placement-ignored:.worktrees".into()).is_none());
    assert!(debug_bounds(cx, "placement-ignored:.env".into()).is_some());
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(projects.join("app/main.rs").is_file());
    assert!(!projects.join("app/.worktrees").exists());
    drawn(cx, "placement-done");
    press(cx, "placement-done");

    view.update(cx, |view, cx| view.open_draft_in_project(beside, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-refusal");
    assert_eq!(
        text_of(cx, "placement-refusal").as_deref(),
        Some(format!(
            "This folder is a git worktree. Copy its main checkout, {}, instead.",
            dunce::canonicalize(&source).unwrap().display()
        )
        .as_str())
    );
    press(cx, "placement-copy");
    for _ in 0..4 {
        tick(cx);
    }
    assert!(!projects.join("app-beside").exists(), "nothing is copied");
}

/// Hundreds of entries git ignores list the first of them and one row for
/// the rest, which ticks them all at once.
#[gpui::test]
fn hundreds_of_ignored_entries_end_in_one_row_for_the_rest(cx: &mut TestAppContext) {
    let base = resolved_dir("env-ignored-many");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-ignored-many", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join(".gitignore"), "target/\n.env\n*.log\n").unwrap();
    for at in 0..300 {
        std::fs::write(source.join(format!("{at:03}.log")), "log").unwrap();
    }
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored-more");
    assert!(debug_bounds(cx, "placement-ignored:000.log".into()).is_some());
    assert!(debug_bounds(cx, "placement-ignored:299.log".into()).is_none());
    reveal(cx, "placement-body", "placement-ignored-more");
    assert!(
        text_of(cx, "placement-ignored-more").is_some_and(|said| said.starts_with("and 283 more")),
        "{:?}",
        text_of(cx, "placement-ignored-more")
    );
    press(cx, "placement-ignored-more");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let dest = projects.join("app");
    assert!(dest.join("299.log").is_file(), "the rest went, ticked as one");
    assert!(!dest.join("000.log").exists(), "the first are ticked each by itself");
}

/// A copy that finishes with its card and its toast closed still tells the
/// operator: a toast of what it did, the chime, and with the window in the
/// background the system's notification, whose click shows the card on
/// what it did.
#[gpui::test]
fn a_copy_finishing_unwatched_still_says_so(cx: &mut TestAppContext) {
    toasts_rest(cx);
    let base = resolved_dir("env-unwatched");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-unwatched", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("big.bin"), vec![9u8; 4 << 20]).unwrap();
    let mine = registered_here(&mut core, &source);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    press(cx, "placement-background");
    on_toast(cx, "copy-toast-show");
    close_toast(cx);
    assert!(debug_bounds(cx, "copy-toast-show".into()).is_none(), "the toast is closed");
    assert!(copy_runs(&view, cx), "and the copy goes on");

    cx.deactivate_window();
    BOX_SLOW.store(false, Ordering::SeqCst);
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    on_toast(cx, "notification-title");
    assert!(
        text_of(cx, "notification-title").is_some_and(|said| said.contains("Copied to box")),
        "{:?}",
        text_of(cx, "notification-title")
    );
    assert!(debug_bounds(cx, "copy-toast-chat".into()).is_none(), "the draft is there already");
    view.read_with(cx, |view, _| assert!(view.chimed.contains(&Sound::Done), "{:?}", view.chimed));
    let tag = copy_tag(&view, cx);
    assert!(
        cx.delivered_system_notifications()
            .iter()
            .any(|notification| notification.tag.as_ref() == tag),
        "the window away, the system says so too"
    );

    view.update(cx, |view, cx| view.open_system_notification(&tag, cx));
    tick(cx);
    assert!(sheet(&view, cx).is_some(), "its click shows the card");
    drawn(cx, "placement-copied");
    press(cx, "placement-done");
    assert!(sheet(&view, cx).is_none());
    assert!(!text_of(cx, "notification-title").is_some_and(|said| said.contains("Copied to box")), "seen");
}

/// A copy that fails with its card closed turns its toast into why, with
/// Show and Resume. Its draft gone to another Project meanwhile stays
/// there: once the Resume finishes, the toast offers a chat on the copy.
#[gpui::test]
fn a_copy_failing_unwatched_says_why_and_resumes_from_its_toast(cx: &mut TestAppContext) {
    toasts_rest(cx);
    let base = resolved_dir("env-failed-away");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-failed-away", &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let big = vec![5u8; 4 << 20];
    std::fs::write(source.join("big.bin"), &big).unwrap();
    let other = project_folder(&base.join("here/web"), None);
    let mine = registered_here(&mut core, &source);
    let web = registered_here(&mut core, &other);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    press(cx, "placement-background");
    on_toast(cx, "copy-toast-show");

    box_down(true);
    BOX_SLOW.store(false, Ordering::SeqCst);
    on_toast(cx, "copy-toast-again");
    assert!(text_of(cx, "notification-title").is_some_and(|said| said.contains("Not copied to box")));
    assert!(
        text_of(cx, "notification-message").is_some_and(|said| said.contains("The connection to box was lost.")),
        "the plain reason: {:?}",
        text_of(cx, "notification-message")
    );
    assert!(debug_bounds(cx, "copy-toast-show".into()).is_some());
    assert!(text_of(cx, "copy-toast-again").is_some_and(|said| said == "Resume"));
    view.read_with(cx, |view, _| assert!(view.chimed.contains(&Sound::NeedsYou)));

    view.update(cx, |view, cx| view.rebind_draft(draft, web, cx));
    box_down(false);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.retry_server(on_box))
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "copy-toast-again");
    assert!(sheet(&view, cx).is_none(), "resumed where it is, the card shut");
    until(cx, |cx| roots_on(&view, on_box, cx).contains(&dest));
    assert_eq!(std::fs::read(dest.join("big.bin")).unwrap(), big);
    on_toast(cx, "copy-toast-chat");
    assert_eq!(draft_project(&view, draft, cx), web, "the draft stays where it went");

    // The window keeps one draft up: the chat is that draft, aimed there.
    press(cx, "copy-toast-chat");
    let chat = focused_draft(&view, cx);
    assert_eq!(draft_project(&view, chat, cx).machine(), on_box, "a chat on the copy");
    settles(cx);
    assert!(debug_bounds(cx, "copy-toast-chat".into()).is_none(), "seen");
}
