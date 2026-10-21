//! The caret on a heading that folds: `▾` open, `▸` folded, in the marks' column and the muted ink; the automations'
//! heading, which does not fold, has none and its name starts where the others' do; a narrow list keeps the caret.
use crate::common;
use common::board::open;
use common::*;
use sw::automations::list::{self as l, Add};
use sw::automations::triggers::parse_schedule;
use sw::json::*;
use sw::shown::ink::Ink;

#[test]
fn a_heading_that_folds_has_a_caret_that_says_which_way_one_that_does_not_has_none() {
    let w = world();
    sw::clock::set(Some(sw::clock::now()));
    for n in 1..=2 { w.add(&format!("Task {n}")); }
    l::add(Add { name: "Nightly audit", prompt: "Audit.", trigger: Some(parse_schedule("daily 03:00").unwrap()), repo: "", folder: &w.root, settings: Obj::new(), research: None }).unwrap();
    let mut s = open(&w.root, 30); s.s.side = false; s.load();
    let lines = s.screen(80);
    let at = |lines: &[String], start: &str| lines.iter().position(|l| l.starts_with(start)).unwrap_or_else(|| panic!("{start}\n{}", lines.join("\n")));
    // Open: `▾` in the marks' column, the name where a task's tag starts.
    let todo = at(&lines, " ▾ to do");
    let task = at(&lines, " ○ PRJ-");
    assert_eq!(lines[todo].find("to do"), lines[task].find("PRJ-"), "{}", lines.join("\n"));
    // The automations do not fold: no caret, and the name still starts in the headings' column.
    let autos = at(&lines, "   automations");
    assert!(!lines[autos].contains('▾') && !lines[autos].contains('▸'));
    // In the muted ink, the name in its own.
    let inked = s.inked(80);
    let caret = inked[todo].0.iter().find(|p| p.t.contains('▾')).map(|p| p.c);
    assert_eq!(caret, Some(Ink::Muted));
    // Folded: `▸`, and how many tasks it holds.
    for _ in 0..5 { if s.s.sel.starts_with("head:") { break; } s.press(&["up"]); }
    s.press(&["enter"]);
    let lines = s.screen(80);
    assert!(lines.iter().any(|l| l == "▌▸ to do  2 tasks"), "{}", lines.join("\n"));
    assert!(!lines.iter().any(|l| l.contains('▾')), "{}", lines.join("\n"));
    // Narrow: the caret stays, the line never runs past the edge.
    for width in [12, 9] {
        let narrow = s.screen(width);
        let line = narrow.iter().find(|l| l.contains('▸')).unwrap_or_else(|| panic!("{}", narrow.join("\n")));
        assert!(line.starts_with("▌▸ to do") && line.chars().count() <= width, "{line:?}");
    }
    s.press(&["enter"]);
    let narrow = s.screen(12);
    assert!(narrow.iter().any(|l| l.starts_with("▌▾ to do")), "{}", narrow.join("\n"));
}
