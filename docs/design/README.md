# Design canon

The visual system Ferrite builds toward. Source of truth, in order:

1. `comps/` — the six canon artboards from the operator's Rich Pane canvas.
   `DirectionDense.dc.html` is canon for transcript density (its note
   supersedes `Main.dc.html`'s airier transcript; Main still governs pane
   structure). Boards not in this directory — the three Direction skins and
   the Decision rail — were rejected by the operator and are not design
   sources.
2. `concept.md` — extracted spec of the Concept boards: Aperture tokens,
   transcript anatomy, the Composer's four states, the L2 cell grammar.
3. `glance.md` — extracted spec of the Glance-system boards: the zoom
   ladder and the 24-pane wall. The wall cell has no thread number: state
   dot, slug name, progress bar, status line, inset attention ring.
4. `sidebar-and-impl.md` — the left nav and fullscreen designs (surfaces
   the canvas does not cover, drawn in its language) plus the map from the
   whole system onto the existing modules and the staged build order.

Operator rulings that bind over anything in these files:

- No Decision rail. Decisions stay in-Pane (card) and on the wall (ring).
- No thread numbers at L3 — identity is slug + position.
- The Composer's `/` skills menu and `@` file mentions ARE wanted (#23),
  overriding `sidebar-and-impl.md`'s must-not-build list on those two.
- Ignore the Directions page entirely.

Tickets: #22 (visual pass), #21 (left nav), #20 (cmd-t/w/f), #23
(Composer behaviors). Open questions live in each spec's final section;
unresolved ones go to the operator, not to invention.
