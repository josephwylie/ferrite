# Packaged Conversation browser window probe (#196)

The [first macOS packaged operator slice](browser-window-2026-11-09/macos-0abd8f1b/README.md)
passed rendering/input and produced the actual Claude Observe-unavailable red.
Windows window startup is under diagnosis. This is the first vertical slice of
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
MCP transport. The peer requests State and then Observe of the selected tab;
its actual response must describe the displayed `Window fixture` button.
A unique per-run marker proves the intended peer received that response.
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
Artifacts include `app.log`, diagnostics, `window-result.json`, a current
failure-diagnostic screenshot where available, before/after PNGs and the
independent `oracle.json`. Provider mode adds the actual host protocol and
peer result. Private controller credentials are not part of the peer protocol.

Initial missing-driver red run:
https://github.com/josephwylie/ferrite/actions/runs/42561582036 . The macOS
packaged probe failed as expected after 90 seconds without a scenario result;
its application and signed package built successfully. This establishes the
missing-driver red, not a passing UI/rendering outcome. The superseding driver
run is the actual two-platform GUI capability check.
Native GPUI window rendering/readback and pointer input are now proven on the
macOS hosted runner by the retained evidence. Windows exited before producing
a window result; exact exit, bootstrap/naked PE stack sizes and startup
checkpoints are collected by the next diagnostic revision. A failure-only naked
startup comparison is explicitly diagnostic and cannot pass browser acceptance.
The existing headless CEF Engine/View feasibility results do not prove this
window path. Neither this initial scenario nor a successful Observe establishes
full controller interactions, both-provider parity, annotations, Composer
history, sustained work-order performance or resource acceptance. Those extend
the same harness once their production features are available; the detailed
preparation contract remains `/tmp/ferrite-browser-196-contract.md` during work.
