//! The board as Ferrite draws it: upstream's view laid out on the fixture
//! rows, then drawn into runs on the cell grid in Ferrite's theme — every
//! part of the screen in its words and its inks — and keystrokes said as
//! the board names keys.

use gpui::{Keystroke, Modifiers, TestAppContext};
use sw_board::board::job::Job;
use sw_board::board::keys::Press;
use sw_board::board::state::{Ask, State, Valid};
use sw_board::board::theme::{greys, picks, Theme, C};
use sw_board::board::update::{list::relist, update, Event};
use sw_board::board::view::screen;
use sw_board::json::*;
use sw_board::shown::ink::{Ink, Inked};

use super::fixture::{self, NOW};
use super::keys::{is_paste, press_of};
use super::render::{self, Run};
use super::{BoardEvent, BoardView};
use crate::theme::{ACCENT, GREEN, PLANE, RED, SELECTION, TEXT, YELLOW};

// ------------------------------------------------------------------ keys

fn stroke(key: &str, typed: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.into(),
        key_char: typed.map(str::to_string),
    }
}

/// What a keystroke should read as: its chord and the character it types.
type Expected = Option<(&'static str, Option<char>)>;

fn chord(press: Option<Press>) -> Option<(String, Option<char>)> {
    press.map(|press| (press.chord, press.ch))
}

/// Every kind of key, as `run/input.rs` says crossterm's: typed characters
/// themselves (capitals as `shift+` their letter), ctrl and alt naming
/// their letter, shift on every other key, ⌘ the window's.
#[test]
fn keystrokes_read_as_the_terminal_board_reads_keys() {
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..none
    };
    let control = Modifiers {
        control: true,
        ..none
    };
    let alt = Modifiers { alt: true, ..none };
    let command = Modifiers {
        platform: true,
        ..none
    };
    let table: [(Keystroke, Expected); 18] = [
        (stroke("a", Some("a"), none), Some(("a", Some('a')))),
        (stroke("y", Some("Y"), shift), Some(("shift+y", Some('Y')))),
        (stroke("n", Some("N"), shift), Some(("shift+n", Some('N')))),
        (stroke("/", Some("?"), shift), Some(("?", Some('?')))),
        (stroke("space", Some(" "), none), Some((" ", Some(' ')))),
        (stroke("c", None, control), Some(("ctrl+c", None))),
        (stroke("j", None, control), Some(("ctrl+j", None))),
        (stroke("space", None, control), Some(("ctrl+space", None))),
        // Option+P composes `π` on macOS: the board reads alt+p.
        (stroke("p", Some("π"), alt), Some(("alt+p", None))),
        (
            stroke("a", Some("Å"), Modifiers { shift: true, ..alt }),
            Some(("alt+shift+a", None)),
        ),
        (stroke("enter", None, none), Some(("enter", None))),
        (stroke("enter", None, shift), Some(("shift+enter", None))),
        (stroke("enter", None, alt), Some(("alt+enter", None))),
        (stroke("tab", None, shift), Some(("shift+tab", None))),
        (stroke("escape", None, none), Some(("escape", None))),
        (stroke("up", None, shift), Some(("shift+up", None))),
        (stroke("f5", None, none), Some(("f5", None))),
        (stroke("k", Some("k"), command), None),
    ];
    for (keystroke, want) in table {
        let want = want.map(|(chord, ch)| (chord.to_string(), ch));
        assert_eq!(chord(press_of(&keystroke)), want, "{keystroke:?}");
    }
}

/// The keys the board's own table names are the ones a keystroke makes:
/// `y`/`n` answer, `Y` is not `y`, ctrl+c closes, every newline key types one.
#[test]
fn keystrokes_press_the_boards_own_keys() {
    use sw_board::board::keys::{is, K};
    let none = Modifiers::default();
    let press = |key: &str, typed: Option<&str>, modifiers| {
        press_of(&stroke(key, typed, modifiers)).expect("a board key")
    };
    let shift = Modifiers {
        shift: true,
        ..none
    };
    let control = Modifiers {
        control: true,
        ..none
    };
    let alt = Modifiers { alt: true, ..none };
    assert!(is(&press("y", Some("y"), none), K::SayYes));
    assert!(
        is(&press("y", Some("Y"), shift), K::SayYes),
        "upper-case y says yes too"
    );
    assert!(is(&press("c", None, control), K::Close));
    assert!(is(&press("/", Some("?"), shift), K::Menu));
    assert!(is(&press("enter", None, shift), K::Newline));
    // The board's newline key is shift+enter; the text box takes ctrl+j and
    // alt+enter as its own editing keys too (`TextBox::handle`).
    for newline in [
        press("enter", None, shift),
        press("j", None, control),
        press("enter", None, alt),
    ] {
        let mut text = sw_board::board::textbox::TextBox::default();
        assert!(text.handle(&newline), "{newline:?}");
        assert_eq!(text.text(), "\n", "{newline:?}");
    }
    assert!(is(&press("p", Some("π"), alt), K::Side));
    assert!(is(&press("k", Some("˚"), alt), K::ChatScrollUp));
}

#[test]
fn command_v_pastes_and_other_command_keys_stay_the_windows() {
    let command = Modifiers {
        platform: true,
        ..Modifiers::default()
    };
    if crate::keymap::PLATFORM == crate::keymap::Platform::Mac {
        assert!(is_paste(&stroke("v", None, command)));
    }
    assert!(press_of(&stroke("v", None, command)).is_none());
}

// ----------------------------------------------------------------- theme

/// Ferrite's colours under the board's rules (`docs/colours.md`): the
/// greys that are read meet 4.5:1 on the reading plane, review is a blue
/// told apart from every status colour, the queue's pink and the
/// automations' purple are their own, and a finished mark keeps 3:1.
#[test]
fn ferrite_theme_holds_the_boards_colour_rules() {
    let theme = render::ferrite_theme(false);
    let pal = &theme.pal;
    let on_plane = |c: C| greys::contrast(c.real().expect("a real colour"), PLANE);
    for grey in [pal.overlay0, pal.subtext0, pal.todo] {
        assert!(
            on_plane(grey) >= greys::FLOOR,
            "{grey:?}: {}",
            on_plane(grey)
        );
    }
    assert!(picks::is_blue(pal.review));
    for beside in picks::REVIEW_BESIDE {
        assert!(
            greys::apart(pal.review, pal.get(beside)) >= picks::REVIEW_APART,
            "review beside {beside}"
        );
    }
    assert!(picks::is_pink(pal.queue));
    assert!(picks::is_purple(pal.auto));
    assert_eq!(
        pal.check, pal.subtext0,
        "the board's check is a plain light grey"
    );
    assert!(on_plane(theme.quiet(Ink::Green)) >= greys::LEAST);
    assert_eq!(
        render::ground(&theme),
        None,
        "the reading plane shows through"
    );
}

/// `board.theme`: `herdr`, `auto` and `dark` are Ferrite's own; a herdr
/// theme's name is that theme, on its own ground.
#[test]
fn board_theme_names_ferrites_colours_or_a_herdr_theme() {
    for own in ["herdr", "auto", "dark", ""] {
        assert_eq!(render::theme_named(own).name, render::FERRITE, "{own}");
    }
    let gruvbox = render::theme_named("gruvbox");
    assert_eq!(gruvbox.name, "gruvbox");
    if !gruvbox.plain {
        assert!(render::ground(&gruvbox).is_some());
    }
}

// ---------------------------------------------------------------- screen

fn board() -> State {
    sw_board::clock::set(Some(NOW));
    fixture::board().state
}

fn draw(state: &mut State, theme: &Theme, width: usize, rows: usize) -> Vec<Vec<Run>> {
    screen(state, theme, width, rows)
        .lines
        .iter()
        .map(|line| render::runs(line, theme))
        .collect()
}

fn text(line: &[Run]) -> String {
    line.iter().map(|run| run.text.as_str()).collect()
}

fn cells(line: &[Run]) -> usize {
    line.iter().map(|run| run.cells).sum()
}

fn run<'a>(line: &'a [Run], words: &str) -> &'a Run {
    line.iter()
        .find(|run| run.text.trim() == words)
        .unwrap_or_else(|| panic!("no run {words:?} in {:?}", text(line)))
}

fn line_with<'a>(lines: &'a [Vec<Run>], words: &str) -> &'a [Run] {
    lines
        .iter()
        .find(|line| text(line).contains(words))
        .unwrap_or_else(|| panic!("no line with {words:?}"))
}

fn colour(theme: &Theme, ink: Ink) -> u32 {
    theme.ink(ink).real().expect("a real colour")
}

/// The list as 1.0 draws it, on the cell grid: the header, the groups by
/// need under their headings, a mark per row in its state's colour, the
/// right-aligned columns, the selected row's gutter and tint; no line runs
/// past the width.
#[test]
fn the_list_draws_its_header_groups_marks_and_columns() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    let lines = draw(&mut state, &theme, 120, 40);
    assert_eq!(lines.len(), 40);
    assert!(lines.iter().all(|line| cells(line) <= 120));
    assert_eq!(
        text(&lines[0]).trim_end(),
        " herdr-projects  proj · 5 need you · 1 ready · 9 working · 2 to do · 1 done"
    );
    let head = run(&lines[0], "herdr-projects");
    assert!(head.bold);
    assert_eq!(head.fg, TEXT);

    let heading = line_with(&lines, "ready to approve");
    assert_eq!(run(heading, "▾").fg, colour(&theme, Ink::Muted));
    assert_eq!(run(heading, "ready to approve").fg, GREEN);
    assert_eq!(run(line_with(&lines, "▾ working"), "working").fg, YELLOW);

    let asks = line_with(&lines, "#9 ");
    let mark = run(asks, "?");
    assert_eq!((mark.fg, mark.bold, mark.cells), (RED, true, 1));
    let review = line_with(&lines, "#8 ");
    assert_eq!(
        run(review, "◑").fg,
        colour(&theme, Ink::Review),
        "the reviewer's blue"
    );
    let ready = line_with(&lines, "#11 ");
    assert_eq!(
        run(ready, "◆").cells,
        1,
        "a fallback glyph has its own cell"
    );
    assert_eq!(run(ready, "R#1").fg, GREEN);
    assert_eq!(run(ready, "PR#60").fg, GREEN);

    let selected = line_with(&lines, "#5 ");
    let gutter = &selected[0];
    assert_eq!((gutter.text.as_str(), gutter.fg), ("▌", ACCENT));
    assert!(
        selected.iter().all(|run| run.bg == Some(SELECTION)),
        "the tint runs to the end"
    );
    assert_eq!(cells(selected), 120);
    assert!(
        text(selected).trim_end().ends_with("1m"),
        "time at the right edge"
    );
    assert!(text(selected).contains("60% 11m left"));
}

/// A row that asks shows its question on the line under it, in the text
/// colour; another live row its news, quieter.
#[test]
fn a_row_that_asks_shows_its_question_under_it() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    let lines = draw(&mut state, &theme, 120, 40);
    let at = lines
        .iter()
        .position(|line| text(line).contains("#9 "))
        .unwrap();
    let question = &lines[at + 1];
    assert!(text(question).contains("Should the old settings file be removed"));
    assert_eq!(question.last().unwrap().fg, TEXT);
    let login = lines
        .iter()
        .position(|line| text(line).contains("#10 "))
        .unwrap();
    assert_eq!(text(&lines[login + 1]).trim(), "Needs a login");
    let drafting = lines
        .iter()
        .position(|line| text(line).contains("#2 "))
        .unwrap();
    assert_eq!(
        run(&lines[drafting + 1], "Drafting").fg,
        colour(&theme, Ink::Soft)
    );
}

/// The text box: its rules, the prompt and its placeholder, the cursor as
/// a reversed cell; a draft in its place; the hint line last, its keys
/// brighter than their words.
#[test]
fn the_text_box_and_the_hint_line() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    let lines = draw(&mut state, &theme, 100, 30);
    let at = lines
        .iter()
        .position(|line| text(line).starts_with("❯"))
        .unwrap();
    assert_eq!(text(&lines[at]).trim_end(), "❯ Add a task");
    assert!(text(&lines[at - 1]).chars().all(|c| c == '─'));
    assert_eq!(lines[at - 1].len(), 1, "a rule is one run");
    let cursor = run(&lines[at], "A");
    assert_eq!(
        (cursor.fg, cursor.bg),
        (PLANE, Some(TEXT)),
        "the cursor cell, reversed"
    );
    let hints = lines.last().unwrap();
    assert_eq!(
        text(hints).trim_end(),
        "  enter open this task's agent · space talk to the agent · ctrl+x stop the agent · ? all keys"
    );
    assert_eq!(run(hints, "enter").fg, colour(&theme, Ink::Soft));
    assert_eq!(
        run(hints, "open this task's agent").fg,
        colour(&theme, Ink::Muted)
    );

    for c in "Hello".chars() {
        update(&mut state, Event::Key(Press::char(c)));
    }
    state.now = NOW;
    let lines = draw(&mut state, &theme, 100, 30);
    assert!(lines.iter().any(|line| text(line).trim_end() == "❯ Hello"));
    assert_eq!(
        text(lines.last().unwrap()).trim_end(),
        "  enter add it as a task · tab add it and put an agent on it · shift+enter new line · esc put away"
    );
}

/// A question replaces the text box: its mark, its words, its keys, and no
/// hint line under it.
#[test]
fn a_question_replaces_the_text_box() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    state.ask = Some(Ask {
        q: "Stop the agent on #5?".into(),
        yes: Job::Nothing,
        no: None,
        valid: Valid::Always,
        keys: vec![("sayYes", "yes".into()), ("sayNo", "no".into())],
        detail: String::new(),
        danger: true,
        nudge: false,
    });
    let lines = draw(&mut state, &theme, 100, 30);
    let ask = &lines[lines.len() - 2];
    assert_eq!(
        text(ask).trim_end(),
        " ? Stop the agent on #5?  y = yes · n = no"
    );
    assert_eq!(run(ask, "?").fg, YELLOW);
    assert!(run(ask, "Stop the agent on #5?").bold);
    assert!(
        text(lines.last().unwrap()).chars().all(|c| c == '─'),
        "the box ends the screen"
    );
    assert!(!lines.iter().any(|line| text(line).starts_with("❯")));
}

/// An error says so in red until esc dismisses it, and says that.
#[test]
fn an_error_stays_until_esc_dismisses_it() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    state.flash("Something broke", Ink::Red);
    let lines = draw(&mut state, &theme, 100, 30);
    let said = line_with(&lines, "Something broke");
    assert_eq!(text(said).trim_end(), "  Something broke  (esc dismisses)");
    assert_eq!(run(said, "Something broke").fg, RED);
    assert_eq!(run(said, "(esc dismisses)").fg, colour(&theme, Ink::Muted));
}

/// Words on their way to another machine are rows of their own, above the
/// box: their mark, their words, why one failed.
#[test]
fn words_for_another_machine_are_rows_of_their_own() {
    let theme = render::ferrite_theme(false);
    let mut state = board();
    state.jobs = vec![
        obj(serde_json::json!({ "id": "j1", "text": "@box fix the build", "state": "sending" })),
        obj(
            serde_json::json!({ "id": "j2", "text": "@box tidy", "state": "error", "msg": "box did not answer" }),
        ),
    ];
    let lines = draw(&mut state, &theme, 100, 30);
    assert_eq!(run(line_with(&lines, "fix the build"), "◐").fg, YELLOW);
    let failed = line_with(&lines, "@box tidy");
    assert_eq!(text(failed).trim_end(), " ✗ @box tidy  box did not answer");
    assert_eq!(run(failed, "✗").fg, RED);
}

/// With colour off nothing is painted: the text colour on the plane, and
/// the selected row in reverse.
#[test]
fn no_color_draws_marks_words_and_reverse_only() {
    let theme = render::ferrite_theme(true);
    let mut state = board();
    let lines = draw(&mut state, &theme, 120, 40);
    for line in &lines {
        for run in line {
            assert!(run.fg == TEXT || run.fg == PLANE, "{run:?}");
        }
    }
    let selected = line_with(&lines, "#5 ");
    assert_eq!((selected[0].fg, selected[0].bg), (PLANE, Some(TEXT)));
    assert_eq!(run(line_with(&lines, "#9 "), "?").bg, None);
}

/// herdr's dots: someone at work is a dot, as a moon is with symbols.
#[test]
fn dots_mode_draws_herdrs_dots() {
    let mut theme = render::ferrite_theme(false);
    theme.symbols = false;
    let mut state = board();
    let lines = draw(&mut state, &theme, 120, 40);
    assert_eq!(run(line_with(&lines, "#2 "), "●").fg, YELLOW);
}

/// Wide text keeps the grid: a CJK or emoji grapheme is a box of its two
/// cells, and the line still ends at the width.
#[test]
fn wide_graphemes_take_their_cells() {
    let theme = render::ferrite_theme(false);
    let line = Inked::of("日本 ok", Ink::Text);
    let runs = render::runs(&line, &theme);
    let shape: Vec<(&str, usize)> = runs
        .iter()
        .map(|run| (run.text.as_str(), run.cells))
        .collect();
    assert_eq!(shape, [("日", 2), ("本", 2), (" ok", 3)]);
    let mut state = board();
    state.sel = "proj/18".into();
    relist(&mut state);
    let lines = draw(&mut state, &theme, 100, 60);
    assert!(lines.iter().all(|line| cells(line) <= 100));
}

/// The fixture rows, `proj/5` given an agent in a pane and a conversation
/// the loader read for it (`snap`): your words, then its answer.
fn talking_board() -> State {
    sw_board::clock::set(Some(NOW));
    let rows = fixture::rows(NOW)
        .into_iter()
        .map(|(_, mut row)| {
            if row.s("id") == "proj/5" {
                row.put("link", serde_json::json!({ "pane": "w1:p5" }));
                row.put(
                    "snap",
                    serde_json::json!({ "talk": { "who": "agent", "readable": true, "list": [
                        { "who": "you", "text": "Check every caller of the resize handler too." },
                        { "who": "them", "text": "Every caller had the same mistake; all three are fixed." },
                    ] } }),
                );
            }
            row
        })
        .collect();
    fixture::board_of(rows).state
}

fn press(state: &mut State, chord: &str) {
    let press = match chord {
        " " => Press::char(' '),
        chord => Press {
            chord: chord.into(),
            ch: None,
        },
    };
    update(state, Event::Key(press));
    state.now = NOW;
}

fn at_line(lines: &[Vec<Run>], starts: &str) -> usize {
    lines
        .iter()
        .position(|line| text(line).starts_with(starts))
        .unwrap_or_else(|| panic!("no line starts {starts:?}"))
}

/// Talking to a task's agent: the chat's head line names it (its mark, its
/// number, its title in bold), and its latest message stands in a rounded
/// box in the agent's ink, under the speaker's label, over the text box.
#[test]
fn the_chat_shows_the_agents_message_in_its_box() {
    let theme = render::ferrite_theme(false);
    let mut state = talking_board();
    press(&mut state, " ");
    assert!(state.reply, "space talks to the selected task's agent");
    let lines = draw(&mut state, &theme, 120, 40);
    let top = at_line(&lines, "╭");
    let head = &lines[top - 1];
    assert!(text(head).contains("#5"), "{:?}", text(head));
    assert!(run(head, "Max length for task titles").bold);
    let working = colour(&theme, Ink::Yellow);
    assert_eq!(
        run(&lines[top], "╭").fg,
        working,
        "the box in the agent's ink"
    );
    assert_eq!(run(&lines[top], "╮").fg, working);
    let bottom = at_line(&lines, "╰");
    assert_eq!(run(&lines[bottom], "╯").fg, working);
    assert!(
        text(&lines[top]).contains("alt+k more ↑"),
        "{:?}",
        text(&lines[top])
    );
    let said = line_with(&lines[top..bottom], "Agent:");
    assert_eq!(run(said, "Agent:").fg, colour(&theme, Ink::Muted));
    assert_eq!(run(said, "│").fg, working);
    let message = line_with(&lines[top..bottom], "Every caller had the same mistake");
    let soft = colour(&theme, Ink::Soft);
    assert!(
        message
            .iter()
            .filter(|run| run.text.chars().any(char::is_alphabetic))
            .all(|run| run.fg == soft),
        "its words, in the soft grey: {message:?}"
    );
    assert!(
        !lines.iter().any(|line| text(line).contains("You:")),
        "the box opens on the newest message alone"
    );
}

/// alt+k opens the box to the conversation above the newest message, alt+j
/// from its end closes it again; alt+c hides the message and shows it.
#[test]
fn the_chat_scrolls_and_hides_on_its_keys() {
    let theme = render::ferrite_theme(false);
    let mut state = talking_board();
    press(&mut state, " ");
    let shows = |state: &mut State, words: &str| {
        draw(state, &theme, 120, 40)
            .iter()
            .any(|line| text(line).contains(words))
    };
    assert!(!shows(&mut state, "You:"));
    press(&mut state, "alt+k");
    assert!(shows(&mut state, "You:"), "your words, above its answer");
    let lines = draw(&mut state, &theme, 120, 40);
    assert_eq!(
        run(line_with(&lines, "You:"), "You:").fg,
        colour(&theme, Ink::Muted)
    );
    press(&mut state, "alt+j");
    assert!(
        !shows(&mut state, "You:"),
        "down from the end: the newest alone"
    );

    press(&mut state, "alt+c");
    assert!(!state.chat_open);
    assert!(!shows(&mut state, "╭"), "hidden: the plain text box");
    press(&mut state, "alt+c");
    assert!(shows(&mut state, "╭"));
}

/// Under 14 lines the chat folds away by itself: the text box and its hints
/// keep the room.
#[test]
fn the_chat_folds_under_fourteen_lines() {
    let theme = render::ferrite_theme(false);
    let mut state = talking_board();
    press(&mut state, " ");
    let lines = draw(&mut state, &theme, 120, 13);
    assert!(state.chat_folded);
    assert!(!lines.iter().any(|line| text(line).starts_with("╭")));
    assert!(lines.iter().any(|line| text(line).starts_with("❯")));
    let lines = draw(&mut state, &theme, 120, 14);
    assert!(!state.chat_folded);
    assert!(lines.iter().any(|line| text(line).starts_with("╭")));
}

/// Done tasks past the first five fold into one quiet line that says the
/// key that shows them.
#[test]
fn done_past_five_folds_into_one_line() {
    let theme = render::ferrite_theme(false);
    sw_board::clock::set(Some(NOW));
    let mut rows: Vec<Obj> = fixture::rows(NOW)
        .into_iter()
        .filter(|(_, row)| row.s("id") == "proj/5")
        .map(|(_, row)| row)
        .collect();
    for n in 0..7 {
        let mut done = rows[0].clone();
        done.put("id", format!("proj/{}", 30 + n));
        done.put("title", format!("Finished {n}"));
        done.put("state", "done");
        done.put("stage", "done");
        done.put("at", NOW - (n + 1) * 60_000);
        rows.push(done);
    }
    let mut state = fixture::board_of(rows).state;
    let lines = draw(&mut state, &theme, 120, 40);
    let shown = lines
        .iter()
        .filter(|line| text(line).contains("Finished"))
        .count();
    assert_eq!(shown, 5);
    let note = line_with(&lines, "more done");
    assert_eq!(text(note).trim_end(), "   ↓ 2 more done (alt+d shows them)");
    assert_eq!(
        run(note, "↓ 2 more done (alt+d shows them)").fg,
        colour(&theme, Ink::Muted)
    );
    press(&mut state, "alt+d");
    let lines = draw(&mut state, &theme, 120, 40);
    assert!(!lines.iter().any(|line| text(line).contains("more done")));
    assert_eq!(
        lines
            .iter()
            .filter(|line| text(line).contains("Finished"))
            .count(),
        7
    );
}

/// A folded heading is one row: ▸, its name in its group's ink, and how
/// many tasks it holds; its rows are gone.
#[test]
fn a_folded_heading_says_how_many_tasks_it_holds() {
    use sw_board::board::update::heads::group_id;
    let theme = render::ferrite_theme(false);
    let mut state = board();
    let open = draw(&mut state, &theme, 120, 60);
    assert_eq!(
        run(line_with(&open, "▾ to do"), "▾").fg,
        colour(&theme, Ink::Muted)
    );
    let id = group_id(&state, "", "to do");
    state.folded.insert(id);
    relist(&mut state);
    let lines = draw(&mut state, &theme, 120, 60);
    let heading = line_with(&lines, "▸ to do");
    assert_eq!(text(heading).trim_end(), " ▸ to do  2 tasks");
    assert_eq!(run(heading, "▸").fg, colour(&theme, Ink::Muted));
    assert_eq!(run(heading, "to do").fg, colour(&theme, Ink::Todo));
    assert_eq!(run(heading, "2 tasks").fg, colour(&theme, Ink::Muted));
    assert!(!lines.iter().any(|line| text(line).contains("TTTTTTTTTT")));
}

/// A part that fails to draw says so in its place, in one red line (the
/// view's `failed`, under its guard). Rust cannot make a part of the
/// vendored view fail in a test, so this draws the line the guard's answer
/// makes, as upstream's own test does.
#[test]
fn a_part_that_fails_says_so_in_red() {
    let theme = render::ferrite_theme(false);
    let why =
        sw_board::board::view::guard::part(|| -> usize { panic!("{}", "no rows") }).unwrap_err();
    let line = Inked::plain("  ").add(Inked::of(
        format!("✗ The list could not be shown: {why}"),
        Ink::Red,
    ));
    let runs = render::runs(&line, &theme);
    assert_eq!(text(&runs), "  ✗ The list could not be shown: no rows");
    assert_eq!(run(&runs, "✗").fg, RED);
    assert_eq!(run(&runs, "The list could not be shown: no rows").fg, RED);
}

/// A view that panics whole is two plain lines that say how to close it,
/// as upstream's runtime says it.
#[test]
fn a_view_that_fails_whole_says_how_to_close() {
    let theme = render::ferrite_theme(false);
    let lines: Vec<Vec<Run>> = super::runtime::view_error()
        .iter()
        .map(|line| render::runs(line, &theme))
        .collect();
    let said: Vec<String> = lines.iter().map(|line| text(line)).collect();
    assert_eq!(said, [" herdr-projects view error", " press esc to close"]);
    assert!(lines.iter().flatten().all(|run| run.fg == TEXT));
}

/// Ctrl+C shows how the board closes; a second one within two seconds
/// closes it — through the view, as GPUI delivers the keys.
#[gpui::test]
fn ctrl_c_twice_closes_the_board(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| BoardView::still(fixture::board(), cx));
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    cx.update(|_, cx| {
        let closed = closed.clone();
        cx.subscribe(&view, move |_, event: &BoardEvent, _| {
            closed.set(closed.get() || *event == BoardEvent::Close)
        })
        .detach();
    });
    view.update_in(cx, |view, window, cx| {
        view.fit(800.0, 600.0);
        window.focus(&view.focus, cx);
    });
    cx.simulate_keystrokes("H i");
    view.update(cx, |view, _| {
        assert_eq!(view.state().unwrap().value(), "Hi");
    });
    cx.simulate_keystrokes("ctrl-c");
    assert!(!closed.get());
    assert!(view.update(cx, |view, _| {
        view.screen().last().is_some_and(|line| {
            line.text()
                .contains("Press ctrl+c again to close the board")
        })
    }));
    cx.simulate_keystrokes("ctrl-c");
    assert!(closed.get());
}

// ------------------------------------------------------------- placement

/// The popup is 90% of the board, and all of it under 110×30 cells, as
/// upstream's takes a whole pane there.
#[test]
fn the_popup_takes_the_whole_board_when_it_is_small() {
    let (columns, rows) = super::POPUP_FULL_BELOW;
    let (wide, tall) = (columns * crate::theme::CH, rows * crate::theme::ROW);
    assert_eq!(
        super::popup_size(wide * 2.0, tall * 2.0),
        (
            wide * 2.0 * super::POPUP_SHARE,
            tall * 2.0 * super::POPUP_SHARE
        )
    );
    assert_eq!(
        super::popup_size(wide - 1.0, tall * 2.0),
        (wide - 1.0, tall * 2.0)
    );
    assert_eq!(
        super::popup_size(wide * 2.0, tall - 1.0),
        (wide * 2.0, tall - 1.0)
    );
}

/// The board's "brings you back to the board" names the key that opens
/// it, as the board writes keys.
#[test]
fn the_board_key_is_the_projects_toggle() {
    let want = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => "cmd+shift+a",
        crate::keymap::Platform::Windows => "ctrl+shift+a",
    };
    assert_eq!(super::board_key(), want);
}
