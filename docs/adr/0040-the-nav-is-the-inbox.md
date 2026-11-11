---
status: accepted
date: 2026-11-10
---

# The nav is the Inbox: one list of thread cards

The nav has carried a Projects tree, an Attention list over it (ADR 0015's
2026-10-24 amendment) and an Inbox view shown in the tree's place behind
the bell or ⌘I. Comparing it with T3 Code's default sidebar, the operator
chose T3's: one flat list of thread cards, no inbox mode to switch into,
"just clean". The Inbox becomes the nav's normal and only view.

## Decision

- **One list of cards.** Each open Thread awaiting the operator is a card
  (ticket "Thread cards in the nav"): its Project's badge and name, its
  status in words with a live time, its title, its branch, pull request
  and Provider. Pending Decisions first, oldest request first, as the
  Inbox orders them.
- **Shelves at the foot.** Working, Snoozed and Parked (Ferrite's word for
  T3's "Settled") fold to a head and count, anchored at the bottom of the
  column above the foot, folded by default, each fold remembered.
- **The Projects tree and the Attention list retire.** No Project
  headings, no `Show more`, no separate Inbox view or bell door; ⌘I and
  the Bar's needs-you count focus the nav. A Project is reached through
  the search row's filter (filter by Project) and its new-project button;
  Groups open from the same filter menu and the palette.

This supersedes ADR 0015's Attention-list amendment. The four Attention
states, their order and their roll-up stay; they now sort one list. The
measured card and shelf spec is `docs/design/t3-reference/sidebar.md`.
