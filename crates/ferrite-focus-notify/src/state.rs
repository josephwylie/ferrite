use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::util::notification_group_id;

const GENERATED_SCRIPT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const FOCUS_ORIGIN_RETENTION: Duration = Duration::from_secs(15);

pub(crate) fn plugin_state_dir() -> PathBuf {
    env::var_os("HERDR_PLUGIN_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("herdr-focus-notify"))
}

/// The most recently used terminal (learned from `pane.focused` events) is
/// persisted here so a click can activate it and skip checks can match it,
/// with no configuration required.
pub(crate) fn remember_terminal(workspace: &str, bundle_id: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    remember_terminal_into(&state_dir, workspace, bundle_id)
}

/// The terminal bound to a pane's workspace, learned from `pane.focused`
/// events. `None` until the workspace has been focused at least once or when
/// the saved bundle id is an obvious non-terminal app.
pub(crate) fn remembered_terminal(workspace: &str) -> Option<String> {
    read_terminal_bindings_from(&terminal_memory_path())?
        .get(workspace)
        .filter(|bundle_id| !is_obvious_non_terminal_bundle(bundle_id))
        .cloned()
}

/// Clears all saved workspace-to-terminal bindings.
pub(crate) fn clear_terminal_bindings() -> io::Result<()> {
    let binding_result = match fs::remove_file(terminal_memory_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    };
    binding_result?;
    clear_focus_origin_markers()
}

/// Marks a focus operation as originating from a notification click. The
/// marker is intentionally short-lived because Herdr delivers the resulting
/// focus event in a separate plugin process.
pub(crate) fn mark_focus_origin(workspace: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    fs::write(focus_origin_marker_path(workspace), [])
}

/// Returns whether a recent notification-originated focus operation is still
/// suppressing terminal learning for this workspace.
pub(crate) fn focus_origin_is_active(workspace: &str) -> bool {
    let path = focus_origin_marker_path(workspace);
    let active = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < FOCUS_ORIGIN_RETENTION);

    if !active {
        let _ = fs::remove_file(path);
    }
    active
}

pub(crate) fn clear_focus_origin(workspace: &str) -> io::Result<()> {
    match fs::remove_file(focus_origin_marker_path(workspace)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(crate) fn focus_origin_marker_path(workspace: &str) -> PathBuf {
    plugin_state_dir().join(format!(
        "focus-origin-{}.marker",
        crate::util::sanitize_group_id(workspace)
    ))
}

fn clear_focus_origin_markers() -> io::Result<()> {
    let state_dir = plugin_state_dir();
    let entries = match fs::read_dir(&state_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("focus-origin-") && name.ends_with(".marker") && path.is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Removes bindings for workspaces that no longer exist in Herdr.
///
/// Uses `herdr pane list` to learn the live workspace set; when that cannot
/// be trusted (command failure or no panes at all) nothing is pruned, so the
/// memory file never loses bindings against an empty world. Best-effort: a
/// stale binding is harmless, it just gets re-learned on the next focus.
/// The rewrite reuses the same atomic write path as learning.
pub(crate) fn prune_stale_workspace_bindings(herdr_bin: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    let path = terminal_memory_path();
    let Some(mut bindings) = read_terminal_bindings_from(&path) else {
        return Ok(());
    };
    let Some(live) = crate::focus::live_workspace_ids(herdr_bin) else {
        return Ok(());
    };

    let before = bindings.len();
    bindings.retain(|workspace, _| live.iter().any(|id| id == workspace));
    if bindings.len() == before {
        return Ok(());
    }

    fs::create_dir_all(&state_dir)?;
    remember_bindings_into(&state_dir, &bindings)
}

fn remember_bindings_into(
    dir: &Path,
    bindings: &std::collections::HashMap<String, String>,
) -> io::Result<()> {
    let path = dir.join("terminal-memory.json");
    let temp_path = dir.join(format!(".terminal-memory-{}.tmp", std::process::id()));
    let json = serde_json::json!({ "workspaces": bindings });
    fs::write(
        &temp_path,
        serde_json::to_vec(&json).map_err(io::Error::other)?,
    )?;
    fs::rename(temp_path, &path)
}

fn remember_terminal_into(dir: &Path, workspace: &str, bundle_id: &str) -> io::Result<()> {
    let path = dir.join("terminal-memory.json");

    // Read existing bindings so other workspaces survive the rewrite.
    let mut bindings = read_terminal_bindings_from(&path).unwrap_or_default();
    bindings.insert(workspace.to_string(), bundle_id.to_string());

    remember_bindings_into(dir, &bindings)
}

/// Reads the per-workspace bindings map (`{ "workspaces": { <id>: <bundle> } }`).
fn read_terminal_bindings_from(path: &Path) -> Option<std::collections::HashMap<String, String>> {
    let content = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let workspaces = value.get("workspaces")?.as_object()?;
    let mut bindings = std::collections::HashMap::new();
    for (workspace, id) in workspaces {
        if let Some(id) = id.as_str() {
            bindings.insert(workspace.clone(), id.to_string());
        }
    }
    if bindings.is_empty() {
        return None;
    }
    Some(bindings)
}

fn terminal_memory_path() -> PathBuf {
    plugin_state_dir().join("terminal-memory.json")
}

pub(crate) fn is_obvious_non_terminal_bundle(bundle_id: &str) -> bool {
    matches!(
        bundle_id,
        "com.google.Chrome"
            | "com.apple.Safari"
            | "org.mozilla.firefox"
            | "com.microsoft.edgemac"
            | "company.thebrowser.Browser"
            | "com.brave.Browser"
            | "com.apple.finder"
            | "com.apple.notificationcenterui"
    )
}

pub(crate) fn mark_notification_cleared(pane_id: &str) -> io::Result<()> {
    let state_dir = plugin_state_dir();
    fs::create_dir_all(&state_dir)?;
    fs::write(cleared_notification_marker_path(pane_id), [])
}

pub(crate) fn reset_notification_clearance(pane_id: &str) -> io::Result<()> {
    match fs::remove_file(cleared_notification_marker_path(pane_id)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(crate) fn cleared_notification_marker_path(pane_id: &str) -> PathBuf {
    plugin_state_dir().join(format!("{}.cleared", notification_group_id(pane_id)))
}

pub(crate) fn cleanup_stale_state_files() -> io::Result<()> {
    cleanup_stale_state_files_in(
        &plugin_state_dir(),
        SystemTime::now(),
        GENERATED_SCRIPT_RETENTION,
        TEMP_FILE_RETENTION,
        FOCUS_ORIGIN_RETENTION,
    )
}

/// The retention period for a generated state file, or None when the file is
/// not managed by cleanup. Cleared markers only gate the gap between a new
/// notification being queued and its script starting (seconds); every send
/// resets the marker first, so any marker old enough to be stale here is
/// inert.
fn retention_for(
    name: &str,
    script_retention: Duration,
    temp_file_retention: Duration,
    focus_origin_retention: Duration,
) -> Option<Duration> {
    if name.starts_with("focus-") && name.ends_with(".sh") {
        Some(script_retention)
    } else if name.starts_with("focus-origin-") && name.ends_with(".marker") {
        Some(focus_origin_retention)
    } else if name.contains(".result.") || name.contains(".status.") || name.ends_with(".cleared") {
        Some(temp_file_retention)
    } else {
        None
    }
}

fn cleanup_stale_state_files_in(
    state_dir: &Path,
    now: SystemTime,
    script_retention: Duration,
    temp_file_retention: Duration,
    focus_origin_retention: Duration,
) -> io::Result<()> {
    let entries = match fs::read_dir(state_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let Some(retention) = retention_for(
            name,
            script_retention,
            temp_file_retention,
            focus_origin_retention,
        ) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }

        let is_stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= retention);

        if is_stale {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_removes_only_generated_state_files() {
        let dir = std::env::temp_dir().join(format!(
            "herdr-focus-notify-state-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("focus-old.sh"), "old").unwrap();
        fs::write(dir.join("herdr-w1-p1.result.123456"), "old").unwrap();
        fs::write(dir.join("herdr-w1-p1.status.123456"), "old").unwrap();
        fs::write(dir.join("herdr-w1-p1.cleared"), "old").unwrap();
        fs::write(dir.join("focus-origin-w1.marker"), "old").unwrap();
        fs::write(dir.join("focus-click.log"), "keep").unwrap();

        cleanup_stale_state_files_in(
            &dir,
            SystemTime::now(),
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
        )
        .unwrap();

        assert!(!dir.join("focus-old.sh").exists());
        assert!(!dir.join("herdr-w1-p1.result.123456").exists());
        assert!(!dir.join("herdr-w1-p1.status.123456").exists());
        assert!(!dir.join("herdr-w1-p1.cleared").exists());
        assert!(!dir.join("focus-origin-w1.marker").exists());
        assert!(dir.join("focus-click.log").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_per_workspace_bindings() {
        let dir = std::env::temp_dir().join(format!(
            "herdr-focus-notify-terminal-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("terminal-memory.json");

        // Two workspaces bind independently and one rewrite keeps the other.
        remember_terminal_into(&dir, "w1", "com.googlecode.iterm2").unwrap();
        remember_terminal_into(&dir, "w2", "net.kovidgoyal.kitty").unwrap();
        let bindings = read_terminal_bindings_from(&path).expect("bindings should exist");
        assert_eq!(
            bindings.get("w1").map(String::as_str),
            Some("com.googlecode.iterm2")
        );
        assert_eq!(
            bindings.get("w2").map(String::as_str),
            Some("net.kovidgoyal.kitty")
        );

        // Overwriting one workspace does not disturb the other.
        remember_terminal_into(&dir, "w1", "app.warp").unwrap();
        let bindings = read_terminal_bindings_from(&path).expect("bindings should exist");
        assert_eq!(bindings.get("w1").map(String::as_str), Some("app.warp"));
        assert_eq!(
            bindings.get("w2").map(String::as_str),
            Some("net.kovidgoyal.kitty")
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
