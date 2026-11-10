# Test suite audit: what to cut, what to make cheaper — 2026-11-10

Read-only audit at 5c6fae3d. Companion to [ci-times-2026-11-10.md](ci-times-2026-11-10.md), which covers caching and build time; this note covers the tests themselves.

**[M]** = measured here (M4, 10 cores, macOS 27.2, rustc 1.97.1, warm dependencies). **[CI]** = from the CI note's run logs. **[E]** = estimate. Nothing was edited; raw per-test timings are in `/tmp/ferrite-audit/out/` until that is cleared.

## Short answer

The suite is not slow because it has too many tests. It is slow because of about a dozen specific things:

- **860 of the 1,067 `ferrite-core` unit tests finish in under 50 ms each and together cost 3% of the unit-test CPU time** [M]. Deleting hundreds of them would save about two seconds.
- **Four unit tests cost 31% of it; the top 50 cost 75%** [M].
- Two integration binaries sit waiting on real timeouts (21 s of wall time between them) [M].
- About 270 tests write a fresh shell stub and execute it. On macOS the first exec of a new executable file costs about 260 ms; a symlink to one shared stub costs about 10-40 ms [M].
- 48 integration binaries each pay about 1.5 s of compile and link, and about 0.35 s of first-launch cost on macOS [M]. Six run no tests on macOS; 32 run none on Windows [CI].
- The vendored `ferrite-reviewr` suite runs on every PR although 4 of 1,308 commits touch that crate.

Rough total if everything under "make cheaper" and "merge" is done: **macOS `cargo test -p ferrite-core` from about 2m18s to about 1m10s, plus about 45 s when the reviewr step is skipped; Windows about 2.5-3.5 min per run** [E]. Only the macOS figures rest on local measurement; treat the Windows ones as guesses until measured there.

About **8-10% of `ferrite-core` tests are redundant or low-value** (roughly 120-150 of 1,458), mostly the same wire fixtures asserted at two or three layers. In the UI crates the share looks higher (15-20% from sampling, not a full read). Cutting those is worth doing for maintenance, not for CI time.

A separate finding: **the tests CI does not run are already broken.** `ferrite-ui` has 1 failing test and `ferrite-cockpit` 2, and the cockpit suite takes 5m24s on a 10-core machine because of one 275 s test [M].

## 1. Measurements

### Where `cargo test -p ferrite-core` goes

Locally the run phase is 92 s wall for 48 binaries run one after another (CI macOS: 2m18s for compile plus run) [M].

| Binary | Tests | Wall | Sum of test time | What sets the wall time |
|---|---|---|---|---|
| lib (`ferrite_core`) | 1,067 (+7 ignored) | 27.1 s at 10 threads, **43.3 s at 3 threads** | 174.8 s | one 20.8 s test; CPU everywhere else |
| `codex_session` | 44 | 14.4 s | 31.8 s | one test sleeps 2 × 6 s |
| `claude_session` | 39 | 6.8 s | 25.3 s | one test sleeps 6 s, one waits out a 5 s timeout |
| 18 × `contract_*` | 133 | 19.4 s in total | 46.5 s | about 0.35 s fixed cost per test |
| `subagent_cockpit` | 18 | 3.0 s | 8.2 s | five tests of 1-2.3 s |
| `provider_progress` | 26 | 1.6 s | 8.6 s | stub per test |
| the other 25 | 131 (+21 ignored) | about 20 s | about 16 s | launch overhead, 0.35-0.9 s each |

The 3-thread lib figure (43 s) matches the macOS runner's 3 cores and its 54 s [CI], so CPU sums are what matter on CI, not the local 10-thread wall time.

### The slowest unit tests (lib, time under the parallel run; solo time in brackets)

| Time | Test | Why |
|---|---|---|
| 20.8 s (17.4) | `server::tests::a_history_larger_than_the_lag_limit_still_arrives` (`src/server/tests.rs:827`) | pushes 36 × 1 MiB turns through a debug-build JSON link because `link::LAGGED_BYTES` is a 32 MiB constant (`src/link.rs:44`) |
| 14.5 s (12.3) | `store::revive_tests::a_bounded_revive_shows_what_a_full_replay_shows` (`src/store/revive_tests.rs:385`) | 24 seeds × 300-700 turns, each replayed twice |
| 12.9 s (5.8) | `activity::retention_tests::trimmed_history_reads_as_a_rebuild_from_the_retained_records` (`src/activity/retention_tests.rs:289`) | 100 seeds × 240 steps, rebuild compared at every step |
| 6.1 s (2.8) | `engine::tests::context::invariant_a_hand_over_after_revive_carries_the_whole_history` (`src/cockpit/tests/context.rs:96`) | 8 × 700 KB answers to exceed the real retention cap |
| 3.4 s | `server::tests::a_thread_drawn_again_arrives_whole` | large history |
| 3.4 s | `server::tests::a_small_history_never_waits_on_a_big_one` | large history |
| 3.0 s | `server::tests::a_review_is_read_by_git_where_the_server_runs` | git |
| 2.9 s | `engine::tests::trimming_the_displayed_history_never_loses_agent_context` | real cap sizes |
| 2.7 s | `link::tests::a_client_too_far_behind_is_lagged_and_nothing_more_is_queued` | fills 32 MiB (`src/link/tests.rs:120`) |
| 2.0 s × 3 | `a_restart_not_back_in_time_reads_failed_until_it_is`, `a_window_sees_its_restart_through`, `a_restart_remakes_a_missing_worktree_first` | wait on `SWEEP_INTERVAL` = 2 s (`src/server.rs:42`) or a restart |
| 1.8 s | `user_commands::tests::a_stop_ends_the_command_and_what_it_started` | fixed `sleep(1500 ms)` at `src/user_commands.rs:380` |

By module (sum of test time): `server::tests` 47.7 s / 102 tests, `engine::tests` 35.1 s / 138, `workspace::tests` 18.1 s / 31 (every one builds a git repo), `store::revive_tests` 17.3 s / 4, `activity::retention_tests` 12.9 s / 3, `store::durability_tests` 11.3 s / 32. Everything else is under 5 s.

### The slowest integration tests

| Time | Test | Why |
|---|---|---|
| 12.6 s | `codex_session::slow_thread_start_and_resume_survive_model_discovery` (`tests/codex_session.rs:834`) | stub runs `sleep 6`, twice in sequence, to outlast the old 5 s limit |
| 6.4 s | `claude_session::a_late_effort_answer_still_starts_the_session` (`tests/claude_session.rs:766`) | stub `sleep 6` |
| 5.3 s | `codex_session::a_silent_server_fails_the_spawn_within_the_budget` (`:820`) | waits out `HANDSHAKE_TIMEOUT` = 5 s (`src/providers/codex.rs:69`) |
| 5.3 s | `claude_session::a_silent_cli_leaves_its_capabilities_empty_rather_than_assumed` (`:702`) | waits out the same 5 s (`src/providers/claude.rs:65`) |
| 1.4 s | `codex_writer_conflict::a_resume_waits_out_the_previous_writer` | real backoff |
| 1-2.3 s × 5 | `subagent_cockpit::*history*` | history reload |

### Fixed costs

- **A freshly written executable costs about 260 ms on its first exec on macOS** (8 samples: 247-326 ms, one 699 ms). The second exec of the same file costs 13-40 ms. `/bin/sh <file>` on a file without the exec bit costs 13-31 ms. A symlink or hard link to an executable that has already run once costs 7-49 ms [M]. This is why every stub-driven test has a floor of about 0.33 s even single-threaded (`contract_sessions`: 0.33, 0.38, 0.43 s for three trivial tests).
- The same cost hits every new test binary: single-test binaries took 0.34-0.46 s on first launch and 0.00-0.02 s on the second [M]. CI always runs fresh binaries.
- Compile and link: 49 integration-test units cost 73.7 CPU-seconds, 0.3-2.5 s each (`cargo --timings`, clean target, CI's profile settings). The lib test build is 33 s and the plain lib 26 s [M].
- **Raising `ferrite-core` to `opt-level = 1` for tests does not pay**: the lib run drops from 43 s to 25 s at 3 threads, but the lib test compile goes from 33 s to 145 s [M]. Don't.

### Tests per file

`ferrite-core`: 1,458 tests ran or were ignored (1,074 lib, 384 integration; 28 ignored). Largest unit files: `cockpit/tests.rs` 123, `transcript.rs` 92, `server/tests.rs` 90, `providers/claude/wire.rs` 40, `providers/codex/wire.rs` 34, `store/durability_tests.rs` 32, `api/tests.rs` 32, `workspace/mod.rs` 31, `store.rs` 30. Largest integration files: `codex_session` 44, `claude_session` 39, `activity` 37, `provider_progress` 26, `import` 22, then 18 each for `subagent_cockpit`, `notifications`, `contract_fidelity`.

Other crates (by `#[test]`/`#[gpui::test]` count): `ferrite-cockpit` 842, `ferrite-reviewr` 313, `ferrite-pane` 246, `ferrite-ui` 119, `ferrite-focus-notify` 54, `ferrite-terminal` 11, `ferrite` 11, `ferrite-frame` 8. Whole workspace: about 3,070.

## 2. Ranked recommendations

Savings are for the macOS `Test core` step (2m18s) unless stated. Windows figures are estimates scaled from the CI note's 2.3-3.5× ratio.

| # | Change | Kind | Saves (macOS) | Saves (Windows) | Coverage lost |
|---|---|---|---|---|---|
| 1 | Shrink the four oversized unit tests | cheaper | about 12-15 s [E from M] | about 30-45 s [E] | none if limits are injected; slightly fewer random seeds |
| 2 | One shared stub, symlinked per test | cheaper | about 15-20 s [E from M] | none (Unix-only tests) | none |
| 3 | Inject the handshake timeouts; stop sleeping 6 s | cheaper | about 14 s [M] | none | none |
| 4 | Merge 48 integration binaries into about 7 | merge | about 20-25 s [E from M] | up to about 40 s [CI] | none |
| 5 | Run `ferrite-reviewr` tests only when that crate or `Cargo.lock` changes | CI | about 40-50 s [E] | 77-103 s [CI] | none on PRs that don't touch it |
| 6 | Delete the duplicate fixture replays and version-pin tests | delete | about 3-5 s | none | none: unit tests assert the same facts |
| 7 | Copy a template repo instead of five `git` calls per fixture | cheaper | about 3 s [E] | about 10-20 s [E] | none |
| 8 | Remove the fixed sleeps listed in section 4 | cheaper | about 3 s | about 3 s | none |

Rough total: **65-75 s off the 2m18s macOS core step, plus the reviewr step**; on Windows 2.5-3.5 min, most of it from 5, 1 and 4.

### 1. Shrink the four oversized unit tests

Each proves a real property but sizes itself off a production constant, in a debug build.

- `a_history_larger_than_the_lag_limit_still_arrives` (`src/server/tests.rs:827`): make the lag limit a field of the link (the store already does this with `Store::scaled`, used at `src/store/revive_tests.rs:389`). With a 1 MiB limit in the test, the same assertion runs in well under a second. `link::tests::a_client_too_far_behind_is_lagged…` (2.7 s) gets the same fix.
- `a_bounded_revive_shows_what_a_full_replay_shows`: 24 seeds at 300-700 turns. Six seeds at the already scaled 16 KiB window still cover every branch the test lists; change `bounded_reads >= 20` to a matching share.
- `trimmed_history_reads_as_a_rebuild_from_the_retained_records`: 100 seeds × 240 steps with a full rebuild at each step. Use 20 seeds in CI and keep 100 behind `#[ignore]` or an env var for local soak runs.
- `invariant_a_hand_over_after_revive_carries_the_whole_history` (`src/cockpit/tests/context.rs:96`): 5.6 MB of answers to pass the real retention cap. Build the cockpit with small `ActivityLimits` instead.

Risk: fewer seeds find fewer rare interleavings. Keep the full sweeps runnable on demand.

### 2. One shared stub, symlinked per test

About 270 tests write a new `#!/bin/sh` file with mode 0700 and exec it: all of `contract_*` through `tests/support/provider_contract.rs:22-128`, 72 `stub(…)` call sites in `claude_session.rs` and `codex_session.rs`, `provider_progress.rs:10`, and `titler`, `oneshot`, `discover` and `browser_providers` in `src/`. Each pays about 260 ms before its first line runs.

Write one dispatcher per test process (`#!/bin/sh` then `. "$0.body"`), run it once, and give each test a symlink to it plus a `<name>.body` file. Measured: the first symlink costs 268 ms, the next seven 10-49 ms. The change is confined to the `stub` helpers.

Caveat: measured on this machine only. The GitHub macOS runner should behave the same (it is the OS's first-exec check), but confirm with one run before relying on the number.

There are also nine separate copies of the `stub`/`replay` helpers across `tests/*.rs`; `provider_progress.rs:10-70` re-implements what `support/provider_contract.rs` already does. Fold them into the one support module while doing this.

### 3. Inject the handshake timeouts

`codex_session` cannot finish in under 12.6 s and `claude_session` in under 6.4 s, whatever else changes, because four tests wait on wall-clock constants.

- Put the handshake and effort-ack timeouts on `ClaudeConfig`/`CodexConfig` (defaults unchanged). The two "silent CLI" tests then use 200 ms.
- `slow_thread_start_and_resume_survive_model_discovery` and `a_late_effort_answer_still_starts_the_session` exist to prove a reply later than the handshake timeout still succeeds. With a 100 ms handshake timeout the stub needs `sleep 0.3`, not `sleep 6`. Until then, run the two loop iterations at `tests/codex_session.rs:835` on two threads: 12.6 s becomes 6.3 s for a two-line change.

### 4. Merge the integration binaries

Cargo's own advice is one integration crate with modules. Proposed layout:

| New binary | From | Tests |
|---|---|---|
| `contract` | the 18 `contract_*.rs` (all share `support/provider_contract.rs`) | 133 |
| `sessions` | `claude_session`, `codex_session`, `claude_subagents`, `codex_subagents`, `codex_subagent_interrupt`, `codex_decision_ids`, `codex_writer_conflict`, `background_handback`, `provider_progress`, `visuals`, `notifications`, `store` | about 170 |
| `folds` (pure, runs on Windows too) | `activity`, `activity_memory`, `progress_lifecycle`, `subagent_cockpit`, `import`, `edit_drafts`, `steer_fold`, `wrap_up`, `api_bridge` | about 100 |
| `live` (behind `required-features = ["live"]`) | `live_claude`, `live_codex`, `live_import`, `live_plans`, `live_progress` | 21, all `#[ignore]` |
| `crash_report` | stays alone: it installs a process-wide panic hook (`tests/crash_report.rs:1-2`) | 1 |
| `windows_session_tree` | stays alone, `cfg(windows)` | 1 |

That is 48 binaries down to 6, with 4 built in a normal CI run. Today six binaries run nothing on macOS (the five `live_*` and `windows_session_tree`) and 32 run nothing on Windows.

`steer_fold.rs`, `wrap_up.rs` and `edit_drafts.rs` are pure `Transcript` tests of 24-63 lines; each is its own linked executable. They belong in `folds`, or beside the transcript unit tests.

Risk: tests that assume a private process (environment variables, a temp directory keyed only by `std::process::id()`, such as `tests/claude_session.rs:49`) can collide once they share one. Merge one group at a time and check for name clashes in the temp paths.

### 5. Path-filter the vendored reviewr suite

`ferrite-reviewr` is upstream code, "verbatim from upstream" for 15 of its source files (`crates/ferrite-reviewr/README.md`), and its 313 tests are mostly upstream's. Four of 1,308 commits touch the crate; the last was 2026-11-04. CI runs all of it on both systems on every PR: 77-103 s on Windows [CI], 41 s locally [M].

Run that step only when `crates/ferrite-reviewr/**` or `Cargo.lock` changes (a `dorny/paths-filter` step, or a separate workflow with `paths:`), and always on main. Nothing is lost on other PRs, because `ferrite-core`'s own review tests still exercise the adapter.

If it stays on every PR, one test sets its wall time: `a_diffs_sides_are_the_committed_blob_and_the_text_git_would_store` (`tests/git_repo.rs:48`) takes 26.7 s by building 12 repositories in sequence with about 35 `git` calls each. Splitting the regimes across threads would bring the binary to about 8 s, at the cost of diverging from upstream.

### 6. Delete: duplicated assertions

These are the tests I would delete outright. Each names what already covers it.

**a. Fixture replays through a real process that restate the parser's unit tests (about 20 tests).** `tests/claude_session.rs:86-99` says "every fixture earns its keep here, not only in the parser's own tests", and so each capture is asserted twice:

| Process-level test | Unit test asserting the same facts on the same fixture |
|---|---|
| `claude_session::a_tool_call_arrives_as_a_start_and_a_completion` (`:362`) | `wire::tests::the_tool_fixture_yields_a_start_and_a_completion_that_agree` (`src/providers/claude/wire.rs:2041`) |
| `claude_session::the_reader_thread_delivers_the_captured_stream` (`:272`): one Init, model id, "hello ferrite", the thinking text, cost 0.03798 | `fixture_yields_exactly_one_init`, `fixture_text_deltas_concatenate_to_the_answer`, `fixture_carries_thinking`, `fixture_ends_with_a_completed_turn` (`wire.rs:1894-1945`) |
| `claude_session::a_failed_turn_ends_with_the_reason_the_cli_gave` (`:550`) | `wire::tests::error_results_carry_their_detail` (`wire.rs:2625`) |
| `claude_session::a_permission_request_arrives_as_a_decision…` (`:407`), `spawn_completes_the_capability_handshake` (`:566`) | the permission and capability tests at `wire.rs:2324-2549` |
| `codex_session` replays at `:341`, `:376`, `:424`, `:456`, `:606` | the matching `providers::codex::wire::tests` |

Keep one replay per provider (`the_reader_thread_delivers_the_captured_stream` and its Codex twin), trimmed to what only a process can show: events arrive in order through real pipes, and drop kills a live child. Delete the other replays. What replaces them: the unit tests in the right-hand column, which also run on Windows, where these integration tests are compiled out.

Within `wire.rs` itself, the five `fixture_*` tests at `:1894-1970` each re-parse the same file to check one field; fold them into one. `fixture_yields_nothing_else` asserts `FIXTURE.lines().count() == 32` and `events.len() == 12`: that is a snapshot of the capture file, not a behaviour.

**b. The version pin, tested about 22 times.** Per provider: six unit tests on `parse_version` and the pin constants (`src/providers/claude.rs:1363-1412`, `src/providers/codex.rs:1610-1660`) and five or six integration tests that spawn a stub to print a version (`tests/claude_session.rs:164-262`, `tests/codex_session.rs:173-260`). `a_cli_exactly_at_the_pin_is_accepted`, `a_cli_at_the_next_major_still_spawns` and `the_last_release_below_the_next_major_is_accepted` each spawn a process to re-prove a comparison the unit tests `the_pinned_boundary_is_met_and_one_below_is_not` and `the_next_major_is_untested…` already prove. Keep per provider: one unit test for parsing, one for the boundary, and three integration tests (missing CLI, below the pin, unreadable banner). Delete about 12.

**c. Tests that copy a constant table (about 15 in core).** They cost no CI time; they cost an edit on every product change and can only fail when someone changes the table on purpose.

- `settings::tests::the_defaults_are_the_documented_ones` (`src/settings.rs:595`): 21 assertions restating `Default`.
- `settings::tests::each_provider_has_an_auto_mode_among_its_modes` (`:868`): asserts two string literals.
- `providers::models::tests::the_fallback_rows_read_as_the_prototype_picker` (`src/providers/models.rs:608`) and `the_fallback_names_are_versioned_and_labels_agree_with_them` (`:568`): the bundled model list and its marketing copy, written out again. #207 changed 287 lines in that one file to add a model. Keep the 66-cell width check at `:635-641`, which is a real constraint; delete the copied tables. `the_fallback_carries_each_models_effort_ladder` (`:700`) is the same pattern.
- The claude/codex `the_displayed_window_matches_the_pins` pair: a compile-time relation between two constants.

### 7. Git fixtures

`init_repo` runs five `git` processes (`src/workspace/mod.rs:1174`, copied at `src/cockpit/tests.rs:362` and `src/workspace/registry.rs:732`) and has 45 call sites. All 31 `workspace::tests` cost 1.0-1.7 s each under load. Build the base repository once per process in a `OnceLock` and copy the directory per test (a few small files). The worktree tests themselves need real git and should keep it. The gain is small on macOS and larger on Windows, where process creation is the suspected reason the lib run is 2.3-3.5× slower.

Also worth one experiment for the Windows gap: every store-backed test (about 300 across `store`, `engine`, `server`) performs real barrier and full syncs (`src/store.rs:1697-1725`); #208 already had to patch a test because `FlushFileBuffers` outlasted a 500 ms deadline. The store counts its syncs for tests (`Shared::syncs`), so a test-only switch that counts without syncing, left off in `durability_tests`, would show quickly whether syncs are the Windows cost. I did not measure this.

### 8. Fixed sleeps and wall-clock assertions

- `src/user_commands.rs:380`: `sleep(1500 ms)` to prove a background job died. Poll for the process group instead, or shorten the job's `sleep 1` to 0.2 s and the wait to 400 ms.
- `tests/claude_session.rs:1146`, `:1155`, `:1304`, `:1480` and `tests/codex_session.rs:1488`: 100-300 ms sleeps "to let it settle". Each is a race that passes by waiting; replace with a wait on the condition.
- Three tests wait on the 2 s `SWEEP_INTERVAL` (`src/server.rs:42`); make it a harness parameter.
- 22 assertions compare elapsed wall time with a budget, six of them in `src/transcript.rs:4780-4915` at 1,500 ms in a debug build. `an_output_tail_takes_a_long_line_in_its_stride` already takes 1.0 s under the parallel run. These guard real quadratic-time regressions and should stay, but they will fail on a loaded 3-core runner before they fail on a regression. Count work (bytes re-read, nodes visited) instead of time, as the store tests do with `read_bytes`.

## 3. Keep

These are the tests that earn their place, with the reason.

- **Wire fixtures and protocol contracts**: `protocol::tests` (17; `every_gen1_message_has_a_frozen_fixture_that_round_trips_byte_for_byte`, the unknown-field and unknown-variant tolerance tests), `providers::{claude,codex}::wire::tests` (74), and the `contract_*` suite (133). They pin what an older or newer server or CLI may send. The contract tests should stay as behaviours but about 50 of them never talk back to the stub (`contract_fidelity`, `contract_state`, `contract_tools`, `contract_usage`, `contract_diffs` make no `session` calls after spawn): they are decoder tests run through `sh` and `cat`. A test-support entry point that feeds frames to the decoder directly would make them pure, remove 50 process spawns, and let them run on Windows, which has no contract coverage today.
- **Persistence and crash safety**: `store::durability_tests` (32), `store::tests` (30; `a_crash_torn_tail_never_loses_the_thread`), `compat_tests`, `revive_tests`, `rewind_tests`. This is the code where a bug loses a user's Thread. Keep all of it; only the seed counts in item 1 change.
- **Process lifecycle**: the session tests that only a real child can show: `a_slow_consumer_stalls_the_cli_instead_of_losing_events`, `stdout_eof_closes_the_session_with_the_exit_status`, `only_the_tail_of_a_noisy_stderr_is_kept`, `a_line_of_invalid_utf8_does_not_end_the_session`, `what_a_session_starts_runs_below_the_server`, `windows_session_tree`, `user_commands::a_stop_ends_the_command_and_what_it_started`.
- **Worktree safety**: `workspace::tests` (`removing_a_dirty_worktree_is_refused_and_loses_nothing`, `two_worktrees_of_one_repo_cannot_touch_each_other`). They need real git and they protect user work.
- **Server and link** (`server::tests`, `link::tests`): resume, lag, restart, park. Mostly fast (56 of 102 under 50 ms) and each names a distinct failure.
- **Named regressions**: the linear-time streaming tests in `transcript.rs`, `activity_memory`, `self_update::tests` (checksum, failed swap), `settings::tests` for corrupt, partial and symlinked files, `providers::limits::tests`.
- **The 860 fast pure unit tests** in general. I read the names of all of them and the bodies of about 60; apart from the items in 6c they describe distinct behaviours. They are cheap. Leave them.

## 4. The other crates

None of these run in CI. Built and run once here [M]:

| Crate | Tests | Run time | Result |
|---|---|---|---|
| `ferrite-cockpit` | 846 | **324 s** (1,064 s summed) | **2 failed**, 3 ignored |
| `ferrite-reviewr` | 311 | 41 s | pass (runs in CI) |
| `ferrite-pane` | 246 | 2.9 s | pass |
| `ferrite-ui` | 119 | 1.9 s | **1 failed** |
| `ferrite-focus-notify` | 54 | 2.7 s | pass |
| `ferrite-terminal` | 11 | 5.1 s | pass |
| `ferrite` | 11 | 3.7 s | pass |
| `ferrite-frame` | 8 | 0.7 s | pass |

Building these test binaries took 8m54s with warm dependencies.

- **`ferrite-ui`: `icons::tests::every_icon_key_loads_an_svg` fails** (`src/icons.rs:464`): it asserts `ICONS.len() == 30 + …` and the list now has 71 entries against an expected 70. This is the constant-count pattern from 6c, and it went stale because nothing runs it. Delete the count assertion; the loop above it, which loads every icon, is the real test.
- **`ferrite-cockpit`: 2 failures.** `render_performance::a_kit_toast_slides_its_own_height_on_the_toast_tokens` (`render_performance.rs:2512`) asserts animation positions against elapsed time and failed under the parallel run. `machines::browser_annotation_host_intercepts_selection…` (`machines.rs:1550`) failed with "path must be shorter than SUN_LEN": its socket path under the temp directory is too long on this machine. I ran each once; the first may be load-dependent.
- **One cockpit test takes 275 s**: `retention::a_transcript_at_its_retention_cap_keeps_its_text_identity_as_it_streams` (`retention.rs:89`) streams 66 × 64 KiB turns to pass the real 4 MiB display cap. It is the same sizing problem as item 1 and has the same fix. Twenty more tests take over 5 s each (`transcript_pointer::a_long_reply_grows_the_card_upward_to_a_few_rows` 32.8 s, `frame_parity::every_board_draws_transcripts_or_wall_tiles` 29.7 s, `completion_checks::multiline_composer_keeps_its_lower_edge…` 23.3 s).
- **Low-value patterns, from sampling**: `ferrite-ui/src/theme.rs` and `motion.rs` test design tokens against themselves (`the_accent_is_retired`, `the_ink_ladder_is_two_tiers`, `the_catalog_keeps_claudes_numbers`, `every_state_word_is_lowercase_and_dashless`, `only_needs_you_and_failure_words_take_a_colour`, which repeats the `match` it tests). The cockpit `*_parity` files assert layout constants through rendered bounds (`frame_parity.rs:36-60`: the 38 px head, the 14 px inset); about 145 assertion blocks in `cockpit/tests/` contain a literal pixel value. `no_source_file_names_the_retired_attention_wash` (`theme.rs`) greps the source tree for a deleted identifier. Keep the contrast and glyph-coverage tests (`ink_clears_its_floor_on_every_ground`, `chrome_glyphs_are_in_the_face…`) and the render-count tests in `render_performance.rs`, which guard real performance behaviour.
- **Decision needed**: 36,000 lines of cockpit tests that CI never runs will keep drifting. Either run a subset in CI (`ferrite-pane` and `ferrite-ui` cost under 5 s of run time, though they need the GPUI stack compiled for tests) or accept that they are local-only and stop counting them as protection. Running all of cockpit is not affordable until the 275 s test and its neighbours are fixed.

## 5. What I did not do

- No Windows measurements. Every Windows saving above is scaled from the CI note's logs.
- The macOS first-exec cost was measured on this machine, not on a GitHub runner.
- I read all test names in `ferrite-core` and the bodies of roughly 150 tests across the workspace. The 8-10% low-value figure for core comes from the duplicates I could pair up (section 6) plus the constant-table tests; the 15-20% for the UI crates is from sampling four files.
- The store-sync hypothesis for the Windows gap is untested.
