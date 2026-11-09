# Embedded controller feasibility gate (#192)

The engine decision in [ADR 0036](../adr/0036-shared-browser-with-conversation-tabs.md)
remains conditional. Neither the retained Chrome benchmarks nor controller
packaging alone proves control of Ferrite's visible embedded pages.

## Recovered evidence

The accepted ADR, six `browser-controller-*` research/raw-result files,
`scripts/bench/browser-controllers/` harness and three benchmark screenshots
were recovered from the local `ferrite-wt-1` design worktree without modifying
that worktree. The original harness launches Chrome, requires Node for its
benchmark orchestration and contains machine-specific defaults. It is retained
as historical reproducible research, not shipped as the application controller
or counted as Ferrite/CEF validation. Its lockfile retains the measured versions.

## Native distribution and trust boundary

Ferrite builds the native Rust agent-browser **0.41.2** controller from upstream
commit `d52a5e90c372ec4710c69adc3837ac14bbd852aa` plus
[`agent-browser-0.41.2-ferrite.patch`](../../scripts/patches/agent-browser-0.41.2-ferrite.patch).
The packager verifies the source commit and exact patch, builds with the upstream
Cargo lockfile and records the patch and executable SHA-256 in
`agent-browser-build.json`. The package includes upstream's Apache-2.0 license. macOS keeps provenance and
licenses in `Contents/Resources/agent-browser`, with only the executable in
`Contents/MacOS`; code signing treats metadata in the latter as unsigned nested
code. `--metadata-directory` selects this layout for staging/offline verification.
Official unmodified upstream executables must not substitute for this hardened
native distribution.

The patch is necessary: upstream's native daemon unconditionally starts a
loopback streaming WebSocket server. Its origin check accepts missing Origin,
`file://` and localhost origins, and its input handler dispatches mouse and key
commands to CDP. The Windows command daemon also uses unauthenticated loopback
TCP. An authenticated CDP proxy or private Unix command socket alone would
leave these alternate input paths open.

Ferrite mode requires `FERRITE_AGENT_BROWSER=1` and a fresh random
`FERRITE_AGENT_BROWSER_TOKEN`. Every newline-delimited JSON request must carry
that capability in `ferriteToken`; it is checked before command dispatch, state
locking and idle activity. The capability is removed before action dispatch.
The unused stream listener is completely disabled, including its dynamic enable
command. Implicit browser launch and explicit launches without a CDP target are
rejected in Ferrite mode; unavailable attachment cannot open unrelated Chrome. Ferrite must launch one
persistent service in a private session directory and keep the capability out
of page content, provider tools, logs and transcripts. Only the authorized
conversation service may issue controller requests. A named agent-browser
session is not a conversation ownership boundary and must not create one daemon
or browser per agent.

Do not turn on CEF's global remote debugging port: that would expose sealed
inline visuals as well as browser tabs. The controller must attach through a
private, authenticated bridge exposing only the service-authorized live page
targets. Browser ownership and document generation remain service decisions.

The pinned-source build needs git, Rust and Python >=3.9 at **build time**. The
packaged application invokes the executable directly; no Node/npm runtime,
per-action launcher or autonomous model loop is involved. Platform code signing changes executable bytes after staging, so the recorded
executable digest describes the pre-sign build; the application signature authenticates the delivered bundle.

## Private CEF bridge scope and remaining gates

The `Engine::open_browser` seam registers external HTTP(S) views in a shared
website request context, distinct from sealed inline visuals. The on-demand
controller uses an ephemeral IPv4-loopback WebSocket requiring a random
256-bit bearer capability at the HTTP upgrade; requests carrying any Origin or
query string are rejected. The global CEF debugging switches are removed.
Synthetic discovery includes only these explicit external views. Flattened page
sessions route through each exact browser's CEF DevTools message observer.
Browser-wide target creation, application shutdown and unrelated CDP domains
are denied. The authorized local conversation service, still to be implemented
under #193, must enforce conversation ownership and serialize target selection
with its operation; the endpoint itself is not a conversation boundary.

The protocol allowlist includes the DOM, Accessibility, Runtime, Page and Input
commands used by native 0.41.2 snapshots and element-reference clicks, plus
page-scoped diagnostics. The real fixture now snapshots the Paint button and
clicks its returned element reference. It does not infer observation support
from successful JavaScript evaluation alone. Transport queues and command
counts are bounded; lost/oversized observations disconnect the controller,
and commands still pending after 25 seconds return an explicit
`completion: uncertain`, `retryable: false` error. A timeout or disconnection
after dispatch never establishes that an action failed to occur.

Cross-origin out-of-process iframe targets are **not implemented** in this
initial bridge: `Target.setAutoAttach` is denied rather than exposing arbitrary
CEF targets. Native 0.41.2 treats that optional enable failure as nonfatal, so
simple top-level workflows remain testable, but successful top-level fixture
runs cannot establish iframe workflow support. Before dependent delivery,
review and implement scoped child-target routing and verify a real cross-origin
iframe fixture through the same Engine/View boundary; do not weaken site
isolation or silently claim full interactive-web coverage. The feasibility
viewport is currently fixed at 600 logical pixels high; the later browser
service must supply full viewport sizing and document-generation policy.

The local CEF-enabled `cargo check` passed on macOS. Local packaged runtime
validation was stopped for disk pressure before a complete new bundle existed;
only CI packaged runs may establish the outstanding Windows/macOS runtime
acceptance. No performance claim follows from compilation or native debug
transport results.

## Windows sandbox layout

CEF's matching `bootstrap.exe` is packaged as `ferrite.exe` and loads
`ferrite.dll`. Its broker context reaches all three CEF execute/initialize calls;
a naked Cargo executable fails closed before initializing CEF. Signing must
cover the executable, application DLL and `chrome_elf.dll` with one identity.
The stage test checks file layout; it does not prove effective renderer token
restrictions. Packaged runtime isolation, including external-page attempts to
reach application operations, filesystem tools and sealed visuals, remains
required on both platforms.

## Local native validation (2026-11-09)

On macOS 27.2/arm64, the retained patch's five Ferrite-specific Rust tests and
all nineteen upstream native-daemon tests passed. The final native debug
executable passed `probe-agent-browser-transport.py`: fail-closed startup,
missing/wrong credentials on observation/close/internal shutdown, two authorized
requests over separate connections to the same daemon, disabled stream startup
and reactivation, and explicit no-CDP launch rejection. These are real native
transport results, not CEF or Windows evidence. The native executable and provenance are retained separately from disposable
compilation caches. Local release compilation was stopped when the operator
reported low disk space; packaged release validation continues on CI.
The patch SHA-256 is
`034e4532e404a1c5e78102b219001c8a473f659a8111a089d1cdbda753c3f2ad`.

The combined packaged smoke driver `probe-embedded-browser.py` additionally
requires the private bridge supplied by the Engine fixture. It uses native
clicks in two concurrently requested, serialized tabs, independently decodes
controller screenshots and restarts the controller before rediscovering the
same targets and page state. The Rust host separately verifies the actual live
View pixels and that its sealed third page was not changed. After capturing those screenshots, each external page attempts both fetch and
full-page navigation to an existing private filesystem sentinel, the sealed
visual scheme and Ferrite's application scheme; all must be refused while its
original HTTP location and painted state remain intact. Page JavaScript also
attempts the private controller WebSocket and must receive a connection error.
The workflow
retains fixture screenshots and the native result manifest. Adding this driver
does not claim that the packaged cross-platform run has passed.

## Packaged native CI validation

[Run 42540596906](https://github.com/josephwylie/ferrite/actions/runs/42540596906)
passed on hosted Windows x64, macOS arm64 and macOS Intel. Every job built the
pinned patched source in release mode, invoked the executable directly, passed
the isolated persistent transport probe and reverified the packaged executable's
SHA-256/provenance offline. [Retained raw manifests](browser-native-validation.json)
include each executable digest, Rust compiler version and transport result.
No browser was attached in these native-only jobs; this evidence does not close
the separate packaged CEF attachment or effective-isolation gates.

## Additional control-surface audit

A subsequent audit found `inspect` can start a second unauthenticated CDP proxy
and open the OS browser. The retained patch now rejects both `inspect` and
`stream_enable` before action dispatch or browser diagnostics, and guards their
direct handlers. The additional focused regression checks that rejection
occurs without side effects even before a browser is attached. The public
transport probe now also requires an explicit inspect-denied error.

The current patch SHA-256 is
`c9a6df0b0cf5f806cfd7eccedfe0f59b781ec02784abe042d36d427b40cc553c`.
The original local and CI evidence above applies to the previous `034e4532`
patch. The subsequent c9a validation below covers the inspector regression. Native CI uses the packager's
`--test` option to execute all six focused Rust security regressions on every
platform before packaging. Local Rust builds remain stopped for disk space.

No other native action creates a listener or opens an external browser:
recording uses existing CDP screencast and ffmpeg pipes; tracing/profiling use
existing CDP IO streams. Standalone dashboard startup is outside daemon mode;
the service's allowlisted environment selects daemon mode exclusively. Browser
launch/provider/Safari/Appium paths remain behind the explicit-CDP requirement.

## Final native hardening validation

[Run 42542183864](https://github.com/josephwylie/ferrite/actions/runs/42542183864)
passed on Windows x64, macOS arm64 and macOS Intel for patch `c9a6df0b`.
Each job passed all **six focused Rust security regressions**, built the native
release executable, rejected inspector/stream activation and missing/wrong
capabilities through the real persistent transport, and verified its packaged
hash offline. [Raw build and transport results](browser-native-validation-c9a6df0b.json)
retain compiler versions, executable hashes and completed job IDs. These results
prove the current native hardening on all supported platforms; the packaged
CEF bridge, OS isolation and unsafe-startup negative smokes remain separate.

## Unsafe Chromium startup overrides

External browsing and its controller are unavailable when startup switches
include `no-sandbox` or `single-process`, with or without `=value`. Chromium's
switch presence semantics apply even to `=0` or `=false`; Windows also accepts
case-insensitive slash prefixes. Matching honors Chromium's `--` argument
terminator and platform switch prefixes. The existing environment escape hatch
still allows sealed visuals, with general browsing unavailable.

Packaged negative smoke cases use `FERRITE_BROWSER_UNSAFE_SMOKE=1` with plain,
valued and platform-prefixed no-sandbox switches. They require external-view
creation and controller access to reject the unsafe configuration, then run the
ordinary sealed-visual pixel/input/idle checks. The intentionally unsandboxed
negative runs do not claim renderer sandbox proof; the normal packaged smoke
still requires the OS sandbox checks. These additions require CI validation.

## Review verification updates

One normalized Chromium switch parser now feeds both CEF sandbox settings and
the external capability gate. Short prefixes, Windows slash/case variants and
valued switches cannot bypass the web-security, site-isolation or feature
override checks. Packaged negative smokes cover those families as well as the
sandbox override while retaining sealed rendering/input tests.

The Python tests exercise the **shipping packager** through its public offline
CLI: tampered executable bytes, wrong source provenance, missing offline data,
and the separate macOS resource metadata directory. The unused official-release
verifier was removed.

Process-count evidence comes from actual OS descendant trees (`ps` on macOS,
`Win32_Process` on Windows), including executable names and PIDs. The fixture
retains samples before controller startup, throughout single-agent work and
throughout two concurrent workers, then after controller reconnect. Each active
phase takes at least three samples while native page work continues. Both
phases assert the same two target IDs; adding tabs cannot explain a process
count difference. The recorded controller/browser counts are computed from
these samples and the concurrent-agent phase must not increase browser process
count over the single-agent baseline. Periodic sampling is not an exhaustive
trace of arbitrarily short-lived OS processes. These updated fixture checks
remain pending packaged CI execution.

## Reproduction and acceptance status

```
python3 scripts/package-agent-browser.py --destination target/browser-controller
python3 scripts/package-agent-browser.py --destination target/browser-controller --offline
python3 scripts/probe-agent-browser-transport.py target/browser-controller/agent-browser
```

On Windows, use `python` and `agent-browser.exe`. The probe creates isolated
state, never attaches to or starts a browser, rejects missing/wrong credentials,
checks reconnect to one persistent native service and verifies the stream
listener is disabled. It cannot prove embedded-page attachment, page pixels,
concurrent target routing, browser process counts, renderer isolation or browser
resource performance. The independent Native browser controller and Packaged
browser feasibility workflows run without depending on core tests. GitHub has
hosted Windows runners; the repository reported zero self-hosted runners during
investigation. A macOS host is not a reason to waive Windows acceptance.

Do not mark #192 complete until packaged Windows and macOS evidence demonstrates
exact target discovery/attachment, concurrent routing/reconnect, same live page
screenshots/actions, effective isolation and process-count behavior. A failing
gate keeps external browsing unavailable; it requires an explicit engine/design
review rather than substituting an external browser.

Primary source references (all pinned to 0.41.2):

- [Native daemon transport](https://github.com/vercel-labs/agent-browser/blob/v0.41.2/cli/src/native/daemon.rs)
- [Stream listener and origin rules](https://github.com/vercel-labs/agent-browser/blob/v0.41.2/cli/src/native/stream/mod.rs)
- [WebSocket input dispatch](https://github.com/vercel-labs/agent-browser/blob/v0.41.2/cli/src/native/stream/websocket.rs)
- [Authenticated CDP discovery headers](https://github.com/vercel-labs/agent-browser/blob/v0.41.2/cli/src/native/cdp/discovery.rs)
