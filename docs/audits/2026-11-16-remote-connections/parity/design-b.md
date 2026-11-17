# Candidate B: one environment session, three routes
Grounded at Ferrite `7eaf7f8d`, T3 `4da2bc5`, architecture/workflow documents and `/tmp/ferrite-parity-how-{core,ui}.md`. Design only: no repo changes, builds or tests.

## Problem and decision
Finish all five gaps through the existing Server execution boundary: native modal/discovery, genuine link pairing, attachments, Changes and a terminal that works without SSH. Evolve `cockpit::Remote` into the one environment session; do not add another reconnect owner above it. Keep SSH and local socket routes, add an authenticated WSS gateway route, and make capability consumers stop asking for `Bridge` or spawning local git/SSH. The distinguishing choice is one route-independent session contract, including server-owned PTYs for both SSH and links, rather than preserving separate SSH-only feature implementations.

## Usage first
```rust
// Add modal controller: validation and asynchronous work retain current draft on failure.
let attempt = connections.prepare(AddConnection::parse(fields)?, expected_environment);
// Existing poll_connections consumes progress/challenges; success produces PreparedConnection.
connections.commit(prepared, &mut settings)?; // existing atomic settings writer/unsaved guards
// Changes and PR Code share this, regardless of local/SSH/link route.
let pending = cockpit.changes(thread, ChangesScope::Against(branch))?;
let pending = cockpit.set_staged(thread, RepoPath::parse(file)?, true)?;
// Attach immediately; progress/ready/error feed the existing draft tile.
let transfer = cockpit.upload_attachment(LocalAttachment::snapshot(path)?)?;
composer.land_upload(draft_id, attachment_id, captured_environment, transfer_result);
// TerminalView consumes one stream; no UI branch for SSH, nor a fallback local shell.
let pending = cockpit.open_terminal(thread, TerminalSize { columns: 100, rows: 30 })?;
let view = Terminal::from_session(pending_result, scrollback)?;
```

## Shape: identity and connection lifecycle
```rust
pub struct EnvironmentId(String); // stable random id persisted once beside Server Store
pub struct RouteId(String);       // transport identity, never a credential
pub struct CredentialRef(String); // opaque key into OS credential store
pub enum Route { Ssh(SshTarget), Link { origin: HttpsOrigin, credential: CredentialRef } }
pub struct ConnectionProfile { route: Route, expected: EnvironmentId }
pub enum AddConnection { Ssh(SshTarget), Link { origin: HttpsOrigin, code: PairingCode } }
pub struct PreparedConnection { machine: Machine, profile: ConnectionProfile /* private */ }
pub enum ConnectFailure { Authentication, ExpiredPairing, IdentityChanged, Incompatible, Unreachable(String) }
```
Keep serialized `{name,target,enabled}` readable. A companion profile map keyed by a namespaced `target` gives new connections their route, expected server identity and credential reference; legacy targets resolve as SSH and acquire an identity after a verified first attach. Link targets are canonical credential-free HTTPS origins, prefixed/type-tagged in profile keys; never put code/token in target or settings. Existing `Machines` remains the sole window catalog and settings writer; it derives display/status from profiles. Match/deduplicate verified EnvironmentId, permitting two distinct routes to one environment; caches become keyed by EnvironmentId rather than route target. Legacy cache keys migrate only after verification. An unexpected server id blocks before a snapshot or saved credential is used.

`Remote` remains the only owner of connection/retry/inbox state. Keep `Bridge::new(Dial)` as the private byte adapter; construction from profiles supplies SSH or WSS Dial. Replace feature-facing `Cockpit::machine()` reads with `environment_id()` and domain methods above. Route-specific SSH verification stays in setup; auth errors stop retries. Additive `DescribeEnvironment` returns id and server capabilities, required before saving. Because existing Welcome is followed immediately by Snapshot, initial attachment must quarantine that snapshot until identity verification succeeds. Local existing/in-process mode calls the same operation implementations directly; no UI route branches.

## Secure link gateway, not raw protocol on TCP
Add a separate headless `ferrited remote serve --listen … --tls-cert … --tls-key … --advertise https://host` gateway; the existing Server/Engine remains unchanged as owner of execution. Gateway alone owns credential/pairing records and exposes a separate owner-only local control socket for `ferrited remote pair` and revocation. It authenticates before opening/relaying the existing Server socket. Public listeners require TLS with a real hostname-valid certificate; an explicit loopback-only listener may sit behind an operator's TLS reverse proxy. No plaintext nonloopback option, TLS bypass, or automatic certificate trust.

The narrow HTTPS surface is `GET /.well-known/ferrite/environment` (stable id, version, features), `POST /v1/pair` (one-use code exchange), and authenticated `GET /v1/session` WebSocket upgrade. Native client uses Authorization header, never URL tokens; one binary WS frame contains one bounded NDJSON line, and the adapter reconstructs the existing byte stream. The gateway enforces size/backpressure/idle limits before relaying. TLS validates before discovery; compare saved origin+EnvironmentId before sending a stored credential, and check the authenticated session's id again. Do not redirect authenticated requests across origins.

Codes are cryptographically random, short-lived, one-use; store hashes, consume atomically, rate-limit attempts. Successful pairing issues a separately random revocable credential. Client persists it in OS credential storage (fail explicitly if unavailable); server stores only its hash with environment/creation/revocation metadata. Redact code/token Debug/log/error paths. If a pair response is lost, a fresh code is required; do not silently make the one-use code reusable. Keep code in modal memory only; full URL parsing extracts `#token` into the code field immediately and leaves a sanitized host. Credentials can be revoked through the local control socket. All v1 paired sessions grant full environment control; do not render fake restricted-permission controls.

## Attachments: upload identity is not a path
```rust
pub struct AttachmentId(String); // full content hash + validated basename, private construction
pub struct ReadyAttachment { environment: EnvironmentId, id: AttachmentId, path: PathBuf, size: u64 }
pub enum UploadState { Uploading { sent: u64, total: u64 }, Ready(ReadyAttachment), Failed(String) }
// Private wire additions, all capabilities gated:
BeginAttachment { key: TransferKey, name: String, size: u64, sha256: String }
WriteAttachment { key: TransferKey, offset: u64, bytes: Base64Bytes }
CommitAttachment { key: TransferKey }
ReadAttachment { id: AttachmentId, offset: u64, limit: u32 }
```
One upload call hides Begin/chunks/Commit, cancellation and retry. Snapshot/retain local bytes first; accept files through existing 64 MiB retention limit; reject directories/oversize explicitly. Use 256 KiB decoded chunks, bounded in-flight work and incremental hashing, never a 64 MiB base64 line. Server owns unique scratch files, exact offsets, length/hash verification and atomic immutable publication. Repeated identical chunk/commit is idempotent; mismatched repeats fail. TTL/scratch quota reclaim abandoned attempts. Basenames reject separators, `.`/`..`, NUL and platform-invalid names; upload cannot choose a server path.

Extend DraftFiles to keep source/preview and destination receipt separately. Send/bootstrap serialize the server receipt path only once Ready for the currently captured EnvironmentId/draft generation; never overwrite local preview with remote path. Same admission state gates both send paths; retry/remove/machine switch discard stale completions. Persist the receipt in draft attachment metadata. History previews fetch/cache immutable bytes through ReadAttachment; add a server-side managed-path-to-AttachmentId resolution for existing path-only prompt history, restricted to the attachment store. Retain cached local bytes by `(EnvironmentId, AttachmentId)` so identical textual paths on two servers cannot collide. Workspace references remain server paths and are never read/uploaded from desktop.

## Changes: one cohesive snapshot and one mutation
```rust
pub enum ChangesScope { Uncommitted, All, Against(String) }
pub struct ChangesSnapshot { files: Vec<FileSides>, branches: Vec<String>, staged: BTreeSet<RepoPath> }
// Public Cockpit methods return existing Receiver<io::Result<T>> asynchronous shape.
fn changes(&mut self, thread: ThreadId, scope: ChangesScope) -> io::Result<Pending<ChangesSnapshot>>;
fn set_staged(&mut self, thread: ThreadId, path: RepoPath, staged: bool) -> io::Result<Pending<ChangesSnapshot>>;
```
New explicit ChangesRead/SetStaged wire commands reuse WireReviewFile and run review_against/list_branches/index work off the Server pump. Server resolves the thread's workspace; validate relative file paths and invoke git with argument arrays and `--`, never desktop cwd or shell interpolation. Serialize Ferrite's own stage mutations per repository; external git remains possible, so do not claim a globally atomic checkout snapshot. SetStaged returns a refreshed snapshot. Scope is per request, never global PickBase. Replace local execution in both Changes and PR Code; keep render/folds/filters/review marks and stale-generation checks. Older servers produce an explicit update-required state, never a misleading local fallback or ignored compare branch.

## Real non-SSH terminal
```rust
pub struct TerminalSession { /* private command sender + bounded output receiver */ }
pub enum TerminalEvent { Output { sequence: u64, bytes: Vec<u8> }, Exited(i32), Disconnected, Reset }
impl TerminalSession {
  fn write(&self, bytes: &[u8]) -> io::Result<()>;
  fn resize(&self, size: TerminalSize) -> io::Result<()>;
  fn close(self) -> io::Result<()>;
}
// Private protocol: TerminalOpen(open_key, thread, size), TerminalInput, TerminalResize,
// TerminalAttach(terminal_id, resume_secret, cursor), TerminalClose; output/exit events.
```
Add headless server PTY ownership using a cross-platform PTY implementation; never import GPUI into core. Open resolves cwd on Server and returns opaque id + unguessable resume secret. Deduplicate Open by random open_key. Server streams bounded bytes with sequence numbers and retains a bounded replay ring; one terminal's output cannot starve command replies. On transport loss retain shell for a short reconnect lease; session reattaches with secret/cursor and either replays or emits Reset if history was truncated. Never replay uncertain keyboard input after reconnect. Close kills the shell process group/Windows job; TTL reaps abandoned terminals. Removing a connection drops its terminal leases but does not stop agents. Refactor ferrite-terminal into an emulator over TerminalSession using alacritty's parser; renderer/selection/keys stay. Local and SSH windows use this same service; delete SSH terminal_args/spawn_shell branching after migration. This is the largest unavoidable functional addition; a WSS Bridge alone cannot provide it.

## Native UI contract
Keep controller -> ferrite-ui presentation -> ferrite-frame renderer. Add native modal with Remote link default, two mode cards, Host/code or Host/Username/Port fields; reuse source-measured geometry (768 px max, 24 px padding, 96 px mode cards, 32 px submit), real focus trap/close semantics, and dedicated SSH verification dialog queue. Editable host autocomplete owns asynchronous discovery, includes SSH config Include and known-host port identities, prefix ordering, arrows/Enter/Escape/Cmd1–9/IME, and selection starts connection. Busy disables fields/cards/submit; close remains supported and generation-cancels pending UI outcomes. Preserve errors/input, reset only on close/success, and never save before verified attachment. Compact rows/empty state/header Add follow reference; route/permissions details show only implemented operations. Visual parity still requires matched viewport/theme/scale captures, not code inspection.

## Module map and ownership
`core/environment.rs` owns public identity/route/request types and onboarding; private `environment/gateway.rs` owns HTTPS/WSS and credential validation. `machine.rs` retains private SSH/Dial implementation and legacy deserialization. `cockpit/remote.rs` remains the environment session and wire translation. `server/attachments.rs`, `server/changes.rs`, `server/terminals.rs` own execution knowledge; `protocol/{attachments,changes,terminals}.rs` are private wire schemas. Existing Cockpit facade is the only feature API. UI changes stay in connection controller/presentation/renderer, DraftFiles, Changes and Terminal adapter. Keep old general Review API for its existing consumers; do not create a second Changes path.

## Red-flag screen, tradeoffs and alternative
One session/reconnect owner, one settings writer, one credential owner and one terminal implementation eliminate split ownership/two-ways drift. Wire/transport types remain inaccessible to feature callers; upload and terminal sequencing are hidden. Facade forwarding is justified only where it adapts in-process versus Server-backed execution. We accept identity/profile migration and terminal adapter work for an environment abstraction whose consumers cannot accidentally operate locally. A narrower Bridge-only alternative has lower immediate churn but leaves SSH-terminal knowledge in UI and requires every feature to remember route-specific rules; it is weaker in interface depth. Rebuilding the full T3 HTTP API was rejected: Ferrite already has a useful execution protocol and needs behavioral, not wire, parity.

## Risks and implementation order
Gateway TLS provisioning/credential-vault availability and terminal replay/teardown need real runtime proof. Are unsupported directories/over-64-MiB attachments an acceptable explicit limit? Is external TLS certificate provisioning acceptable for first remote-link hosting? These are product choices; the design defaults above are implementable without making UI fake. Candidate estimate: substantial multi-package work, approximately 7–12 focused engineer-days plus 3–6 UI/verification days, not a one-form patch. Start with stable identity, capabilities and the new three operation families; then gateway/session integration, then feature consumers/modal. Narrow filtered nextest per changed package once after final edit, one typecheck, then one actual remote E2E proving uploaded bytes, server git index, streaming terminal/reconnect and native screenshots. No tests run in this design pass.

## Synthesis decision
Candidate B only; parent chooses the base and grafts. Highest-value grafts even if this candidate loses: server-owned PTY, cohesive ChangesSnapshot, environment-scoped attachment receipts, and authenticated TLS ingress before the owner-only Server socket.
