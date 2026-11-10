# Packaged Conversation browser acceptance (#196)

Current delivery status and completed validation results are recorded in
[PR #197](https://github.com/josephwylie/ferrite/pull/197). This document describes
the bounded packaged acceptance run and the coverage that remains deferred.

The earlier narrow action gate passed on **both platforms from f6461382**:
[run42571982121](https://github.com/josephwylie/ferrite/actions/runs/42571982121).
Retained [Windows](browser-window-2026-11-09/windows-f6461382/README.md) and
[macOS](browser-window-2026-11-09/macos-f6461382/README.md) evidence shows actual
ClaudeSession → bound service → native controller → the owning Preview, with
opaque references/receipts, a blue native Window image and independent two-click
HTTP oracle. The PR's evidence section links the later integrated runs; use their
individual job outcomes for the executed scenarios below.

Run **one** `browser-window.yml` workflow at the fully integrated ref. It builds
one packaged application per supported OS and reuses that executable for:

- Existing Engine/View pixel, input, idle-frame, isolation and native controller
  checks, including a real operator HTML-select popup/option click. This explicit
  `--visual-smoke` step uses mock keychain mode; production encryption/profile
  evidence remains the separately retained native/keychain results.
- Actual operator Preview input and both actual provider adapters. Codex starts
  a fresh production Thread so dynamic tools are advertised at thread/start.
- One full work-order task per provider for correctness. Repeated performance
  blocks are deferred; this run does not establish warm latency percentiles.
  Each task contains the prescribed32 logical navigation/actions plus11 readiness
  waits, fresh observations, two saves and independent reopened-field/audit checks.
- Region and element annotation selection, all seven drawing tools, Blue ink/
  six-pixel width, text/comment, collection/review/removal and actual Composer
  batch send to both providers. Native image bytes must match retained hashes.
  Navigation and last-tab close precede Add; a new Server/window then verifies
  original history context, image bytes, review rendering and zero eager pages.
- Actual provider/manual hold/release, stale/foreign-tab references, same-label
  replacement, reload, concurrent request serialization, bounded partial-target
  recovery and an actual committed write whose reply is withheld past the Server
  deadline. The fixture independently checks no duplicate/replayed write.
  Inactive clean-page reclamation and URL-only restoration are checked afterward.
- Raw per-PID CPU time and RSS (macOS) or Working Set (Windows), with controller,
  browser, application, Server/provider and harness separation; sealed baseline,
  active work, quiet and shutdown phases; lifetime gaps and sampler cost. Native
  process-ready, attachment and command timings are separate from provider
  request/response times. These bounded samples cannot establish hours-long leak
  absence, a numerical budget or strong latency percentiles.

A separate single macOS job reuses its focused build for core/provider, native
input, editor, annotation-host and preview regressions. The primary CI's known
unrelated baseline failures remain visible; this workflow does not suppress them.
Private controller endpoints/tokens never enter provider results or diagnostics.
The optional compiler cache and exact native-stage cache do not relax checks.

After integration, the root can initiate final validation with:

```
gh workflow run browser-window.yml --ref <fully-integrated-ref>
```

The workflow has no special inputs. Individual reproduction with an existing
packaged `browser-smoke` build uses `scripts/probe-browser-window.py`:

```
python scripts/probe-browser-window.py <app> --provider claude --work-orders --tasks 1 --output <artifacts>/work-orders-claude
python scripts/probe-browser-window.py <app> --provider codex --annotations --output <artifacts>/annotations-codex
python scripts/probe-browser-window.py <app> --provider claude --boundaries --output <artifacts>/boundaries-claude
```

The work-order HTTP server is the same module used by the historical sustained
fixture. Only the page writes `/api/records`; the runner reads `/oracle` and
`/audit`. Delays remain180ms list/editor,150ms custom field and250ms save.
Persistence survives navigation, not fixture-server restart. Complete task
outcomes, failed durations, observed payload bytes and operation timings remain
in raw JSON; fast failures are never included as latency wins.

Remaining acceptance limitations must stay explicit: the new boundary scenario
uses two tabs in one Thread. It does not replace actual two-conversation/shared-
login, unavailable-owner, controller reconnect and multiwindow feature-level
runs. Prior native/profile and core/GPUI tests cover lower seams of those cases.
Real manual text/drag versus queued agent input and the complete annotation
cancel/navigation/resize/failure matrix also remain outside this bounded run.
Those deferred checks and repeated performance measurements keep #196 open;
the completed checks do not establish full release acceptance on their own.
