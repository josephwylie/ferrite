---
status: accepted
date: 2026-11-10
---

# T3 Code as the transcript reference

ADR 0007 made Claude's transcript the reference for both Providers. On
2026-11-10 the operator compared it with T3 Code (pingdotgg/t3code, open
source) and chose T3's: its turns read as one quiet header over the
answer, its work rows say what each step was at a glance, and its
finished work gets out of the way. T3 Code replaces Claude as the
reference for how a turn's work is grouped, labelled, folded and timed.
The measured spec, with T3's file and line for each value, is
`docs/design/t3-reference/transcript.md`.

## What changes

- **A turn's header sits at its top**, directly under the prompt:
  `Working for 11m 24s` while it runs (spaced parts, no shimmer), over a
  hairline. When the turn settles the same row becomes the **turn fold**,
  `Worked for 24m 55s ›`, collapsed by default, with any duration (no 30s
  floor). Collapsed, it hides the turn's work and leaves the final
  answer, compactions, live subagents, questions and failures. An
  interrupted turn reads `You stopped after …`; a steer starts a new fold,
  `Continued for …`. This replaces the `Worked for 38s · 8:53 pm` stamp at
  the turn's end.
- **The running step is its own live row at the tail** (`Running sed`,
  `Thinking`), shimmering, rather than carried into one working line.
- **Work rows carry an icon** in a 24px gutter, one per kind (command,
  edit, read, search, web, reasoning, mixed, agent…), a one-line label,
  a timestamp on hover, and a chevron at the right edge whose slot is kept
  when the row cannot open. Reasoning uses the brain icon, not `∴`.
- **Group summaries** name at most two kinds, commands and edits first,
  joined with "and" (`Ran 4 commands and changed 10 files`); the rest is
  `performed N other actions`. Past tense always; edits count unique
  files; failure shows in the icon's tint, not the text. One tool call is
  never a group.
- **An expanded group is a list, not a card**: 24px rows capped at 288px
  (or half the view), fading at an edge only when more lies past it.
- **Prompt times are absolute** (`8:26 pm`), under the bubble with its
  actions.
- New rows: the context-compaction divider, the subagent card, the
  changed-files card, the background-work line with its dialog.

## What stays from ADR 0007

One layout for both Providers; proportional Markdown prose with Ferrite's
enlarged headings; literal tool output and source whitespace; failures
never hidden; Provider facts (reasoning, tool identity, results, timing)
kept accurate; durations persisted with the turn, not regenerated on
replay. Ferrite's own motion where it already exceeds T3 (the bubble pop,
the stream fade) stays.
