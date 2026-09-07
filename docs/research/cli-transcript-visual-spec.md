# Claude and Codex: measured CLI transcript presentation

Captured 2026-09-06, Australia/Adelaide. Claude Code **2.1.263**, Codex CLI
**0.153.4**. These are observations of the installed CLIs, not mockups.

**Product direction, 2026-09-06:** the operator chose Claude's design approach
for both Providers. [ADR 0006](../adr/0007-claude-transcript-presentation.md)
records that decision. The comparisons below preserve the observed differences;
they do not require separate Claude and Codex visual designs in Ferrite.

The [implementation plan](claude-transcript-implementation-plan.md) maps these
observations to 60 implementation considerations, current native code, a staged
delivery sequence, missing evidence and all 71 labelled states.

Start with the [interactive frame viewer](cli-capture-2026-09-06/viewer.html).
Turn on **Show spaces**, optionally **Grid**, then hover individual cells.
The [state catalogue](cli-capture-2026-09-06/STATE-CATALOGUE.md) links every
labelled image, whitespace image, literal text grid, and cell/style record.
The [machine spacing audit](cli-capture-2026-09-06/spacing-audit.json) enumerates
each nonempty row's leading spaces and every interword run of ordinary spaces.

## What “exact” means here

Every coordinate below is a **zero-based terminal cell coordinate**. A quoted
ordinary space means U+0020; `NBSP` means U+00A0. `·` in whitespace diagrams
represents one ordinary space, not a middle dot present in the CLI.

The continuous, bidirectional PTY recordings preserve original bytes and
timestamps. tmux 3.7b parsed those bytes into screen grids, sampled nominally
every 100 ms; unchanged grids were omitted. Cursor metadata can create another
sample even without a text change. Raw recordings retain output occurring
between samples. This is **not** a guarantee that every intermediate repaint
has its own PNG, nor that every provider feature was exercised.

Grid snapshots retain foreground/background tokens, bold, dim, italic,
underline, strike, inverse, blink, conceal, OSC-8 links, Unicode text and cell
widths. Some trailing spaces are explicit rendered cells; others are empty
terminal padding. Identical blank cells do not reveal their original cause.
The raw stream and saved model Markdown are needed for that distinction.

Images use a declared reconstruction: Menlo 16 px, 10 px cell pitch, 22 px
row pitch, 20 px outside padding. **These pixel values are our renderer's
choices, not measurements of Apple Terminal.** Font fallback, anti-aliasing,
dim intensity, cursor outline and ANSI colours 0–15 likewise belong to the
reconstruction. The cell geometry and emitted colour indices remain inspectable
independently. Font assets/provenance are documented in the [capture README](cli-capture-2026-09-06/README.md).

## Environment and scope

| Property | Captured value |
| --- | --- |
| Terminal parser | tmux 3.7b, isolated socket, no user tmux config |
| Terminal advertisement | `TERM=tmux-256color`, `COLORTERM=truecolor` |
| Default foreground / background | `#d8dee9` / `#17191f` |
| Main viewport | 120 columns × 44 rows |
| Additional viewports | 120×90, 80×90, 60×90, 60×44 |
| Claude model | Fable 5.1, high effort, inherited account selection |
| Codex model | gpt-5.6-terra, low; `/plan` changed this Session to xhigh |
| Claude rendering | Fullscreen alternate screen; detailed viewer; scrollback export |
| Codex rendering | Normal screen/scrollback; alternate screen for detailed transcript |
| Claude execution | Manual permissions; Read/Glob/Grep/Bash/AskUserQuestion only; hooks disabled for invocation; no MCP configuration |
| Codex execution | Read-only shell sandbox, on-request approvals, web search disabled |
| Tasks | Synthetic file reads; printing; deliberate exit 7; short waits; disposable questions; exact Markdown reproduction |

The standard CLIs inherited account authentication and some user configuration.
Consequently the custom Codex status line and Claude remote-control link are
**this installation's configuration**, not universal product defaults. The
Codex startup capture shows existing MCP servers loading; no external tool was
requested for the fixture tasks. Neither Session edited Ferrite. Both exited
normally. The recorder's isolated tmux server was stopped.

An initial probe inherited `NO_COLOR=1`; it was stopped and kept separately.
The primary evidence explicitly removed that variable. Codex's recorded terminal
input contains both OSC 10 and OSC 11 responses: palette detection really did
receive the declared foreground/background. Neither CLI changed tab stops in
the recorded output. HT in the exported grids is therefore expanded to the
next default eight-column stop.

## Stable block geometry

| Element | Claude | Codex |
| --- | --- | --- |
| User marker | `❯` U+276F at column 0 | `›` U+203A at column 0 |
| Submitted user prefix | `❯ `; first content column 2 | `› `; first content column 2 |
| Submitted user continuation | Two ordinary spaces | Two ordinary spaces |
| User fill | Indexed background 237; foreground 231; marker 239 | RGB `#323439`; marker bold + dim |
| User outer blank rows | A separating blank row; no Codex-style filled padding row in the inspected block | One background-filled blank row above and below |
| Assistant marker | `⏺` U+23FA, indexed foreground 231 | `•` U+2022, dim |
| Assistant first text | Column 2 | Column 2 |
| Assistant continuation | Column 2, plus source/list indentation | Column 2, plus source/list indentation |
| Paragraph separation | One empty row in the fixture | One empty row in the fixture |
| First answer heading | Shares the assistant marker's row | Shares the assistant marker's row |
| Normal body | Default foreground; not dim | Default foreground; not dim |

Evidence: [Claude read completion](cli-capture-2026-09-06/claude/rendered/02-read-complete.png),
[Codex read completion](cli-capture-2026-09-06/codex/rendered/02-read-complete.png).

The marker column and content column are distinct styled cells. Do not insert
an extra gap when styles change. In `**Bold**, *italic*`, the comma immediately
follows the last bold letter; only the following ordinary space separates the
next word. The exported cell records preserve this boundary exactly.

## Word spacing and Unicode

The code fixture line occupies these columns in both large Markdown captures:

```text
column  00 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25
cell     ·  ·  o  n  e  ·  ·  t  w  o  ·  ·  ·  t  h  r  e  e  ·  ·  ·  ·  f  o  u  r
```

Thus `one` starts at 2, `two` at 7, `three` at 13, `four` at 22. The gaps
are exactly 2, 3 and 4 cells. They are not CSS word spacing. Single/double/triple
spaces in ordinary prose also survive in the inspected response.

Evidence: Claude `12-markdown-120x90`, row **75**; Codex
`11-markdown-120x90`, row **74**. All subsequent row references in this section
use those two images, respectively.

| Fixture | Claude | Codex |
| --- | --- | --- |
| Four leading spaces in code | Four source spaces + two-column answer gutter; text at 6 | Same; text at 6 |
| Actual tab in code | Text at 8; exported cells contain spaces | Text at 8; tmux export contains HT after the two-column gutter |
| Blank code line | Blank row retained | Blank row retained |
| Two trailing source spaces | Present in saved model Markdown; visually indistinguishable from remaining terminal padding | Same |
| `界` | Two terminal columns | Two terminal columns |
| `e` + combining acute | Model returned composed `é`; visible one cell | Model retained U+0065 U+0301; both code points occupy one cell |
| NBSP between alpha/beta | Model replaced it with ordinary space | Model retained U+00A0 |
| 124-character unbroken code token, width 120 | Displayed as 118 + 6 characters | Same |
| Same token, width 60 | Displayed as 58 + 58 + 8 characters | Same |

The submitted [source fixture](cli-capture-2026-09-06/fixture/formatting.md),
[Claude response source](cli-capture-2026-09-06/model-source/claude-formatting.md),
and [Codex response source](cli-capture-2026-09-06/model-source/codex-formatting.md)
are retained with exact diffs. This is how we verified that Claude's NBSP and
accent changes occurred **before** Markdown presentation; attributing those
changes solely to the renderer would be wrong. Both omitted the final newline
after the closing code fence.

Claude's *draft Composer* also displayed the pasted decomposed accent as `é`,
while Codex's draft retained the combining sequence. Those are separately
observed input-path behaviours, not proof of the model's normalization cause.

## Markdown presentation

Compare [Claude 120×90](cli-capture-2026-09-06/claude/rendered/12-markdown-120x90.png)
with [Codex 120×90](cli-capture-2026-09-06/codex/rendered/11-markdown-120x90.png).
Each response was requested from the same fixture; source differences are above.

| Structure | Claude, observed | Codex, observed |
| --- | --- | --- |
| H1 | Removes `# `; bold, italic, underlined | Retains `# `; bold, underlined |
| H2 | Removes hashes; bold | Retains `## `; bold |
| H3 | Removes hashes; bold | Retains `### `; bold, italic |
| H4–H6 | Removes hashes; bold | Retains hashes; italic |
| Adjacent headings | One blank row between each | One blank row between each |
| Strong/emphasis | Bold / italic | Bold / italic |
| Strikethrough | Literal `~~strike~~`, no strike attribute | Tildes removed; strike attribute set |
| Inline code | Backticks removed; foreground index 153 | Backticks removed; foreground index 6 |
| Web link | Label only, index 12, OSC-8 target, no underline in captured style | Label plus parenthesized URL; index 6, underline, OSC-8 targets |
| Soft source newline | New display row | New display row |
| Markdown hard break | New display row; two trailing syntax spaces not shown as extra text | Same |
| Horizontal rule source `---` | Literal three hyphens | Three em dashes `———` |
| Code fence | Delimiters and language identifier absent in display | Same |
| Known Python code | ANSI palette syntax colours | Truecolour syntax spans |
| Unknown/plain text fence | Literal uncoloured text | Literal uncoloured text |

### Lists

The outer unordered marker is `- ` at columns 2–3 in both. First item text starts
at 4. The **nested** marker starts at column **4** in Claude and **6** in Codex;
nested text starts at 6 and 8. Wrapped nested continuation starts at 6 and 8.
The Codex fixture also inserts a blank row before the second outer item where
Claude does not. This difference is visible with identical list Markdown.

For `10. ` at 60 columns, Claude's continuation begins at column **6**;
Codex's begins at **5** in the captured narrow rendering. Keep that difference
in the evidence; Ferrite's shared presentation follows Claude's hanging-indent
approach under ADR 0006 rather than selecting indentation by Provider.
Evidence: `18-multiline-draft-60` / `17-multiline-draft-60`, whose upper transcript
retains the narrow Markdown layout.

### Blockquotes

Claude: two spaces, dim `▎` U+258E at column 2, ordinary space at 3, italic
text at 4. Both quoted lines repeat the marker. Codex: two spaces then `> `,
text at 4; the whole quote line is foreground index 2. Neither is equivalent
to adding a generic coloured left border to ordinary body text.

### Tables

Claude emits a complete box grid: `┌ ┬ ┐`, `│`, `├ ┼ ┤`, `└ ┴ ┘`.
Every data row has an intervening horizontal separator; all are default
foreground in this capture. The box starts at column 2. Header text is centered
within its cells, including the source's nominally left-aligned Name/Note
columns. Numeric values align right. Ordinary double spaces survive inside
`two  spaces`.

Codex emits no outer box or vertical separators. Header cells are bold
`#f9e2af`. A heavy `━` rule follows the header; a light `─` rule separates the
two data rows. Rule colour is `#3d4047`. Column regions are separated by gaps;
the first header/data content starts at column 3. Both tables fit unchanged in
the inspected 60-column viewport; this fixture does not exercise overflow-to-
record transformations.

## Tool calls, grouping and output

### Claude compact and detailed views

After the two reads, compact mode shows `  Read 2 files`: leading two spaces,
muted index 246 text, count `2` bold. Neither individual call nor its output
occupies a separate compact row. After shell completion, the same structural
slot reads `  Ran 1 shell command`, also with a bold count.

This includes the intentional exit-7 command: its settled compact group does
not itself expose a red failure preview. The assistant's following sentence
explains exit 7. Do not generalize this to every error class.

During execution, a labelled command has a marker in column 0, text at 2,
and a detail row with `  ⎿  $ `. Wrapped command text begins at column 5.
The compact running command preview **collapses repeated spaces** inside the
quoted shell text: `first  second   third` appears as `first second third`.
The approval panel retains the exact spaces. This is a verified presentation
difference, not a changed command.

Detailed transcript (`Ctrl+O`) replaces the group with individual calls:

```text
⏺ Read(/absolute/path/sample.txt)
  ⎿ ⍽Read 8 lines
```

Here `⍽` denotes the NBSP following the ordinary space after `⎿`; inspect cells
to avoid silently replacing it. The successful marker uses foreground 114;
`Read` is bold; parentheses are adjacent to the name/path, without extra gaps.
The path is an OSC-8 `file://` link. Read outputs show line counts, not full file
contents. Shell calls show `Bash(...)` and retained output; interrupted shell
markers use index 211 in the exported detailed transcript.

Detailed assistant messages gain a separate right-aligned metadata row. In the
120-column read capture, time begins at column 87 and model information follows
one ordinary space. Content remains left-aligned at column 2. The bottom
Composer is replaced by a rule and viewer shortcuts; `verbose` sits at the right.
The full-screen viewer's `[` command exports the transcript to normal terminal
scrollback; this export has its own ANSI span segmentation.

### Codex compact and detailed views

Read-only exploration becomes two rows:

```text
• Explored
  └ Read sample.txt, sample.py
```

`Explored` is bold. Bullet/tree glyphs and the comma-space separator are dim;
`Read` is cyan (index 6); filenames use default foreground. The tree prefix is
two spaces, `└`, one space. It is a four-column gutter.

Ordinary shell results use `• Ran COMMAND`. The bullet is bold green (index 2)
for the successful print and bold red (index 1) for exit 7. `Ran` is bold;
the command has shell syntax colouring. Command continuation is `  │ `;
output begins `  └ ` and continues at column 4. Output text is dim.

For forty output rows, the compact preview is **five rows**:

```text
  └ row 01  alpha   beta    gamma
    row 02  alpha   beta    gamma
    … +36 lines (ctrl + t to view transcript)
    row 39  alpha   beta    gamma
    row 40  alpha   beta    gamma
```

The literal spaces inside each printed row survive. The omission count counts
the 36 missing source lines; the omission message itself occupies a display row.
Evidence: [truncation](cli-capture-2026-09-06/codex/rendered/06-output-truncated.png).

`Ctrl+T` uses a completely different tool representation: magenta `$ ` at
column 0, syntax-coloured command at 2, command continuation at 4, and raw
retained output starting at column 0. Success ends with bold green `✓`, then
dim ` • 0ms` in the initial read example. Exit 7 ends with bold red `✗`,
` (7)`, then dim ` • 7.92s` in the later detailed capture.

The background-wait example has `• Waited for background terminal` in bold,
followed by dim ` · COMMAND`; command continuation here is not given the ordinary
`  │ ` tree. Its settled compact output retained only `finished fixture`.
Similarly, the failed-command detail retained `fixture stderr`; the earlier
stdout was visible during activity and referenced by the final answer. Do not
assume the final tool card reconstructs all earlier streamed output.

The overlay starts with dim `/ T R A N S C R I P T / / …` and a bottom rule with
scroll percentage (`100%` at the bottom, `15%` in the inspected page). Two footer
rows provide arrow/page/home/end navigation and quit/edit shortcuts. The overlay
uses the terminal alternate screen; returning restores the normal view.

## Reasoning, streaming and timing

Claude's visible active state uses an animated star-like marker, a changing
activity word ending in U+2026, and sometimes parenthesized seconds plus `↓`
token count. Accent spans include indices 174 and 180; time/count uses 246.
An optional tip is another row starting `  ⎿ `; it can occupy space even when
there is no readable reasoning paragraph. The high-effort indicator can sit
right-aligned above the Composer. In these tasks, no ordinary readable thinking
body appeared in compact or detailed views. This does not establish absence of
model reasoning or the behaviour of `--thinking-display summarized`.

After completion, Claude shows a muted star and phrases such as `Worked for
7s · done 8:53 pm` or `Baked for 10s · done 9:02 pm`. The activity word changes
between turns. The clock is local wall time; duration is separate. Tool elapsed
labels can include time spent waiting for approval, so they are not equivalent
to process runtime. The approval was deliberately left open while the renderer
was being built; long captured waits are not normal model-latency measurements.

Codex has a live shimmer assembled from per-character RGB spans, not a single
flat colour. It displays a heading, then dim `(12s • esc to interrupt)`. A
reasoning summary heading replaced `Working`: `Confirming heading and formatting
requirements`. The exact active example is sampled source frame **84**.
After completion, the same heading appears as a separate dim/bold/italic
transcript row before the final answer. Its first bullet has different styling
from the text. Other turns show headings such as `Planning exact whitespace
preservation`. These are provider-visible summaries, not a reconstruction of
private reasoning.

During the waiting command, status also included ` · 1 background terminal
running · /ps to view…`; its tail truncated at the right edge. The installed
status line independently changed through Working, Waiting and Ready.

Both providers exposed partial Markdown before completion. Claude sampled frame
**742** contains headings and the beginning of prose with `esc to interrupt`
still in the footer; frame **743**, about 106 ms later, contains the completed
tail and completion stamp. Codex frames **370–372** contain successively more
Markdown while its status remains Working. Do not infer token timing from this
100 ms sampling; raw output chunk timestamps provide finer arrival evidence,
but reads/chunks are not model-token boundaries.

## Decisions

### Claude shell approval

The [captured approval](cli-capture-2026-09-06/claude/rendered/04-command-approval.png)
begins with a full-width index-153 rule. Its title starts at column 1, bold in
153. A bold promotional tip occupies a separate row. The command starts at
column 3; the wrapped example includes a dim `│` at 3 and command text at 5.
The plain description begins at 3. Blank rows separate description, reason,
question, choices and footer.

`❯` is at column 1; numeric label at 3; option text at 6. The selected option
uses index 153; numbers are 246. Four choices were offered: one-time Yes,
persistent command prefix permission, switching to auto mode, and No. Only the
one-time choice was used. Footer: `Esc to cancel · Tab to amend`, muted at 1.

### Codex shell approval and rejection

The [approval panel](cli-capture-2026-09-06/codex/rendered/08-command-approval.png)
uses the same `#323439` background family as user input. Its title starts at
column 2, bold. Separate blocks show Environment, Reason (italic explanation),
and `$ COMMAND`, with a blank row between. The selected `› 1. …` begins at 0
and is bold cyan. Unselected choices start with two spaces. The footer starts
at 2, outside the filled panel.

Rejection was actually exercised. It adds red `✗ You canceled …`, with
`canceled` bold and command dim. It also leaves a red-bullet `Ran` row with
`(no output)`, then red `■ Conversation interrupted …`. **That `Ran` wording
does not prove the rejected command executed.** The request was canceled and
the tool record says rejected. Ferrite should retain outcome semantics from
the provider events rather than inferring them from the display verb.

### Claude single and multi-select questions

Single-select question: muted top rule; tab ` ☐ Display ` with background 153
and foreground 16; question bold foreground 231; selected arrow at 0, numeric
label at 2, option at 5. Description begins at 5 on the next row. It adds
`Type something.` and a separate `Chat about this` choice, followed by muted
navigation hints. Moving selection changes colour/arrow without changing text.

Multi-select is materially different: header includes left/right arrows,
`☐ Elements` and a Submit tab. Options show literal `[ ]` / `[x]` checkboxes;
their descriptions start at 2 rather than the single-select description's 5.
Space toggles a choice. Right moves to the review page: `☒ Elements`, selected
Submit tab, bold `Review your answers`, a bullet question, green `→ Text, Tools`,
and explicit Submit answers/Cancel choices. No display setting was changed.

After submission, a muted assistant marker introduces `User answered Claude's
questions:`. The answer row begins `  ⎿ NBSP· ` and contains the question,
ordinary spaces around `→`, then comma-space-separated selections. A separate
ordinary assistant message acknowledges it.

### Codex question with notes

`/plan` visibly changed the Session's effort from low to xhigh and added a
magenta Plan mode footer. The [question panel](cli-capture-2026-09-06/codex/rendered/23-question.png)
shows `Question 1/1 (1 unanswered)` dim at column 2, question cyan at 2, a blank
row, then selected `› 1.` at 2. Labels and descriptions are on the same row,
aligned into columns. An extra `None of the above` row offers notes.

Tab opens a notes field, preserving the question/options above. The note's
marker is at 2 and content starts at 4. `Keep  two   spaces.` retains its exact
gaps. The footer changes to notes-specific hints. Enter submits the answer.

The settled transcript is `• Questions 1/1 answered` with a bold title and dim
count. An indented bullet shows the question. `    answer: ` and `    note: `
labels are dim; values cyan. It preserves the selected label's `(Recommended)`
suffix and the note's spaces. A separate assistant acknowledgement follows.

## Composer, menus, queue and interruption

Claude's one-line idle Composer is bounded by full-width index-244 horizontal
rules. In the 120×44 idle capture they occupy rows 40 and 42; `❯ NBSP` starts
row 41, followed by a dim suggestion. Ordinary draft text is not dim. The
multiline Composer grows upward while its lower rule/footer remain anchored.
Wrapped continuation has two leading spaces; four source indent spaces start
text at column 6. The 60-column version grows to six text rows.

Codex uses a background-filled blank row above and below input instead of
Claude's pair of rules. Its bold `›` and content start at 0 and 2. The same
multiline fixture wraps with the two-column continuation gutter and retains the
four source spaces. Long custom status text truncates with U+2026 at width 60.

Both slash menus and model menus are captured. Claude's model panel starts
with a repeated `▔` rule in index 153 and an indented title. The current model
has a green check. Codex's model panel is filled, with a title at 2 and a
selected bold-cyan row at 0. We canceled both selectors; no model default was
changed. The actual model choices in these captures are account/version data,
not a menu specification for every installation.

File-prefix completion was probed with `@sa`. Claude suggested fixture files
and previously available home-directory paths. Codex showed a broader picker
including installed plugins and Filesystem Only / Plugins search modes; no
suggestion was selected. Question-mark **pasting** and
question-mark **keypress** were captured separately: bracketed paste inserted
`?` as draft text; pressing the key on an empty Composer opened shortcuts.
Claude shows a three-column shortcut grid; Codex shows two columns with a
customization row. This distinction would be lost in a plain prompt transcript.

Queued input:

- Claude shows an indented `❯ After this finishes…` above the Composer and a
  dim `Press up to edit queued messages` placeholder. After Esc interrupted the
  current tool, the queued request ran and received its own answer/completion.
- Codex shows `• Queued follow-up inputs`, then `  ↳ MESSAGE`, and a further
  indented `shift + ← edit last queued message` hint. Tab queued it while the
  turn was still active. After interruption, the queued text returned to the
  Composer rather than appearing as a completed follow-up in this capture.

Claude interruption is a muted `  ⎿ NBSPInterrupted · What should Claude do
instead?`; the completed shell group remains. Codex interruption is the red
square/message described above. Both historical interruption text and the
current ready/working status can coexist; they must not be conflated.

The exit frames also include **tmux's** `Pane is dead …` line. That line is
recorder infrastructure, not CLI presentation. Claude's CLI exit prints a resume
command in dim text. Codex prints usage and resume instructions with cyan command
text. These should not be copied into Ferrite's ordinary answer renderer.

## Applying this evidence to Ferrite

Ferrite remains a native GPUI app receiving structured provider events, per
CONTEXT.md and ADR 0001/0003. The capture harness is research tooling only.

Use separate presentation recipes for user text, assistant text, visible
reasoning summaries, read/search groups, shell calls, tool output, questions,
approvals, status and completion. Preserve the same underlying tool identity
when its visible representation changes from running to grouped or detailed.
Do not infer failure, execution or concurrency from a glyph or display verb.

Preserve Markdown source and literal tool-output whitespace at their respective
boundaries. A rich text renderer's default list spacing, soft-break rules,
blockquote border and table style require deliberate configuration. Use Claude's
presentation as the shared reference for both Providers, per ADR 0006. The
Codex captures supply behavioural evidence and comparison, not a second visual
target. The two-column answer gutter is a layout unit; it does not require
converting all Ferrite text to literal prefixed spaces.

Provider-authored progress text, elapsed runtime, local completion timestamps,
and live execution state are separate inputs. Frame timings from this experiment
include deliberate operator delays and cannot serve as animation or latency
budgets. Use the byte/grid evidence for rendering, not for model-performance
claims.

## Coverage boundaries

Observed: startup/MCP loading, idle/suggestion, submitted prompts, commentary,
read grouping, command approval/accept/reject, running shell, stdout/stderr,
nonzero exit, success, output truncation, detailed transcript, scroll/page/export,
reasoning headline/shimmer, partial Markdown, completion, multiline drafting,
Unicode/whitespace, resizing, slash/model/file menus, shortcut overlay, queue,
interruption, single/multi-select questions, notes/review/answered forms and exit.

Not exercised: actual file edits/diffs; destructive operations; network failures;
rate-limit exhaustion; authentication failure; context compaction; long sessions
with eviction; image attachments; MCP permission variants; subagent displays;
Claude classic renderer/light themes; explicit thinking-display opt-in; every
configuration/model combination; emoji ZWJ/flag grapheme sequences; native text
selection and OS mouse hitboxes. Nothing in this report claims those states
look like the captured ones. The [pinned source reference](cli-rendering-source-reference.md)
documents additional branches without pretending they were observed.

One Claude frame raced a 120→80 resize: its grid and dimension metadata were
inconsistent. It remains in raw evidence, is listed in `render-quality.json`,
and is excluded from derived replay. No unrecognized ANSI sequences remain in
the rendered sample set. Eight parser contract tests and byte-index/hash checks
validate spacing, Unicode width, tab expansion, style boundaries, links, blank
background cells and explicit reporting of unsupported input.
