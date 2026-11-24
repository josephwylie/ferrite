//! Portable JSON: a setup file's strings that name a place in the home
//! directory, kept as [`HOME`] then `/` between names, so the same file on
//! Windows and on Linux reads, and hashes, the same. Only string values
//! are rewritten, and only the ones that start with the home directory; a
//! `~` the operator wrote is theirs, and left as written. Every other byte
//! of the file is kept as it was, so a file written out again on the side
//! it came from is the file it was.

/// What a portable string has in place of the home directory.
pub const HOME: &str = "<home>";

/// A home directory as one side spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelling {
    home: String,
    /// Windows: `\` between names, `/` read as one too, and no case.
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

    /// `value` from [`HOME`], when it is the home directory or inside it.
    fn portable(&self, value: &str) -> Option<String> {
        let head = value.get(..self.home.len())?;
        let same = if self.windows {
            let fold = |text: &str| text.replace('\\', "/").to_lowercase();
            fold(head) == fold(&self.home)
        } else {
            head == self.home
        };
        let rest = &value[self.home.len()..];
        let separated = rest.is_empty() || rest.starts_with('/') || (self.windows && rest.starts_with('\\'));
        (same && separated).then(|| {
            let rest = if self.windows { rest.replace('\\', "/") } else { rest.to_string() };
            format!("{HOME}{rest}")
        })
    }

    /// `value`, a portable one, in this side's home and separators.
    fn expanded(&self, value: &str) -> Option<String> {
        let rest = value.strip_prefix(HOME)?;
        if !(rest.is_empty() || rest.starts_with('/')) {
            return None;
        }
        let rest = if self.windows { rest.replace('/', "\\") } else { rest.to_string() };
        Some(format!("{}{rest}", self.home))
    }
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
    fn only_strings_that_start_with_the_home_are_rewritten() {
        let json = br#"{"a": "/home/josep", "b": "/home/josep/x y", "c": "/home/josepine", "d": "x /home/josep", "e": ["/home/josep/l"], "f": 3, "g": "say \"hi\" /home/josep", "h": "~/bin/status"}"#;
        let carried = String::from_utf8(portable(json, &linux())).unwrap();
        assert_eq!(
            carried,
            r#"{"a": "<home>", "b": "<home>/x y", "c": "/home/josepine", "d": "x /home/josep", "e": ["<home>/l"], "f": 3, "g": "say \"hi\" /home/josep", "h": "~/bin/status"}"#
        );
        let on_windows = String::from_utf8(expand(carried.as_bytes(), &windows())).unwrap();
        assert!(on_windows.contains(r#""h": "~/bin/status""#), "a ~ the operator wrote is theirs: {on_windows}");
    }

    #[test]
    fn what_is_not_json_comes_back_as_it_was() {
        let torn = br#"{"a": "/home/josep/x", "b": "/home/jo"#;
        assert_eq!(portable(torn, &linux()), br#"{"a": "<home>/x", "b": "/home/jo"#.to_vec());
    }
}
