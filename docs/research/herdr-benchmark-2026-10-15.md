# Herdr benchmark — 2026-10-15

What Ferrite should take from [Herdr](https://github.com/herdrdev/herdr)
(v0.9.4, Apache-2.0, ~44k stars), architecturally and feature-wise. Herdr is a
Rust terminal multiplexer for coding agents: a background server owns PTYs, a
TUI client attaches to it, and agent state is inferred from the screen.

Sources: a read-only clone of `herdrdev/herdr`, its docs
(<https://herdr.dev/docs/persistence-remote/>), ~50 plugin READMEs from the
`herdr-plugin` GitHub topic (~1,700 repos), four Hacker News threads, GitHub
issues/discussions, and reviews (linked at the end).

## Decisions so far

| # | Item | Decision |
|---|---|---|
| 1 | `ferrited` server (large version) | Yes |
| 2 | Socket API + CLI, the Herdr way (not MCP) | Yes |
| 3 | Bundled agent skill | Yes |
| 4 | Sound + OS notifications | Yes |
| 5 | Navigator status filters | Yes |
| 6 | Remote over SSH | Yes |
| 7 | Git ahead/behind on rows | Yes |
| — | Four-state status (computed from protocol events, not the screen) | Yes |
| — | Turn useful plugins into native features | Yes |
| — | Coordinator Threads (herdr-projects pattern) | Yes, in scope |
| — | Mermaid diagrams in the transcript | Yes |
| — | Upgrade by restart-when-idle + resume, not live handoff | Yes |
| — | Calmer, more minimal nav and top band | Yes |
| — | Worktree setup steps on create | No |
| — | Worktree removal/cleanup | No |
| — | Separate data folder for debug builds | No |

## Status

Where `feat/terminal-ui-overhaul` stands on every item below, as of the
end of 2026-10-15. "Partial" names what is missing.

| Item | Status | Notes |
|---|---|---|
| #1 `ferrited` server | Done | ADR 0025: Engine carved out of Cockpit, frozen gen-1 contract, snapshot + replay reconnect, one owner per Decision, flush on Decisions and turn end, interrupted turns, macOS logout flush, LaunchAgent / Windows Run key, `systemd --user` (ADR 0026) |
| Upgrade by restart-when-idle | Done | `server restart {when: idle\|now}`, tied to self-update (ADR 0023, 0025) |
| #2 Socket API + CLI | Partial | ADR 0027: `api.sock`, one operations table, `thread.*`, `agent.prompt/wait/read`, `decision.*`, `events.subscribe`, `notification.show`, `FERRITE_THREAD_ID`/`FERRITE_SOCKET`/`FERRITE_BIN`, `--machine`. Not done: named agents (Threads are addressed by id), `layout export/apply`. `worktree create` is `thread.start --worktree`. CLI output is not visible from a Windows terminal |
| #3 Bundled agent skill | Done | `skills/ferrite/SKILL.md`, `ferrite --skill [install]` |
| #6 Remote over SSH | Partial | ADR 0026: `ferrited bridge`, `--remote`, saved machines in the nav, status-only links, parked and new Threads, off-screen alerts. Not done on a machine: live branch label and status, opened files and user commands (run locally), review beyond the last turn, an open Thread's Project; phone access deferred by plan |
| Four-state status | Done | ADR 0015: blocked > done > working > idle from protocol events |
| Attention sort, roll-up headings, collapsed rail | Done | ADR 0015; rail digits are jump keys |
| Row details (#7 ahead/behind, CI, waiting reason, background work) | Done | `nav::RowGit`; reason words `approval` / `plan` / `question`; green ring for background work |
| #5 Palette filters, workspace jump, cwd footer | Done | `b` `w` `i` `d` `a` |
| #4 Sounds + OS notifications + toasts | Done | ADR 0017: two chimes, custom files, per-provider mute (not per-agent), click focuses, 1 s delay, 8 s / 5 s expiry |
| Review slot | Done | ADR 0021: uncommitted / branch / last turn, line comments |
| Session import + allowance warning | Done | ADR 0022 |
| Custom palette commands + user keymap | Done | ADR 0024 |
| Coordinator Threads | Done | ADR 0028: briefs, worktree workers, `## Remember` memory, need ordering, opt-in PR follow-up (CI, reviews, inline comments) |
| Mermaid diagrams | Done | ADR 0016 |
| Hygiene: durable settings, banner, clean env, diagnostics, find, self-update, staggered revival | Done | ADRs 0018–0020, 0023 |
| Hygiene: confirm quit while working | Superseded | Added, then dropped by ADR 0025: quitting the window no longer stops agents |
| UI cleanliness 1, 2, 3, 6 | Done | ADR 0014: band kept for the window drag but empty; one bottom bar |
| UI cleanliness 4, 5 | Done | ADR 0015: needs-you strip gone, one heading style |
| Store never reuses a Thread id | Done | ADR 0029 (found while building the API) |
| Worktree setup / cleanup, debug data folder | Not done | Declined in the plan |

## Where Ferrite is already ahead

- **Exact state.** Herdr guesses state by regex-matching the screen
  (`src/detect/manifests/claude.toml` matches the title spinner and
  `esc to interrupt`); ~424 issues mention detection. Ferrite reads it from
  stream-json / app-server events.
- **Decisions.** Herdr knows an agent is "blocked"; Ferrite knows the exact
  request and answers it with one key from any Pane.
- **Subagents.** Ferrite shows them as tabs; Herdr users ask for this (#5210).
- **Rendering.** 120 Hz GPUI vs Herdr's 60 Hz cap (top-voted issue #1134). No
  terminal-in-terminal bugs (copy/paste, colors, key conflicts).
- **Rich transcript.** Native markdown, diff cards, syntax-highlighted reader,
  inline visuals. In Herdr this is whatever each CLI's TUI draws.
- **Working timers and deferred finish.** A finish waits until children
  settle; Herdr has neither.

## Foundation

### 1. `ferrited` — headless server

Herdr's single most-praised property is that "if you quit the app, the agents
keep running". Its server split makes detach/reattach, multiple clients,
remote, and the automation API possible.

- A headless binary built from `ferrite-core` owns every `SessionLifecycle`,
  the provider children, the Thread store, and pending Decisions.
- Runs as a per-user LaunchAgent on macOS (correct keychain context for
  `claude`/`codex` logins, automatic restart). Windows: a per-user background
  process; Linux remote hosts: `systemd --user`.
- The GPUI app is a client over a local socket (mode 0600 under `~/.ferrite`).
- Two protocol tiers, as in Herdr: a frozen, generation-versioned client
  contract (hello/welcome, capabilities, optional fields, `Unknown` enum
  fallbacks) and a JSON API for CLI/automation.
- Reconnect = snapshot `{boot_id, revision, threads, roster, decisions}` plus
  replay of `SessionEvent`s after the last seen `(thread, seq)`. Simpler than
  Herdr: Ferrite streams structured events, not terminal cell grids, and the
  store is already an event log.
- Decisions have one owner: first valid answer wins, is broadcast to every
  client, and late answers get `already_answered`. Never auto-retry an answer.
- Streaming text uses a latest-wins lane per Thread; Decisions and turn
  boundaries use a reliable lane (Herdr's two-lane transport).
- Force a store flush on turn end and on every Decision. A Thread restored with
  an incomplete turn shows as interrupted.
- Flush and record resume ids before macOS logout/shutdown (port Herdr's
  `src/platform/macos/shutdown.rs` approach).

**Upgrading the server.** Herdr's "live handoff" passes running agent processes
from the old server to the new one so nothing stops. For Ferrite that would
mean rebuilding each provider's protocol state mid-stream — fragile. Instead,
when Ferrite updates: wait until agents are idle (or the user says "update
now"), stop the old server, start the new one, and resume each Thread with
`claude --resume` / `thread/resume`. An idle Thread is fully on disk, so
nothing is lost. This happens once per Ferrite update, not per turn. The app
and server can update independently because the client contract is frozen.

| Event | Today | With `ferrited` |
|---|---|---|
| App quit / crash | All agents stop | Agents keep running |
| SSH drop (remote) | n/a | Remote agents keep running |
| Server crash / reboot | — | Threads resume automatically |
| Ferrite update | Agents stop | Restart when idle, then resume |

### 2. Socket API and `ferrite` CLI

Copy Herdr's shape (`src/api/schema/operations.rs`): each operation declared
once with its wire name, CLI binding and JSON Schema. Newline-delimited JSON
over the socket.

Herdr's built-in orchestration, all of which Ferrite lacks:

- Named agents (`agent start reviewer --kind codex`), unique per server.
- `agent prompt <name> "…" --wait`: refuses with `agent_blocked` if the target
  is waiting on a Decision; returns `agent_prompt_stalled` if no activity
  within 5 s; `--wait` only settles after the agent was seen working.
- `agent wait --until <state>`, `agent read`, `agent list`, `agent focus`.
- `events subscribe` (state changes, Decisions, lifecycle), subscribe before
  snapshot.
- Caller context injected into every child (`HERDR_PANE_ID`,
  `HERDR_SOCKET_PATH`…) so an agent can target "my own Thread".
- `--machine <label>`: the same commands against a remote server.
- `notification show`, `layout export/apply`, `worktree create`.

Ferrite equivalents: `thread start/list/focus`, `agent prompt --wait`,
`agent wait`, `agent read`, `decision list/answer`, `events subscribe`, with
`FERRITE_THREAD_ID` and `FERRITE_SOCKET` in each Session's environment.

### 3. Bundled agent skill

Ship a `SKILL.md` (like Herdr's `skills/herdr/SKILL.md`) that teaches agents
to start, prompt and wait on other Threads through the CLI, with safety rules:
don't answer another agent's Decision without the user, a timeout doesn't prove
non-delivery, never resend blindly.

### 6. Remote over SSH

`ferrite --remote host` runs `ssh host ferrited bridge` and exposes it as a
local socket, as Herdr's `SshStdioBridge` does. Saved machines appear in one
sidebar; only the selected machine streams transcripts, the rest send status.
Start with "`ferrited` must already be on the remote PATH". Phone access later
(community demand is high: collie, herdr-web-ui, pairfob).

## Status and sidebar

- **Four states, Herdr's model, Ferrite's signals.** Blocked > Done > Working >
  Idle. Done = turn finished and not yet seen; "seen" = the Thread was actually
  on screen while the window had focus. Ferrite already tracks `unread` but
  ignores it on the dot (`ferrite/src/cockpit.rs` `thread_status`).
  Glyphs/colors: working ● yellow, blocked ● red, done ✓ teal, idle ○.
- **Attention sort** toggle (Herdr #318) and **roll-up status** on Project and
  Group headings (max priority of members).
- **Collapsed rail** that keeps numbered rows with dots (Ferrite folds to 0).
- **Row details:** git ahead/behind (#7, already collected in
  `ferrite-core/src/workspace/mod.rs` but unused), CI glyph, waiting reason
  (permission / plan / question), "background work still running" (#5264).
- **Palette (#5):** single-key status filters (`b` `w` `i` `d` `a`),
  workspace jump, cwd in the detail footer.

## Notifications (#4)

- Two sounds: done and needs-you; custom files; per-agent mute.
- OS notifications when the window is unfocused; click focuses the Thread.
- Toasts: ~1 s delay, re-validate state before showing, timed expiry
  (8 s attention / 5 s finished), new event for a Thread replaces its toast.

## Features taken from plugins

- **Review slot** (herdr-reviewr, 885★, MIT): diff beside the Thread scoped
  to uncommitted / branch / last turn; line comments become prompt context.
- **Searchable session import** (memex): search past Claude/Codex sessions
  and resume one.
- **Low-allowance warning** (herdr-agent-usage) at 5 h / weekly thresholds.
- **Custom palette commands and user keymap** (open in editor, run tests,
  `gh pr create`).
- **Coordinator Threads** (herdr-projects, 673★, MIT, community-built on
  Herdr's API). In scope; built on #2. You talk to one coordinator Thread
  that never does the work itself. It proposes tasks; on your go-ahead it
  starts one worker Thread per task, each on its own worktree and branch, each
  briefed with the project's goal, standing instructions and shared memory
  (lessons a worker reports flow back for the next one). The coordinator's
  Group shows workers ordered by what needs you: needs you → ready for review
  → working. Failing CI and review comments on a worker's PR are sent back to
  that worker.
- **Mermaid diagrams.** Render ` ```mermaid ` blocks in the transcript and
  reader as diagrams. Herdr users ask for this; a terminal can't do it.
  Likely route: the existing off-screen visual renderer (ADR 0013) with a
  bundled `mermaid.js` (MIT), no network.

Plugins with no license (worktree-setup, gh-pr, focus-notify, herdr-board):
ideas only, no code.

## Hygiene

- Flush before rename for `settings.json`, `registry.json`, `groups.json`
  (currently temp file + rename without fsync).
- Banner when `settings.json` can't be parsed (today: silent defaults).
- Strip inherited `CLAUDECODE` and similar from agents' environment
  (`ferrite-core/src/providers/claude.rs`).
- Confirm quit while agents are working.
- Find in transcript (Cmd-F).
- Log file and crash diagnostics.
- Self-update for Ferrite with a "What's New" screen.
- Stagger launch revival; move git calls off the launch path.

## UI cleanliness

Herdr's chrome is noticeably calmer. Principles to study (see
`assets/screenshot.png` in the Herdr repo):

- Sidebar is two plain lists — spaces (name + branch underneath) and agents
  (name + one state word, right-aligned). No boxes, chips or icons.
- One glyph per row, color carries state; everything else is muted text.
- Lowercase section labels (`agents`, `new`) instead of headers or buttons.
- No top toolbar beyond the window title; status lives in a single bottom line.
- Panes separated by a thin border; only the focused pane's border is accented.

Audit of `feat/terminal-ui-overhaul` (visual-reference scenes
`parity-group`, `parity-solo`, `navrail`) against Herdr:

1. **Top band repeats what's already on screen.** The breadcrumb restates the
   focused Thread's title, state and branch (all in its nav row and pane
   header) plus a `⌘K commands` hint, bell and nav toggle. Herdr has no top
   bar. Candidate: remove the band; bell and toggle move to the nav footer
   or bottom bar.
2. **Two stacked bottom bars.** The composer footer (mode, model, ctx, files,
   `⇧⇥ mode · ? shortcuts`) sits on top of the global bar (Groups, usage,
   clock). Herdr has one. Candidate: one bar.
3. **Hints everywhere.** `⌘K commands`, `⌘D`, `? shortcuts`, `⇧⇥ mode`,
   placeholder `Steer this thread · / for commands · drop or paste images`,
   `queue a follow-up · ⏎ sends when the turn ends`. Herdr shows almost
   none. Candidate: keep `?` as the single way in; placeholders shrink to
   one short phrase.
4. **The needs-you strip duplicates rows.** "Close stale issues" appears in
   the strip and again in its Group. Herdr sorts by attention instead.
   Candidate: drop the strip; the attention sort plus colored dots do its job.
5. **Inconsistent nav headings.** `needs you` (lowercase muted), `Groups`
   (bold capital), project names (bold) and `parked 3`. Herdr uses one style
   of lowercase muted label. Candidate: one label style.
6. **Session card is heavy.** Large logo plus the full temp path plus start
   time above the transcript. Candidate: one muted line, or nothing; the
   pane header already names the Thread.

Already Herdr-like: one mono font, square planes, one glyph per row with a
right-aligned state word, thin pane borders with the focused one accented.

## Community signal

Loved: at-a-glance status; agents survive closing the app; reattach from
phone/SSH; mouse-first defaults; scriptable API; "it stays out of the way".
Complaints: wrong status (detection), 60 Hz lag, remote lag over hours,
copy/paste, terminal-in-terminal glitches, Windows beta, trust after YC.
Herdr's main pitch against GUI tools is "if you quit the app, the agents
stop" — `ferrited` removes that argument.

## Sources

- <https://github.com/herdrdev/herdr>
- <https://herdr.dev/docs/persistence-remote/>
- <https://herdr.dev/compare/>
- <https://news.ycombinator.com/item?id=48756578>
- <https://news.ycombinator.com/item?id=49201003>
- <https://news.ycombinator.com/item?id=48714802>
- <https://news.ycombinator.com/item?id=49652188>
- <https://betterstack.com/community/guides/ai/herdr-ai-agent/>
- <https://www.bitdoze.com/herdr-agent-multiplexer/>
- <https://flaviocopes.com/herdr-vs-cmux/>
- <https://getmoshi.app/guides/tmux-or-herdr>
- <https://yage.ai/share/herdr-agent-multiplexer-en-20260715.html>
- <https://github.com/eliasstravik/herdr-projects>
- <https://github.com/persiyanov/herdr-reviewr>
