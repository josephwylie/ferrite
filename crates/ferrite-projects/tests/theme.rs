//! The board takes its colours and status marks from herdr's own config, resolved the way herdr resolves them: its
//! default, the board's own `board.theme`, herdr's names, aliases, custom colours and marks, light and dark, and what
//! the terminal says of itself. The rest is in theme_*.rs: the colours' meanings, the greys, the config, the paint.
use sw::board::theme::answers::read_answers;
use sw::board::theme::{resolve, Resolved, Seen, Theme, C, TOKENS};
use sw::herdr::palette::{parse_color, THEME_NAMES};
use sw::shown::ink::Ink;

fn cfg(s: &str) -> toml::Table { s.parse().unwrap_or_else(|e| panic!("{s}: {e}")) }
fn res(c: &toml::Table, light: Option<bool>, want: &str) -> Resolved { resolve(c, light, want, Seen::default()) }
fn named(c: &str) -> C { C::of(parse_color(c)) }
const RGB: fn(u32) -> C = C::Rgb;

#[test]
fn with_no_theme_set_the_board_uses_herdrs_default_catppuccin_and_its_own_symbols() {
    let none = cfg("onboarding = false\n[ui]\n");
    let r = res(&none, None, "herdr");
    // The old board's `resolve` said "symbols: false" for "the config did not ask for symbols"; the marks it drew were
    // its own symbols all the same (only "dots" by name gives herdr's dots). Rust's `symbols` is the marks drawn.
    assert_eq!(r.name, "catppuccin"); assert!(r.symbols); assert_eq!(res(&toml::Table::new(), None, "herdr").name, "catppuccin");
    let p = r.pal;
    assert_eq!(p.accent, RGB(0x89b4fa)); assert_eq!(p.yellow, RGB(0xf9e2af)); assert_eq!(p.red, RGB(0xf38ba8)); assert_eq!(p.teal, RGB(0x94e2d5)); assert_eq!(p.green, RGB(0xa6e3a1));
}

#[test]
fn the_boards_own_setting_herdrs_theme_as_herdr_shows_it_or_one_of_herdrs_themes_by_its_name() {
    // With no theme in herdr's config herdr shows its default, so the board does too: the two never look like two programs.
    let none = cfg("onboarding = false\n[ui]\n");
    let all = [none.clone(), toml::Table::new(), cfg("[theme.custom]\n"), cfg("[theme]\nauto_switch = false\n"), cfg("[ui]\naccent = \"red\"\n")];
    for want in ["herdr", "auto"] { for c in &all { assert_eq!(res(c, None, want).name, "catppuccin", "{want} {c:?}"); } }
    let e = sw::store::settings_list::entry_of("board.theme").unwrap();
    assert_eq!(e.builtin, "herdr");
    let mut values = vec!["herdr", "dark", "light"]; values.extend_from_slice(THEME_NAMES);
    assert_eq!(e.values, values);
    assert_eq!(res(&cfg("[theme]\nname = \"nord\"\n"), None, "herdr").name, "nord");
    assert_eq!(res(&cfg("[theme]\nname = \"gruvbox\"\nauto_switch = true\n"), Some(true), "herdr").name, "gruvbox-light");
    assert_eq!(res(&none, None, "dark").name, "catppuccin"); assert_eq!(res(&none, None, "light").name, "catppuccin-latte");
    // A theme by its name is that theme whatever the config names: for a herdr opened from another machine, whose
    // client paints with a config the board cannot read. "terminal" is the terminal's own colours.
    for c in [none.clone(), cfg("[theme]\nname = \"nord\"\n"), cfg("[theme]\nname = \"nord\"\nauto_switch = true\ndark_name = \"dracula\"\n")] {
        let t = res(&c, Some(true), "terminal");
        assert_eq!(t.name, "terminal");
        assert_eq!(t.pal.text, C::Own); assert_eq!(t.pal.yellow, named("yellow")); assert_eq!(t.pal.teal, named("cyan")); assert_eq!(t.pal.green, named("green")); assert_eq!(t.pal.red, named("lightred"));
    }
    for name in THEME_NAMES {
        let t = res(&cfg("[theme]\nname = \"dracula\"\nauto_switch = true\n"), Some(true), name);
        assert_eq!(t.name, *name); assert_eq!(t.pal.red, res(&cfg(&format!("[theme]\nname = \"{name}\"\n")), None, "herdr").pal.red, "{name}");
    }
    assert_eq!(res(&cfg("[theme.custom]\nred = \"#010203\"\n"), None, "terminal").pal.red, RGB(0x010203)); // custom colours apply to it, as in herdr
    assert_eq!(res(&cfg("[theme]\nname = \"nord\"\n"), None, "no-such-theme").name, "nord"); // a word that names nothing: herdr's own
}

#[test]
fn the_terminal_says_whether_it_is_light_or_dark() {
    let (said, done) = read_answers("\x1b]11;rgb:0d0d/1616/1c1c\x1b\\\x1b[?62;22c");
    assert_eq!(said.light, Some(false)); assert!(done);
    let (said, done) = read_answers("\x1b]11;rgb:ffff/ffff/ffff\x07");
    assert_eq!((said.light, said.fg, said.bg, done), (Some(true), None, Some(0xffffff), false));
    let (said, done) = read_answers("\x1b]11;rgb:aa");
    assert_eq!((said.light, said.fg, said.bg, done), (None, None, None, false));
    // Its text colour and its background, for the greys: each answer by its own number.
    let (said, done) = read_answers("\x1b]10;rgb:d0d0/d7d7/e5e5\x1b\\\x1b]11;rgb:0b/1a/33\x1b\\\x1b[?62c");
    assert_eq!((said.light, said.fg, said.bg, done), (Some(false), Some(0xd0d7e5), Some(0x0b1a33), true));
}

#[test]
fn herdrs_theme_name_aliases_custom_colours_old_accent_setting_and_symbol_marks_are_followed() {
    let c = cfg("[theme]\nname = \"Tokyo Night\" # mine\n[theme.custom]\nred = \"#f00\"\ngreen = \"rgb(1, 2, 3)\"\nteal = \"lightblue\"\npanel_bg = \"reset\"\n[ui]\nstatus_indicators = \"symbols\"\naccent = \"magenta\"\n[[keys.command]]\nname = \"dracula\"\n");
    let r = res(&c, None, "herdr");
    assert_eq!(r.name, "tokyo-night"); assert!(r.symbols);
    let p = &r.pal;
    assert_eq!(p.yellow, RGB(0xe0af68)); assert_eq!(p.red, RGB(0xff0000)); assert_eq!(p.green, RGB(0x010203)); assert_eq!(p.panel_bg, C::Own);
    assert_eq!(p.teal, named("lightblue")); assert_eq!(p.accent, named("magenta"));
    assert_eq!(res(&cfg("[theme]\nname = \"no-such\"\n"), None, "herdr").pal.accent, RGB(0x89b4fa));
    // Names that every JS object happened to carry were unknown names there: herdr's fallbacks, never a broken palette
    // or colour. Here they are plain unknown words, kept for the same promise.
    let t = Theme::default_dark(false);
    for odd in ["constructor", "toString", "__proto__", "hasOwnProperty"] {
        let r = res(&cfg(&format!("[theme]\nname = \"{odd}\"\n[theme.custom]\nred = \"{odd}\"\n[ui]\nstatus_indicators = \"{odd}\"\n")), None, "herdr");
        assert_eq!(r.pal.accent, RGB(0x89b4fa), "{odd}"); assert_eq!(r.pal.green, RGB(0xa6e3a1), "{odd}"); assert_eq!(r.pal.red, named("cyan"), "{odd}");
        assert!(r.symbols, "{odd}: not \"dots\", so the board's own symbols");
        assert_eq!(t.mark(odd, ""), "·"); assert_eq!(t.pal.get(odd), C::Own);
    }
    // Every colour name the board paints with is one of herdr's (or the board's own four, or the selected row). The old
    // test searched the view's source for names; here an ink is a type, so the check is that each names a colour.
    let inks = [Ink::Plain, Ink::Text, Ink::Soft, Ink::Muted, Ink::Red, Ink::Green, Ink::Yellow, Ink::Teal, Ink::Peach, Ink::Mauve, Ink::Accent, Ink::Review, Ink::Check, Ink::Todo, Ink::Auto, Ink::Rule, Ink::Dim, Ink::Sidebar, Ink::Selection];
    for i in inks { let n = i.name(); assert!(TOKENS.contains(&n) || ["review", "check", "todo", "auto", "plain", "selection"].contains(&n), "{n} is not a herdr colour"); }
    for i in inks { if !matches!(i, Ink::Plain | Ink::Sidebar) { assert_ne!(t.ink(i), C::Own, "{}: Catppuccin has it", i.name()); } }
}

#[test]
fn light_and_dark_only_when_herdr_switches_by_appearance_or_the_boards_own_setting_picks_one() {
    let manual = cfg("[theme]\nname = \"gruvbox\"\n");
    assert_eq!(res(&manual, Some(true), "herdr").name, "gruvbox");
    assert_eq!(res(&manual, Some(true), "light").name, "gruvbox-light"); assert_eq!(res(&cfg("[theme]\nname = \"gruvbox-light\"\n"), None, "dark").name, "gruvbox");
    let auto = cfg("[theme]\nname = \"gruvbox\"\nauto_switch = true\nlight_name = \"rose-pine-dawn\"\n[theme.custom]\naccent = \"#010101\"\n[theme.custom.light]\naccent = \"#020202\"\n");
    assert_eq!(res(&auto, Some(false), "herdr").name, "gruvbox"); assert_eq!(res(&auto, None, "herdr").pal.accent, RGB(0x010101));
    assert_eq!(res(&auto, Some(true), "herdr").name, "rose-pine-dawn"); assert_eq!(res(&auto, Some(true), "herdr").pal.accent, RGB(0x020202));
}
