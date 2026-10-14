# 0015 — A diagnostics log and crash reports

Status: accepted 2026-10-15 (Herdr benchmark, Hygiene).

## Context

Ferrite reported trouble with `eprintln!`. A Dock launch has no terminal,
so every one of those lines was lost, a panic left nothing behind, and a
Session that died said why only in its Pane. A bug report had to be
reconstructed from memory.

## Decision

- `ferrite_core::diagnostics` owns one process-wide log under
  `~/.ferrite/logs`: `ferrite.log`, rotated to `ferrite.log.1` past 1 MB, so
  it never holds more than about 2 MB. `log!` writes a timestamped line to
  it and still prints to stderr. Process-wide on purpose: a Session ending
  on a reader thread and a panic on any thread reach the same file without
  threading a handle through the core.
- `diagnostics::open` is the first thing a launch does. It also installs a
  panic hook that writes `crash-<time>.txt` (version, platform, message,
  thread, backtrace) beside the log before the default hook runs.
- The core records why each Session ended: the CLI's exit (`Closed`'s
  reason, with its stderr tail), a park, the memory watchdog's restart, and
  the quit.
- Settings › About's Diagnostics copies a report — version, platform, CLI
  versions, the newest crash report and the log's last 200 lines — for the
  operator to paste into a bug report. Nothing is sent anywhere.

## Consequences

- The logged lines are the ones already printed; no new logging framework
  or levels. Test runs never call `open`, so they write no files.
