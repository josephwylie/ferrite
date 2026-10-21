---
status: accepted
date: 2026-09-06
---

# Claude as the shared transcript design reference

After reviewing the [captured CLI transcripts](../research/cli-transcript-visual-spec.md),
the operator chose Claude's design approach because its transcript is easier to
use. Ferrite uses Claude as the primary presentation reference across **both
Providers**, including Codex Threads. This supersedes the research recommendation
to reproduce each Provider's CLI styling separately.

The shared presentation follows Claude's hierarchy: readable answers, compact
tool summaries, restrained secondary text, consistent gutters and spacing, and
details available through disclosure. Commentary separates activity groups;
completed routine work recedes so the answer remains easy to find. Markdown
headings render as headings with their syntax removed. Provider identity does
not select a different transcript layout.

The operator explicitly retained Ferrite's proportional Markdown prose and
existing enlarged heading sizes. Claude's terminal font geometry is not an
implementation target; the remaining hierarchy, spacing and disclosure findings
still apply.

Provider-specific facts remain accurate: available reasoning, tool identities,
results, timing, execution status and Decision delivery. The existing native
selection, literal tool output, independent disclosures and visible failure
previews in [ADR 0003](0003-native-gpui-components.md) still apply. Claude's
usability principles guide presentation; observed CLI quirks do not require
regressions in Markdown support or hiding meaningful failures.

The cell measurements guide proportions and spacing decisions. Reconstruction
font sizes and padding are not native pixel specifications. Implementation
continues through GPUI and Ferrite's shared presentation tokens; this decision
does not introduce a terminal renderer into the application.


Completion uses one quiet locally observed timestamp and elapsed duration,
persisted with the turn rather than regenerated on replay. The stamp reads
`Worked for 38s · 8:53 pm`; an interrupted or failed turn states its elapsed on
its own row (`Interrupted · 4.1s`, `Failed · 4.1s · <message>`); provider process runtime and approval-wait breakdowns are shown only if supplied.
Reasoning appears once in the visible transcript/live area; only the live marker
animates. Exact literal tool output and question notes retain source whitespace.

Native tabs retain U+0009 and the platform shaper's local run advance. There is
no terminal-wide eight-column grid or source-to-space rewrite. The reference
fixture records macOS tab advances with the then-bundled JetBrains Mono; Ferrite
now bundles Geist Mono. Ordered markers
use native measured widths, and overflowing tables use the existing horizontal
scroll table layout. Headings are semibold with no italic or underline;
H1 and H2 are enlarged relative to the answer size.

The [final findings account](../research/ferrite-transcript-implementation-2026-09-06/README.md)
records all F01–F60 dispositions, native artifacts and comparison limits.
