# Ferrite and T3 Code investigation

The user wants an extremely detailed source-backed inventory of Ferrite's broken workflows, missing T3 capabilities, weak interactions, and absent motion. They report unreachable PR views, PR links that underline but do not open, unreliable PR operations, missing subagent views, unexpected background-work navigation, an unattractive browser, and unnatural keyboard and mouse behavior around the right panel.

This is an investigation. Do not change application code, settings, dependencies, branches, or existing user data. Write only your assigned report and issue JSON. Do not commit or open a PR. The parent owns runtime UI access. Do not drive the live app or launch workers yourself. Source inspection is sufficient for source-proven findings. Tell the parent which runtime check would settle uncertainty.

## Sources

- Ferrite root is `/Users/josephwylie/Desktop/Projects/ferrite`.
- Ferrite baseline is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Current T3 source is `/tmp/ferrite-t3-audit-source-20261113` at `74573fc29b901bafbf8f1dbb2eb29606da1fa448`.
- Older Ferrite comparison specifications are in `docs/design/t3-reference/`. They used T3 `bcee0f5d`. Treat them as historical intent, not current implementation evidence.
- Ferrite ADRs 0038, 0039, and 0040 explain recent intent. The September audits may contain useful regressions, but require fresh verification.
- Read `/Users/josephwylie/.codex/pstack/docs/harness/codex.md` and the relevant investigation instructions in `/Users/josephwylie/.codex/pstack/skills/poteto-mode/SKILL.md`. This swarm uses general-purpose explorer roles. The parent owns the workflow. Read `/Users/josephwylie/.codex/pstack/skills/unslop/SKILL.md` before writing.

## Method

Inspect both implementations. Trace visible affordances through event handlers, state changes, rendering, persistence, and backend operations. Search broadly before calling a feature absent. Separate bugs, partial implementations, missing capabilities, intentional product differences, and unverified risks. A source-defined feature is not proof that the installed binary works. A passing unit test is not proof that a click reaches its handler.

Record the exact commits and method. Cite paths and line numbers for every consequential claim. T3 citations should include commit-pinned GitHub links. Ferrite citations should use repository-relative paths with line numbers in the report. Distinguish runtime-reproduced, source-proven, inferred, and unverified claims. Do not inflate severity or treat every T3 capability as a mandatory Ferrite requirement. Explain user impact. Include positive coverage and missing runtime checks.

The user explicitly wants useful motion across surfaces. A T3 default of no animation does not settle that product preference. Document both facts. Do not claim measured performance without a measurement method. Avoid numeric quality scores that imply measurements we did not perform.

## Deliverables

Write `reports/<slice>.md` and `reports/<slice>.json` in this audit directory. Return PASS, ISSUES, or BLOCKED, the paths, your strongest findings, and limitations. Do not stop after the first issue. Exhaust the assigned slice without duplicating other workers' ownership.

The Markdown report opens with the user's goal and includes an overview, key concepts, traced flows, a source map, detailed findings, supported T3 capabilities, positive Ferrite behavior, and verification gaps. Each finding has a stable slice-prefixed ID, priority, category, evidence level, symptom, trigger, expected and actual behavior, cause, source citations, recommendation, and an observable acceptance check.

The JSON is an array. Each item has `id`, `title`, `priority`, `category`, `evidence_level`, `impact`, `ferrite_evidence`, `t3_evidence`, `recommendation`, and `acceptance`. Use priority P0 for data loss or a broadly unusable critical workflow, P1 for broken or inaccessible primary behavior, P2 for partial workflows and material usability gaps, and P3 for minor polish. Categories are `defect`, `missing-capability`, `partial-capability`, `interaction`, `motion`, `intentional-difference`, or `verification-gap`. Evidence levels are `runtime-reproduced`, `source-proven`, `inferred`, or `unverified`. Evidence arrays contain concise path-and-line strings. Do not put positive features into issue JSON.

## Coverage partitions

1. `pr-navigation`. PR entry points, links, routes, visibility, selected targets, PR pane reachability.
2. `pr-operations`. PR creation, loading, review, checks, comments, mutations, refresh, authentication, error recovery.
3. `subagents-background`. Agent identity, tree, inspect and follow, parent links, background work, unexpected routing.
4. `browser`. Browser chrome, page layout, tabs, controls, focus boundary, inspect tools, history, persistence, failures.
5. `input-focus`. Keyboard shortcuts, mouse events, focus, hit testing, propagation, right-panel ownership, accessibility.
6. `motion`. Every structural and micro-interaction transition, interruption, reduced motion, loading and feedback.
7. `panel-tools`. Right panel state, tabs, sizing, files, changes, diff, terminal, preview, persistence.
8. `composer`. Drafts, send, queue, steer, stop, attachments, slash commands, mentions, model controls.
9. `transcript`. Rendering, grouping, scroll, history, selection, copy, search, links, tool details, errors and questions.
10. `providers-settings`. Providers, sessions, model capability support, permissions, settings, connection lifecycle.
11. `projects-navigation`. Project and thread lists, search, sorting, archive, worktrees, rename, navigation history.
12. `coverage-quality`. T3 inventory beyond assigned slices, old promises versus evidence, test blind spots and regression protection.
13. `link-affordances`. The user's follow-up explicitly reports that right-clicking a link offers Copy without Open in browser. Inspect every link context, the actual context menu, system-browser opening, new-tab opening, modifier-clicks, middle-clicks, address copying, keyboard activation, and target-aware routing. Cross-reference existing browser and PR issues instead of counting them twice.
14. `scroll-quality`. Audit transitions between manual scrolling, smooth scrolling, anchors, streaming, disclosure, reflow, and history changes. Feature existence does not establish reliable behavior.
15. `quote-attachments`. Audit the full selection, citation, annotation, attachment, send, replay, and recovery journey, including whether users can add their own text to cited content. Inspect rendered layout where possible.

## User clarifications

The user explicitly rejected treating implementation presence as evidence of production quality. They report intermittent buggy jump scrolling, snapping sidebar collapse, poor attachment and quote presentation, and inability to add their own text when citing something. They do not want to enumerate every defect. The audit must discover incomplete interaction journeys, include failure and transition states, and avoid declaring a feature good because a handler or model exists.

The parent recovered native screenshots after the initial timeout. Runtime observations must now distinguish initial failures from later successful observations. A separate headless interaction worker may use a fresh temporary Store and the real GPUI driver to avoid competing with the operator's active window. That worker must record the exact source build, fake-provider limitations, observations, screenshots, and cleanup.

## Completion predicate

Every partition returns evidence or an explicit gap. The parent checks high-impact claims, removes duplicates, and records unresolved contradictions. The final artifact has a consolidated tally, priorities, source evidence, runtime evidence where available, and a sequenced remediation recommendation. No claim of exhaustive runtime parity is allowed without exercising every relevant flow.
