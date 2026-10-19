//! herdr's config is read however it is written (tables, inline tables, dotted keys, the whole of TOML), a file that
//! cannot be read gives herdr's defaults, and the review page takes the same theme and follows the browser.
use crate::common;
use sw::board::theme::page::page_theme_of;
use sw::board::theme::{read_config, resolve, Resolved, Seen, C};
use sw::herdr::palette::parse_color;

fn cfg(s: &str) -> toml::Table { s.parse().unwrap_or_else(|e| panic!("{s}: {e}")) }
fn res(c: &toml::Table) -> Resolved { resolve(c, None, "herdr", Seen::default()) }
/// The page's colours without the board's own `--c-*` list, which the old page did not have.
fn page(c: &str, want: &str) -> String { regex::Regex::new(r";--c-[a-z0-9-]+:#[0-9a-f]{6}").unwrap().replace_all(&page_theme_of(&cfg(c), want), "").into_owned() }
fn like(s: &str, re: &str) { assert!(regex::Regex::new(re).unwrap().is_match(s), "{s}\n  does not match {re}"); }

#[test]
fn herdrs_config_is_read_however_it_is_written_tables_inline_tables_dotted_keys() {
    let want = cfg("[theme]\nname = \"nord\"\nauto_switch = true\n[theme.custom]\nred = \"#010203\"\naccent = \"#040506\"\n[theme.custom.light]\ntext = \"#070809\"\n");
    let pick = |c: &toml::Table| (c.get("theme").cloned(), c.get("ui").and_then(|u| u.get("status_indicators")).cloned());
    let want = (want.get("theme").cloned(), Some(toml::Value::from("symbols")));
    assert_eq!(pick(&cfg("[theme]\nname = \"nord\"\nauto_switch = true\n[theme.custom]\nred = \"#010203\"\naccent = \"#040506\"\n[theme.custom.light]\ntext = \"#070809\"\n[ui]\nstatus_indicators = \"symbols\"\n")), want);
    assert_eq!(pick(&cfg("[theme]\nname = 'nord' # x\nauto_switch = true\ncustom = { red = \"#010203\", accent = \"#040506\", light = { text = \"#070809\" } } # y\n[ui]\nstatus_indicators = \"symbols\"\nrows = [[\"a\", \"b\"]]\nn = 3\n")), want);
    assert_eq!(pick(&cfg("theme = { name = \"nord\", auto_switch = true, custom.red = \"#010203\", custom.accent = \"#040506\", custom.light.text = \"#070809\" }\nui.status_indicators = \"symbols\"\n")), want);
    // The old board's own reader took a `[theme.custom]` header after `theme.custom.red = …` had made that table. TOML
    // forbids it and herdr (the toml crate, as here) rejects the file, so the board takes herdr's defaults, as herdr does.
    let twice = "theme.name = \"nord\"\ntheme.auto_switch = true\ntheme.custom.red = \"#010203\"\n[theme.custom]\naccent = \"#040506\"\nlight.text = \"#070809\"\n[[keys.command]]\nname = \"x\"\n[ui]\nstatus_indicators = \"symbols\"\n";
    assert!(twice.parse::<toml::Table>().is_err());
    assert_eq!(pick(&cfg("theme.name = \"nord\"\ntheme.auto_switch = true\ntheme.custom.red = \"#010203\"\ntheme.custom.accent = \"#040506\"\ntheme.custom.light.text = \"#070809\"\n[[keys.command]]\nname = \"x\"\n[ui]\nstatus_indicators = \"symbols\"\n")), want);
    let p = res(&cfg("[theme]\ncustom = { red = \"#010203\", accent = \"#040506\" }\n")).pal;
    assert_eq!(p.red, C::Rgb(0x010203)); assert_eq!(p.accent, C::Rgb(0x040506));
}

#[test]
fn the_whole_of_toml_is_read_so_nothing_herdr_accepts_is_lost() {
    // An inline table with a number in it, multi-line and escaped strings: all valid for herdr.
    let c = cfg("ui = { sidebar_width = 30, status_indicators = \"symbols\", accent = \"magenta\" }\n[theme]\nname = \"\"\"tokyo-\\\n   night\"\"\"\ndark_name = \"dr\\u0061cula\"\nlight_name = '''\none-light'''\n[theme.custom]\n\"red\" = \"\\U00000023f00\" # c\n");
    assert_eq!(c, cfg("[ui]\nsidebar_width = 30\nstatus_indicators = \"symbols\"\naccent = \"magenta\"\n[theme]\nname = \"tokyo-night\"\ndark_name = \"dracula\"\nlight_name = \"one-light\"\n[theme.custom]\nred = \"#f00\"\n"));
    let r = res(&c);
    assert!(r.symbols); assert_eq!(r.name, "tokyo-night"); assert_eq!(r.pal.accent, C::of(parse_color("magenta"))); assert_eq!(r.pal.red, C::Rgb(0xff0000));
    let all = cfg("a = [1, 2.5, -3e2, 0xff, 1_000, # c\n  true, \"x\", { b.c = 'y' },\n]\nd = 1979-05-27T07:32:00Z\ne = 1979-05-27 07:32:00 # c\nf = \"\"\"a\"\"b\"\"\"\"\"\ng = \"a\\tb\\\"c\\\\\"\n[[k.cmd]]\nkey = \"x\"\n[k.cmd.sub]\nz = inf\n[[k.cmd]]\nkey = \"y\"\n[h.\"i.j\"]\n'k l' = false\n");
    let a = all["a"].as_array().unwrap();
    assert_eq!((a[0].as_integer(), a[1].as_float(), a[2].as_float(), a[3].as_integer(), a[4].as_integer(), a[5].as_bool(), a[6].as_str()), (Some(1), Some(2.5), Some(-300.0), Some(255), Some(1000), Some(true), Some("x")));
    assert_eq!(a[7]["b"]["c"].as_str(), Some("y"));
    assert_eq!(all["d"].to_string(), "1979-05-27T07:32:00Z"); assert_eq!(all["e"].to_string(), "1979-05-27T07:32:00"); // the same time; TOML writes it with a T
    assert_eq!(all["f"].as_str(), Some("a\"\"b\"\"")); assert_eq!(all["g"].as_str(), Some("a\tb\"c\\"));
    let cmd = all["k"]["cmd"].as_array().unwrap();
    assert_eq!(cmd.len(), 2); assert_eq!(cmd[0]["key"].as_str(), Some("x")); assert_eq!(cmd[0]["sub"]["z"].as_float(), Some(f64::INFINITY)); assert_eq!(cmd[1]["key"].as_str(), Some("y"));
    assert_eq!(all["h"]["i.j"]["k l"].as_bool(), Some(false));
    for bad in ["a = \"x", "a = ", "a b = 1", "[t\na = 1", "a = \"\\q\"", "a = 1 2", "a = {b = 1", "a = nope"] { assert!(bad.parse::<toml::Table>().is_err(), "{bad}"); }
    // A file herdr could not read either gives herdr's defaults.
    let _w = common::world();
    unsafe { std::env::remove_var("HERDR_CONFIG_PATH") };
    let file = sw::extras::herdr_config();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "this is = not toml [").unwrap();
    assert!(read_config().is_empty());
    std::fs::write(&file, "[theme]\nname = \"nord\"\n").unwrap();
    assert_eq!(res(&read_config()).name, "nord");
    // Names that were special to the old board's objects (it guarded them so no key reached the objects every part of
    // the program shared) are plain keys here; a config made of them is herdr's defaults.
    let odd = cfg("[__proto__]\nstatus_indicators = \"dots\"\npolluted = true\n[constructor.prototype]\npolluted = true\n[a]\ntoString = \"t\"\n[[__proto__.list]]\npolluted = true\n[ui]\n\"__proto__\".accent = \"red\"\n");
    assert_eq!(odd["a"]["toString"].as_str(), Some("t"));
    assert!(res(&odd).symbols); assert_eq!(res(&odd).pal.accent, C::Rgb(0x89b4fa));
}

#[test]
fn the_review_page_takes_the_same_theme_and_follows_the_browser_where_herdr_follows_the_terminal() {
    assert_eq!(page("", "herdr"), ":root{color-scheme:dark;--bg:#181825;--ink:#cdd6f4;--muted:#a6adc8;--line:#45475a;--accent:#89b4fa;--well:#313244;--ok:#a6e3a1;--bad:#f38ba8}");
    // The board's own colours follow on the same page, for the web board.
    like(&page_theme_of(&toml::Table::new(), "herdr"), r"--bad:#f38ba8;--c-panel-bg:#181825;.*--c-review:#[0-9a-f]{6};.*\}$");
    like(&page("[theme]\nname = \"gruvbox-light\"\n[theme.custom]\ngreen = \"#010203\"\n", "auto"), r"^:root\{color-scheme:light;--bg:#fbf1c7;--ink:#3c3836;.*--ok:#010203;--bad:#9d0006\}$");
    like(&page("[theme]\nname = \"gruvbox\"\n", "light"), r"^:root\{color-scheme:light;--bg:#fbf1c7;");
    let both = page("[theme]\nname = \"nord\"\nauto_switch = true\nlight_name = \"one-light\"\n", "auto");
    like(&both, r"^:root\{color-scheme:dark;--bg:#2e3440;.*\}@media \(prefers-color-scheme:light\)\{:root\{color-scheme:light;--bg:#fafafa;.*\}\}$");
    // Colours given by name are kept on the page too; only the terminal theme's own colours give way to the browser's.
    like(&page("[theme.custom]\ngreen = \"red\"\naccent = \"lightmagenta\"\ntext = \"reset\"\n", "auto"), r"--ink:#cdd6f4;.*--accent:#ff00ff;.*--ok:#cd0000;--bad:#f38ba8\}$");
    like(&page("[theme]\nname = \"terminal\"\n[theme.custom]\ngreen = \"red\"\n", "auto"), r"--ok:#cd0000;--bad:#f38ba8\}@media.*--ok:#cd0000;--bad:#d20f39\}\}$");
    for (c, w) in [("[theme]\nname = \"terminal\"\n", "auto"), ("", "terminal"), ("[theme]\nname = \"nord\"\n", "terminal")] {
        like(&page(c, w), r"--bg:#181825;.*prefers-color-scheme:light.*--bg:#eff1f5;--ink:#4c4f69;"); // the terminal's colours mean the browser's
    }
}
