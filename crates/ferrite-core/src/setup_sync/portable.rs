//! Portable JSON: a setup file's strings that name a place in the home
//! directory, kept as [`HOME`] then `/` between names, so the same file on
//! Windows and on Linux reads, and hashes, the same. Only string values
//! are rewritten, and in them only the home directory where a path starts
//! (a whole value, or one in a command such as `node "C:\Users\me\a.js"`),
//! never inside a longer different name; a `~` the operator wrote is
//! theirs, and left as written. Every other byte of the file is kept as it
//! was, so a file written out again on the side it came from is the file
//! it was, but for the separators of a Windows path spelled with `/` or
//! `\\`, which come back as `\`.

/// What a portable string has in place of the home directory.
pub const HOME: &str = "<home>";

/// A home directory as one side spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelling {
    home: String,
    /// Windows: `\` between names, `/` and `\\` read as one too, and no
    /// case.
    windows: bool,
}

impl Spelling {
    pub fn new(home: &str, windows: bool) -> Self {
        let home = home.trim_end_matches(|c| c == '/' || (windows && c == '\\'));
        Self {
            home: home.to_string(),
            windows,
        }
    }

    /// `value` with each place in the home directory from [`HOME`].
    fn portable(&self, value: &str) -> Option<String> {
        let found = spots(value, |at| self.home_at(value, at));
        let windows = self.windows;
        replaced(value, &found, HOME, |rest| match windows {
            true => rest.replace("\\\\", "/").replace('\\', "/"),
            false => rest.to_string(),
        })
    }

    /// `value`, a portable one, in this side's home and separators.
    fn expanded(&self, value: &str) -> Option<String> {
        let found = spots(value, |at| {
            let end = at + value[at..].strip_prefix(HOME).map(|_| HOME.len())?;
            ends_name(&value[end..], false).then_some(end)
        });
        let windows = self.windows;
        replaced(value, &found, &self.home, |rest| match windows {
            true => rest.replace('/', "\\"),
            false => rest.to_string(),
        })
    }

    /// Where the home directory, spelled from `at` in `value`, ends.
    fn home_at(&self, value: &str, at: usize) -> Option<usize> {
        let mut end = at;
        for wanted in self.home.chars() {
            let rest = &value[end..];
            end += if self.windows && (wanted == '\\' || wanted == '/') {
                if rest.starts_with("\\\\") {
                    2
                } else if rest.starts_with(['\\', '/']) {
                    1
                } else {
                    return None;
                }
            } else {
                let found = rest.chars().next()?;
                let same = match self.windows {
                    true => found.to_lowercase().eq(wanted.to_lowercase()),
                    false => found == wanted,
                };
                if !same {
                    return None;
                }
                found.len_utf8()
            };
        }
        ends_name(&value[end..], self.windows).then_some(end)
    }
}

/// A character a file's name may run on with.
fn in_name(c: char) -> bool {
    c.is_alphanumeric() || "_.-~".contains(c)
}

/// Whether what follows a path's start ends its name there.
fn ends_name(after: &str, windows: bool) -> bool {
    after
        .chars()
        .next()
        .is_none_or(|c| c == '/' || (windows && c == '\\') || !in_name(c))
}

/// Each place in `value` a path starts and `found` says the home
/// directory is spelled to, as (start, end).
fn spots(value: &str, found: impl Fn(usize) -> Option<usize>) -> Vec<(usize, usize)> {
    let mut spots = Vec::new();
    let mut at = 0;
    while at < value.len() {
        let starts = value[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !in_name(c) && c != '/' && c != '\\');
        match starts.then(|| found(at)).flatten() {
            Some(end) => {
                spots.push((at, end));
                at = end.max(at + 1);
            }
            None => at += value[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    spots
}

/// `value` with `home` at each of `spots`, and the rest of each path's
/// separators as `separate` writes them. A value that is one path runs to
/// its end; a path in a command runs to the next space, quote or path.
fn replaced(value: &str, spots: &[(usize, usize)], home: &str, separate: impl Fn(&str) -> String) -> Option<String> {
    if spots.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(value.len());
    let mut from = 0;
    for (index, (start, end)) in spots.iter().enumerate() {
        let next = spots.get(index + 1).map_or(value.len(), |(start, _)| *start);
        let path_end = if spots.len() == 1 && *start == 0 {
            value.len()
        } else {
            value[*end..next]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map_or(next, |stop| end + stop)
        };
        out.push_str(&value[from..*start]);
        out.push_str(home);
        out.push_str(&separate(&value[*end..path_end]));
        from = path_end;
    }
    out.push_str(&value[from..]);
    Some(out)
}

/// `json` with each string value naming a place in `home` from [`HOME`].
pub fn portable(json: &[u8], home: &Spelling) -> Vec<u8> {
    rewrite(json, |value| home.portable(value))
}

/// `json`, portable, with each string value from [`HOME`] in `home`.
pub fn expand(json: &[u8], home: &Spelling) -> Vec<u8> {
    rewrite(json, |value| home.expanded(value))
}

/// `json` with each string value `change` answers for in its place,
/// written as JSON writes a string; keys and every other byte as they
/// were. Bytes that are not JSON come back as they were.
fn rewrite(json: &[u8], change: impl Fn(&str) -> Option<String>) -> Vec<u8> {
    let mut out = Vec::with_capacity(json.len());
    let mut at = 0;
    while let Some(start) = json[at..].iter().position(|byte| *byte == b'"').map(|found| at + found) {
        let Some(end) = string_end(json, start) else {
            break;
        };
        out.extend_from_slice(&json[at..start]);
        let literal = &json[start..=end];
        let key = json[end + 1..]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| *byte == b':');
        let changed = (!key)
            .then(|| serde_json::from_slice::<String>(literal).ok())
            .flatten()
            .and_then(|value| change(&value))
            .and_then(|value| serde_json::to_string(&value).ok());
        match changed {
            Some(written) => out.extend_from_slice(written.as_bytes()),
            None => out.extend_from_slice(literal),
        }
        at = end + 1;
    }
    out.extend_from_slice(&json[at..]);
    out
}

/// Where the string that opens at `start` closes.
fn string_end(json: &[u8], start: usize) -> Option<usize> {
    let mut at = start + 1;
    while at < json.len() {
        match json[at] {
            b'\\' => at += 2,
            b'"' => return Some(at),
            _ => at += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows() -> Spelling {
        Spelling::new(r"C:\Users\josep", true)
    }

    fn linux() -> Spelling {
        Spelling::new("/home/josep", false)
    }

    const PLUGINS: &str = r#"{
  "version": 2,
  "plugins": {
    "pstack@cursor-plugins": [
      {
        "scope": "user",
        "installPath": "C:\\Users\\josep\\.claude\\plugins\\cache\\cursor-plugins\\pstack\\0.15.47",
        "version": "0.15.47",
        "note": "C:\\Users\\joseph\\elsewhere"
      }
    ]
  },
  "C:\\Users\\josep\\as-a-key": "C:\\Users\\josep"
}"#;

    #[test]
    fn a_plugin_path_goes_from_windows_to_linux_and_back_unchanged() {
        let carried = portable(PLUGINS.as_bytes(), &windows());
        let text = String::from_utf8(carried.clone()).unwrap();
        assert!(text.contains(r#""installPath": "<home>/.claude/plugins/cache/cursor-plugins/pstack/0.15.47""#), "{text}");
        assert!(text.contains(r#""note": "C:\\Users\\joseph\\elsewhere""#), "a longer name is not the home");
        assert!(text.contains(r#""C:\\Users\\josep\\as-a-key": "<home>""#), "a key is kept, its value is not");

        let on_linux = expand(&carried, &linux());
        let text = String::from_utf8(on_linux.clone()).unwrap();
        assert!(text.contains(r#""installPath": "/home/josep/.claude/plugins/cache/cursor-plugins/pstack/0.15.47""#), "{text}");
        assert_eq!(portable(&on_linux, &linux()), carried, "Linux reads it as the same file");

        let back = expand(&portable(&on_linux, &linux()), &windows());
        assert_eq!(String::from_utf8(back).unwrap(), PLUGINS);
    }

    #[test]
    fn windows_reads_its_home_in_any_case_and_either_separator() {
        let json = br#"["c:\\users\\JOSEP\\a", "C:/Users/josep/b", "C:\\Users\\josep"]"#;
        assert_eq!(portable(json, &windows()), br#"["<home>/a", "<home>/b", "<home>"]"#.to_vec());
    }

    #[test]
    fn only_where_a_path_starts_is_the_home_rewritten() {
        let json = br#"{"a": "/home/josep", "b": "/home/josep/x y", "c": "/home/josepine", "d": "x /home/josep", "e": ["/home/josep/l"], "f": 3, "g": "say \"hi\" /home/josep", "h": "~/bin/status", "i": "/mnt/home/josep/x", "j": "--config=/home/josep/c"}"#;
        let carried = String::from_utf8(portable(json, &linux())).unwrap();
        assert_eq!(
            carried,
            r#"{"a": "<home>", "b": "<home>/x y", "c": "/home/josepine", "d": "x <home>", "e": ["<home>/l"], "f": 3, "g": "say \"hi\" <home>", "h": "~/bin/status", "i": "/mnt/home/josep/x", "j": "--config=<home>/c"}"#
        );
        let on_windows = String::from_utf8(expand(carried.as_bytes(), &windows())).unwrap();
        assert!(on_windows.contains(r#""h": "~/bin/status""#), "a ~ the operator wrote is theirs: {on_windows}");
        assert!(on_windows.contains(r#""j": "--config=C:\\Users\\josep\\c""#), "{on_windows}");
    }

    #[test]
    fn a_hook_command_finds_the_home_anywhere_in_it_in_either_spelling() {
        let hooks = [
            r#"{"command": "node \"C:\\Users\\josep\\.claude\\hooks\\a.js\" --x"}"#,
            r#"{"command": "node \"C:/Users/josep/.claude/hooks/a.js\" --x"}"#,
            r#"{"command": "node \"C:\\\\Users\\\\josep\\\\.claude\\\\hooks\\\\a.js\" --x"}"#,
        ];
        for hook in hooks {
            let carried = String::from_utf8(portable(hook.as_bytes(), &windows())).unwrap();
            assert_eq!(carried, r#"{"command": "node \"<home>/.claude/hooks/a.js\" --x"}"#, "{hook}");
            let on_linux = String::from_utf8(expand(carried.as_bytes(), &linux())).unwrap();
            assert_eq!(on_linux, r#"{"command": "node \"/home/josep/.claude/hooks/a.js\" --x"}"#);
        }
        let back = String::from_utf8(expand(&portable(hooks[0].as_bytes(), &windows()), &windows())).unwrap();
        assert_eq!(back, hooks[0], "a Windows hook comes back as it was");
        let two = br#"{"command": "C:\\Users\\josep\\bin\\run.exe C:\\Users\\josep\\a C:\\Users\\josephine\\b"}"#;
        let carried = String::from_utf8(portable(two, &windows())).unwrap();
        assert_eq!(carried, r#"{"command": "<home>/bin/run.exe <home>/a C:\\Users\\josephine\\b"}"#);
    }

    #[test]
    fn what_is_not_json_comes_back_as_it_was() {
        let torn = br#"{"a": "/home/josep/x", "b": "/home/jo"#;
        assert_eq!(portable(torn, &linux()), br#"{"a": "<home>/x", "b": "/home/jo"#.to_vec());
    }
}
