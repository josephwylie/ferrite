# Embedded controller feasibility gate (#192)

**Accepted on Windows x64 and macOS arm64.** [Packaged validation run
42551085327](https://github.com/josephwylie/ferrite/actions/runs/42551085327)
passed both platforms at commit `ff5a2fa13cc88a056f5bc974de1c5dd90e73b519`.
This supplies ADR0036's attachment/isolation feasibility evidence for continuing
with CEF and native agent-browser. Conversation service, preview, annotations
and sustained resource/performance acceptance continue in the dependent tickets.

## Packaged results

| Observed outcome | macOS arm64 | Windows x64 |
|---|---:|---:|
| Embedded external targets, fixed throughout comparison | 2 | 2 |
| Concurrent agent workers | 2 | 2 |
| Browser process count, single / concurrent agents | 6 / 6 | 6 / 6 |
| Controller processes serving either phase | 1 | 1 |
| OS process samples, single / concurrent phase | 45 / 92 | 5 / 10 |
| Native snapshot-reference clicks with verified pixels | 9 | 9 |
| Unauthorized controller upgrade refusals | 3 | 3 |
| Page privileged fetch/navigation refusals | 12 | 12 |
| Page attempts to open controller WebSockets refused | 2 | 2 |
| Unsafe startup configurations rejected while sealed smoke passes | 6 | 6 |

Both jobs discovered only the two registered external targets, switched and
operated them through the persistent native daemon, captured matching native
screenshots, and restarted/reconnected the controller to the same target IDs
and live page state. Rust independently verified blue/green pixels from those
exact live `View`s and an unchanged red sealed third view. Target selection and
operation were serialized, including concurrently requested work.

Process evidence comes from actual OS descendant trees (`ps` / `Win32_Process`),
with executable names and PIDs retained before controller startup, throughout
single/two-agent work and after reconnect. The two phases assert identical tab
identities. Periodic sampling is not an exhaustive trace of arbitrarily brief
OS processes.

Raw results, compiler/source provenance and complete sampled trees are retained
in [macOS evidence](embedded-browser-macos-ff5a2fa1.json) and
[Windows evidence](embedded-browser-windows-ff5a2fa1.json). The linked run also
retains the actual fixture PNGs as platform artifacts. Windows independently
verified a real renderer OS token: `restricted=True, integrity=0`, compared
with the browser process `restricted=False, integrity=12288`. macOS helpers
enter CEF's seatbelt sandbox before loading the framework; sandbox initialization
asserts success. Both platforms additionally exercised actual page attempts to
reach private files, Ferrite schemes and the controller endpoint.

## Native distribution and authorization

The #192 packaged evidence used native Rust agent-browser **0.41.2**, upstream commit
`d52a5e90c372ec4710c69adc3837ac14bbd852aa`, plus the retained
[hardening patch](../../scripts/patches/agent-browser-0.41.2-ferrite.patch), SHA256
`c9a6df0b0cf5f806cfd7eccedfe0f59b781ec02784abe042d36d427b40cc553c`.
The current retained patch also includes #194 exact-node reference handling and
page-session diagnostic buckets. Its SHA256 is
`3cdede7f2ee6b15e56f877e801d045c691c5ddf3e4564526e71cfc7165701e33`.
The original #192 hashes above remain historical evidence; they do not describe
this extension. [Native behavior run 42560579166](https://github.com/josephwylie/ferrite/actions/runs/42560579166)
validates the updated controller through its authenticated public command
transport against isolated runner-installed Chrome. This is controller evidence;
it does not establish #194 production-window/provider workflow acceptance.

The packager verifies the source commit and exact patch, builds with the
upstream lockfile, and records source/patch/compiler/platform/executable hashes
in `agent-browser-build.json`. Apache-2.0 licensing is packaged alongside it.

[Native validation run 42551085354](https://github.com/josephwylie/ferrite/actions/runs/42551085354)
passed Windows x64, macOS arm64 and macOS Intel: six focused Rust security
regressions, six public packager CLI tests, direct native invocation, persistent
service authorization/reconnect and offline artifact verification. Earlier
[raw native manifests](browser-native-validation-c9a6df0b.json) retain the original #192
hardening patch's independent all-platform validation. No Node/npm runtime,
per-action launcher or autonomous model loop is used.

The upstream daemon unconditionally exposes a stream WebSocket server and uses
unauthenticated loopback TCP for Windows commands. Ferrite mode instead requires
`FERRITE_AGENT_BROWSER=1` and a fresh `FERRITE_AGENT_BROWSER_TOKEN`; every JSON
request carries `ferriteToken`, checked before dispatch and stripped before
execution. Streaming and the inspector's alternate CDP/browser-opener path are
disabled. Implicit browser launch/recovery is rejected; explicit attachment is
required. The service uses a private temporary namespace and one persistent
daemon, never a default named session/browser per agent.

CEF's global debugging endpoint remains disabled: startup strips its debugging
port/address/pipe switches. The private authenticated WebSocket exposes only
registered external targets; sealed visuals cannot be discovered or attached.
Missing/wrong capabilities and page Origin headers receive HTTP403 or a closed
connection. The authorized native connection must subsequently complete the
entire fixture, so a dead endpoint cannot pass as successful authorization.

## Platform isolation and packaging

Windows packages matching CEF `bootstrap.exe` as `ferrite.exe`, loading
`ferrite.dll` and propagating the broker context through all CEF entry points.
A naked Cargo executable fails closed. The executable, client DLL and
`chrome_elf.dll` require matching signatures (or all unsigned for development).
PowerShell smoke checks explicitly wait for the GUI process and use its actual
exit code. The Windows terminal's Unix-only teardown code was gated so the
packaged application can compile with its native ConPTY backend.

One normalized Chromium switch parser drives both sandbox configuration and
external capability checks, including short prefixes, Windows slash/case,
values and the `--` terminator. Sandbox, web-security, site-isolation and feature
overrides make external browsing/controller access unavailable. Negative runs
then exercise the full sealed-visual pixel/input checks; intentionally unsafe
negative runs do not claim renderer sandbox proof. Normal Windows validation
always requires the actual OS-token check.

macOS places only executable code in `Contents/MacOS`; controller provenance
and licenses live in `Contents/Resources/agent-browser`. Code signing changes
executable bytes, so the retained executable digest describes the **pre-sign
source build**; the app signature authenticates delivered bundle bytes. No
runtime consumer compares that pre-sign digest with the signed executable.

CI builds native once. `--from-stage` checks source, patch, platform,
architecture and executable hash before any destination writes, then copies the
original provenance/license without invoking git or Rust. Public tests verify
this with build tools unavailable. This removed measured duplicate native LTO
of 4–5 minutes during staging. Compiler caching mirrors primary CI because
rust-cache alone excludes the vendored GPUI path dependencies.

## Reproduction

```
python3 scripts/package-agent-browser.py --destination target/browser-controller --test
python3 scripts/package-agent-browser.py --destination target/browser-controller --offline
python3 scripts/probe-agent-browser-transport.py target/browser-controller/agent-browser
```

Windows uses `python` and `agent-browser.exe`. Packaged cross-platform execution
is automated by `.github/workflows/browser-feasibility.yml`. It uses isolated
profiles and fixture pages. Native-only checks do not attach or start a browser.
The packaged fixture controls the real embedded engine and verifies live pixels.

## Recovered design evidence

[ADR0036](../adr/0036-shared-browser-with-conversation-tabs.md), the six
`browser-controller-*` research/raw-result files, the
`scripts/bench/browser-controllers/` harness and three original screenshots were
recovered from the local design worktree without modifying it. That historical
harness uses Chrome and Node for benchmark orchestration; its measurements are
not Ferrite/CEF performance guarantees. ADR0013's sealed trust boundary remains.
