# Ferrite production-quality audit against T3 Code

The goal is a detailed account of where Ferrite fails complete everyday workflows, what T3 Code does differently, and what must change to reach a production-quality bar. The user should not have to enumerate broken clicks, missing recovery paths, awkward quoting, or incomplete motion.

The source evidence supports the concern that many features stop before the full interaction is complete. The strongest pattern is inconsistent ownership of navigation, keyboard input, content, and resource lifetime. A tab can exist without its backing view. A focused browser can still invoke the conversation's Submit action. A citation can display a comment supplied by a fixture without giving the user a way to author that comment. These are integration problems with concrete user consequences. [Architecture analysis](architecture.md).

Application source is unchanged. This directory contains the audit, evidence, a reproducible tally, and a proposed repair sequence. It is not an implementation or a claim that Ferrite now meets the quality bar.

**Scope correction:** the first pass was too source-heavy to satisfy the requested investigation of everyday product quality. Its tally did not establish that the existing features work, and its runtime checks did not cover the browser's visual integration, panel sizing history, or the full agent journey. The corrective pass below tests those paths directly and records what remains unexercised. Completion of a source partition is not completion of a user journey.

Start with the [existing-workflow coverage matrix](reports/workflow-coverage-correction.md), [agent/live-output reproduction](reports/agent-live-correction.md), [panel sizing reproduction](reports/panel-runtime-correction.md), and [browser visual critique](reports/visual-quality-correction.md). The [T3 surface contracts](reports/t3-surface-contracts.md) explain its actual sizing, composition and motion behavior from pinned source. The runtime correction is reconciled into the tally below. Its remaining verification gaps are explicit; no complete journey is certified production-ready.

## Actual T3 subscription and source comparison

**Systematic surface reconciliation:** the earlier “browser preview” interpretation of the top-right floating box was wrong. The box is Thread details. The follow-up now maps rendered roots, placement, coexistence, input and resource lifetime, with independent coverage reconciliation and fresh child/parent, file/details and terminal/file checks. Start with [the corrections and surface map](reports/surface-reconciliation.md). This supersedes broad feature-presence language; it does not certify every runtime variant.

**Floating details correction:** T3's top-right Thread details card was only named in the checklist. It is now recorded as the separate composition finding `quality-correction-005`, with exact installed source and fresh native toggle, shortcut, responsive-popover, coexistence and dismissal checks. [Report](reports/thread-details-card-correction.md). Earlier validation snapshots predate this addition.

**Follow-up correction:** subagent placement itself needs to change in the repair proposal. T3 opens a child as a main conversation; Ferrite's normal agent click opens the compact right panel. Fixing that panel's missing controls alone does not capture the intended experience. [Placement evidence and corrected primary route](reports/subagent-placement-correction.md).

The follow-up used real **GPT-6.1 Sol and Claude Sonnet 5.5** sessions in T3, with actual children, pre-completion command output, approvals, interruption, citations/comments/images, queue editing, side-chat context delivery, PR navigation, browser interaction, files and terminals. All 21 settings sections were inspected. The installed Nightly resolves to **1264c206aee0dce805d6279809118bc091f29d2f**, 746 commits behind the newer comparison checkout; installed and newer behavior are separated.

Start with [the live conclusions and Ferrite acceptance requirements](reports/t3-live-conclusions.md). Follow the [273-case coverage reconciliation](reports/t3-live-coverage-reconciliation.md), [exact installed provenance](reports/t3-installed-source-match.md), [conversation/provider source match](reports/t3-live-source-correspondence.md), [browser source match](reports/t3-browser-live-source-match.md), [PR/panel/motion source match](reports/t3-pr-live-source-match.md), and [files/terminal source match](reports/t3-files-terminal-live-source-match.md). Checklist IDs overlap and include unexecuted variants; they are not full passes or unique feature counts.

The [native observation ledger](evidence/t3-live/observations.md) records outcomes and corrections. Native T3 screenshots were displayed during execution, not saved as app-chrome PNGs. The saved [browser content export](evidence/t3-live/browser-content-export.png) shows page content only. The Ferrite defect screenshots below remain saved evidence.

Both test roots were archived after their children completed; T3 quit, test surfaces closed, changed preferences restored, the fixture server stopped and owned bounded processes disappeared. [Cleanup manifest](evidence/t3-live/cleanup.json). An actual Sonnet child raised eight audit-consistency flags, now reconciled with explicit limits. [Independent artifact review](reports/t3-live-independent-review.md).

The initial T3 run strengthened repair criteria without closing any Ferrite failure. The later floating-details correction adds one distinct source-proven composition finding; the current tally is 177. No complete Ferrite workflow is certified.

## Reproductions and visual evidence

The corrective pass covers 52 existing-feature journeys: **22 have a reproduced failing branch, five have only a successful bounded branch, 23 remain unexercised at runtime, and two are inconclusive**. The original 136 repair/quality records map to these journeys. The later floating-details record, quality-correction-005, has its separate report and is outside this earlier 52-journey snapshot. The subsequent PR-scope correction also moves pr-navigation-03 from optional capability to required repair scope; current repair/quality total is 138. These counts describe coverage, not a quality score. No full journey is certified; source review, partial runtime checks and complete release verification remain distinct. [Detailed coverage and remaining checks](reports/workflow-coverage-correction.md).

| Finding | Saved evidence and explanation |
|---|---|
| Agent output exists, but View live output says No output yet. | [Child output](evidence/agent-live-correction/06-agent-open-live-two-nested.png) → [wrong Background destination](evidence/agent-live-correction/09-main-live-output-click.png). [Report](reports/agent-live-correction.md). |
| Agent reopen restores a named tab with no body. | [Blank reopened Agent](evidence/agent-live-correction/12-reopened-agent-blank.png). [Report](reports/agent-live-correction.md). |
| Agent prose is cut off at ordinary width. | [420px panel](evidence/agent-live-correction/13-completed-agent.png) → [maximized panel](evidence/agent-live-correction/14-agent-maximized-output.png). Full endings appear only in the wider state. |
| A tool visit permanently enlarges the shared panel. | [Overview before](evidence/panel-correction/03-overview-reopened.png) → [Overview after Files](evidence/panel-correction/05-overview-after-files.png): 320px → 864px at a 1440px window; restart preserves it. [Report](reports/panel-runtime-correction.md). |
| Browser repeats headings and tab strips and shifts during loading. | [Saved host chrome](evidence/panel-correction/13-browser-after-files.png); [native loaded/loading observations](evidence/native-correction.md); [visual critique](reports/visual-quality-correction.md). The saved headless host-unavailable state is not a claim that native CEF cannot load pages. |
| PR refresh leaves the detail view contradicting current status. | [Stale detail](evidence/pr-entry-correction/14-refreshed-stale-summary.png) versus [fresh Overview](evidence/pr-entry-correction/15-overview-fresh-checks.png). [Report](reports/pr-entry-runtime-correction.md). |
| New unsent work is absent after restart. | [Draft before](evidence/draft-lifecycle-correction/03-unsent-draft-before.png) → [after window restart](evidence/draft-lifecycle-correction/06-new-draft-after-restart.png). Existing-thread draft restored as control; normal Quit was not tested. |
| Long quoted context has an unreadable, non-inspectable preview. | [Whole response in an 88px tile](evidence/attachment-correction/13-whole-response-cited.png). [Report and successful full-payload control](reports/attachment-runtime-correction.md). |
| Files tabs extend outside the window; tree and preview can stay stale. | [Overflowing strip](evidence/files-correction/26-kept-tabs-overflow.png). [External edit/add/delete checks](reports/files-runtime-correction.md). |
| Search Enter can open a result that is not visible. | [Retained scroll offset](evidence/navigation-correction/14-search-offscreen-zero.png). [Search and Snoozed Rename report](reports/navigation-runtime-correction.md). |

The parent rejected the apparent blank paging and missing-form-input failures as product findings because their headless captures and hit geometry were insufficient. Working-row rename completion is also inconclusive. Those gaps remain in the matrix; they are not silently converted into passes or defects.

## Read the audit

| Document | Purpose |
|---|---|
| [Consolidated findings](findings.md) | Every retained finding, priority, evidence level, source citations, recommendation, and acceptance check. |
| [Tally CSV](tally.csv) | Filterable inventory for planning and tracking. |
| [Issue JSON](issues.json) | Machine-readable inventory with merged-record references and review notes. |
| [Counts](counts.json) | Exact counts regenerated from the issue records. |
| [Architecture](architecture.md) | Shared causes, data and event flows, and reusable implementation boundaries. |
| [Remediation program](remediation.md) | Independently verifiable repair units and their release gates. |
| [Runtime checks](reports/runtime-interactions.md) | Real GPUI input results, screenshots, successful counterexamples, provider payloads, and cleanup. |
| [Independent Fable review](reports/independent-review.md) | A skeptical check of seven high-impact source claims, priority inflation, and evidence limits. |
| [Decisions](decisions.tsv) | What changed during the investigation and why. |

## Tally and interpretation

The consolidated inventory contains 177 records from 181 original records. Four overlapping records are merged while their affected workflows and acceptance checks remain visible. There are 30 P1, 133 P2, and 14 P3 records. No P0 claim remains after review. [Generated counts](counts.json).

Sixteen records are marked runtime-reproduced, 150 source-proven, nine inferred, and two unverified. Several source-proven or inferred records additionally contain reproduced branches without upgrading their untested branches. The adjudication notes preserve those boundaries. These are 177 audit findings, not 177 reproduced bugs.

| Kind | Records | Meaning |
|---|---:|---|
| Defect | 62 | A source-defined failure or an explicitly labeled suspected failure. Confidence is recorded separately. |
| Partial capability | 42 | A visible or supported workflow loses meaningful behavior or completeness. |
| Interaction | 26 | Pointer, keyboard, layout, navigation, or discoverability failure. |
| Missing capability | 29 | An absent capability. Most are product candidates, not automatically required repairs. |
| Motion | 6 | Missing or incomplete transition behavior. The matrix retains separate surfaces. |
| Intentional difference | 6 | A current product choice whose consequences need explicit assessment. |
| Verification gap | 6 | A missing check or unresolved reported symptom. These are not counted as reproduced defects. |

The inventory separates 138 repair or quality records, 27 capability candidates, six product choices, and six verification gaps. Citation commenting is a required workflow gap because the user explicitly asked for it. Optional voice input, guest sharing, device panels, and multi-model prompt fan-out do not take precedence over broken navigation, content loss, or input ownership.

P1 marks high-priority primary-workflow, integrity, or trust concerns. P2 marks material usability, partial behavior, and recoverable failures. P3 marks smaller polish or optional scope. Priority does not establish frequency or reproduction. An unverified user-reported Cite failure can have P1 triage priority while remaining unverified.

## The reported problems have concrete traces

| User concern | What the investigation establishes | Main records |
|---|---|---|
| PR badges underline but appear to do nothing. | Reproduced with two scratch repositories: click B’s badge while A is selected, and A stays visible with no panel. The focused-thread control opens Browser. | [PR runtime reproduction](reports/pr-entry-runtime-correction.md) |
| The PR view is inaccessible. | A native PR panel exists, but the obvious sidebar and footer affordances bypass it. There is no independent project PR inbox, and native targets remain bound to the current branch. | [PR entry points](reports/pr-navigation.md) |
| PRs do not work reliably. | Both menu and palette Refresh updated Overview while native checks, description and comments stayed stale. Re-entering the PR view recovered them. Other review/action gaps remain source-backed. | [PR runtime reproduction](reports/pr-entry-runtime-correction.md), [PR operations](reports/pr-operations.md) |
| Subagent viewing is incomplete or lands in background work. | Reproduced: View live output opens empty Background despite child output; nested-agent links do nothing; Agent and SideChat reopen blank; child prose clips at ordinary width. Direct child opening and forwarded live updates worked as controls. | [Agent runtime evidence](reports/agent-live-correction.md), [SideChat reopen](reports/draft-lifecycle-runtime-correction.md) |
| Basic keys and clicks fail around the right panel. | Global actions run before raw panel or browser key handlers. Files edits trigger focus reconciliation that lacks a Files owner. Several panel controls lack keyboard roles. Find and Close still route through older ownership rules. | [Input and focus](reports/input-focus.md) |
| Right-clicking a link cannot open it in the system browser. | Ferrite's transcript menu lacks an explicit system-browser action. PR badge context can copy the thread URL. Custom file chips lose link-menu metadata. T3's app-rendered Markdown menu explicitly offers integrated and system-browser choices. Its generic webpage menu is a different, more limited path. | [Basic link behavior](reports/link-affordances.md) |
| Browser looks unfinished. | Duplicate generic Browser heading, inset card, nested page tabs and toolbar consume height and obscure hierarchy. Native Loading removes Annotate and adds a status row, shifting the address and page. | [Visual correction](reports/visual-quality-correction.md), [T3 contracts](reports/t3-surface-contracts.md) |
| Opening the panel produces an oversized surface. | Files/Changes overwrite the shared saved width. A 320px Overview becomes 864px and stays that way through close/reopen and restart. | [Measured runtime states](reports/panel-runtime-correction.md) |
| Sidebar collapse snaps. | Sidebar and right-panel widths explicitly snap by current Ferrite policy. This is an actual requirement gap. T3 defaults its optional panel animation to zero, which does not override the user's stated preference. | [Motion matrix](reports/motion.md) |
| Jump scrolling is intermittently wrong. | Manual scrollbar input does not consistently cancel the automatic owner or send anchor. Track clicks use the wrong distance. Other findings cover latest-button visibility, disclosure, End, Find, reflow, and paging under a pinned prompt. | [Scroll transition audit](reports/scroll-quality.md) |
| Attachments and quoting look unfinished. | Runtime context attachment compresses about 991 characters into an 88-pixel tile with no click-to-inspect. Full text still reached the provider. Image-format failures remain source-backed; actual picker/drop and image preview remain untested. | [Attachment reproduction](reports/attachment-runtime-correction.md), [source audit](reports/quote-attachments.md) |
| A citation cannot carry the user's own attached text. | Production Cite entry points create an empty per-quote comment, and the quote card has no comment editor. A clean runtime check did successfully send a quote with ordinary composer text. The per-quote authoring gap is confirmed; the reported intermittent inability to cite or add ordinary text remains unresolved. | [Quote source audit](reports/quote-attachments.md), [runtime check](reports/runtime-interactions.md) |

These findings do not establish that every reported intermittent symptom has the same cause. Each report states the trigger, source condition, confidence, and remaining runtime check.

## Important problems found beyond the examples

The inventory also records failures the user did not have to name.

- An idle send can clear the draft before admission fails. A never-sent pane has no durable draft record. [Composer](reports/composer.md).
- A failed file save is silent, and closing removes the buffer before saving. A stale editor can overwrite an agent's newer disk content. T3's stale-write protection was not established either, so that second finding is a Ferrite integrity issue rather than a claimed T3 advantage. [Panel tools](reports/panel-tools.md).
- The file tree caches entries without refresh. File tabs can become inaccessible. Changes truncates each file at 120 displayed lines without an expansion action. [Panel tools](reports/panel-tools.md).
- Remote settings can be written locally while the remote server rereads its unchanged settings. CLI update targeting also mixes local tools with remote sessions. Approval UI can display settings that differ from the running session's sandbox. [Providers and settings](reports/providers-settings.md).
- Rename can focus an editor that Parked and Snoozed rows never render. Search selection can drift when rows reorder. Undo Park does not reconstruct lost grouping. [Projects and navigation](reports/projects-navigation.md).
- Find omits folded conversation content. Successful turns hide plan history. Chapter pins have no durable navigation contract. [Transcript](reports/transcript.md).
- Quoted `@path` and skill syntax can be scanned as active provider input. Literal blockquote prefixes can become noneditable citation objects after restore. [Quote and attachment audit](reports/quote-attachments.md).
- Ordinary PR CI does not run the cockpit and pane interaction suites. A browser-specific workflow runs a narrow subset under narrower triggers. Several tests call handlers directly and therefore miss action dispatch or focus failures. [Coverage audit](reports/coverage-quality.md).

## Coverage

The investigation used 15 source partitions, an architecture reconciliation, independent Fable review, runtime-access diagnosis, isolated interaction testing, and 12 corrective specialist assignments covering runtime journeys, visual presentation, T3 contracts and coverage reconciliation. The broad comparison uses current T3 source, not only the older transcript reference.

| Area | Detailed report |
|---|---|
| PR reachability and target navigation | [PR navigation](reports/pr-navigation.md) |
| PR data, creation, review, refresh, and recovery | [PR operations](reports/pr-operations.md) |
| Agents, ancestry, history, and background work | [Subagents and background work](reports/subagents-background.md) |
| Browser chrome, pages, controls, and persistence | [Browser](reports/browser.md) |
| Keyboard, pointer, focus, and accessibility | [Input and focus](reports/input-focus.md) |
| Structural motion and micro-interactions | [Motion](reports/motion.md), including a 72-row surface matrix |
| Panel, files, changes, and terminal | [Panel tools](reports/panel-tools.md) |
| Draft, send, queue, steer, and attachments | [Composer](reports/composer.md) |
| Transcript, search, rendering, and history | [Transcript](reports/transcript.md) |
| Providers, permissions, settings, and remote use | [Providers and settings](reports/providers-settings.md) |
| Projects, threads, search, archive, and worktrees | [Projects and navigation](reports/projects-navigation.md) |
| Broader T3 inventory and test coverage | [Coverage and quality](reports/coverage-quality.md) |
| Link menus, destinations, copying, and activation | [Link affordances](reports/link-affordances.md) |
| Scroll state transitions and interruption | [Scroll quality](reports/scroll-quality.md) |
| Quote, comment, attachment, send, and replay | [Quote and attachments](reports/quote-attachments.md) |

## Verification and limits

Ferrite source is pinned to `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. T3 is pinned to `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. The prior Ferrite reference used T3 `bcee0f5d2872ba193c66ed9ba979bbe424f89ff9`. The current source was fetched for this audit. References in the consolidated inventory are commit-pinned links.

The installed Ferrite application reports version 0.4.0, but its exact source commit was not established. Source claims apply to the pinned checkout. Headless verification uses the explicitly built checkout and a temporary Store. Native observations of the installed app remain separately labeled.

Initial screenshot capture timed out and browser observations were rejected. Native screenshots and browser Observe later succeeded after the audit thread and its panel were selected. Subsequent controls again lost their usable state. The audit does not turn an attempted click into a reproduction. The exact chronology is in [runtime observations](evidence/runtime-observations.md), and the browser eligibility check is explained in [runtime access](evidence/runtime-access.md).

To extend verification beyond that intermittent access, the pinned source was built successfully with `cargo build --locked -p ferrite --features headless`. The real GPUI window then ran with a fresh temporary Store, scratch files, and a deterministic fake provider. Pointer and key events went through the production bindings. This build has no CEF, so it verifies host interactions, not live Chromium input, popups, or website dialogs. [Build evidence](evidence/headless-build.txt), [runtime report](reports/runtime-interactions.md).

| Executed interaction | Observed result |
|---|---|
| Click Files editor, type separate `x`, `y`, `z` keys. | `xyz` entered the conversation composer; the file stayed unchanged. |
| Click Files filter, type separate `s`, `a`, `m` keys. | `sam` entered the composer; the filter stayed empty. |
| Click file content and press Cmd+F. | Find searched and highlighted the conversation. |
| Leave a sentinel draft, select Files in the empty-panel launcher, press Enter. | The sentinel reached the fake provider; the launcher remained open. |
| Select text, Cite, type ordinary text, and send. | The provider received `> amber\n\nMy clean annotation.`. This clean case succeeded. |
| Click the quote tile and try to type a comment there. | No comment editor appeared; the key was unhandled. |
| Right-click an ordinary transcript web link. | Open link and Copy link appeared; explicit system-browser choice was absent. |
| Right-click the inline `sample.rs` chip. | Whole-message actions appeared instead of file-target actions. |
| Press Escape with the link menu open. | The menu remained visible. |

The [runtime report](reports/runtime-interactions.md) links screenshots and command records for these results. The parent inspected the reproduced editor/filter states, unintended-send screenshot, clean quote-plus-text screenshot, web-link menu, and file-chip menu. The corrective pass subsequently reproduced PR target activation and Agent/SideChat reopen failures. Live Chromium keyboard handling, intermittent scrolling under manual input, multiwindow behavior, image-format failures, valid form entry and full motion timing still require targeted runtime verification. The source reports give their specific acceptance conditions; no blanket quality pass is implied.

The exact narrow test command ran once and passed all 12 selected tests, with 938 skipped.

```sh
cargo nextest run -p ferrite-cockpit -E 'test(cockpit::tests::send_scroll::)'
```

The [test log](evidence/scroll-nextest.log) records the result. Existing send-scroll tests do not cover every newly identified transition. The [scroll report](reports/scroll-quality.md) maps those omissions. This audit does not claim full-suite, complete application E2E, or full visual parity coverage.

The original independent review sampled seven high-impact source claims; it did not review the later corrective cases. It also checked source-reference existence and line bounds. The final assembler validates 1,010 distinct source references across all partitions. Mechanical citation validation does not prove every interpretation. [Corrective artifact validation](evidence/correction-artifact-validation.json) checks generated outputs, local links, structured case coverage and fixture-script syntax. The parent corrected the overstated quote-comment capability, conditional P0 ratings, duplicate records, an overly broad File-reopen inference, and a stale runtime-access description. [Review](reports/independent-review.md), [adjudications](adjudications.json).

## What the production bar requires

A feature is complete only when its ordinary entry points reach the correct target, pointer and keyboard actions agree, its content survives failure and recovery, and its visible states explain what happened. Resizing, switching threads, opening another panel, scrolling manually, retrying, and restarting are part of the same workflow.

The repair sequence should first make input routing and content retention dependable. It should then unify target identity, complete the PR and Agent views, restore resources correctly, and give scrolling and motion a consistent ownership rule. Attachment and citation presentation must be reviewed as text and interaction design, not accepted because a tile renderer exists. The full sequence and acceptance checks are in [remediation.md](remediation.md).

This is a recommendation to repair shared causes in verifiable units. The evidence does not justify an undirected rewrite or importing every T3 capability before the existing workflows work.

## Reproduce the inventory

The records use a stable shape with identity, priority, category, confidence, impact, source evidence, recommendation, and acceptance check. Review decisions are separate from original slice records.

```sh
python3 docs/audits/2026-11-13-t3-parity/assemble.py --write
python3 docs/audits/2026-11-13-t3-parity/assemble.py
```

The temporary T3 checkout at `/tmp/ferrite-t3-audit-source-20261113` was removed at the user's request after the audit. To rerun the assembler, obtain `https://github.com/pingdotgg/t3code.git`, check out `74573fc29b901bafbf8f1dbb2eb29606da1fa448`, and pass `--t3 /path/to/pinned/t3code` to both commands. The script validates required partitions, record fields, IDs, priorities, confidence labels, merge targets, source files, and line bounds. It reproduces the JSON, CSV, Markdown inventory, and counts.

The tools checklist's 132 local checkout links were converted to URLs pinned to that same commit before removal. Earlier validation fingerprints describe the audit at their recorded times and predate this link-only cleanup. Reports, saved evidence and test fixtures were retained; the separate performance worktrees were untouched. [Cleanup record](evidence/worktree-cleanup.json).

## Principles and decisions

- Experience First changed the audit unit from implemented widgets to complete user journeys, including presentation and recovery.
- Fix Root Causes led the investigation through event dispatch, ownership, persistence, and backend state instead of stopping at visible symptoms.
- Prove It Works kept runtime results separate from source proof and introduced isolated real GPUI checks after native observation proved intermittent.
- Build the Lever produced the shared investigation brief, structured records, and rerunnable inventory assembler.
- Guard the Context Window divided source reading among bounded specialists while keeping one consolidated result.
- Model the Domain shaped the finding records and the recommendation to retain citation, attachment, target, and operation identity across the workflow.

## Attention

The original seven-claim sample was reviewed by Fable; subsequent correction evidence was reconciled by the parent.

The independent review did not semantically validate every record. It flagged conditional severity, duplicate roots, source-versus-runtime limits, and unproven T3 advantages. The consolidated inventory applies the accepted corrections. Its original zero-runtime statement predates the subsequent checks documented above.

The sole extra-model Claude review Thread, 1001, was closed and a subsequent lookup confirmed that it was no longer open. All corrective specialists completed; their isolated fixture threads were closed and tracked servers, headless windows, fake providers and process groups were verified absent. The native reference T3 app was quit. The parent’s temporary HTTP server was stopped and browser State confirmed no owned pages remain. Empty Browser/Agent inspection surfaces may remain in this audit thread after operator navigation interrupted final surface cleanup; they are not running worker threads. [Original cleanup](evidence/headless/cleanup.json), [corrective cleanup reconciliation](evidence/correction-cleanup.json), [native record](evidence/native-correction.md).
