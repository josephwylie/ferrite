//! Native session discovery without starting or resuming a Thread.

use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::{import::Candidate, store::Provider};

use super::catalog;

const PAGE_LIMIT: u64 = 256;

/// List the newest native sessions with an importable rollout path. Call off
/// the UI thread; discovery never starts or resumes a provider Thread.
pub fn list(program: &str, cap: usize) -> io::Result<Vec<Candidate>> {
    if cap == 0 {
        return Ok(Vec::new());
    }
    catalog::request_only(program, Duration::from_secs(10), move |writer, reader| {
        query(writer, reader, cap)
    })
}

fn query(
    mut writer: impl Write,
    mut reader: impl BufRead,
    cap: usize,
) -> io::Result<Vec<Candidate>> {
    catalog::initialize(&mut writer, &mut reader)?;
    let mut cursor = None;
    let mut cursors = HashSet::new();
    let mut ids = HashSet::new();
    let mut candidates = Vec::new();
    for id in 2..2 + PAGE_LIMIT {
        catalog::write(
            &mut writer,
            json!({
                "id": id, "method": "thread/list",
                "params": {
                    "cursor": cursor,
                    "limit": (cap - candidates.len()).clamp(1, 100),
                    "sortKey": "updated_at", "sortDirection": "desc",
                    "modelProviders": [], "archived": false
                }
            }),
        )?;
        let page = catalog::response(&mut reader, id)?;
        let rows = page.get("data").and_then(Value::as_array).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Codex thread/list carried no data",
            )
        })?;
        for row in rows {
            // Until native history adoption is available, import requires a
            // rollout path. Thread identity always comes from the native id.
            let Some(path) = row["path"].as_str().filter(|path| !path.is_empty()) else {
                continue;
            };
            let Some(session_id) = row["id"].as_str().filter(|id| !id.is_empty()) else {
                continue;
            };
            if !ids.insert(session_id.to_owned()) {
                continue;
            }
            let modified = row["updatedAt"].as_i64().and_then(|seconds| {
                let duration = Duration::from_secs(seconds.unsigned_abs());
                if seconds < 0 {
                    UNIX_EPOCH.checked_sub(duration)
                } else {
                    UNIX_EPOCH.checked_add(duration)
                }
            });
            candidates.push(Candidate {
                provider: Provider::Codex,
                path: PathBuf::from(path),
                modified,
                title: row["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .or_else(|| row["preview"].as_str())
                    .map(str::to_owned),
                cwd: row["cwd"].as_str().map(PathBuf::from),
                session_id: Some(session_id.to_owned()),
            });
            if candidates.len() == cap {
                return Ok(candidates);
            }
        }
        cursor = page["nextCursor"]
            .as_str()
            .filter(|cursor| !cursor.is_empty())
            .map(str::to_owned);
        let Some(next) = &cursor else {
            return Ok(candidates);
        };
        if !cursors.insert(next.clone()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Codex thread/list repeated its cursor",
            ));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Codex thread/list exceeded its page limit",
    ))
}
