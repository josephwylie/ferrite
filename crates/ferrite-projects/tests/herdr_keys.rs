//! The key that opens the board: prefix+a is offered, and a key herdr or the user's config already uses is never taken.
use sw::herdr::keys::{built_in, check, parse_defaults, suggest, used, Defaults, OFFERED};

const DEFAULT_CONFIG: &str = "[update]\n# channel = \"stable\"\n\n[keys]\n# prefix = \"ctrl+b\"\n# workspace_picker = \"prefix+w\"\n# open_worktree = \"\"    # optional\n# switch_tab = \"prefix+1..9\"\n# split_horizontal = \"prefix+minus\"\n# reload_config = \"prefix+shift+r\"\n# [[keys.command]]\n# key = \"prefix+alt+g\"\n# type = \"popup\"\n# command = \"lazygit\"\n\n[ui]\n# other = \"prefix+a\"\n";
const YOURS: &str = "your herdr config";
fn defs() -> Defaults { parse_defaults(DEFAULT_CONFIG).unwrap() }
fn s(k: &str, n: &str) -> (String, String) { (k.into(), n.into()) }
fn ok(key: &str, text: &str, d: &Defaults) -> bool { check(key, text, d, YOURS).is_ok() }
fn msg(key: &str, text: &str, d: &Defaults) -> String { check(key, text, d, YOURS).err().unwrap_or_default() }
fn has(text: &str, re: &str) -> bool { regex::Regex::new(re).unwrap().is_match(text) }

#[test]
fn prefix_a_is_what_the_first_run_offers_and_the_next_free_key_when_the_config_already_binds_it() {
    let d = defs();
    let want: Defaults = [("prefix", "ctrl+b"), ("workspace_picker", "prefix+w"), ("switch_tab", "prefix+1..9"), ("split_horizontal", "prefix+minus"), ("reload_config", "prefix+shift+r")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(d, want, "not the commented example, not another table");
    assert_eq!(parse_defaults("nothing here"), None);
    assert_eq!(suggest("", &d), s("prefix+a", ""));
    assert_eq!(suggest("", &built_in()), s("prefix+a", ""), "herdr could not be asked: its 0.9.3 keys are known anyway");
    assert_eq!(suggest("[[keys.command]]\nkey = 'Prefix+A'\ntype = \"popup\"\ncommand = \"lazygit\"\n", &d), s("prefix+y", "prefix+a is already bound in your config (lazygit), so this is the next free key."));
    assert_eq!(suggest("[keys]\nnew_tab = \"prefix+a\"\nnext_agent = \"prefix+y\"\n", &d), s("prefix+u", "prefix+a is already bound in your config (new tab), so this is the next free key."));
    // however the config writes a binding: a list of keys, a command with several keys, single quotes
    assert_eq!(suggest("[keys]\nnew_tab = [\"prefix+c\", \"prefix+a\"]\n", &d).0, "prefix+y");
    assert_eq!(suggest("[keys]\nprefix = [\"ctrl+b\", \"prefix+a\"]\nextra_prefixes = [\"prefix+y\"]\n", &d).0, "prefix+u");
    assert_eq!(suggest("[[keys.command]]\nkey = [\"prefix+g\", \"prefix+a\"]\ntype = \"popup\"\ncommand = \"lazygit\"\n", &d), s("prefix+y", "prefix+a is already bound in your config (lazygit), so this is the next free key."));
    assert_eq!(suggest("not toml [\nnew_tab = [\"prefix+c\", 'prefix+a']\n", &d).0, "prefix+y");
    let all = OFFERED.iter().enumerate().map(|(i, k)| format!("a{i} = \"{k}\"")).collect::<Vec<_>>().join("\n");
    assert_eq!(suggest(&format!("[keys]\n{all}\n"), &d).0, "", "nothing left to offer: the user types one");
    for k in OFFERED { assert!(ok(k, "", &built_in()), "{k}: none of the offered keys is herdr's own"); }
}

#[test]
fn a_key_herdr_or_the_users_config_uses_is_refused_with_what_it_does_today() {
    let d = defs();
    assert_eq!(check("prefix+w", "", &d, YOURS), Err("herdr itself uses prefix+w (workspace picker): the board would switch that off. Choose another.".into()));
    assert!(!ok("prefix+w", "", &built_in()), "also when herdr cannot be asked for its defaults");
    assert!(!ok("Prefix+W", "", &d));
    assert!(!ok("prefix+5", "", &d), "prefix+1..9 is nine keys");
    assert!(!ok("prefix+-", "", &d), "another spelling of prefix+minus");
    assert!(!ok("prefix+r+shift", "", &d), "the parts in another order");
    assert!(!ok("ctrl+b", "", &d), "the prefix key itself");
    // keys herdr binds but does not print in its default config are known all the same, whether or not herdr answers
    let real = built_in();
    assert_eq!((real["swap_pane_left"].as_str(), real["copy_mode"].as_str()), ("prefix+shift+h", "prefix+["));
    assert_eq!(check("prefix+shift+h", "", &real, YOURS), Err("herdr itself uses prefix+shift+h (swap pane left): the board would switch that off. Choose another.".into()));
    for k in ["prefix+shift+j", "prefix+shift+k", "prefix+shift+l", "prefix+w", "prefix+c", "prefix+tab"] { assert!(!ok(k, "", &real), "{k}"); }
    // the older [keys.indexed] table binds nine keys, and a list binds each key in it
    assert!(has(&msg("ctrl+3", "[keys.indexed]\ntabs = \"ctrl\"\n", &d), r"ctrl\+3 is already bound in your herdr config \(switch tabs\)"));
    assert!(!ok("alt+shift+9", "[keys.indexed]\nworkspaces = \"shift+alt\"\n", &d));
    assert!(has(&msg("prefix+t", "[keys]\nnew_tab = [\"prefix+c\", \"prefix+t\"]\n", &d), r"already bound in your herdr config \(new tab\)"));
    assert!(!ok("prefix+f", "[keys]\nfullscreen = [\"prefix+f\"]\n", &real)); assert!(ok("prefix+z", "[keys]\nfullscreen = [\"prefix+f\"]\n", &real), "zoom's older name");
    assert!(msg("prefix w", "", &d).contains("is not a key herdr understands"));
    assert!(msg("prefix+", "", &d).contains("is not a key herdr understands"));
    let conf = "[keys]\nworkspace_picker = \"prefix+o\"\n\n[[keys.command]]\nkey = \"prefix+g\"\ntype = \"popup\"\ncommand = \"lazygit\"\ndescription = \"git\"\n";
    assert_eq!(check("prefix+g", conf, &d, "~/.config/herdr/config.toml"), Err("prefix+g is already bound in ~/.config/herdr/config.toml (git): the board would replace it. Choose another.".into()));
    assert!(has(&msg("prefix+o", conf, &d), r"already bound in your herdr config \(workspace picker\)"));
    assert!(ok("prefix+w", conf, &d), "the user moved herdr's action off it: the key is free");
    assert!(ok("prefix+a", conf, &d));
    // the board's own binding is not "taken", and a config that cannot be read still has its keys found
    assert!(!used("[[keys.command]]\nkey = \"prefix+a\"\ntype = \"plugin_action\"\ncommand = \"herdr-projects.open\"\n", &d).contains_key("prefix+a"));
    assert!(!ok("prefix+g", "this is = not toml [\nkey = \"prefix+g\"\n", &d));
    assert!(!ok("prefix+w", "this is = not toml [\nworkspace_picker = \"\"\n", &d));
}
