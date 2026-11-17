# PR entry and refresh: production runtime correction

The original dead-sidebar-link complaint reproduces through real PR discovery and real window mouse events. With A selected, clicking B's PR badge leaves A selected and shows no panel. The native PR view also works through its separate Overview entry, including actual description, checks, and Timeline data. Refresh then breaks the review: Overview accepts the new CI result while the native PR view retains its old checks, description, and comments.

This corrects the evidence status of the relevant source-only findings. It does not say that Ferrite has no PR screen, that every PR click is dead, or that GitHub pages fail in the installed browser.

## Runtime and isolation

- Ferrite source: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Existing `target/debug/ferrite` SHA-256: `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`.
- Real headless production window, 1440 × 900 logical pixels; screenshots 2880 × 1800 at scale 2.
- Fresh Store: `/tmp/ferrite-audit-runtime-qg7923qc/threads`; scratch Git repositories `repo-a` and `repo-b`, each with a real `main` commit, `audit-pr` branch, local commit, and synthetic GitHub remote.
- Thread 1: **PR B destination 202**, repo-b, PR #202. Thread 2: **PR A selected 101**, repo-a, PR #101.
- Fake Claude/Codex executables declare version 9999 and never launch paid agents. The fake Claude implements the actual provider protocol. No `pr_landed`, `fixture_pr_snapshot`, direct UI mutation, or test-world injection was used.
- Sidebar discovery executed the production `gh pr view --json number,state,isDraft,url,title,statusCheckRollup` command in each scratch checkout. Its fake executable recorded cwd and arguments in [gh-requests.jsonl](../evidence/pr-entry-correction/gh-requests.jsonl).
- The detail reader uses a different executable resolver. `ferrite-reviewr/src/proc.rs:14–24,41–61` prepends common host directories before inherited PATH, so it initially bypassed the fake `gh`. That first read reached installed `gh` and returned “Could not resolve to a Repository” for the synthetic repo; it was read-only, not a forge write. We stopped and isolated that seam before further detailed reads.
- Installed `gh config --help` documents `http_unix_socket`. A child-only `GH_CONFIG_DIR` then pointed to a temporary config using a local UNIX socket. A wholly synthetic token replaced credentials in those child processes. The socket fixture never forwards requests and refuses GraphQL mutations. This exercised installed `gh`, production reviewr branch association, detail GraphQL parsing, server response, and native rendering. No system trust, global auth/config, installed executable, or application code changed. [HTTP request log](../evidence/pr-entry-correction/gh-http-requests.jsonl), [preflight](../evidence/pr-entry-correction/gh-socket-preflight.json), [fixture server](../evidence/pr-entry-correction/gh-socket-peer.py).

All commands, including unsuccessful setup attempts, are retained in [commands.jsonl](../evidence/pr-entry-correction/commands.jsonl). Two early shortcut guesses opened Files and entered the synthetic phrase “Refresh PR status” into the fake-agent composer; screenshots 09–11 are setup attempts, not evidence of a refresh action. The verified refresh used the actual thread context menu and then the actual Command-K palette command.

## NAV-01: another thread's underlined PR still does nothing visible

**P1; runtime-confirmed.** Original finding `pr-navigation-01`.

Precondition: A is selected, panel hidden, B visible in the sidebar, both PR badges obtained through production `gh` discovery. The real `nav-card-pr-1` badge was at `(82, 220, 36, 16)` logical pixels. The window driver sent a left click at its center `(100, 228)`.

The top title remained **PR A selected 101**; `window find panel` returned “nothing drawn as `panel`”. B's underlined #202 hovered and showed its tooltips, but the requested destination did not become visible. Both the initial manual run and the later automated run reproduced this.

- [Before](../evidence/pr-entry-correction/18-cross-before.png)
- [After the actual B badge click](../evidence/pr-entry-correction/19-cross-after.png)
- [Rendered title before](../evidence/pr-entry-correction/18-title-before.png) / [after](../evidence/pr-entry-correction/19-title-after.png)
- [Machine verdict](../evidence/pr-entry-correction/cross-thread-verdict.json)

The red-capable command actually executed was:

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/pr-entry-correction/repro.py cross-thread
```

It exited **1**, reporting `before_title: PR A selected 101`, `after_title: PR A selected 101`, and `panel_visible: false`. The script asserts B's title and a visible panel after the click; it reads rendered title pixels using Tesseract and queries actual element bounds, not private UI state.

The narrow control changed only the clicked badge: clicking A's own PR badge at `(100, 311)` opened Browser visibly. Its panel measured `(835, 0, 605, 899)` and the left navigation collapsed. [Focused badge result](../evidence/pr-entry-correction/02-focused-thread-pr-click.png). That refutes a universal dead hit target or a missing PR URL.

Cause matches the runtime: `cockpit.rs:12806–12819` stops the card's propagation and calls `open_pr(thread)`; `:1826–1831` opens that thread's browser without selecting it. Browser revelation addresses that thread's pane (`cockpit/browser.rs:637–647`), while the visible panel belongs to the focused pane (`cockpit/panel.rs:204–211`). The hidden thread's browser state may change without the explicit user navigation becoming visible.

**Acceptance:** An explicit B PR click must visibly activate B's intended destination on that click, including when A's panel is hidden. Keep automatic agent reveals and explicit user navigation distinct. Repeat with B absent from the board and from the empty board; those two branches were not exercised here.

## NAV-02: native PR detail is real, but the obvious badge bypasses it

**Runtime-confirmed split entry behavior.** Supports `pr-navigation-02`; this is not a claim that the native screen is absent.

Clicking the focused PR badge opened **Browser**, with no `pr-number` native element. In contrast, clicking Overview's `panel-pr` row at `(851, 230, 573, 32)` opened a native **#101** tab, header, Summary, Guide, Timeline, and Code selectors. [Overview entry](../evidence/pr-entry-correction/03-overview-pr-entry.png).

After isolating installed `gh`, native Summary displayed **REPO-A detail revision 1**, its description, `audit-check`, and “Checks passing”; Timeline displayed `audit-reviewer` and **Comment revision 1 for repo-a**. The HTTP log contains the production branch association and detail queries for repo-a/#101.

- [Native Summary with actual fixture read](../evidence/pr-entry-correction/07-native-summary.png)
- [Native Timeline](../evidence/pr-entry-correction/08-native-timeline.png)

The badge and Overview are materially different user journeys. Finding an implementation and a rendered tab is insufficient evidence that the sidebar control takes the user there.

**Boundary:** Headless was built without CEF. The Browser destination correctly reported that this window lacks browser capability. This proves the Browser route and visible error handling, not whether the GitHub page loads in installed Ferrite. No external browser modifier click, hover-popover external link, footer PR icon, PR writes, Guide generation, or Code review interaction was verified in this slice.

## NAV-03: Refresh PR status leaves the open review stale

**P1 candidate; runtime-confirmed.** Supports `pr-operations-001` in the original operations audit; no severity/count changes are made to the consolidated inventory here.

1. Load native Summary and Timeline with revision 1 and a successful check.
2. Change the local forge fixture to revision 2 and a failing check, keeping the same PR number, open state, and draft state.
3. Right-click A's real sidebar row and press the displayed **S — Refresh PR status** menu shortcut. [Actual menu](../evidence/pr-entry-correction/12-thread-menu-refresh.png).
4. The production lightweight `gh pr view` ran again. No new detailed GraphQL read ran.
5. The native view still showed **Checks passing**, revision 1 description, and revision 1 comment. Overview for that same thread showed **checks failing**.

Evidence: [stale Timeline](../evidence/pr-entry-correction/13-refreshed-stale-timeline.png), [stale Summary](../evidence/pr-entry-correction/14-refreshed-stale-summary.png), [fresh Overview check state](../evidence/pr-entry-correction/15-overview-fresh-checks.png).

The automated repetition tested the opposite CI direction, revision 2/failure → revision 3/success, using Command-K → “Refresh PR status” → Enter. It again ran one lightweight read and zero detail reads. Rendered Summary remained revision 2 with **1 failing**. The exact red-capable command:

```sh
python3 docs/audits/2026-11-13-t3-parity/evidence/pr-entry-correction/repro.py refresh
```

It exited **1**. [Palette command](../evidence/pr-entry-correction/21-refresh-command.png), [unchanged review after refresh](../evidence/pr-entry-correction/22-refresh-after.png), [rendered-text verdict and read counts](../evidence/pr-entry-correction/refresh-verdict.json). The harness now resets the starting fixture state to make repeated runs independent of the lifecycle control below.

Recovery is specific: clicking Overview and its PR row again does issue a new detailed read, and revision 2/failure appeared. [Recovered Summary](../evidence/pr-entry-correction/17-reopened-summary-settled.png). The intermediate frame 16 says “Reading…” and is not evidence of a blank or permanently broken review.

The lifecycle control also worked: changing the fixture from OPEN to MERGED, then invoking the same refresh command, issued new detail requests and displayed merged revision 4, successful checks, and revision 4's comment. [Merged refreshed Summary](../evidence/pr-entry-correction/24-lifecycle-refresh-settled.png), [new Timeline comment](../evidence/pr-entry-correction/25-lifecycle-new-comment.png). This was a synthetic read transition; no merge was performed.

The cause is a split cache invalidation contract. `check_pr` reads lightweight facts (`cockpit.rs:1751–1784`); `pr_landed` forwards changed facts (`:1811–1822`). `pr_reading_changed` invalidates the native snapshot only for a different PR number or lifecycle/draft tone (`panel/pull_request.rs:323–336`). CI/title/body/comment changes do not qualify. Re-entry explicitly calls `read_pr_snapshot(thread, true)` through `panel.rs:254–263`, explaining the working recovery.

**Acceptance:** Refresh of the currently displayed PR must refresh its checks, body, comments, and displayed lifecycle consistently. Keep a stale snapshot visibly marked while reading, rather than presenting outdated passing CI as current. Regression verification must use the real refresh entry and a changing forge response, not direct `pr_landed` or snapshot injection.

## Additional visual observation and limits

The initial native detail error was rendered as one centered, unwrapped line wider than the 605px panel. Its beginning and remedy were cut off; the native header and tabs were still visible. [Captured error](../evidence/pr-entry-correction/04-overview-native-pr.png). The fixture repository's absence caused the error; the clipping is the visible defect. This is supporting evidence for error-state layout, not a claim that normal GitHub repositories fail.

No timing claim about sidebar animation is made from these settled screenshots. We did not test live installed CEF, network auth expiration, multiple PRs on one branch, no-PR badges, arbitrary repo browsing, external modifiers, review writes, or agent-generated PR actions. Those remain separate journeys.

## Cleanup and verification

No application code edits, build, Rust unit tests, or typecheck were performed. Verification was through the existing real headless binary, actual window events, production Git/gh reads, saved screenshots, OCR assertions, and narrow differential fixture changes.

Thread 1 and Thread 2 were both closed through the CLI before shutdown; successful close responses are recorded at the end of `commands.jsonl`. All created server/window/socket process groups were terminated and checked absent. All five recorded fake provider PIDs were checked absent. The abandoned Vision OCR process was terminated and checked absent; the retained OCR script uses bounded Tesseract calls. [Cleanup record](../evidence/pr-entry-correction/cleanup.json), [process ledger](../evidence/pr-entry-correction/state.json).

The audit scripts, logs, screenshots, and synthetic Git repos are retained as evidence. They run only against their disposable Store. No user thread, actual repository contents, or installed Ferrite window was changed by this worker.
