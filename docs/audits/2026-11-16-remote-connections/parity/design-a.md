# Candidate A: authenticated ingress into the existing Server
Grounded at Ferrite `7eaf7f8d`, T3 `4da2bc5`, the existing architecture audit and `/tmp/ferrite-parity-how-{core,ui}.md`. Design only: no repository edits, builds or tests.

## Problem and boundary
Complete the five confirmed gaps: native setup modal, SSH discovery, Remote-link pairing, remote attachments and Changes. Remote-link terminal is mandatory. Preserve Server/Engine/Store ownership, owner-only local sockets, existing SSH setup and agents surviving window closure. No relay, cloud discovery, load balancing or route-management product expansion.

## Usage first
```rust
// Modal owns cancellation and preserves fields/errors until success.
let attempt = connections.connect(ConnectRequest::Link(PairingFields::parse(host, code)?));
// AttemptEvent::{Progress, SshChallenge, Ready, Failed}
let established = attempt.ready()?; // Includes authenticated Welcome identity check.
settings.save(established.machine())?;

let upload = cockpit.prepare_attachment(LocalAttachment::snapshot(path)?);
composer.accept_upload(upload.result()?, captured_environment, captured_draft_revision);
let read = cockpit.changes(ChangesQuery { workspace, scope, base });
changes.accept(read.await?, captured_environment, captured_generation);
cockpit.change_index(IndexChange { workspace, path, stage, expected_index });
let session = cockpit.open_terminal(TerminalRequest { workspace, size })?;
let terminal = Terminal::from_session(session, scrollback);
```
Operator workflow: `ferrited remote enable --listen … --cert … --key … --public-url https://…`, then `ferrited auth pairing create`; output is `https://host/pair#token=…`. `auth sessions list/revoke` manages issued sessions. These are proposed CLI commands.

## Minimal shared contracts
```rust
pub struct MachineId(Uuid);                   // Stable local registration/cache identity.
pub struct ServerId([u8; 32]);                // Random durable identity per Server store.
pub struct CredentialRef(String);            // OS credential-store key; never the secret.
pub struct Machine { id: MachineId, server: Option<ServerId>, name: String, enabled: bool, address: Address }
pub enum Address { Ssh(SshTarget), Link { origin: HttpsOrigin, credential: CredentialRef } }
pub enum ConnectRequest { Ssh(SshTarget), Link(PairingFields) }
pub struct EstablishedConnection { /* verified Machine and capabilities */ }
impl Connections {
    pub fn connect(&self, request: ConnectRequest) -> ConnectionAttempt { todo!() }
    pub fn bridge(&self, machine: &Machine) -> Result<Bridge, ConnectError> { todo!() }
}
```
Read legacy `{name,target,enabled}` as SSH; persist generated stable MachineId on the next successful settings save. Keep one route per registration for this scope. Reject a second verified registration of the same ServerId with a clear existing-environment result. Do not implement routing/failover. Replace `MachineKey(target)` with stable MachineId; captured asynchronous operations also check ServerId. Preserve view recreation when switching machines. Existing SSH targets remain supported.
`Connections` hides parsing, credentials, pairing/SSH preparation and identity verification. WSS returns the existing `Dial -> Tunnel {from,to,ended}` to `Bridge::new`; Cockpit/link clients remain unchanged. Link failures distinguish expired/revoked authorization, identity mismatch, TLS, unreachable and incompatible server. Re-pair repairs authorization; SSH sign-in remains SSH-only.

## Ingress and auth ownership
```rust
// server/remote.rs: HTTPS/WSS framing, private library types.
pub fn start_remote(config: RemoteListen, server: ServerIngress) -> io::Result<RemoteGuard> { todo!() }
// server/access.rs: owned by the existing Server pump, not listener threads.
enum Principal { LocalOwner, Paired(SessionId) }
struct Access { identity: ServerId, grants: GrantStore, sessions: SessionStore }
impl Access {
    fn issue_pairing(&mut self, owner: LocalOwner, ttl: Duration) -> PairingLink { todo!() }
    fn exchange(&mut self, code: Secret, label: ClientLabel) -> Result<IssuedSession, AuthError> { todo!() }
    fn authorize(&self, bearer: Secret) -> Result<SessionId, AuthError> { todo!() }
    fn revoke(&mut self, owner: LocalOwner, session: SessionId) -> Result<(), AuthError> { todo!() }
}
```
Endpoints: metadata-only `GET /.well-known/ferrite/environment`, bounded `POST /v1/pair`, then Authorization-header-authenticated `GET /v1/link` upgrade. Binary WSS chunks carry the existing newline JSON stream. No credentials in query parameters, cookies, logs or display targets. Pair URLs use fragments. Reject unexpected Origin headers; no wildcard CORS. The authorized stream enters `link::Connection` and the same Server pump, with a trusted non-wire Principal. Never expose raw JSON TCP or forward unauthenticated bytes to server.sock.
Use maintained TLS/HTTP/WebSocket libraries. Nonloopback ingress requires TLS; client validates hostname/certificate with no insecure toggle. Plain HTTP is limited to explicitly configured loopback TLS-proxy/test use, never automatic downgrade. Bare host defaults HTTPS. Bound requests, handshake time, connections and queues; retain existing line/lag limits. Rate-limit pairing per source and globally.
Pair grants have >=128-bit CSPRNG secrets, ten-minute default expiry, hash-only persistence and atomic one-use consumption with session creation. Sessions have >=256-bit secrets, bounded lifetime, hash-only host persistence, constant-time comparison and explicit revocation. Persist before replying. Lost exchange response requires a fresh code; never reuse consumed grants. Native client stores reconnect credentials in OS secure storage, with no plaintext-settings fallback.
Server owns authorization and checks expiry/revocation before dispatching every queued request. Revocation closes related sockets and PTYs and fences queued operations; already-running operations may finish. CLI auth administration is local-owner-only. Listener threads ask the Server pump to exchange/authorize; they do not write duplicate auth caches.
Welcome adds optional ServerId plus explicit capabilities with serde defaults. Pairing requires identity-bearing Welcome matching descriptor/exchange before saving. SSH learns identity through authenticated SSH; old SSH servers remain supported. Changed identity stops reconnect. ServerId is bound to validated TLS/authenticated SSH, not a substitute for TLS verification. Preserve identity on ordinary restart; intentional store clones need fresh identity.

## Attachments: one completion boundary
```rust
pub struct LocalAttachment { /* immutable local snapshot, basename, digest, size */ }
pub struct PreparedAttachment { environment: ServerId, remote: AttachmentRef, local_preview: Option<PathBuf> }
pub enum TransferState { Queued, Uploading { sent: u64, total: u64 }, Ready(PreparedAttachment), Failed(TransferError) }
// Public: Cockpit::prepare_attachment(LocalAttachment) -> AttachmentJob.
// Private wire: AttachmentBegin -> existing ref or upload id/offset;
// AttachmentChunk(id,offset,bytes) -> offset; AttachmentFinish -> ref;
// AttachmentAbort (idempotent); AttachmentRead(ref,offset,max_bytes) -> bounded bytes.
```
Use <=256 KiB decoded chunks, existing 64 MiB retention cap, verified digest/length, validated basename and per-upload scratch files with atomic immutable publication. Same offset/content retries are idempotent; mismatches fail. Session-owned uploads expire and have concurrency/disk quotas. No arbitrary path writes. Directories fail explicitly. Local files upload; Workspace references remain remote and are never read on the desktop.
Keep local preview/source separately from remote reference. Server resolves AttachmentRef to its retained path before existing provider prompt processing. Persist reference metadata for drafts/history, and use authenticated bounded AttachmentRead for restored previews/cache keyed by ServerId+digest. Send/bootstrap share one admission gate; pending/failed files block send while preserving text/chips. Completion must match environment, draft generation and attachment id; cancel/remove/switch discards stale results. Do not simply remove NotOnMachine or replace every local path with a remote path.

## Changes: cohesive server read and typed mutation
```rust
pub struct ChangesQuery { workspace: WorkspaceRef, scope: ChangeScope, base: Option<BranchName> }
pub struct ChangesSnapshot { files: Vec<FileSides>, branches: Vec<BranchName>, staged: BTreeSet<RepoRelativePath>, index: IndexRevision }
pub struct IndexChange { workspace: WorkspaceRef, path: RepoRelativePath, stage: bool, expected_index: IndexRevision }
// Cockpit::{changes,change_index} -> Reply<ChangesSnapshot>.
// Private Op::{ChangesRead,ChangesIndex}; reuse WireReviewFile and review_against.
```
Server gathers branches/staged paths and review in the actual checkout. Serialize index mutations per checkout; compare expected revision and report conflicts. Validate repo-relative paths and execute git with argument boundaries. Keep base per request; do not use global PickBase. Changes and PR Code remove their local git/read bypass and share the facade. Preserve generation checks. Capability-gate new operations; old servers request upgrade rather than silently use local git/wrong base. Existing ReviewRead remains the legacy Review API sharing underlying review logic.

## Real remote PTY
```rust
pub trait TerminalSession: Send {
    fn input(&self, bytes: &[u8]) -> io::Result<()>;
    fn resize(&self, size: TerminalSize) -> io::Result<()>;
    fn take_output(&mut self) -> Receiver<TerminalOutput>;
    fn close(&self); // Idempotent; Drop closes.
}
// Cockpit::open_terminal(request) -> Reply<Box<dyn TerminalSession>>.
// Wire: TerminalOpen/Input/Resize/Close and directed Output/Exited events.
```
Server spawns a genuine headless PTY in verified remote workspace. Bound output/input/dimensions; terminal ids are owner-checked. Disconnect/close/revocation kills only those PTYs/process groups, never agents. Reconnect restores environment and offers terminal retry; persistent PTY resume is not claimed. Updated SSH and WSS use this operation. Add session backend seam in ferrite-terminal at its hardcoded EventLoopSender/tty::Pty boundary, preserving Term/grid/parser/rendering. Shared session contract belongs in UI-free core. No remote-to-local shell fallback; SSH legacy capability fallback can remain for old servers only.

## Native UI and implementation partition
Controller owns modal session/generation, mode, fields, discovery/highlight, attempt and error. Existing dialog mechanics supply focus, Escape/backdrop and cancellation; dedicated typed frame renderer matches T3 geometry (768 px dialog, two 96 px mode cards, Remote Host/Code, SSH Host then User/Port, full-width Add). Popup consumes first Escape, selection resolves and immediately connects, manual host remains freeform; Close cancels and stale completion cannot save. Reopen defaults Remote link. Busy disables fields/cards; SSH challenge uses separate modal. Source UI grounding supplies exact dimensions; installed baseline is already captured.
Agree machine/protocol/capability contracts first. Own `machine/connections+link` and `server/access+remote`; attachment domain/server + pane admission; changes domain/server + Changes panel; terminal core/session; modal controller/UI frame as independent partitions. Shared protocol/dispatch edits need one owner.

## Alternatives, red flags and decision
Standalone gateway loses: auth revocation cannot fence commands already forwarded and ownership splits. Replacing all link clients with HTTP loses: broad churn without extra capability. SCP/SSH git loses: per-transport operation paths leave Remote link incomplete. Raw TCP+Hello token loses: weak encryption/auth boundary. This candidate keeps Bridge and Server while hiding upload stages/auth framing behind whole operations. Single auth owner, explicit path provenance and captured identities address split ownership/stale state. No transport types in pane APIs. Synthesis choice belongs to parent.
Tradeoffs: add TLS/auth dependencies; support legacy SSH through capabilities; bounded uploads preserve current size limit; terminal ends on connection teardown; credential store is required. Remaining decisions: TLS termination deployment and whether PTY resume is required. Do not claim cloud/relay/permission parity.
Next: shared contracts, then TLS fixture proving pair/attach/reconnect/revoke, then modal integration. After implementation use narrow `cargo nextest run -p <crate> <filter>` once green then one applicable cargo check; integrated E2E once covers remote message+attachment, stream, Changes/staging, Files, PTY, reconnect/removal. Capture matching T3/Ferrite logical sizes and report visual deltas.
