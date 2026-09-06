//! Provider-owned command discovery, without a Thread or model turn.
//! Run off the UI thread; every probe is bounded and reaps its process.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::{store::Provider, SessionCommand};
use serde_json::{json, Value};

pub type Discovery = mpsc::Receiver<io::Result<Vec<SessionCommand>>>;

pub fn list(program: &str, provider: Provider, cwd: &Path) -> io::Result<Vec<SessionCommand>> {
    list_with_timeout(program, provider, cwd, Duration::from_secs(10))
}

fn list_with_timeout(
    program: &str,
    provider: Provider,
    cwd: &Path,
    timeout: Duration,
) -> io::Result<Vec<SessionCommand>> {
    let mut command = Command::new(super::spawnable_program(program));
    match provider {
        Provider::Codex => {
            command.arg("app-server");
        }
        Provider::Claude => {
            command.args([
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
            ]);
        }
    }
    let mut child = command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    #[cfg(windows)]
    let job = super::job::SessionJob::assign_or_reap(&mut child)?;
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let cwd = cwd.to_path_buf();
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("ferrite-command-list".into())
        .spawn(move || {
            let _ = tx.send(query(stdin, BufReader::new(stdout), provider, &cwd));
        });
    let result = worker.and_then(|_| {
        rx.recv_timeout(timeout).map_err(|error| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("Command discovery did not complete: {error}"),
            )
        })?
    });
    #[cfg(windows)]
    job.terminate();
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn write(writer: &mut impl Write, value: Value) -> io::Result<()> {
    writeln!(writer, "{value}")?;
    writer.flush()
}

fn query(
    mut writer: impl Write,
    mut reader: impl BufRead,
    provider: Provider,
    cwd: &Path,
) -> io::Result<Vec<SessionCommand>> {
    match provider {
        Provider::Codex => write(
            &mut writer,
            json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"ferrite","version":env!("CARGO_PKG_VERSION")}}}),
        )?,
        Provider::Claude => write(
            &mut writer,
            json!({"type":"control_request","request_id":"commands","request":{"subtype":"initialize"}}),
        )?,
    }
    let mut line = String::new();
    let mut initialized = false;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Provider closed during command discovery",
            ));
        }
        match provider {
            Provider::Claude => {
                if let Some(capabilities) =
                    super::claude::wire::parse_capabilities(&line, "commands")
                {
                    return Ok(capabilities.commands);
                }
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value["response"]["request_id"] == "commands"
                        && value["response"]["subtype"] == "error"
                    {
                        return Err(io::Error::other(value["response"]["error"].to_string()));
                    }
                }
            }
            Provider::Codex => {
                if !initialized {
                    if let Some(result) = super::codex::wire::parse_response(&line, 1) {
                        result.map_err(io::Error::other)?;
                        write(&mut writer, json!({"method":"initialized"}))?;
                        write(
                            &mut writer,
                            json!({"id":2,"method":"skills/list","params":{"cwds":[cwd],"forceReload":true}}),
                        )?;
                        initialized = true;
                    }
                } else if let Some(result) = super::codex::wire::parse_response(&line, 2) {
                    let result = result.map_err(io::Error::other)?;
                    if !result["data"].is_array() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "skills/list carried no data",
                        ));
                    }
                    return Ok(super::codex::wire::parse_skills(&result));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_discovery_is_scoped_and_never_creates_a_thread() {
        let input = concat!("{\"id\":1,\"result\":{}}\n", "{\"id\":2,\"result\":{\"data\":[{\"skills\":[{\"name\":\"global\",\"path\":\"/home/skills/global/SKILL.md\",\"enabled\":true},{\"name\":\"disabled\",\"path\":\"/disabled\",\"enabled\":false}]}]}}\n");
        let mut output = Vec::new();
        let commands = query(
            &mut output,
            input.as_bytes(),
            Provider::Codex,
            Path::new("/workspace"),
        )
        .unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "global");
        let requests: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            requests
                .iter()
                .map(|r| r["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["initialize", "initialized", "skills/list"]
        );
        assert_eq!(requests[2]["params"]["cwds"], json!(["/workspace"]));
    }

    #[test]
    fn claude_discovery_uses_the_effective_menu_without_sending_a_prompt() {
        let input = "{\"type\":\"control_response\",\"response\":{\"request_id\":\"commands\",\"subtype\":\"success\",\"response\":{\"commands\":[{\"name\":\"global\",\"description\":\"Global skill\"}]}}}\n";
        let mut output = Vec::new();
        let commands = query(
            &mut output,
            input.as_bytes(),
            Provider::Claude,
            Path::new("/workspace"),
        )
        .unwrap();
        assert_eq!(commands[0].name, "global");
        let request: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(request["request"]["subtype"], "initialize");
        assert_eq!(request["type"], "control_request");
    }
    #[test]
    #[ignore = "queries installed provider CLIs, without model turns"]
    fn installed_providers_discover_global_commands_without_threads() {
        let cwd = std::env::current_dir().unwrap();
        for provider in [Provider::Claude, Provider::Codex] {
            let program = super::super::discover::program(provider);
            let commands = list(&program, provider, &cwd).unwrap();
            assert!(
                !commands.is_empty(),
                "installed provider should have commands"
            );
            println!(
                "{provider:?}: {} commands; diagnosing-bugs={}",
                commands.len(),
                commands
                    .iter()
                    .any(|command| command.name == "diagnosing-bugs")
            );
        }
    }

    #[test]
    fn refused_and_malformed_catalogs_fail_instead_of_becoming_empty_menus() {
        for tail in [
            "{\"id\":2,\"error\":{\"message\":\"refused\"}}\n",
            "{\"id\":2,\"result\":{}}\n",
        ] {
            let input = format!("{{\"id\":1,\"result\":{{}}}}\n{tail}");
            assert!(query(
                Vec::new(),
                input.as_bytes(),
                Provider::Codex,
                Path::new("/workspace")
            )
            .is_err());
        }
        let input = "{\"type\":\"control_response\",\"response\":{\"request_id\":\"commands\",\"subtype\":\"error\",\"error\":\"refused\"}}\n";
        assert!(query(
            Vec::new(),
            input.as_bytes(),
            Provider::Claude,
            Path::new("/workspace")
        )
        .is_err());
    }
}
