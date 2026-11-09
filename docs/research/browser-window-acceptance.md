# Packaged Conversation browser window probe (#196)

The [first macOS packaged operator slice](browser-window-2026-11-09/macos-0abd8f1b/README.md)
passed rendering/input and produced the actual Claude Observe-unavailable red.
The [Windows operator slice](browser-window-2026-11-09/windows-9080268e/README.md)
now also passes after increasing both executable stack reserves to 8 MiB. Its
actual provider Observe reaches a new attachment failure: the native controller
attempts `Target.createTarget`, which the embedded endpoint denies. This is the first vertical slice of
#196, not complete browser/annotation acceptance.

The opt-in `browser-smoke` application feature installs a bounded scenario in
Ferrite's ordinary native GPUI window. The window still uses production
Machines, the actual Server and socket protocol, CockpitView, Preview and CEF.
It imports one idle fixture Thread, routes a loopback URL through
`Preview::open_url`, renders the complete native window, finds a known red
fixture button in those pixels and dispatches a pointer click through the
window's normal GPUI input path. The next window screenshot must contain the
green button, and an independent local HTTP server must have received exactly
one click. This does not test OS-level mouse injection.

`--provider claude` additionally starts a deterministic external CLI peer through
normal production discovery and Session launch. A temporary PATH shim calls
`scripts/browser-window-provider.py`, which implements the captured Claude SDK
MCP transport. The peer checks the actual advertised tool definitions, requests
State and Observe of the selected tab, then uses the returned exact element
reference and observation receipt to click `Window fixture`. A fresh Observe
must still describe that fixture. The same native Window must paint the button
blue, and the independent HTTP oracle must count exactly two clicks: operator
then provider. A successful tool acknowledgement alone cannot satisfy the probe.
Fresh Observe polling is bounded when the initial operator gesture has not yet
settled; it never retries a write. A unique per-run marker proves the intended
peer received the response.
It never fabricates a browser response, edits fixture data or calls a model.
An unavailable response is a failure. Codex and broader provider scenarios are
subsequent slices; the peer's Codex transport is not yet exercised by this probe.

Run `.github/workflows/browser-window.yml` for staged Windows x64 and signed
macOS arm64 builds. It is independent of the pre-existing primary-CI core test
failures. The installer accepts `FERRITE_BUILD_FEATURES=browser-smoke` to append
the opt-in feature while retaining CEF; ordinary packaging is unchanged.

With an existing packaged smoke build, the narrow reproduction is:

```
python scripts/probe-browser-window.py <packaged-ferrite> --output <artifact-directory>
python scripts/probe-browser-window.py <packaged-ferrite> --provider claude --output <artifact-directory>/claude
```

The probe creates a temporary Store, Settings and browser profile and disables
Projects, titling and automatic update work. The actual Server has its own
process handle and is stopped/reaped before cleanup. The normal native window
must close within the bounded scenario; a hung or unsupported renderer fails.
Artifacts include executable SHA-256 and platform provenance, `app.log`,
diagnostics, `window-result.json`, a current failure screenshot where available,
before/after PNGs and the independent `oracle.json`. Provider mode adds the
actual host protocol, tool definitions, operation trace, peer result and blue
Preview PNG. After ordinary app exit, an OS process check rejects surviving
native controllers before cleanup. The public Stop request then stops the
isolated Server. Private controller credentials are not part of the peer
protocol. Existing completion markers are removed before a new invocation.

The workflow reuses only an exact unsigned native stage keyed by OS,
architecture, compiler and packager/patch hashes. The packager strictly checks
provenance and the binary hash before reuse. Native-stage caching and optional
compiler-cache fallback do not relax application builds or acceptance checks.

The historical work-order benchmark and upcoming packaged scenario share
`scripts/bench/browser-controllers/fixture-server.mjs`. Run it with
`node scripts/bench/browser-controllers/fixture-server.mjs`; stdout reports its
loopback URL. This starts only the HTTP fixture. GET `/oracle?name=...` reads
persisted records and GET `/audit` reads successful save events; neither writes
data. List/editor, custom-field and save delays remain 180, 150 and 250 ms.
Persistence survives navigation, not a fixture-server restart. This extracted
fixture is not yet exercised by the packaged provider scenario.

Initial missing-driver red run:
https://github.com/josephwylie/ferrite/actions/runs/42561582036 . The macOS
packaged probe failed as expected after 90 seconds without a scenario result;
its application and signed package built successfully. This establishes the
missing-driver red, not a passing UI/rendering outcome. The superseding driver
run is the actual two-platform GUI capability check.
Native GPUI window rendering/readback and pointer input are now proven on the
macOS hosted runner by the retained evidence. Windows reached the actual production CockpitView/native Window before
exiting with `0xC00000FD`; both packaged and naked executables reserved 1 MiB.
The naked log explicitly reports a main-thread stack overflow. The next packaged run verified both reserves at 8 MiB and passed the actual
Windows operator Preview/render/input scenario. Its subsequent actual provider
Observe failed at native attachment; the endpoint denied `Target.createTarget`. A failure-only naked
startup comparison is explicitly diagnostic and cannot pass browser acceptance.
The existing headless CEF Engine/View feasibility results do not prove this
window path. Neither the initial operator slice nor a successful single-button provider
scenario establishes full controller interactions, both-provider parity, annotations, Composer
history, sustained work-order performance or resource acceptance. Those extend
the same harness once their production features are available; the detailed
preparation contract remains `/tmp/ferrite-browser-196-contract.md` during work.
