# Remote connections

The user wants to connect Ferrite to a cloud server or another PC and run coding agents there, with T3 Code's SSH setup workflow and polished native settings. The deliverable is a reviewable PR with visual evidence. Do not merge or deploy.

## Completion predicate

Settings exposes Connections. A user can add an SSH host or alias with optional username and port, see progress and recover from errors, connect to a real Ferrite Server, switch to its threads, disconnect, reconnect, and remove the saved connection. Setup installs or reuses the remote server. The Server owns agents, filesystem operations, settings, and durable state. User terminals run on the selected machine through SSH. Every changed UI state has a reviewed native screenshot. Narrow relevant tests pass once after the final code edit, followed by one typecheck and one end-to-end flow run.

## Grounding

- Base Ferrite commit is `f7884cff`. Worktree is `../ferrite-remote-connections`, branch `feat/remote-connections`. The warm worktree hook cloned the build. The original checkout contains unrelated user changes.
- Reference T3 source is `/tmp/ferrite-t3-reference`, commit `4da2bc520c38cd6b36014d2888774168fbf4c77f`, from `pingdotgg/t3code`.
- Core `Machine { name, target, enabled }` and `Bridge` already attach through `ssh ferrited bridge`. `Server` owns remote execution. Window-local `Machines` owns selection and off-screen status. Target is the connection key.
- Current Settings hides machines in About, saves an untested target immediately, clears invalid input, and offers Sign in even when ferrited is missing.
- ADR 0026 delegates credentials to OpenSSH and keeps the server alive independently of the window. ADR 0025 and PR 286 separate local window preferences from server-owned defaults.
- T3 Add environment SSH uses host/alias autocomplete, optional Username and Port. Keys come from SSH config and agent. Inputs disable while Adding, failures remain inline, success saves and resets.
- Ferrite currently has no downloadable headless Linux server release. An automatic installer must add real artifacts or honestly use source installation. Do not invent existing URLs.

## Delivery workflow

1. Trace execution and history, record how and why, and compare T3 source.
2. Compare two independent architecture sketches before implementing. Review typed state, transport ownership, credential handling, lifecycle, native UI fit, and verification cost.
3. Implement server setup and connection lifecycle behind core types. Implement the native Connections page and connect-before-save form against that boundary.
4. Review the diff independently. Run the narrow tests, typecheck, native screenshot matrix, and the end-to-end flow. Record limits explicitly.
5. Commit the decision trail and visual evidence, push the branch, and open a PR for review.

## Throughput checkpoint

Parallel work follows separate ownership. Architecture exploration is read-only. Core setup and settings UI may be implemented independently after the contract is chosen. Only one build/test process runs at a time. Disk starts at about 15 GiB free. Use plain cargo and the cloned target. Do not build T3 or download its dependencies.

## Review constraints

Model the Domain requires typed setup progress and connection state rather than coordinated booleans. Foundational Thinking keeps the existing Server as the execution owner. Laziness Protocol favors extending the SSH boundary instead of introducing a second execution stack. Experience First makes failures recoverable in Settings. Prove It Works requires native renders and actual protocol attachment.

The user excludes Anthropic models. Configured Claude seats are omitted. Independent OpenAI attempts provide review, with reduced provider diversity. No other-provider cross-judge runs.

External evidence coverage has source control and ChatGPT Space for long-form documents. No matching issue tracker, team chat, infrastructure observability, error tracking, or product analytics MCP is available. Those are evidence gaps.
