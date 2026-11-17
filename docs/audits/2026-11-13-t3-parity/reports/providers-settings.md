# Providers, settings and connection lifecycle

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants a source-backed account of Ferrite workflows that are broken, incomplete or missing compared with current T3 Code, with enough detail to make reliable sessions and settings work in practice. This report owns providers, session lifecycle, model capabilities, permission settings, connection diagnostics, remote settings and personalization.

## Overview

Ferrite has substantive native Claude and Codex integrations, durable threads, in-place model/effort changes, provider capability gates, searchable settings and SSH reconnect support. It is inaccurate to call those capabilities absent. The most consequential problems in this slice concern state ownership. Approval cards use global sandbox defaults as though they were the running session's actual permissions. Remote Settings writes a local file and asks a remote server to reread a different file. Provider updates execute locally but invalidate sessions on the active server.

The remaining gaps concern setup and recovery, provider breadth, project defaults and deliberate product differences. Extra drivers, light themes and relay connectivity are not prerequisites for fixing Ferrite's existing Claude/Codex workflows.

## Method and limits

- Ferrite was inspected at `62947f91d5cd6ee494aaae9ed09d794a853e5bab` in the shared repository.
- T3 Code was inspected at `74573fc29b901bafbf8f1dbb2eb29606da1fa448` in `/tmp/ferrite-t3-audit-source-20261113`. Links below are pinned to that commit.
- Method was source tracing through visible controls, handlers, state, persistence and provider/server operations. Broad searches included provider, settings, auth/login, sandbox/permission, resume/recovery, machine/SSH, theme and model catalog code. Existing test bodies were read for coverage and blind spots.
- No application code, settings or user data changed. No live UI, providers, test suite or typecheck ran. A source-proven control path is not a successful click in the installed binary. Runtime checks below remain outstanding.
- The codex harness adapter, Investigation instructions, How structure and Unslop rules were read. The parent owns the swarm and runtime checks; no nested agents were started. Throughput checkpoint: n/a, read-only investigation.
- ADRs 0038, 0039 and 0040 establish transcript/panel/nav intent, not a blanket promise to copy every T3 setting. Historical `docs/design/t3-reference` is not evidence of current T3 behavior.

## Key concepts

A Ferrite `Thread` is durable app history. A provider `Session` is a live process that can end without deleting the Thread. The Thread retains a native resume target. A local or SSH `Server` owns the live sessions; the window sends commands and mirrors activity. Settings combines window preferences and session/server defaults in one JSON file. Those concepts explain why ordinary local settings work and why SSH reveals an ownership defect.

T3 separates driver kind, provider instance, provider session and provider thread. Its environment identity travels with settings and provider-management requests. It also distinguishes client preferences from server settings and scoped project overrides. These distinctions are implementation facts, not a recommendation to transplant T3's architecture wholesale.

## Traced flows

### Open settings, search and persist a local default

1. The gear/`cmd-,` invokes `open_settings` and `toggle_settings`; opening resets to New threads, clears search and probes CLI versions if unknown. `crates/ferrite-cockpit/src/cockpit.rs:4010-4044`.
2. Six pages are built in the sheet: New threads, Permissions, Behaviour, Notifications, Server and About. Model/effort rows retain saved values. `crates/ferrite-cockpit/src/cockpit.rs:4485-4566,4805-4835`.
3. Search matches row title, hint, page, group and option words case-insensitively. The frame filters across pages. `crates/ferrite-ui/src/frame/settings.rs:183-192`; `crates/ferrite-frame/src/settings.rs:117-167`.
4. A choice reaches `setting_change`, then `change_settings`, which mutates memory, saves JSON and informs the cockpit. Save failures set a visible group error. `crates/ferrite-cockpit/src/cockpit.rs:4319-4364`.
5. Local SettingsChanged makes the server reload that same file and update session defaults. Save uses `store::replace_file`. Invalid JSON is preserved and a persistent notification explains that settings are temporary. `crates/ferrite-core/src/settings.rs:284-324`; `crates/ferrite-core/src/server.rs:784-789,1042-1053`; `crates/ferrite-cockpit/src/cockpit.rs:4202-4225`.
6. Across SSH, step 5 reloads the remote server's separate file. PS-02 describes the broken variant.

### Start, park, resume and tune a session

1. New-thread defaults choose one of two Providers and optional model/effort. A first prompt uses `bootstrap_draft`; the remote command includes provider, model, effort, workspace and prompt. `crates/ferrite-core/src/cockpit.rs:3190-3225`.
2. `Spawner` resolves the CLI and combines per-thread permission, launch args and shared defaults. Codex sandbox is omitted when no explicit setting exists. `crates/ferrite-core/src/spawner.rs:398-464`.
3. Startup runs off the UI caller. A SessionLifecycle is Starting until its receiver supplies a ready adapter; a failure is explicit. `crates/ferrite-core/src/session.rs:27-62`.
4. Codex chooses `thread/start` or `thread/resume`, obtains the effective model/approval/sandbox and announces permission mode. `crates/ferrite-core/src/providers/codex.rs:504-522,1037-1061`.
5. On a later send after the Session ends, the Thread's model, effort, native resume id and announced permission mode are passed into the replacement. `crates/ferrite-core/src/engine.rs:2532-2574,2635-2655`.
6. Model changes await native acknowledgement; unsupported model controls can resume the same conversation. Effort changes await acknowledgement without replacing the session. Rejections retain the old choice. `crates/ferrite-core/src/engine.rs:4191-4273`.
7. A missing native conversation is handled as startup failure and preserves the old target, producing PS-05. This is distinct from a healthy park/revive or server connection reconnect.

### Read model capabilities and operate native controls

The model catalog prefers provider announcements, then a saved catalog, then a bundled fallback. A saved choice remains available even when superseded, and effort options derive from the selected model's ladder. `crates/ferrite-core/src/providers/models.rs:128-158,204-226`; `crates/ferrite-core/src/providers/models/cache.rs:26-60`; `crates/ferrite-cockpit/src/cockpit.rs:4526-4537`.

The shared Session interface defaults unsupported controls to an explicit error. Claude advertises context/MCP refresh, MCP reconnect, task operations, permission modes and rewind support. Codex offers MCP status/login/reload and native permission changes. Codex MCP login sends `mcpServer/oauth/login`; permission changes send `thread/settings/update`. `crates/ferrite-core/src/providers/mod.rs:121-139`; `crates/ferrite-core/src/providers/claude.rs:556-600`; `crates/ferrite-core/src/providers/codex.rs:777-785`; `crates/ferrite-core/src/providers/codex/controls.rs:72-108`. Provider-account auth must not be confused with these existing MCP controls.

### Connect to a machine and recover transport

1. Settings > About > Machines accepts an SSH target, validates it and saves it. The hint explicitly requires `ferrited` on remote PATH. `crates/ferrite-cockpit/src/cockpit/machines.rs:354-385`.
2. The bridge runs SSH with keepalive and, on Unix, a shared control master. Remote signing-in happens in a terminal, without Ferrite storing a password. `crates/ferrite-core/src/machine.rs:1-7,354-380`.
3. Connection failures distinguish absent daemon, sign-in and unreachable. Sign-in failures wait for operator action rather than repeatedly submitting authentication attempts. `crates/ferrite-core/src/machine.rs:80-128`; `crates/ferrite-core/src/cockpit/remote.rs:259-291`.
4. Transport attachment retries with bounded exponential backoff. Successful attach records the server boot id and reestablishes watches with sequence cursors. `crates/ferrite-core/src/cockpit/remote.rs:776-800,829-892`.
5. A disconnect clears in-flight requests and never blindly resends mutations; startup operations are settled. Server restart has explicit grace and state handling. `crates/ferrite-core/src/cockpit/remote.rs:895-908,973-983`. Actual network failure behavior still needs runtime exercise.

## Source map

| Area | Ferrite source | Current T3 source |
| --- | --- | --- |
| Provider identity | `crates/ferrite-core/src/store.rs:146` | `packages/client-runtime/src/providerClients.ts:30`; `packages/contracts/src/providerInstance.ts:74` |
| Settings data and persistence | `crates/ferrite-core/src/settings.rs:28,284` | `apps/web/src/hooks/useSettings.ts:602` |
| Settings controls | `crates/ferrite-cockpit/src/cockpit.rs:4456` | `apps/web/src/components/settings/ProviderSettingsPanel.tsx:974` |
| Settings search | `crates/ferrite-ui/src/frame/settings.rs:183` | `apps/web/src/components/settings/settingsSearch.ts` |
| Session construction | `crates/ferrite-core/src/spawner.rs:398`; `crates/ferrite-core/src/session.rs:27` | `apps/server/src/orchestration-v2/ProviderTurnStartService.ts:983` |
| Permission description | `crates/ferrite-cockpit/src/cockpit/decisions.rs:65` | `apps/web/src/components/settings/ProjectDefaultsSettings.tsx:335` |
| Account readiness | `crates/ferrite-cockpit/src/cli_updates.rs:240` | `apps/web/src/components/settings/ProviderAuthenticationSection.tsx:43` |
| Remote transport | `crates/ferrite-core/src/machine.rs:1`; `crates/ferrite-core/src/cockpit/remote.rs:244` | `apps/web/src/components/settings/AddEnvironmentDialog.tsx:635` |
| Project defaults | `crates/ferrite-core/src/workspace/registry.rs:35` | `apps/web/src/components/settings/SettingsScopeContext.tsx:58` |
| Appearance | `crates/ferrite-ui/src/theme.rs:1263` | `apps/web/src/components/settings/AppearanceSettingsPanel.tsx:87` |

## Detailed findings

### PS-01: Approval cards can state the wrong Codex sandbox

Priority P1. Category `defect`. Evidence `source-proven`.

Symptom and impact: The operator can approve a command while the card claims a more restrictive sandbox than the process actually has. This is an incorrect permission description, not evidence that Ferrite escapes the provider sandbox.

Trigger: Leave Settings > Permissions > Codex > Sandbox at CLI default while Codex config uses workspace-write or danger-full-access. Alternatively, start a session under one explicit sandbox and change the global default before its next approval.

Expected behavior: The approval head describes that session's effective negotiated sandbox. An unknown sandbox stays unknown; changing future-session defaults cannot relabel an existing session.

Actual behavior: The card calls decision_context, which reads current global settings.sandbox_for. None becomes read-only. The launch path instead omits sandbox for None and permits Codex configuration to resolve it. Explicit global changes also immediately change the label without changing the existing process.

Cause: The adapter parses an effective CodexCapabilities.sandbox but the decision projection carries permission mode and uses Settings for sandbox. The UI has substituted a default for live session state.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/decisions.rs:65-79`.
- `crates/ferrite-core/src/settings.rs:365-372`.
- `crates/ferrite-pane/src/decision.rs:113-123`.
- `crates/ferrite-core/src/spawner.rs:453-464`.
- `crates/ferrite-core/src/providers/codex.rs:504-522`.
- `crates/ferrite-core/src/providers/codex.rs:239-251`.
- `crates/ferrite-core/src/providers/codex.rs:1048-1055`.

T3 evidence is not needed for this internal correctness defect. The claim compares Ferrite's own displayed state with its own launch and handshake state.

Recommendation: Project the negotiated sandbox through session events, thread state and the wire snapshot, and render that value. Keep requested defaults distinct from effective values.

Observable acceptance check: With a stub Codex handshake returning workspaceWrite while Settings is CLI default, an actual clicked approval card says workspace-write. Change the default to read-only and verify the existing card remains workspace-write. Repeat for a remote thread and dangerFullAccess. No real destructive command is needed.

### PS-02: Remote permission and server defaults save to the local machine

Priority P1. Category `defect`. Evidence `source-proven`.

Symptom and impact: On an SSH machine, settings can appear saved while new remote sessions continue using the remote server's old permission and sandbox defaults. Factory concurrency and other server-side preferences have the same ownership problem.

Trigger: Attach to another machine, change Claude permissions or Codex sandbox in Settings, then create a new remote thread.

Expected behavior: The UI names the settings target and persists server defaults there, or explicitly labels these controls local-only and prevents users from assuming they configure the connected machine.

Actual behavior: Preferences.dir remains the local settings_dir even when core comes from a remote machine. change_settings writes that local JSON and sends SettingsChanged with no values. The remote server reloads its own settings_dir. Bootstrap explicitly sends model and effort but no permission or sandbox, so those values come from the unchanged remote defaults.

Cause: The SettingsChanged protocol assumes window and server share a filesystem. The UI reused that protocol across SSH without a remote settings mutation or target-specific settings snapshot.

Ferrite evidence:

- `crates/ferrite/src/lib.rs:274-301`.
- `crates/ferrite/src/lib.rs:394-406`.
- `crates/ferrite-cockpit/src/cockpit.rs:4319-4334`.
- `crates/ferrite-core/src/cockpit.rs:2477-2488`.
- `crates/ferrite-core/src/server.rs:784-789`.
- `crates/ferrite-core/src/server.rs:1042-1053`.
- `crates/ferrite-core/src/cockpit.rs:3216-3224`.

T3 comparison evidence:

- [apps/web/src/hooks/useSettings.ts:602](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useSettings.ts#L602).
- [apps/web/src/components/settings/SettingsScopeContext.tsx:58](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsScopeContext.tsx#L58).

Recommendation: Separate window preferences from server settings. Add a targeted server-settings read/write operation with an acknowledgement and display the machine whose defaults are edited. Do not silently synchronize unrelated local values.

Observable acceptance check: Use two isolated settings directories with different permission/sandbox defaults. Through a remote window change one default, create a thread and inspect its provider launch arguments/handshake. Only the selected machine changes, relaunch retains it, and denied/offline writes display an unsaved state.

### PS-03: Provider version checks and updates target local binaries while a remote server is active

Priority P2. Category `defect`. Evidence `source-proven`.

Symptom and impact: A user trying to repair a remote CLI sees the local CLI version and can update the local installation. Ferrite then tells the remote server that its CLI was replaced, causing unnecessary remote session retirement without upgrading that remote CLI.

Trigger: Open Settings > About > Provider CLIs on an SSH machine whose installed provider version differs from this computer, then press Update or allow an automatic update.

Expected behavior: The version, update execution, live-session gate and subsequent invalidation all refer to the same clearly named machine.

Actual behavior: probe_cli_versions and update::run execute in the window process on the local machine. live_sessions and cli_replaced use the currently attached cockpit/server. No on_machine guard or machine target enters the update request.

Cause: The update coordinator mixes local filesystem work with active-server lifecycle operations. Version diagnostics have no machine label to expose the mismatch.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:4033-4044`.
- `crates/ferrite-cockpit/src/cockpit.rs:4083-4115`.
- `crates/ferrite-cockpit/src/cockpit.rs:12968-12975`.
- `crates/ferrite-core/src/providers/discover.rs:170-184`.
- `crates/ferrite-core/src/cockpit.rs:1624-1636`.
- `crates/ferrite-core/src/providers/update.rs:214-231`.

T3 comparison evidence:

- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:635](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L635).
- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:657](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L657).

Recommendation: Route remote provider inspection and upgrades through the selected server, with machine-labelled progress. If remote upgrades remain unsupported, label local controls and never invalidate remote sessions after a local update.

Observable acceptance check: Use fake local and remote provider executables with distinct versions and update markers. In a remote window, update the selected remote provider and verify only its marker changes and only its sessions retire. A local-only fallback must name this computer and leave remote sessions intact.

### PS-04: Provider setup stops at CLI presence and update status

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: A new user or a user with expired credentials cannot complete provider setup, identify the active account, or retry provider sign-in from Ferrite settings. Existing native CLI authentication can still work.

Trigger: Use Ferrite without one CLI installed, with an installed but signed-out CLI, or after credentials expire or an account changes outside the app.

Expected behavior: Provider readiness separates installation, authentication, compatibility and model availability. The operator gets a concrete next action and can recheck after completing it.

Actual behavior: The settings row reports version/path or not installed, and Missing produces no action button. Settings has no provider account or sign-in section. Spawn errors distinguish missing/version/handshake failures, but there is no settings-level provider authentication state machine. MCP sign-in and SSH sign-in are separate supported features, not provider-account setup.

Cause: Provider initialization depends on existing CLI installations and their external credentials. Discovery probes --version; the settings page exposes update state rather than provider readiness.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit.rs:4743-4753`.
- `crates/ferrite-cockpit/src/cockpit.rs:4804-4835`.
- `crates/ferrite-cockpit/src/cockpit.rs:12968-12975`.
- `crates/ferrite-cockpit/src/cli_updates.rs:240-263`.
- `crates/ferrite-core/src/providers/update.rs:194-218`.
- `crates/ferrite-core/src/providers/codex.rs:184-227`.

T3 comparison evidence:

- [apps/web/src/components/settings/ProviderAuthenticationSection.tsx:43](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderAuthenticationSection.tsx#L43).
- [apps/web/src/components/settings/ProviderAuthenticationSection.tsx:279](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderAuthenticationSection.tsx#L279).
- [apps/web/src/components/settings/ProviderInstanceCard.tsx:750](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderInstanceCard.tsx#L750).
- [apps/web/src/components/settings/ProviderSetupSection.tsx:110](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSetupSection.tsx#L110).
- [apps/server/src/provider/ProviderAuthService.ts:93](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/provider/ProviderAuthService.ts#L93).

Recommendation: Add a compact provider readiness/setup panel with the correct machine, detected binary, version, account/auth status where supported, setup instructions or managed install, recheck, and sign-in/retry actions. Preserve externally managed CLI installs.

Observable acceptance check: Exercise absent CLI, signed-out CLI, expired auth, supported logged-in CLI and CLI version rejection. Each state has a distinct message and working next action. Rechecking reflects changed credentials without starting a paid prompt. Account switching must not leave existing helpers using the old credential binding.

### PS-05: An unavailable native conversation has no same-thread context recovery path

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: If a provider's saved conversation disappears or cannot be resumed, the Ferrite transcript can remain readable while sending another prompt repeats the same failed resume. Users must reconstruct the conversation or switch providers through a separate workflow.

Trigger: Resume an existing Codex thread whose provider-native session is unavailable, then retry sending. A stub provider returning a permanent thread/resume failure is sufficient.

Expected behavior: Preserve the durable transcript and draft, explain why native resume failed, and offer a same-thread recovery into a fresh native conversation using disclosed retained context. Do not recover automatically for a thread-in-use error.

Actual behavior: The regular startup failure path drops the Session, reports a Notice and gives up awaiting work. end_session leaves the resume id intact. The next spawn passes the same resume id again. No source path in this normal restart flow distinguishes permanent missing-history failure or synthesizes replacement context.

Cause: Ferrite models provider resume identity as a single durable target and treats startup errors uniformly. Its separate handover/fork functionality does not provide a native-resume-failure recovery action. Codex does retry transient active-writer conflicts before returning a failure, at `crates/ferrite-core/src/providers/codex.rs:302-317`; this finding concerns permanent missing native history.

Ferrite evidence:

- `crates/ferrite-core/src/engine.rs:622-625`.
- `crates/ferrite-core/src/engine.rs:1051-1054`.
- `crates/ferrite-core/src/engine.rs:2583-2590`.
- `crates/ferrite-core/src/engine.rs:2635-2644`.
- `crates/ferrite-core/src/engine.rs:3236-3241`.
- `crates/ferrite-core/src/providers/codex.rs:504-508`.

T3 comparison evidence:

- [apps/server/src/orchestration-v2/ProviderTurnStartService.ts:983](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/ProviderTurnStartService.ts#L983).
- [apps/server/src/orchestration-v2/ProviderTurnStartService.ts:1307](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/ProviderTurnStartService.ts#L1307).

Recommendation: Classify resume errors, preserve the failed prompt, and provide an explicit fresh-conversation recovery action with a context preview or clear disclosure. Keep provider-lock contention as a refusal.

Observable acceptance check: With a provider stub that rejects the old native id, recover within the same Ferrite thread and send the preserved prompt once to a new native id with retained context. The old transcript remains. A thread-in-use refusal must not silently fork. Live-provider behavior remains to be checked.

### PS-06: Provider choices do not include T3's additional drivers or named provider instances

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: Users of Cursor, Grok, OpenCode, Antigravity, Kiro, Muse, Pi, FX or registry ACP agents cannot select those drivers in Ferrite. Users cannot configure separate named instances of one driver with different credentials, environments or binaries.

Trigger: Try to add a non-Claude/non-Codex driver, or use two separately named Codex/Claude configurations without changing external CLI configuration.

Expected behavior: If provider breadth is a product goal, provider instances are explicit identities with their own validated config and runtime support. Otherwise Ferrite should state its two-provider scope.

Actual behavior: Provider is a closed Claude/Codex enum. Default controls iterate those two values and discovery picks one newest local CLI per provider. T3 has a driver registry and a separate instance-id/config map. Ferrite CLI launch args/env are useful per-thread escape hatches, not an in-app instance registry.

Cause: Ferrite intentionally has deep native adapters for two providers; the source does not claim extensible drivers. This is a product capability difference, not proof that its two adapters malfunction.

Ferrite evidence:

- `crates/ferrite-core/src/store.rs:146-157`.
- `crates/ferrite-core/src/spawner.rs:408-420`.
- `crates/ferrite-core/src/providers/discover.rs:170-198`.
- `crates/ferrite-core/src/settings.rs:28-50`.
- `crates/ferrite-cockpit/src/cockpit.rs:4485-4504`.

T3 comparison evidence:

- [packages/client-runtime/src/providerClients.ts:30](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/providerClients.ts#L30).
- [packages/contracts/src/providerInstance.ts:74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/contracts/src/providerInstance.ts#L74).
- [apps/web/src/components/settings/ProviderSettingsPanel.tsx:974](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderSettingsPanel.tsx#L974).

Recommendation: Decide which additional providers the product actually wants before expanding. If multiple configurations matter first, introduce instance identity separately from driver kind and retain per-driver capability gates.

Observable acceptance check: For any adopted driver, complete setup, new thread, stream, approval, stop, reopen and failure recovery. For instances, run two named configurations concurrently, restart Ferrite and verify each resumes with its original config. Do not mark an icon or model-list entry as support.

### PS-07: Claude Auto and Don't ask are available per session but absent from new-thread defaults

Priority P2. Category `partial-capability`. Evidence `source-proven`.

Symptom and impact: An operator who consistently uses Claude Auto or Don't ask must change each new thread or edit JSON/native CLI configuration. Settings cannot express all permission modes that Ferrite already supports.

Trigger: Choose Auto or Don't ask in a live Claude thread, then look for the same choice in Settings > Permissions as a default for future threads.

Expected behavior: Supported and suitable launch modes are selectable as defaults, with capability-aware wording. A saved supported mode remains visibly selected.

Actual behavior: The live list includes default, acceptEdits, plan, dontAsk, bypassPermissions and auto, subject to bypass restrictions. The Settings list only includes CLI default, Ask, Accept edits, Plan and Bypass permissions. A hand-edited auto value has no matching selected segment.

Cause: The defaults UI duplicates a smaller static mode list instead of sharing the supported launch-mode catalog.

Ferrite evidence:

- `crates/ferrite-core/src/providers/claude.rs:533-551`.
- `crates/ferrite-cockpit/src/cockpit.rs:4580-4595`.
- `crates/ferrite-core/src/settings.rs:354-361`.
- `crates/ferrite-core/src/spawner.rs:427-430`.

T3 comparison evidence:

- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L335).

Recommendation: Build defaults from a provider-specific launch-mode catalog that includes supported Auto and Don't ask, while retaining live-only restrictions for mode changes and a visible representation for unknown saved values.

Observable acceptance check: Select Auto in Settings through the UI, relaunch and create a Claude thread. The provider receives auto and the setting remains selected. Repeat for dontAsk and a previously saved unsupported value. This is an internal Ferrite inconsistency; T3's unified modes are only a related comparison.

### PS-08: Provider defaults have no project override or fallback-model settings

Priority P2. Category `missing-capability`. Evidence `source-proven`.

Symptom and impact: Users who work across projects with different model/permission needs must keep changing thread choices or global defaults. A chosen default provider exhausting its allowance does not have a configured fallback model for new threads.

Trigger: Keep one project on a conservative provider/model/permission default and another on a different default, or choose a fallback provider for new threads when the primary is out of usage.

Expected behavior: If this workflow is in scope, defaults show their global/project source and allow a project override without altering existing threads. Fallback behavior is explicit and limited to the defined trigger.

Actual behavior: Ferrite Settings holds one set of defaults per provider. Project records hold identity, root and additional roots, not provider/model/permission overrides. There is no fallback-model setting. Existing per-thread choices and provider handover cover manual adjustment.

Cause: Defaults live in one machine-local settings object rather than a scoped resolution model. This is capability scope, not a failure of existing per-thread persistence.

Ferrite evidence:

- `crates/ferrite-core/src/settings.rs:28-50`.
- `crates/ferrite-core/src/workspace/registry.rs:35-51`.
- `crates/ferrite-core/src/workspace/registry.rs:87-95`.
- `crates/ferrite-cockpit/src/cockpit.rs:4485-4566`.

T3 comparison evidence:

- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:250](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L250).
- [apps/web/src/components/settings/ProjectDefaultsSettings.tsx:335](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L335).
- [apps/web/src/components/settings/SettingsScopeContext.tsx:58](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/SettingsScopeContext.tsx#L58).

Recommendation: Prioritize project defaults only if repeated manual changes are a real workflow. Use a small inheritance model and show the resolved source. Treat allowance-triggered fallback as a separate opt-in policy.

Observable acceptance check: Configure two projects with different defaults, create a thread in each and verify the right provider/model/permission after restart. Existing threads retain their own choices. A synthetic exhausted allowance selects only the configured fallback for new threads and visibly explains that selection.

### PS-09: Appearance is deliberately dark-only with fixed font families

Priority P3. Category `intentional-difference`. Evidence `source-proven`.

Symptom and impact: Users cannot choose a light/system theme, import a theme, or adjust interface and monospace font families independently. Transcript size and notification customization do exist.

Trigger: Try to match system light appearance, choose a preferred reading/code typeface, or import an existing theme.

Expected behavior: This requires a product decision. If adopted, appearance choices persist, preserve readable contrast and apply consistently across panels and native components.

Actual behavior: Theme initialization explicitly selects ThemeMode::Dark and assigns fixed UI/code families. The theme file documents dark-only intent. The settings schema exposes reading size but no theme or font-family controls.

Cause: This is an explicit design choice, not an accidentally disconnected theme switch. T3 has appearance/theme library and separate typography settings.

Ferrite evidence:

- `crates/ferrite-ui/src/theme.rs:172-173`.
- `crates/ferrite-ui/src/theme.rs:1263-1276`.
- `crates/ferrite-core/src/settings.rs:28-147`.
- `crates/ferrite-core/src/settings.rs:188-205`.

T3 comparison evidence:

- [apps/web/src/components/settings/AppearanceSettingsPanel.tsx:87](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L87).
- [apps/web/src/components/settings/AppearanceSettingsPanel.tsx:145](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L145).
- [apps/web/src/components/settings/TypographySettings.tsx:74](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/TypographySettings.tsx#L74).
- [apps/web/src/components/settings/TypographySettings.tsx:144](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/TypographySettings.tsx#L144).

Recommendation: Keep the distinction visible in the parity inventory. Revisit dark-only and typography scope based on user preference rather than copying T3's whole theme editor.

Observable acceptance check: If expanded, change appearance and fonts, restart and inspect transcript, composer, menus, terminal and settings at representative sizes. If scope stays fixed, documentation accurately states the supported choices.

### PS-10: Remote access supports SSH, not T3's pairing links and alternate routes

Priority P2. Category `intentional-difference`. Evidence `source-proven`.

Symptom and impact: A machine reachable through a T3 pairing URL or relay cannot be added through Ferrite's machine settings. Ferrite also expects ferrited to be installed on the remote PATH before connection.

Trigger: Add a machine by pairing code/URL, use relay fallback when a direct route disappears, or connect before the remote daemon is installed.

Expected behavior: This requires an explicit access-model decision. SSH support should work as documented; URL/relay parity requires a separately designed authentication and route lifecycle.

Actual behavior: Ferrite adds an SSH target and runs ferrited bridge. Its failure model distinguishes missing daemon, sign-in and unreachable. T3 offers Remote link and SSH and can add a T3 Connect fallback route to the same machine.

Cause: Ferrite's accepted ADR chooses SSH and external daemon installation. No broader remote pairing/relay route is represented in Machine.

Ferrite evidence:

- `crates/ferrite-core/src/machine.rs:1-7`.
- `crates/ferrite-core/src/machine.rs:80-128`.
- `crates/ferrite-core/src/machine.rs:379-385`.
- `crates/ferrite-cockpit/src/cockpit/machines.rs:354-385`.
- `docs/adr/0026-machines-over-ssh.md:8-29`.

T3 comparison evidence:

- [apps/web/src/components/settings/AddEnvironmentDialog.tsx:635](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AddEnvironmentDialog.tsx#L635).
- [apps/web/src/components/settings/AddEnvironmentDialog.tsx:438](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AddEnvironmentDialog.tsx#L438).

Recommendation: Do not describe remote support as absent. Document the existing SSH prerequisite and decide whether pairing/relay or assisted bootstrap is needed. Keep these optional expansions separate from the real remote settings/update defects.

Observable acceptance check: For present scope, a configured SSH target reconnects and names an absent daemon clearly. If alternate routes are added, pair once, force direct-route failure and verify failover preserves machine/thread identity and requires no duplicate environment.

## Supported T3 capabilities and positive Ferrite coverage

| Capability | T3 source evidence | Ferrite status |
| --- | --- | --- |
| Additional provider drivers and independent instances | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/packages/client-runtime/src/providerClients.ts#L30) | Two deep native adapters. Per-thread CLI launch settings are available, but no instance registry. PS-06 is a scope gap. |
| Account sign-in, retry, change account and sign-out | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderAuthenticationSection.tsx#L279) | Existing CLI credentials are used. MCP OAuth and SSH sign-in are implemented separately. PS-04 covers provider account setup. |
| Remote settings and provider update targets | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/hooks/useSettings.ts#L602) | Local settings persist atomically. SSH settings and updates confuse local and server ownership. PS-02 and PS-03. |
| Recovery after native resume failure | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/server/src/orchestration-v2/ProviderTurnStartService.ts#L1307) | Ordinary native resume, park/revive and transport reconnect exist. Fresh-context recovery after a permanent native failure is missing. PS-05. |
| Model preferences, custom models, visibility and order | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProviderInstanceCard.tsx#L1403) | Dynamic model catalogs, cache/fallback, favorites and model-specific effort ladders exist. Custom model management and ordering are narrower. Composer/model-picker ownership covers detailed selection UX. |
| Project-specific defaults and allowance fallback | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/ProjectDefaultsSettings.tsx#L250) | Global defaults and durable per-thread choices exist. Project override/fallback policy is absent. PS-08. |
| Searchable settings | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/settingsSearch.ts#L1493) | Ferrite searches page/group/row/options across six pages. Settings are reachable by keyboard and gear. No missing-settings claim is warranted. |
| Themes and configurable typography | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AppearanceSettingsPanel.tsx#L145) | Ferrite provides nine transcript sizes from 12 through 24 px, custom sounds and per-provider sound toggles. Fixed dark theme and font families are deliberate. PS-09. |
| Remote pairing and route choices | [Pinned implementation](https://github.com/pingdotgg/t3code/blob/74573fc29b901bafbf8f1dbb2eb29606da1fa448/apps/web/src/components/settings/AddEnvironmentDialog.tsx#L635) | Ferrite has multi-machine SSH, sign-in prompts, failure states and reconnect. URL pairing/relay are a separate scope choice. PS-10. |

Other positive details matter for remediation. Codex startup retries a transient active-writer conflict before failing, at `crates/ferrite-core/src/providers/codex.rs:302-317`. The settings reader does not destroy a damaged JSON file. It warns persistently and avoids saving over it. CLI discovery searches beyond PATH and picks the newest responding installation. Updates wait until a provider has no live sessions for automatic installation; local manual updates retire old sessions as they settle. Model/effort setting acknowledgements preserve the old state on rejection. A remote disconnect does not blindly replay an in-flight mutation. These behaviors should survive any settings redesign.

Source anchors are `crates/ferrite-core/src/settings.rs:284-324`, `crates/ferrite-cockpit/src/cockpit.rs:4202-4225,4081-4115`, `crates/ferrite-core/src/providers/discover.rs:170-198`, `crates/ferrite-core/src/engine.rs:4191-4273` and `crates/ferrite-core/src/cockpit/remote.rs:895-908`.

## Verification gaps and regression protection

No findings in this slice are runtime-reproduced. The source establishes the incorrect data paths, but it does not establish how often users encounter them or whether the installed binary matches this commit.

Existing coverage includes UI simulation for settings opening, searching, clicking and saving in `crates/ferrite-cockpit/src/cockpit/tests/settings_window.rs:253`; corruption and symlink behavior in `crates/ferrite-core/src/settings.rs:665,721,743,807`; native permission and MCP actions in `crates/ferrite-cockpit/src/cockpit/tests/provider_controls.rs:165,315`; acknowledged effort changes and refusals in `crates/ferrite-core/src/cockpit/tests.rs:4675,4708`; and non-replay/sign-in reconnect cases in `crates/ferrite-core/src/cockpit/remote/tests.rs:139,184,225`. These tests were inspected, not run. Their local/fake setup does not prove machine-specific settings persistence, runtime sandbox wording or provider account readiness.

The parent should prioritize these runtime checks:

1. Open an approval for a stub session whose negotiated sandbox differs from the default and inspect the actual displayed words. Change the future default while the session remains open.
2. Use separate local and remote settings stores and fake provider binaries. Change a remote permission default through the UI, relaunch, then check the remote launch parameters. This also distinguishes a displayed setting from a persisted remote setting.
3. Verify provider version and update targeting on two machines with deliberately different versions. Do not update the user's real CLI for the audit.
4. Exercise a permanent missing-native-session response. Verify the prompt's visibility and retry behavior, then test lock contention separately. Do not delete user history to induce it.
5. Exercise provider readiness with absent, signed-out, expired and healthy states in disposable configuration. A passing CLI version probe is not authentication proof.
6. Disconnect transport during streaming and during a queued mutation, then reconnect to both the same boot and a new server boot. Check transcript continuity and absence of duplicate side effects.
7. Change model and effort before first prompt, while idle after a turn, after a provider refusal, after park/revive and after server restart. Confirm stored choices and effective choices agree.
8. Inspect settings search, focus and labels in the actual installed UI. The input-focus partition owns general keyboard behavior; the motion partition owns animation and reduced-motion review.

This report does not recommend running the full suite. Any implementation should add the smallest regression fixture for its changed path, run the narrowest applicable nextest target once after the last edit, then one typecheck and one flow E2E where required by AGENTS.md. No test pass is claimed here.

## Remediation order

Correct live permission descriptions first. Then give remote server settings and provider maintenance explicit machine ownership. Those changes repair promises the existing UI already makes. Add provider readiness and native-history recovery next so failed launches become recoverable workflows. Finally decide the scope of extra providers, project defaults, themes and alternate remote routes. Copying T3's inventory without that ordering would leave the three concrete correctness defects in place.
