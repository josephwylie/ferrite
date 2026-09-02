# Ferrite — Context

Ferrite is an open-source, native (GPUI) multi-provider agent cockpit: many
coding-agent conversations run in parallel and are watched, steered, and
approved from one dense grid. This file is the project's canonical language —
glossary only, no implementation.

## Glossary

**Thread**
One durable agent conversation: its history, its provider choice, and its
workspace binding (a dedicated worktree or the main checkout). Threads outlive
any running process.
_Avoid_: chat, conversation, task.

**Session**
The live provider process currently serving a Thread. A Thread may have no
Session (parked) or one (hot). Sessions are disposable; Threads are not.
_Avoid_: process (when meaning the conversation), instance.

**Provider**
An agent backend a Session runs on. v1: Claude and Codex.
_Avoid_: model (a provider selects models; it is not one).

**Pane**
The visible cell in the cockpit grid showing one Thread at some zoom level.
A view, never an identity.
_Avoid_: window, tile, terminal.

**Cockpit**
The visible Panes: **Solo** shows exactly one Thread; **Group** shows the exact
ordered membership of one durable Group (two or more Threads).
_Avoid_: dashboard, workspace (see Workspace binding).

**Solo**
The default Cockpit view: exactly the focused Thread's Pane.

**Group**
A durable, operator-ordered set of two or more Threads from one registered
project. Opening its header shows exactly its members in the Cockpit.

**Roster**
The Cockpit's state, headless: the open Panes in order, the one holding the
keyboard, the view (Solo or one Group), the fullscreen, and this launch's
park order. Changed only by the operator's acts on the Cockpit (close,
reopen, enter a Group, drop); the window mirrors it and paints.
_Avoid_: layout (that is what the Roster's grid computes), tab list.

**Semantic zoom**
A Pane's rendering chosen by its size: **L1** (near: transcript + prompt),
**L2** (instruments: progress, tests, diff stats), **L3** (wall: status LED +
one signal). No user mode-switch; size decides.

**Composer**
The prompt line at the bottom of a focused Pane: one growing text line,
keyboard-driven menus, queue-while-busy.
_Avoid_: chat box, input field.

**Decision**
Anything a Thread is blocked on that only the operator can answer (tool
permission, plan approval). Decisions are answerable from Pane, wall badge,
and (planned) Remote.

**Workspace binding**
The checkout a Thread works in: a per-Thread worktree or the main checkout —
chosen at Thread creation. Nothing else in v1.

**Remote**
Following and answering Decisions away from the desktop. Wanted soon;
scope under discussion.

**Not a harness, not an agent.** Ferrite never runs its own agent loop and
never calls model APIs itself. It orchestrates official agent CLIs (each
Provider's own harness) and renders what they stream. The agents' behavior,
auth, and ToS posture belong to their vendors, not to Ferrite.

**No Terminal.** Ferrite has no terminal, PTY, or shell concept. Agents run
as headless processes streaming structured events. Any future "shell pane"
would be a new concept, not a revival of this one.

## Settled product decisions (2026-08-25)

- Open-source project; dual-licensed MIT + Apache-2.0.
- Name kept: Ferrite (iOS audio app collision accepted — distant category).
- v1 providers: Claude + Codex, minimal.
- v1 cockpit: Solo by default; an opened Group shows its exact membership with
  semantic zoom L1–L3.
- v1 git: workspace binding only (worktree or main); no checkpoints yet.
- Issue-tracker/board integration: later, not v1.
- Platforms: macOS (Metal) AND Windows (DirectX 11) first-class from the start.
- Remote: full control (watching, Decisions, prompting, Thread creation) —
  wanted soon after v1 core.
- Accessibility: no v1 story (GPUI limitation); one honest README note, topic
  closed until GPUI grows AccessKit-style support upstream.
- Own repository at projects/ferrite (codingOS is an ignoring holder repo).
- Sessions are operator-budgeted, never pooled: open Pane = live Session
  once the Thread exists (a draft Pane spawns nothing until its first send,
  #29), closed Pane = parked Thread. No hot-pool cap, no hidden eviction;
  memory is shown, and a watchdog restarts leaking Sessions visibly.
