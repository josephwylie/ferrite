# Remote workflow parity completion

The user requires the remote connection and coding workflow to match T3 Code in appearance and behavior. The earlier SSH-only definition of done was too narrow.

## Acceptance

The actual app must support SSH and remote-link pairing, native modal setup with matching controls and keyboard behavior, included SSH host discovery, remote attachments, and remote Changes. Verification must exercise a remote project and thread, a message with an attachment, streamed transcript, changed-file review, files, terminal, reconnect and removal. Reference captures and Ferrite captures must be shown together. A visual difference is recorded, not described as identical.

## Throughput checkpoint

Five verified gaps cross connection settings, transport/authentication, attachment delivery and source-control operations. The riskiest unknown is remote-link ingress because Ferrite currently uses an owner-only local socket or SSH. Two architecture candidates precede implementation. The remote environment remains the owner of agents, project files and durable state.

Read-only exploration runs in parallel. Implementation partitions by module ownership after shared protocol contracts are agreed. Builds and final native verification use one existing warmed worktree to limit disk growth. Narrow tests cover each changed package, followed by one typecheck. Final integrated E2E covers the whole user flow.

## Sequence

1. Capture T3 and prior Ferrite baselines and trace ownership.
2. Compare two integration designs and record the selected data shapes.
3. Implement connection setup, remote attachments and remote Changes with isolated ownership.
4. Run narrow tests, typecheck and a complete native workflow. Inspect visual differences directly.
5. Conduct independent adversarial and visual review, correct proven issues, and update PR 310 with evidence.

## Baseline

Native T3 captures are under `t3/`. Source and installed app versions are recorded in `baseline.json`. The source remains authoritative for known fixes newer than the installed app. No zero-pixel-difference claim is supported yet.
