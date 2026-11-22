---
status: accepted
date: 2026-11-22
---

# The Server watches PRs for Threads

An agent that opens a pull request should hear when its checks fail,
when a reviewer comments, and when it merges, without anyone telling it.
T3 Code does this: its agent calls `watch_pull_request`, and T3's server
wakes it with the news. Ferrite does the same, in T3's words and by T3's
rules (ADR 0038: T3 Code is the reference). This ADR records where the
watch lives, how the wake reaches the agent, and where Ferrite differs
from T3 and why.

## Decision

- **The Server watches, not a window.** A watch is the agent's, and the
  agent runs on the Server (ADR 0025). So the Server reads the PR with its
  own `gh` account, and a watch goes on while no window is attached. The
  window's own PR reading for the nav (`PrReads`, ADR 0041) is a different
  thing and stays.
- **A watch is a header fact on its Thread.** `WatchFact` (`pr_watch`)
  keeps each watch that is on and the last ended one per PR. It is amended
  through the Store as Snooze is, so it survives a restart. News is saved
  before its wake is held (persist, then act), so a restart repeats
  nothing. Watching a PR again starts from its ended watch, as T3's
  `thread.pull-request.watch` does. `Store::fork` copies no watch: the
  watch's wakes are its source's agent's.
- **The watcher is T3's `PullRequestWatchReactor`.** A sweep every two
  minutes, after the last one ends. One read per PR, however many Threads
  watch it, and at most four at once, off the pump. T3's plan for each
  read: a cheap fingerprint first, and the full read only when it moved,
  when checks are still settling, or after 10 minutes without a
  fingerprint or 30 with one. A new watch is read at once, and that read
  is its baseline: what is already on the PR is recorded, not told. A
  cursor that only moved is saved at once. News is held until the pass
  ends, so each Thread gets one wake for everything the pass found.
- **Reads go through `gh api graphql`.** These are T3's queries:
  fingerprint, detail, check pages, comment, review and review-thread
  pages, plus a REST read of the head's workflow runs for unfinished and
  approval-waiting runs. So every `Change` T3 can tell is filled, and the
  agent's own remarks are known by the viewer's login. They sit behind
  core's `GitHub` seam, the same seam the Factory's GitHub reads use, so
  a test scripts the reader and the clock.
- **A wake is a server wake, not the operator's prompt.** `wake::Wake` is
  sent as a turn of its own (`Engine::send_wake`). It has its own `wake`
  log record (schema 19), and its transcript block is a prompt marked with
  a `WakeNote` (its `summary` and outcome), so a window can draw T3's
  compact row. It is never stored as a `prompt` and never goes through
  `Op::Send`. It is general: a Delegate's result is the other kind
  (ADR 0047), held in the same one-per-Thread slot.
- **When a wake goes.** It waits while a turn runs or prompts are queued,
  and starts an ended Session. While a Decision is pending, or a wake
  already waits for the Thread, the pass saves nothing for that Thread, so
  the next read finds the same news. So a Thread has at most one wake
  waiting. A waiting wake is dropped when its watch ends or moves to
  another head (T3's `isStalePullRequestWatchStamp`). A wake for several
  PRs keeps the sections still current.
- **How a watch ends.** A merge or a close ends it with one wake, and
  with none when the agent's own `gh pr merge|close` or `gh stack merge`
  named the PR (T3's `agentEnds`). That command counts while it runs. If it
  succeeds, it counts until the read made at its end is done, so a later
  `--auto` merge is still news. Interrupting the agent or parking its
  Thread ends every watch with no wake. Escape interrupts only while a
  turn runs, as T3's Stop does (`canInterruptRunningThread`), so at rest
  it leaves the watch on. Ten comment-only wakes in a row
  end it, as in T3. A rate limit pauses the watch. A lost connection is
  tried again without counting. Any other failed read counts, and eight
  in a row give up with T3's give-up wake.
- **A watch that holds is background work.** `ActivityView::background_work`
  counts it, so the Thread sits on the Working shelf, its Notice waits
  until the watch lets go, and the background line lists `Watching pull
  request #N` (T3's `monitor` task), `Waiting` while it holds. A Session's
  rest (`settled`) does not count it: the watch is the Server's, so it
  keeps no Session and holds back no restart.
- **A wake draws as T3's notification row.** Its Block's `WakeNote` makes
  the transcript draw a work row (its summary, an eye, or the alert for a
  failure) that opens to the whole text, where a prompt draws a bubble. It
  opens a turn as a prompt does, and is never the Thread's title, the
  prompt `Copy prompt` reads, or one `Retry` sends again.
- **The window hears a watch through the Thread's digest.** `Digest`
  carries `Watching`: each PR watched, by URL and number, whether it holds
  the Thread working (T3's `awaited`: `until: merged`, or checks still
  running on the head), and whether a wake waits. The Server sends a new
  digest when any of it changes, for a held Thread. The pump copies it
  into the window's Facts, the one copy the card's eye, the palette, the
  PR menu and the background line read. A digest received during one of
  the window's own acts still waits for the pump: a watch that holds
  nothing has no later message to bring it.
- **`pr.watch` and `pr.unwatch`** start and stop a watch over the API
  (ADR 0027), for the calling Thread unless one is named. Anyone may call
  them: a watch changes only what news a Thread's agent hears. A new watch is
  read fresh first: a PR already merged or closed starts no watch and
  says so in T3's words, and whether checks were still running seeds the
  watch, so a failure CI makes right after the call is news.

- **The agent's tools are T3's.** Every Thread's Main Session, on Claude
  and Codex, is offered `watch_pull_request` (with `until`) and
  `unwatch_pull_request` in T3's words, with Ferrite in place of T3 Code.
  They are Ferrite's own tools (`api::tools`), pre-allowed, and a call
  reaches the Server as `pr.watch` or `pr.unwatch` from the Session's own
  Thread. A subagent's watch is refused in T3's words before the Server
  hears it. Claude's call counts as Main's unless the stream named its
  `tool_use` a subagent's (`parent_tool_use_id`, the check the receipt
  for `update_plan` makes); Codex's counts as Main's when it comes on
  Main's own thread.

## Where Ferrite differs from T3, and why

- **There is no change feed.** T3 also reads a PR when its host reports
  a change. Ferrite has no webhook or subscription, so it waits for the
  sweep, as T3 does for any host without one.
- **Groups are by PR, not by Project and PR.** T3 groups by Project
  because checkout access differs between Projects. The Server reads
  every PR as one `gh` account, so one read serves every Thread.
- **Detail and remarks are one query.** T3 reads them apart. A failed
  remarks page still leaves the remarks unread while the checks reach the
  evaluator, as T3's does. Approval-waiting runs are one REST read, with no
  guard for a head two PRs share. Review-thread tails are read again on
  each full read: there is no tail cache.
- **A waiting wake is not durable.** T3 queues the wake as a message in its
  store. Ferrite saves the news and holds the wake in memory. A restart
  between the two loses that one wake, but never repeats it.
- **A subagent sees the tools and is refused.** T3 hides them from a
  subagent. One Claude or Codex process serves Main and its subagents
  with one tool list, so Ferrite refuses the call instead, in T3's words.
- **The tools take no thread or host.** A Session's tools are about its
  own Thread (`api::tools`), and the Server reads only GitHub. A Codex
  Thread resumed from before the tools existed keeps its old tools, and
  watches through `ferrite pr watch`.
- **The API answers in snake_case** (`was_watching`), as every operation
  of ADR 0027 does.
- **The operator stops a watch, and starts one where a Server runs.**
  T3's details card stops a watch from its eye, and its PR menu starts and
  stops one (`Watch for changes`, `Stop watching`). Ferrite's do the same,
  and the palette has `Watch PR for changes` and `Stop watching PR`. They
  ask through `Op::WatchPr` for the window's Thread. A start needs the
  Server's watcher, so a window with no Server shows only the stop.
