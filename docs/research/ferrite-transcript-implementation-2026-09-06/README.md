# Transcript findings: final implementation account

2026-09-07. Supersedes the earlier partial report.

**All 60 register entries are accounted for: 57 implemented, verified or deliberately
adapted; 3 excluded (F05, F58, F59).** This is not a claim of 57 new features.
The operator also retained enlarged heading sizes under F13. Heading emphasis
F14 was not excluded and is now implemented.

The remaining pass fixed plain/unknown fences, heading emphasis, exact answer
notes and small-output copying, narrow-table overflow, duplicated live reasoning,
durable completion observations, exact approval input, elapsed labels,
indicator-only motion and reading-position preservation. Existing native GPUI
controls, selection, highlighting and scroll machinery are reused.

## Validation and review

`cargo test --locked --workspace --no-fail-fast`: **919 passed, 0 failed,
20 existing live-CLI tests ignored**, across 21 targets. Includes 271 native UI
tests. [Original workspace output](workspace-tests.log).

`python3 scripts/test_cli_capture.py`: **8 passed**.
[Original parser output](capture-tests.log).

`cargo build --locked -p ferrite` passed. The optional native framebuffer build
and all 14 captures passed. `git diff --check` passed. Existing dead-code and
upstream future-compatibility warnings remain; no new ignored tests.

Root authored the narrow tests and observed failures before assigning production
work to Terra medium agents. Agents ran only assigned exact tests. Root reviewed
implementation and ran the workspace suite. New checks cover native pointer/copy,
streaming document identity, wrapped multi-select/reject/ack, Composer resize/caret,
reading anchor, reasoning handoff, highlighting and durable completion/reopen.

Review corrected an overbroad heading exclusion and caught duplicate reasoning
in the framebuffer. An agent initially exempted formatted headings from
deduplication; root rejected that exception and extended the test to bold source.
Root strengthened the prior multiline-command assertion to
require the entire original command, and changed an old trimmed-note expectation
to exact source. The overflow test verifies native track geometry; toolkit wheel
routing remains its existing implementation. GPUI debug layout bounds do not
measure scroll paint translation, so an attempted gesture assertion against
those bounds was removed instead of changing working toolkit scroll code.

Completion adds a direct dependency on the already resolved chrono version and
one backward-compatible store schema revision. Old logs remain readable. A live
completion is observed before runtime teardown, accepted/deduplicated by Activity,
then persisted; replay never fabricates a new clock reading. A gated disk-boundary
test also verifies that a child completion arriving after a history checkpoint
survives the pending-prefix reload. Cost stays absent.

## Integration with current main

Final integrated `cargo test --locked --workspace --no-fail-fast`: **957 passed,
0 failed, 20 existing tests ignored**, across 21 targets, including 297 native UI
tests. [Integrated workspace output](integrated-workspace-tests.log). The 8 parser
checks also passed after integration.

The presentation changes now use the retained transcript renderer from PR #70
(561bcc0), including its native list, selection document and per-subject entities.
Width reflow preserves the next visible row through the native scroll facade.
Received-reasoning visibility is checked against native window-space row bounds.
The child completion test now also evicts and reloads ordinary disk history,
verifying that the exact original completion observation survives both paths.
The native test helper now measures the disclosure button directly, eliminating
an extra wrapper that displaced absolute controls only in test builds. All
original pointer, scroll-offset and retained-entity assertions remain enabled.

The 14 linked screenshots were refreshed after integration; formatting, live
reasoning, narrow approval and narrow edge cases were visually inspected.
[Integrated native capture output](integrated-native-capture.log).

## Every finding

| ID | Final status | Implementation, evidence or reason |
| --- | --- | --- |
| F01 | Implemented | Shared 9px marker + 8px gap; answer continuations align with content. Native caret/gutter test. |
| F02 | Implemented | Prompt marker and content use the same 17px inset; neutral prompt surface retained. |
| F03 | Implemented | 10px semantic gaps; no tight-list item gap; loose paragraphs retain their gap. Native geometry checks. |
| F04 | Implemented | Nested tight/loose/task-list cases covered by the native spacing test. |
| F05 | Excluded | Operator explicitly retained proportional prose and existing font families. |
| F06 | Verified / fixed | Repeated source spaces retained; native drag-copy, styled Markdown and exact literal-output checks. |
| F07 | Verified | Native adjacent style runs retain punctuation and source spaces without inserted separators. Styled plain/source selection fixture and edges screenshots. |
| F08 | Verified | Literal indentation is source; marker/result gutters are layout. Native copy excludes decorative markers. |
| F09 | Native adaptation | Tabs remain literal source and use the platform shaper in each native text run. No terminal-wide tab grid. See native-tab-metrics.json and the tab policy below. |
| F10 | Fixed | One native literal document preserves small-output blank lines and trailing source bytes. Copy strips one structural newline, rather than all source newlines. Fenced-code plain/source selection also checked. |
| F11 | Verified | CJK, combining accent, NBSP and tabs survive exact native copy. No imitation of model-side normalization. |
| F12 | Verified / adapted | 124-character unbroken native Markdown token wraps and copies without byte loss. Native narrow/wide edge fixtures. Terminal 118+6/58+58+8 breaks remain capture evidence, not proportional-layout targets. |
| F13 | Retained | Markdown syntax is removed. H1 1.5×, H2 1.3×, remaining levels 1.15× sizes retained as requested. |
| F14 | Implemented | H1 bold, italic and underlined; H2–H6 bold. Native sizes unchanged. Corrects the earlier erroneous exclusion. |
| F15 | Implemented / adapted | Native marker and nesting layout retained with tight-list spacing and actual ordered starts. Hanging continuations use toolkit marker measurement, not terminal-cell coordinates. |
| F16 | Implemented / verified | Ordered start preserved in parser, render and source reconstruction. Native 9/100 marker-width test; 9/10 and separate 100-start screenshot fixtures. |
| F17 | Verified | Native soft newline and hard-break rendering retained; hard-break geometry regression check passes. |
| F18 | Implemented | 1px quote rail, italic prose; multiline and nested native fixtures inspected. |
| F19 | Implemented | Inline code uses semantic ink without raised chip chrome; literal repeated spaces preserved. |
| F20 | Verified | Native label-only links retain URI. Actual pointer activation verifies an exact escaped query and fragment; no external URL is opened in the test. |
| F21 | Implemented | Native and fallback code omit language-header/raised-container chrome; mono code remains. |
| F22 | Fixed | Plain, unknown and absent fence languages remain plain. Python/py strings, comments and numbers use existing lexer; Rust keywords retained. Token concatenation preserves exact source. |
| F23 | Implemented | Restrained native table grid and row separators; no box-drawing characters added to copied data. |
| F24 | Implemented | Centered regular-weight headers, source-aligned data, existing 8px/4px cell inset; existing adaptive horizontal-scroll table handles narrow overflow. Native overflow-track geometry check. |
| F25 | Retained deviation | Correct native strikethrough and horizontal rules retained; reproducing CLI literal-markup quirks would regress Markdown support. |
| F26 | Implemented | A completed singleton receives the same compact activity summary as larger groups; a lone running call keeps live details. |
| F27 | Implemented | Counts bold in muted, selectable summaries. Ranges use digits in controlled app-owned summary labels; no provider prose or tool-path parsing. |
| F28 | Verified | Commentary and other non-tool blocks remain grouping boundaries. Core mixed/singleton checks. |
| F29 | Implemented / verified | Call identity and open choices survive running→settled→multi-call transitions. Native pointer regression test. |
| F30 | Verified | Native Name(summary), status marker and disclosure retained; exact multiline input and narrow geometry checks. |
| F31 | Retained adaptation | Results align beneath calls; reads settle compactly; full available content remains in disclosure. Missing line counts are never invented. |
| F32 | Fixed / verified | Small and large input/output disclosures preserve actual spacing. Exact source-copy regression includes blank lines and terminal newline. |
| F33 | Verified | Existing bounded large-output textarea retains exact text, selection and scrolling as output streams. Small output now has one native literal identity. |
| F34 | Retained deviation | Failure previews stay visible inside closed groups; hiding them to match a CLI screenshot would reduce useful failure information. |
| F35 | Verified | Events determine completed/failed/interrupted/rejected outcomes; labels do not overwrite lifecycle facts. |
| F36 | Native adaptation | In-place independent disclosure retained. No separate CLI transcript-view mode: it is a different navigation/composer mode, outside the accepted native disclosure design. |
| F37 | Retained | Explicit native link targets preserved. No clickable destination fabricated from truncated labels or provider text without an actual target. |
| F38 | Implemented / verified | Group and individual state independent. Closing a group overrides inherited leader expansion. Pointer, keyboard and hidden-subject tests retained. |
| F39 | Implemented | Only live activity indicator animates; caption and elapsed/token metadata stay steady. No promotional tip row. |
| F40 | Verified | Only received reasoning is displayed. Short text has no false disclosure; longer supplied details retain native disclosure. |
| F41 | Fixed | Received reasoning appears once in L1; the separate live status uses its factual phase when the supplied headline already appears in history. Completion leaves one historical summary; compact views retain their headline. |
| F42 | Verified | Actual streamed heading/list/fence chunks retain native TextView identity and selected prose through turn completion. |
| F43 | Implemented | One muted Completed · seconds elapsed · observed local HH:MM row. Live observation is persisted once for accepted main/child completion; reopen uses the saved values. No cost or whimsical verb. |
| F44 | Clarified | Turn and tool observations say elapsed. No process-runtime or wait-duration breakdown is invented where events provide none. Captured arrival delays are not performance budgets. |
| F45 | Verified | Historical interruption remains alongside a newly running turn and inspectable tools. Native interrupted fixture plus existing lifecycle/disclosure checks. |
| F46 | Fixed / verified | Per-subject block reading anchor survives detached streaming, disclosure and narrower resize. Root also checked expansion above the reading position; anchor candidate is restricted to visible units. |
| F47 | Implemented / verified | Main and child approvals expose exact selectable command text before answering; repaint retains selection. Approval framebuffer plus native copy/no-submission and existing typed-response/focus checks. No captured account-specific permission options copied. |
| F48 | Verified | Actual wrapped choice/description bounds fit a narrow question island, with non-overlapping hit targets. Single-choice form and focus checks retained. |
| F49 | Retained / verified | Native checkboxes, per-question drafts and explicit Send answer retained. All questions are directly available in the form; an additional CLI review page is unnecessary. |
| F50 | Fixed | Answer notes retain leading/trailing and repeated spaces. Meaningful label suffix survives selection, rejected delivery and retry. |
| F51 | Verified | Async form and draft survive until acknowledgement; rejection permits exact retry. Acknowledged form disappears and Composer draft remains untouched. Historical answer records stay non-interactive. |
| F52 | Verified | Long indented/tabbed Composer draft grows upward from a fixed lower edge; exact draft and visible end caret survive narrow/wide resizing. |
| F53 | Verified | Existing queue/unqueue, draft editing and provider delivery semantics retained. Native production-key queue checks plus core lifecycle suite. |
| F54 | Verified / adapted | Typed and pasted question marks both remain literal in Ferrite. Actual keyboard/paste test verifies no submission; no CLI-only shortcut added. |
| F55 | Retained adaptation | Existing native command/model/file menus use actual available items. Current picker, navigation and focus checks retained; captured account inventory not copied. |
| F56 | Verified | Transcript changes remain within L1/rich presentation; native Composer, navigation and semantic zoom boundaries retained. Full native suite covers other levels. |
| F57 | Native adaptation | Existing semantic Ferrite body/secondary/status/link/code palette retained. Native captures verify emphasis. Terminal RGB/dim multipliers cannot be established from reconstructed palette mappings. |
| F58 | Excluded | CLI export, resume instructions, alternate-screen and tmux machinery do not belong in ordinary Ferrite transcripts. No new export workflow requested. |
| F59 | Excluded | CLI trust/startup, warnings and harness-failure rows are not assistant answers. Native lifecycle and connection UI remains authoritative. |
| F60 | Verified | Independent native character/word/line/cross-block selection, hidden-detail exclusion, eviction, inactive-pane and Unicode checks retained. No claim of native CLI mouse-selection parity. |

## What was intentionally not done, and why

- **Terminal font and heading-size imitation (F05/F13):** explicitly rejected by
  the operator. Native proportional fonts and heading scales remain.
- **Terminal cell/tab/wrap coordinates (F09/F12/F15):** native text runs use local
  platform shaping. An 8-column global terminal grid conflicts with the retained
  native geometry. Source tabs/spaces are preserved, not converted into padding.
- **CLI rendering quirks and hidden failures (F25/F34/F35):** would regress correct
  Markdown or conceal actual outcomes.
- **Separate full-screen transcript viewer and review page (F36/F49):** native
  in-place disclosure and explicit answer submission already serve these tasks;
  CLI viewer controls are a different navigation mode.
- **Invented path targets or timing breakdowns (F31/F37/F43/F44):** unavailable
  facts cannot be reconstructed reliably. Old logs without an observed completion
  timestamp do not acquire a fabricated one; elapsed is not process runtime.
- **CLI shortcuts, account inventories, promotional/startup/export/tmux chrome
  (F39/F54/F55/F58/F59):** these belong to CLI/account workflows, not Ferrite's
  accepted transcript design.
- **Exact Terminal RGB and CLI mouse hitbox parity (F57/F60):** original evidence
  contains emitted styles/cells and reconstructed PNGs, not native Terminal font,
  palette or OS hitbox measurements. Ferrite's native behavior is checked separately.

Windows execution was not available on this macOS host. Existing live-provider
integration tests remain ignored by the default suite because they start real
CLI sessions and depend on credentials/runtime setup. The excluded vendor crate's
standalone suite was not run; its patched renderer is compiled and exercised by
Ferrite's native acceptance tests. No new ignored tests were introduced.

No claim is made that the original CLI recording covers every edit/diff,
authentication, network, compaction, light-theme or attachment variant. Those
missing recordings are limits on CLI comparison, not unimplemented changes in
this 60-entry native transcript register.

## Native artifacts and measurement policy

All current images use the real Cockpit renderer, native GPUI/platform text,
bundled fonts, disposable fixture stores and synthetic provider events. No live
model or operator store is involved. Images are 2× pixels for 720×1400 and
1000×1400 logical windows. They are actual Ferrite framebuffers, not HTML or
Terminal reconstructions. Local completion times and elapsed values reflect the
fixture run; animation stills are sampled, not a frame-perfect timing reference.

| State | Narrow | Wide |
| --- | --- | --- |
| Saved Claude formatting source | [PNG](formatting-narrow.png) | [PNG](formatting-wide.png) |
| Headings, quotes, numbered lists, tables, whitespace and long token | [PNG](edges-narrow.png) | [PNG](edges-wide.png) |
| Live Markdown and received reasoning | [PNG](live-narrow.png) | [PNG](live-wide.png) |
| Wrapped multi-select question | [PNG](decision-narrow.png) | [PNG](decision-wide.png) |
| Exact approval command | [PNG](approval-narrow.png) | [PNG](approval-wide.png) |
| Independently expanded tool details | [PNG](expanded-narrow.png) | [PNG](expanded-wide.png) |
| Historical interruption and new turn | [PNG](interrupted-narrow.png) | [PNG](interrupted-wide.png) |

Tab policy: retain U+0009 in source/copy and delegate its advance to the native
shaper at each text-run origin. Ferrite does not promise a cross-document fixed
number of spaces. [Native tab measurements](native-tab-metrics.json) record the
position of `b` at 12px JetBrains Mono for several literal inputs, independently
of decorative gutters. On this host, `\tb` and `a\tb` place `b` at 28px;
`aaaa\tb` places it at 56px. Eight literal spaces measure 57.6px. Thus the
observed local tab interval is 28px, not eight mono advances. This is a measured
macOS snapshot, not a Windows font contract. Markdown code copy, literal-output copy and Composer tests preserve
the same tab bytes.

The reading anchor is a stable rendered block/run and viewport inset. Reflow may
change line breaks inside that native Markdown run; no custom character-level
scroll engine was introduced. Streaming selection retains the native document
and selected source range. Existing bounded render/cache eviction remains in use.

```sh
cargo run --locked -p ferrite --features visual-reference -- --visual-reference /tmp/ferrite-native-reference
```
