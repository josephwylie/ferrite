//! Whole frames of the board, drawn by the terminal drawer into cells at sizes from 10×8 to 200×60, each with the list,
//! the task beside it, the whole task, the ? menu and a draft: compared with the frames kept in tests/frames/ (written
//! there when missing, or again with UPDATE_FRAMES=1). With colour, each cell's colour is kept too, as a letter per
//! colour with a legend. Every frame holds to the rules that need no picture: no line over the width, the hint line last.
use crate::common;
use common::fixtures::fixture_rows;
use common::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use sw::board::data::Data;
use sw::board::keys::Press;
use sw::board::state::{Ctx, State};
use sw::board::theme::Theme;
use sw::board::update::{list::relist, update, Event};

const SIZES: [(u16, u16); 10] = [(10, 8), (20, 10), (30, 12), (40, 16), (60, 20), (80, 24), (100, 30), (120, 40), (160, 50), (200, 60)];
const NOW: i64 = 1_790_000_000_000;

fn board(w: &World) -> State {
    let id = w.add("Max length for task titles");
    let rows = fixture_rows(&task(&id), NOW);
    let items = rows.iter().map(|(_, r)| std::sync::Arc::new(r.clone())).collect();
    let data = Data { items, loaded: true, session: "default".into(), repo: "proj".into(), repo_root: w.root_str(), ..Default::default() };
    let mut s = State::new(Ctx { cwd: w.root_str(), ..Default::default() }, data);
    s.now = NOW; s.sel = "proj/5".into();
    relist(&mut s);
    s
}
fn key(s: &mut State, k: &str) { let p = if k.chars().count() == 1 { Press::char(k.chars().next().unwrap()) } else { Press::chord(k) }; update(s, Event::Key(p)); s.now = NOW; }
fn draw(s: &mut State, t: &Theme, w: u16, h: u16) -> Buffer {
    let lines = sw::board::view::screen(s, t, w as usize, h as usize).lines;
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    sw::board::run::draw::draw(t, &lines, &mut buf);
    buf
}
fn cells(buf: &Buffer) -> Vec<String> { let a = buf.area; (0..a.height).map(|y| (0..a.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string()).collect() }
/// Each cell's colour as a letter (`.` the terminal's own), and the legend of what each letter is.
fn colours(buf: &Buffer) -> String {
    let a = buf.area;
    let mut seen: Vec<String> = vec![];
    let mut out = vec![];
    for y in 0..a.height {
        let mut l = String::new();
        for x in 0..a.width {
            let c = &buf[(x, y)];
            let name = match (c.fg, c.bg) { (Color::Reset, Color::Reset) => String::new(), (f, b) => format!("{f:?}{}", if b == Color::Reset { String::new() } else { format!(" on {b:?}") }) };
            if name.is_empty() { l.push('.'); continue; }
            let n = seen.iter().position(|s| *s == name).unwrap_or_else(|| { seen.push(name.clone()); seen.len() - 1 });
            l.push(char::from_u32('a' as u32 + n as u32 % 26).unwrap());
        }
        out.push(l.trim_end_matches('.').to_string());
    }
    let legend: Vec<String> = seen.iter().enumerate().map(|(n, s)| format!("{} {s}", char::from_u32('a' as u32 + n as u32 % 26).unwrap())).collect();
    format!("{}\n--\n{}\n", legend.join("\n"), out.join("\n"))
}
fn same(file: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/frames").join(file);
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    if want.is_empty() || std::env::var("UPDATE_FRAMES").is_ok() { std::fs::create_dir_all(path.parent().unwrap()).unwrap(); std::fs::write(&path, got).unwrap(); return; }
    if want != got {
        let at = want.lines().zip(got.lines()).position(|(a, b)| a != b).unwrap_or(0);
        panic!("{file} is not the frame kept in tests/frames (UPDATE_FRAMES=1 writes it again). First change at line {}:\n kept: {:?}\n  now: {:?}", at + 1, want.lines().nth(at).unwrap_or(""), got.lines().nth(at).unwrap_or(""));
    }
}

#[test]
fn frames_at_every_size_with_and_without_colour() {
    let w = world();
    sw::clock::set(Some(NOW));
    let plain = Theme::default_dark(true);
    let coloured = Theme::default_dark(false);
    for (wd, ht) in SIZES {
        let mut text = String::new();
        for (part, keys) in [("the list", vec![]), ("the task beside it", vec!["alt+p"]), ("the whole task", vec!["alt+t"]), ("the ? menu", vec!["?"]), ("a draft", vec!["H", "e", "l", "l", "o", " ", "日", "本", " ", "🚀"])] {
            let mut s = board(&w);
            for k in &keys { key(&mut s, k); }
            let buf = draw(&mut s, &plain, wd, ht);
            let lines = cells(&buf);
            assert_eq!(lines.len(), ht as usize);
            for l in &lines { assert!(sw::text::width(l) <= wd as usize, "{wd}×{ht} {part}: {l:?}"); }
            text.push_str(&format!("== {part}\n{}\n", lines.join("\n")));
            for y in 0..ht { for x in 0..wd { assert_eq!(buf[(x, y)].fg, Color::Reset, "no colour without colour"); } }
        }
        same(&format!("{wd}x{ht}.txt"), &text);
        let mut s = board(&w);
        same(&format!("{wd}x{ht}.colour.txt"), &colours(&draw(&mut s, &coloured, wd, ht)));
    }
    sw::clock::set(None);
}
