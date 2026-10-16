//! The board stands still: a size that changes for a moment and is back does not move it (seen on a real board: 141x46
//! to 138x44 and back within 0.6 s, and to 47x44 when a phone-sized herdr window took the session); what herdr said
//! about another size, or says only once, is not drawn for; and a second herdr window on the session has its terminal
//! taken twice, after which the board says what frees the size. The board is opened as the program opens it: a size
//! has to last a second before it is laid out for.
use crate::common;
use common::board::{open, Board};
use common::*;
use sw::board::keys::key_name;
use sw::board::state::Held;
use sw::board::update::size::Shown;
use sw::board::update::{list::relist, update, Effect, Event};
use sw::json::*;

fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }
/// An open board in a terminal of `term`: `at` is what a window resize does (the terminal has another size, and the
/// board is drawn for it); `asked` counts the times herdr was asked what it shows; `took` the terminal taken back.
struct Still { b: Board, term: (usize, usize), asked: usize, took: Vec<(u16, u16)> }
fn still(w: &World, cols: usize, rows: usize) -> Still {
    let mut b = open(&w.root, rows);
    b.s.ctx.settle_ms = 1000; b.s.ctx.pane = "w1:p1".into();
    // "5h ago" on every row, however far a test moves the clock.
    let ago = sw::clock::now() - 5 * 3600000;
    for r in b.s.data.items.iter_mut() { std::sync::Arc::make_mut(r).put("at", ago); }
    relist(&mut b.s);
    let mut s = Still { b, term: (cols, rows), asked: 0, took: vec![] };
    s.at(cols, rows);
    s
}
impl Still {
    fn at(&mut self, cols: usize, rows: usize) -> Vec<String> { self.term = (cols, rows); self.b.rows = rows; let l = self.b.screen(cols); if std::mem::take(&mut self.b.s.size.ask) { self.asked += 1; } l }
    fn wait(&mut self, ms: i64) { sw::clock::advance(ms); }
    /// herdr's answer about what it shows of the pane, for the terminal as it is now; a take it leads to works.
    fn see(&mut self, cols: u16, rows: u16) {
        let fx = update(&mut self.b.s, Event::Shown { term: (self.term.0 as u16, self.term.1 as u16), shown: Some(Shown { cols, rows, split: false }), by: None });
        for e in fx { if let Effect::Take { cols, rows } = e { self.took.push((cols, rows)); update(&mut self.b.s, Event::Took(Ok(()))); } }
    }
}
/// The screen as it was, in a terminal of another size: the same lines, cut on the right and from the top.
fn cut(lines: &[String], cols: usize, rows: usize) -> Vec<String> { lines[lines.len().saturating_sub(rows)..].iter().map(|l| l.chars().take(cols).collect::<String>().trim_end().to_string()).collect() }
/// The list on the left, the task panel on the right.
fn side_panel(lines: &[String]) -> bool { lines.iter().filter(|l| has(l, r"\S.* │ ")).count() > 10 }
fn squash(lines: &[String]) -> String { regex::Regex::new(r"\s+").unwrap().replace_all(&lines.join(" "), " ").to_string() }

#[test]
fn a_size_that_changes_for_a_moment_and_is_back_does_not_move_the_board() {
    let w = world();
    for i in 1..=30 { w.add(&format!("Task number {i}")); }
    sw::clock::set(Some(sw::clock::now()));
    let mut s = still(&w, 141, 46);
    let wide = s.at(141, 46);
    assert!(side_panel(&wide)); assert_eq!(wide.len(), 46);
    // Another window's size for a moment: the screen is the one it was, cut to the room there is. The text box and
    // its hints, at the bottom, stay.
    for (cols, rows, ms) in [(138, 44, 600), (140, 46, 200), (47, 44, 900), (200, 60, 500)] {
        let note = format!("{cols}x{rows}");
        let during = s.at(cols, rows);
        let n = rows.min(46);
        assert_eq!(during.len(), rows, "{note}");
        assert_eq!(during[..n].to_vec(), cut(&wide, cols, rows), "{note}");
        assert!(has(&during[n - 1], r"^[ ▌] \S"), "{note}: the hints are on its last drawn row");
        s.wait(ms / 2); assert_eq!(s.at(cols, rows)[..n].to_vec(), cut(&wide, cols, rows), "{note}, still");
        s.wait(ms / 2);
        assert_eq!(s.at(141, 46), wide, "{note}: back, and nothing has moved");
        s.wait(3000);
    }
    // A size that lasts is the board's size after a second: laid out for it, as a board opened at that size is.
    assert_eq!(s.at(47, 44), cut(&wide, 47, 44));
    s.wait(1000);
    let narrow = s.at(47, 44);
    assert!(!side_panel(&narrow)); assert_eq!(narrow.len(), 44); assert!(has(&narrow[43], r"^[ ▌] \S"));
    for l in &narrow { assert!(sw::text::width(l) <= 47); }
    assert_ne!(narrow, cut(&wide, 47, 44));
    {
        let mut fresh = open(&w.root, 44);
        fresh.s.sel = s.b.s.sel.clone();
        let ago = sw::clock::now() - 5 * 3600000;
        for r in fresh.s.data.items.iter_mut() { std::sync::Arc::make_mut(r).put("at", ago); }
        relist(&mut fresh.s);
        assert_eq!(narrow, fresh.screen(47));
    }
    // Right after a size has lasted, a short change is held like any other (the reviewer's case: 141 to 120 columns
    // for good, then 47x44 for a fifth of a second): the side panel does not go and come back.
    s.at(120, 46); s.wait(1150);
    let mid = s.at(120, 46);
    assert!(side_panel(&mid)); for l in &mid { assert!(sw::text::width(l) <= 120); }
    assert_eq!(s.at(47, 44), cut(&mid, 47, 44)); s.wait(200);
    assert_eq!(s.at(120, 46), mid);
    // A window that is being dragged: the screen as it was, cut, and laid out again once a second, never per step.
    let mut laid = mid.clone();
    for cols in [118, 116, 114, 112, 110, 108, 106, 104] {
        s.wait(300);
        let l = s.at(cols, 46);
        if l != cut(&laid, cols, 46) { laid = l.clone(); assert!(side_panel(&l), "{cols}"); for x in &l { assert!(sw::text::width(x) <= cols); } }
    }
    assert_ne!(laid, mid, "it did follow");
    s.wait(1000); s.at(141, 46); s.wait(1000);
    assert_eq!(s.at(141, 46), wide);
    // Without the wait (the tests' boards, and the web board) a size is the board's at once.
    let mut plain = open(&w.root, 46); plain.screen(141); assert!(!side_panel(&plain.screen(47)));
}

#[test]
fn what_herdr_said_about_another_size_or_says_once_is_not_drawn_for() {
    let w = world();
    for i in 1..=30 { w.add(&format!("Task number {i}")); }
    sw::clock::set(Some(sw::clock::now()));
    let mut s = still(&w, 141, 46);
    s.see(141, 46);
    let wide = s.at(141, 46);
    // The size changes: herdr is asked at once, and what it said for the old size is put aside.
    let asked = s.asked;
    s.at(47, 44); assert_eq!(s.asked, asked + 1);
    s.see(47, 44); // herdr's answer for the moment the terminal was that small
    let asked = s.asked;
    assert_eq!(s.at(141, 46), wide); assert_eq!(s.asked, asked + 1); // back: its own size, at once, and herdr is asked again
    // The answer from during that moment comes in late: it disagrees with the terminal, once. Nothing is drawn for it,
    // nothing is said, nobody is disconnected.
    s.see(47, 44);
    assert_eq!(s.at(141, 46), wide); assert!(!wide.join("\n").contains("Another viewer")); assert_eq!(s.took, vec![]);
    s.see(141, 46); s.wait(2000); s.see(141, 46);
    assert_eq!(s.at(141, 46), wide); assert_eq!(s.took, vec![]);
    // The same said again two seconds on is how things are: the board draws inside what herdr shows, and says so.
    s.see(47, 44); s.wait(2000); s.see(47, 44);
    let inside = s.at(141, 46);
    assert!(squash(&inside).contains("Another viewer holds this terminal at 141×46; herdr shows it at 47×44."), "{}", inside.join("\n"));
    for l in &inside { assert!(sw::text::width(l) <= 46, "{l}"); }
}

// Seen in herdr's own log of a real session: the board took its terminal 179 times in twelve minutes, every four
// seconds, while a second herdr window (47x44, then 138x44) was on the session. Taking the terminal disconnects a
// viewer attached to that one terminal; a second herdr window is not one, so nothing changed and it went on.
#[test]
fn a_second_herdr_window_on_the_session_the_board_takes_its_terminal_twice_then_says_what_frees_the_size() {
    let w = world();
    w.add("A task");
    sw::clock::set(Some(sw::clock::now()));
    let mut s = still(&w, 54, 24);
    let look = |s: &mut Still| { s.wait(2000); s.see(142, 47); }; // the board asks every two seconds
    look(&mut s); assert_eq!(s.took, vec![]); // once is a size change under way
    look(&mut s); assert_eq!(s.took, vec![(54, 24)]);
    assert!(has(&s.at(54, 24).join("\n"), r"Another viewer holds this terminal at 54×24; herdr\s+shows it at 142×47\."), "{}", s.at(54, 24).join("\n"));
    for _ in 0..30 { look(&mut s); }
    assert_eq!(s.took.len(), 2, "a minute on: not once more");
    let text = squash(&s.at(54, 24));
    assert!(text.contains("herdr shows it at 142×47. A key typed in this window takes it back: another he"), "three lines of a narrow window: what frees it is in them: {text}");
    {
        let mut room = still(&w, 100, 24);
        for _ in 0..6 { room.wait(2000); room.see(142, 47); }
        let t = squash(&room.at(100, 24));
        assert!(t.contains("A key typed in this window takes it back: another herdr window (a second device, a browser tab) has this session at its own size."), "{t}");
    }
    assert!(!has(&text, r"trying again|turned on, takes it back"));
    // The line at the top (the one a cut-out still shows) says the same, with what frees the size first.
    s.b.s.held = Some(Held { columns: 54, rows: 24, room: (142, 47), by: vec![], known: false });
    assert!(has(&s.at(54, 24)[0], r"^[ ▌]! Shown cut off: another herdr window on this"), "{}", s.at(54, 24)[0]);
    assert!(!s.at(54, 24).join("\n").contains("Another viewer holds"), "one note, not two");
    s.b.s.held.as_mut().unwrap().by = vec![(4242, 60)]; // a real attach on the terminal: the line names the key that ends it, never its process
    let top = squash(&s.at(54, 24)[..3]);
    assert!(has(&top, &format!(r"Shown cut off: a remote view holds this pane at 54×24 [^.]*\. {} ends that view", regex::escape(&key_name("free")))), "{top}");
    assert!(!s.at(54, 24).join("\n").contains("kill 4242"));
    s.b.s.held = None;
    // You type in your window and herdr gives it the size: nothing claims to have disconnected anybody.
    s.at(141, 46); s.wait(2000); s.see(141, 46);
    assert!(!has(&s.at(141, 46).join("\n"), "disconnected|Another viewer"));
    // The next time the sizes come apart is a new one: the board tries again, twice at most.
    s.at(54, 24);
    for _ in 0..12 { look(&mut s); }
    assert_eq!(s.took.len(), 4);
}
