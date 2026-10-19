//! The grey inks, quiet but always readable: the grey of greyed-out words, the grey of words that have to be read and
//! the dim grey of "to do" each stand out from the background they are drawn on by a stated floor. The measure is the
//! WCAG 2 contrast ratio. The floor is 4.5 to 1, what WCAG asks of text; where a theme's own plain text is so quiet
//! that 4.5 would leave the grey less than 1.5 times dimmer than it, the floor comes down with it, and never under
//! 3 to 1. A theme's grey under its floor is pulled towards the plain text colour, only as far as the floor. The
//! background is the terminal's own where it said so (the board paints none), else the theme's. On the selected row a
//! grey keeps 3 to 1, or as much as that row's own text has to give, never nearer plain text than 1.3 times dimmer.
//! The terminal's own colours cannot be measured unasked: where the terminal told its text colour and background, the
//! grey is mixed from those two and measured like any other; where it did not, greyed words are the plain text colour.
use super::C;

pub const FLOOR: f64 = 4.5;
pub const LEAST: f64 = 3.0;
pub const DIMMER: f64 = 1.5;
pub const TODO_DIMMER: f64 = 1.3;
pub const TODO_APART: f64 = 60.0;
pub const TODO_BESIDE: [&str; 2] = ["green", "review"];
const GREYS: [u32; 5] = [0x808080, 0x6b7280, 0x555555, 0x9ca3af, 0xaaaaaa];
/// The terminal's 16 colours as xterm shows them: what a colour given by name looks like, as near as can be known.
pub const XTERM: [u32; 16] = [0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff];

pub fn rgb(c: u32) -> [f64; 3] { [(c >> 16) as f64, ((c >> 8) & 255) as f64, (c & 255) as f64] }
/// How far apart two colours look. One of the terminal's 16 is measured as xterm draws it; the terminal's own colour
/// cannot be known, and counts as far.
pub fn apart(a: C, b: C) -> f64 {
    if a == b { return 0.0; }
    match (a.drawn(), b.drawn()) { (Some(x), Some(y)) => { let (x, y) = (rgb(x), rgb(y)); ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt() } _ => f64::INFINITY }
}
fn linear(v: f64) -> f64 { let v = v / 255.0; if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
pub fn lum(c: u32) -> f64 { let [r, g, b] = rgb(c); 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b) }
/// The WCAG 2 contrast ratio of two real colours, 1 (the same) to 21 (black on white).
pub fn contrast(a: u32, b: u32) -> f64 { let (x, y) = (lum(a), lum(b)); let (hi, lo) = if x > y { (x, y) } else { (y, x) }; (hi + 0.05) / (lo + 0.05) }
pub fn mix(a: u32, b: u32, part: f64) -> u32 {
    let (x, y) = (rgb(a), rgb(b));
    (0..3).fold(0, |all, i| (all << 8) | (x[i] + (y[i] - x[i]) * part).round() as u32)
}
fn pole(bg: u32) -> u32 { if lum(bg) < 0.18 { 0xffffff } else { 0x000000 } }
/// The floor on this background, beside this plain text colour.
pub fn floor_on(bg: u32, text: Option<u32>) -> f64 { match text { Some(t) => (contrast(t, bg) / DIMMER).clamp(LEAST, FLOOR), None => FLOOR } }
/// The colour, or the nearest to it that meets the floor: pulled towards the plain text colour (towards black or
/// white where that is not known, or is itself under the floor). A colour that cannot be measured is left as it is.
pub fn readable(c: C, bg: C, text: C, row: C) -> C {
    let (Some(c), Some(bg)) = (c.real(), bg.real()) else { return c };
    let text = text.real();
    let floor = floor_on(bg, text);
    let ok = |x: u32| contrast(x, bg) >= floor;
    let on_row = row.real().map(|r| text.map(|t| (contrast(t, r) / TODO_DIMMER).min(LEAST)).unwrap_or(LEAST)).unwrap_or(0.0);
    let both = |x: u32| ok(x) && (on_row == 0.0 || (contrast(x, row.real().unwrap()) >= on_row && text.map(|t| contrast(t, bg) / contrast(x, bg) >= TODO_DIMMER).unwrap_or(true)));
    let to = text.filter(|t| ok(*t)).unwrap_or(pole(bg));
    for fits in [&both as &dyn Fn(u32) -> bool, &ok] { for i in 0..=32 { let x = mix(c, to, i as f64 / 32.0); if fits(x) { return C::Rgb(x); } } }
    C::Rgb(to)
}
/// A colour made quieter and still seen: halfway to the background, or as near to that as keeps it 3 to 1, on the
/// selected row too where the colour itself has it. A colour that cannot be measured stays as it is.
pub fn quieter(c: C, bg: C, row: C) -> C {
    let (Some(x), Some(b)) = (c.real(), bg.real()) else { return c };
    let on_row = row.real().map(|r| contrast(x, r).min(LEAST)).unwrap_or(0.0);
    for i in (0..=16).rev() { let m = mix(x, b, i as f64 / 32.0); if contrast(m, b) >= LEAST && (on_row == 0.0 || contrast(m, row.real().unwrap()) >= on_row) { return C::Rgb(m); } }
    c
}
/// The board's greys made readable, in place. The "terminal" theme's own greys are replaced as said above; a colour you
/// named yourself is kept (`own`: the theme's colour is still the terminal theme's own).
pub fn readable_greys(p: &mut super::Pal, terminal_own: &dyn Fn(&str, C) -> bool, ground: C, text: C, seen_bg: bool) {
    let row = p.row();
    let known = seen_bg && text.real().is_some();
    for (k, part) in [("overlay0", 0.5), ("overlay1", 0.4), ("subtext0", 0.25)] {
        let now = p.get(k);
        let v = if terminal_own(k, now) { if known { readable(C::Rgb(mix(text.real().unwrap(), ground.real().unwrap(), part)), ground, text, C::Own) } else { C::Own } } else { readable(now, ground, text, row) };
        p.set(k, v);
    }
}
/// "todo": the dim grey of a task nobody has started, never taken for plain text nor for a state that has a colour.
/// The theme's greyed-out grey where it meets the floor, is 1.3 times dimmer than text or more and is told apart from
/// the green of "ready" and the blue of "under review"; else the theme's other grey, else one of the board's greys.
/// Where text or background is the terminal's own and was not told, it is the plain text colour.
pub fn todo_grey(p: &super::Pal, ground: C, text: C) -> C {
    let (Some(t), Some(bg)) = (text.real(), ground.real()) else { return C::Own };
    let least = |c: C| TODO_BESIDE.iter().map(|k| apart(c, p.get(k))).fold(f64::INFINITY, f64::min);
    let reads = |c: C| c.real().map(|x| contrast(x, bg) >= floor_on(bg, Some(t)) && contrast(t, bg) / contrast(x, bg) >= TODO_DIMMER).unwrap_or(false);
    let all: Vec<C> = [p.overlay0, p.overlay1].into_iter().chain(GREYS.iter().map(|g| C::Rgb(*g))).filter(|c| c.real().is_some()).map(|c| readable(c, ground, text, p.row())).collect();
    if let Some(c) = all.iter().find(|c| reads(**c) && least(**c) >= TODO_APART) { return *c; }
    let pool: Vec<C> = if all.iter().any(|c| reads(*c)) { all.iter().copied().filter(|c| reads(*c)).collect() } else { all };
    pool.into_iter().fold(None, |best: Option<C>, c| match best { Some(b) if least(b) >= least(c) => Some(b), _ => Some(c) }).unwrap_or(C::Own)
}
