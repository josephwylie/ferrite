//! A title's length limit: one number, the same for a task typed, added or renamed. A longer title is shortened at a
//! word and its full text is kept in the description; the command line says so to whoever gave it.
use crate::common;
use common::*;
use sw::json::*;
use sw::store::titles::{short_title, titled, TITLE_MAX};
use sw::text::width;

const LONG: &str = "After the startup dialog handoff, finish jobs wrongly mark the untouched task as stopped without a summary.";
const SHORT: &str = "After the startup dialog handoff, finish jobs wrongly mark…";
fn len(s: &str) -> usize { s.chars().count() }
fn st(s: &str) -> String { short_title(s, TITLE_MAX) }

#[test]
fn a_title_that_fits_is_kept_a_longer_one_is_cut_at_a_word_and_ends_in_an_ellipsis() {
    assert_eq!(TITLE_MAX, 60);
    assert_eq!(st("Add a column"), "Add a column");
    let exact = "x".repeat(60); assert_eq!(st(&exact), exact);
    assert_eq!(st(LONG), SHORT); assert!(len(SHORT) <= 60);
    // Only the full stop was too much: it goes, and nothing is marked as cut.
    assert_eq!(st(&format!("{exact}.")), exact); assert!(!titled(&format!("{exact}."), "").cut);
    // One very long word has no word boundary to cut at: it is cut where the room ends.
    assert_eq!(st(&"y".repeat(90)), format!("{}…", "y".repeat(59)));
    // The cut never leaves a comma or a dash hanging, and characters are counted, not bytes.
    assert_eq!(st(&format!("{}cd, - efgh ijkl", "ab ".repeat(18))), format!("{}cd…", "ab ".repeat(18)));
    assert_eq!(len(&st(&"é".repeat(80))), 60);
    // Room on the screen is what is limited: a wide character takes two cells, so 30 of them fill a title.
    assert_eq!(st(&"界".repeat(30)), "界".repeat(30)); assert_eq!(st(&"界".repeat(60)), format!("{}…", "界".repeat(29))); assert_eq!(st(&"😀".repeat(80)), format!("{}…", "😀".repeat(29)));
    // The one measure is the board's own (text::width): what JS checked against pi-tui's, each by the cells it takes.
    for (x, cells) in [("界", 2), ("😀", 2), ("☀️", 2), ("⌚", 2), ("🈀", 2), ("👨‍👩‍👧", 2), ("🇸🇪", 2), ("e\u{301}", 1), ("ｆ", 2), ("a", 1)] { assert_eq!(width(x), cells, "{x}"); }
    for x in ["Add a column".to_string(), "界".repeat(60), "😀".repeat(80), "☀️".repeat(60), "©️".repeat(60), "↔️".repeat(60), "⌚".repeat(60), "⚡".repeat(60), "🈀".repeat(60), "👨‍👩‍👧".repeat(40), "🇸🇪".repeat(40), "한국어 제목 ".repeat(20), "e\u{301}".repeat(80), "ｆｕｌｌ ｗｉｄｔｈ ".repeat(9), "日本語のタイトル and English words mixed ".repeat(4)] {
        assert!(width(&st(&x)) <= 60, "{x}");
    }
    assert_eq!(st("two\nlines"), "two lines");
    for n in [61, 75, 120, 300] { assert!(len(&st(&"word ".repeat(n)[..n])) <= 60, "{n}"); }
}

/// The program run in the world's repo: what it printed.
fn board(w: &World, args: &[&str]) -> String {
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_ferrite-projects")).args(args).current_dir(&w.root)
        .env("HERDR_SOCKET_PATH", "/nonexistent").env("HERDR_BIN_PATH", "/bin/false").output().unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn real_programs_add_rename_and_report_title_follow_the_limit_and_the_full_text_stays_in_the_description() {
    let w = world();
    use sw::actions::filing::{add, rename, report, AddOpts, Report};
    let d = |s: &str| AddOpts { description: s.into(), ..Default::default() };
    let one = add(&w.root, LONG, d("It happens on a slow machine.")).unwrap();
    assert_eq!((one.t("title"), one.t("description")), (SHORT.to_string(), format!("{LONG}\n\nIt happens on a slow machine.")));
    // A description that already says it is not given it twice; a title that fits changes nothing.
    assert_eq!(add(&w.root, LONG, d(&format!("Seen today:\n{LONG}"))).unwrap().t("description"), format!("Seen today:\n{LONG}"));
    assert_eq!((add(&w.root, LONG, d("")).unwrap().t("description"), add(&w.root, "Add a column", d("why")).unwrap().t("description")), (LONG.to_string(), "why".to_string()));
    // Renamed: the same rule, and what the task already said is kept under the new full text.
    let plain = add(&w.root, "Old name", d("Details.")).unwrap();
    let id = plain.t("id");
    assert_eq!((rename(&id, "New name").unwrap().t("title"), task(&id).t("description")), ("New name".to_string(), "Details.".to_string()));
    let again = rename(&id, LONG).unwrap();
    assert_eq!((again.t("title"), again.t("description")), (SHORT.to_string(), format!("{LONG}\n\nDetails.")));
    assert_eq!(rename(&id, "  ").unwrap().t("title"), SHORT); // an empty title changes nothing
    let told = report(&id, Report { title: "Renamed while it is marked as blocked ".repeat(3), state: "blocked".into(), msg: "why".into(), ..Default::default() }).unwrap();
    assert!(len(told.s("title")) <= 60); assert_eq!(told.s("title"), "Renamed while it is marked as blocked Renamed while it is…");
    assert!(regex::Regex::new(r"^Renamed while it is marked as blocked Renamed .*\n\n").unwrap().is_match(told.s("description")), "{}", told.s("description"));
    // The command line says so, with the number, to whoever gave the long title; a title that fits says nothing.
    let root = w.root_str();
    assert!(board(&w, &["add", LONG, "--repo", &root]).contains("added The title was over 60 characters, so it was shortened to \"After the startup dialog handoff, finish jobs wrongly mark…\" and the full text was kept in the description."));
    assert!(!board(&w, &["add", "A short one", "--repo", &root]).contains("60 characters"));
    assert!(regex::Regex::new(r"updated \(title\) The title was over 60 characters").unwrap().is_match(&board(&w, &["report", &id, "--title", LONG])));
    assert!(board(&w, &["report", "--title", LONG, "--repo", &root]).contains("added The title was over 60 characters"));
    assert!(sw::store::tasks::list(Default::default()).iter().all(|t| len(t.s("title")) <= 60));
    assert!(board(&w, &["help"]).contains("a title is at most 60 characters"));
}
