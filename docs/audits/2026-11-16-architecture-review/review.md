# Ferrite architecture review

The goal is to make feature changes local and reduce unrelated regressions. This pass reviews the architecture and proposes a staged redesign. It does not implement the migration.

The review examines the working tree based on `f7884cffe3a1cc26434f2778598f371f81fad294` on 2026-11-16. Extensive uncommitted application and vendor changes already existed. Source references describe that working tree, not a clean release. No application files were changed and no tests were run.

## Assessment

Ferrite has useful architectural foundations. Core is independent of GPUI. The server owns running sessions. Protocol fixtures preserve compatibility. Transcript rendering accepts structured inputs and emits events. Frame slots separate app chrome from the Cockpit. Browser operations carry ownership and document identities.

The main weakness is that ownership boundaries inside those layers remain broad. Feature modules often extend the same mutable owner. Callers must coordinate state mutation, projection, invalidation, persistence, and asynchronous work. Tests sometimes take a shorter route than production.

These are observed mechanisms. Their contribution to the user's historical regressions is an inference. This review does not measure defect frequency, build latency, or runtime responsiveness.

## Current structure

The production crate graph is acyclic. `ferrite` composes the application. `ferrite-cockpit` depends on pane, UI, core, terminal, reviewr, and notifications. Pane depends on UI, core, and reviewr. Frame depends on UI and core. UI depends on core. Core depends on reviewr.

The runtime path is window action, core Cockpit client, server command, engine mutation, published state, window mirror, presentation projection, then GPUI rendering. The in-process fixture path can instead call engine methods directly.

| Responsibility | Present owner | Intended direction |
|---|---|---|
| Session execution and durable thread state | Engine, Store, Server | Preserve server authority and the store worker. |
| Wire compatibility and transport | Protocol, Link, Remote | Keep wire parsing at transport edges. |
| Window mirror and command submission | Core Cockpit | Give local and remote adapters the same command semantics. |
| Pane, navigation, panel, and browser coordination | CockpitView | Move each coherent state model behind a small interface. |
| Retained transcript rendering | TranscriptView | Preserve its input and event contract. |
| Native browser execution | Browser service, window controller, CEF worker | Preserve fences while concentrating lifecycle ownership. |
| App chrome | Frame interface | Preserve the existing separation. |

## Findings

### 1. Test and production commands have different entry paths

`Cockpit::act` accepts separate local and remote closures in `crates/ferrite-core/src/cockpit.rs:582`. `send` calls `Engine::send` locally but submits `Op::Send` remotely at line 1796. `Engine::execute` adds hold policy and command checks in `crates/ferrite-core/src/engine/commands.rs:32`.

The mechanism is proven by source. It does not mean every local test is wrong. Server and protocol tests already exist. In-process fixtures normally retain Activity because only the Server sets watched membership. Consequently the hold-policy difference alone is not evidence of wrong fixture behavior. Unknown-thread checks and result conversion remain concrete differences worth characterizing.

First characterize those differences through one behavior table. Align shared policy where the outcomes should agree. A common command entry point is a candidate if it removes repeated decisions without changing deliberate fixture semantics. Keep transport errors such as an unconfirmed reply separate from domain refusal. Preserve direct engine methods for internal operations where appropriate. Do not blindly route server-only operations through the engine.

### 2. Synchronous reply waits cross into UI work

`Remote::execute` waits on `recv_timeout(REPLY_WAIT)` inside a loop in `crates/ferrite-core/src/cockpit/remote.rs:335`. Every unrelated message enters the inbox and begins another full wait. This is an inactivity timeout, not an overall command deadline.

The browser path makes this coupling concrete. The Cockpit pump handles browser events, calls `browser_page`, and waits for its reply. The server acknowledges some page changes after disk work. See `crates/ferrite-cockpit/src/cockpit/browser.rs:703`, `crates/ferrite-core/src/cockpit.rs:287`, `crates/ferrite-core/src/server/browser.rs:245`, and `server/browser/disk.rs:26`.

Offloading disk work from the server does not remove the GUI caller's wait. This is a demonstrated blocking dependency. Its frequency and visible latency were not measured.

Recommended change is asynchronous command submission and completion with explicit request identity, pending state, cancellation, and unconfirmed outcomes. An absolute deadline is a bounded corrective step, but does not eliminate blocking. Preserve durable browser acknowledgements before reclaiming resources. Do not turn uncertain commands into automatic retries.

### 3. Cached UI parts still share one mutable owner

`CockpitView` owns the core client, pane list, browsers, tools, navigation, overlays, and preferences in `crates/ferrite-cockpit/src/cockpit.rs:352`. `PartView::render` calls back into that owner through a weak entity in `cockpit/parts.rs:48`. Cached parts track reads from that same owner.

`CockpitPane` exposes `PaneView` through `DerefMut` at `cockpit.rs:346`. Public pane fields expose additional mutable presentation state at `crates/ferrite-pane/src/pane.rs:108`.

Moving more methods into separate files would retain these dependencies. The proposed direction is state-owning modules with explicit input and event contracts. Keep cached parts during migration. Move one cohesive responsibility at a time and remove direct mutation after its callers migrate.

### 4. Invalidation repeats knowledge about presentation inputs

The Cockpit constructs `TranscriptKey` in `cockpit.rs:2068`. Side chat constructs it in `cockpit/side_chat.rs:532`. `TranscriptInput::key` constructs it again in `crates/ferrite-pane/src/transcript.rs:271`. These reduce the skill catalogue to its length. Shared chrome invalidation separately compares a formatted selection of values at `cockpit.rs:2836`.

Duplicated key construction is observed. A current stale-catalogue defect is not established because another revision may change on that path.

Recommended change is one projection constructor owning both input and its revision identity, plus a typed shared-chrome snapshot. Keys must cover observable dependencies without discarding the existing incremental rendering behavior. Existing stale-chrome and render-isolation tests are useful gates.

### 5. Rendering reconciles application state and saves it

Cockpit rendering reconciles Panel state and changes transcript watch membership in `cockpit.rs:10094` and `:10144`. `tend_panel` starts resource work and saves state in `cockpit/panel.rs:419`. `Surfaces::save` performs filesystem operations in `cockpit/surfaces.rs:401`.

The panel save path clears its dirty flag before attempting the write at `cockpit/panel.rs:369`. A failure therefore does not remain pending on that flag alone.

Recommended change is a Panel state model whose transitions produce explicit resource and persistence effects. Drawing reads its presentation state. Layout-dependent measurements can remain in GPUI. Failed saves remain pending or visibly failed according to a defined policy. Preserve tab identity, operator revision precedence, and restart restoration.

### 6. Browser authority is valuable but distributed

Server ownership checks, document generations, leases, native revalidation, pause epochs, and resource reclamation protect real concurrency boundaries. Preserve them. The window browser owner also carries address editing, dialog focus, control queues, captures, and Panel reveal state in `cockpit/browser.rs:40`.

Recommended change is a window browser controller that owns request lifecycle and native resources. Presentation owns address editing and dialog display. Panel reveals carry explicit operator revision. Keep process-boundary validation at each trust boundary. Encapsulation must not remove deliberate revalidation between asynchronous phases.

### 7. Server work shares one progress loop

`Server::serve` drains incoming messages before `Engine::pump` in `crates/ferrite-core/src/server.rs:500`. The drain has no per-pass batch bound. Some commands synchronously perform slow work. Worktree deletion reaches git subprocess output through `engine.rs:2290` and `workspace/mod.rs:1008`.

The structural risk is that work for one Thread delays event consumption and replies for another. Starvation and latency were not reproduced. Bound intake per pass, then move slow preparation behind pending operations whose completions the Engine applies. Preserve serialized state mutation. A deterministic blocked operation must leave another Thread's events and interruption able to progress.

The Engine's own `Thread` state is also a future ownership candidate at `engine.rs:590`. Extract session-transition rules as one coherent responsibility. Preserve `SessionLifecycle`, `SpawnRequest`, journal publication, and persistence backpressure. Avoid a single enum combining every independent Thread concern.

### 8. Browser request lifetime stops at transport boundaries

Server requests receive a deadline in `server/browser.rs:518`. Expiration removes a pending request at line 541. The `BrowserRequest` wire message and window `ControlRequest` do not carry that deadline or cancellation information. The native worker starts its own deadline after accepting work in `crates/ferrite-pane/src/visual/browser_control/native.rs:83`.

The inferred risk is that a queued write expires at the server and later begins native execution. Existing uncertainty handling limits unsafe follow-up, but this sequence was not reproduced.

Model queued, dispatched, completed, expired-before-dispatch, and uncertain-after-dispatch outcomes explicitly. Carry a bounded budget or an explicit cancellation handshake across the process boundary. Do not assume clocks on separate machines agree. Test a worker blocked behind another Thread, expiration before dispatch, and timeout after dispatch. Preserve uncertainty for work that may already have applied.

### 9. Verification selection leaves relevant changes unchecked

`.github/workflows/ci.yml:97` runs core, reviewr, cockpit, pane, and UI suites. It builds the app but does not invoke the existing app, frame, or terminal unit suites. Those suites include launch-directory behavior, settings interaction, and PTY resizing. This is missing CI execution, not missing tests.

The packaged-browser workflow uses a path list at `.github/workflows/browser-window.yml:16`. It omits relevant inputs including `Cargo.lock`, pane and cockpit manifests, `crates/ferrite-core/src/server/browser.rs`, and `vendor/gpui-base/**`. A PR changing only those paths does not trigger that workflow.

Recommended change is a tested verification selector. Use reverse package dependencies where available and explicit exceptional inputs for packaging and vendored code. Every workspace member receives a verification decision. A required summary must distinguish an intentional skip from a missing job. Repository branch-protection settings were not inspected.

## Alternatives

Two additional local improvements should precede broader ownership extraction.

- `cockpit/transcript_glue.rs:39` computes rewind availability separately from `engine/rewind.rs:365`. Core additionally checks outstanding rewind, replacement, native pending queue, and operator-turn state. Publish typed operation availability from core while retaining execution-time validation. Distinguish rewind, fork-before-prompt, and whole-conversation fork. The observed problem is duplicated policy with different conditions, not an unsafe rewind.
- `cockpit/transcript_glue.rs:208` captures a pane index in a deferred preview callback. It later calls `focus_pane(index)`. Carry `PaneIdentity` across deferred work and resolve its current position at execution. A disappearing or replaced pane needs an explicit policy. The missing identity revalidation is observed. A user-visible race was not reproduced.

| Approach | Benefit | Cost and judgment |
|---|---|---|
| More helper methods and tests around current owners | Small initial diff | Leaves coordination knowledge in callers. Useful for immediate containment. |
| State-owning modules within current crates | Local transitions, smaller interfaces, existing renderer retained | Requires careful caller migration. Recommended. |
| Broad crate split or actor/event-bus rewrite | Strong separation if designed correctly | Adds transport, ordering, and migration work before ownership is settled. Not justified by current evidence. |

A small shared-value crate may eventually remove UI's dependency on the entire runtime. First inventory actual consumers of cadence, provider identity, settings values, and model descriptors. Avoid creating a miscellaneous types crate.

## Proposed migration sequence

Each row is a stage, potentially several small PRs. Complete one verified unit before starting its dependent unit. This is a proposed sequence, not an implementation commitment or effort estimate.

| Stage | Concrete result | Completion evidence |
|---|---|---|
| 1. Verification selection | Relevant existing suites and CEF checks run for their inputs | Selector fixtures cover every workspace member, dependency changes, vendor changes, lockfile changes, and intentional docs-only skips. |
| 2. Bounded progress | Absolute reply deadlines, bounded server intake, then slow-operation isolation | Continuous unrelated traffic cannot extend the original deadline or prevent another Thread's progress. Each change is a separate verified unit. |
| 3. Targeted command completion | A reproduced blocking command family completes asynchronously, with request lifetime preserved | Delayed replies leave unrelated input responsive. Expired queued writes never start. Disconnect, stale completion, and uncertain outcome are explicit. Characterize local/socket policy parity before changing dispatch. |
| 4. Presentation identity | One owner constructs transcript input and its cache identity | Settings, focus, stream, and equal-length catalogue changes update correctly. Unrelated panes retain rendering isolation. |
| 5. Panel ownership | State transitions own activation, release, and persistence | Close, reopen, operator priority, restart restore, and failed-save behavior pass. Render does not write panel files. |
| 6. Browser lifecycle | Controller owns pending operations, request lifetimes, and native resources | Expired queued writes do not start. Dispatched uncertain writes are not replayed. Navigation, close, owner replacement, disconnect, and operator input reject late results. Native packaged-browser checks pass. |
| 7. Pane ownership | One pane state slice no longer permits external field mutation | Keyboard focus, selection, subject switching, and render isolation remain correct. Migrated direct writes and obsolete accessors are removed. |
| 8. Dependency constraints | Existing architectural rules become executable | Core cannot acquire GPUI. Frame cannot depend on Cockpit. Any new shared-value crate excludes runtime and transport dependencies. |

The existing dependency constraints can be enforced in parallel with stage 1. A crate extraction itself follows ownership work. Browser request lifetime and targeted async completion belong to stage 3 before broader extraction in stage 6. Reproduce the blocking path before committing to an async migration. Do not migrate all commands merely because they share a helper. Shared operation availability and identity-based deferred actions are independent small units that can precede stages 4 and 5. Engine session-transition ownership follows command parity and progress checks, with one transition family per change.

Use `cargo nextest run -p <crate>` with the narrowest relevant filter after the last edit, then one applicable typecheck. Run E2E once before handing back a changed user flow. The full suite belongs in CI. No such verification was needed for this review-only artifact.

## Review method and limits

Five read-only specialists examined core, UI, browser, verification, and comment-documented invariants. A sixth synthesized the architecture. The parent spot-checked primary source. An independent Opus source review confirmed the major mechanisms and challenged the proposed scope.

Opus corrected the local hold-policy interpretation, identified the third cache-key construction site, and recommended a deadline fix before broad command migration. Those changes are incorporated above. It also raised a possible Panel pruning risk during partial attachment. This remains unproven and requires tracing snapshot completeness before altering pruning. The save is dirty-gated, not performed on every render.

The Opus reviewer started before these report files existed. It reviewed the source-backed findings, not the final report or decision trail. That artifact-review gap remains explicit. Its additional CI observations and callback-registration concern were not independently verified here and are not promoted to findings.

The throughput checkpoint was to establish the dirty baseline first, partition read-only investigation by responsibility, keep shared application files untouched, and serialize report synthesis. The smallest useful review partition was a coherent subsystem or verification contract.

The principles that shaped the recommendation were Minimize Reader Load, Foundational Thinking, Model the Domain, Boundary Discipline, Type System Discipline, Subtract Before You Add, Laziness Protocol, Sequence Work into Verifiable Units, Prove It Works, and Encode Lessons in Structure. Their effect was to prioritize explicit ownership and enforceable behavior over file moves or additional layers.

Unresolved work includes runtime reproduction, multi-window browser handoff, exhaustive pane mutation inventory, branch protection inspection, and representative historical regression analysis. This report establishes structural risks. It does not certify the current application or the proposed implementation.

The decision trail is [decisions.tsv](decisions.tsv).
