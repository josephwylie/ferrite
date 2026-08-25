# Ferrite

Ferrite is a native (GPUI) cockpit for running many coding agents in parallel.
Each **Thread** — one durable agent conversation, with its Provider and its
workspace binding — shows in a **Pane** of the **Cockpit** grid, up to a
24-pane wall. A Pane's size picks its semantic zoom: near, the transcript and
a Composer; smaller, instruments; at wall range, a status LED. **Decisions** —
anything a Thread is blocked on that only you can answer — are answerable
straight from the wall with one key. Ferrite is not a harness and never talks
to a Provider's API itself: it drives the official Claude and Codex CLIs and
renders what they stream.

**v1 is keyboard-first.** There is no pointer support yet; everything is
driven from the keyboard.

## Requirements

- A working `claude` or `codex` CLI on your PATH, already logged in.
  Ferrite checks the version at Session start: `claude` 2.1.224 up to (not
  including) 3.0.0, `codex` 0.149.1 up to 1.0.0.
- macOS on Apple silicon, or Windows x64.

## Install

Download the latest archive from
[GitHub Releases](https://github.com/josephwylie/ferrite/releases/latest).

### macOS (Apple silicon)

```sh
tar -xzf ferrite-v*-aarch64-apple-darwin.tar.gz
xattr -c ferrite   # unsigned binary; clear the download quarantine
./ferrite
```

### Windows (x64)

Unzip `ferrite-v*-x86_64-pc-windows-msvc.zip` and run `ferrite.exe`. The
binary is unsigned, so SmartScreen objects the first time: choose
**More info → Run anyway**.

## Quickstart

Run `ferrite`. The Cockpit opens with one Thread on the Claude provider;
type into the Composer and press `enter` to prompt it. A prompt sent while
the Thread is busy is held and goes out when the turn ends; sending another
replaces the held one.

Shortcuts are spelled with `cmd` below; on Windows the same shortcuts use
`ctrl`.

- `cmd-n` — new Thread in the current checkout
- `cmd-shift-n` — new Thread in its own git worktree
- `escape` — interrupt the running Session
- `y` / `n` / `a` — answer a Decision (allow / deny / always), from the
  focused Pane or straight from the wall
- `cmd-]` / `cmd-[` — next / previous Pane; `cmd-d` — jump to the next Decision
- `cmd-w` — park a Thread; `cmd-o` — revive the newest parked one

Flags:

- `--provider claude|codex` — Provider for the first Thread (default: `claude`)
- `--import <path>` — adopt an existing Claude or Codex CLI session file as a
  Thread and continue it (repeatable)

Threads persist in `~/.ferrite/threads` (`%USERPROFILE%\.ferrite\threads` on
Windows; override with `FERRITE_STORE`).

## Accessibility

Ferrite draws its own pixels (GPUI); there is no screen-reader support today.
This is a known limitation, revisited when the framework grows an
accessibility bridge upstream.

## Development

Rust workspace; the renderer decision and the gpui `=0.2.2` pin are recorded
in `docs/adr/0001-gpui-renderer.md`, project vocabulary in `CONTEXT.md`, and
the original 24-pane render spike in `spikes/panes24/`.

## License

Dual-licensed under either of [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
