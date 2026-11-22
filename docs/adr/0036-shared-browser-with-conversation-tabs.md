---
status: accepted
date: 2026-11-08
---

# Share browser infrastructure while conversations own tabs

Ferrite will manage an on-demand browser service shared across conversations on Windows and macOS. Each conversation owns its tabs; the right-hand preview displays those exact live pages, and agent control pauses while the operator interacts. This avoids requiring a separate complete browser instance for every agent while keeping agents from navigating each other's pages.

Conversations share website login state while retaining separate tabs. Signing out or changing a site's shared session can therefore affect other conversations. Remember logins and tab URLs across restarts, loading restored pages only when needed. Inactive pages may unload to save memory, protecting tabs agents are using and unfinished forms; reloaded pages may lose temporary state.

The user approved investigating extension and optimization of the existing CEF engine, revising the earlier Chromium exclusion for this investigation. Final adoption depends on resource measurements and proper isolation for external browsing on both platforms. This does not mean one renderer process for arbitrary sites, and does not supersede ADR 0013's sealed-visual policy. Suspension, control handoff, and headless behavior remain unresolved.

## Amendment (2026-11-21): the window never waits on the Server for its pages

A window's Browser asks of the Server never wait on the window's thread
(ADR 0041, an ask never waits on the window's thread). These are its
attach, its actions, its pages' reports, the operator's input, a
reclaim and its outcome, and an annotation's protection, capture and
image. Each returns an `Ask` that the pump answers, with the reply or
with the link gone. The ones with no deadline of their own are also
answered "did not answer in time" after `REPLY_WAIT`. A capture's
upload and its image keep the deadlines they had, and a control's
dispatch grant waits as it did. Settling the operator's input and releasing a lease are
tells.

- **The tabs a window shows are the Server's `Browser` messages, never a
  reply's state.** The Server publishes each state to the owning window
  before the reply that carries it. A reply says whether the ask went and
  what it grants: a lease, an input's revision, a reservation.
- **The operator's input reaches the page only once the Server has
  authorized it**, one input at a time per page. Agent control pauses
  from the first input event, before the Server answers, as it did. An
  input whose page moved on while it was asked is dropped and settled.
- **A reclaim is two asks.** The Server reserves the page, the window
  unloads it if the document is still the one reserved, and the window
  says whether it did. Neither is asked again while one for that tab is
  on its way.
- **An annotation begins in two asks too** (input, then protection).
  Its editor starts once the lease is granted. Closing or cancelling it
  meanwhile gives the page back when the answer comes, and an action
  pressed meanwhile then runs.
- **A Server that does not answer leaves the window drawing.** The
  Browser's notice says why once the ask's time is up, or that the
  Browser disconnected.
