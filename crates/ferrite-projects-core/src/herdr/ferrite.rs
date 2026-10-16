//! Ferrite: where herdr's command line and socket meet Ferrite's Server (ADR 0031). Every herdr command this crate
//! runs (`cli::launch`) and every request it makes on herdr's socket (`socket::ask`) arrives here in herdr's own
//! words, after a test's stand-in has had its turn. Each is answered from Ferrite's API (ADR 0027) once it is wired;
//! until then it is refused `not_wired` before anything runs, which callers treat as any refusal of herdr's.
use super::cli::{HRes, HerdrError, Ran};
use serde_json::Value;

pub fn launch(_socket: Option<&str>, args: &[String], _input: &str, _timeout_ms: u64) -> HRes<Ran> {
    Err(not_wired(&format!("`herdr {}`", command(args))))
}

pub fn ask(method: &str, _params: &Value, _socket: Option<&str>) -> HRes<Value> {
    Err(not_wired(&format!("herdr's `{method}`")))
}

fn not_wired(what: &str) -> HerdrError {
    HerdrError {
        code: "not_wired".into(),
        message: format!("Ferrite has not wired {what} to its Server yet."),
    }
}

/// A command's leading words ("pane get"), never its ids or the text it carries.
fn command(args: &[String]) -> String {
    let is_word = |a: &&String| !a.starts_with('-') && a.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let words: Vec<&str> = args.iter().take_while(is_word).take(3).map(String::as_str).collect();
    match words.is_empty() {
        true => args.first().cloned().unwrap_or_default(),
        false => words.join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unwired_command_is_refused_by_name_only() {
        let args: Vec<String> = ["agent", "prompt", "w1:p2", "a secret prompt"].map(String::from).into();
        let Err(e) = launch(None, &args, "", 1000) else { panic!("a command ran") };
        assert_eq!(e.code, "not_wired");
        assert_eq!(e.message, "Ferrite has not wired `herdr agent prompt` to its Server yet.");
    }

    #[test]
    fn a_bare_flag_is_named_as_it_is() {
        let Err(e) = launch(None, &["--default-config".to_string()], "", 1000) else { panic!("a command ran") };
        assert!(e.message.contains("`herdr --default-config`"));
    }
}
