# Remote workflow verification

## Proven behavior

Native runs use separate client and remote Stores, an actual task-owned OpenSSH server, the actual Ferrite Server and authenticated gateway, native UI events and captured frames. The final [native-7 run](native-7/summary.json) passed all eleven checks at source `545742f1`, with 49 native captures and confirmed process cleanup. It covers existing and new chat image upload, streaming tools/completion, remote file reads/saves, actual Git stage/unstage, terminal commands, reconnect and removal on both routes. SSH created thread 2 from thread 1; Remote link created thread 4 from thread 3. The new-thread checks prove distinct persisted identity, provider process, project and working directory. Parent visual review had rejected native-6's weaker new-thread assertion; the resulting production fix and stronger harness are both included. See [visual review](visual-review.md).

A separate [actual Codex smoke](real-codex/summary.json) passed using installed `codex-cli 0.169.0` and GPT-6-Astra. It launched over SSH, read an opaque token from the synthetic remote project and described an uploaded native screenshot. The [transcript](real-codex/transcript.json) records command execution and the exact response. The smoke used source `c3743ff8` and its recorded binary hash, before later gateway/UI fixes. It was not repeated as a second model call; subsequent changes are covered by deterministic checks.

## Focused checks

The passing evidence is retained with the earlier failed logs so fixes are traceable:

| Area | Passing log | Evidence |
| --- | --- | --- |
| Core remote/upload/Changes/host routing | [core-tests-4](evidence/core-tests-4.log), [ui-remaining](evidence/ui-remaining.log) | Relevant groups passed; PTY tail failure in core-tests-4 was fixed and rerun in ui-remaining |
| Changes generations and modal key routing | [ui-fixes](evidence/ui-fixes.log) | 24 scoped tests passed |
| Typed pairing URL and actual clipboard paste | [pairing-input-tests](evidence/pairing-input-tests.log) | Both input paths passed with retained server identity |
| Authenticated API routing, TLS and revocation | [link-final-tests](evidence/link-final-tests.log) | Network tests passed; its incorrect paste simulation was superseded by pairing-input-tests |
| Fragmented attachment upload | [link-upload-tests](evidence/link-upload-tests.log) | Real 786449-byte upload with 512-byte writes, matching retained bytes and reconnect |
| Compact geometry and focus | [compact-tests-5](evidence/compact-tests-5.log) | Three tests passed, including actual bounds at 640×420 and normal viewport |
| Remote draft project identity | [new-thread-tests](evidence/new-thread-tests.log) | One regression passed after removing the local header; identity comes from the live mirror |
| Terminal paste and cleanup | [terminal-final-tests](evidence/terminal-final-tests.log) | Four real PTY tests passed, including 512 KiB paste and disconnect while input stalls |
| Typecheck | [review-final-typecheck](evidence/review-final-typecheck.log) | `CARGO_INCREMENTAL=0 CARGO_HTTP_MULTIPLEXING=false cargo check -p ferrite --features headless` passed at 545742f1 |
| Build | [review-final-build](evidence/review-final-build.log) | Ferrite and ferrited native headless binaries built at 545742f1 |
| Actual model | [real-codex](evidence/real-codex.log) | Real model smoke passed; owned processes stopped |
| Compact native captures | [compact-final](compact-final/summary.json) | Four original captures; owned processes stopped |

Exact commands and outcomes are in the linked logs. Narrow nextest runs used `--lib` and the shared package feature graph (`ferrite-core`, `ferrite-pane`, `ferrite-cockpit`, `ferrite-frame`, `ferrite-core/test-support`) to avoid compiling unrelated integration binaries. Full-suite execution belongs to CI.

The final native command was:

```sh
python3 scripts/verify-remote-parity.py --output docs/audits/2026-11-16-remote-connections/parity/native-7 --size 1470x923 verify
```

[Final native log](evidence/native-7.log). Native-7 is the accepted run; earlier attempts were retained to explain the fixes and are not added together as passing coverage.

## What these checks do not establish

- The SSH fixture is localhost and shares the host filesystem. Store identity, same-ID thread collisions, local file sentinels and SSH request records assert routing. This is not a second physical PC or public cloud deployment.
- Runtime binaries were preinstalled in the task fixture. A published cold installer on a clean Linux host was not exercised.
- The native Remote link run uses explicitly allowed loopback HTTP. Production HTTPS certificate validation and authenticated WebSocket behavior have separate network tests.
- `window attach` invokes the same production Composer attachment path as the picker. It does not exercise the operating-system picker itself.
- The complete matrix uses an explicit Codex protocol fixture; no model API is called there. Actual Codex is separately evidenced over SSH. No Anthropic models were used.
- Pairing expiry is covered by access tests, not a ten-minute native wait. Native failure exercises rejected codes.
- T3 native reference chat is local, and its installed build is older than the pinned source. Pixel identity and complete T3 product parity are not established.

## Cleanup and evidence retention

Only task-owned processes are stopped, with process identity checks. Final summaries record cleanup. Synthetic diagnostic keychain entries were removed without reading their secrets. Completed source-only worker worktrees were removed; dirty interrogation worktrees were preserved. The PR worktree remains until merge. No other worktree caches or user projects were deleted.

Earlier failed-run raw files and actual-model protocol traces were moved to a local task archive to avoid duplicate evidence and unrelated raw traffic in the PR. [Archive map](evidence/failed-run-archive.json), retained failure captures, summaries and logs explain those moves. The final native screenshots remain original, with their provenance sidecars.

Independent source review found the terminal issues and reviewed their fixes plus the project identity changes. [Review report](evidence/final-independent-review.md). [Terminal backpressure policy](terminal-backpressure.md) documents the five-second sustained-input-stall limit.
