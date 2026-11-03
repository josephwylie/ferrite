# Ledger — every promise and decision, tracked

This is the single list of what has been promised or asked for. A promise
counts as made only once it is written here. It was rebuilt on 2026-11-04 from
a full audit of the session log against the code: the three parts are in the
session scratchpad as `ledger1-3.md`. Each row is ticked only with evidence,
meaning a commit and a test or render.

## Owner decisions (standing)

These are not built:

- Inbox
- Remote Control
- Mic and voice input
- Plan mode
- The ChatGPT side

Settings: the owner fixes it themselves.

These are kept as they are:

- The red style on every Remove button.
- White working orbs.
- The Codex-style hero.
- The `/` project name.

## Done tonight

- [x] Reply to a selection: a centred pill, with Quote rows on the composer and on the sent message (`c35a2791`).
- [x] Sidebar text at 13px, and a darker sidebar at `#212121` (`c35a2791`).
- [x] Thinner text: no CoreGraphics glyph dilation (`e64ce45f`).
- [x] Dev builds use `debug = "line-tables-only"`.

## Lane M — motion, from Claude's source (`motion-claude-source.md`)

- [ ] All 14 gaps, the ⌘B snap, the row-hover snap, and the preview MP4.
- [ ] Composer height eases instead of jumping.
- [ ] Crossfade on thread and view switch, if Claude has one; check the source doc.
- [ ] The working-line shimmer leaves the ~30 fps clock.
- [ ] The kit's toasts follow Ferrite's motion.
- [ ] Three tooltips still use the old default style (`panel/preview.rs:371` and others).

## Lane W — the working line and transcript rows

- [x] The working line uses the tool's description, not the raw command.
- [x] Muted ink for the working line; the orbs stay white.
- [x] The token count is live for the whole turn, thinking included, with an arrow icon and rolling, eased numbers.
- [x] While the model is thinking, the working line says so and shows the thread's effort.
- [x] No duplicate running row above the working line.
- [x] "Running …" rows use the UI font; only code is mono.
- [x] The placeholder leaves a gap before the "↵".
- [x] Errors show once: no `failed ·` stamp over the failure strip.
- [x] Relative message times ("52 minutes ago"), with the exact time on hover.
- [x] Date dividers in long threads.
- [x] PageUp and PageDown scroll the transcript.

## Lane C — the composer and picker

- [x] Image attachments show as thumbnail cards in the composer tray (`6733a514`): the picture on an 88px card, × on hover or keyboard focus, a click opens the lightbox. The card is the sent image's frame too.
- [x] Add an image message to a render scene (`6733a514`): the `composer` scene sends one and has one in the tray.
- [x] `/skill` picks become chips, as `@` mentions do (`7a300afe`), sent as plain `/name`.
- [x] Slash-picker rows carry a source tag (Skill, Plugin, built-in) (`7a300afe`). Claude's come from its initialize list (`builtin`, `plugin:` names, the `(user)`/`(project)` suffixes, MCP prompts). Codex's skills give theirs from their scope; Codex reports nothing for its built-ins or prompts, so those rows are untagged.
- [x] The + menu matches Claude, with Connectors › and Plugins ›, and a submenu for each plugin (`7a300afe`). Plugins lists the plugins that bring commands, each a submenu of them. A plugin with no commands has nothing to pick, so it is left out, and Codex reports no plugins. Claude's other plugin and connector actions have no data behind them in Ferrite, so they are left out.
- [x] Each queued message can be edited and removed, and sent now (`73efd684`): a row each, with its own actions. Send now sends that message next. It steers into the running turn where the provider steers (Codex) and stops the turn where it cannot (Claude).

## Lane A — actions and menus

- [x] Actions under each answer: Copy, Fork and ⋯, as Claude has (`transcript/answer.rs`). Fork takes the conversation through that answer (before the next prompt, or all of it for the latest) and opens the new thread focused. ⋯ holds Copy message, Reply (Claude's "Attach message as context", as a Quote row) and Fork from here. Pin and Read aloud are left out because nothing backs them. Copy as Markdown is left out because Copy already copies the Markdown. Scene: `answer-hover`.
- [x] The thread ▾ menu gains Copy link and Fork. The ▾ opens the same menu as the sidebar row. There is no `ferrite://` scheme, so Copy link copies `ferrite thread focus <id>`, the CLI command that lands every window on the thread. Fork forks the whole conversation.
- [x] The sidebar row menu gains Claude's items, in Claude's order with rules and letter keys (`cockpit/context_menu.rs`): Open in › (Finder, Terminal; Ferrite knows no editors), Pin P, Mark as unread U, Rename R, Fork F, Toggle fullscreen T (it moved off F), Copy link K, Move to group › (Claude's "Move to section"; projects are checkouts and can't be moved between), and Park thread A as Claude's Archive. Submenus open beside the menu, on hover or on a press. Scenes: `contextmenu`, `row-menu`.
- [x] The approval split button gains "Allow and switch to Auto": the provider's own mode from `Settings::auto_mode_for` (Claude `auto`, Codex `never`), with no list hard-coded in the UI.
- [x] Stop a subagent row from the keyboard. An agent's row is now a Tab stop: ↵ opens the agent and ⌫ stops it (`cockpit::StopAgent`, the same stop op Escape uses). It is listed in the shortcuts sheet.
- [x] Palette commands for rewind and fork: "Rewind to last message" opens the last prompt's Rewind menu, and "Fork thread" forks the whole conversation.
- [x] "Start a side chat" from a selection, beside Reply. It forks the whole thread into a new focused thread, with the passage as a Quote row in its Composer. **Difference:** Claude's side chat is a floating, ephemeral panel that shares the session (`/btw`, ⌘;, tabs, "Clear side chat"). Ferrite has no place for that, so its side chat is an ordinary forked thread.

## Lane P — the panel, preview and sidebar

- [x] A page and preview pane: open a rendered page or URL beside the transcript, with a title menu, expand and close (`fa36929a`). An `.html` file draws live through the visuals engine under `--features cef`; a local URL, or any page without Chromium, shows the can't-show note with Open in browser. The engine is sandboxed and loads no URLs.
- [x] Preview cards in the transcript: a thumbnail, Open and ⋯ (`cdc328f3`). The thumbnail is a globe glyph; no page snapshot yet.
- [x] A lightbox: a dimmed overlay with arrows, "1 of N", zoom and download (`bcef8604`, `cda2c1c5`).
- [x] The right panel is closed by default (`8a9ac2e3`).
- [x] Toasts no longer cover the right panel (`4eb04ac0`, `2cba4d9c`).
- [x] Long sidebar titles fade out instead of ending in "…" (`76e2c416`).
- [x] Remove the "↑N" ahead/behind counts from project headings (`fdc65449`).
- [x] When the panel is hidden, the window title shows the branch (`69525ef2`).
- [x] File cards, with Open in, for files the agent produces (`cdc328f3`). Open in offers Finder and the default app; Ferrite knows no list of editors.

## Lane X — Codex parity and checks

- [ ] Edit and rewind on Codex threads.
- [ ] Check the subagent drill-in against a Claude capture. It needs the Claude app on screen, so it waits for the owner.

## Lane G — GPUI and assets

- [ ] Apply upstream #65952: fresh updates for reused cached views.
- [ ] Icons at a true 1px stroke.

## Waiting on the owner (asked in the morning summary)

- The type scale with base 13, and the owner's hex palette from early on. Since then, Claude's measured values have been applied instead; it needs a confirm.
- Bring back the PR and Details tabs?
- Save on Environment?
- A Solo view tab?
- Grey side-panel row icons?
- "Edit" or "Update" wording for each provider?
- The Projects removal: PR #190 still needs to land. Then cargo-sweep, rust-trim and mem-guard come back on, and merged worktrees are removed.
- Push this branch and open its PR.
- Threaded comments on pages; this follows the page pane.
- Provider options, CLI session import and home suggestions; these are deferred.
