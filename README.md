# Ferrite

Ferrite runs a lot of coding agents at once and puts them all on one screen.

Each conversation is a **Thread**. A Thread remembers its history, which
provider it uses (Claude or Codex), and which folder or git worktree it works
in. Each Thread gets a **Pane**. You can look at one Pane at a time, or make a
**Group** and see several side by side.

Panes show more or less depending on how big they are. A big Pane shows the
full transcript and a prompt box. A medium one shows progress and stats. A tiny
one is just a status light.

When an agent needs your permission or asks you a question, that's a
**Decision**. You can answer it with one key from anywhere, even from a tiny
Pane — you don't have to click into it first.

Ferrite doesn't talk to any API itself. It runs the real `claude` and `codex`
CLIs and shows you what they print.

Everything works from the keyboard. The mouse works too — click, drag, select,
right-click, resize.

If an agent spawns **subagents**, they show up as tabs in that agent's Pane.
Busy ones come first and have animated dots. Click a tab to read what it's
doing and answer its questions. The "Main" tab takes you back to the parent
conversation and whatever you'd typed. Opening a subagent tab just shows you
the transcript; it doesn't start or restart anything.

## Requirements

- `claude` or `codex` installed and logged in. Ferrite checks the version when
  it starts a session: `claude` 2.1.224 or newer (below 3.0.0), `codex` 0.149.1
  or newer (below 1.0.0).
- macOS on Apple silicon, or Windows x64.

If you have more than one copy of a CLI installed, Ferrite uses the newest one.
It checks your PATH plus the usual places (`~/.local/bin`, Homebrew, nvm, volta,
bun), asks each one its version, and picks the highest. That way it doesn't
matter whether you launched Ferrite from a terminal or from the Finder.
Settings › About shows which copy it picked.

## Install

Grab the latest installer from
[GitHub Releases](https://github.com/josephwylie/ferrite/releases/latest).

### macOS (Apple silicon)

Open the `.dmg` and drag Ferrite to Applications. Releases aren't signed yet, so
the first launch needs **Control-click Ferrite → Open → Open**. After that it
opens normally.

If you have the source, `scripts/install-app.sh` builds a release binary and
installs it to `/Applications/Ferrite.app`.

### Windows (x64)

Run `ferrite-v*-x86_64-pc-windows-msvc-setup.exe`. Releases aren't signed yet,
so SmartScreen may complain — choose **More info → Run anyway**. It installs for
your user only, no admin needed, and adds itself to the Start menu and to the
uninstall list in Windows Settings.

Maintainers: if no signing credentials are configured, releases go out unsigned.
See [docs/release-signing.md](docs/release-signing.md) to set signing up.

## Quickstart

Run `ferrite`. You get one Thread on Claude. Type in the box at the bottom and
press `enter`.

Prompts sent while the agent is working stack above the prompt box, newest
first. The provider controls when they run; Codex can use a follow-up during
the current turn. Use `↑` to retrieve the latest queued prompt for editing,
or `backspace` on an empty prompt box to remove it.

Shortcuts below use `cmd`; on Windows use `ctrl`.

- `cmd-t` / `cmd-n` — new Thread. You pick the provider, model, project folder,
  and where it works: the checkout as it stands, one of the repo's existing
  worktrees or branches, or a new branch — in the checkout or in a fresh
  worktree.
- `cmd-shift-n` — new Thread in its own git worktree
- `cmd-g` — new Group: the focused Thread plus a new one beside it (the
  titlebar's **New Group**, when the Thread is in no Group yet)
- `escape` — interrupt whatever's running
- `y` / `n` / `a` — answer a Decision: allow, deny, or always allow. Works from
  the focused Pane or from any Pane, however small.
- `cmd-]` / `cmd-[` — next / previous Pane
- `cmd-d` — jump to the next Decision
- `cmd-f` — make the focused Pane fullscreen (and back)
- `cmd-b` — hide the sidebar
- `cmd-i` — the bell: everything that finished while you were looking elsewhere
- `cmd-w` — park a Thread; `cmd-o` — bring back the last one you parked
- `cmd-,` — Settings: defaults for new Threads (provider, model, effort),
  Claude's permission mode, Codex's approval policy and sandbox, naming, and
  confirmations. Search by name or description. Changes save straight to
  `~/.ferrite/settings.json`.
- `cmd-v` — paste into the focused Pane's prompt box from anywhere in the window
- `cmd-c` — copy what you've selected in a transcript

When Claude asks a multiple-choice question, the Pane draws it as a form. Click
an option or press `1`–`4`, or type your own answer, then `enter`. While the
question is up, letter keys just type.

### The prompt box

`alt-backspace` / `alt-delete` delete a word. `cmd-backspace` / `cmd-delete`
delete to the start or end of the line. `alt-←` / `alt-→` move by word,
`cmd-←` / `cmd-→` jump to the ends, and holding shift with any of those
selects. `cmd-a` selects everything, `cmd-z` / `cmd-shift-z` undo and redo.

The box wraps and grows one row per line, up to eight, then scrolls.
`shift-enter` adds a line, `enter` sends.

`/` at the start opens the agent's own command menu, plus Ferrite's `/model`,
`/effort`, and `/import`. Type `/` followed immediately by a letter anywhere in
your prompt to find a skill. A space ends the skill search. Hover over a skill
or use the arrow keys to highlight it, then press `tab` or `enter` to select it.
Selecting a skill keeps the text around it.

`@` completes file and folder names from the Thread's checkout. `↑` brings back
earlier prompts — or, if you're in the middle of a multi-line draft, just moves
up a row.

### With the mouse

**Drag files onto a Pane** to attach them to its prompt. Whichever Pane is
under the pointer takes the drop, Groups included. Attachments sit in a small
tray above the prompt box. Images get thumbnails — click one to preview it, or
hit its × to remove it. Other files show as file cards. Both stay in the chat
after you send, and they're still there when you reopen the Thread. Attachments
follow queued prompts and history recall too. Claude and Codex get supported
images as real image input; anything else is passed as a file path for their
tools to read.

**Background work shows as chips** on the right of that same tray: when Claude
sends a shell command or a subagent to the background, or Codex leaves a command
running as a background terminal, each running task gets a chip with a green
pulse and its description. Hit a chip's × to stop that task. Chips leave when
their task finishes, and the tray goes with the last one.

**Click the pencil next to a Project** in the project filter to add or remove
directories, or delete the project. The directory it was created from stays
primary, and you can't delete a project that still has Threads.

**Click a text file card** in the transcript to open the native reader in
its own slot beside the Thread, in Solo or in a Group. It resizes and moves
like any Pane, and keeps its place until you quit. Rust, Python, JavaScript,
TypeScript, Go, C/C++, Java, shell, TOML, YAML and JSON files are syntax
highlighted. Markdown links resolve from
the document's folder, and opening another file replaces that Thread's
reader. Files edited by the Thread or its subagents stay listed in the
**Files changed** row above the prompt for quick access.

**Right-click** a Thread or Group in the sidebar for: rename, open or resume,
fullscreen, new Thread in the same project, reveal in Finder, copy path, park,
leave or dissolve a Group, and delete (press twice). Right-click a transcript
for: copy the selection or the whole thing, rename, fullscreen, reveal, copy
path, park, close.

**Double-click** a Pane title (or click the title in the sidebar) to rename it.
A new Thread is named from your first prompt right away. If *Titles* is on in
Settings, Claude then writes a short 3–6 word title in the background (haiku,
low effort, one turn, no tools). If you rename it yourself, your name sticks.
The title also goes to the agent — Codex's thread name, Claude's `--name`.

**When an agent finishes** — its turn is done and no subagents are still
running — the bell at the top of the sidebar counts it, a toast pops up in the
top right, and if you can see the Pane its focus ring pulses. Click the toast,
the bell, or the sidebar row to jump there. Once you land on it the pulse
stops. Subagents finishing don't notify you, and neither does an interrupt or a
turn that ends by sending a prompt you'd queued. Toasts are silent and stay
inside the app.

**Status dots** in the sidebar: green working, amber waiting on you, red
closed, dim idle, hollow parked.

**In a Group**, drag the divider between two Panes to resize them. Drag a
Pane's header onto another Pane to swap them (drop in the middle) or split that
slot (drop near an edge). Works at any Pane size. Each Group remembers its own
arrangement. Drag a Thread from the sidebar onto a Pane to add it to the Group
on that side; on a lone Thread, that starts a Group. Medium Panes keep their prompt box, so you can still tell a small
Pane what to do.

**The model picker** sits at the right edge of the prompt box and lists each
provider's models by name ("Fable 5.1", "GPT-5.6 Sol"). You can switch models
mid-conversation and the session resumes on the new one. Switching to the
*other provider* after you've already sent a prompt hands the conversation
over: it starts a fresh session there and sends a summary of what happened so
far ahead of your next prompt. The **effort chip** next to it lists the
reasoning levels that model supports, with your default at the top.

### While a turn is running

The most recent thinking or reasoning heading stays pinned above the prompt
box, along with the current tool and elapsed time. The same thing shows in
small Panes and in the selected subagent's tab. Retries, compaction, tool
output, and plans all show up here. If a provider doesn't emit summaries you
still see its tools and phases. Reopening old history never restarts the
clocks.

### Flags

- `--provider claude|codex` — provider for the first Thread (default `claude`)
- `--import <path>` — take an existing Claude or Codex session file and
  continue it as a Thread. Repeatable.

Threads live in `~/.ferrite/threads` (`%USERPROFILE%\.ferrite\threads` on
Windows, or set `FERRITE_STORE`). Settings live next to them in
`~/.ferrite/settings.json`.

## Accessibility

Ferrite draws its own UI with GPUI, so there's no screen reader support right
now. We know. We'll revisit it if GPUI grows an accessibility bridge.

## Development

Rust workspace. The renderer choice and the GPUI Kit `=0.6.0` pin are in
`docs/adr/0003-native-gpui-components.md`, the project's vocabulary is in
`CONTEXT.md`, and the original 24-pane render spike is in `spikes/panes24/`.

Subagent architecture is in
[ADR 0002](docs/adr/0002-subagent-activity.md). Requirements, validation, and
review notes are on
[issue #1](https://github.com/josephwylie/ferrite/issues/1).

## License

Dual-licensed under [Apache License 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), your choice.
