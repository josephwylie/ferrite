# Connection setup design

Two independent OpenAI architecture reviewers compared complete alternatives before the implementation contract was finalized. Both kept the existing Server and SSH execution boundary. No Anthropic model was used. The configured cross-provider judge was omitted because the user excluded that provider.

## Alternatives

| Criterion | Explicit setup attempt | Install during every bridge dial |
| --- | --- | --- |
| User owns installation and cancellation | Direct | Requires routing through background attachments |
| Progress and failure ownership | One attempt | Existing shared bridge failure slot needs restructuring |
| Ordinary reconnect behavior | Reuses installed server | Can download software after upgrade or removal |
| Existing protocol and API behavior | Preserved | Must keep API-only dial separate |
| Existing target identity | Preserved | Preserved |

Choose the explicit setup attempt. Candidate B's reviewer also recommends this boundary if lazy installation requires a new local framing protocol. It does here because the current bridge does not route progress to individual attachments.

## Contract

The caller validates the SSH host, optional username, and optional port into a `Machine`. It starts one `setup::Attempt`, polls typed progress and native authentication questions, and saves only its verified success. Dropping the attempt cancels its owned SSH process. Errors leave the form intact.

The attempt runs installation and the real Hello/Welcome handshake through one owned SSH child. It does not create a temporary bridge listener. Normal execution continues through the existing Bridge and Server. This adapts candidate A's ready-bridge handoff to avoid a second listener and adoption protocol.

Keep the persisted `Machine` shape. A new explicitly supplied port becomes part of a canonical SSH URI in `target`, so two endpoints cannot accidentally share a connection or SSH master. Blank fields retain SSH config behavior. This adopts candidate B's endpoint representation instead of adding independently mutable username and port fields.

OpenSSH owns authentication and keys. A private owner-only askpass socket carries an in-memory question to the native form. A response is consumed once, expires, and is never persisted. Host verification requires an explicit answer.

Bootstrap installs a verified release archive atomically into a versioned runtime directory. Reuse a prepared PATH server and verify its actual protocol connection. An older compatible running Server produces a warning. Installing a binary does not replace the live Server or end its agents.

Disconnect and Remove close this client's connection. The remote Server remains the owner of its agents and state. Removing the selected saved connection returns the window to this computer rather than retaining an unsaved copy.

## Release and verification limits

New public archives become available only when a tag runs the release workflow. The PR can verify archive packaging and installer behavior with controlled fixtures before then. A real localhost SSH daemon verifies authentication, protocol attachment, remote ownership, and disconnect behavior. It does not prove an arbitrary cloud host's libc or service configuration.

The production native headless window supplies screenshots and click/key verification. Its driver now follows the current Cockpit after a machine switch and answers an in-flight click on the Cockpit that received it.

## Remote tool ownership

Review found that the existing Files editor and Terminal still interpreted remote paths locally. This must be corrected before advertising cloud connections. A disabled panel would avoid the wrong operation but would leave the expected coding workflow incomplete.

For Files, compare extending separate directory/read/write commands with one typed file-operation boundary. Choose the latter. `FileOperation` carries bounded snapshot, read, and conflict-aware write requests to the existing Server. A UI-free core module owns canonical root containment, SHA256 content fingerprints, and atomic replacement. Local and remote saves use the same content and replacement rules. The native tree consumes asynchronous remote snapshots; editors use asynchronous server reads and saves. Restored tabs, refresh, filter, reload, overwrite, and quit handling must use the same remote handle. Images and external-app actions must never interpret a remote path locally.

For Terminal, compare exposing renderer-specific process options with a small explicit command API. Choose the explicit API. Core Bridge constructs SSH arguments using the existing target, control socket, and safely quoted remote working directory. The terminal crate starts that command in its existing PTY without exposing renderer internals. A nonzero remote SSH exit remains visible with Retry. The existing local shell behavior stays intact.

Windows OpenSSH cannot share an interactive authentication master. Setup must prove a subsequent batch connection before it saves an environment on Windows. A password-only login must produce SSH-key or agent guidance rather than a misleading success.
