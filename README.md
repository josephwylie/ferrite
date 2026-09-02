# Ferrite

Ferrite is a native (GPUI) cockpit for running many coding agents in parallel.
Each **Thread** — one durable agent conversation, with its Provider and its
workspace binding — shows in a **Pane** of the **Cockpit**. You see one
Thread at a time, or one **Group** — Threads you have put together, whose
membership is the Cockpit's grid. A Pane's size picks its semantic zoom:
near, the transcript and a Composer; smaller, instruments; at wall range, a
status LED. **Decisions** — anything a Thread is blocked on that only you
can answer — are answerable at wall range with one key, without focusing
the Pane first. Ferrite is not a harness and never talks
to a Provider's API itself: it drives the official Claude and Codex CLIs and
renders what they stream.

**Keyboard-first, pointer-complete.** Everything has a key; the pointer
clicks, drags, selects, right-clicks and resizes as well.

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

From a checkout, `scripts/install-app.sh` builds the release binary and
installs it as `/Applications/Ferrite.app`, so it opens from the Dock,
Spotlight or Launchpad.

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

- `cmd-t` / `cmd-n` — new Thread: a draft Pane whose band picks the
  Provider and model, the Project (any registered folder, or *Choose
  folder…*), and the checkout or a fresh worktree
- `cmd-shift-n` — new Thread in its own git worktree
- `escape` — interrupt the running Session
- `y` / `n` / `a` — answer a Decision (allow / deny / always), from the
  focused Pane or from any Pane at wall range
- `cmd-]` / `cmd-[` — next / previous Pane; `cmd-d` — jump to the next Decision
- `cmd-f` — the focused Pane fullscreen, and back; `cmd-b` — fold the nav
- `cmd-w` — park a Thread; `cmd-o` — revive the newest parked one
- `cmd-,` — Settings: the Provider and model new Threads start on, Claude's
  permission mode, Codex's approval policy and sandbox, naming and
  confirmation behaviour. Saved to `~/.ferrite/settings.json`.

In the Composer: `alt-backspace` / `alt-delete` take a word,
`cmd-backspace` / `cmd-delete` the line halves, `alt-←` / `alt-→` step by
word, `cmd-←` / `cmd-→` jump to the ends, shift with any of those selects,
`cmd-a` selects all, `cmd-z` / `cmd-shift-z` undo and redo. `/` opens the
Session's own command menu, `@` completes files and folders under the
Thread's checkout, `↑` recalls earlier prompts.

With the pointer:

- **Right-click** a Thread, Group or Project in the nav, or anywhere in a
  Pane, for its menu: rename, focus, fullscreen, new Thread in the same
  Project, reveal in Finder, copy path, park, leave or dissolve a Group,
  delete (two presses).
- **Double-click** a Pane's title, or click a nav row's title, to rename
  it. Untitled Threads are named from their first prompt.
- In a **Group**, drag the seam between two Panes to resize them, and
  drag a Pane's title onto another Pane to swap (the centre) or to split
  its slot (an edge). The arrangement is remembered per Group.
- The **model picker** at the Composer's right edge lists each Provider's
  models by name; the model can change mid-conversation (the Session
  resumes under it); the Provider is fixed once the first prompt is sent.

Flags:

- `--provider claude|codex` — Provider for the first Thread (default: `claude`)
- `--import <path>` — adopt an existing Claude or Codex CLI session file as a
  Thread and continue it (repeatable)

Threads persist in `~/.ferrite/threads` (`%USERPROFILE%\.ferrite\threads` on
Windows; override with `FERRITE_STORE`); settings beside them in
`~/.ferrite/settings.json`.

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
