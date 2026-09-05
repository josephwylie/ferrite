# 0003 — Native GPUI components with Ferrite presentation

Status: accepted by operator instruction, 2026-09-05.

## Decision

Use the GPUI Kit 0.6.0 facade and its compatible platform/base/component stack.
The toolkit owns rich-text parsing and selection, popup-menu interaction,
and Settings categories. Ferrite supplies its existing tokens,
provider icons, pane frames, command actions and durable Thread identity.

Keep Ferrite's existing split-tree layout and resize/drop handling. Native dock
integration produced flashing and unacceptable streaming latency in the real
macOS app despite passing headless geometry checks. The operator requested the
simplest stable fix, so the native dock integration and tab persistence are
removed from this change. Centre drops swap panes; edge drops split them.

Answer blocks retain exact Markdown source for the native renderer, including
URLs and nesting. Adjacent answer sections share a cached native document.
Every Markdown block carries the answer's original identity, so both visible-tail
and core-history eviction update the existing parser instead of mounting an empty
replacement. The GPUI Base patch skips hitbox calculations for fully clipped
text; selection and copying of offscreen text remain unchanged. See
[dependency patch](../../vendor/README.md).
Tool output remains literal and selectable. HTML code fences offer a native
formatted preview; this is not a browser, CSS layout engine or JavaScript host.
GPUI 0.6's default text document requests full height inside the transcript's
auto-height rows. An unbounded line cap selects its natural-height path, avoiding
circular sizing without truncating content or introducing another scroll owner.

Root owns the active text-selection scope. The focused transcript participates
in that scope, with inactive panes isolated. A document evicted from the render
window clears its selection. Native double-click selects a word and triple-click
a paragraph; GPUI Kit 0.6 does not extend a double-click drag word-wise.

Consecutive shell calls share a display disclosure, retaining each call's
identity and result. Success output starts collapsed, failures stay visible,
and streaming preserves the operator's expansion choice. No parallel-execution
claim is inferred. See [Claude CLI research](../research/claude-code-tool-grouping.md).

## Consequences

Remove the duplicate custom selection engine. Keep the existing split/seam
renderer and keep provider execution, replay and saved event formats independent
of GPUI. Native
controls still require integration checks for focus, streaming layout, scrolling
and persistence; adopting a component does not validate those automatically.
