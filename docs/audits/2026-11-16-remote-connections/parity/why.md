# Why this architecture

The requested experience is one client operating agents on another computer. The remote computer must own the project, provider process, transcript, attachments, Git index, and terminal. A connection is a route to that owner; it is not a second local agent implementation.

Ferrite already separates its native client from the Server through the Cockpit protocol. Extending that boundary keeps existing chat and transcript behavior available remotely. SSH transports the existing protocol. Remote link authenticates an encrypted gateway before forwarding that same protocol to the owner-only Server socket. Neither route substitutes local file or Git operations when the connection is unavailable.

An attachment needs two identities: its local preview source and the immutable remote file actually submitted to the provider. Bounded, hashed upload receipts connect those identities without pretending that a local pathname exists on the remote computer. Changes similarly needs a single server-owned snapshot and mutation path so the displayed diff and staged index refer to the same checkout.

Local availability is independent of remote registration. Explicitly representing no selected environment prevents the old Local default from becoming a hidden fallback when Local is disabled. Disabling a client connection does not stop running remote agents.

The T3 reference determines the connection interaction: default Remote link mode, SSH mode, editable host discovery, selection that starts connection, progress, authentication, validation, cancellation, and environment controls. Existing Ferrite ownership and persistence determine how that interaction is implemented safely. Visual similarity remains a capture-based acceptance check, not a consequence of choosing this architecture.

The independent design candidates and their limits are recorded in `design-a.md`, `design-b.md`, and `design.md`. Runtime evidence and the final visual verdict must be read separately from this rationale.
