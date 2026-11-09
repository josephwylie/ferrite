---
status: accepted
date: 2026-11-08
---

# Share browser infrastructure while conversations own tabs

Ferrite will manage an on-demand browser service shared across conversations on Windows and macOS. Each conversation owns its tabs; the right-hand preview displays those exact live pages, and agent control pauses while the operator interacts. This avoids requiring a separate complete browser instance for every agent while keeping agents from navigating each other's pages.

Conversations share website login state while retaining separate tabs. Signing out or changing a site's shared session can therefore affect other conversations. Remember logins and tab URLs across restarts, loading restored pages only when needed. Inactive pages may unload to save memory, protecting tabs agents are using and unfinished forms; reloaded pages may lose temporary state.

The user approved investigating extension and optimization of the existing CEF engine, revising the earlier Chromium exclusion for this investigation. Final adoption depends on resource measurements and proper isolation for external browsing on both platforms. This does not mean one renderer process for arbitrary sites, and does not supersede ADR 0013's sealed-visual policy. Suspension, control handoff, and headless behavior remain unresolved.

