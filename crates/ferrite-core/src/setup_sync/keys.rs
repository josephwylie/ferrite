//! A chosen file's keys (ADR 0050): each key the table names is an entry of
//! its own, its value carried as portable JSON in a canonical form (object
//! keys sorted, so a tool that reorders its file changes nothing), and one
//! is written back at a time with every other byte of the file as it was: a
//! JSON file by the span of that one value, a TOML file through
//! `toml_edit`, which keeps its comments and order. A value that names a
//! place of this computer outside the home is this computer's alone.

use std::io;
use std::ops::Range;

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, TableLike};

use super::portable::{self, Spelling};
use super::{Format, Key, Shape, OTHERS};

/// One key as a side holds it.
pub(super) struct Listed {
    /// Its path in the file: `<name>`, or `<name>/<entry>`, each name
    /// spelled as a path's name may be ([`encode`]).
    pub key: String,
    /// Its value as it is carried.
    pub carried: Vec<u8>,
    /// It names a place of this computer outside the home: it never moves.
    pub own: bool,
}

/// Each key of `raw`, a file of `format`, that `keys` names.
pub(super) fn list(format: Format, keys: &[Key], raw: &[u8], home: &Spelling) -> io::Result<Vec<Listed>> {
    let mut listed = Vec::new();
    for (name, value) in read(format, raw)? {
        let (Some(key), Some(spelled)) = (named(keys, &name), encode(&name)) else {
            continue;
        };
        match (key.shape, value) {
            (Shape::Whole, value) => listed.push(carried(spelled, &value, home)),
            (Shape::Each, Value::Object(entries)) => {
                for (entry, value) in entries {
                    if let Some(entry) = encode(&entry) {
                        listed.push(carried(format!("{spelled}/{entry}"), &value, home));
                    }
                }
            }
            (Shape::Each, _) => {}
        }
    }
    Ok(listed)
}

/// Whether `key` is a key path `keys` names.
pub(super) fn names(keys: &[Key], key: &str) -> bool {
    parse(keys, key).is_ok()
}

/// `raw`, or a new file when there is none, with `key` set to `carried`,
/// its value as a sync carries it; every other byte as it was.
pub(super) fn set(format: Format, keys: &[Key], raw: Option<&[u8]>, key: &str, carried: &[u8], home: &Spelling) -> io::Result<Vec<u8>> {
    let at = parse(keys, key)?;
    let value: Value = serde_json::from_slice(&portable::expand(carried, home)).map_err(not_json)?;
    match format {
        Format::Json => json::set(raw.unwrap_or(b"{}"), &at, value),
        Format::Toml => toml::set(raw.unwrap_or_default(), &at, &value),
    }
}

/// `raw` without `key`; `None` when it has no such key.
pub(super) fn remove(format: Format, keys: &[Key], raw: &[u8], key: &str) -> io::Result<Option<Vec<u8>>> {
    let at = parse(keys, key)?;
    match format {
        Format::Json => json::remove(raw, &at),
        Format::Toml => toml::remove(raw, &at),
    }
}

/// A key path, its names as the file spells them.
struct At {
    name: String,
    entry: Option<String>,
}

fn parse(keys: &[Key], key: &str) -> io::Result<At> {
    let (name, entry) = match key.split_once('/') {
        Some((name, entry)) => (decode(name), Some(decode(entry))),
        None => (decode(key), None),
    };
    let shape = named(keys, &name).map(|key| key.shape).filter(|_| !name.is_empty());
    match (shape, entry.filter(|entry| !entry.is_empty())) {
        (Some(Shape::Whole), None) => Ok(At { name, entry: None }),
        (Some(Shape::Each), Some(entry)) if key.matches('/').count() == 1 => Ok(At { name, entry: Some(entry) }),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("`{key}` is not a key Setup sync keeps"))),
    }
}

/// The key of `keys` that the file's top-level key `name` is.
fn named<'a>(keys: &'a [Key], name: &str) -> Option<&'a Key> {
    keys.iter()
        .find(|key| key.name == name)
        .or_else(|| keys.iter().find(|key| key.name == OTHERS))
}

fn carried(key: String, value: &Value, home: &Spelling) -> Listed {
    let text = serde_json::to_vec_pretty(&canonical(value)).unwrap_or_default();
    let carried = portable::portable(&text, home);
    let own = serde_json::from_slice(&carried).is_ok_and(|value| rooted(&value));
    Listed { key, carried, own }
}

/// `value` with each object's keys in order.
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|(one, _), (other, _)| one.cmp(other));
            Value::Object(entries.into_iter().map(|(key, value)| (key.clone(), canonical(value))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        value => value.clone(),
    }
}

/// Whether a string in `value`, or a word of one, is an absolute path:
/// one in the home is carried as `<home>/…` already, so this is a place of
/// this computer's own.
fn rooted(value: &Value) -> bool {
    let absolute = |word: &str| {
        let bytes = word.as_bytes();
        word.starts_with('/')
            || word.starts_with("\\\\")
            || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
    };
    match value {
        Value::String(text) => text.split(|c: char| c.is_whitespace() || "\"'=".contains(c)).any(absolute),
        Value::Array(items) => items.iter().any(rooted),
        Value::Object(map) => map.values().any(rooted),
        _ => false,
    }
}

/// A key's name as a name of a path: what a path's name cannot hold, or
/// holds as a separator, is `%` and its hex. `None` for an empty name.
fn encode(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    if name.chars().all(|c| c == '.') {
        return Some(name.replace('.', "%2E"));
    }
    Some(
        name.chars()
            .map(|c| match c {
                '%' | '/' | '\\' | ':' | '\0'..='\x1f' | '\x7f' => format!("%{:02X}", c as u32),
                c => c.to_string(),
            })
            .collect(),
    )
}

fn decode(spelled: &str) -> String {
    let mut bytes = Vec::with_capacity(spelled.len());
    let mut rest = spelled.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        let hex = after.get(..2).and_then(|hex| std::str::from_utf8(hex).ok()).and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (byte, hex) {
            (b'%', Some(decoded)) => {
                bytes.push(decoded);
                rest = &after[2..];
            }
            (byte, _) => {
                bytes.push(byte);
                rest = after;
            }
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The file's top-level keys and their values.
fn read(format: Format, raw: &[u8]) -> io::Result<Map<String, Value>> {
    match format {
        Format::Json => match serde_json::from_slice(raw).map_err(not_json)? {
            Value::Object(document) => Ok(document),
            _ => Err(invalid("it is not a JSON object")),
        },
        Format::Toml => Ok(toml::document(raw)?.iter().filter_map(|(key, item)| Some((key.to_string(), toml::json(item)?))).collect()),
    }
}

fn not_json(error: serde_json::Error) -> io::Error {
    invalid(&format!("it is not JSON ({error})"))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

/// A JSON file edited by the span of one value, so every other byte stays:
/// its spacing, its order, how each string is escaped.
mod json {
    use super::*;

    pub(super) fn set(text: &[u8], at: &At, value: Value) -> io::Result<Vec<u8>> {
        let root = Object::at(text, space(text, 0))?;
        match &at.entry {
            None => Ok(root.put(text, &at.name, &value)),
            Some(entry) => match root.member(&at.name) {
                Some(member) if text.get(member.value.start) == Some(&b'{') => {
                    Ok(Object::at(text, member.value.start)?.put(text, entry, &value))
                }
                Some(_) => Err(invalid(&format!("its `{}` is not an object", at.name))),
                None => Ok(root.put(text, &at.name, &Value::Object(Map::from_iter([(entry.clone(), value)])))),
            },
        }
    }

    pub(super) fn remove(text: &[u8], at: &At) -> io::Result<Option<Vec<u8>>> {
        let root = Object::at(text, space(text, 0))?;
        Ok(match &at.entry {
            None => root.without(text, &at.name),
            Some(entry) => match root.member(&at.name) {
                Some(member) if text.get(member.value.start) == Some(&b'{') => {
                    Object::at(text, member.value.start)?.without(text, entry)
                }
                _ => None,
            },
        })
    }

    /// An object in the text: where it opens and closes, and its members.
    struct Object {
        open: usize,
        close: usize,
        members: Vec<Member>,
    }

    struct Member {
        key: String,
        /// From the key's opening quote.
        start: usize,
        /// What stands between the key and its value: `": "`.
        colon: Range<usize>,
        value: Range<usize>,
    }

    impl Object {
        fn at(text: &[u8], open: usize) -> io::Result<Self> {
            if text.get(open) != Some(&b'{') {
                return Err(invalid("it is not a JSON object"));
            }
            let mut members = Vec::new();
            let mut at = space(text, open + 1);
            if text.get(at) == Some(&b'}') {
                return Ok(Self { open, close: at, members });
            }
            loop {
                let key_end = string_end(text, at)?;
                let key = serde_json::from_slice(&text[at..key_end]).map_err(not_json)?;
                let colon = space(text, key_end);
                if text.get(colon) != Some(&b':') {
                    return Err(torn());
                }
                let value = space(text, colon + 1);
                let value_end = value_end(text, value)?;
                members.push(Member { key, start: at, colon: key_end..value, value: value..value_end });
                let next = space(text, value_end);
                match text.get(next) {
                    Some(b',') => at = space(text, next + 1),
                    Some(b'}') => return Ok(Self { open, close: next, members }),
                    _ => return Err(torn()),
                }
            }
        }

        fn member(&self, key: &str) -> Option<&Member> {
            self.members.iter().find(|member| member.key == key)
        }

        /// The text with `key` holding `value`, in its place or after the
        /// last member, spaced as that one is.
        fn put(&self, text: &[u8], key: &str, value: &Value) -> Vec<u8> {
            let style = Style::of(text, self);
            if let Some(member) = self.member(key) {
                let written = style.render(value, &indent(text, member.start));
                return splice(text, member.value.clone(), &written);
            }
            match self.members.last() {
                Some(last) => {
                    let gap = match &text[before_space(text, last.start)..last.start] {
                        b"" => " ".to_string(),
                        gap => String::from_utf8_lossy(gap).into_owned(),
                    };
                    let quoted = serde_json::to_string(key).unwrap_or_default();
                    let colon = String::from_utf8_lossy(&text[last.colon.clone()]).into_owned();
                    let written = style.render(value, &indent(text, last.start));
                    splice(text, last.value.end..last.value.end, &format!(",{gap}{quoted}{colon}{written}"))
                }
                None => {
                    let object = Value::Object(Map::from_iter([(key.to_string(), value.clone())]));
                    splice(text, self.open..self.close + 1, &style.render(&object, &indent(text, self.open)))
                }
            }
        }

        /// The text without `key`, its comma with it; `None` without one.
        fn without(&self, text: &[u8], key: &str) -> Option<Vec<u8>> {
            let at = self.members.iter().position(|member| member.key == key)?;
            let gone = match (at.checked_sub(1), self.members.get(at + 1)) {
                (_, Some(next)) => self.members[at].start..next.start,
                (Some(before), None) => self.members[before].value.end..self.members[at].value.end,
                (None, None) => self.open + 1..self.close,
            };
            Some(splice(text, gone, ""))
        }
    }

    /// How the file writes what is added: on one line where the object is,
    /// else indented, with the file's own line ends.
    struct Style {
        one_line: bool,
        newline: &'static str,
    }

    impl Style {
        fn of(text: &[u8], object: &Object) -> Self {
            Self {
                one_line: !text[object.open..object.close].contains(&b'\n'),
                newline: if text.windows(2).any(|pair| pair == b"\r\n") { "\r\n" } else { "\n" },
            }
        }

        fn render(&self, value: &Value, indent: &str) -> String {
            if self.one_line {
                return serde_json::to_string(value).unwrap_or_default();
            }
            let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
            pretty.replace('\n', &format!("{}{indent}", self.newline))
        }
    }

    fn splice(text: &[u8], range: Range<usize>, with: &str) -> Vec<u8> {
        [&text[..range.start], with.as_bytes(), &text[range.end..]].concat()
    }

    fn space(text: &[u8], mut at: usize) -> usize {
        while text.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        at
    }

    fn before_space(text: &[u8], mut at: usize) -> usize {
        while at > 0 && text[at - 1].is_ascii_whitespace() {
            at -= 1;
        }
        at
    }

    /// The spaces the line holding `at` begins with.
    fn indent(text: &[u8], at: usize) -> String {
        let line = text[..at].iter().rposition(|byte| *byte == b'\n').map_or(0, |newline| newline + 1);
        let end = line + text[line..].iter().take_while(|byte| matches!(byte, b' ' | b'\t')).count();
        String::from_utf8_lossy(&text[line..end]).into_owned()
    }

    /// Just past the string that opens at `at`.
    fn string_end(text: &[u8], at: usize) -> io::Result<usize> {
        if text.get(at) != Some(&b'"') {
            return Err(torn());
        }
        let mut at = at + 1;
        while let Some(byte) = text.get(at) {
            match byte {
                b'\\' => at += 2,
                b'"' => return Ok(at + 1),
                _ => at += 1,
            }
        }
        Err(torn())
    }

    /// Just past the value that starts at `at`.
    fn value_end(text: &[u8], at: usize) -> io::Result<usize> {
        match text.get(at) {
            Some(b'"') => string_end(text, at),
            Some(b'{' | b'[') => {
                let mut depth = 0usize;
                let mut at = at;
                while let Some(byte) = text.get(at) {
                    match byte {
                        b'"' => {
                            at = string_end(text, at)?;
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return Ok(at + 1);
                            }
                        }
                        _ => {}
                    }
                    at += 1;
                }
                Err(torn())
            }
            Some(_) => Ok(at + text[at..].iter().take_while(|byte| !b",}] \t\r\n".contains(byte)).count()),
            None => Err(torn()),
        }
    }

    fn torn() -> io::Error {
        invalid("it is not JSON")
    }
}

/// A TOML file edited through `toml_edit`: a value replaced keeps the
/// comments around it and its place, a table its header's.
mod toml {
    use super::*;

    pub(super) fn document(raw: &[u8]) -> io::Result<DocumentMut> {
        let text = std::str::from_utf8(raw).map_err(|_| invalid("it is not text"))?;
        text.parse().map_err(|error: toml_edit::TomlError| {
            let first = error.to_string().lines().next().unwrap_or_default().to_string();
            invalid(&format!("it is not TOML ({first})"))
        })
    }

    pub(super) fn set(raw: &[u8], at: &At, value: &Value) -> io::Result<Vec<u8>> {
        let mut document = document(raw)?;
        let root = document.as_table_mut();
        match &at.entry {
            None => put(root, &at.name, value, false)?,
            Some(entry) => {
                let parent = root.entry(&at.name).or_insert_with(|| {
                    let mut table = toml_edit::Table::new();
                    table.set_implicit(true);
                    Item::Table(table)
                });
                let standard = parent.is_table();
                let parent = parent
                    .as_table_like_mut()
                    .ok_or_else(|| invalid(&format!("its `{}` is not a table", at.name)))?;
                put(parent, entry, value, standard)?;
            }
        }
        Ok(document.to_string().into_bytes())
    }

    pub(super) fn remove(raw: &[u8], at: &At) -> io::Result<Option<Vec<u8>>> {
        let mut document = document(raw)?;
        let root = document.as_table_mut();
        let removed = match &at.entry {
            None => root.remove(&at.name),
            Some(entry) => root.get_mut(&at.name).and_then(Item::as_table_like_mut).and_then(|parent| parent.remove(entry)),
        };
        Ok(removed.map(|_| document.to_string().into_bytes()))
    }

    /// `parent`'s `key` holding `value`: a table where one was, or where a
    /// new entry of a `standard` table goes, as Codex writes them; the
    /// comments and place of what was there kept.
    fn put(parent: &mut dyn TableLike, key: &str, value: &Value, standard: bool) -> io::Result<()> {
        let made = match (value, parent.get(key)) {
            (Value::Object(map), Some(Item::Table(old))) => {
                let mut table = table(map)?;
                *table.decor_mut() = old.decor().clone();
                table.set_position(old.position());
                Item::Table(table)
            }
            (Value::Object(map), None) if standard => Item::Table(table(map)?),
            (value, Some(Item::Value(old))) => {
                let mut made = to_toml(value)?;
                *made.decor_mut() = old.decor().clone();
                Item::Value(made)
            }
            (value, _) => Item::Value(to_toml(value)?),
        };
        match parent.get_mut(key) {
            Some(there) => *there = made,
            None => {
                parent.insert(key, made);
            }
        }
        Ok(())
    }

    fn table(map: &Map<String, Value>) -> io::Result<toml_edit::Table> {
        let mut table = toml_edit::Table::new();
        for (key, value) in map {
            table.insert(key, Item::Value(to_toml(value)?));
        }
        Ok(table)
    }

    fn to_toml(value: &Value) -> io::Result<toml_edit::Value> {
        Ok(match value {
            Value::Null => return Err(invalid("TOML holds no null")),
            Value::Bool(on) => (*on).into(),
            Value::Number(number) => match number.as_i64() {
                Some(whole) => whole.into(),
                None => number.as_f64().unwrap_or_default().into(),
            },
            Value::String(text) => text.as_str().into(),
            Value::Array(items) => {
                let items: io::Result<Vec<toml_edit::Value>> = items.iter().map(to_toml).collect();
                toml_edit::Value::Array(items?.into_iter().collect())
            }
            Value::Object(map) => {
                let mut table = toml_edit::InlineTable::new();
                for (key, value) in map {
                    table.insert(key, to_toml(value)?);
                }
                toml_edit::Value::InlineTable(table)
            }
        })
    }

    /// `item` as JSON; `None` for what holds nothing.
    pub(super) fn json(item: &Item) -> Option<Value> {
        match item {
            Item::None => None,
            Item::Value(value) => Some(value_json(value)),
            Item::Table(table) => Some(Value::Object(table.iter().filter_map(|(key, item)| Some((key.to_string(), json(item)?))).collect())),
            Item::ArrayOfTables(tables) => Some(Value::Array(
                tables
                    .iter()
                    .map(|table| Value::Object(table.iter().filter_map(|(key, item)| Some((key.to_string(), json(item)?))).collect()))
                    .collect(),
            )),
        }
    }

    fn value_json(value: &toml_edit::Value) -> Value {
        match value {
            toml_edit::Value::String(text) => Value::String(text.value().clone()),
            toml_edit::Value::Integer(whole) => Value::from(*whole.value()),
            toml_edit::Value::Float(float) => serde_json::Number::from_f64(*float.value()).map_or(Value::Null, Value::Number),
            toml_edit::Value::Boolean(on) => Value::Bool(*on.value()),
            toml_edit::Value::Datetime(when) => Value::String(when.value().to_string()),
            toml_edit::Value::Array(items) => Value::Array(items.iter().map(value_json).collect()),
            toml_edit::Value::InlineTable(table) => {
                Value::Object(table.iter().map(|(key, value)| (key.to_string(), value_json(value))).collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup_sync::{item_at, Source};

    fn keys_of(path: &str) -> (Format, &'static [Key]) {
        match item_at(path).map(|item| item.source) {
            Some(Source::Keys { format, keys }) => (format, keys),
            _ => panic!("{path} is no chosen file"),
        }
    }

    fn linux() -> Spelling {
        Spelling::new("/home/me", false)
    }

    fn value(listed: &[Listed], key: &str) -> Option<Value> {
        listed.iter().find(|listed| listed.key == key).map(|listed| serde_json::from_slice(&listed.carried).unwrap())
    }

    #[test]
    fn a_json_key_is_written_with_every_other_byte_of_the_file_as_it_was() {
        let (format, keys) = keys_of(".claude/settings.json");
        let file = "{\n    \"$schema\": \"https://json.schemastore.org/claude-code-settings.json\",\n    \"model\": \"opus\",\n    \"enabledPlugins\": {\n        \"pstack@cursor-plugins\": true\n    },\n    \"env\": {\"TERM\": \"caf\\u00e9\"}\n}\n";
        let set = |raw: &str, key: &str, value: &str| String::from_utf8(super::set(format, keys, Some(raw.as_bytes()), key, value.as_bytes(), &linux()).unwrap()).unwrap();

        let model = set(file, "model", "\"sonnet\"");
        assert_eq!(model, file.replace("\"opus\"", "\"sonnet\""), "only the value moved");
        let added = set(file, "enabledPlugins/sentry@cursor-plugins", "true");
        assert_eq!(
            added,
            file.replace("\"pstack@cursor-plugins\": true\n", "\"pstack@cursor-plugins\": true,\n        \"sentry@cursor-plugins\": true\n")
        );
        let theme = set(file, "theme", "{\"name\": \"dark\", \"size\": 2}");
        assert!(theme.starts_with(&file[..file.len() - 3]), "{theme}");
        assert!(theme.ends_with("},\n    \"theme\": {\n      \"name\": \"dark\",\n      \"size\": 2\n    }\n}\n"), "{theme}");
        assert!(theme.contains("caf\\u00e9"), "a string is escaped as it was");

        let fresh = set("{}", "extraKnownMarketplaces/cursor-plugins", "{\"source\": {\"repo\": \"cursor/plugins\"}}");
        let fresh: Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(fresh["extraKnownMarketplaces"]["cursor-plugins"]["source"]["repo"], "cursor/plugins");

        let removed = |raw: &str, key: &str| super::remove(format, keys, raw.as_bytes(), key).unwrap().map(|bytes| String::from_utf8(bytes).unwrap());
        assert_eq!(removed(&added, "enabledPlugins/sentry@cursor-plugins").as_deref(), Some(file));
        assert_eq!(removed(file, "model").as_deref(), Some(file.replace("    \"model\": \"opus\",\n", "").as_str()));
        assert_eq!(removed(file, "enabledPlugins/nothing"), None);
        let one_line = r#"{"model": "opus", "enabledPlugins": {"a@m": true}}"#;
        assert_eq!(set(one_line, "enabledPlugins/b@m", "true"), r#"{"model": "opus", "enabledPlugins": {"a@m": true, "b@m": true}}"#);
    }

    #[test]
    fn a_toml_key_is_written_with_its_comments_and_the_order_of_the_rest_kept() {
        let (format, keys) = keys_of(".codex/config.toml");
        let file = "# Codex, by hand\nmodel = \"gpt-a\" # the one I like\nnotify = [\"/opt/notify\"]\n\n[projects.\"/home/me/code\"]\ntrust_level = \"trusted\"\n\n# what I use\n[plugins.\"pstack@cursor-plugins\"]\nenabled = true\n";
        let set = |raw: &str, key: &str, value: &str| String::from_utf8(super::set(format, keys, Some(raw.as_bytes()), key, value.as_bytes(), &linux()).unwrap()).unwrap();

        assert_eq!(set(file, "model", "\"gpt-b\""), file.replace("\"gpt-a\"", "\"gpt-b\""));
        let off = set(file, "plugins/pstack@cursor-plugins", "{\"enabled\": false}");
        assert_eq!(off, file.replace("enabled = true", "enabled = false"), "the comment over the table stays");
        let added = set(file, "plugins/sentry@cursor-plugins", "{\"enabled\": true}");
        assert_eq!(added, format!("{file}\n[plugins.\"sentry@cursor-plugins\"]\nenabled = true\n"));
        let effort = set(file, "model_reasoning_effort", "\"high\"");
        assert!(effort.starts_with("# Codex, by hand\nmodel = \"gpt-a\" # the one I like\nnotify = [\"/opt/notify\"]\nmodel_reasoning_effort = \"high\"\n"), "{effort}");

        let listed = list(format, keys, added.as_bytes(), &linux()).unwrap();
        let named: Vec<&str> = listed.iter().map(|listed| listed.key.as_str()).collect();
        assert_eq!(named, ["model", "plugins/pstack@cursor-plugins", "plugins/sentry@cursor-plugins"], "notify and projects are never read");
        assert_eq!(value(&listed, "plugins/sentry@cursor-plugins"), Some(serde_json::json!({"enabled": true})));
        let gone = super::remove(format, keys, added.as_bytes(), "plugins/sentry@cursor-plugins").unwrap().unwrap();
        assert_eq!(String::from_utf8(gone).unwrap(), file);
    }

    #[test]
    fn a_key_is_carried_alike_however_its_file_orders_or_spells_it() {
        let (format, keys) = keys_of(".claude/settings.json");
        let windows = Spelling::new(r"C:\Users\me", true);
        let here = br#"{"hooks": {"Stop": [{"command": "node C:\\Users\\me\\h.js", "type": "command"}]}, "model": "opus"}"#;
        let there = br#"{"model": "opus", "hooks": {"Stop": [{"type": "command", "command": "node /home/me/h.js"}]}}"#;
        let (here, there) = (list(format, keys, here, &windows).unwrap(), list(format, keys, there, &linux()).unwrap());
        for key in ["hooks", "model"] {
            let carried = |listed: &[Listed]| listed.iter().find(|listed| listed.key == key).map(|listed| listed.carried.clone());
            assert_eq!(carried(&here), carried(&there), "{key}");
        }
        assert!(here.iter().all(|listed| !listed.own));

        let own = br#"{"statusLine": {"command": "/opt/homebrew/bin/node ~/s.js"}, "env": {"PATH": "C:\\tools;D:\\bin"}, "model": "opus"}"#;
        let own: Vec<(String, bool)> = list(format, keys, own, &linux()).unwrap().into_iter().map(|listed| (listed.key, listed.own)).collect();
        assert_eq!(own, [("statusLine".into(), true), ("env".into(), true), ("model".into(), false)]);
    }

    #[test]
    fn a_name_a_path_cannot_hold_is_spelled_so_it_can_and_read_back() {
        for name in ["github/copilot", "a:b", "50%", "..", "plain@market"] {
            let spelled = encode(name).unwrap();
            assert!(crate::tree::check_path(&spelled).is_ok(), "{spelled}");
            assert_eq!(decode(&spelled), name);
        }
        assert_eq!(encode(""), None);
        let (_, keys) = keys_of(".claude.json");
        assert!(names(keys, "mcpServers/github%2Fcopilot"));
        assert!(!names(keys, "mcpServers") && !names(keys, "projects/x") && !names(keys, "numStartups"));
    }
}
