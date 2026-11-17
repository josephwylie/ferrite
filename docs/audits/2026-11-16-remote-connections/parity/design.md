# Integration decision

The data model separates a connection route from authenticated server identity, a local attachment source from its remote retained reference, and a Changes request from the computer executing it.

Candidate A is the base. Keep the existing Server, Cockpit and Bridge owners. Add authenticated encrypted ingress and cohesive Server operations. Candidate B contributes backwards-compatible Machine registration and explicit environment-scoped attachment state. Do not replace the whole session/reconnect model.

The architecture review rejected per-feature SSH subprocesses for uploads and git. Both operations travel through the existing protocol on every route. Changes reads return file sides, branches and staged paths together. Its comparison base stays request-local. Stage and unstage execute on the same Server.

Attachment transfer uses bounded chunks and immutable retained content. The pane keeps its local preview source separate from the remote reference. A remote send is permitted only when current uploads are ready for the selected machine. Failed, cancelled and stale completions preserve draft text and cannot replace a different attachment.

Remote link uses a typed endpoint and separately stored credential. Bare hosts default to HTTPS. Plaintext is allowed only for explicit loopback development or a local reverse proxy. Pairing grants expire and are consumed once. The gateway verifies credentials before bridging the owner-only socket and must support revocation and bounded requests. Connection setup verifies a real protocol attachment before saving. Legacy SSH settings remain readable.

Terminal adaptation is resolved at the transport boundary. Preserve the proven SSH PTY. A link route must provide a real authenticated server PTY with input, output, resize and cleanup. An internal terminal relay process may feed the existing local terminal emulator if that keeps the public UI contract unchanged. Credentials must never enter command arguments. This is an explicit reduction of Candidate B's proposed all-route terminal rewrite, not permission for a fake or local fallback.

The Add Environment modal follows the captured T3 composition and pinned source behavior. Remote link is the default. SSH discovery is editable autocomplete, includes Include files, and selection starts setup. Auth prompts, cancellation, error retention and late results are explicit editor states. The feature must retain existing unsaved-file and settings-save guards.

## Work ownership

- Transport owns remote ingress, endpoint/credentials, bridge factory, CLI and link terminal integration.
- Operations owns attachment transfer core and Changes core/facade/protocol/UI routing.
- Attachment UI owns draft lifecycle, local previews, send admission and upload state presentation.
- Connections owns modal, autocomplete, connection settings and onboarding presentation.
- Parent owns host discovery, integration, test/build sequence, native verification and PR evidence.

Shared protocol files belong to Operations. Transport must use its gateway for terminal streams or coordinate an explicit shared-schema handoff. Source worktrees do not carry build clones. The existing warmed integration worktree owns compilation.

## Review limits

Both design candidates used OpenAI gpt-6-astra. Configured Anthropic candidates and cross-provider judge were omitted under the user's prohibition. This is independent same-model exploration. No visual equivalence is implied by this design. Only captured comparisons can establish it.
