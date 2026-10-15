//! The `ferrite` CLI (ADR 0026): one API request from the command line,
//! its answer printed as the JSON line the Server sent. It is built from
//! the operations' one declaration (`schema`): `ferrite <group> <verb>`,
//! the declared positional parameters in order, every other one as a
//! `--flag` typed by its JSON Schema, and after `--` every word is an
//! argument. `--current` names the calling Thread (`FERRITE_THREAD_ID`); a
//! `cwd` not given is the CLI's own.
//!
//! Exit status, as Herdr's: 0 answered, 1 refused by the Server (the error
//! on stderr, as JSON) or no Server reached, 2 a wrong command line.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::schema::Operation;
use super::{address, ApiError, Code, Method, Request};
use crate::link::{self, TryClone};

/// The agent skill: how an agent in one Thread drives the others. Bundled,
/// so the printed skill always matches this binary.
pub const SKILL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../skills/ferrite/SKILL.md"
));

/// Run the CLI when `args` (the process's, program first) are its; `None`
/// leaves them to the app. The exit status otherwise.
pub fn run(args: &[String], store: &Path) -> Option<i32> {
    let words: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    Some(match words.as_slice() {
        ["--skill"] => {
            print!("{SKILL}");
            0
        }
        ["--skill", "install"] => install_skill(),
        ["--schema"] => {
            let schema = schemars::schema_for!(Request).to_value();
            println!(
                "{}",
                serde_json::to_string_pretty(&schema).unwrap_or_default()
            );
            0
        }
        ["help" | "--help"] => {
            print!("{}", usage(None));
            0
        }
        [group, rest @ ..] if groups().any(|known| known == *group) => command(group, rest, store),
        _ => return None,
    })
}

fn groups() -> impl Iterator<Item = &'static str> {
    Method::OPERATIONS.iter().map(|op| op.words().0)
}

fn command(group: &str, rest: &[&str], store: &Path) -> i32 {
    let Some((verb, args)) = rest
        .split_first()
        .filter(|(verb, _)| !verb.starts_with('-'))
    else {
        print!("{}", usage(Some(group)));
        return 0;
    };
    let Some(op) = Method::OPERATIONS
        .iter()
        .find(|op| op.words() == (group, *verb))
    else {
        eprint!(
            "ferrite: no command `{group} {verb}`\n\n{}",
            usage(Some(group))
        );
        return 2;
    };
    let context = Context {
        thread: std::env::var("FERRITE_THREAD_ID").ok(),
        cwd: std::env::current_dir().unwrap_or_default(),
    };
    match parse(op, args, &context) {
        Ok(request) => {
            let socket = std::env::var_os("FERRITE_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_else(|| address(store));
            call(&request, &socket, &mut io::stdout(), &mut io::stderr())
        }
        Err(error) => {
            eprint!("ferrite: {error}\n\n{}", usage(Some(group)));
            2
        }
    }
}

/// What the command line is read against.
pub struct Context {
    /// `FERRITE_THREAD_ID`: the Thread `--current` names.
    pub thread: Option<String>,
    /// Where a `cwd` is resolved from, and what one not given is.
    pub cwd: PathBuf,
}

/// The request one operation's arguments make.
pub fn parse(op: &Operation, args: &[&str], context: &Context) -> Result<Request, String> {
    let schema = (op.params)().to_value();
    let defs = &schema["$defs"];
    let properties = schema["properties"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut params = Map::new();
    let mut positional = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if *arg == "--" {
            positional.extend(args.by_ref());
            break;
        }
        if *arg == "--current" {
            if !properties.contains_key("thread") {
                return Err(format!("`{}` takes no Thread", op.name));
            }
            let thread = context
                .thread
                .as_deref()
                .ok_or("--current needs FERRITE_THREAD_ID: run it from inside a Ferrite Thread")?;
            params.insert("thread".into(), Kind::Integer.parse(thread)?);
            continue;
        }
        let Some(flag) = arg.strip_prefix("--") else {
            positional.push(*arg);
            continue;
        };
        let (name, inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (flag, None),
        };
        let field = name.replace('-', "_");
        let property = properties
            .get(&field)
            .ok_or_else(|| format!("`{}` takes no --{name}", op.name))?;
        let kind = kind(property, defs);
        let value = match (&kind, inline) {
            (Kind::Flag, None) => Value::Bool(true),
            (kind, Some(text)) => kind.parse(text)?,
            (kind, None) => kind.parse(args.next().ok_or(format!("--{name} needs a value"))?)?,
        };
        match (params.get_mut(&field), value) {
            (Some(Value::Array(items)), Value::Array(more)) => items.extend(more),
            (_, value) => {
                params.insert(field, value);
            }
        }
    }
    let mut unfilled = op
        .positional
        .iter()
        .filter(|name| !params.contains_key(**name))
        .copied()
        .collect::<Vec<_>>()
        .into_iter();
    for arg in positional {
        let name = unfilled
            .next()
            .ok_or_else(|| format!("`{arg}`: `{}` takes no more arguments", op.name))?;
        params.insert(name.into(), kind(&properties[name], defs).parse(arg)?);
    }
    if properties.contains_key("cwd") {
        let cwd = match params.get("cwd").and_then(Value::as_str) {
            Some(given) => context.cwd.join(given),
            None => context.cwd.clone(),
        };
        params.insert("cwd".into(), json!(cwd));
    }
    serde_json::from_value(json!({ "id": "1", "method": op.name, "params": params }))
        .map_err(|error| error.to_string())
}

/// How a parameter is written on the command line.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// Present or not: `--wait`.
    Flag,
    Integer,
    Text,
    /// Repeated: `--threads 3 --threads 4`.
    List(Box<Kind>),
    /// Anything else, as JSON: `--answer '{"kind":"allow","input":{}}'`.
    Json,
}

impl Kind {
    fn parse(&self, text: &str) -> Result<Value, String> {
        match self {
            Kind::Flag => text
                .parse::<bool>()
                .map(Value::Bool)
                .map_err(|_| format!("`{text}` is not true or false")),
            Kind::Integer => text
                .parse::<u64>()
                .map(Value::from)
                .map_err(|_| format!("`{text}` is not a whole number")),
            Kind::Text => Ok(Value::String(text.into())),
            Kind::List(item) => Ok(Value::Array(vec![item.parse(text)?])),
            Kind::Json => serde_json::from_str(text).map_err(|error| format!("`{text}`: {error}")),
        }
    }
}

/// The schema a parameter's own stands for: a reference followed, an
/// optional one's `null` set aside.
fn resolve<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    if let Some(name) = schema["$ref"].as_str() {
        return resolve(&defs[name.rsplit('/').next().unwrap_or_default()], defs);
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = schema[key].as_array() {
            let real: Vec<&Value> = branches
                .iter()
                .filter(|branch| branch["type"] != "null")
                .collect();
            if let [one] = real.as_slice() {
                return resolve(one, defs);
            }
        }
    }
    schema
}

fn kind(schema: &Value, defs: &Value) -> Kind {
    let schema = resolve(schema, defs);
    if schema.get("enum").is_some() {
        return Kind::Text;
    }
    let types: Vec<&str> = match &schema["type"] {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    match types.into_iter().find(|kind| *kind != "null") {
        Some("boolean") => Kind::Flag,
        Some("integer" | "number") => Kind::Integer,
        Some("string") => Kind::Text,
        Some("array") => Kind::List(Box::new(kind(&schema["items"], defs))),
        _ => Kind::Json,
    }
}

/// `ferrite help`, or one group's commands.
pub fn usage(group: Option<&str>) -> String {
    let mut out = String::from("Usage: ferrite <group> <verb> [arguments] [--flags]\n\n");
    for op in Method::OPERATIONS
        .iter()
        .filter(|op| group.is_none_or(|group| op.words().0 == group))
    {
        let (group, verb) = op.words();
        out.push_str(&format!(
            "  ferrite {group} {verb}{}\n      {}\n",
            synopsis(op),
            op.summary
        ));
    }
    if group.is_none() {
        out.push_str(
            "\n  ferrite --skill [install]\n      Print the agent skill, or install it for Claude and Codex.\n  \
             ferrite --schema\n      The API's JSON Schema.\n",
        );
    }
    out.push_str(
        "\n--current names the calling Thread; after --, every word is an argument\n\
         (`agent prompt 12 -- \"--flags in the prompt\"`). Answers are JSON lines; a\n\
         refusal is JSON on stderr with exit status 1, a wrong command line exits 2.\n",
    );
    out
}

/// ` <thread> <text> [--wait] [--timeout-ms <n>]`.
fn synopsis(op: &Operation) -> String {
    let schema = (op.params)().to_value();
    let defs = &schema["$defs"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut out: String = op
        .positional
        .iter()
        .map(|name| format!(" <{name}>"))
        .collect();
    for (name, property) in schema["properties"].as_object().into_iter().flatten() {
        if op.positional.contains(&name.as_str()) {
            continue;
        }
        let flag = name.replace('_', "-");
        let written = match kind(property, defs) {
            Kind::Flag => format!("--{flag}"),
            Kind::Integer => format!("--{flag} <n>"),
            Kind::Json => format!("--{flag} <json>"),
            Kind::List(_) => format!("--{flag} <{name}>…"),
            Kind::Text => match resolve(property, defs)["enum"].as_array() {
                Some(choices) => {
                    let choices: Vec<&str> = choices.iter().filter_map(Value::as_str).collect();
                    format!("--{flag} <{}>", choices.join("|"))
                }
                None => format!("--{flag} <{name}>"),
            },
        };
        if required.contains(&name.as_str()) {
            out.push_str(&format!(" {written}"));
        } else {
            out.push_str(&format!(" [{written}]"));
        }
    }
    out
}

/// Ask, and print what comes back, a whole line at a time: the answer to
/// `out`, or a refusal to `err`, and after a subscription's answer every
/// event until the Server closes the connection.
pub fn call<E: Write>(request: &Request, socket: &Path, out: &mut impl Write, err: &mut E) -> i32 {
    let refused = |err: &mut E, error: ApiError| {
        print(
            err,
            &json!({ "id": request.id, "error": error }).to_string(),
        );
        1
    };
    let mut stream = match link::connect(socket) {
        Ok(stream) => stream,
        Err(error) => {
            return refused(
                err,
                ApiError::new(
                    Code::ServerUnavailable,
                    format!("no Ferrite server at {}: {error}", socket.display()),
                ),
            )
        }
    };
    let asked = link::send(&mut stream, request).and_then(|()| stream.try_clone());
    let mut lines = match asked {
        Ok(reader) => link::Lines::new(reader),
        Err(error) => return refused(err, lost(error)),
    };
    let streams = matches!(request.method, Method::EventsSubscribe(_));
    loop {
        match lines.next_line() {
            Ok(Some(line)) => {
                let line = String::from_utf8_lossy(&line);
                let answer: Value = serde_json::from_str(&line).unwrap_or_default();
                if answer.get("error").is_some() {
                    print(err, &line);
                    return 1;
                }
                print(out, &line);
                if !streams {
                    return 0;
                }
            }
            Ok(None) if streams => return 0,
            Ok(None) => return refused(err, lost(io::ErrorKind::UnexpectedEof.into())),
            Err(error) => return refused(err, lost(error)),
        }
    }
}

/// One line, in one write: a reader never sees half of it.
fn print(to: &mut impl Write, line: &str) {
    let _ = to.write_all(format!("{line}\n").as_bytes());
}

/// The connection broke before an answer: the request may have been
/// carried out all the same.
fn lost(error: io::Error) -> ApiError {
    ApiError::new(
        Code::ServerUnavailable,
        format!("the connection ended before an answer ({error}); the request may still have been carried out"),
    )
}

/// Install the skill where Claude Code and Codex look for the operator's
/// own skills.
fn install_skill() -> i32 {
    let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
    else {
        eprintln!("ferrite: no home directory to install the skill in");
        return 1;
    };
    let codex = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let mut installed = Vec::new();
    for root in [home.join(".claude"), codex] {
        let path = root.join("skills").join("ferrite").join("SKILL.md");
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, SKILL));
        if let Err(error) = written {
            eprintln!("ferrite: could not install {}: {error}", path.display());
            return 1;
        }
        installed.push(path);
    }
    println!("{}", json!({ "installed": installed }));
    0
}
