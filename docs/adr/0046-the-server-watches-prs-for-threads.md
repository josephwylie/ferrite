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
  window's own PR reading for the nav (`PrWatch`, ADR 0041) is a different
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
  `Op::Send`. It is general: a Delegate's result will be the next one.
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
  Thread ends every watch with no wake. Ten comment-only wakes in a row
  end it, as in T3. A rate limit pauses the watch. A lost connection is
  tried again without counting. Any other failed read counts, and eight
  in a row give up with T3's give-up wake.
- **`pr.watch` and `pr.unwatch`** start and stop a watch over the API
  (ADR 0027), for the calling Thread unless one is named. A new watch is
  read fresh first: a PR already merged or closed starts no watch and
  says so in T3's words, and whether checks were still running seeds the
  watch, so a failure CI makes right after the call is news.

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
- **The API answers in snake_case** (`was_watching`), as every operation
  of ADR 0027 does.
- **What the window draws is not here.** The notice row, the Working
  shelf hold for `until`, and deferring a Notice belong to the window,
  and come with it.
