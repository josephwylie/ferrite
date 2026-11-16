# Core explorer: attachments, Changes, and authenticated remote links
Grounded in Ferrite `7eaf7f8db9f9c313189259e799be7322589aa1f0` at `/Users/josephwylie/Desktop/Projects/ferrite-remote-connections`, with T3 Code reference `4da2bc520c38cd6b36014d2888774168fbf4c77f` at `/tmp/ferrite-t3-reference`. Read-only exploration; no repository modifications, application execution, builds, installs, or tests. This is architectural evidence, not runtime or visual parity proof. The rationale in `docs/audits/2026-11-16-remote-connections/architecture.md` was read and preserved.

## Components Found

### Attachment ownership

- `attachment::{Destination, Source, Readiness, Failure}` — `crates/ferrite-core/src/attachment.rs:23`: distinguishes a local picker/drop/paste file from a workspace reference and explicitly rejects `Remote + Local` as `NotOnMachine` before examining its bytes.
- `attachment::{retain, retain_bytes}` — same file, approximately lines 165 and 180: copies bytes to `<store>/attachments/<sha256-prefix>/<original-name>`, with same-content/name deduplication and `.part` then rename publication. Existing `retain` limit is 64 MiB; native image/PDF limit is 5 MiB; prompt inline total is 20 MiB.
- `DraftFiles` — `crates/ferrite-pane/src/draft_files.rs`: owns attachment paths, local-source paths, readiness and destination for each composer. `retain` returns immediately for a remote destination because the current contract assumes remote paths already reside there.
- `Composer::{aim_files, has_blocked_file, retain_files, content, take_draft, prompt, add_files, add_local_files}` — `crates/ferrite-pane/src/composer.rs:384,392,402,700,720,789,830,835`: public pane seam for attach/send state. `content()` preserves `files` and `local` separately; `take_draft` clears the composer only after constructing both typed content and serialized prompt.
- `CockpitView::{aim_attachments, admit_files, pane_root}` — `crates/ferrite-cockpit/src/cockpit/attachments.rs`: chooses Remote for a machine window, configures the local preferences directory as retained storage, and gates every send on readiness followed by retention.
- `prompt_files::{compose, split, paths}` — `crates/ferrite-core/src/prompt_files.rs`: prompt attachment persistence is trailing ordinary text `@"/absolute/path"`, not a separate upload schema. Server-side providers resolve these paths on their own machine.
- `Attachments` — `crates/ferrite-pane/src/attachments.rs`: draws readiness and retry controls and local image thumbnails from paths. Current rendering assumes image bytes can be opened locally.

### Remote reads and Changes

- `Cockpit::{review_diff, review_bases, pick_review_base, file_operation}` — `crates/ferrite-core/src/cockpit.rs:1993,2149,2168,1956`: existing facade routes operations to a Server or reads locally off-thread for an in-process engine.
- `Remote::{review_diff, reviewed, review_as_before, review_bases, file_operation}` — `crates/ferrite-core/src/cockpit/remote.rs:512,528,544,731,459`: maps requests/replies by command id and returns `Receiver<io::Result<T>>`; `review_read` falls back to old `review_diff` only on Unsupported.
- `Op::{ReviewRead, ReviewDiff, ListBases, PickBase, FileOperation}` — `crates/ferrite-core/src/protocol.rs:478,469,531,539,149`: already transports remote review and editor operations.
- `Done::{Review, Diff, Bases, FileOperation}` and `WireReviewFile` — protocol and `protocol/review.rs:20`: transport full old/new text, change kind, additions/deletions, rename path, and binary/large/unreadable notice. No staged-state metadata is currently on `WireReviewFile`.
- `Server` request dispatcher — `crates/ferrite-core/src/server.rs:836,854,874`: runs file and git work via `read_off_pump`; the remote Server owns paths and execution.
- `ferrite_reviewr::ferrite::{Read, FileSides, review, review_against, bases}` — `crates/ferrite-reviewr/src/ferrite.rs:29,124,138,144,442`: reusable non-UI git logic. `review_against(repo, read, Option<&str>)` supports per-call comparison base; ordinary `review` reads the persisted/default base.
- `Changes`, `Scope`, `Diff`, `CockpitView::{read_changes, stage_file}` — `crates/ferrite-cockpit/src/cockpit/tools/changes.rs:143,39,77,434,378`: Changes and PR Code panels share this implementation, including scope/folds/review marks/filter. It currently runs local `git`, local branch reads, and local `review_against` regardless of selected machine.
- `beside::{answered, land}` — `crates/ferrite-cockpit/src/cockpit/beside.rs:818,837`: existing async receiver landing pattern for the older Review view; avoids blocking the UI and drops results when scope changed.
- `RemoteFiles` — `crates/ferrite-cockpit/src/cockpit/tools/remote_files.rs`: recent remote editor pattern captures Bridge identity, checks `Arc::ptr_eq`, awaits asynchronously with a 30-second bound, and reports closed/cancelled connections rather than falling back locally.

### Connection/auth transport

- `Machine` — `crates/ferrite-core/src/machine.rs:27`: saved shape is exactly `{name, target, enabled}`. `target` currently means SSH host/alias or `ssh://user@host:port`; `Destination::parse` is SSH-specific and `check_target` delegates to it.
- `Tunnel`, `Dial`, `Bridge::new` — `crates/ferrite-core/src/machine.rs:269,277,324`: reusable transport seam. `Tunnel { from: Box<dyn Read + Send>, to: Box<dyn Write + Send>, ended: Box<dyn FnOnce() -> Option<Failure> + Send> }`; `Dial = Box<dyn Fn() -> io::Result<Tunnel> + Send + Sync>`; `Bridge::new(name: &str, socket: PathBuf, dial: Dial)` supplies a process-owned local socket for any such duplex transport.
- `Bridge::{ssh, attach, terminal_args}` — same file, lines 307,368,338: SSH construction supplies the dialer and SSH metadata. `attach` delegates to `link::Client::attach` on the local bridge socket. `terminal_args` explicitly errors when the bridge has no SSH metadata.
- `link::{Client, Attached, Lines, send, relay}` — `crates/ferrite-core/src/link.rs`: newline-delimited JSON protocol, 64 MiB max line, 32 MiB queued-output lag bound; `Client::attach` accepts only a local socket path and receives Welcome then Snapshot. Keeping Bridge means this client contract need not change.
- `link::unix::Listener` — `crates/ferrite-core/src/link/unix.rs`: store directory 0700, socket 0600, matching peer uid enforced before protocol. Windows counterpart is an owner-only named pipe. There is no network authentication in protocol Hello.
- `server::{ferrited, bridge::run}` — `crates/ferrite-core/src/server.rs:89`, `server/bridge.rs`: CLI has server, bridge, api-bridge, version and Linux service actions. `ferrited bridge` ensures Server and relays stdio. No pairing or network-listener command exists.
- `Machines` — `crates/ferrite-cockpit/src/machines.rs`: machine identity and reconciliation use `Machine.target`; reconciliation filters targets through SSH `check_target`. A new route cannot be added only in the Settings form.
- Terminal startup — `crates/ferrite-cockpit/src/cockpit/tools/terminal.rs:113`: remote terminal executes local `ssh` using `Bridge::terminal_args`; local terminal executes the directory shell. **No Terminal/PTY operations or events were found in `protocol.rs` or Cockpit facade.** Existing protocol does not secretly supply terminal support for a link-only Bridge.

### T3 comparison anchors

- `AddEnvironmentDialog.tsx:214,347,412,649` — `apps/web/src/components/settings/`: Remote link is host + pairing code; pasting a full pairing URL populates both fields. It calls `connectPairing` and retains errors.
- `resolveRemotePairingTarget` — `packages/shared/src/remote.ts:205`: resolves HTTP/HTTPS/WS/WSS host and token, defaults bare host to HTTPS, handles `#token` and legacy query tokens, supports hosted pairing URLs and `t3code://pair` links.
- `connectPairing` — `apps/web/src/connection/onboarding.ts:13`: calls `ConnectionOnboarding.registerPairing` using a single-flight command.
- `provisionDesktopSshEnvironment` — `apps/web/src/connection/platform.ts:230`: SSH provisioning obtains a pairing token, fetches environment descriptor, validates expected environment identity, exchanges the token for authenticated access. The T3 SSH and Remote-link routes converge on authenticated environment state.
- Environment auth API — `packages/contracts/src/environmentHttp.ts:586-640`: descriptor at `/.well-known/t3/environment`, `/oauth/token`, authenticated WebSocket ticket endpoint, create/revoke pairing link endpoints. This is T3's API, not something Ferrite currently implements.
- `attachmentUploadQueue.ts:136,168,265` and `attachmentUploadState.ts` — `apps/web/src/lib/`: per-environment attachment upload state includes uploading/progress, ready/id, failed/reason; bytes POST to a minted upload URL and ready state carries environment identity. Draft upload ids are invalid for another environment. Pending uploads block send; failed uploads can retry on send.

## Flow

### Current attachment flow

1. Picker/drop/paste calls `Composer::add_local_files`; workspace selection calls `add_files`. `DraftFiles::add` retains origin separately from the path.
2. `CockpitView::aim_attachments` configures Destination::Remote whenever `on_machine()`; `DraftFiles::inspect` calls core `attachment::inspect`.
3. Remote + Local immediately becomes Failed(NotOnMachine), with the exact explanation that Ferrite does not upload it. Remote + Workspace is accepted without local I/O.
4. Existing-thread `CockpitView::submit` calls `admit_files` around `cockpit.rs:5699`; new-thread `bootstrap_draft` does the same around `:8010`. Both stop before clearing the prompt when blocked.
5. Local attachment retention copies external files while workspace paths stay live. For remote destination retention is skipped.
6. Composer serializes absolute path trailers; send/bootstrap transports text. Provider-side path interpretation happens on the Server's computer. Merely removing NotOnMachine will send meaningless local paths to that machine.
**Minimal safe integration seam (recommendation, not observed implementation):** add an explicit core attachment-transfer operation, owned by Server and reachable through Cockpit/Remote, which accepts bounded named bytes and returns the Server-retained absolute path. Reuse existing immutable content storage after validating a single basename. Keep local source/preview separate from remote path until the upload succeeds; commit the remote reference only for the still-current composer and machine. Start uploads on attachment edits (T3 behavior) or gate sends asynchronously in the common `admit_files` seam; never block GPUI awaiting a network receiver. Both bootstrap and normal send must share the resulting admission state. Failure/retry/remove and machine switch must cancel/discard stale completions; preserve the draft on every failure.
The existing editor `FileOperation::Write` is not an upload seam: it only accepts UTF-8 text ≤2 MiB and canonicalizes an already-existing path within an existing workspace. It cannot write a new binary attachment into remote content storage.
A single base64 message needs a decoded cap comfortably below 48 MiB due to the 64 MiB line limit and framing overhead. Supporting existing 64 MiB retained files requires chunking or a separate authenticated byte stream. `retain_bytes` itself has no upload-size or filename validation and uses shared `.part` per content hash; it must not be exposed directly to arbitrary network names or concurrent uploads without guarding traversal and scratch-file collisions.

### Current Changes flow

1. Panel `SurfaceId::Changes` calls `ensure_changes`; PR Code calls `ensure_branch_changes`. Both create state keyed by `Diff::{Changes,PullRequest}(ThreadId)`.
2. `read_changes` obtains `thread_path` from the selected Thread's workspace and increments a generation. That path can be remote.
3. It spawns local work: `git::list_branches(root)`, `staged_paths(root)` using local `git diff --cached --name-only`, and local `review_against(root, read, base)`.
4. FileSides become displayed hunks plus whitespace-filtered hunks and content revision; only the current generation lands. Rendering/review marks/filtering are entirely reusable.
5. Stage/unstage calls local `git add -- path` or `git restore --staged -- path`, then rereads. This needs routing as much as reads do.
6. Contrast the existing Review-beside view: calls `Cockpit::review_diff`, then `Remote::review_diff`, protocol ReviewRead, Server `read_off_pump`, remote review core, Done::Review, and async landing.
**Minimal safe integration seam:** move Changes' complete read payload and stage action into a core operation facade that routes through the Server. Either (a) extend ReviewRead with optional per-call base and add explicit stage-status/stage-operation commands, or (b) add a cohesive ChangesRead request/reply carrying FileSides, branch names and staged paths plus a ChangesStage request. The latter keeps current per-panel base semantics isolated and avoids several loosely synchronized remote reads. Reuse WireReviewFile for sides and existing review core. Preserve generation checks and async landing in the panel. Local and remote must pass through the same facade for predictable behavior.
Do not emulate `Scope::Against` by calling global `PickBase` before ReviewRead: that writes persisted worktree selection and changes another review's scope. Current Changes compares per-call and is separate from PR Code scope. Optional generation-1 fields must use serde defaults; enums need Unknown fallback; append protocol fixtures rather than rewriting old fixtures. An old Server ignoring an optional `base` would silently compare the wrong branch, so capability-gate or use a new explicit operation for that behavior.

### Authenticated Remote link

1. Today the trust boundary is OS peer identity plus SSH authentication. Gen-1 Hello contains generation, peer name/version and capabilities, with no credential field.
2. `Bridge::new` already adapts arbitrary duplex byte transport to the local socket that Client understands. This is the narrow seam for a new transport while preserving Server/Engine/Store ownership.
3. A new HTTPS/WebSocket/HTTP-tunnel transport must authenticate **before** forwarding any bytes to owner-only server.sock. It must validate TLS server identity and never treat a pairing code as an SSH target/password. A bearer code printed into a URL must not appear in logs/errors, persistent display target, or child argv.
4. Pairing needs issuance, expiry, one-time consumption, authenticated session credentials, persistent endpoint identity, and reconnect semantics; neither issuance nor credential storage exists in the observed Ferrite path. A successful pair must prove protocol attach before saving the connection, as current SSH setup does.
5. Machine persistence/identity, validation, Bridge factory, failure/recovery and Settings UI must distinguish SSH and link routes. Keep source settings backward-compatible with `{name,target,enabled}`.
6. A link transport alone supports existing agent, Files and review protocol operations once authenticated. It does **not** make Terminal work. Remote terminal currently creates a local SSH PTY process, and no protocol PTY service was found. Full T3-like non-SSH remote terminal therefore needs explicit remote PTY start/input/resize/output/close messages and a terminal-view stream adapter, or a separately supplied SSH route. Showing a link form while always rejecting its terminal is not full functional parity.
**Scope verdict:** attachments and Changes fit small additive Server/Cockpit operation seams with pane state updates. Pairing is a materially larger new network/authentication feature, not an existing form toggle. Preserve the existing owner-only local listener; put authenticated encrypted ingress in a dedicated core module/process boundary, bridge it into server.sock through `Bridge::Dial`, and keep raw gen-1 execution commands off unauthenticated TCP.

## Files Read
Ferrite whole/sections: audit `architecture.md`; `crates/ferrite-core/Cargo.toml`; core `attachment.rs`, `prompt_files.rs`, `file_operations.rs`, `cockpit.rs`, `cockpit/remote.rs`, `protocol.rs`, `protocol/review.rs`, `server.rs`, `server/bridge.rs`, `server/tests/file_operations.rs`, `machine.rs`, `link.rs`, `link/unix.rs`, `bin/ferrited.rs`; pane `draft_files.rs`, `composer.rs`, `attachments.rs`; cockpit `cockpit.rs`, `cockpit/attachments.rs`, `cockpit/tools/changes.rs`, `cockpit/tools/remote_files.rs`, `cockpit/beside.rs`, `cockpit/tests/changes.rs`; reviewr `ferrite.rs`.
Ferrite search matches only: cockpit `panel/changes.rs`, `panel.rs`, `tools.rs`, `connections.rs`, `tools/terminal.rs`, `src/machines.rs`; frame `settings/connections.rs`; core `machine/setup.rs`, `settings.rs`, `engine/commands.rs`, `server/agent.rs`, `api.rs`, `api/{tools,tests,cli,schema}.rs`, `protocol/tests.rs`, `cockpit/remote/tests.rs`, `link/windows.rs`, provider wire modules and attachment tests. These are not full-file audits.
T3 whole/sections: `apps/web/src/components/settings/AddEnvironmentDialog.tsx`; `packages/shared/src/remote.ts`; `apps/web/src/connection/{onboarding,platform}.ts`; `apps/web/src/lib/{attachmentUploadQueue,attachmentUploadState}.ts`; `packages/contracts/src/environmentHttp.ts`.
T3 search matches only: SSH tunnel/command; contracts auth; client-runtime connection onboarding and tests; server attachmentStore/startupAccess; environment auth; attachment presentation/draft storage. Full server auth was not traced.
Workflow: `/Users/josephwylie/.codex/pstack/docs/harness/codex.md`, `skills/how/SKILL.md`, `skills/how/references/explorer-prompt.md`.

## Boundaries

- GPUI/pane owns input state, local source bytes/preview and progress. Core owns upload validation/retention, git operations, protocol and networking. Server owns remote filesystem, git index, agents and durable history.
- No need to add provider-specific upload APIs: replace submitted attachment path with a durable path on the provider's own Server before existing prompt processing.
- Machine changes rebuild Cockpit views, protecting ThreadId collisions across stores. Any new completion must also check its captured Bridge/session and draft identity; thread id alone is insufficient.
- A network ingress cannot inherit Unix socket peer-uid authorization: its authenticated session becomes the security boundary. Existing API and protocol have powerful file/agent actions; an auth-only UI is insufficient.
- Transport capabilities matter beyond attach: SSH's interactive auth and Terminal cannot automatically operate on a network-only bridge.

## Non-Obvious Things

- There are two diff experiences: the Review-beside view already routes remotely; the newer Changes/PR Code panels bypass that facade. Routing only one entry point leaves the shared PR Code gap.
- Attachment rejection precedes local file existence checks; simply changing the error wording or enabling send is incorrect.
- Remote workspace attachment paths are intentionally accepted without local inspection. Upload logic must only read local-source files, not try to open all remote paths on the desktop.
- Local and remote directories can have the same textual prefix; source identity must not be inferred from `path.starts_with(remote_root)`.
- Sending remote absolute paths can break local thumbnail/history previews unless local display source or an authenticated fetch/cache mapping is maintained separately. This is a visual parity dependency, not only a wire concern.
- Current content store deduplication and `.part` publication help idempotency, but transfer input validation, concurrency, size limits and expiry/cleanup are not supplied by `retain_bytes` itself.
- The existing 64 MiB retained-file limit is larger than a safe base64 payload inside the 64 MiB protocol frame.
- `review_bases` returns `Bases { rows, picked }`; a row includes current/default metadata and optional OID. It can feed Compare against without shelling out locally.
- Raw network reuse of `ClientMessage::Hello` offers no authentication. Reusing gen-1 is sound only behind authenticated encryption.
- `Bridge::carry` formats a dial-start failure as “ssh could not start”, another small text/generalization point for additional transports.
- Existing narrow tests provide useful harnesses: core server file-operation roundtrips; remote review fallback; GPUI Changes tests; composer attachment tests. None were run in this exploration.

## Open Questions

1. Product upload limit, chunking/streaming strategy, pending-upload cleanup, and whether directories are supported. Current local attachment behavior accepts directories/live references; remote upload cannot silently pretend a directory transferred.
2. How local preview bytes and remote durable path should persist across sent history/retry, restored drafts and app restart. Current prompt schema keeps only paths, although DraftContent has local-source identity.
3. Exact current provider-side behavior for all binary types was not inspected in full; provider wire searches show Claude image/PDF handling and Codex image handling, but upload should preserve their existing semantics.
4. No existing certificate provisioning, network listener, pairing token store, stable server identity or credential vault integration was found in the relevant core path. Choice of encrypted ingress, certificate identity and secure client session storage remains design work, not an implemented feature.
5. Cross-platform PTY ownership and terminal-view remote stream integration were not explored beyond proving the current SSH-only startup path and absence of protocol terminal operations.
6. The full T3 client-runtime registerPairing and server credential lifecycle were located but not deeply read. T3 is behavioral reference; Ferrite should not claim T3 wire interoperability without implementing its API, which is not required for visual parity.
7. No real network, upload, filesystem-isolation, rendered screenshots, native interaction or replay tests were performed. Runtime proof must use a Server-only file path and verify actual remote bytes/git index so a local-path coincidence cannot pass.
Expected implementation scope: core attachment transfer/store + protocol fixtures + Cockpit/Remote facade; pane/composer attachment lifecycle and preview mapping; core Changes operation facade + remote panel routing; new authenticated encrypted network ingress/pairing + persisted route/session identity + Bridge factory + corresponding CLI/UI + remote PTY service if full non-SSH terminal parity is required. Unit verification should be the narrow affected test filters with `cargo nextest run -p <crate>`, then one applicable typecheck, and one native user-flow E2E with screenshots after final edits, per the supplied AGENTS instructions.
