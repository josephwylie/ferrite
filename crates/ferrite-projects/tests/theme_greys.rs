//! The greys are quiet but always readable: each stands out from the background by the floor, in every theme, on the
//! terminal's own colours where it told them, on the selected row; and a finished mark is its colour made quieter.
use sw::board::theme::greys::{contrast, floor_on, quieter, readable, FLOOR, LEAST, TODO_DIMMER};
use sw::board::theme::{resolve, Pal, Seen, Theme, C};
use sw::herdr::palette::THEME_NAMES;
use sw::shown::ink::{Ink, Inked};

fn on(name: &str, seen: Seen) -> Pal { resolve(&toml::Table::new(), None, name, seen).pal }
fn real(c: C) -> u32 { c.real().unwrap_or_else(|| panic!("{c:?} is not a real colour")) }
fn told(fg: Option<u32>, bg: u32) -> Seen { Seen { light: None, fg, bg: Some(bg) } }

#[test]
fn the_greys_are_quiet_but_always_readable_each_stands_out_from_the_background_by_the_floor_in_every_theme() {
    // The measure is the WCAG contrast ratio; the floor 4.5 to 1, lower only where the theme's own text is quiet, never under 3.
    assert_eq!(contrast(0x000000, 0xffffff), 21.0); assert_eq!(contrast(0x777777, 0x777777), 1.0); assert_eq!(FLOOR, 4.5); assert_eq!(LEAST, 3.0);
    assert_eq!(floor_on(0x2e3440, Some(0xeceff4)), 4.5); assert!((floor_on(0x002b36, Some(0x93a1a1)) - 3.74).abs() < 0.01); assert_eq!(floor_on(0xfdf6e3, Some(0x657b83)), 3.0);
    for name in THEME_NAMES.iter().filter(|n| **n != "terminal") {
        let p = on(name, Seen::default()); let (bg, text) = (real(p.panel_bg), real(p.text)); let floor = floor_on(bg, Some(text));
        for k in ["todo", "overlay0", "subtext0", "check"] { let c = contrast(real(p.get(k)), bg); assert!(c >= floor, "{name}: {k} is {c:.2} to 1, under {floor:.2}"); }
        let todo = contrast(real(p.todo), bg);
        assert!(todo >= LEAST && todo <= contrast(text, bg) / TODO_DIMMER, "{name}: to do is quiet");
    }
    // Nord's grey was 1.7 to 1, nearly its background: it is pulled towards its text colour, as far as the floor and no further.
    assert!(contrast(0x4c566a, 0x2e3440) < 2.0);
    { let c = contrast(real(on("nord", Seen::default()).overlay0), 0x2e3440); assert!((4.5..4.8).contains(&c), "{c}"); }
    assert_eq!(readable(C::Rgb(0x9d8e7c), C::Rgb(0x282828), C::Rgb(0xebdbb2), C::Own), C::Rgb(0x9d8e7c)); // a grey that reads is left as it is
    // The terminal's own colours cannot be measured unasked, and none of its greys can be trusted: bright black is nearly
    // the background on a dark navy, white on a light one, and faint text falls to 1.9 to 1 on Solarized light. With
    // nothing told, greyed words and "to do" are the plain text colour, the one colour known to read there.
    { let p = on("terminal", Seen::default()); for k in ["todo", "overlay0", "overlay1", "subtext0", "text"] { assert_eq!(p.get(k), C::Own, "{k}"); } }
    assert!(contrast(0xb1b9b3, 0xfdf6e3) < 2.0); // what faint made of Solarized light's text colour
    // Told only its background, the text colour is still not known, and a grey measured beside a guess can come out
    // stronger than the real text (on Solarized light, 4.6 to 1 beside text of 4.1): the plain text colour then too.
    for bg in [0x0b1a33, 0xfdf6e3, 0xffffff, 0x002b36] { let p = on("terminal", told(None, bg)); for k in ["todo", "overlay0", "overlay1", "subtext0"] { assert_eq!(p.get(k), C::Own, "{k} on {bg:x}"); } }
    // Another theme knows its own text colour: told only the background, its greys are measured on it and stay dimmer than its text.
    for (name, bg) in [("nord", 0x0b1a33), ("catppuccin-latte", 0xfdf6e3), ("solarized-light", 0xffffff)] {
        let p = on(name, told(None, bg));
        assert!(contrast(real(p.todo), bg) >= floor_on(bg, p.text.real()), "{name}"); assert!(contrast(real(p.text), bg) / contrast(real(p.todo), bg) >= TODO_DIMMER, "{name}");
    }
    // Where the terminal told its colours, the greys are mixed from those two and measured: on a dark navy and on white.
    for (fg, bg) in [(0xd0d7e5, 0x0b1a33), (0x000000, 0xffffff), (0x839496, 0x002b36), (0x657b83, 0xfdf6e3)] {
        let p = on("terminal", told(Some(fg), bg)); let floor = floor_on(bg, Some(fg));
        for k in ["todo", "overlay0", "subtext0"] { let c = contrast(real(p.get(k)), bg); assert!(c >= floor, "{k} on {bg:x}: {c}"); }
        for k in ["todo", "overlay0"] { assert!(contrast(fg, bg) / contrast(real(p.get(k)), bg) >= TODO_DIMMER, "{k} is dimmer than text on {bg:x}"); }
    }
    // Another theme on a terminal whose background is not the theme's: measured against the one it is really drawn on.
    { let bg = 0x0b1a33; let p = on("nord", told(Some(0xd0d7e5), bg)); assert!(contrast(real(p.todo), bg) >= 4.5); assert!(contrast(real(p.overlay0), bg) >= 4.5); }
    // On the selected row, whose background is the theme's own, a grey keeps 3 to 1 where that row's text has it to give.
    for name in ["catppuccin", "nord", "gruvbox", "solarized", "catppuccin-latte"] { let p = on(name, Seen::default()); let c = contrast(real(p.overlay0), real(p.selection_bg)); assert!(c >= 3.0, "{name}: {c}"); }
}

#[test]
fn a_finished_mark_is_its_colour_made_quieter_measured_and_a_colour_you_named_from_the_sixteen_is_kept() {
    // A finished task's mark is its colour made quieter, measured: never under 3 to 1, and no faint attribute, whose
    // strength is the terminal's to say. A colour that cannot be measured stays as it is.
    for (c, bg) in [(0x94e2d5, 0x181825), (0x179299, 0xeff1f5), (0x427b58, 0xfbf1c7)] {
        let q = real(quieter(C::Rgb(c), C::Rgb(bg), C::Own));
        assert!(contrast(q, bg) >= 3.0 && contrast(q, bg) < contrast(c, bg), "{c:x}: {}", contrast(q, bg));
    }
    assert_eq!(quieter(C::Ansi(6), C::Own, C::Own), C::Ansi(6)); assert_eq!(quieter(C::Rgb(0x777777), C::Rgb(0x808080), C::Own), C::Rgb(0x777777));
    let t = Theme::default_dark(false);
    assert_eq!(t.quiet(Ink::Teal), C::Rgb(0x5e8a88)); assert_ne!(t.quiet(Ink::Teal), t.ink(Ink::Teal));
    // The web board draws it quieter itself: the piece says so.
    let piece = serde_json::to_value(&Inked::of("✓", Ink::Teal).quiet().0[0]).unwrap();
    assert_eq!((piece["t"].as_str(), piece["c"].as_str(), piece["quiet"].as_bool()), (Some("✓"), Some("teal"), Some(true)));
    // A colour you named from the terminal's 16 is kept: it cannot be measured.
    let c: toml::Table = "[theme.custom]\noverlay0 = \"gray\"\n".parse().unwrap();
    assert_eq!(resolve(&c, None, "herdr", Seen::default()).pal.overlay0, C::Ansi(7));
}
