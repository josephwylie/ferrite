# Ferrite — Context

Ferrite is an open-source, native (GPUI) multi-provider agent cockpit: many
coding-agent conversations run in parallel and are watched, steered, and
approved from one dense grid. This file is the project's canonical language —
glossary only, no implementation.

## Glossary

**Thread**
One durable agent conversation: its history, its provider choice, and its
workspace binding (a dedicated worktree or the main checkout). Threads outlive
any running process. A Thread's id is never given to another, even after the
Thread is deleted.
_Avoid_: chat, conversation, task.

**Session**
The live provider process currently serving a Thread. A parked Thread has
none; an open Thread has one only while it is in use — it starts when the
Thread needs it and ends when the Thread has been idle a while, and the
operator never sees either. Sessions are disposable; Threads are not.
_Avoid_: process (when meaning the conversation), instance.

**Activity**
The Main and subagent activity observed within one Thread: their transcripts,
relationships, latest outcomes, and Decisions. Observation does not create
additional Ferrite Sessions or Threads.

**Digest**
What every window knows of a Thread it does not draw at Transcript level:
what Main is doing, the Decisions it waits on, its Wall tile, its counts and
how its latest turn finished. A window holds a Thread's whole Activity only
while it draws its Transcript.
_Avoid_: summary; not the Handover's digest of earlier exchanges.

**Subject**
The execution context a Pane is showing within its Thread: Main or one
Subagent. Switching Subject changes the view, not the Thread or its Title.

**Main**
The Thread's primary agent context, addressed by its Composer and interrupt
control. Main can be idle while a Subagent is working; interrupting Main
stops its turn alone, never a Subagent or background task.

**Subagent**
A provider-managed agent delegated within Main's ancestry, observed through
the owning Session. It may delegate further or work again after completion;
its transcript and Decisions remain its own. Escape on its tab stops it alone.
_Avoid_: Thread, Session, worker Pane.

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

**Panel**
The right column, beside the board: the focused Thread's changes, its
pull request and its context, one tab each, and its **preview** while one
is open. It follows the focused Thread in every view (Solo, a Group's
focused Pane, a fullscreen Pane, with a Review open beside it), shown
while the operator has it on; there is none without a Thread (a draft,
the empty board, the Projects coordinator), but for a draft's preview.
_Avoid_: sidebar (that is the nav), Beside slot (that opens inside the
board).

**Preview**
A file, an image or a command's output opened from a Pane — a file link
in an answer, a ⌘-clicked path, a tool row, a prompt's image, a shelf
chip, a changed file, a review's line — read in the Panel's Preview tab,
which widens the Panel while it shows. Each Pane keeps its own; the Panel
shows the focused Pane's. Another file opening replaces it in place.
_Avoid_: reader (the board's reader slot is the Review's), viewer (that is
the image's part of it).

**Bar**
The window's one strip at the bottom: the view tabs, the needs-you count,
usage, free disk when it runs low, and the clock. It carries no status
line: a Thread's mode, model and Effort are chips in its Composer, and its
context and changes are the Panel's.
_Avoid_: status bar, status line (both are gone).

**Group**
A durable, operator-ordered set of two or more Threads, which may span
Projects. Each Thread retains its own Workspace binding. Opening the Group's
header shows exactly its members in the Cockpit.

**Roster**
The Cockpit's state, headless: the open Panes in order, the one holding the
keyboard, the view (Solo or one Group), the fullscreen, and this launch's
park order. Changed only by the operator's acts on the Cockpit (close,
reopen, enter a Group, drop); the window mirrors it and paints.
_Avoid_: layout (that is what the Roster's grid computes), tab list.

**Layout**
A Group's arrangement of its Panes: a binary split tree (each split a row
or a column with a share for its first side), persisted with the Group.
Seams between Panes resize it; dropping a Pane on another swaps them (the
centre) or splits that slot (an edge). A Pane's open Review takes a slot of
its own in the same tree (in Solo too), the reader slot, placed for the
session only — the persisted Layout names Threads alone. Files and images
open in the Panel's preview instead.
_Avoid_: grid (that is what an even Layout happens to be), mosaic.

**Title**
What a Thread is called: the operator's own name for it, else a short
title a small model wrote from the first prompt (the titler, off by
turning *Titles* off), else the first line of its first prompt, else its
number. Told to the Session where the Provider takes one.

**Effort**
A Thread's reasoning depth, one rung of the ladder its model announces
(Claude: low…max; Codex: low…ultra, per model). Chosen per Thread by the
effort chip or `/effort`, defaulting to Settings. Changes apply to the next
turn on the same Session; they never restart it or consume a pending Handover.

**Handover**
Switching a Thread to the other Provider after its first prompt: a fresh
Session there, the Transcript kept, and a digest of the earlier exchanges
carried ahead of the next prompt, once. Not a resume — the new Provider
never saw the old conversation.

**Header**
A Thread's durable facts: its Provider, model, Effort, Title, Workspace
binding and Project. They are written first when the Thread is created.
Each later change is appended to the Thread's history as an amendment, and
the newest amendment wins. Earlier history is never rewritten to change one.
_Avoid_: metadata, sidecar.

**Settings**
The operator's defaults for new Threads (Provider, model, effort,
permissions) and for the app's behaviour, in one file beside the store. Never history: a
Thread's header remains the durable truth about that Thread.

**Semantic zoom**
A Pane's rendering chosen by its size: **Transcript** (near: the transcript
and its Composer) or **Wall** (far: the state word, the last few lines, quick
answers). No user mode-switch; size decides.

**Composer**
The prompt box at the bottom of a Transcript Pane: text that wraps and
grows a row per line, keyboard-driven menus, queue-while-busy.
_Avoid_: chat box, input field.

**Decision**
A live request that only the operator can answer: tool permission, plan
approval, or a Question, belonging to Main, a Subagent, or an unattributed
request within the owning Session. Several may be pending at once; historical
requests are not answerable.

**Question**
A Decision whose input is a form — Claude's `AskUserQuestion`. The Pane
draws the questions and their options; the answer is the tool allowed
with the picks (and any typed words) folded into its input, the shape
Claude Code's own UI sends.

**Workspace binding**
The checkout a Thread works in: a per-Thread worktree or the main checkout —
chosen at Thread creation, and following Main after it: into a worktree of
the same repo its agent makes, enters, or works in for a run of tool calls,
and back to the main checkout when it leaves or the worktree is gone. The
Pane's checkout label reads the binding; nothing else moves it.

**Visual**
An interactive page an agent shows inline in its transcript through
Ferrite's own `show_visual` tool: a mockup, chart, diagram or timeline.
It builds up while the agent writes it, and the agent is answered with a
screenshot so it can check its work. Offered at the operator's level —
Off, When asked, or Automatic — and never a Decision.
_Avoid_: artifact, widget, web view (that is how it is drawn).

**Diagram**
A ` ```mermaid ` fence in an answer or the reader, drawn as the picture it
describes once it is on screen; until then, or when it can't be drawn, it
reads as code. Not a Visual: nobody asked for it, and it is not live.

**Review**
A diff of a Thread's changes beside its Pane — uncommitted, the branch
against its base, the last turn, or one of the branch's commits — whose
lines take comments; it also searches the checkout and shows the branch's
pull request. herdr-reviewr's, vendored. The comments wait on the Composer
and go to the Thread with its next prompt.
_Avoid_: compare (the read-only view it replaced), code review (a PR's).

**Notice**
Ferrite's own record that a Thread's Main finished for good: its turn ended
and no Subagent is still working, so the operator is wanted back. Unread
until it is **seen** — its Thread on screen while the window has focus —
or opened from the Needs-you view; shown there, by a toast, a chime, the
Thread's done mark, and a system notification
(herdr-focus-notify's) unless it is seen first or its Thread is on the
board while the window has focus. A Subagent finishing, an interrupt, or a held
prompt going out is never a Notice.
_Avoid_: alert, ping, notification (the system's own surface is that).

**Attention**
Which of four states a Thread is in, most urgent first: **blocked** (a
Decision waits), **done** (a Notice not yet seen), **working** (Main, or
only its Subagents or background tasks), **idle**. What the nav sorts and
rolls up by and the palette filters by; Ferrite's finer states (failing,
background, failed, parked) are faces within these.
_Avoid_: status (too broad), unread (that is the Notice's).

**Needs-you view**
The nav's list of what wants the operator, shown in the tree's place: the
blocked Threads oldest-first (the queue ⌘D walks), then the Notices under
`Recent`. Opened from the nav's `Needs you` row, the Bar's count or ⌘I. It
replaced the bell's list.
_Avoid_: bell, inbox, needs-you strip (the strip in the tree is gone).

**Attention list**
The nav's always-on list, over `Projects`, of the Threads in flight by
their Attention: `Needs you` (blocked, oldest request first), `Done` (a
Notice not yet seen, newest first) and `Working` (creation order). Each
row is one line. A section with no rows is not drawn, and each section
folds to its head and count. The list is herdr's Agents panel without the
idle Threads; the tree below still lists every Thread. A finish never
leaves from under the operator: the focused Thread is held under `Done`
when it was listed there (a done Thread opened from it stays, read) or
finished while the operator watched it (seen at once, it moves from
`Working` to `Done`). It leaves when focus moves to another Thread.
Nothing else is held, so no row stands in a section that no longer says
what happened. That hold is the nav's; the Notice is read as soon as it
is seen.
_Avoid_: smart filtering (the setting it replaced), agents panel
(herdr's), queue (that is ⌘D's).

**Draft**
A Pane's pending Provider, Effort, Project, and Workspace choices before its
first prompt creates a Thread. It has no durable history or Session.

**Current Thread**
The Thread the operator is in: the focused one, else the open Thread focus
last left (focus on a Draft). A new Thread starts in its Project; only a
Project named outright (the palette's "New thread in …") outranks it.
_Avoid_: active thread, last thread.

**Remote**
Following and answering Decisions away from the desktop. Wanted soon;
scope under discussion.

**Server**
The headless Ferrite process (`ferrite server`) that owns every Session,
the Threads' history and the pending Decisions, so agents keep running
when no window is open. Windows are its clients; several may attach.
_Avoid_: daemon, backend (that is a Provider), host.

**Machine**
Another computer whose Server a window reaches over ssh (`ferrite --remote
<target>`, or saved in Settings). The window is on one machine at a time —
this computer, `local`, or another — and watches only that one's Threads;
the others stream status to the nav. A machine runs `ferrited`, the Server
without a window.
_Avoid_: host, remote (as a noun), node.

**API**
The Server's automation surface: requests and answers as JSON lines on a
socket of its own, spoken by the `ferrite` CLI, so a script or an agent in
one Thread can start, prompt, wait on and read other Threads. Only the
operator answers a Decision; an agent passes the operator's answer on.
_Avoid_: MCP, RPC, plugin API.

**Board**
Projects' one list of tasks (ADR 0031, herdr-projects 1.0 vendored), every
repository's on this machine, each task tagged with its repository and
grouped by what it waits for: needs you, ready to approve, working, to do,
done. Its data is herdr-projects' home folder, `~/.herdr-projects`;
its command line is herdr-projects 1.0's program, shipped in the app as
`ferrite-projects`.
_Avoid_: kanban, queue, project (a Ferrite Project is a registered
repository root).

**Task**
One piece of work on the board: yours, or delegated to an ordinary agent
that works it in a worktree and branch of its own, as a Ferrite Thread.
Finished work goes down the finish line (committed, main merged in, the
repository's check, a draft pull request, a second agent's review) and
merges only on the operator's yes unless its settings say otherwise.
_Avoid_: ticket, job, project thread.

**Coordinator**
The board's own agent, one long-lived chat at the bottom of the board:
what the operator types becomes tasks, added or delegated. It plans and
delegates; the tasks' agents do the work.
_Avoid_: manager, orchestrator, lead agent.

**Keeper**
The small background process (`ferrite-projects keeper`) that carries the
board's tasks through the finish line and landing while no board is open.
_Avoid_: ticker, daemon.

**Automation**
A rule that adds a task, and puts an agent on it, on a schedule or when
something happens on the board (a merge, say).
_Avoid_: routine, cron job.

**Work board**
herdr-projects 1.0's board (ADR 0031): the list of tasks by need — needs
you, ready to approve, working, to do, done — each delegated to an agent,
with the chat to the Coordinator under it. Opened by the projects key in
the board's place (`board.open: tab`) or as a sheet over it (`popup`).
_Avoid_: dashboard, kanban; "the board" alone is the Cockpit's Panes.

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
  semantic zoom.
- v1 git: workspace binding only (worktree or main); no checkpoints yet.
- Issue-tracker/board integration: later, not v1.
- Platforms: macOS (Metal) AND Windows (DirectX 11) first-class from the start.
- Remote: full control (watching, Decisions, prompting, Thread creation) —
  wanted soon after v1 core.
- Accessibility: no v1 story (GPUI limitation); one honest README note, topic
  closed until GPUI grows AccessKit-style support upstream.
- Own repository at projects/ferrite (codingOS is an ignoring holder repo).
- Sessions follow attention (amended 2026-10-20, ADR 0032; was "open Pane =
  live Session"): a Thread costs memory and a provider process only while it
  is used. Closed Pane = parked Thread; an idle open Thread's Session ends
  and restarts on demand, invisibly. A watchdog still restarts leaking
  Sessions visibly.
