# Coverage and quality audit

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a full Ferrite product audit against current T3 Code, including broken interactions beyond the named PR, subagent, browser and panel complaints. This report identifies unassigned capability differences, checks earlier promises against current source, and examines why a correct-looking view can remain unreachable.

## Overview

The most consequential coverage problem is that the ordinary CI workflow tests `ferrite-core`, conditionally tests `ferrite-reviewr`, and builds the app. It does not run the `ferrite-cockpit` interaction tests or the `ferrite-pane` tests. The same workflow explicitly reserves Chromium builds for release. Source evidence is `.github/workflows/ci.yml:3-7,87-93,124-133`.

The existing UI tests have useful real-input coverage. They also contain contracts that skip the path the operator uses. One test called `an_empty_panel_says_open_a_surface_and_takes_its_keys` calls the key handler directly. The PR opener test invokes the plus-menu action directly. The required-form editability test rejects an invalid value but never proves that a valid typed value reaches the provider. These are concrete coverage weaknesses, not evidence that every test is ineffective. Sources are `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220`, `tests/pr.rs:226-247` and `tests/provider_forms.rs:306-359` under that same tests directory.

Ferrite already has remote machines and scheduled automation. Remote work uses SSH, and schedules belong to Factory workflows. Calling either whole capability absent would be wrong. The narrower differences are guest participation, device panels, one-time schedules, sub-hour schedules, and thread-targeted scheduled prompts. Sources are `crates/ferrite-core/src/machine.rs:1-7,22-33`, `crates/ferrite-core/src/factory/starts.rs:27-153`, and the T3 references below.

## Method and limits

- Ferrite baseline is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- T3 baseline is `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, inspected at `/tmp/ferrite-t3-audit-source-20261113`.
- This worker performed source inspection only. No application code, settings, dependencies, branches, existing user data or live UI were changed by this worker. No tests were run. CQ-09 includes a separately identified parent-observed CLI reproduction recorded in `evidence/runtime-observations.md:30-45`. No other finding claims runtime reproduction.
- Searches covered Ferrite application crates, the full API operation catalog, panel identity types, palette commands, settings page types, CI, current UI tests, recent ADRs, both September audit findings files, and T3 contracts, routes, relevant components and desktop startup.
- A T3 contract export proves that a capability exists in its code model. A component and action trace provide stronger implementation evidence. Neither proves availability in every installed version, account, operating system or environment.
- Absence claims are bounded. The finite Ferrite panel-kind and recurrence enums establish missing variants. Guest-sharing absence is an inference from searches and public entry points, so it is labelled inferred.

## Key concepts

A capability is the work a user can complete. An affordance is the thing they click or focus to start it. A handler test begins after that affordance has already worked. A rendering assertion proves that a node or rectangle exists, but does not prove input reaches it. A transport fixture proves a local contract, but does not prove an authenticated GitHub or Chromium operation succeeds. These distinctions explain the recommendations below.

For this audit, parity does not mean copying every T3 product choice. Ferrite explicitly chose T3's transcript organization, tabbed panel and flat Inbox in ADRs `0038-t3-code-as-the-transcript-reference.md:8-16`, `0039-the-panel-is-one-tabbed-surface-host.md:16-34`, and `0040-the-nav-is-the-inbox.md:15-31`. Guest sharing and device hosting do not acquire that same priority merely because T3 has them.

## Current T3 capability inventory

The following is a coverage map, not a claim of exhaustive execution. Rows owned by another partition identify the inspection boundary so the consolidated audit can check coverage without duplicating its detailed bugs. All T3 links pin the inspected commit.

| Capability family | Current source evidence | Ferrite coverage and owner |
|---|---|---|
| Unified right-panel tabs, per-thread active tab, maximization and persistence | [Panel union and state](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L15-L125). | Present in `cockpit/surfaces.rs:25-95`. Detailed differences belong to panel-tools. |
| Diff, files, individual file, browser, terminal and side chat panels | [Rendered panel dispatch](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L86-L215). | Corresponding variants exist in `cockpit/surfaces.rs:28-45`. Browser and panel-tools own quality. |
| Terminal splits inside one panel tab | [Terminal identity and split direction](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L41-L48). | Ferrite's panel identity is one `Terminal(u64)` at `cockpit/surfaces.rs:38-39`. Split behavior belongs to panel-tools. |
| Global and per-thread PR lists, PR detail, summary, guide, timeline and code | [List/detail kinds and tabs](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L15-L81), [rendered lists and details](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L120-L175). | PR navigation and operations own reachability and behavior. The Ferrite detail-tab enum is at `cockpit/surfaces.rs:56-76`. |
| PR link destinations retained until rendered | [revealUrl dispatch and settlement](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L145-L159). | PR navigation owns route and anchor behavior. |
| Operator choice protected against proactive panel changes | [revision-checked proactive action contract](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L113-L125). | Ferrite records an operator revision in `cockpit/surfaces.rs:8-11,92-95`. Subagents-background and panel-tools audit its callers. |
| Device panels for iOS and Android | [device target](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L30-L40), [rendered device panel](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L187-L201). | No corresponding Ferrite panel variant at `cockpit/surfaces.rs:28-46`. CQ-08 below. |
| External device sources and host connection state | [SSH, EAS and Simlock source model and editors](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/DeviceSourcesSettings.tsx#L6-L77). | An extension opportunity, not proof Ferrite's existing browser is defective. CQ-08 bounds the finding to device inspection. |
| Guest thread viewing and proposed guest messages | [guest contract](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/guestShare.ts#L13-L65), [create, error, copy and stop controls](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/GuestShareControls.tsx#L73-L202). | Inferred missing first-class capability. CQ-07. T3 requires Labs enablement and a T3 Connect tunnel, `guestShare.ts:67-70`. |
| Recurring, fixed-time, one-time and webhook scheduled tasks | [schedule union](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L25-L74), [webhooks and upsert union](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L158-L215). | Ferrite Factory already supports schedules, GitHub issue/PR triggers and external scripts, `factory/starts.rs:27-153`. CQ-06 describes only narrower differences. |
| Scheduled task enable, run now, delete, destination selection and errors | [operation controls](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L738-L770), [destination](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L1250-L1283). | Ferrite has workflow automation pause/resume and external task submission, `api/schema.rs:204-213`. The workflow model is a deliberate product shape. |
| Remote connections, local network access, WSL and Tailscale setup | [network/WSL prompts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ConnectionsSettings.tsx#L1322-L1392), [Tailscale setup](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ConnectionsSettings.tsx#L1506-L1565). | Ferrite has SSH machines, `machine.rs:1-7,22-33`. Do not label remote work missing. Non-SSH onboarding is a product difference, not a required PR repair. |
| Linked environments and load balancing | [rendered controls](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ConnectionsSettings.tsx#L1649-L1656). | No parity verdict established here. Providers-settings owns connection lifecycle; cross-machine load balancing needs a separate product decision. |
| Provider identity, instances, setup, runtime, policy, model selection and usage limits | [current exported contracts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L17-L27). | Providers-settings owns detailed provider support. Contract inventory is not a claim all providers support all features. |
| 1Password secret-provider configuration and provider-variable sources | [settings access, add/edit and empty state](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SecretsSettings.tsx#L31-L119). | Provider-settings owns credentials. Ferrite SSH intentionally delegates credentials to SSH, `machine.rs:1-7`; that does not establish secret-provider parity. |
| Local/cloud speech configuration, models and microphone setup | [voice module operations](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/VoiceSettingsPanel.tsx#L1-L73), [model browser](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/VoiceSettingsPanel.tsx#L812-L835). | Composer owns voice-input parity. This is more than accepting an audio attachment. |
| Desktop Snap Shot capture setup and customizable shortcut | [desktop bridge and setup dependencies](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SnapShotSettings.tsx#L14-L62), [candidate validation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SnapShotSettings.tsx#L126-L160). | Unverified Ferrite equivalent. Browser screenshots and attachment import do not alone prove a system capture shortcut. Retain in product backlog inventory. |
| Speech-independent screen-reader activity announcements | [status/detail live-region handling](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/ThreadActivityAnnouncer.tsx#L40-L69). | Input-focus owns accessible interaction. OS accessibility-tree verification remains an explicit gap below. |
| Thread search, tags, metadata, source-control and project clone contracts | [exported search/navigation and repository contracts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L32-L55). | Projects-navigation owns implementation differences. Export presence only establishes current inventory. |
| Background work, agent sessions and orchestrator integration | [background contract](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L5), [orchestrator and agent sessions](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L45-L56). | Subagents-background owns semantics and routing. |
| Citations, composer context, attachments and checkpoint diffs | [current exported contracts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L2-L4), [attachment/diff contracts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L25-L26). | Transcript, composer and panel-tools own complete interaction traces. |
| Browser imports, profiles, automation and captures | [current exported contracts](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L59-L65). | Browser owns engine/session/persistence behavior. |
| Worktree setup, cleanup, storage and maintenance | [cleanup UI entry points](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/StorageSettings.tsx#L548), [storage cleanup](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/StorageSettings.tsx#L800), [worktree setup contract](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/index.ts#L68-L71). | Projects-navigation owns worktrees. Cleanup-on-merge is a user instruction and should not be inferred from a cleanup settings page. |
| Window geometry and maximized-state restoration | [restore](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L452-L470), [persist](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L535-L565), [maximize](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L983). | Ferrite launch resets geometry, `crates/ferrite/src/lib.rs:349-357`. CQ-05. |
| Real-stack keyboard scenario tests | [Tab reaches the enabled submit control](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/scripts/scenarios/add-project-tab-reaches-submit.e2e.ts#L5-L21), [isolated scenario homes](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/scripts/e2e.config.ts#L1-L2). | Ferrite has production-tree GPUI tests and headless server driving. Its CI and selected tests leave the gaps below. |

Ferrite paths abbreviated as `cockpit/...` in this table are under `crates/ferrite-cockpit/src/`. Core paths abbreviated as `machine.rs`, `factory/...` and `api/...` are under `crates/ferrite-core/src/`.

## Traced flows and detailed findings

### coverage-quality-01. UI interaction tests do not gate ordinary PR CI

Priority P1. Category verification-gap. Evidence source-proven.

The trigger is a PR that changes UI code. Expected protection is that the affected behavioral tests run before the app is considered green. Actual CI executes core tests, conditional review-core tests and an application build. Windows repeats that shape on main or a full-ci PR. Building a dependency does not execute that dependency's test targets. Sources are `.github/workflows/ci.yml:73-93,95-133` and the separate UI test target at `crates/ferrite-cockpit/Cargo.toml:56-65`.

This permits regressions in click dispatch, keyboard ownership and tab activation to reach a green build even when relevant GPUI tests exist. It does not prove any historical release passed or failed them elsewhere.

Recommendation. Add the narrow affected `ferrite-cockpit`, `ferrite-pane`, `ferrite-frame` or `ferrite-ui` test targets to code-change CI. Keep the developer's local narrow-test policy. Do not replace targeted local verification with another full-suite run.

Acceptance. A deliberate failure in a PR-relevant cockpit interaction test fails the required PR check. Restoring it makes the same check green. Record the exact included test filters so a skipped target cannot masquerade as coverage.

### coverage-quality-02. Some tests bypass the exact entry path that must work

Priority P2. Category verification-gap. Evidence source-proven.

The empty-panel test verifies geometry and then calls `surface_empty_key` directly for arrows, Enter and a bare letter. It therefore bypasses platform key dispatch, binding precedence and focus. Sources are `crates/ferrite-cockpit/src/cockpit/tests/tools.rs:190-220` and the production raw-key listener at `crates/ferrite-cockpit/src/cockpit/panel/strip.rs:687-691`.

The PR test clicks Overview but calls `run_opener` directly for the plus-menu route. Its setup preloads the PR at a 1900 by 900 viewport. Sources are `crates/ferrite-cockpit/src/cockpit/tests/pr.rs:201-247`. The menu/palette test checks labels and refresh execution, rather than activation of the Open PR entry, at `tests/pr.rs:92-134`.

Expected behavior is that an operator's visible entry point reaches the intended result with the normal keymap and focus tree. Actual tests prove the helper and selected route work after bypassing some of those conditions. The input-focus and PR-navigation reports own the resulting application defects. This report owns the shared test weakness.

Recommendation. Preserve useful helper tests, then add narrow regression tests that click the plus button, navigate its menu and activate the chosen row through real key or pointer dispatch. Exercise focused and nonfocused thread PR chips. Start with a populated composer so an accidental Submit becomes observable.

Acceptance. Enter on the focused empty-panel row opens that panel and sends no prompt. Clicking each visible PR entry point produces a visible intended PR view. The test must fail when an ancestor consumes Enter or a child click is intercepted, even if `surface_empty_key` and `run_opener` still work when called directly.

T3 has a useful behavioral example. Its [add-project keyboard scenario](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/scripts/scenarios/add-project-tab-reaches-submit.e2e.ts#L12-L20) finds real roles, fills the field, presses Tab and asserts focus. That style tests the route to the action.

### coverage-quality-03. A required-form editability test can pass without successful editing

Priority P2. Category verification-gap. Evidence source-proven.

The test named `contract_mcp_required_input_without_default_is_editable_and_validated` clicks the Count field, types invalid `9`, asserts that no reply was sent, checks for a validation message and cancels. It never enters valid `2` and asserts the outgoing answer. Source is `crates/ferrite-cockpit/src/cockpit/tests/provider_forms.rs:306-359`. A separate defaults test submits prepopulated values at `tests/provider_forms.rs:229-276`, which is a different path.

The September audit already demonstrated why this is weak. Its UX-03 test reproduced input landing in the composer while the negative form test still passed. Historical evidence is `docs/audits/2026-09-14-full-ux/findings.md:50-68`. The current source preserves that negative-only contract. This does not establish that the original focus bug still reproduces today.

Expected coverage proves both validation and successful entry. Actual coverage can accept either an invalid edited value or a still-empty required field. Both produce no reply and a validation error.

Recommendation. Add one valid typed-value path through the real field, repaint the owning thread, submit, and inspect the exact transported typed answer. Assert that the prompt draft was unchanged. Keep the invalid-path test too.

Acceptance. Typing `2` into the required integer field survives a provider update and returns one integer value of 2 to the captured decision handle. The composer remains unchanged. Invalid `9` produces a visible error and sends nothing.

### coverage-quality-04. Default UI tests do not establish native Chromium behavior

Priority P2. Category verification-gap. Evidence source-proven.

CI intentionally omits Chromium builds, `.github/workflows/ci.yml:3-7,92-93`. The engine module distinguishes synthetic test frames from the optional real CEF engine, `crates/ferrite-pane/src/visual/web/mod.rs:14-29`. The feature definitions separate `cef`, `headless`, `visual-reference` and `browser-smoke`, `crates/ferrite-cockpit/Cargo.toml:11-23`.

Expected release confidence includes the actual host-to-Chromium input boundary, page focus, navigation and rendered-frame delivery. A default build and synthetic browser test do not establish those outcomes. This matters directly to the user's browser and panel complaints. It is not a claim that no native smoke tool exists.

Recommendation. Keep fast deterministic unit coverage. Add a small feature-aware native smoke gate for browser changes and run it once before handing back a browser user-flow change. Use a local fixture page and temporary settings/store. Avoid external sites and the operator's authenticated browser data.

Acceptance. On a CEF-enabled build, a real click focuses a local page input, typing and Enter reach that input, back/forward alter the visible document, and host Escape/focus return work at the boundary. Assert URL/document state as well as capture output. A fake-engine pass cannot satisfy this gate.

### coverage-quality-05. Window geometry still resets at launch

Priority P2. Category interaction. Evidence source-proven.

The trigger is reopening Ferrite after moving or resizing the window. The launch path always calculates a centered 1440 by 900 rectangle and passes it as `WindowBounds::Windowed`. Source is `crates/ferrite/src/lib.rs:349-357`. This matches the old UX-40 claim, `docs/audits/2026-09-14-full-ux/findings.md:711-724`.

Expected behavior is to restore the user's usable previous size, position and maximized state, with a fallback when its display no longer exists. T3 reads saved bounds, validates them against current displays, persists later bounds and restores maximization. Sources are [DesktopWindow.ts:452-470](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L452-L470), [535-565](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L535-L565) and [983](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/desktop/src/window/DesktopWindow.ts#L983).

The effect is repeated manual rearrangement, particularly for users keeping Ferrite beside another app. Panel-width persistence does not compensate for losing the top-level window arrangement.

Recommendation. Persist normal window bounds and maximized state, restore them only when usable on an attached display, and fall back to a fitting centered rectangle. Do not save minimized or transient zero-size bounds.

Acceptance. Move and resize a temporary-profile window, relaunch and observe the same usable bounds. Repeat maximized, then with the saved monitor disconnected. The window remains visible in every case.

### coverage-quality-06. Factory schedules do not cover one-time or sub-hour scheduled prompts

Priority P3. Category partial-capability. Evidence source-proven.

The trigger is wanting to run a prompt once at a chosen future time, or every few minutes. Ferrite's recurrence enum contains Daily, Weekdays, Weekly and Interval. Its interval picker offers 1, 2, 3, 4, 6, 8 and 12 hours. Its schedule instructions create workflow Tasks. Sources are `crates/ferrite-core/src/factory/starts.rs:80-153` and `api/schema.rs:204-213`.

T3 represents one-time dates, intervals of at least 60 seconds, fixed weekdays and webhooks. It also offers a thread destination selector. Sources are [scheduledTask.ts:25-74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L25-L74), [202-215](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/scheduledTask.ts#L202-L215) and [ScheduledTasksSettings.tsx:1250-1283](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ScheduledTasksSettings.tsx#L1250-L1283).

This is an optional product extension, not evidence that Ferrite's recurring workflow system is broken. The Factory already has missed-run and overlap policies at `factory/starts.rs:114-133`, which should be retained.

Recommendation. Decide whether lightweight thread reminders are in scope. If they are, support one-time scheduling and an explicit destination without requiring users to simulate it with a repeating Factory workflow. Document the distinction if workflow automation remains the sole model.

Acceptance. An opted-in one-time task fires once after restart, never repeats, shows its destination and result, and does not silently duplicate an active run. Recurring Factory behavior remains unchanged.

### coverage-quality-07. No first-class guest thread participation was found

Priority P3. Category missing-capability. Evidence inferred.

The trigger is inviting another person to watch a thread and propose a message without controlling the owner's agent. T3 has a Labs-gated guest link with expiry, creation errors, copy feedback, pending proposed messages and Stop sharing. It requires a T3 Connect tunnel. Sources are [guestShare.ts:13-70](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/guestShare.ts#L13-L70) and [GuestShareControls.tsx:73-202](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/GuestShareControls.tsx#L73-L202).

Ferrite's public API exposes owner-side thread, agent and decision actions, `crates/ferrite-core/src/api/schema.rs:118-137`. Its caller model distinguishes Anyone, Operator, Worker and Coordinator, `api/schema.rs:244-257`. Searches of application crates, API/protocol, palette and settings found no guest-share creation, recipient link or proposed-message inbox. The palette and settings entry-point models are `crates/ferrite-cockpit/src/palette.rs:139-190` and `crates/ferrite-ui/src/frame/settings.rs:15-43`.

Expected behavior here is a product-parity option, not a previously accepted Ferrite requirement. Actual evidence establishes no discovered first-class route. SSH machine attachment is a different capability and should not be described as guest collaboration.

Recommendation. Keep this below broken primary workflows. If collaboration becomes a product goal, define read access, expiry, revocation and owner approval of guest messages before adding a Share button.

Acceptance. If adopted, a guest can read the intended thread and propose a message. The owner must explicitly send or dismiss it. Revocation ends access, and unrelated threads remain inaccessible. Until adopted, classify this as a deliberate scope decision.

### coverage-quality-08. T3's device panel has no Ferrite panel counterpart

Priority P3. Category missing-capability. Evidence source-proven for the panel model.

T3 has an iOS/Android device target in its panel union and renders a DevicePanel for it. Sources are [rightPanelStore.ts:30-40](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/rightPanelStore.ts#L30-L40) and [RightPanelSurfaceContent.tsx:187-201](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L187-L201). Ferrite's exhaustive `SurfaceId` enum has no device kind, `crates/ferrite-cockpit/src/cockpit/surfaces.rs:25-46`.

The trigger is mobile-app development that needs an embedded device alongside the conversation. The parity expectation is an optional device-inspection capability. Actual Ferrite panels cover files, browser, terminal, side chat, agents, background work and a PR. A provider could still invoke external device tools; this finding does not claim otherwise.

Recommendation. Treat this as an explicit product boundary after the current browser and panel interactions work. If adopted, use the same tab identity and focus contract rather than introducing another competing side area.

Acceptance. Opening a device creates one identified device tab, a second open activates it, device input stays within that tab, and disconnect/reconnect has a visible state. Existing browser and terminal focus must remain stable.

### coverage-quality-09. CLI help advertises variadic provider arguments that the parser rejects

Priority P2. Category interaction. Evidence runtime-reproduced by the parent, supported by source.

The trigger is launching a provider with more than one passthrough option. The CLI synopsis prints list flags as `--args <args>…`, `crates/ferrite-core/src/api/cli.rs:448-469`. The parser consumes one value for each named flag and collects a list only when the same flag appears again, `api/cli.rs:252-270`. Its own launch test repeats `--args`, `api/tests.rs:1629-1656`.

Expected behavior is that the published synopsis clearly communicates the accepted invocation. Actual parent-observed behavior was that `--args --model fable --effort high` failed with `thread.start takes no --effort`, while `--args=--model --args=fable --args=--effort --args=high` launched successfully. The exact commands and installed CLI version are recorded in `docs/audits/2026-11-13-t3-parity/evidence/runtime-observations.md:8,30-45`. This worker did not launch or modify a thread.

The impact is avoidable failed automation and provider setup. An operator following the apparent list syntax cannot pass ordinary provider flags. The parser is deterministic; the discoverability contract is misleading.

Recommendation. Print repeatable-list syntax and a provider-argument example in generated help, or explicitly implement and document a trailing passthrough boundary. Prefer the smaller help correction unless a variadic passthrough design is chosen. Update callers that currently rely on the unsupported form.

Acceptance. The exact example emitted by `ferrite help thread start` parses to `args = ["--model", "fable", "--effort", "high"]`. A help/example regression checks that contract without launching a provider. Unknown Ferrite flags still produce a precise error.

## Earlier claims and promises reconciled

These are fresh source statuses, not fresh runtime verdicts. Unlisted September findings remain with their relevant audit partitions; this table does not silently mark them fixed.

| Earlier claim or promise | Current status | Fresh evidence and remaining boundary |
|---|---|---|
| UX-02. The first y/n/a in an empty composer can answer an approval. | Still present as an intentional tested interaction. | `crates/ferrite-ui/src/keymap.rs:195-197`; `cockpit/tests/composer_menus.rs:957-1027` explicitly avoids those first letters before demonstrating y approval. Input-focus owns the finding. |
| UX-03. Required form input can land in the composer. | Actual defect unknown; test blind spot still present. | Current invalid-only test is `cockpit/tests/provider_forms.rs:306-359`. A valid typed submission must be exercised. CQ-03. |
| UX-08. Parking loses the draft and attachments. | Partial counter-evidence; full old claim unknown. | Current text-draft save and restore tests are `cockpit/tests/drafts.rs:20-61`. They do not establish park/revive attachment persistence. Composer owns the complete lifecycle. |
| UX-30. No transcript Find and Cmd+F changes layout. | Fixed in source for the stated missing capability. | `crates/ferrite-ui/src/keymap.rs:265` binds Find. `cockpit/tests/find.rs:61-84` presses Cmd+F and targets an offscreen occurrence. This audit did not rerun it. |
| UX-40. Window position and size reset. | Still present in source. | `crates/ferrite/src/lib.rs:349-357` constructs fixed centered bounds. CQ-05. |
| UI-11. Image preview has no detail-inspection path. | Fixed in source for the stated missing capability. | `crates/ferrite-pane/src/image_viewer.rs:1-30` describes implemented fit/actual zoom, pinch and drag; `cockpit/tests/image_viewer.rs:375-442` exercises zoom and reduced motion. Native trackpad behavior remains unexecuted here. |
| UI-12. Code has no direct Copy action. | Fixed in source for direct Copy. | `crates/ferrite-pane/src/rich.rs:796-820` labels the action, copies exact code and sets feedback. This does not settle every code-block styling complaint. |
| UI-20. Tool-disclosure keyboard focus has no visible target. | Fixed in source for the tested path. | `cockpit/tests/layout_polish.rs:104-156` uses production keys, checks the painted target and activates it. It is not a blanket accessibility verdict. |
| ADR 0039. One per-thread list of tabbed panel kinds. | Implemented in the data model; caller correctness remains separate. | `cockpit/surfaces.rs:25-95` defines identities, active tab and persisted state. CQ-02 demonstrates why a model assertion cannot close navigation findings. |
| ADR 0038. A real headless window can receive server-driven input. | Implemented infrastructure. | `crates/ferrite-cockpit/src/headless.rs:1-5,27-48` uses the production window and loaded key settings on a throwaway store. It bypasses the OS input and accessibility stack by design. |
| Old T3 motion reference. Panel animation defaults to zero. | Current source still describes zero to avoid layout work; the user's motion requirement remains independent. | `docs/design/t3-reference/motion.md:3,14-16` is historical. Current [settings.ts:414-416](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/settings.ts#L414-L416) retains that design comment and default constant. The motion worker owns exact current default resolution. |

Ferrite `cockpit/...` paths in this table are under `crates/ferrite-cockpit/src/`.

The old T3 references explicitly pin `bcee0f5d`, for example `docs/design/t3-reference/panel-composer.md:3`, `sidebar.md:3` and `motion.md:3`. Their line numbers must not be reused as evidence for current T3 without a fresh lookup. The window and required-form examples also show why a completed September polish audit cannot certify November behavior.

## Positive Ferrite behavior worth preserving

- UI test windows mount the toolkit Root and the production CockpitWindow, `crates/ferrite-cockpit/src/cockpit/tests.rs:406-434`. This covers the real host tree instead of a standalone leaf component.
- The production-key helper initializes toolkit components and loads the full Mac key table, `tests.rs:922-932`. Tests should consistently use this when asserting key behavior.
- Link tests distinguish plain clicks, one-pixel movement, highlighted text, real selection drags and trackpad-style press/release without an intervening frame, `cockpit/tests/link_clicks.rs:56-161`.
- Selection regressions assert scroll position during press, drag, release and double-click, `cockpit/tests/interaction_regressions.rs:73-130`. These are observable interaction contracts.
- The headless runner uses a throwaway store and the configured keymap, `headless.rs:27-48`. It is suitable for isolated end-to-end server/window checks without disturbing the operator.
- SSH machines distinguish sign-in, missing installation and unreachable failures, `crates/ferrite-core/src/machine.rs:79-129`. Remote work is an existing product strength.
- Factory schedules explicitly model missed and overlapping runs, `crates/ferrite-core/src/factory/starts.rs:114-133`. New one-time scheduling should build on those semantics.
- PR tests already cover double-confirmation, disarming after leaving the panel and stale restored PR identities, `cockpit/tests/pr.rs:277-410`. Their value increases when CI actually runs them and their entry paths are exercised.

## Regression protection and remaining runtime checks

Use a small behavioral matrix. Each row below should assert the visible destination, focus owner and outgoing action, not only a changed field inside `CockpitView`.

| Narrow regression | Setup and observable assertion | Existing test home |
|---|---|---|
| Panel keyboard ownership | Empty panel, populated composer, focus row, Enter. Panel opens and no provider prompt is sent. | `cockpit/tests/tools.rs`. |
| PR entry-point equivalence | Overview, plus menu, palette, focused nav chip, another thread's chip and transcript link. Each shows the intended target, or explicitly opens the configured browser. | `cockpit/tests/pr.rs`, `link_clicks.rs`, `nav_shelves.rs`. |
| Valid decision form | Empty required field, type 2, repaint, submit. Exact integer answer reaches the captured handle and draft stays unchanged. | `cockpit/tests/provider_forms.rs`. |
| Browser host boundary | CEF-enabled local fixture page, field focused, type, Enter, Escape and host shortcut. Verify both page state and host focus. | Browser smoke feature, then relevant browser test file. |
| Narrow geometry | Repeat reachability at a small laptop viewport. Click visible controls through clipping and overlays. A debug rectangle alone does not count as a successful click. | The affected cockpit test, replacing reliance on PR fixture's 1900 by 900 setup. |
| Focus with asynchronous state | Open a menu or decision; receive a provider update; activate the same visible row. Assert the captured target and draft. | Existing owner-specific tests. |
| Window restoration | Save bounds, restart temporary profile, restore, then remove saved display. Check actual usable position. | App launch/window tests. |
| OS accessibility | Inspect accessible name, role, value, enabled state and focus for a PR chip, panel tab, browser address field and decision control. Navigate and activate with assistive technology. | Explicit native acceptance check; no equivalent proof was established from the inspected GPUI tests. |

Accessibility labels do exist, such as `crates/ferrite-pane/src/attachments.rs:242,334,415`, `crates/ferrite-pane/src/decision.rs:1030`, `crates/ferrite-cockpit/src/cockpit/subagents.rs:474` and the code Copy button at `rich.rs:809`. Their presence is positive but does not prove that the native accessibility tree exposes the entire workflow. Headless input bypasses that stack by definition, `headless.rs:1-5`. No VoiceOver or Windows assistive-technology run was performed here.

For a future focused fix, run the relevant `cargo nextest run -p <crate>` filter once after the last edit, then one typecheck. Examples are `cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::tools::an_empty_panel_says_open_a_surface_and_takes_its_keys)'` or the matching newly added regression, followed by `cargo check -p ferrite-cockpit`. A user-flow change also receives one appropriate end-to-end run. These commands are recommendations, not claimed results. Do not run the full suite for this report.

## Source map and delivery order

The contract inventory is T3's pinned `packages/contracts/src/index.ts:1-75`. Its panel dispatch is `apps/web/src/components/chat/RightPanelSurfaceContent.tsx:86-215`. Its unassigned product controls are guest sharing, scheduled tasks, device sources and desktop window startup linked above. Ferrite's corresponding structural sources are `cockpit/surfaces.rs`, `factory/starts.rs`, `machine.rs`, `api/schema.rs`, `crates/ferrite/src/lib.rs` and `.github/workflows/ci.yml`.

First make primary interaction regressions execute in CI. Then close handler-bypass and successful-form-input coverage gaps while the owning partitions repair the actual workflows. Add the feature-aware Chromium check and restore window geometry. Keep guest participation, device hosting and broader scheduling behind explicit product prioritization. This order addresses the user's existing broken work before expanding what Ferrite promises.
