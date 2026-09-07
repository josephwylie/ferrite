# Claude transcript: evidence-to-implementation plan

2026-09-06. Evidence and implementation checklist.

**Operator correction:** retain proportional Markdown prose and existing heading
sizes. F05 and the heading-size change in F13 are excluded. Implementation is
authorized with Terra medium-effort agents, narrowly scoped behavioural tests,
and parent review of both tests and production changes.

## Direction and confidence

Use Claude's presentation approach for both Providers, as accepted in
[ADR 0006](../adr/0007-claude-transcript-presentation.md). The job is to let an
operator following several Threads quickly distinguish answers, routine work,
active work, and Decisions, then inspect details without losing their place.

The earlier conversational summary was not an exhaustive findings inventory.
The evidence contains 71 labelled states, 142 derived images, and 1,589 sampled
raw frames. The space-marked images are alternate views of the same states;
many sampled frames differ only in animation, cursor state, or incremental text.
Those counts are not counts of independent design findings.

This register breaks the evidence into **60 implementation considerations**.
Some are observations already documented, some are newly identified mismatches
with Ferrite, and some are deliberate native adaptations. It is not a claim to
have discovered every detail in every raw repaint. All 71 labelled states are
assigned an implementation disposition in the appendix.

Evidence authority, strongest first:

1. Received source and structured events establish text and execution facts.
2. Captured cells establish character positions, whitespace, and emitted styles.
3. Raw output timestamps establish output arrival, not model-token timing.
4. Reconstructed images make that evidence inspectable. Their Menlo 16px font,
   10px cell pitch, 22px row pitch, palette mapping, and 20px outer padding are
   reconstruction settings, **not measurements of the native Terminal window**.
5. Pinned source findings cover additional branches but are not observed states.

This register records the pre-implementation assessment. Native framebuffer
artifacts and the final implementation disposition are recorded in
[ferrite-transcript-implementation-2026-09-06/README.md](ferrite-transcript-implementation-2026-09-06/README.md).
A registered finding is not automatically a new feature or a passed visual check.

## Evidence key and current implementation

`C/<label>` and `X/<label>` identify Claude and Codex captures in the
[state catalogue](cli-capture-2026-09-06/STATE-CATALOGUE.md). Each entry links its
PNG, space-marked PNG, literal row map and cell JSON. Coordinates are zero-based.
The [measured report](cli-transcript-visual-spec.md) supplies the detailed
observations; [spacing-audit.json](cli-capture-2026-09-06/spacing-audit.json)
retains every ordinary-space run on each nonempty labelled row.

Current owners:

| Owner | Relevant responsibility |
| --- | --- |
| [theme.rs](../../crates/ferrite/src/theme.rs) | 12px body size, 1.55 line height, 10px block gap, 9px tool gutter + 8px gap, 17px result indent; shared palette and fonts |
| [pane.rs](../../crates/ferrite/src/pane.rs) | Transcript layout, prompt blocks, reasoning, tools, progress, disclosures, Decisions |
| [rich.rs](../../crates/ferrite/src/rich.rs) | Cached native Markdown, system UI prose font, foreground, heading scales, table styles, literal output |
| [select.rs](../../crates/ferrite/src/select.rs) | Stable selectable text identities |
| [transcript.rs](../../crates/ferrite-core/src/transcript.rs) | ToolActivity grouping and summary text; semantic blocks |
| [native text node](../../vendor/gpui-base/src/text/node.rs) / [style](../../vendor/gpui-base/src/text/style.rs) | Paragraph/list spacing, heading and quote rendering, list geometry |
| [composer.rs](../../crates/ferrite/src/composer.rs) | Native drafting and input interaction |

Do not spread CLI-specific styling into provider adapters. Adapters establish
facts; the shared native presentation consumes them. Existing event formats,
independent disclosure state, source identity, selection, and scroll behaviour
remain requirements.

## Findings register

Dispositions: **Change** = concrete mismatch to address; **Verify** = relevant
code exists or native behaviour needs measurement; **Adapt** = explicit native
design translation proposed; **Preserve** = existing requirement to retain;
**Exclude** = observed CLI machinery outside this transcript change.

### Geometry and text

| ID | Evidence / finding | Ferrite disposition and acceptance |
| --- | --- | --- |
| F01 | C/02-read-complete: assistant marker at column 0, text and continuations at 2. | **Change.** Establish one message marker/content alignment. Current answer rows lack the explicit shared tool-style gutter. Wrapped text must align with the first text character. |
| F02 | Same frame: user marker and continuation also use a two-cell prefix. | **Adapt.** Align native prompt content with the shared reading column; retain the accepted neutral raised surface. Current prompts have 10px horizontal and 5px vertical padding. |
| F03 | C/12-markdown-120x90: paragraphs/headings have an empty separating row; tight list items do not. | **Change.** Separate semantic block, tight-list, and loose-list spacing. Current BLOCK_GAP is also passed to native list spacing. Update ADR 0003's universal-list-gap statement when implemented. |
| F04 | Same frame, rows 53–56: outer item, nested item, wrapped continuation, next outer item form a contiguous group. | **Change.** Preserve tight-list rhythm through nesting; no extra paragraph margin inside a simple item. Loose lists need a separate new fixture. |
| F05 | Claude uses terminal-cell alignment throughout. Ferrite Markdown explicitly selects FONT_UI while literal/tool content inherits mono. | **Excluded by operator.** Preserve proportional Markdown prose and current font families. |
| F06 | C/12 rows 45 and 75: prose and code retain repeated spaces; code gaps are exactly 2, 3, 4 cells. | **Verify.** No whitespace collapse in shaping/rendering/copy. Do not simulate source spaces with letter-spacing or word-spacing. |
| F07 | C/12 row 51: comma touches the preceding bold word; a single source space precedes italic text. | **Verify.** Adjacent style runs introduce zero additional width. Test punctuation at bold, italic, code and link boundaries. |
| F08 | C/12 row 76: four code indent spaces plus two-cell content gutter. | **Preserve.** Source indentation and layout inset stay separate; selection copies source spaces, not decorative gutters. |
| F09 | C/12 row 77: actual tab lands at column 8 under the recorded default terminal tab stops. | **Adapt.** Declare Ferrite's tab-stop origin and width and verify it. Do not silently assume a native text view's local tab stops match a global terminal grid. |
| F10 | C/12 rows 78–80: blank code line survives; trailing source spaces cannot be distinguished from screen padding by pixels alone. | **Verify.** Preserve blank lines and trailing bytes in source/copy tests. Visual tests alone cannot prove the latter. |
| F11 | C/12 and X/11: wide CJK occupies two cells; combining marks share a cell; Claude's response source already normalized NBSP/accent. | **Preserve.** Never imitate model normalization in Ferrite. Keep received Unicode; measure fallback, wrapping and caret/selection separately. |
| F12 | C/12 and C/14: 124-character token wraps 118+6 and 58+58+8 at 120/60 columns. | **Verify.** Compare these exact breaks in a calibrated fixed-width test harness; separately test actual native Pane widths. No clipping or discarded characters. |

### Markdown

| ID | Evidence / finding | Ferrite disposition and acceptance |
| --- | --- | --- |
| F13 | C/12: heading syntax removed; all headings occupy the terminal's ordinary row height. | **Preserve.** Native Markdown already removes syntax. Operator retained Ferrite's H1 1.5×, H2 1.3×, other headings 1.15× sizing. |
| F14 | C/12: H1 bold/italic/underlined; H2–H6 bold. | **Adapt.** Encode heading emphasis explicitly in the native style, retaining semantic heading levels. Do not infer typography from visible hash marks. |
| F15 | C/12 rows 53–56: outer list text at 4, nested text at 6; continuation at 6. | **Change.** Calibrate marker width, marker gap, nesting increment and hanging indent independently. Native list defaults are not evidence of a match. |
| F16 | Narrow C/18: `10.` continuation begins at 6. | **Verify.** Derive hanging indent from the displayed number width; test 9, 10 and 100. Use Claude's approach for both providers. |
| F17 | C/12 rows 46–49: soft source newline and Markdown hard break both become new rows. | **Verify.** Lock the chosen soft-break behaviour and ensure hard-break syntax spaces do not become extra visible text. |
| F18 | C/12 rows 61–62: dim quote marker, one space, italic text; marker repeats for both lines. | **Adapt.** Use a restrained native quote rail and italic text with measured inset; test multiline/nested quotes rather than assuming a generic border matches. |
| F19 | C/12 row 51: inline code has coloured ink, no surrounding backticks. | **Change.** Current rich style adds a raised background. Evaluate removing chip-like treatment to match the quieter Claude presentation; preserve literal internal spaces. |
| F20 | C/12: links display the label only, with an OSC-8 target. | **Preserve.** Keep the destination in the native link model, not appended inline. Verify interaction/copy and choose legible Ferrite link ink; indexed blue's reconstructed RGB is not authoritative. |
| F21 | C/03 and C/12: fences/language labels are absent from displayed code. | **Adapt.** Reduce code chrome; code stays mono and syntax-aware. Current fallback code blocks show a language label and raised container. Audit both fallback and native Markdown paths. |
| F22 | Known Python receives syntax styling; plain/unknown fences remain plain. | **Preserve.** Retain native highlighting and uncoloured fallback; test identical glyph advances across token boundaries. |
| F23 | C/12 rows 64–70: full table grid and separators between every data row. | **Change.** Current rich.rs removes table/cell borders. Add a restrained native grid; do not draw box-drawing characters as table data. |
| F24 | Same table: centered regular-weight headers, right-aligned numbers, preserved double spaces. | **Change.** Current headers are semibold and cells have 8px/6px padding. Specify alignment, weight and padding independently; source alignment and narrow overflow need fixtures beyond this fitting table. |
| F25 | Claude exposes literal strike tildes and literal `---` in this fixture. | **Preserve.** Keep correct native strikethrough and horizontal rules under ADR 0006; these observed quirks are not required regressions. |

### Tools and disclosure

| ID | Evidence / finding | Ferrite disposition and acceptance |
| --- | --- | --- |
| F26 | C/02 and C/12: completed summaries include both `Read 2 files` and `Read 1 file`. | **Change.** ToolActivity::at_start currently returns None for fewer than two tools. Give a single routine completed tool the same compact-summary presentation without losing its individual identity. |
| F27 | C/02 and C/07: summary is muted; numeric count is bold. | **Change.** Current group summary is one uniformly styled string. Expose style ranges or structured summary parts; do not parse meaning back from formatted text. |
| F28 | C/02: commentary before and answer after the read group remain separate messages. | **Preserve.** Consecutive-tool grouping already stops at non-tool blocks. Never merge across commentary to reduce row count. |
| F29 | C/42-live-tool versus C/06-command-failed: live command detail settles into a compact group. | **Verify.** Keep call IDs and disclosure choices stable through running/result/group transitions, including when a second tool arrives. A static completed screenshot cannot validate this. |
| F30 | C/03: bold Read, adjacent parentheses/path, state-coloured marker. | **Verify.** Current tool summaries already build Name(summary). Check whitespace/style boundaries and truncation; retain a visible native disclosure affordance. |
| F31 | C/03: result has an elbow and extra spacing; successful reads show line counts rather than file contents. | **Adapt.** Keep the result aligned under call content and reads compact. Preserve full available content in disclosure; do not fabricate line counts when unavailable. |
| F32 | C/42: running command preview collapses repeated spaces; approval preserves them. | **Preserve.** Ferrite's exact input/output disclosure must keep the actual spaces. Treat the CLI preview collapse as a presentation quirk, not an instruction to normalize commands. |
| F33 | C/07–08: 40-line output is hidden behind the compact shell summary and exposed in detail. | **Verify.** Existing group and per-call disclosures provide the structure; test long output, omission counts, wrapping and copy. Keep successful work compact. |
| F34 | C/06 versus X/04–05: failure presentation varies; Claude's settled compact group can hide exit 7. | **Preserve.** Ferrite intentionally keeps failure previews visible, including inside closed groups. This approved deviation takes priority over visual imitation. |
| F35 | X/09-approval-rejected: display says Ran despite cancellation. | **Preserve.** Outcome comes from events, never labels. Rejected, interrupted, failed and unavailable must remain distinguishable. |
| F36 | C/03: detailed transcript also adds right-aligned time/model metadata and replaces the Composer with viewer controls. | **Adapt.** Prefer native in-place disclosure for the current scope. A full detailed-view mode would need its own layout/state specification; it is not equivalent to expanding every row. |
| F37 | C/03: file paths carry OSC-8 file targets; label and target can differ. | **Verify.** Preserve safe actionable native path targets where available; never infer them solely from a truncated display label. |
| F38 | Current Ferrite already keeps separate group and individual disclosure IDs. | **Preserve.** Opening a group must not open every tool; streamed updates must not reset either choice or copied text identity. Verify mouse and keyboard routes. |

### Activity and transitions

| ID | Evidence / finding | Ferrite disposition and acceptance |
| --- | --- | --- |
| F39 | C/40 and C/42: live marker/activity word, optional elapsed/token metadata and optional tip row. | **Adapt.** Keep one compact live area. Ferrite currently pulses the entire progress row; consider limiting motion to the activity indicator so metadata remains steady. Promotional tips are excluded. |
| F40 | X/41-live-reasoning-heading: supplied reasoning headline; Claude capture has no readable reasoning body. | **Preserve.** Existing reasoning preview uses received text. Show no invented explanation, placeholder thinking transcript, or disclosure without further text. |
| F41 | X/41 and settled answer: live summary can later appear as historical dim/italic text. | **Verify.** Prevent duplicated live and settled summaries during the handoff; distinguish completion of a text block from completion of the turn. |
| F42 | C/43 and X/43: partial Markdown appears while work remains active. | **Verify.** Replay incremental headings, fences and lists, then completion. Cached parser/source identity must survive streaming and preserve scroll position. |
| F43 | C/02 and C/12: muted completion marker with duration and local completion time. | **Adapt.** Specify one quiet completion recipe backed by actual timing fields. Avoid copying whimsical verbs as if they were provider facts. |
| F44 | Long approval waits are included in some captured elapsed labels. | **Preserve.** Distinguish turn elapsed, wait time and measured process runtime. Do not use capture delays as performance or animation budgets. |
| F45 | C/21 and X/21: historical interruption remains while current status changes. | **Verify.** Historical notices must not make an idle/newly-running turn look interrupted; completed tools remain inspectable. |
| F46 | C/29–30 and X/29–30: history can be browsed independently of the live tail. | **Verify.** Existing follow-tail flag is a starting point. Replay while scrolled up, expanding a tool and resizing; no forced jump or viewport-anchor loss. Long-session eviction needs new evidence. |

### Decisions, Composer and scope boundaries

| ID | Evidence / finding | Ferrite disposition and acceptance |
| --- | --- | --- |
| F47 | C/04 and X/08: approval separates command, explanation, choices and shortcuts. | **Verify.** Use native Decision controls with clear grouping and exact command text. Preserve typed delivery semantics; imitate neither promotional rows nor account-dependent permission options. |
| F48 | C/09–10: single choice label and description occupy separate aligned rows; selection changes appearance without changing content. | **Verify.** Existing native question form needs geometry/focus acceptance with wrapped labels and descriptions. |
| F49 | C/22–24: multiple selection, checked states and explicit answer review. | **Adapt.** Retain native checkboxes and clear submit action; verify multi-question navigation and drafts. A separate review page is a proposed interaction choice, not automatically required by Claude styling. |
| F50 | X/23–25: notes preserve repeated spaces; answered record preserves selected label and note. | **Preserve.** Native free text and settled answer summary must retain the actual answer. Never remove meaningful suffixes or silently normalize spaces. |
| F51 | C/11 and C/25: answered form becomes a historical summary, followed by a separate assistant acknowledgement. | **Verify.** Keep historical answer records non-interactive; retain a submitting form/draft until the adapter acknowledges, per ADR 0003. |
| F52 | C/17–18: multiline Composer grows upward while lower boundary remains anchored. | **Verify.** Existing native Composer needs long-line, indentation, resize and caret visibility checks; do not copy terminal row numbers into application coordinates. |
| F53 | C/20–21 versus X/20–21: queued text is visibly separate; post-interrupt handling differs. | **Preserve.** Make queued/editable/submitted state unambiguous while keeping Ferrite's actual delivery behaviour. Visual similarity must not change when a prompt executes. |
| F54 | C/27–28 and X/27–28: pasted `?` inserts text, typed `?` can open shortcuts. | **Verify.** Test keyboard versus paste separately. Do not hijack literal pasted content to reproduce a shortcut. |
| F55 | C/15–16, C/26 and X equivalents: slash/model/file menus have selection and account-specific content. | **Adapt.** Keep native menus and Ferrite's actual available commands/models/files. Match hierarchy and spacing, not captured account inventory. |
| F56 | C/01 and narrow drafts: Composer/footer surfaces remain distinct from history. | **Adapt.** Keep existing native chrome and semantic zoom boundaries. Transcript tokens must not accidentally restyle global navigation or L2/L3. |
| F57 | Recorded styles include indexed colours, truecolour, dim, bold and per-character animation spans. | **Adapt.** Define semantic body/secondary/status/link/code colours. Exact emitted colour indices are known; their native Terminal RGB and dim multiplier are not. |
| F58 | C/31-export-scrollback uses terminal export; C/32 and X/31 include CLI resume output and tmux death text. | **Exclude.** No tmux status, terminal resume instructions, or alternate-screen mechanics in ordinary Ferrite answers. Native export, if added, is a separate feature. |
| F59 | X/00–01: MCP startup and workspace trust appear before task work. | **Exclude** from transcript styling scope. Use session setup/Decision semantics where Ferrite supports them; do not reproduce this installation's startup inventory. |
| F60 | No native mouse-selection or OS hitbox evidence was captured. | **Verify.** Existing native selection is an independent acceptance requirement: word/paragraph selection, cross-block copy, hidden-detail exclusion, inactive-pane isolation and Unicode selection. Never claim CLI selection parity from these images. |

## Implementation sequence

### 1. Build a native reference fixture before changing the theme

Use the saved model Markdown verbatim and construct deterministic semantic
events for read, shell success/failure, permission, question and interruption.
Reuse existing GPUI test views and replay seams; do not create a second HTML
transcript renderer or make network model calls during visual tests.

Provide compact/expanded and running/completed variants at three content widths
corresponding to 60, 80 and 120 measured monospace advances, plus actual narrow
and wide native Panes. Freeze clocks/animation only for deterministic stills;
retain a separate timed transition replay. Save native screenshots and bounds.

This first slice establishes a Ferrite baseline and demonstrates what current
code actually draws. It also validates which source-level concerns reproduce.

### 2. Establish typography, gutter and vertical rhythm

Address F01–17 and F57 in theme.rs, rich.rs and the transcript wrapper. Use
separate tokens for message gutter, nesting step, paragraph gap, tight-list gap,
loose-list gap and tool detail inset. Keep decorative gutters out of text data.
Resolve the existing ADR 0003 gap rule alongside the implementation.

Use the existing native tool gutter as the shared message-content alignment,
with tight lists without an extra inter-item gap. Retain proportional prose,
heading sizes and surrounding app typography as instructed by the operator.
The reconstructed PNG settings are not a pixel specification to paste in.

Acceptance: repeated spaces, style boundaries, wrapping, headings, nested and
multi-digit lists; measured first-line/continuation alignment; source/copy
round-trip assertions for invisible whitespace.

### 3. Finish Markdown presentation

Address F18–25: quiet code, quote treatment, table grid and alignment, link
presentation. Configure native styles first. Extend the existing vendor patch
only where the API lacks a needed control, keeping the patch isolated and
documented. Preserve correct Markdown behaviours despite CLI quirks.

Acceptance: the exact saved formatting fixture, unknown-language code, fitting
and overflowing tables, nested quotes/lists, narrow wrapping and selection.

### 4. Finish tool presentation and lifecycle

Address F26–38: singleton summaries, emphasized counts, compact success,
independent disclosure, visible failures and literal details. Keep group
membership and call identity separate from display. Avoid semantic inference
from shell command spelling or past-tense summary labels.

Acceptance: single read, two reads, mixed tools, commentary between calls,
running-to-complete, second call arriving after manual expansion, failure in a
closed group, unavailable result, long output and exact copied command/output.

### 5. Integrate live work, Decisions and Composer boundaries

Address F39–56 and F60. Keep the accepted provider delivery distinction. Verify
progress-to-answer handoff, completion metadata, queue/interrupt handling,
question submission acknowledgement, draft retention, keyboard/paste behaviour
and scroll anchors. Presentation changes should not require provider execution
changes unless an actually missing fact is identified.

### 6. Accept against evidence, including intentional deviations

Each change links its finding IDs, reference states, native before/after output
and checks. Compare geometry, text, styles and transitions separately. A global
pixel-diff threshold across unlike renderers/fonts would be misleading.

Use meaningful GPUI geometry/selection tests and core lifecycle tests; then a
batched native visual pass, one repair batch and one confirmation pass. Verify
macOS and Windows font shaping when those environments are available; record
any platform not run. No endless aesthetic tuning loop.

Done means every in-scope requirement is implemented and checked or has a
documented intentional deviation. It does not mean all CLIs' possible states
have been discovered.

## Missing evidence to collect deliberately

High priority before claiming broad transcript parity:

- Edits/diffs in a disposable fixture directory: create, modify, delete, rename,
  multiline changes and failed edit. The existing recordings exercised no edits.
- Empty output, very long paths, loose/task lists, nested quotes, overflowing
  tables, more numbered-list widths, emoji ZWJ/flags, RTL/mixed-direction text.
- Long history and eviction, resize while scrolled away from the tail, rapid
  tool completion, resume/restart and streaming across disclosure changes.
- Subagent activity and Decisions: materially important for Ferrite's Activity
  model, absent from this CLI fixture run.
- Available reasoning-display configurations. The absence of a Claude thinking
  body in this run is not evidence about every configuration.
- Provider/network errors and compaction using safe synthetic event fixtures
  where a real capture would require disruptive or expensive actions. Label
  these as synthetic, not observed CLI presentation.

Light themes, authentication failure, MCP permission variants, attachments and
native CLI selection remain outside the current evidence. Capture only the
states relevant to the next implementation slice; do not delay known typography
and grouping fixes while claiming an unattainable universal inventory.

## Labelled-state coverage

The appendix below assigns every labelled capture to register IDs. Assignment
means the state has a documented disposition, not that every pixel or every
transition into it has passed native acceptance.

| Provider | State and cell evidence | Findings / disposition |
| --- | --- | --- |
| claude | [01-idle](cli-capture-2026-09-06/claude/rendered/01-idle-cells.json) | F02, F52, F56–57 |
| claude | [02-read-complete](cli-capture-2026-09-06/claude/rendered/02-read-complete-cells.json) | F01–08, F13, F21–22, F26–31, F43 |
| claude | [03-detailed-transcript](cli-capture-2026-09-06/claude/rendered/03-detailed-transcript-cells.json) | F21–22, F30–31, F36–38, F46 |
| claude | [04-command-approval](cli-capture-2026-09-06/claude/rendered/04-command-approval-cells.json) | F32, F44, F47 |
| claude | [05-command-running](cli-capture-2026-09-06/claude/rendered/05-command-running-cells.json) | F29–32, F39, F44 |
| claude | [06-command-failed](cli-capture-2026-09-06/claude/rendered/06-command-failed-cells.json) | F29, F34, F43–44 |
| claude | [07-output-collapsed](cli-capture-2026-09-06/claude/rendered/07-output-collapsed-cells.json) | F26–27, F33 |
| claude | [08-long-output-detailed](cli-capture-2026-09-06/claude/rendered/08-long-output-detailed-cells.json) | F30–33, F36–38 |
| claude | [09-question](cli-capture-2026-09-06/claude/rendered/09-question-cells.json) | F47–48 |
| claude | [10-question-selected](cli-capture-2026-09-06/claude/rendered/10-question-selected-cells.json) | F48 |
| claude | [11-question-answered](cli-capture-2026-09-06/claude/rendered/11-question-answered-cells.json) | F51 |
| claude | [12-markdown-120x90](cli-capture-2026-09-06/claude/rendered/12-markdown-120x90-cells.json) | F03–25, F26–27, F43 |
| claude | [13-markdown-80x90](cli-capture-2026-09-06/claude/rendered/13-markdown-80x90-cells.json) | F03–25 |
| claude | [14-markdown-60x90](cli-capture-2026-09-06/claude/rendered/14-markdown-60x90-cells.json) | F03–25 |
| claude | [15-slash-menu](cli-capture-2026-09-06/claude/rendered/15-slash-menu-cells.json) | F55 |
| claude | [16-model-menu](cli-capture-2026-09-06/claude/rendered/16-model-menu-cells.json) | F55 |
| claude | [17-multiline-draft](cli-capture-2026-09-06/claude/rendered/17-multiline-draft-cells.json) | F06, F08–11, F52 |
| claude | [18-multiline-draft-60](cli-capture-2026-09-06/claude/rendered/18-multiline-draft-60-cells.json) | F06, F08–12, F16, F52 |
| claude | [19-draft-cleared](cli-capture-2026-09-06/claude/rendered/19-draft-cleared-cells.json) | F52, F56 |
| claude | [20-queued](cli-capture-2026-09-06/claude/rendered/20-queued-cells.json) | F53 |
| claude | [21-interrupted](cli-capture-2026-09-06/claude/rendered/21-interrupted-cells.json) | F35, F45, F53 |
| claude | [22-multiselect](cli-capture-2026-09-06/claude/rendered/22-multiselect-cells.json) | F49 |
| claude | [23-multiselect-checked](cli-capture-2026-09-06/claude/rendered/23-multiselect-checked-cells.json) | F49 |
| claude | [24-multiselect-review](cli-capture-2026-09-06/claude/rendered/24-multiselect-review-cells.json) | F49 |
| claude | [25-multiselect-answered](cli-capture-2026-09-06/claude/rendered/25-multiselect-answered-cells.json) | F49, F51 |
| claude | [26-file-completion](cli-capture-2026-09-06/claude/rendered/26-file-completion-cells.json) | F55 |
| claude | [27-pasted-question-mark](cli-capture-2026-09-06/claude/rendered/27-pasted-question-mark-cells.json) | F54 |
| claude | [28-shortcut-key](cli-capture-2026-09-06/claude/rendered/28-shortcut-key-cells.json) | F54 |
| claude | [29-transcript-top](cli-capture-2026-09-06/claude/rendered/29-transcript-top-cells.json) | F36, F46 |
| claude | [30-transcript-page](cli-capture-2026-09-06/claude/rendered/30-transcript-page-cells.json) | F36, F46 |
| claude | [31-export-scrollback](cli-capture-2026-09-06/claude/rendered/31-export-scrollback-cells.json) | F58 |
| claude | [32-exit](cli-capture-2026-09-06/claude/rendered/32-exit-cells.json) | F58 |
| claude | [40-working-spinner](cli-capture-2026-09-06/claude/rendered/40-working-spinner-cells.json) | F39–40, F44, F57 |
| claude | [42-live-tool](cli-capture-2026-09-06/claude/rendered/42-live-tool-cells.json) | F29, F32, F39, F44 |
| claude | [43-markdown-first-paint](cli-capture-2026-09-06/claude/rendered/43-markdown-first-paint-cells.json) | F13–25, F42 |
| codex | [00-mcp-startup](cli-capture-2026-09-06/codex/rendered/00-mcp-startup-cells.json) | F59 |
| codex | [01-workspace-trust](cli-capture-2026-09-06/codex/rendered/01-workspace-trust-cells.json) | F47, F59 |
| codex | [02-read-complete](cli-capture-2026-09-06/codex/rendered/02-read-complete-cells.json) | F01–08, F13, F21–22, F26–31, F43 |
| codex | [03-detailed-transcript](cli-capture-2026-09-06/codex/rendered/03-detailed-transcript-cells.json) | F21–22, F30–31, F36–38, F46 |
| codex | [04-command-failed](cli-capture-2026-09-06/codex/rendered/04-command-failed-cells.json) | F34–35 |
| codex | [05-failure-detailed](cli-capture-2026-09-06/codex/rendered/05-failure-detailed-cells.json) | F33–35, F38 |
| codex | [06-output-truncated](cli-capture-2026-09-06/codex/rendered/06-output-truncated-cells.json) | F33 |
| codex | [07-long-output-detailed](cli-capture-2026-09-06/codex/rendered/07-long-output-detailed-cells.json) | F33, F36–38 |
| codex | [08-command-approval](cli-capture-2026-09-06/codex/rendered/08-command-approval-cells.json) | F44, F47 |
| codex | [09-approval-rejected](cli-capture-2026-09-06/codex/rendered/09-approval-rejected-cells.json) | F35, F45, F47 |
| codex | [10-markdown-120](cli-capture-2026-09-06/codex/rendered/10-markdown-120-cells.json) | F03–25 |
| codex | [11-markdown-120x90](cli-capture-2026-09-06/codex/rendered/11-markdown-120x90-cells.json) | F03–25 |
| codex | [12-markdown-80x90](cli-capture-2026-09-06/codex/rendered/12-markdown-80x90-cells.json) | F03–25 |
| codex | [13-markdown-60x90](cli-capture-2026-09-06/codex/rendered/13-markdown-60x90-cells.json) | F03–25 |
| codex | [14-slash-menu](cli-capture-2026-09-06/codex/rendered/14-slash-menu-cells.json) | F55 |
| codex | [15-model-menu](cli-capture-2026-09-06/codex/rendered/15-model-menu-cells.json) | F55 |
| codex | [16-multiline-draft](cli-capture-2026-09-06/codex/rendered/16-multiline-draft-cells.json) | F06, F08–11, F52 |
| codex | [17-multiline-draft-60](cli-capture-2026-09-06/codex/rendered/17-multiline-draft-60-cells.json) | F06, F08–12, F16, F52 |
| codex | [18-draft-cleared](cli-capture-2026-09-06/codex/rendered/18-draft-cleared-cells.json) | F52, F56 |
| codex | [19-working-background](cli-capture-2026-09-06/codex/rendered/19-working-background-cells.json) | F39–41, F44 |
| codex | [20-queued](cli-capture-2026-09-06/codex/rendered/20-queued-cells.json) | F53 |
| codex | [21-interrupted](cli-capture-2026-09-06/codex/rendered/21-interrupted-cells.json) | F35, F45, F53 |
| codex | [22-plan-mode](cli-capture-2026-09-06/codex/rendered/22-plan-mode-cells.json) | F40, F55 |
| codex | [23-question](cli-capture-2026-09-06/codex/rendered/23-question-cells.json) | F47–48, F50 |
| codex | [24-question-notes](cli-capture-2026-09-06/codex/rendered/24-question-notes-cells.json) | F50 |
| codex | [25-question-answered](cli-capture-2026-09-06/codex/rendered/25-question-answered-cells.json) | F50–51 |
| codex | [26-file-completion](cli-capture-2026-09-06/codex/rendered/26-file-completion-cells.json) | F55 |
| codex | [27-pasted-question-mark](cli-capture-2026-09-06/codex/rendered/27-pasted-question-mark-cells.json) | F54 |
| codex | [28-shortcut-key](cli-capture-2026-09-06/codex/rendered/28-shortcut-key-cells.json) | F54 |
| codex | [29-transcript-top](cli-capture-2026-09-06/codex/rendered/29-transcript-top-cells.json) | F36, F46 |
| codex | [30-transcript-page](cli-capture-2026-09-06/codex/rendered/30-transcript-page-cells.json) | F36, F46 |
| codex | [31-exit](cli-capture-2026-09-06/codex/rendered/31-exit-cells.json) | F58 |
| codex | [40-working-spinner](cli-capture-2026-09-06/codex/rendered/40-working-spinner-cells.json) | F39–40, F44, F57 |
| codex | [41-live-reasoning-heading](cli-capture-2026-09-06/codex/rendered/41-live-reasoning-heading-cells.json) | F40–41 |
| codex | [42-answer-first-observed](cli-capture-2026-09-06/codex/rendered/42-answer-first-observed-cells.json) | F41–42 |
| codex | [43-markdown-first-paint](cli-capture-2026-09-06/codex/rendered/43-markdown-first-paint-cells.json) | F13–25, F42 |
