# sw-flow

herdr-projects 1.0's `crates/flow`: the flow of a task: delegating, the finish line, landing, the keeper, other machines, automations, the review page.

Vendored verbatim from [herdr-projects](https://github.com/eliasstravik/herdr-projects)
at commit `c019063b65b3ff6a68688345b9f304533c8fb83c` (1.0.1+8), MIT licensed,
Copyright (c) 2026 Elias Stravik — see `LICENSE`. The vendored crates, their
seams and how upstream updates merge are in `../ferrite-projects-core/README.md`
and `docs/adr/0031-projects-1-0-vendored.md`.

## Ferrite changes

None inside upstream's files. Ferrite's own tests, in `tests/`, hold the
seams to the flow: `ferrite_program.rs` (the launcher and turn-end hooks
run the bundled program) and `ferrite_hook_events.rs` (the events
Ferrite's Server hands `hook`, from ferrite-core's
`server/fixtures/projects-hook/`, reach their tasks as herdr's do). How
the Server runs the keeper and hooks: ADR 0031, Seam 4.

## Each flow behaviour and what proves it

The flow is upstream's, unchanged; it reaches Ferrite only through herdr's
calls (ADR 0031, Seam 1), the hook's events and the keeper (Seam 4), the
program's own path, and the agent programs' turn-end hooks. Upstream's
proof of each behaviour is its own test program
(`crates/ferrite-projects/tests/`, against a stand-in herdr). Through
Ferrite, what this crate's seams and the Server decide is proved here, in
`crates/ferrite-projects/tests/ferrite.rs` and in ferrite-core (named
below). What the rows marked "adapter" do through Ferrite is only herdr's
calls, which the adapter (`sw-core`'s `herdr/ferrite.rs`) answers from
the Server (Seam 1), each call tested there against a stand-in Server
(`herdr/ferrite/tests.rs`); the rows themselves are proved against
herdr's stand-in only, as upstream's test program runs on it.

| Behaviour | Upstream's proof | Through Ferrite |
|---|---|---|
| Hook on Thread events (agent detected, status, pane closed, worktree removed), housekeeping once in 2 s | `lifecycle*.rs`, `cleanup_*.rs` | `tests/ferrite_hook_events.rs`; ferrite-core `server::projects::tests`, `server::tests::projects_hook_hears_threads_as_herdrs_panes`, `server::tests::projects_hook_hears_a_removed_worktree`; a Thread deleted with its tree: `cockpit::tests::deleting_a_legacy_worktree_thread_tells_its_tree_removed`; no event's hook before the startup hook returned: `server::projects::tests::the_events_wait_for_the_startup_hook_which_looks_again_once` |
| Keeper: started by hooks, one per machine, restarted on Server start, ends with the Server, replaces itself on update | `keeper_update.rs`, `lifecycle.rs` | started by the startup hook once `take-over` returned: `server::projects::tests::startup_takes_over_then_runs_the_startup_hook_then_the_events`; past a keeper of the Server before: `…the_events_wait_for_the_startup_hook_which_looks_again_once`; ends with the Server: `Herdr::alive` (adapter) |
| A task an agent or the Coordinator delegates links this Server (`current_socket()`) | `delegate*.rs` | every agent is told its pane, herdr's socket and the program: ferrite-core `spawner::tests::while_projects_run_every_agent_is_in_herdrs_pane_environment`, `server::projects::tests::an_agents_pane_is_its_thread_as_its_hooks_name_it` |
| Turn-end detection (`turn-ended`, `plan-written`), helpers | `turns.rs`, `submit_turns.rs`, `cli.rs` | `tests/ferrite_program.rs` (hooks name the bundled program); Claude's `--settings` (`Stop`, `StopFailure`) last on its command line, with herdr's pane environment: ferrite-core `tests/claude_session.rs` `projects_settings_reach_the_cli_after_ferrites_flags`; Codex `notify` to the app-server: `providers::codex::launch` tests; that each CLI then runs them: a live run only (ADR 0031, Seam 4) |
| Finish line: commit leftovers, wrong-branch guard, sync with main, conflict send-back and `conflicts.jsonl`, `check.command` with progress and `check.again`, setup retry, push and draft PR, already merged / no change, archived repo | `flow*.rs`, `guard*.rs`, `sync*.rs`, `check_*.rs`, `pull_requests*.rs`, `finish_missed.rs`, `endings.rs` | git and gh run as plain processes with the Server's login environment; its agent calls: adapter |
| Reviewer Thread per round (`Reviewer #N`, `Plan review`, `Picker` titles), verdict, gate, send-back rounds, PR comments, `reviewneeds` from Attention `blocked` | `tabs.rs`, `gate*.rs`, `talk*.rs`, `chat_review.rs`, `plan_rest.rs`, `attempts_rest.rs` | `blocked` reaches the hook as `agent_status`: `server::projects::tests::every_attention_is_a_status_herdr_has`; titles via `thread.rename`: adapter |
| Evidence page server, approve and merge (PR or local), merge line, checks gate, workflow triggers, fine-grained tokens, PR polling, `merge.by`, `requireApproval`, resync | `board_evidence.rs`, `merge*.rs`, `auto_merge*.rs`, `pull_requests_*.rs`, `remerge.rs`, `rearm*.rs` | plain processes (gh, git, the page server) with the Server's environment; no herdr call beyond the adapter's |
| Clean-up: workspace close, worktree remove never forced, quiet periods | `cleanup_*.rs` | `worktree.remove` is never forced (ferrite-core `server::calls::projects`); its `worktree.removed` hook: `server::projects::tests` |
| Provider retries, stuck recovery, waiting-for-you reports | `retry*.rs`, `rearm_stuck.rs`, `waiting*.rs` | the worker known as its task's agent from the pane environment every Session is told, so its `report --state blocked` is its own: `crates/ferrite-projects` `tests/ferrite.rs` `under_ferrite_a_tasks_agent_is_known_by_its_thread`; the rest: adapter (`agent.read`, `agent.prompt`) |
| Fallback hand-over (`Agent (codex)`) | `fallback*.rs` | `thread.revive`: adapter, which must keep the Thread open across the hand-over (ADR 0031, Seam 4) |
| Plan first (Claude plan mode, read-only chmod; bwrap on Linux, refusal on macOS) | `plan*.rs` | Claude's arguments pass through `thread.start --args`; the bwrap launcher prefix: adapter |
| Attempts in one Space, limits (time, tokens), after/chains (cross-machine), automations (schedules, events, runaway guard, firings), research-only, slots and queue | `attempts*.rs`, `limits*.rs`, `after*.rs`, `automations*.rs`, `research*.rs`, `board_queue.rs` | run by the keeper's tick and the hook, which run under the Server (above); their agent calls: adapter |
| Watcher one-shot (`claude -p` and the like) | `watcher*.rs` | plain processes, not Threads: no Ferrite call |
| Coordinator (staged Thread in the home Project, AGENTS.md/CLAUDE.md rules, restart, `coordinator new`) | `coordinator*.rs` | adapter (`project.open`, `thread.start`) |
| Reliable hand-over (`submit.rs`: READY_MS, SUBMIT_MS, Enter again) | `submit*.rs` | adapter (`agent.prompt`) |
| Start-failure explanations, `no_provider` for kinds Ferrite cannot run | `agent_start_failed.rs`, `agent_kinds*.rs` | `no_provider` is the adapter's refusal at `agent start` |

Still refused, and why:

- No Ferrite-side test runs the rows marked "adapter" end to end: see
  above.
- A herdr call Ferrite has no counterpart for (the terminal runtime's,
  herdr's own setup's) is refused `not_wired` before anything runs (Seam
  1), which callers treat as any refusal of herdr's.
- Agent kinds Ferrite has no Provider for (all but `claude` and `codex`)
  fail `no_provider` at `agent start`.
- The board inside herdr's web UI (`webui/`) finds no web UI and does
  nothing: Ferrite has none.
