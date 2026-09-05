//! Model discovery without a Thread: initialize, model/list, then exit.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::ModelInfo;

/// Ask the installed CLI for its current model menu. Call off the UI
/// thread. No thread/start, turn/start, or model invocation is involved.
pub fn list(program: &str) -> io::Result<Vec<ModelInfo>> {
    list_with_timeout(program, Duration::from_secs(10))
}

fn list_with_timeout(program: &str, timeout: Duration) -> io::Result<Vec<ModelInfo>> {
    let program = super::super::spawnable_program(program);
    super::check_version(&program).map_err(io::Error::other)?;
    let mut child = Command::new(program)
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    #[cfg(windows)]
    let job = super::super::job::SessionJob::assign_or_reap(&mut child)?;
    let stdin = child.stdin.take().expect("stdin was piped");
    let stdout = child.stdout.take().expect("stdout was piped");
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("ferrite-model-list".into())
        .spawn(move || {
            let _ = tx.send(query(stdin, BufReader::new(stdout)));
        });
    let result = match worker {
        Ok(_) => rx
            .recv_timeout(timeout)
            .map_err(|error| {
                io::Error::new(
                    if matches!(error, mpsc::RecvTimeoutError::Timeout) {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::UnexpectedEof
                    },
                    format!("Codex model discovery did not complete: {error}"),
                )
            })
            .and_then(|result| result),
        Err(error) => Err(error),
    };
    // Reap on success, protocol failure and timeout. Windows' npm shim
    // requires ending the whole process tree, as a Session does.
    #[cfg(windows)]
    job.terminate();
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn write(writer: &mut impl Write, message: Value) -> io::Result<()> {
    writeln!(writer, "{message}")?;
    writer.flush()
}

fn response(reader: &mut impl BufRead, id: u64) -> io::Result<Value> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Codex closed during model discovery",
            ));
        }
        if let Some(result) = super::wire::parse_response(&line, id) {
            return result.map_err(io::Error::other);
        }
    }
}

fn query(mut writer: impl Write, mut reader: impl BufRead) -> io::Result<Vec<ModelInfo>> {
    write(
        &mut writer,
        json!({
            "id": 1, "method": "initialize",
            "params": {"clientInfo": {"name": "ferrite", "version": env!("CARGO_PKG_VERSION")}}
        }),
    )?;
    response(&mut reader, 1)?;
    write(&mut writer, json!({"method": "initialized"}))?;
    let mut cursor = None;
    let mut cursors = std::collections::HashSet::new();
    let mut data = Vec::new();
    for id in 2.. {
        let params = cursor.map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
        write(
            &mut writer,
            json!({"id": id, "method": "model/list", "params": params}),
        )?;
        let page = response(&mut reader, id)?;
        let rows = page.get("data").and_then(Value::as_array).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Codex model/list carried no data",
            )
        })?;
        data.extend(rows.iter().cloned());
        cursor = page
            .get("nextCursor")
            .and_then(Value::as_str)
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_string);
        let Some(next) = &cursor else {
            return Ok(super::wire::parse_models(&json!({"data": data})));
        };
        if !cursors.insert(next.clone()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Codex model/list repeated its cursor",
            ));
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_or_malformed_catalog_does_not_become_a_menu() {
        for result in [
            json!({"id":2,"error":{"code":-32601,"message":"unsupported"}}),
            json!({"id":2,"result":{}}),
        ] {
            let replies = format!("{{\"id\":1,\"result\":{{}}}}\n{result}\n");
            assert!(query(Vec::new(), replies.as_bytes()).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_silent_catalog_times_out_and_reaps_its_process() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("ferrite-model-timeout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("codex");
        let pid = dir.join("pid");
        std::fs::write(&program, format!("#!/bin/sh\ncase \"$1\" in --version) echo 'codex-cli 0.153.4'; exit 0;; esac\necho $$ > '{}'\nexec sleep 30\n", pid.display())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let error =
            list_with_timeout(program.to_str().unwrap(), Duration::from_millis(250)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid = std::fs::read_to_string(pid).unwrap();
        assert!(
            !Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success(),
            "discovery must reap its child on timeout"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn discovery_paginates_and_keeps_the_providers_default_and_efforts() {
        let replies = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"notification\",\"params\":{}}\n",
            "{\"id\":2,\"result\":{\"data\":[{\"id\":\"first-model\"}],\"nextCursor\":\"page-2\"}}\n",
            "{\"id\":3,\"result\":{\"data\":[{\"id\":\"future-model\",\"isDefault\":true,\"defaultReasoningEffort\":\"ultra\",\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"ultra\"}]},{\"id\":\"hidden-model\",\"hidden\":true}],\"nextCursor\":null}}\n"
        );
        let mut sent = Vec::new();
        let rows = query(&mut sent, replies.as_bytes()).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.value.as_str())
                .collect::<Vec<_>>(),
            ["future-model", "first-model"]
        );
        assert_eq!(rows[0].efforts, ["ultra"]);
        assert_eq!(rows[0].default_effort.as_deref(), Some("ultra"));
        let requests: Vec<Value> = String::from_utf8(sent)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            requests
                .iter()
                .map(|request| request["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["initialize", "initialized", "model/list", "model/list"]
        );
        assert_eq!(requests[3]["params"]["cursor"], "page-2");
    }
}
