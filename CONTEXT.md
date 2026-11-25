# Ferrite — Context

Ferrite is an open-source, native (GPUI) multi-provider agent cockpit: many
coding-agent conversations run in parallel and are watched, steered, and
approved from one window, one Thread at a time. This file is the project's
canonical language — glossary only, no implementation.

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
additional Ferrite Sessions or Threads; a Delegate is a Thread started on
purpose, on another Machine, and is not observed here.

**Digest**
What every window knows of a Thread it does not draw: what Main is doing,
the Decisions it waits on, its Wall card, its counts and how its latest
turn finished. A window holds a Thread's whole Activity only
while it draws its Transcript.
_Avoid_: summary; not the Handover's digest of earlier exchanges.

**Subject**
The execution context a Pane is showing within its Thread: Main or one
Subagent. Switching Subject changes the view, not the Thread or its Title.

**Main**
The Thread's primary agent context, addressed by its Composer and interrupt
control. Main can be idle while a Subagent is working; interrupting Main
stops its turn alone, never a Subagent, a Delegate or background task.

**Subagent**
A provider-managed agent delegated within Main's ancestry, observed through
the owning Session. It may delegate further or work again after completion;
its transcript and Decisions remain its own. Escape on its tab stops it alone.
Work handed to another Machine is not a Subagent: it is a Delegate, a
Thread of its own there.
_Avoid_: Thread, Session, worker Pane.

**Provider**
An agent backend a Session runs on. v1: Claude and Codex.
_Avoid_: model (a provider selects models; it is not one).

**Pane**
The board's one plane, showing one Thread: its transcript and its Composer,
at any size. A view, never an identity.
_Avoid_: window, tile, terminal, fullscreen Pane (Solo already fills the
board).

**Cockpit**
The window's main area. It shows one Thread at a time (**Solo**), or the
empty board when none is open.
_Avoid_: dashboard, workspace (see Workspace binding), split, Group view.

**Solo**
The Cockpit's one view: exactly the focused Thread's Pane. ⌘{ / ⌘} step it
to the previous or next open Thread in the nav's order, and ⌘1…⌘9 go to
one by place; a parked Thread is skipped, never revived.

**Panel**
The right column, beside the board: the focused Thread's **Surfaces** as
tabs (ADR 0039). It follows the focused Thread, with a Review open beside
it too, shown while the operator has it on; there is none without a Thread (a
draft, the empty board), but for a draft's preview.
_Avoid_: sidebar (that is the nav), Beside slot (that opens inside the
board), dock (its columns are Surfaces now).

**Surface**
One tab in the Panel, one per thing it shows: the Overview, Changes,
Files, a Preview, a Browser tab, a Terminal, a side chat, background
work, a pull request. Opening one already open brings its tab forward.
Each Thread keeps its own. A Subagent is not one: it opens in the Pane,
which switches its Subject to it.
_Avoid_: section, pane (that is the board's), view.

**Preview**
A file, an image or a command's output opened from a Pane — a file link
in an answer, a ⌘-clicked path, a tool row, a prompt's image, a shelf
chip, a changed file, a review's line — read as a Surface in the Panel,
which widens while it shows. Each file gets its own tab; opening one
already open brings its tab forward.
_Avoid_: reader (the board's reader slot is the Review's), viewer (that is
the image's part of it).

**Bar**
Gone. The window has no strip at the bottom: usage, and free disk when it
runs low, are in the nav's foot. A Thread's mode, model and Effort are
chips in its Composer, and its context and changes are the Panel's.
_Avoid_: bottom bar, status bar, status line (all gone).

**Group**
Gone (ADR 0025's 2026-11-17 amendment). An older Store's `groups.json` is
left as it was and never read, so its members are ordinary Threads. Only
generation 1's protocol still spells the word.
_Avoid_: folder (the nav has none).

**Roster**
The Cockpit's state, headless: the open Panes in order, the one holding the
keyboard (the one on screen, or none: another machine's Thread that merely
opened is not where the operator is), the drafts, and this launch's park
order. Changed only by the operator's acts on the Cockpit (close, reopen,
focus); the window mirrors it and paints.
_Avoid_: layout (that is the board's split), tab list.

**Layout**
The board's split while a Review is open beside the shown Pane: the Pane
and the Review's reader slot, the seam between them resizing it, kept for
the session only. Files and images open in the Panel's preview instead.
_Avoid_: grid, mosaic, board of Panes (one Thread shows at a time).

**Title**
What a Thread is called: the operator's own name for it, else a short
title a small model wrote from the first prompt (the titler, run once by
the Thread's Server, off by turning *Name threads automatically* off for
that machine), else the first line of its first prompt, else its number.
Told to the Session where the Provider takes one.

**Effort**
A Thread's reasoning depth, one rung of the ladder its model announces
(Claude: low…max; Codex: low…ultra, per model). Chosen per Thread by the
effort chip or `/effort`, defaulting to Settings. A Claude Session starts at
it (`--effort`, as T3 Code starts one). Changes apply to the next turn on the
same Session; they never restart it or consume a pending Handover.

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

**Sheet**
A dialog that covers the whole window over a dimmed veil: Settings, the
Project card, Background work, New task, and a Task's Rework or Direct
this task. One is up at a time.
_Avoid_: modal.

**Popup**
A menu or small card that hangs from a point or a control over the window,
with no veil: the context menu, the permission mode menu, the plan,
checks and session controls cards, and the Composer's model picker, band
menus and attach menu. One is up at a time. While it is up its keys are
its own; Escape or a press outside closes it.
_Avoid_: float, overlay.

**Menu button**
A button that opens the context menu under itself: the nav's Filter and
Factory, a nav card's Snooze, the view title's chevron, the more doors
(a page card's `⋯` among them) and the Panel's `+`. The menu opens as the pointer goes down, and a
second press on the button shuts it. While the menu is up the button
reads as open and shows no tooltip.

**Composer**
The prompt box at the bottom of a Pane: text that wraps and
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
Pane's checkout label reads the binding; nothing else moves it. A
worktree Ferrite makes lives inside its project, at
`<root>/.worktrees/<branch>`, kept out of `git status` by the repo's
`.git/info/exclude` (never its tracked `.gitignore`); one made before that
stays where it is, since a Session is keyed to its cwd.

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

**Conversation browser**
The browser tabs that belong to one Thread, shared by the operator and that
Thread's agent.
_Avoid_: agent-private browser.

**Browser annotation**
Feedback attached to a selected page element or a marked region of a page,
using comments, drawings, or both.

**Review**
A diff of a Thread's changes beside its Pane, the one thing that stands
beside one — uncommitted, the branch
against its base, the last turn, or one of the branch's commits — whose
lines take comments; it also searches the checkout and shows the branch's
pull request. herdr-reviewr's, vendored. The comments wait on the Composer
and go to the Thread with its next prompt.
_Avoid_: compare (the read-only view it replaced), code review (a PR's),
beside Thread (no Thread opens beside another).

**Notice**
Ferrite's own record that a Thread's Main finished for good: its turn ended
and no Subagent or background task (a shell, a monitor) is still working,
no Delegate is open or still to wake it, and no PR watch holds it (checks
still running, or waiting for the merge), so the operator is wanted back.
Unread
until it is **seen** — its Thread on screen while the window has focus —
or opened from the Inbox; shown there, by a toast, a chime, the
Thread's done mark, and a system notification
(herdr-focus-notify's) unless it is seen first or its Thread is on the
board while the window has focus. A Subagent finishing, an interrupt, or a held
prompt going out is never a Notice.
_Avoid_: alert, ping, notification (the system's own surface is that).

**Attention**
Which of four states a Thread is in, most urgent first: **blocked** (a
Decision waits), **done** (a Notice not yet seen), **working** (Main, or
only its Subagents, background tasks or Delegates), **idle**. What the nav sorts and
rolls up by and the palette filters by; Ferrite's finer states (failing,
background, failed, parked) are faces within these.
_Avoid_: status (too broad), unread (that is the Notice's).

**Inbox**
The nav itself (ADR 0040): one list of thread cards, each carrying its
Project's badge, for the open Threads awaiting the operator. Pending
Decisions come first, oldest request first, then the Thread that came
back latest: a Thread done with its work returns at the top, and nothing
else moves a card. Threads executing Main, Subagent, background or
queued work, or held by a PR watch, sit on the Working shelf, the latest to
start first; Snoozed and
Parked Threads on their own shelves; the shelves fold to a head and count
at the foot of the list. The rest stay in Inbox until sent back to work or
Parked. Reading or dismissing a Notice changes presentation, never queue
membership. Inbox can be empty while Working remains populated. Inbox
counts Threads; unread and blocked indicators keep their own meanings.
_Avoid_: Settled (T3's word; Parked is Ferrite's), Projects tree and
Attention list (the nav's former structure), Needs-you view (the former
name).

**PR watch**
A Thread's agent asking the Server to keep reading one of its pull requests
and wake it with what changes: failed checks, passed required checks, new
comments or reviews, conflicts, and once when the PR merges or closes,
unless the agent merged or closed it itself. The Server reads it every two
minutes with its own `gh` account and keeps the watch on the Thread, so it
outlives windows and restarts. Stopping the agent or parking the Thread
ends it. A fork watches nothing. While the PR's checks run, or until it
merges when the agent asked so, the watch holds its Thread working; the
operator stops one from the details card's eye, the PR's menu or the
palette, and starts one from the last two.
_Avoid_: subscription, PR monitor.

**Server wake**
A prompt the Server sends a Thread's agent on its own: a PR watch's news,
or a Delegate's result. It is never the operator's prompt: the log keeps it
as a wake, and a window draws it as a one-line notice row with its summary,
which opens to its whole text. It waits while a Decision is pending or a
turn runs, and starts an ended Session.
_Avoid_: notification (a toast), message.

**Snooze**
A temporary deferral of an open Thread until a chosen local time or duration.
Snoozing keeps its history and agent execution intact. A new Decision, a
failed run or an operator follow-up wakes it early; an ordinary completion
or reading a Notice does not. Snoozed Threads stay out of Inbox and Working
until they wake, and remain available under the collapsed Snoozed shelf.
The Server saves Snooze for every window and restores it after a restart.
_Avoid_: Park (the separate action for finished handling), stopped agent.

**Draft**
A Pane's pending Provider, Effort, Project, and Workspace choices before its
first prompt creates a Thread. It has no durable history or Session. Its
environment is its Project's Machine, a choice on its environment chip
until that first prompt: picking another moves it to its Project's
counterpart there, made by a copy when there is none (ADR 0048). The Draft
waits on that copy, with no Send, and moves there once it is in; a copy
that stopped short keeps it waiting until it is resumed or the operator
picks where the Draft runs.

**Current Thread**
The Thread the operator is in: the focused one, else the open Thread focus
last left (focus on a Draft). A new Thread starts in its Project; only a
Project named outright (the palette's "New thread in …") outranks it.
_Avoid_: active thread, last thread.

**Remote**
Following and answering Decisions away from the computer the agents run
on: a window holding another Machine's Server.

**Server**
The headless Ferrite process (`ferrite server`) that owns every Session,
the Threads' history and the pending Decisions, so agents keep running
when no window is open. Windows are its clients; several may attach.
Each has a stable id kept in its Store (`ServerId`), which its `welcome`
and the API's `identity.get` name on every route to it.
_Avoid_: daemon, backend (that is a Provider), host.

**Machine**
Another computer whose Server a window reaches over ssh (`ferrite --remote
<target>`, or saved in Settings) or over a remote link (`ferrited remote
serve`, `docs/guides/remote-links.md`). The window holds every enabled
machine at once — this computer, `local`, and the others — each a held
machine in its one Cockpit, and opens any of their Threads in place
(ADR 0045). The nav is one list across them: a Thread on another
machine sits among this computer's and wears a small server glyph.
Another machine streams a Thread's transcript only while the window
draws it, and its status, Decisions and Notices for every Thread. A
machine runs `ferrited`, the Server without a window.
_Avoid_: host, remote (as a noun), node.

**Peer**
A saved Machine's Server as this Server asks it (`peers::Peers`): through
the machine's API, as the Server's own caller, never a Thread's. Known by
its `ServerId`, so a machine pointed elsewhere is never taken for it.
_Avoid_: linked environment (T3 Code's word), remote.

**Delegate**
A Thread one Thread's agent started on another Machine to do one piece of
work (T3 Code's delegated task in a linked environment). The starting
Thread's Server keeps its record on that Thread, follows it there through
its Peer until it ends, and wakes the agent with its result. Until then
its Thread counts as working. A cancel, or stopping or parking its Thread,
ends it here at once and stops it there, retrying a stop that cannot get
there yet; interrupting Main does not. It may start Delegates of its own.
An agent starts one with T3 Code's tools (`delegate_task`) while both
Machines have Settings › Labs › Linked machines on. The operator sees it at
its `delegate_task` call in its Thread's transcript and in its Lineage, where
they open or stop it, and while it waits on them the foot of that transcript
says so and opens it there. Its own Thread there names the Thread that
delegated it, over its rows and in its Lineage.
_Avoid_: child, subagent (a Provider's), task (the Factory's), remote
thread.

**Placement**
Where a Project of this computer lives on a Machine: the folder its copy
was sent to there, kept for every copy (`placements.json` in the Machine's
own directory, ADR 0048). A Project with a repository also finds its
counterpart on a Machine by that repository.
_Avoid_: mirror, sync target, remote path.

**Unfinished copy**
A copy of a Project to a Machine that began and has not ended: cancelled,
cut off, or stopped with the app. This computer keeps it beside the
Machine's Placements (`unfinished.json`) from before its first byte until
it ends, or until it stops with nothing landed. Only its Resume changes its
destination: it sends what is not there, and leaves each file someone else
changed there as it is and lists it; it removes nothing (ADR 0048).
_Avoid_: partial copy, interrupted sync, half copy.

**Copy to a Machine**
One run of copying a Project of this computer to a Machine, one at a time
for each Project and Machine, the Machine as it is saved (turned off and on
again, its copies are still its own). The window keeps it from Copy until
the operator has seen how it ended, and one that stopped short while its
Draft waits on it; a toast that went by itself is not seeing it, unless
the copy's Draft moved there and it left nothing as it was, which leaves
nothing to see; its system notification goes with it. Its card,
a toast and every Draft's environment chip all show it. Cancel, or its
Machine turned off or removed, stops it unless all of it had landed by
then: it is stopped or done, never both. The card closed, it runs on. A
Machine removed takes it with it: it can never go again (ADR 0048).
_Avoid_: transfer, upload, sync job.

**Setup sync**
The operator's agent setup kept the same on this computer and each Machine,
both ways, with this computer as the hub. What the operator wrote travels as
files: skills (`~/.agents/skills` too), commands, agents, instructions,
rules, hooks and output styles. What they chose travels as keys of the files
their tools keep, by an allowlist: preferences, permissions, plugins and the
marketplaces they come from when those are fetched from elsewhere, and MCP
servers that run nothing only one side has. The rest of those files (hooks,
the status line, the environment), and all a tool derives (plugin
downloads, caches, install records), never moves; each side's tool installs
what was turned on. It runs on its own as a Machine connects, as the setup
here changes and every minute, unless that Machine's "Sync my setup" is
off. A file both sides changed goes to the newer, and the other copy is
kept; a key goes to this computer's at the first sync and to the newer
after, and the toast says what it was. Text that differs only in line ends
is the same. A folder on one side where the other has a link stays as each
has it unless it holds only what the link leads to. Two sign-ins that
differ with none agreed wait for the operator to pick one, and a sign-in is
never removed: a side signed out takes the other's. A sync that would
remove most of an item, or all of one, waits for "Sync anyway". What a sync
removes or writes over on either side is kept in that side's trash for 14
days. One sync of a home runs at a time, whichever computer drives it: a
sync holds this computer's home and the Machine's from its first scan to
its end, and an install holds its home as a sync does; a sync that finds
either held tries again a few seconds later. Both sides must sync by the same table, or the row says which Ferrite
to update, and a base another table wrote is read as none (ADR 0048, ADR
0050).
_Avoid_: dotfile sync, backup.

**API**
The Server's automation surface: requests and answers as JSON lines on a
socket of its own, spoken by the `ferrite` CLI, so a script or an agent in
one Thread can start, prompt, wait on and read other Threads. Only the
operator answers a Decision; an agent passes the operator's answer on.
_Avoid_: MCP, RPC, plugin API.

**Factory**
A Project's delegated work: its Coordinator, its Workflows and the board of
its Tasks. The operator hands work to it and comes back for questions and
approvals. One per Project, on the Server that owns the Project.
_Avoid_: board (that is one view of it), projects, pipeline.

**Workflow**
A process the operator authored for a Project: ordered Steps, how Tasks
start, who approves and what finishes one. Saved in numbered versions; a
Task keeps the version it started with. The operator's own, never part of
the repository.
_Avoid_: pipeline, automation, flow.

**Step**
One position in a Workflow: a name, instructions, skills, Provider, model,
Effort, and where a Task goes on each outcome the Step can report. The
window may label a Step "agent".
_Avoid_: stage, node, role.

**Task**
One piece of delegated work moving through one version of a Workflow, or
through a single Step when handed off one-off. It is queued, working,
needs the operator, ready to approve, stopped or completed. It carries its
instructions, the operator's answers and feedback, the evidence its Steps
reported and the pull request it concerns, if any.
_Avoid_: job, ticket, issue (the tracker's), Thread (a Task is not a
conversation).

**Worker**
The Thread running one Step for one Task. Owned by its Task: reached
through the Task, never listed among the operator's own Threads, never in
the Inbox.
_Avoid_: agent (the Provider's), subagent (that is a Session's own), child.

**Coordinator**
A Project's one Thread for directing its Factory in conversation:
discussing work, proposing a Task, a Workflow change or a redirect, which
the operator accepts or dismisses. Owned by the Factory as a Worker is by
its Task. Talking to it starts nothing.
_Avoid_: orchestrator, manager, assistant.

**Not a harness, not an agent.** Ferrite never runs its own agent loop and
never calls model APIs itself. It orchestrates official agent CLIs (each
Provider's own harness) and renders what they stream. The agents' behavior,
auth, and ToS posture belong to their vendors, not to Ferrite.

**No agent in a terminal.** Agents run as headless processes streaming
structured events, never in a PTY. The one terminal is a Surface: the
operator's own shell in the Thread's folder.

## Settled product decisions (2026-08-25)

- Open-source project; dual-licensed MIT + Apache-2.0.
- Name kept: Ferrite (iOS audio app collision accepted — distant category).
- v1 providers: Claude + Codex, minimal.
- v1 cockpit: Solo by default; an opened Group shows its exact membership with
  semantic zoom. Amended 2026-11-17: one Thread at a time, and Groups are
  gone; their members are ordinary Threads. Amended 2026-11-17: semantic
  zoom is gone; a Pane draws its transcript and Composer at any size.
- v1 git: workspace binding only (worktree or main); no checkpoints yet.
- Issue-tracker/board integration: later, not v1. Amended 2026-11-10 (ADR 0037):
  a Project has a Factory; the operator's tracker stays the tracker.
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
