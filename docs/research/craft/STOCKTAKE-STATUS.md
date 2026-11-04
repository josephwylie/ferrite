# Stocktake status: where every stocktaked feature went

Audited 2026-11-04 against `crates/` and `git log --oneline -300` on `transcript-app-style` (HEAD `ed5541cf` plus uncommitted work). The ledger's word was not taken alone. Sources: `docs/research/claude-quality-bar.md` and `docs/research/codex-quality-bar.md`. Each feature or behaviour gets one row; where both apps describe the same thing (scroll-to-bottom, date dividers, the image thumbnail), it is one row.

Scope follows the owner: settings, the ChatGPT side, voice and mic, plan mode and Remote Control are OUT OF SCOPE. The rest is judged on function and quality, not a 1:1 look. DIFFERENT BY DESIGN cites the recorded decision. Rows marked **KNOWN** are already being fixed and were not re-investigated.

## Summary

| Status | Count |
|---|---|
| BUILT | 84 |
| PARTIAL | 69 |
| MISSING | 36 |
| DIFFERENT BY DESIGN | 15 |
| OUT OF SCOPE | 7 |
| **Total** | **211** |

15 rows carry a KNOWN tag.

### Reachability

Every BUILT row was traced to a call site in the real window, fed by real provider data. None is reachable only from tests or visual-reference scenes. The one scene-only code path is **XD5** (scrolling a new prompt to the top): the Runway exists in `transcript.rs:855`, but its entry point `transcript_glue.rs:398` is `allow(dead_code)` outside `visual-reference`, so it is listed as MISSING. Two BUILT-adjacent caveats:

- **PG2:** the page pane needs the non-default `cef` feature. A plain `cargo run` shows only the can't-show note; `scripts/install-app.sh` turns the feature on.
- **US1, US2:** Codex data is thinner. Limits are not fetched up front and there is no plan name or context breakdown.

### Ledger corrections found

- **CO14:** the running placeholder promises "↵ to send, ⌘↵ to queue for later", but both keys bind `cockpit::Submit` (`ferrite-ui/src/keymap.rs:200,204`). The hint describes a difference that doesn't exist.
- Lane C's "+ menu matches Claude" is true only by recorded omission: no Browse, no Chrome toggle, no GitHub issue. Connectors and Plugins date from `3b33960c`; `7a300afe` added only the per-plugin submenus.
- Lane P's page pane tick doesn't mention that a default build has no Chromium (PG2). Its lightbox "zoom" is a fit/actual toggle, not the zoom menu the bar describes (IM4).
- One core gap sits under several rows: `ToolState` has no Stopped or Denied (`ferrite-core/src/transcript.rs:526`), and the todo card lives in the working tail. That is why ER1, ER2, TR5, TR7, TR12 and XTR2 all fall short together.
- `docs/research/craft/motion.md`'s "Ferrite, from code" section is stale (140 ms menus, the thread-switch veil). `motion-claude-source.md` is current.

## MISSING and PARTIAL, ranked by how much a daily user would notice

| # | ID | Feature | Status | What's missing | Evidence |
|---|---|---|---|---|---|
| 1 | ER2 | Deny ends the turn; the summary records "(1 stopped)"; the todo shows ⊖ | BUILT | Deny doesn't end the turn: the model carries on. **Audit 11-04:** 626bd327, 37ec2662: deny sends interrupt; the turn closes aborted_tools | `claude/wire.rs:1174` sends `deny` with no interrupt |
| 2 | CO14 | Placeholder changes with state ("Type / for commands"; "↵ to send, ⌘↵ to queue") | BUILT | **The running hint is false.** ↵ and ⌘↵ both bind `cockpit::Submit` (`keymap.rs:200,204`): on Claude both queue, on Codex both steer. **Audit 11-04:** a8ae6b24: the hint reads "↵ to steer this turn" | `pane/composer.rs:895-905` (1c1e5980) |
| 3 | QS3 | ↵ while running steers: a pending bubble in the transcript with "Send now" | DIFFERENT BY DESIGN | **Re-checked (the owner disputed it).** "Send now" exists, but in the queue strip above the composer, not on a bubble. Nothing goes into the transcript until the provider says the message has started. See the QS3 note in the full table. **Audit 11-04:** Ledger standing decision 2026-11-04: no pending bubble; the queue card is enough | `cockpit.rs:5486-5491` → `pane/composer.rs:665` `queue_card`, `:799` `queued_send_now`; `engine.rs:3253-3283` |
| 4 | ER1 | Stop: "You stopped after Ns", the row says stopped, the todo stays, the composer is back | BUILT | The owner wants Codex's form. The row says "no result", and the todo vanishes. **Audit 11-04:** 626bd327: "You stopped after Ns"; the plan stays; Stopped state | `pane.rs:3762` "interrupted · Ns"; `transcript.rs:2321` |
| 5 | TR12 | Todo card: ○, spinner, ✓, ⊖ blocked | BUILT | No ⊖ state. The card lives in the working tail, so it vanishes when the turn ends. **Audit 11-04:** 39785ffd, 626bd327: the plan stays under the turn; a blocked step shows a bar in a ring | `pane/working.rs:520` (ed48e679) |
| 6 | TR5 | One prose summary line per tool group, with "(1 stopped) (1 failed)" | BUILT | No "stopped": `ToolState` has no Stopped (`core transcript.rs:526`), so a stopped call reads "without a result". **Audit 11-04:** 626bd327: "(1 stopped)" | `pane/text.rs:401-442` `group_summary` (095df35b) |
| 7 | XTR1 | "Working for Ns" turn header that collapses to "Worked for Ns ›" and expands to commentary, plan and commands | PARTIAL | Only a "Worked for Ns" stamp at the end, and it does not expand. No live header. **Audit 11-04:** Live line has its ▸ (ad8c7239). Claude has no "Working for" header (Codex does); the end stamp still does not expand: needs a product call | `pane/text.rs:16` `turn_stamp` (8b846be4) |
| 8 | TR18 | Thin overlay scrollbar on the transcript | BUILT | No scrollbar. The minimap shows only on hover. **Audit 11-04:** a8ae6b24: thin overlay scrollbar | transcript list ~`transcript.rs:2470`; DETAIL-AUDIT B1 |
| 9 | XCO9 | Drafts persist per thread | BUILT | Kept in memory only while the thread's Pane is open. Lost on park and on restart. **Audit 11-04:** 4123d281: drafts persist per thread | `core/cockpit.rs:681` |
| 10 | AP9 | The turn clock keeps counting during an approval | BUILT | The working line is hidden while a decision stands. **Audit 11-04:** 626bd327: the working line stays over a waiting Decision (Solo) | `pane/working.rs:40`, `cockpit.rs:1834` |
| 11 | NS5 | Chips under the composer: computer, folder, worktree, model+effort, permission, usage ring | BUILT | A draft (new session) has no permission chip and no usage ring (`mode: None, ring: None`). No "This computer". Worktree is a Workspace picker, not a checkbox. **Audit 11-04:** Permission word and usage ring on a draft (ledger item 22; ring not run) | `cockpit.rs:8066` `composer_toolbar`, `:7951` `draft_band_chips` (bfa6b2e6) |
| 12 | XTR5 | Code blocks as cards: language header, wrap toggle, copy | PARTIAL | Copy on hover and an HTML preview. No language header, no wrap toggle. **Audit 11-04:** ef52f8ba: language header and Copy built; still no wrap toggle | `rich.rs:693-760` (05fc2232) |
| 13 | TR9 | A Read row click opens the file in the right panel | BUILT | A plain click on a tool row's path goes to the OS opener. Only prose file chips reach the panel. **Audit 11-04:** 05e9f63f: the Read row path opens its file in Files | `pane/text.rs:179` → `rich.rs:593` → `file_links.rs:85` `cx.open_url` |
| 14 | SH6 | Unread dot (Codex blue) plus a badge; clears on view; Mark as unread | BUILT | The dot is white. No badge anywhere. **Audit 11-04:** 6b6072a5: blue unread dot; no count badge | `nav.rs:1545` (d880a138); `context_menu.rs:370` (0e42e74b) |
| 15 | SH2 | Sidebar header: bell with a dot when a turn finishes; search | BUILT | The bell opens Activity, but it never gets a dot. Search is the ⌘K palette. **Audit 11-04:** 6b6072a5: the bell wears a dot; bell sections 8c514306, bca332a1 | `nav.rs:881,895`; `cockpit.rs:13379,13347` (1c1e5980, 3b33960c) |
| 16 | AP8 | Sidebar dot or "Needs approval" pill while waiting; a bell dot | BUILT | Orange dot built. No bell or badge dot. **Audit 11-04:** 6b6072a5: bell dot | `nav.rs:1531` `state_mark`, `:2124` (folded rail) |
| 17 | QS4 | Steer delivered at the next tool boundary, answered in the same turn | BUILT | **Corrected.** On Claude the message is *not* held until the turn ends. Ferrite writes it to the CLI mid-turn as an `isAsync` user frame, and the CLI itself folds such messages in between tool rounds. What Ferrite lacks: it doesn't know this happens (`steers()` is false), so the strip says "Queued" and Send now interrupts. A folded prompt also restarts the turn clock and plan. No capture with tools exists yet. See the QS4 note in the full table. **Audit 11-04:** Ledger next-build 12: Claude steering surfaced (Steering, fold without interrupt) | Codex `providers/codex.rs:591`; Claude `claude.rs:628-644`, `providers/mod.rs:172-177`, `engine.rs:3789-3792` |
| 18 | XQ7 | Loading a thread: centred spinner, model-chip skeleton | BUILT | The instant switch is built. No loading spinner, no skeleton chips. **Audit 11-04:** 613e6bf9: the model chip shimmers while the Session starts | switch snaps (LEDGER Lane M) |
| 19 | TR10 | Long-running command: "View live output ▸" | PARTIAL | The core keeps `output_tails`, but nothing draws them. **Audit 11-04:** HD64: background output opens in the Background pane. Foreground commands still have no "View live output" | — |
| 20 | RP2 | Title-bar tool: Terminal (real PTY in the project) | BUILT | Only "Open in › Terminal" (Terminal.app). **Audit 11-04:** c3464a0c, b8a13756: real terminal in the dock | `view_head.rs:23-25` "not drawn" |
| 21 | SK1 | Customize › Skills page (Yours/Discover, search, sort) | MISSING |  | — |
| 22 | RP5 | Title-bar tool: Files | BUILT | Opens Finder. In the app, files can be browsed only through the Review's tree. **Audit 11-04:** 804ba262: Files in the dock | `view_head.rs:215` (Reveal in Finder); `review.rs:2070` |
| 23 | TR7 | Expanded group: bordered rows, status words coloured (Failed red, Stopped grey) | BUILT | No grey Stopped (same core gap as TR5). **Audit 11-04:** 626bd327: ToolState::Stopped never reads red | `pane.rs:4165`, `:4064` (095df35b) |
| 24 | XTR2 | Plan inline as checkboxes, kept in the turn | BUILT | Ticks live, then disappears at turn end. **Audit 11-04:** 39785ffd: the plan is kept with its turn | `pane/working.rs:520` (ed48e679) |
| 25 | LT3 | Long work summaries collapse ("Worked for ›") | PARTIAL | Tool groups collapse; whole turns don't (see XTR1). | — |
| 26 | TR11 | Rows switch to past tense when done | PARTIAL | Ferrite's verbs do. The provider's own description ("Waiting 15 seconds…") stays in the present tense (`text.rs:253`). | `pane/text.rs:93-104` (8cea78b3) |
| 27 | SH5 | Pinned threads (pin/unpin, pinned section, ⌥⌘P) | PARTIAL | No ⌥⌘P chord, only the menu letter. | `nav.rs:186`, `context_menu.rs:359-363`, `cockpit.rs:7193` (3b33960c) |
| 28 | SH9 | Thread ⋯ menu with icons, chords and separators | PARTIAL | No icons. Letters, not ⌥⌘ chords. No New side chat, Share, Copy ›, Open in new window or scheduled task. | `context_menu.rs:339-421` |
| 29 | SN3 | ⌘K command palette | PARTIAL | KNOWN: Esc bug. Otherwise rich (threads, projects, about 40 commands). | `cockpit/palette.rs` |
| 30 | SH15 | Row right-click menu | PARTIAL | KNOWN: right-click target mismatch. | `context_menu.rs:339`, `cockpit.rs:13401` |
| 31 | CO20 | Picking a skill inserts `/name` as a chip | BUILT | KNOWN: skill chips. **Audit 11-04:** Superseded: a skill is blue ink, as Claude (102c2e76, db13e437) | (7a300afe) |
| 32 | CO16 | Slash picker opens on "/" with a "Type to filter" hint | PARTIAL | KNOWN: mono slash picker. Also no "Type to filter" hint. | `cockpit.rs:6106` `derive_menu` |
| 33 | XD5 | A new turn scrolls the prompt to the top, leaving room below | BUILT | **SCENE-ONLY:** the code exists but only reference scenes reach it. A real send scrolls to the bottom. **Audit 11-04:** a8ae6b24: a real send scrolls its prompt to the top | `transcript.rs:855` Runway; `transcript_glue.rs:398` `allow(dead_code)` without `visual-reference` |
| 34 | US1 | Usage ring fed by real plan limits (both providers) | PARTIAL | Claude is complete. Codex is never asked up front (empty until its first update) and sends no plan name. | Claude `claude/wire.rs:759`; Codex `codex/wire.rs:225,492`; `engine.rs:3299`; `pane/usage.rs:18` |
| 35 | US2 | Context usage from real data (both providers) | PARTIAL | Codex has no category breakdown. | Claude `claude.rs:523,1050-1068`; Codex `codex/wire.rs:896`; `panel/sections.rs:41-60` |
| 36 | CO8 | Effort submenu: explanatory line, Recommended tag, usage-warning pill, Ultracode toggle | PARTIAL | A bare ladder with ✓. None of the explanations, tags or pills. **Audit 11-04:** Recommended tag on Medium built; explanation line, Max usage pill and Ultracode need data Ferrite lacks | `cockpit.rs:12690` `effort_submenu` (9e42e44e) |
| 37 | AT11 | Non-image attachment as a pill above the bubble; click opens the default app | PARTIAL | A chip inside the bubble. A click opens Ferrite's preview. | `pane.rs:3219-3240` |
| 38 | AT5 | Text file in the composer as a card with a first-page preview | PARTIAL | A small chip with an icon and name. | `attachments.rs:134` |
| 39 | AT9 | Failed attachment state (red tile, "Failed") | MISSING |  | — |
| 40 | IM4 | Lightbox: dim, scale from origin, title, zoom menu, download, ← →, "1 of N", Esc | PARTIAL | A fit/actual toggle instead of a zoom menu. No scale-from-origin. File name instead of alt-text title. | `cockpit/lightbox.rs:1-120,248` (bcef8604, cda2c1c5) |
| 41 | RP9 | File tree with a fuzzy filter that reveals the current file; changed / all sections | BUILT | The Review's tree only. No filter field, no sections, no reveal. **Audit 11-04:** 804ba262: tree with filter and git colours | `review.rs:2070` `tree_body` (3c2de915) |
| 42 | RP19 | One continuous highlighted diff; a tree click scrolls to the file | PARTIAL | A tree click opens the whole file instead of scrolling to it. **Audit 11-04:** 49170fa6: one list of files with hunks | `review.rs:1406-1445,877-904` (3c2de915) |
| 43 | RP16 | Changes scope menu (last turn, uncommitted, unstaged, staged, committed, branch) | PARTIAL | Scope tokens rather than a dropdown. No unstaged/staged split. **Audit 11-04:** 49170fa6: scope dropdown (All changes, Uncommitted, Compare against a branch); no staged/unstaged split | `review.rs:2351-2380` (3c2de915) |
| 44 | RP17 | Changes toolbar (word diffs, whitespace, wrap, collapse all, jump to file) | PARTIAL | Keyboard only. No toggles, no jump-to-file. **Audit 11-04:** 49170fa6: ⋯ has Collapse/Expand all, Word wrap, Hide whitespace, Refresh; no word diffs or jump to file | `review.rs:500-530`; `panel/changes.rs:88` |
| 45 | RP18 | Changes file tree with filter, +N counts and stage buttons | PARTIAL | Per-file blocks with +N −N. No filter, no staging. **Audit 11-04:** 49170fa6: files with hunks, reviewed marks; no filter or staging | `panel/changes.rs:142` |
| 46 | RP20 | Changes empty state with "View branch diff" | BUILT | "No uncommitted changes" text only. **Audit 11-04:** Changes empty state links to View branch diff (BUILT, not ticked) | `review.rs:2340-2345` |
| 47 | SA2 | An agent opens in the right panel (←, title, maximise, ×) | PARTIAL | It drills in in place of the Pane's transcript. No maximise or ×. **Audit 11-04:** Lane G1 (subagent drill-in) | `transcript_glue.rs:203`, `subagents.rs:467-520` (3b33960c, 64baa4c6) |
| 48 | SA3 | Agent view: prompt, model badge, own rows, result | PARTIAL | No model badge (`AgentInfo` has no model). **Audit 11-04:** Lane G1 (subagent drill-in) | `subagents.rs:467-520`; `core activity.rs:70` |
| 49 | SA4 | Subagents list (Active · N / Done · N) | MISSING | **Audit 11-04:** Lane G1 (subagent drill-in) | — |
| 50 | PG2 | Page pane beside the transcript (rendered `.html`) | PARTIAL | Behind the non-default `cef` feature: a plain `cargo run` shows only the can't-show note. `install-app.sh` turns it on. Loads no URLs. | `panel/preview.rs:23-30`, `ferrite-pane/src/preview.rs` (fa36929a) |
| 51 | RP6 | Title-bar ⋯: Side chat ⌘;, iOS Simulator, Keep computer awake | MISSING | KNOWN: the side chat (to be built). No ⋯, simulator or keep-awake. | `view_head.rs:23-25`; LEDGER Lane A |
| 52 | NS6 | Folder picker: search, "No folder ✓", Recent, "Open folder…" | PARTIAL | Has the project list with ✓, "Add project…" and a typed path row. No search field, no Recent heading, no "Open folder…" dialog. | `cockpit.rs:7574` `band_rows` |
| 53 | SH18 | Search threads | PARTIAL | Titles only, not message content. | `palette.rs:1-27` |
| 54 | XD6 | "Starting your chat…" shimmer; composer disabled while starting | BUILT | A "Starting" line with the mark. No shimmer text, and the composer stays enabled. **Audit 11-04:** 613e6bf9: the chip shimmers while the Session starts | `pane/working.rs:89` (2f77f419) |
| 55 | MO5 | Thread switch snaps, skeleton chips until loaded | BUILT | The snap is built. No skeletons (same as XQ7). **Audit 11-04:** 613e6bf9 | (1c1e5980) |
| 56 | CO18 | Highlighted row's description in a side tooltip | PARTIAL | An inline muted column, with a tooltip on mouse hover only. The keyboard-highlighted row gets none. | `pane/menus.rs:46` |
| 57 | TR3b | Edit a sent prompt in place (Codex full-width editor) | PARTIAL | Rewinds and refills the composer, as Claude does. No in-place editor. | `transcript_glue.rs:311` (rewind with edit) |
| 58 | TR16 | File names in prose are links (Codex adds a file-type icon) | PARTIAL | Claude's form is built. No file-type icon. | `attachments.rs:500` (8b846be4) |
| 59 | TR17 | Scroll-to-bottom: round, translucent ↓ | PARTIAL | Opaque fill. | `transcript.rs:1699` (58720cf7) |
| 60 | AP3 | Approval card: question, intent, `$ command` well with syntax colours | PARTIAL | The command is one flat green, not syntax-coloured. | `decision.rs:131,351,896` (ed5541cf) |
| 61 | SH10 | Fork › (Fork chat / Fork chat in new worktree) | PARTIAL | A single Fork row. | `context_menu.rs:373-380` |
| 62 | SH11 | Copy › (deeplink, Markdown, working directory) | PARTIAL | Copy path and Copy link (a CLI command, since there is no URL scheme). No Copy as Markdown. | `context_menu.rs:395-396` |
| 63 | SH12 | Open in › Finder, Terminal, Xcode with app icons | PARTIAL | Finder and Terminal, no icons. | `context_menu.rs:436-440` (0e42e74b) |
| 64 | SH19 | Open a thread in a new window | MISSING |  | — |
| 65 | SH8 | Thread header: folder icon, title, ⋯, checklist toggle, new-window button | PARTIAL | ▾ instead of ⋯ by design (components.md:15). No checklist toggle and no new-window button. | `view_head.rs:243,437,196` |
| 66 | RP7 | File viewer tab strip, preview tabs in italic | BUILT | Overview plus one preview tab that is replaced in place. **Audit 11-04:** 804ba262: preview tabs in italic | `panel/preview.rs:1-36`, `panel.rs:23-26` (8ebbb97e) |
| 67 | RP8 | File viewer: breadcrumb, line numbers, current line, soft wrap, loading skeleton | BUILT | Line numbers and the target line. The path is plain text. No wrap, no skeleton. **Audit 11-04:** 804ba262: breadcrumb, lines, wrap, current line | `panel/preview.rs:742-822,628` (8ebbb97e) |
| 68 | RP22 | Right panel as browser-like tabs (+, ×) | PARTIAL | One scroll of sections, plus a preview tab. | `panel.rs:1-6,23-26` |
| 69 | RP23 | Maximise panel with a docked compact composer | PARTIAL | Expand widens a preview only. | `panel.rs:62-73` (fa36929a) |
| 70 | RP21 | Files: inline "Show changes"; "Open ⌄" split | MISSING |  | — |
| 71 | RP4 | Title-bar tool: Browser (+ launcher, URL bar, tools grid) | MISSING | The page pane loads no URLs. | `view_head.rs:23-25` |
| 72 | XD1 | "Ran commands" group nesting shell cards (max height, scroll, fade) | PARTIAL | No bottom fade mask. | `pane/output.rs:499` (12 lines, "… +N lines") |
| 73 | TR3 | User bubble hover row: ✓✓, relative time, copy, rewind, branch | PARTIAL | No delivered ✓✓. | `transcript.rs:1185-1215` (3b33960c), `:1843` (bfbeb3e2) |
| 74 | TR2 | User bubble right-aligned, filled, radius ~12–16, ≤70% of the column | PARTIAL | Radius 10 (geometry.md:28 records the choice). Width cap 84%. | `pane.rs:3480`, `theme.rs:898,1532` (58720cf7, 241f18e8) |
| 75 | XQ5 | Composer metrics (~98 pt tall, radius ~20, aligned with the column) | PARTIAL | Radius 12. 16 px wider than the column on each side on purpose (geometry.md:63). | `theme.rs:2276` (96), `R_FLOAT` 12, `COMPOSER_MAX_W` |
| 76 | SK2 | Discover grids and install | MISSING |  | — |
| 77 | SK3 | Your skills grouped by source, with ⋯ | MISSING |  | — |
| 78 | SK4 | Skill detail (rendered SKILL.md, enable, uninstall, Try now) | MISSING |  | — |
| 79 | SK6 | Skill templates carousel | MISSING |  | — |
| 80 | CO5 | + menu: Connectors ▸ (Browse, Manage, Claude in Chrome toggle) | PARTIAL | MCP servers with their state plus "Manage connectors". No Browse and no Chrome toggle (LEDGER: "left out"). Hidden unless the session reports servers. | `cockpit.rs:8324` `attach_submenus` (3b33960c) |
| 81 | CO7 | Model menu: "Model · this session" header, number keys 1–4 | DIFFERENT BY DESIGN | No header. **Audit 11-04:** Claude's own model-menu capture (claude/model-menu.png) has no header | `cockpit.rs:12637` digits 1–9 (d6275d4b) |
| 82 | CO9 | More models ▸ | PARTIAL | Holds the other provider's models (a handover), not more of the same provider's. | `cockpit.rs:12474` (d6275d4b) |
| 83 | XSQ1 | Codex queue strip: ↪ Steer, delete, ⋯ (Edit, Open in side chat, Turn off queueing) | PARTIAL | Three buttons. No ⋯, no "Open in side chat", no "Turn off queueing". **Audit 11-04:** Still three buttons | `cockpit.rs:8090` |
| 84 | XCO1 | Codex environment tray above the box ("Choose project · This computer") | PARTIAL | Project plus branch/workspace. No machine. | `pane/composer.rs:216` `env_strip` (3b33960c) |
| 85 | XCO3 | Spinner in the composer while running; effort in dimmer ink on the chip | PARTIAL | Dim effort is built. No spinner. **Audit 11-04:** No spinner in the composer | `pane/composer.rs:22` |
| 86 | NS3 | Empty-state suggestion list that drops items once they no longer apply (import CLI sessions, turn on Auto, choose a folder) | MISSING | Only the `/import` slash row exists. | LEDGER "home suggestions … deferred" |
| 87 | NS2 | Environment card ("Default", "Choose environment") | MISSING | No environment card. The env strip (Project, Workspace) stands in; machines are picked only in the nav. | — |
| 88 | TR1 | Transcript header "Started on this computer · folder · Save as environment" | MISSING | "Save on Environment?" waits on the owner. | — |
| 89 | AP10 | Confirm dialog before turning on Full access / bypass | MISSING |  | DENSITY.md:101 |
| 90 | IM5 | Tool rows with image thumbnails and a hover carousel | MISSING |  | — |
| 91 | IM1 | "Viewed N images" row with thumbnail tiles | MISSING | Tool results carry no images. | — |
| 92 | AT4 | Annotate in the lightbox (pen, shapes, text, colour, undo, Save) | MISSING |  | — |
| 93 | AT7 | Sketch canvas | MISSING |  | — |
| 94 | XD10 | Live markdown in the composer (``` → code block) | MISSING |  | — |
| 95 | XD3 | Colour swatch beside hex values | MISSING |  | — |
| 96 | XD4 | File reference as "From ◉ theme.rs" | MISSING |  | — |
| 97 | XCO8 | "Full access is on" callout with "Don't show again" | MISSING |  | — |
| 98 | XCO6 | Codex + menu items: Attach frontmost app, Goal, Record a skill | MISSING | Plan mode is out of scope; Sketch is AT7. | — |
| 99 | CO3 | + menu: Import GitHub issue | MISSING |  | — |
| 100 | CO10 | Fast mode section | MISSING |  | — |
| 101 | SH13 | Add scheduled task | MISSING |  | — |
| 102 | OP1 | Code Review page | MISSING | The Review-branch diff (⇧⌘D) is a different feature. | — |
| 103 | RP10 | First-run coach mark | MISSING |  | — |
| 104 | RP11 | Viewer ⋯ settings (wrap, indent guides, tab size) | MISSING |  | — |
| 105 | MO6 | Lightbox image scales up from its spot | MISSING | Only the backdrop fades. | `cockpit/lightbox.rs:235` |

## Full table

ID prefixes: NS new session, CO/XCO composer (Claude/Codex), XD Codex deeper pass, QS/XSQ queue and steer, AT attachments, TR/XTR transcript, ER errors and stop, LT long threads, XQ/QB measured quality bar, AP approvals, SH sidebar and header, SN shell, OP other pages, RP right panel and title-bar tools, SA subagents, IM images, SK skills, PG pages, MO motion, US usage data, OS out of scope.

| ID | Feature | Status | Evidence (file:line, commit) | Note |
|---|---|---|---|---|
| NS1 | Empty state: animated mascot / logo mark | DIFFERENT BY DESIGN | `pane.rs:1388` `start_hero`, `components.rs:778` `embossed_mark` (3b33960c); LEDGER "The Codex-style hero" | A still watermark, not animated. |
| NS2 | Environment card ("Default", "Choose environment") | MISSING | — | No environment card. The env strip (Project, Workspace) stands in; machines are picked only in the nav. |
| NS3 | Empty-state suggestion list that drops items once they no longer apply (import CLI sessions, turn on Auto, choose a folder) | MISSING | LEDGER "home suggestions … deferred" | Only the `/import` slash row exists. |
| NS4 | Empty-state placeholder "Describe a task…" / hero "What should we build?" | BUILT | `pane/composer.rs:910` (6c3825e4); `pane.rs:1412` (3b33960c) |  |
| NS5 | Chips under the composer: computer, folder, worktree, model+effort, permission, usage ring | BUILT | `cockpit.rs:8066` `composer_toolbar`, `:7951` `draft_band_chips` (bfa6b2e6) | A draft (new session) has no permission chip and no usage ring (`mode: None, ring: None`). No "This computer". Worktree is a Workspace picker, not a checkbox. **Audit 11-04:** Permission word and usage ring on a draft (ledger item 22; ring not run) |
| NS6 | Folder picker: search, "No folder ✓", Recent, "Open folder…" | PARTIAL | `cockpit.rs:7574` `band_rows` | Has the project list with ✓, "Add project…" and a typed path row. No search field, no Recent heading, no "Open folder…" dialog. |
| CO1 | + menu: Add files or photos ⌘U | BUILT | `cockpit.rs:8241`; `keymap.rs:253` (6c3825e4) |  |
| CO2 | + menu: Add folder | BUILT | `cockpit.rs:8242`, `:8470` (6c3825e4) |  |
| CO3 | + menu: Import GitHub issue | MISSING | — |  |
| CO4 | + menu: Slash commands | BUILT | `cockpit.rs:8243` (6c3825e4) |  |
| CO5 | + menu: Connectors ▸ (Browse, Manage, Claude in Chrome toggle) | PARTIAL | `cockpit.rs:8324` `attach_submenus` (3b33960c) | MCP servers with their state plus "Manage connectors". No Browse and no Chrome toggle (LEDGER: "left out"). Hidden unless the session reports servers. |
| CO6 | + menu: Plugins ▸ with a submenu per plugin | BUILT | `cockpit.rs:8411` (7a300afe) | Only plugins that bring commands. Claude only, since Codex reports no plugins. |
| CO7 | Model menu: "Model · this session" header, number keys 1–4 | DIFFERENT BY DESIGN | `cockpit.rs:12637` digits 1–9 (d6275d4b) | No header. **Audit 11-04:** Claude's own model-menu capture (claude/model-menu.png) has no header |
| CO8 | Effort submenu: explanatory line, Recommended tag, usage-warning pill, Ultracode toggle | PARTIAL | `cockpit.rs:12690` `effort_submenu` (9e42e44e) | A bare ladder with ✓. None of the explanations, tags or pills. **Audit 11-04:** Recommended tag on Medium built; explanation line, Max usage pill and Ultracode need data Ferrite lacks |
| CO9 | More models ▸ | PARTIAL | `cockpit.rs:12474` (d6275d4b) | Holds the other provider's models (a handover), not more of the same provider's. |
| CO10 | Fast mode section | MISSING | — |  |
| CO11 | Permission menu: header, two-line rows (name + consequence) | BUILT | `cockpit.rs:11735`, `theme.rs:521`, `menu.rs:165` (1c1e5980, 6c3825e4) |  |
| CO12 | Permission menu: number keys, Recommended/Default tags | BUILT | `cockpit.rs:11744-11752` (6c3825e4) |  |
| CO13 | Permission chip tinted when Manual | BUILT | `pane/composer.rs:38`, `theme.rs:514` (d6275d4b) | Only Manual is amber. Bypass is not tinted. |
| CO14 | Placeholder changes with state ("Type / for commands"; "↵ to send, ⌘↵ to queue") | BUILT | `pane/composer.rs:895-905` (1c1e5980) | **The running hint is false.** ↵ and ⌘↵ both bind `cockpit::Submit` (`keymap.rs:200,204`): on Claude both queue, on Codex both steer. **Audit 11-04:** a8ae6b24: the hint reads "↵ to steer this turn" |
| CO15 | Send becomes ■ stop while running | BUILT | `pane/composer.rs:97,165` (58720cf7) | Stop shows only while the box is empty. |
| CO16 | Slash picker opens on "/" with a "Type to filter" hint | PARTIAL | `cockpit.rs:6106` `derive_menu` | KNOWN: mono slash picker. Also no "Type to filter" hint. |
| CO17 | Slash picker fuzzy filter | BUILT | `cockpit.rs:9626` via `fuzzy.rs` (09f66c0f) |  |
| CO18 | Highlighted row's description in a side tooltip | PARTIAL | `pane/menus.rs:46` | An inline muted column, with a tooltip on mouse hover only. The keyboard-highlighted row gets none. |
| CO19 | Skills and commands in one list, with a source label per row | BUILT | `cockpit.rs:9643`, `CommandSource::tag` (7a300afe) | Codex's built-ins and prompts are untagged, because Codex gives no data. |
| CO20 | Picking a skill inserts `/name` as a chip | BUILT | (7a300afe) | KNOWN: skill chips. **Audit 11-04:** Superseded: a skill is blue ink, as Claude (102c2e76, db13e437) |
| CO21 | Usage ring hover text | BUILT | `pane/usage.rs:28` (1c1e5980) | Live threads only, not drafts. |
| CO22 | Usage ring popover: plan, bar, "See all →" | BUILT | `pane/usage.rs:87` (1c1e5980, b64267b1) | "See all →" left out by a recorded choice (LEDGER, CP7). Codex sends no plan name, so it has no header (see US1). |
| XCO1 | Codex environment tray above the box ("Choose project · This computer") | PARTIAL | `pane/composer.rs:216` `env_strip` (3b33960c) | Project plus branch/workspace. No machine. |
| XCO2 | Codex model popover: effort slider, model › link, reset, speed | DIFFERENT BY DESIGN | `cockpit.rs:12458` "Claude's `Effort Medium ›`" | Follows Claude's menu with an Effort submenu. |
| XCO3 | Spinner in the composer while running; effort in dimmer ink on the chip | PARTIAL | `pane/composer.rs:22` | Dim effort is built. No spinner. **Audit 11-04:** No spinner in the composer |
| XCO4 | Codex model list: "Select model", Default row, ✓, side tooltip | DIFFERENT BY DESIGN | `cockpit.rs:12558` doc | Claude's shape. |
| XCO5 | Codex + menu: full-width, sectioned, inline descriptions | DIFFERENT BY DESIGN | LEDGER Lane C "+ menu matches Claude" |  |
| XCO6 | Codex + menu items: Attach frontmost app, Goal, Record a skill | MISSING | — | Plan mode is out of scope; Sketch is AT7. |
| XCO7 | Codex permission menu: question header, icons, descriptions | DIFFERENT BY DESIGN | `cockpit.rs:11730` "Claude's mode menu" |  |
| XCO8 | "Full access is on" callout with "Don't show again" | MISSING | — |  |
| XCO9 | Drafts persist per thread | BUILT | `core/cockpit.rs:681` | Kept in memory only while the thread's Pane is open. Lost on park and on restart. **Audit 11-04:** 4123d281: drafts persist per thread |
| XD6 | "Starting your chat…" shimmer; composer disabled while starting | BUILT | `pane/working.rs:89` (2f77f419) | A "Starting" line with the mark. No shimmer text, and the composer stays enabled. **Audit 11-04:** 613e6bf9: the chip shimmers while the Session starts |
| XD9 | Ghost follow-up suggestion, Tab to accept | BUILT | `cockpit.rs:5631`, `pane/composer.rs:892` (6c9053ac) | The hint reads "⇥ accept". |
| XD10 | Live markdown in the composer (``` → code block) | MISSING | — |  |
| QS1 | Queue strip with send now, edit and remove per message | BUILT | `pane/composer.rs:678`, `cockpit.rs:8090` (1c1e5980, 73efd684) |  |
| QS2 | Queue strip fades in over ~100 ms | DIFFERENT BY DESIGN | `pane/composer.rs:676`; motion-measured.md "Snap both ways" |  |
| QS3 | ↵ while running steers: a pending bubble in the transcript with "Send now" | DIFFERENT BY DESIGN | `cockpit.rs:5486-5491` `needs_queue` → `Cockpit::queue`; `cockpit.rs:8080-8100` (the row actions); `pane/composer.rs:665-760` `queue_card`, `:780-794` `queued_sends_now`, `:799-808` `queued_send_now`; `engine.rs:3253-3283` (73efd684) | **Re-checked 2026-11-04 (disputed).** After ↵ during a turn, this is what the user sees. The composer clears. A card appears just above the composer box, one row per queued message, up to 3 rows before it scrolls (`QUEUE_ROWS_SHOWN`). The top row starts with **"1 message queued"** (or "N messages queued") in bold. Each row shows its message on one cut grey line, then three icon buttons: ↵ (tooltip "Send now"), ✎ "Edit" and × "Remove". On Codex, a message already folding into the turn adds a grey "Steering" and loses Edit and Remove. While the provider hasn't yet acknowledged a message, its row shows no buttons. **Nothing appears in the transcript yet:** the prompt bubble is drawn only when the provider reports that the message started (`QueueEvent::Started` → `Input::Prompt`). So the owner is right that "Send now" exists, and the original row is right that there is no pending bubble. The keyboard works on the latest message: ↵ on an empty line takes it back, ⌫ removes it, ↑ edits it. What Send now does: with no turn running, the message goes next. On Codex mid-turn, it is taken back and re-sent as a steer. On Claude mid-turn, it **interrupts the turn** (`engine.rs:3792`). The CLI keeps queued input through a plain interrupt, so the message then runs as the next turn. **Audit 11-04:** Ledger standing decision 2026-11-04: no pending bubble; the queue card is enough |
| QS4 | Steer delivered at the next tool boundary, answered in the same turn | PARTIAL | Codex `providers/codex.rs:591` `turn/steer` (2f48425d). Claude: `engine.rs:3593-3626` `queue` → `:3628-3664` `enqueue` → `claude.rs:628-644` (an `isAsync` user frame written mid-turn); `providers/claude/queue.rs:38-62` (`command_lifecycle`); `providers/mod.rs:172-177` (`steers()` defaults to false for Claude; Codex true at `:409-412`) | **Corrected 2026-11-04.** The old note ("on Claude the message waits for the turn to end") was wrong. Ferrite never holds a Claude message until the turn ends: it writes it to the CLI's stdin at once, while the turn runs. The CLI owns the schedule. The Agent SDK's own declaration (0.3.285 `sdk.d.ts:5648`, `user_message_uuids`) says a turn consumes "any queued user message folded into the running turn between tool rounds". `docs/research/claude-native-queue.md` records the same, plus batching of pending messages, though it never ran a probe with tools. So Claude already gets mid-turn steering when the turn has another tool round, and the message runs as the next turn when it doesn't. **What is missing is Ferrite's side:** (1) `ClaudeSession` doesn't report `steers()`, so the strip says "Queued", never "Steering", and Send now interrupts instead of letting the fold happen (`engine.rs:3789-3792`). (2) A folded message arrives as `Input::Prompt`, which restarts the turn clock and ends the turn's plan (`transcript.rs:1968-1985`). (3) The result's `user_message_uuids` is never read, so nothing confirms which turn took the message (HD5). (4) Ferrite has no recorded capture of a fold with tools; one is needed before `steers()` is switched on. The SDK's `priority: 'now' \| 'next' \| 'later'` user-message field (`sdk.d.ts:6170`) is unused; the declaration doesn't define its meaning. |
| QS5 | A queued message runs as the next turn | BUILT | engine/queue.rs (2f48425d) |  |
| QS6 | Background shell completion re-wakes the session | BUILT | `providers/claude/activity.rs:155` (41b3d652) |  |
| XSQ1 | Codex queue strip: ↪ Steer, delete, ⋯ (Edit, Open in side chat, Turn off queueing) | PARTIAL | `cockpit.rs:8090` | Three buttons. No ⋯, no "Open in side chat", no "Turn off queueing". **Audit 11-04:** Still three buttons |
| XSQ2 | Steer places the message inline in the same turn | BUILT | `providers/codex/activity.rs:1267` (2f48425d) | Codex. |
| AT1 | ⌘U opens the system file dialog | BUILT | `keymap.rs:253`, `cockpit.rs:8472` (6c3825e4) |  |
| AT2 | Composer image thumbnail ~90 px, × on hover | BUILT | `attachments.rs:252`, `theme.rs:2229` (6733a514) |  |
| AT3 | Thumbnail click opens the lightbox | BUILT | `attachments.rs:~284` (6733a514) |  |
| AT4 | Annotate in the lightbox (pen, shapes, text, colour, undo, Save) | MISSING | — |  |
| AT5 | Text file in the composer as a card with a first-page preview | PARTIAL | `attachments.rs:134` | A small chip with an icon and name. |
| AT6 | Composer grows to fit the tray | BUILT | `pane/composer.rs:470` (58720cf7); ease (401c7d8d) |  |
| AT7 | Sketch canvas | MISSING | — |  |
| AT8 | Drag-and-drop or paste a file or image | BUILT | `pane/composer.rs:425` (8e90ac1d); `composer.rs:490,1771` (249a7be4) |  |
| AT9 | Failed attachment state (red tile, "Failed") | MISSING | — |  |
| AT10 | Sent image renders large and right-aligned above the bubble | BUILT | `pane.rs:3578-3620` `inline_image` (bc5a2473) | Up to 480×320. |
| AT11 | Non-image attachment as a pill above the bubble; click opens the default app | PARTIAL | `pane.rs:3219-3240` | A chip inside the bubble. A click opens Ferrite's preview. |
| XQ5 | Composer metrics (~98 pt tall, radius ~20, aligned with the column) | PARTIAL | `theme.rs:2276` (96), `R_FLOAT` 12, `COMPOSER_MAX_W` | Radius 12. 16 px wider than the column on each side on purpose (geometry.md:63). |
| TR1 | Transcript header "Started on this computer · folder · Save as environment" | MISSING | — | "Save on Environment?" waits on the owner. |
| TR2 | User bubble right-aligned, filled, radius ~12–16, ≤70% of the column | PARTIAL | `pane.rs:3480`, `theme.rs:898,1532` (58720cf7, 241f18e8) | Radius 10 (geometry.md:28 records the choice). Width cap 84%. |
| TR3 | User bubble hover row: ✓✓, relative time, copy, rewind, branch | PARTIAL | `transcript.rs:1185-1215` (3b33960c), `:1843` (bfbeb3e2) | No delivered ✓✓. |
| TR3b | Edit a sent prompt in place (Codex full-width editor) | PARTIAL | `transcript_glue.rs:311` (rewind with edit) | Rewinds and refills the composer, as Claude does. No in-place editor. |
| TR4 | Answer action row: copy, fork, ⋯, time | BUILT | `transcript.rs:1100-1135`, answer.rs (0e42e74b) | Pin, read aloud and rate left out: "nothing backs them" (LEDGER Lane A). |
| TR4b | Tooltips on message action icons | BUILT | `transcript.rs:1572-1587` (0e42e74b, c66f5f5a) |  |
| TR5 | One prose summary line per tool group, with "(1 stopped) (1 failed)" | BUILT | `pane/text.rs:401-442` `group_summary` (095df35b) | No "stopped": `ToolState` has no Stopped (`core transcript.rs:526`), so a stopped call reads "without a result". **Audit 11-04:** 626bd327: "(1 stopped)" |
| TR6 | Live step line with shimmer | BUILT | `pane/working.rs:468,363` (1c1e5980, bc5a2473) | KNOWN: motion. The live step is on the working line, not the group line. |
| TR7 | Expanded group: bordered rows, status words coloured (Failed red, Stopped grey) | BUILT | `pane.rs:4165`, `:4064` (095df35b) | No grey Stopped (same core gap as TR5). **Audit 11-04:** 626bd327: ToolState::Stopped never reads red |
| TR8 | Failed command: `$ cmd` well, "Exit code 1", stderr, calm | BUILT | `pane/output.rs:190` (1c1e5980) |  |
| TR9 | A Read row click opens the file in the right panel | BUILT | `pane/text.rs:179` → `rich.rs:593` → `file_links.rs:85` `cx.open_url` | A plain click on a tool row's path goes to the OS opener. Only prose file chips reach the panel. **Audit 11-04:** 05e9f63f: the Read row path opens its file in Files |
| TR10 | Long-running command: "View live output ▸" | PARTIAL | — | The core keeps `output_tails`, but nothing draws them. **Audit 11-04:** HD64: background output opens in the Background pane. Foreground commands still have no "View live output" |
| TR11 | Rows switch to past tense when done | PARTIAL | `pane/text.rs:93-104` (8cea78b3) | Ferrite's verbs do. The provider's own description ("Waiting 15 seconds…") stays in the present tense (`text.rs:253`). |
| TR12 | Todo card: ○, spinner, ✓, ⊖ blocked | BUILT | `pane/working.rs:520` (ed48e679) | No ⊖ state. The card lives in the working tail, so it vanishes when the turn ends. **Audit 11-04:** 39785ffd, 626bd327: the plan stays under the turn; a blocked step shows a bar in a ring |
| TR13 | Todo re-planned in place | BUILT | `progress.rs:168-226`; Claude `claude/wire.rs:513-560`, Codex `codex/wire.rs:407` | Both providers. |
| TR14 | Live status line: glyph, "9s · 87 tokens" / "Thinking" | BUILT | `pane/working.rs:330-440` (bfbeb3e2) |  |
| TR15 | Markdown tables with mono chips and a header hairline | BUILT | `rich.rs:916-932` (ed5541cf) |  |
| TR16 | File names in prose are links (Codex adds a file-type icon) | PARTIAL | `attachments.rs:500` (8b846be4) | Claude's form is built. No file-type icon. |
| TR17 | Scroll-to-bottom: round, translucent ↓ | PARTIAL | `transcript.rs:1699` (58720cf7) | Opaque fill. |
| TR18 | Thin overlay scrollbar on the transcript | BUILT | transcript list ~`transcript.rs:2470`; DETAIL-AUDIT B1 | No scrollbar. The minimap shows only on hover. **Audit 11-04:** a8ae6b24: thin overlay scrollbar |
| XTR1 | "Working for Ns" turn header that collapses to "Worked for Ns ›" and expands to commentary, plan and commands | PARTIAL | `pane/text.rs:16` `turn_stamp` (8b846be4) | Only a "Worked for Ns" stamp at the end, and it does not expand. No live header. **Audit 11-04:** Live line has its ▸ (ad8c7239). Claude has no "Working for" header (Codex does); the end stamp still does not expand: needs a product call |
| XTR2 | Plan inline as checkboxes, kept in the turn | BUILT | `pane/working.rs:520` (ed48e679) | Ticks live, then disappears at turn end. **Audit 11-04:** 39785ffd: the plan is kept with its turn |
| XTR3 | "Thinking" as shimmer text | BUILT | `pane/working.rs:363` | KNOWN: motion. |
| XTR4 | Streaming fades new tokens and rows in | BUILT | `rich.rs:550`, `transcript.rs:1772` (241f18e8, 2735a69f) | KNOWN: motion. |
| XTR5 | Code blocks as cards: language header, wrap toggle, copy | PARTIAL | `rich.rs:693-760` (05fc2232) | Copy on hover and an HTML preview. No language header, no wrap toggle. **Audit 11-04:** ef52f8ba: language header and Copy built; still no wrap toggle |
| XTR6 | Diffs in red/green text | BUILT | `pane/output.rs:904` (f70a7d21) |  |
| XTR8 | Inline code in the user bubble as chips | BUILT | `pane::code_spans` (ed5541cf) |  |
| XTR9 | File output card with "Open in ⌄" | BUILT | `transcript/outputs.rs` (cdc328f3) | Finder and the default app only. |
| XTR13 | Turn-timeline minimap | BUILT | `transcript.rs:2077` (bc5a2473) | On the right edge, shown only on hover. |
| XD1 | "Ran commands" group nesting shell cards (max height, scroll, fade) | PARTIAL | `pane/output.rs:499` (12 lines, "… +N lines") | No bottom fade mask. |
| XD2 | One live activity row, swapped in place, present tense | BUILT | `pane/working.rs:468` (2735a69f) | The icon is the spinner, not one per kind. |
| XD3 | Colour swatch beside hex values | MISSING | — |  |
| XD4 | File reference as "From ◉ theme.rs" | MISSING | — |  |
| XD5 | A new turn scrolls the prompt to the top, leaving room below | BUILT | `transcript.rs:855` Runway; `transcript_glue.rs:398` `allow(dead_code)` without `visual-reference` | **SCENE-ONLY:** the code exists but only reference scenes reach it. A real send scrolls to the bottom. **Audit 11-04:** a8ae6b24: a real send scrolls its prompt to the top |
| XD7 | Answer ⋯ "Branch in new chat"; time on hover | BUILT | answer.rs "Fork from here" (0e42e74b) |  |
| ER1 | Stop: "You stopped after Ns", the row says stopped, the todo stays, the composer is back | BUILT | `pane.rs:3762` "interrupted · Ns"; `transcript.rs:2321` | The owner wants Codex's form. The row says "no result", and the todo vanishes. **Audit 11-04:** 626bd327: "You stopped after Ns"; the plan stays; Stopped state |
| ER2 | Deny ends the turn; the summary records "(1 stopped)"; the todo shows ⊖ | BUILT | `claude/wire.rs:1174` sends `deny` with no interrupt | Deny doesn't end the turn: the model carries on. **Audit 11-04:** 626bd327, 37ec2662: deny sends interrupt; the turn closes aborted_tools |
| ER3 | Turn and provider errors shown calmly, with Retry | BUILT | `pane.rs:2251` `failure_strip` (73ea2552) | A red x on a wash, less calm than Codex. |
| ER4 | A failure shows once | BUILT | `transcript/rows.rs:783` (bfbeb3e2) |  |
| LT1 | Date dividers between days and on resume | BUILT | `transcript/rows.rs:509-520`, `transcript.rs:1873` (bfbeb3e2) | Only on a day change, not on resume. No time. |
| LT2 | A long thread opens at the end; older content loads lazily | BUILT | `transcript/scroll.rs:29` (d5f7d221) | Lazy layout, but the whole history is held in memory. |
| LT3 | Long work summaries collapse ("Worked for ›") | PARTIAL | — | Tool groups collapse; whole turns don't (see XTR1). |
| LT4 | PageUp/PageDown, find in the transcript | BUILT | `transcript.rs:837` (bfbeb3e2); ⌘F `cockpit.rs:6036` (3c2de915) |  |
| XQ2 | Transcript column 736, composer edges aligned | DIFFERENT BY DESIGN | `theme.rs:1510` (241f18e8); geometry.md:63 | Column 736; the composer is wider on purpose (Claude's frame). |
| XQ3 | Body text 14/23 | DIFFERENT BY DESIGN | `theme.rs:749,799-803`; typecolour.md:44 | 14/21, measured from Claude. |
| XQ7 | Loading a thread: centred spinner, model-chip skeleton | BUILT | switch snaps (LEDGER Lane M) | The instant switch is built. No loading spinner, no skeleton chips. **Audit 11-04:** 613e6bf9: the model chip shimmers while the Session starts |
| TXS | Selection pops "Start a side chat · Reply" | BUILT | `transcript/reply.rs:124-190`, `cockpit.rs:2045` (c35a2791, 0e42e74b) | KNOWN: the side chat is a fork. |
| AP1 | Approval inline in the transcript (Claude) vs replacing the composer (Codex) | BUILT | `subagents.rs:230` `request_frame`, `decisions.rs:50` (6c3825e4) | Claude's. Codex's form is a recorded choice against (components.md:23). |
| AP2 | "Needs your input" pill row | BUILT | `decision.rs:813`, `subagents.rs:256` (6c3825e4) | Solo only. Group Panes leave it out on purpose. |
| AP3 | Approval card: question, intent, `$ command` well with syntax colours | PARTIAL | `decision.rs:131,351,896` (ed5541cf) | The command is one flat green, not syntax-coloured. |
| AP4 | Deny (Esc) plus a split primary | BUILT | `decision.rs:1270-1343`, `keymap.rs:418` (6c3825e4) | Deny shows no Esc hint. Esc denies only when the composer is empty. |
| AP5 | The split default adapts (Allow once, then Always allow) | DIFFERENT BY DESIGN | `decision.rs:1311`; components.md:23 | "Making the broader grant the default is the wrong default." |
| AP6 | "Allow and switch to Auto" | BUILT | `subagents.rs:774-790,1520-1560` (0e42e74b) |  |
| AP7 | "Allow similar commands ⓘ" | DIFFERENT BY DESIGN | `decision.rs:1417` "Always allow <pattern>" plus a note line; components.md:23 | The same job, done in words. |
| AP8 | Sidebar dot or "Needs approval" pill while waiting; a bell dot | BUILT | `nav.rs:1531` `state_mark`, `:2124` (folded rail) | Orange dot built. No bell or badge dot. **Audit 11-04:** 6b6072a5: bell dot |
| AP9 | The turn clock keeps counting during an approval | BUILT | `pane/working.rs:40`, `cockpit.rs:1834` | The working line is hidden while a decision stands. **Audit 11-04:** 626bd327: the working line stays over a waiting Decision (Solo) |
| AP10 | Confirm dialog before turning on Full access / bypass | MISSING | DENSITY.md:101 |  |
| AP11 | Non-shell approvals (edits, fetch, MCP) and questions/forms | BUILT | `pane.rs:2631`, `decision.rs:139`, `subagents.rs:1654,1913,1992` (73ea2552, b320bd8e) | MCP and other tools fall back to pretty-printed JSON. |
| SH1 | Left icon rail (Home, Scheduled, Plugins, …) | DIFFERENT BY DESIGN | geometry.md:71 "Ferrite has already chosen Claude's" | Views plus a folded 52 px rail (`nav.rs:2101`). |
| SH2 | Sidebar header: bell with a dot when a turn finishes; search | BUILT | `nav.rs:881,895`; `cockpit.rs:13379,13347` (1c1e5980, 3b33960c) | The bell opens Activity, but it never gets a dot. Search is the ⌘K palette. **Audit 11-04:** 6b6072a5: the bell wears a dot; bell sections 8c514306, bca332a1 |
| SH3 | Rows: New chat, projects with "Show more", Recents | BUILT | `cockpit.rs:13431`; `nav.rs:162,273,290` (3b33960c) |  |
| SH4 | Row hover shows pin and archive icons | DIFFERENT BY DESIGN | `cockpit.rs:14243`, `nav.rs:1071`; components.md:14 | Hover shows ⋯ (Claude's). |
| SH5 | Pinned threads (pin/unpin, pinned section, ⌥⌘P) | PARTIAL | `nav.rs:186`, `context_menu.rs:359-363`, `cockpit.rs:7193` (3b33960c) | No ⌥⌘P chord, only the menu letter. |
| SH6 | Unread dot (Codex blue) plus a badge; clears on view; Mark as unread | BUILT | `nav.rs:1545` (d880a138); `context_menu.rs:370` (0e42e74b) | The dot is white. No badge anywhere. **Audit 11-04:** 6b6072a5: blue unread dot; no count badge |
| SH7 | A new chat appears at once with a spinner and is auto-titled live | BUILT | `cockpit.rs:11361,11401` (b320bd8e); `ferrite/src/lib.rs:380` |  |
| SH8 | Thread header: folder icon, title, ⋯, checklist toggle, new-window button | PARTIAL | `view_head.rs:243,437,196` | ▾ instead of ⋯ by design (components.md:15). No checklist toggle and no new-window button. |
| SH9 | Thread ⋯ menu with icons, chords and separators | PARTIAL | `context_menu.rs:339-421` | No icons. Letters, not ⌥⌘ chords. No New side chat, Share, Copy ›, Open in new window or scheduled task. |
| SH10 | Fork › (Fork chat / Fork chat in new worktree) | PARTIAL | `context_menu.rs:373-380` | A single Fork row. |
| SH11 | Copy › (deeplink, Markdown, working directory) | PARTIAL | `context_menu.rs:395-396` | Copy path and Copy link (a CLI command, since there is no URL scheme). No Copy as Markdown. |
| SH12 | Open in › Finder, Terminal, Xcode with app icons | PARTIAL | `context_menu.rs:436-440` (0e42e74b) | Finder and Terminal, no icons. |
| SH13 | Add scheduled task | MISSING | — |  |
| SH14 | Project header hover: "New chat in <project>" | BUILT | `cockpit.rs:14003` (58720cf7) |  |
| SH15 | Row right-click menu | PARTIAL | `context_menu.rs:339`, `cockpit.rs:13401` | KNOWN: right-click target mismatch. |
| SH16 | Archive (park) and delete | BUILT | `context_menu.rs:415,418` (573aa84f, 348adeed) | No ⇧⌘A. |
| SH17 | Rename a thread | BUILT | `cockpit.rs:3273` (c0e7dfd2) | No ⌥⌘R. |
| SH18 | Search threads | PARTIAL | `palette.rs:1-27` | Titles only, not message content. |
| SH19 | Open a thread in a new window | MISSING | — |  |
| SH20 | Long titles fade; project heading counts removed | BUILT | `nav.rs:582,619` (76e2c416, ed5541cf); fdc65449 |  |
| SN1 | System notification when a turn finishes or needs you | BUILT | `cockpit.rs:13113-13128`, `system_notifications.rs` (3c2de915) |  |
| SN2 | Back and forward between threads | BUILT | `keymap.rs:244-245`, `view_head.rs:99` (3b33960c) |  |
| SN3 | ⌘K command palette | PARTIAL | `cockpit/palette.rs` | KNOWN: Esc bug. Otherwise rich (threads, projects, about 40 commands). |
| SN4 | Keyboard shortcuts sheet | BUILT | `shortcuts.rs`, ⌘/ `keymap.rs:371` (2dfbffa7) |  |
| OP1 | Code Review page | MISSING | — | The Review-branch diff (⇧⌘D) is a different feature. |
| OP2 | Space (Pages, Slides, Sites, Images) | OUT OF SCOPE | ChatGPT side |  |
| SW1 | ChatGPT / Codex switcher | OUT OF SCOPE | ChatGPT side |  |
| RP1 | Title-bar tool: session rail | BUILT | `cockpit/panel.rs:223`, ⌥⌘B `keymap.rs:286` (d902ffff) | A single panel toggle. |
| RP2 | Title-bar tool: Terminal (real PTY in the project) | BUILT | `view_head.rs:23-25` "not drawn" | Only "Open in › Terminal" (Terminal.app). **Audit 11-04:** c3464a0c, b8a13756: real terminal in the dock |
| RP3 | Title-bar tool: Changes ⇧⌘D | BUILT | `keymap.rs:306`, `view_head.rs:211`, review.rs (3c2de915) | The Review slot beside the Pane, not a panel tab. |
| RP4 | Title-bar tool: Browser (+ launcher, URL bar, tools grid) | MISSING | `view_head.rs:23-25` | The page pane loads no URLs. |
| RP5 | Title-bar tool: Files | BUILT | `view_head.rs:215` (Reveal in Finder); `review.rs:2070` | Opens Finder. In the app, files can be browsed only through the Review's tree. **Audit 11-04:** 804ba262: Files in the dock |
| RP6 | Title-bar ⋯: Side chat ⌘;, iOS Simulator, Keep computer awake | MISSING | `view_head.rs:23-25`; LEDGER Lane A | KNOWN: the side chat (to be built). No ⋯, simulator or keep-awake. |
| RP7 | File viewer tab strip, preview tabs in italic | BUILT | `panel/preview.rs:1-36`, `panel.rs:23-26` (8ebbb97e) | Overview plus one preview tab that is replaced in place. **Audit 11-04:** 804ba262: preview tabs in italic |
| RP8 | File viewer: breadcrumb, line numbers, current line, soft wrap, loading skeleton | BUILT | `panel/preview.rs:742-822,628` (8ebbb97e) | Line numbers and the target line. The path is plain text. No wrap, no skeleton. **Audit 11-04:** 804ba262: breadcrumb, lines, wrap, current line |
| RP9 | File tree with a fuzzy filter that reveals the current file; changed / all sections | BUILT | `review.rs:2070` `tree_body` (3c2de915) | The Review's tree only. No filter field, no sections, no reveal. **Audit 11-04:** 804ba262: tree with filter and git colours |
| RP10 | First-run coach mark | MISSING | — |  |
| RP11 | Viewer ⋯ settings (wrap, indent guides, tab size) | MISSING | — |  |
| RP12 | Session rail: Environment (computer, folder, branch) | BUILT | `panel/sections.rs:83-115` (1c1e5980) | No Save. Remote Control is out of scope. |
| RP13 | Context breakdown (% used, stacked bar, category rows) | BUILT | `pane/usage.rs:347,487,667` (1238b8bf) | Claude only. Codex shows the reading and bar, with no categories (US2). |
| RP14 | Connectors "N connected" | BUILT | `panel/sections.rs:221-280` (cd3d351f) |  |
| RP15 | Changes "+N −N" with Create PR | BUILT | `panel/changes.rs:53-123,261-296` (1c1e5980, 3b33960c) | Counts the agent's own edits, not a git diff. |
| RP16 | Changes scope menu (last turn, uncommitted, unstaged, staged, committed, branch) | PARTIAL | `review.rs:2351-2380` (3c2de915) | Scope tokens rather than a dropdown. No unstaged/staged split. **Audit 11-04:** 49170fa6: scope dropdown (All changes, Uncommitted, Compare against a branch); no staged/unstaged split |
| RP17 | Changes toolbar (word diffs, whitespace, wrap, collapse all, jump to file) | PARTIAL | `review.rs:500-530`; `panel/changes.rs:88` | Keyboard only. No toggles, no jump-to-file. **Audit 11-04:** 49170fa6: ⋯ has Collapse/Expand all, Word wrap, Hide whitespace, Refresh; no word diffs or jump to file |
| RP18 | Changes file tree with filter, +N counts and stage buttons | PARTIAL | `panel/changes.rs:142` | Per-file blocks with +N −N. No filter, no staging. **Audit 11-04:** 49170fa6: files with hunks, reviewed marks; no filter or staging |
| RP19 | One continuous highlighted diff; a tree click scrolls to the file | PARTIAL | `review.rs:1406-1445,877-904` (3c2de915) | A tree click opens the whole file instead of scrolling to it. **Audit 11-04:** 49170fa6: one list of files with hunks |
| RP20 | Changes empty state with "View branch diff" | BUILT | `review.rs:2340-2345` | "No uncommitted changes" text only. **Audit 11-04:** Changes empty state links to View branch diff (BUILT, not ticked) |
| RP21 | Files: inline "Show changes"; "Open ⌄" split | MISSING | — |  |
| RP22 | Right panel as browser-like tabs (+, ×) | PARTIAL | `panel.rs:1-6,23-26` | One scroll of sections, plus a preview tab. |
| RP23 | Maximise panel with a docked compact composer | PARTIAL | `panel.rs:62-73` (fa36929a) | Expand widens a preview only. |
| SA1 | Subagents: "Ran 2 agents ▸" (Claude) / avatar line (Codex) | BUILT | `pane/agents.rs:1-80`, `pane/text.rs:76`, `transcript_glue.rs:55-78` (3b33960c) | Claude's form. The Codex avatar line is not drawn. |
| SA2 | An agent opens in the right panel (←, title, maximise, ×) | PARTIAL | `transcript_glue.rs:203`, `subagents.rs:467-520` (3b33960c, 64baa4c6) | It drills in in place of the Pane's transcript. No maximise or ×. **Audit 11-04:** Lane G1 (subagent drill-in) |
| SA3 | Agent view: prompt, model badge, own rows, result | PARTIAL | `subagents.rs:467-520`; `core activity.rs:70` | No model badge (`AgentInfo` has no model). **Audit 11-04:** Lane G1 (subagent drill-in) |
| SA4 | Subagents list (Active · N / Done · N) | MISSING | — | **Audit 11-04:** Lane G1 (subagent drill-in) |
| SA5 | Stop a subagent | BUILT | `cockpit.rs:5583,5600,5828` (3c2de915, 0e42e74b) | Keyboard only (Esc or ⌫). No visible button. |
| SA6 | Parent turn summarises the agents | BUILT | — (the model's own prose) |  |
| IM1 | "Viewed N images" row with thumbnail tiles | MISSING | — | Tool results carry no images. |
| IM2 | Images inline in the answer as rounded cards | BUILT | `rich.rs:865-871` (bc5a2473) | 1 px edge; no rounding set. |
| IM4 | Lightbox: dim, scale from origin, title, zoom menu, download, ← →, "1 of N", Esc | PARTIAL | `cockpit/lightbox.rs:1-120,248` (bcef8604, cda2c1c5) | A fit/actual toggle instead of a zoom menu. No scale-from-origin. File name instead of alt-text title. |
| IM5 | Tool rows with image thumbnails and a hover carousel | MISSING | — |  |
| SK1 | Customize › Skills page (Yours/Discover, search, sort) | MISSING | — |  |
| SK2 | Discover grids and install | MISSING | — |  |
| SK3 | Your skills grouped by source, with ⋯ | MISSING | — |  |
| SK4 | Skill detail (rendered SKILL.md, enable, uninstall, Try now) | MISSING | — |  |
| SK5 | Codex `$` skill picker | DIFFERENT BY DESIGN | components.md:31; LEDGER Lane C | Skills fold into `/` with a source tag. KNOWN: slash picker. |
| SK6 | Skill templates carousel | MISSING | — |  |
| PG1 | Page preview cards in the transcript | BUILT | `transcript/outputs.rs:8-20,167,261`, `rows.rs:476` (cdc328f3) | A globe glyph, not a snapshot. |
| PG2 | Page pane beside the transcript (rendered `.html`) | PARTIAL | `panel/preview.rs:23-30`, `ferrite-pane/src/preview.rs` (fa36929a) | Behind the non-default `cef` feature: a plain `cargo run` shows only the can't-show note. `install-app.sh` turns it on. Loads no URLs. |
| MO1 | Menus snap open (Claude) vs Codex model popover blur-in | DIFFERENT BY DESIGN | `theme.rs:3018` `MOTION_MENU_IN_MS = 0` (21d4810e); motion-measured.md | gpui has no blur filter. |
| MO3 | Shimmer and animated working glyph | BUILT | `pane/working.rs:363,127`, `components.rs:554` (401c7d8d, e60c7010) | KNOWN: motion not playing live. |
| MO5 | Thread switch snaps, skeleton chips until loaded | BUILT | (1c1e5980) | The snap is built. No skeletons (same as XQ7). **Audit 11-04:** 613e6bf9 |
| MO6 | Lightbox image scales up from its spot | MISSING | `cockpit/lightbox.rs:235` | Only the backdrop fades. |
| MO7 | Composer height eases | BUILT | `composer.rs:570-581,1338` (401c7d8d) | KNOWN: motion. Claude snaps here; easing is a recorded choice. |
| MO8 | Toast in/out motion | BUILT | `cockpit/palette.rs:1554-1561` (66727e5d); `theme.rs:1312` (401c7d8d) | KNOWN: motion. |
| MO9 | Rolling token count | BUILT | `pane/working.rs:389` (bfbeb3e2) | KNOWN: motion. |
| MO10 | Chevron and fold motion | BUILT | `pane.rs:4388` (94281267, 2735a69f); `cockpit.rs:7170-7172` | KNOWN: motion. Transcript folds snap by design. |
| MO11 | Reduced motion respected | BUILT | `motion.rs:2086-2106`, `ferrite/src/lib.rs:218` (492e923f) | Read once at launch. |
| US1 | Usage ring fed by real plan limits (both providers) | PARTIAL | Claude `claude/wire.rs:759`; Codex `codex/wire.rs:225,492`; `engine.rs:3299`; `pane/usage.rs:18` | Claude is complete. Codex is never asked up front (empty until its first update) and sends no plan name. |
| US2 | Context usage from real data (both providers) | PARTIAL | Claude `claude.rs:523,1050-1068`; Codex `codex/wire.rs:896`; `panel/sections.rs:41-60` | Codex has no category breakdown. |
| US3 | Live token count from real data | BUILT | `transcript.rs:599-635,1322`, `pane/working.rs:286` (bfbeb3e2) | Codex has no thinking estimate. |
| QB1 | Overlay scrollbar thumb ~6 px | BUILT | `theme.rs:1791-1800` (436b19f4) | 4 px at rest, 6 px under the pointer. The transcript itself has none (TR18). |
| QB2 | 1 px hairlines between regions | BUILT | `cockpit.rs:5361-5380,10769` (d880a138) |  |
| QB3 | Surfaces palette | BUILT | `theme.rs:239-266` (c35a2791, d880a138) | The owner's confirmation is pending (LEDGER). |
| QB4 | User bubble padding ~16/12, radius ~16 | DIFFERENT BY DESIGN | `theme.rs:1533,898`; geometry.md:28 | Padding 16/10, radius 10. |
| QB5 | Scroll-to-bottom ~30 pt, centred above the composer | BUILT | `theme.rs:1681-1687`, `transcript.rs:1736` (241f18e8) |  |
| QB6 | Thin text (no glyph dilation) | BUILT | e64ce45f; `ferrite/src/lib.rs:68` |  |
| QB7 | 1 px icon strokes | BUILT | `icons.rs:296-370`, `ferrite/src/lib.rs:213` (401c7d8d) | Kit-internal icons keep their baked stroke. |
| QB8 | One tooltip style everywhere | BUILT | `menu.rs:427,521,1395` (401c7d8d) |  |
| OS1 | Mic and voice input; waveform send button | OUT OF SCOPE | Owner: voice and mic |  |
| OS2 | Read aloud on answers | OUT OF SCOPE | Owner: voice |  |
| OS3 | Plan mode (+ menu, permission) | OUT OF SCOPE | Owner: plan mode |  |
| OS4 | Remote Control toggle in the session rail | OUT OF SCOPE | Owner: Remote Control |  |
| OS5 | Settings (incl. Fast mode "Buy credits", provider options) | OUT OF SCOPE | Owner: settings |  |

## claude-desktop-transcript-handoff.md

Audited 2026-11-04 against `crates/` and the git history on `transcript-app-style` (HEAD `ed5541cf` plus uncommitted work). The source is `docs/research/claude-desktop-transcript-handoff.md`, sections 4–24, including the §19 integration checks and the §22 fixtures (shown as "fx N"). `docs/research/transcript-architecture-gaps.md` (2026-11-03) audited the same doc a day earlier. Its step 0–2 fixes landed in `dc19326b` and `241f18e8`, and every row below was re-checked against today's code rather than taken from it.

Scope follows the owner's list (settings, voice and mic, plan mode, Remote Control, the ChatGPT side, an in-app terminal). Two more parts are OUT OF SCOPE because the handoff says so itself: "Behavior found in one path must not automatically become a requirement for the others" (§2.4–2.6, §17, §18). That covers the Cowork and shared-renderer behaviour and the §24 diagnostics. Where a requirement is already a row above, it is listed under **Already in this file** and not counted again.

### Summary

| Status | Count |
|---|---|
| BUILT | 61 |
| PARTIAL | 11 |
| MISSING | 10 |
| DIFFERENT BY DESIGN | 0 |
| OUT OF SCOPE | 3 |
| **Total** | **85** |

**Reachability:** every BUILT row runs in the real window on real provider frames. None is reachable only from tests or scenes. The one scene-only hit found is `Drafting` in `pane.rs:5699` and `visual_reference/feature_scenes.rs`: test and scene prose, not a provisional-tool-row state (that is HD39, PARTIAL). Two rows rest on tests for the requirement, but the behaviour itself is live: HD6 (`tests/provider_progress.rs:522`) and HD82 (`rows.rs:991`).

**Bottom line:** the execution foundation the handoff asks for first (§21's implementation sequence, step 1) is done: identities, dedup, supersession, generation guards, the policy table, task patches, run guards, stream loss and orphan results. What is left falls into three groups. Some outputs arrive late or in another place: background output, VCS events and result attribution. Some are Desktop's own app tools: the turn plan, wrap-up and side sessions. The rest is small wire detail.

### Already in this file (not counted again)

| Handoff requirement | Row |
|---|---|
| §10, §14 interrupted tools say "stopped"; Deny ends the turn | ER1, ER2, TR5, TR7 (the core `ToolState` has no Stopped) |
| §11 the checklist stays with its turn after it ends | TR12, XTR2 |
| §10 user-message delivery states (sending, queued, steered) | QS1, QS3, QS4, XSQ1, XSQ2 |
| §14 per-task stop control | SA5 |
| §14 nested agent transcript, its navigation and list | SA1, SA2, SA3, SA4 |
| §14 background shell output while it runs | TR10 (core keeps `output_tails`; nothing draws them) |
| §15 question and permission forms | AP1, AP4, AP11 |
| §17 file output cards; open only what exists | XTR9, PG1 |
| §20 long history opens at the end, lazy layout | LT2 |
| §20 Create PR from the changes | RP15 |
| §6 prompt suggestions | XD9 |
| §10 starting or setting-up state | XD6 |
| §19.47 thinking estimate vs billed usage | US3 |
| §14 app side sessions (the side chat) | RP6, TXS (KNOWN) |

### MISSING and PARTIAL, ranked by how much a daily user would notice

| # | ID | Requirement (§) | Status | What's missing | Evidence |
|---|---|---|---|---|---|
| 1 | HD64 | Background output joins its originating call (§14, fx 22) | BUILT | `task_notification`'s `output_file` and resource links are never read. A backgrounded Bash call or agent settles at launch, and its real output never reaches that row. **Audit 11-04:** 70cd0668/63a5de61: background output joins its row | grep `output_file`, `resource_link`: no hits in `crates/` |
| 2 | HD39 | A provisional tool row while tool input streams (§7.5, fx 2) | BUILT | Only `show_visual` drafts. Any other tool has no row until its complete `assistant` message, so a long Edit or Write is invisible while its input streams. **Audit 11-04:** 63a5de61: Edit and Write stream | `claude/activity.rs:955` (visual drafts), `:965-991` (no tool row on `content_block_start`) |
| 3 | HD5 | A turn's result names the prompts it answered (`user_message_uuid(s)`) (§4) | BUILT | Never read. A prompt that was batched or folded mid-turn has no confirmed turn. This blocks turning on Claude steering (QS4). **Audit 11-04:** 63a5de61: user_message_uuids | Only `rewind.rs:183` reads a uuid (`last_seen_user_message_uuid`) |
| 4 | HD58 | Wrap-up turn box: recap, ask, blocked, at most two validated next actions (commit, create_pr, merge, fix_ci, …) (§13, fx 36–39) | BUILT | No `ccd_turn` equivalent. The answer's last prose stands in. The 11-03 gaps audit chose not to build it, but no owner decision is recorded. **Audit 11-04:** 5427026f: Claude's captured box; reply-style steps | `transcript-architecture-gaps.md:137,368` |
| 5 | HD27 | Workspace and PR side effects from `code_change_published` / `vcs_state_changed` (§20) | BUILT | Not decoded. Changes counts the agent's own edits, so background work or a pushed PR does not refresh it (see RP15's note). **Audit 11-04:** 63a5de61: PR re-read after commit, push, PR, handback | grep: no hits |
| 6 | HD79 | A truncating resume or rewind is validated (`--resume-drops-turn`) (§19, fx 50) | BUILT | A rewind cuts at `--resume-session-at` without validation, so a message the turn absorbed mid-way can be dropped silently. This becomes more likely once Claude steering is on. **Audit 11-04:** 63a5de61: --resume-drops-turn | `claude.rs:306`; grep `resume-drops-turn`: no hits |
| 7 | HD12 | `tool_progress` heartbeat and `subagent_retry` (§5) | PARTIAL | Elapsed time only. A subagent's API retries and attempt counts are invisible. | `claude/wire.rs:382-388` |
| 8 | HD83 | Scroll position kept per session (§20) | BUILT | A `TranscriptView` starts at the tail. No saved offset keyed by thread was found. Not verified whether a Solo thread switch keeps the old view alive. **Audit 11-04:** 5e0d3d3e: pinned by a test | `ferrite-pane/src/transcript.rs:424`, `transcript/scroll.rs:29-31` |
| 9 | HD71 | Human review comments reach the agent as typed data (§16, fx 41) | PARTIAL | Diff comments (file, side, line range, snippet) ride the next prompt. There are no threads or replies, no record of which turn got them, no rendered-element anchors, and no data-not-instructions boundary. A selection Reply quotes text (TXS). **Audit 11-04:** Lane G1 owns it | `ferrite-pane/src/composer.rs:460` → `ferrite-reviewr/src/ferrite.rs:530` `attach`, `export.rs:21` |
| 10 | HD22 | `permission_denied` kept by call id, even before its row exists (§5, fx 7) | PARTIAL | The event is not decoded. The orphan-result buffer covers the usual result-before-row case. | `transcript.rs:821,2012-2021,2409` `settle_orphan` |
| 11 | HD68 | `AskUserQuestion` option previews (§15) | BUILT | Parsed into `Question` options but drawn nowhere. Multi-select, free text and answers are BUILT (HD67). **Audit 11-04:** 5e0d3d3e: option previews | `ferrite-core/src/questions.rs:46,134`; no `preview` use in the decision UI |
| 12 | HD57 | Custom turn plan (`update_plan`: scope, ≤12 items, advance/done/start, linked sessions) (§12, fx 33–35) | DONE | `mcp__ferrite__update_plan` on Ferrite's server with §12's contract and guidance, moved only by Main's confirmed receipts, settled by an unblocked wrap-up; Codex's native `update_plan` enabled with the same guidance. No linked sessions (Ferrite has none). **Audit 11-04:** 68ed21a6 | `update_plan.rs`, `contract_plans.rs` (real 2.1.318 and 0.166.0 captures) |
| 13 | HD65 | App side sessions: proposal tools, linked plan rows, parent wakeups (§14) | BUILT | No such mechanism. The side chat is a fork (TXS, RP6, KNOWN). **Audit 11-04:** db13e437: the side chat is a right-docked tabbed pane | — |
| 14 | HD70 | `ReportFindings` card grouped by file; a failed call stays a failure (§16, fx 40) | PARTIAL | A generic row; failure shows as `Failed`. No findings card. | `transcript/tools.rs:64` (falls to `Other`); `pane/text.rs:47,82` |
| 15 | HD8 | Task hierarchy: `run_id` guard, `parent_task_id` tree, a child before its parent stays visible (§14, fx 19–20) | PARTIAL | `run_id` is BUILT. `parent_task_id` is never read. | `claude/activity.rs:53,270,1346` |
| 16 | HD60 | A stopped or failed task keeps its reason (`worker_restart` is not "you stopped it") (§14) | MISSING | `reason` is not read. `killed` and `stopped` both map to Interrupted. | `claude/activity.rs:1503`; grep `worker_restart`: none |
| 17 | HD14 | Capability record from `system/init` (tools list, deferred tools) (§21, fx 48) | PARTIAL | Initialize answers are kept (modes, models, commands) and `msg_lifecycle_v1` is detected, but `init`'s tool list is not recorded. | `claude.rs:202-217`; `claude/queue.rs:15-19` |
| 18 | HD75 | The session records its CLI version and backend (§19.1) | PARTIAL | The version is checked against the pin at spawn, then thrown away. | `claude.rs:1217-1239` |
| 19 | HD18 | Hook lifecycle events (§5, §19) | MISSING | Ignored, which is safe. No hook spinner. | grep `hook_started`: none |
| 20 | HD55 | Failed `TaskCreate` behaviour pinned by a test (fx 31) | PARTIAL | Receipts only, so no row appears, which is the chosen behaviour. No test. | `claude/wire.rs:520`; no failing-create case in `tests/contract_plans.rs` |
| 21 | HD44 | A fallback dedup key for frames with no uuid (§8) | MISSING | `id: None` is never deduped. Claude frames carry a uuid, so this is low impact. | `activity.rs:2569` `delivery_id` |

### Full table

ID prefix: HD, a handoff requirement. "fx N" is the matching §22 fixture.

| ID | Requirement (§) | Status | Evidence (file:line, commit) | Note |
|---|---|---|---|---|
| HD1 | App session id and runtime session id kept apart; resume by runtime id (§4, §19.6) | BUILT | `ThreadId` vs `Transcript.session_id`; `claude.rs:300-310` `--resume` | |
| HD2 | Replay dedup by event identity, never by text (§4, §8, fx 10, 15) | BUILT | `activity.rs:442` `seen.insert`, `:2569` `delivery_id` | |
| HD3 | A streamed message and its completed form are one visible copy, joined by API `message.id` (§7.4, fx 1) | BUILT | `claude/activity.rs:747,1152` `bind_main_stream_block`; `activity.rs:1830` `TextSnapshot` | Two text blocks in one message fall back to the delivery id (gaps audit, untested) |
| HD4 | Every outgoing prompt carries its own UUID (§4) | BUILT | `engine.rs:5480` `send_as`; `claude.rs:628-644` `enqueue` | |
| HD5 | Result attribution (`user_message_uuid(s)`) (§4) | BUILT | `claude/queue.rs` (`user_message_uuids` on `status` and `result`) | Next build 11 **Audit 11-04:** 63a5de61: user_message_uuids |
| HD6 | Tool call and result joined by id, including out-of-order parallel results (§4, fx 4) | BUILT | `transcript.rs` `settle_tool` by call; `tests/provider_progress.rs:522` | |
| HD7 | `parent_tool_use_id` / `agent_id` attribution; child prose and todos never reach Main (§4, §19.14, fx 23) | BUILT | `claude/activity.rs` scope; `activity.rs` `Subject::Subagent` | |
| HD8 | Task hierarchy and run identity (§14, fx 19–20) | PARTIAL | `claude/activity.rs:53,270,1346` | Ranked #15 |
| HD9 | Todo, runtime task and app session kept apart (§4) | BUILT | `progress.rs` `plan` vs `tasks`; `Subject::Subagent` | |
| HD10 | An outer `user` frame is read by content: tool results and replays are never human bubbles (§5, §19.13, fx 5) | BUILT | `claude/activity.rs` main user frames skipped; results become `ToolCompleted` | |
| HD11 | `result` is not process exit; a background turn ending is distinct (§5) | BUILT | `claude/activity.rs:332` | |
| HD12 | `tool_progress` with heartbeat and retry (§5) | PARTIAL | `claude/wire.rs:382-388` | Ranked #7 |
| HD13 | `tool_use_summary` attached by `preceding_tool_use_ids`; never hides a failure (§5, §9.3, fx 25) | BUILT | `claude/wire.rs:293`; `transcript.rs:2240-2256` `caption`; `pane/text.rs` `group_summary` (dc19326b, 241f18e8) | |
| HD14 | Capability record from init (§21, fx 48) | PARTIAL | `claude.rs:202-217`; `claude/queue.rs:15-19` | Ranked #17 |
| HD15 | `system/status`, including compacting and its null end (§5) | BUILT | `claude/wire.rs:425-441` | |
| HD16 | `api_retry` shown as retrying, not failed (§5) | BUILT | `claude/wire.rs:412-424` | |
| HD17 | `compact_boundary` is not a new conversation (§5) | BUILT | `claude/wire.rs:442` | |
| HD18 | Hook lifecycle events (§5) | MISSING | — | Ranked #19 |
| HD19 | `informational` and local command output become notices (§5) | BUILT | `claude/activity.rs:237,252` | |
| HD20 | `task_*` lifecycle; `task_updated` merged as a patch (§14, §19.16, fx 18) | BUILT | `claude/wire.rs:449-470`; `progress.rs:234-243`; agents `claude/activity.rs:1412-1415` (dc19326b) | |
| HD21 | `background_tasks_changed` replaces the set, empty included (§14, fx 17) | BUILT | `claude/wire.rs:486`; `progress.rs` snapshot | |
| HD22 | `permission_denied` kept by call id (§5, fx 7) | PARTIAL | `transcript.rs:821,2012-2021,2409` | Ranked #10 |
| HD23 | `thinking_tokens` as an estimate, separate from billed usage (§5, §19) | BUILT | `claude/wire.rs:443` | See US3 |
| HD24 | `commands_changed` updates the command menu live (§5) | BUILT | `claude/wire.rs:1430` | |
| HD25 | `session_state_changed` (§5) | BUILT | `claude.rs:264` env flag; `claude/wire.rs` state → `RunState` | |
| HD26 | Unknown events and fields are forward-compatible, never raw text in chat (§5, §19.21, fx 8) | BUILT | `claude/wire.rs:2336` `junk_lines_are_ignored_never_fatal` | |
| HD27 | VCS and code-publication events (§20) | BUILT | `Transcript::published` with `CockpitView::refresh_published` | PR re-read after a commit, push, PR or handback; the CLI events themselves are not decoded **Audit 11-04:** 63a5de61: PR re-read after commit, push, PR, handback |
| HD28 | Partial messages enabled (§6, §19.10) | BUILT | `claude.rs:277` `--include-partial-messages` | |
| HD29 | Permission broker routed to the app (§6, §15) | BUILT | `claude.rs:284-290` `--permission-prompt-tool stdio` | |
| HD30 | `perTaskStopAffordance` declared, backed by a real stop (§6, §14, §19.28, fx 15) | BUILT | `claude.rs:429`; `stop_subagent` `claude.rs:702` | See SA5 |
| HD31 | No dialog kinds declared that the UI can't answer; unknown control requests refused by id (§15, §19.27, fx 27) | BUILT | `claude/wire.rs:956`; test `:2371` `an_unknown_control_request_is_refused_by_its_id` | |
| HD32 | The SDK environment is merged with the inherited one, not replacing it (§11, §19.4) | BUILT | `claude.rs:262-271` (`envs` adds to the inherited env) | |
| HD33 | `claude_code` preset and default settings sources; Ferrite's own tool registered only when offered (§6, §19.2–3, §17) | BUILT | `claude.rs:272-337` (no prompt override, no `--setting-sources`; `--allowedTools` for `show_visual` only when offered); the session-context preface `engine.rs:5455-5470` | |
| HD34 | Additional directories (§6) | BUILT | `claude.rs:329-331` `--add-dir` | |
| HD35 | stderr is kept for diagnostics, never mixed into model text (§6) | BUILT | `claude.rs:1119` `drain_stderr`, `:1157` `exit_reason` | |
| HD36 | Requested vs effective model and effort (§6) | BUILT | `claude.rs:464-473` `SettingAck` | |
| HD37 | Delta batching keeps order and flushes on structure (§7.2, fx 44) | BUILT | `activity.rs` record merge; the store and link merges (gaps audit §1) | Structural only: no 16 ms / 250 ms visible-vs-background cadence, and none needed |
| HD38 | Tool input is not execution: `content_block_stop` is not success (§7.5, §19.11, fx 3) | BUILT | `claude/activity.rs:991` (`content_block_stop` does nothing); the row stays Running until `ToolCompleted` | |
| HD39 | Provisional tool row while input streams (§7.5, fx 2) | BUILT | `claude/drafts.rs`, `Transcript::draft_edit` | Edit, MultiEdit, Write **Audit 11-04:** 63a5de61: Edit and Write stream |
| HD40 | Main history comes from Ferrite's own log; the runtime is resumed for context, never rebuilt from bubbles (§8) | BUILT | `store.rs:4065` replay; `claude.rs:300-304` | |
| HD41 | Child history merges a frozen prefix with the live tail once (§8, fx 10) | BUILT | `engine/history.rs:60,279,314` `HistoryContent` | |
| HD42 | A superseded process can't change its replacement's state (§8, §19.8, fx 9) | BUILT | `activity.rs:285,1078` `Observe { generation }`; `claude/activity.rs:162-190` (a new init resets) | |
| HD43 | Window reconnect resynchronises from the server (§8) | BUILT | `protocol.rs:757-772` `WireInput` replay/live; `cockpit/mirror.rs` | |
| HD44 | Fallback dedup key (§8) | MISSING | — | Ranked #21 |
| HD45 | One explicit tool-presentation policy table (§9.1, §21) | BUILT | `transcript/tools.rs:33-44,64` `policy` (dc19326b) | |
| HD46 | `AskUserQuestion` and `ExitPlanMode` stand alone (§9.1) | BUILT | `transcript/tools.rs:70-71` | |
| HD47 | Todo tools are state-only, drawn when they fail (§9.1, fx 6) | BUILT | `transcript/tools.rs:84`; `transcript.rs:208` | |
| HD48 | Code mode never folds narration (§9.4, §18, fx 45) | BUILT | `ferrite-pane/src/transcript/rows.rs` (narration muted, never hidden) | |
| HD49 | Waiting for the user is an explicit state with its request (§10, §21) | BUILT | `transcript.rs` `Status::Blocked`; `activity.rs:248` `DecisionHandle` | AP9 covers the clock |
| HD50 | Retrying and compacting visible on the live line (§10, fx 47) | BUILT | `pane/working.rs` caption; `claude/wire.rs:412-441` | |
| HD51 | Main settles while background work stays visible (§10, §14, fx 14) | BUILT | `progress.rs` `end_turn` keeps tasks; `ferrite-cockpit/src/background_chips.rs` | |
| HD52 | A stream that ends without a result fails its turn, with no fabricated success (§10, §19.9, fx 12) | BUILT | `transcript.rs:146,2276-2297` (241f18e8) | The failure strip shows the error |
| HD53 | A disconnected session can be reconnected (§10) | BUILT | `transcript.rs:1936` `Revived`; `engine.rs:995` | |
| HD54 | Native todo projection: receipts only, child excluded, `TodoWrite` replaces, `TaskCreate` keyed by returned id, delete removes (§11, fx 28–30, 32) | BUILT | `claude/wire.rs:520-594`; `progress.rs` | TR12/XTR2: the card leaves with the turn |
| HD55 | Failed `TaskCreate` pinned by a test (fx 31) | PARTIAL | `claude/wire.rs:520` | Ranked #20 |
| HD56 | Todo tools opted in for newer models (§11, §19.24) | BUILT | `claude.rs:265` `CLAUDE_CODE_ENABLE_TODO_TOOLS` | |
| HD57 | Custom turn-plan tool (§12) | DONE | — | Ranked #12 **Audit 11-04:** 68ed21a6 |
| HD58 | Wrap-up and next-action turn box (§13) | BUILT | `wrap_up.rs`, `pane/wrap_up.rs` | Claude only; reply-style steps, no app action kinds **Audit 11-04:** 5427026f: Claude's captured box; reply-style steps |
| HD59 | Task status unions kept apart (`killed`, `paused`, `stopped`) (§14, §19.17) | BUILT | `claude/activity.rs:1503-1504`; `claude/wire.rs:468` | |
| HD60 | A task's stop or failure reason is kept (§14) | MISSING | — | Ranked #16 |
| HD61 | `skip_transcript` and `ambient` apply apart from execution state; no endless spinner (§14, §19.18, fx 21) | BUILT | `claude/wire.rs:321,351` | |
| HD62 | `forwardSubagentText` on, with no false promise of nested token streaming (§14, §19.29, fx 24) | BUILT | `claude.rs:282`; `claude/activity.rs:722,789` `TranscriptCoverage::Live` | |
| HD63 | Agent progress summaries enabled on purpose and shown on the agent row (§14, §19.30) | BUILT | `claude.rs:425-430`; `claude/activity.rs:1394-1398`; `pane/agents.rs:24-63` (241f18e8) | |
| HD64 | Background output joins its call (§14, fx 22) | BUILT | `claude/activity.rs` (`handback`), `ToolBlock::background`, `panel/background.rs` | Claude; Codex terminals still settle at turn end **Audit 11-04:** 70cd0668/63a5de61: background output joins its row |
| HD65 | App side sessions (§14) | BUILT | — | Ranked #13 **Audit 11-04:** db13e437: the side chat is a right-docked tabbed pane |
| HD66 | A Decision resolves once; stale clicks and closed runtimes are rejected (§15, §19.26) | BUILT | `activity.rs:248` `DecisionHandle`; `claude.rs:733-754` | |
| HD67 | `AskUserQuestion`: 1–4 questions, multi-select, free text; answers sent as updated input; routed once (§15, fx 26) | BUILT | `ferrite-core/src/questions.rs:51-180`; `claude/wire.rs` question response | Forms UI: AP11 |
| HD68 | Question option previews (§15) | BUILT | `questions.rs:46,134` | Ranked #11 **Audit 11-04:** 5e0d3d3e: option previews |
| HD69 | Unsupported elicitation or dialog answered with decline/cancel, never a fabricated yes (§15) | BUILT | `claude/wire.rs:1053,1150-1160` | |
| HD70 | `ReportFindings` card (§16) | PARTIAL | `transcript/tools.rs:64` | Ranked #14 |
| HD71 | Human comments as typed data (§16) | PARTIAL | `composer.rs:460`; `ferrite-reviewr/src/ferrite.rs:530` | Ranked #9 **Audit 11-04:** Lane G1 owns it |
| HD72 | Cowork tools: `present_files`, `import_files`, artifacts, writing drafts, widgets, `send_user_message` (§17) | OUT OF SCOPE | Handoff §2.2–2.4, §17 ("Many belong to Cowork … rather than the normal Code runtime") | Ferrite's file cards are XTR9/PG1 |
| HD73 | A generic renderer for unknown or custom tools, with a visible error even for specialised ones (§17, §19.34, fx 6, 8) | BUILT | `pane/text.rs:47,82` `ToolKind::Other`; `transcript.rs:208,532` `Failed` | MCP tools fall back to pretty-printed JSON (AP11) |
| HD74 | Shared-renderer folding: classifier, latest-two rows, display groups (§18) | OUT OF SCOPE | Handoff §2.5–2.6 ("not established as the rule for the current Code transcript") | Code's no-fold rule is HD48 |
| HD75 | CLI version recorded per session (§19.1) | PARTIAL | `claude.rs:1217-1239` | Ranked #18 |
| HD76 | Open input stream for a multi-turn session (§19.5) | BUILT | `claude.rs:273` `--input-format stream-json` | |
| HD77 | Session persistence kept on for resume (§19.7) | BUILT | No `--no-session-persistence` in `spawn`; only the title one-shot uses it (`claude.rs:1301`) | |
| HD78 | Precise resume or fork point for rewind and fork (§19) | BUILT | `claude.rs:306-310` `--resume-session-at`, `--fork-session`; `claude/rewind.rs` | |
| HD79 | Truncating resume validated (§19, fx 50) | BUILT | `ClaudeConfig::resume_drops` (`--resume-drops-turn`) | — **Audit 11-04:** 63a5de61: --resume-drops-turn |
| HD80 | File checkpointing coordinated with conversation rewind; a fork doesn't claim a new worktree (§20, fx 49) | BUILT | `claude.rs:268`; `claude/rewind.rs`; `ControlKind::RewindFiles` (401c7d8d) | |
| HD81 | Follow new output; scrolling up stops it (§20) | BUILT | `transcript/scroll.rs:29-31,62` `FollowMode::Tail` | |
| HD82 | Older content prepends with rows and expansion stable (§20, fx 11, 46) | BUILT | `rows.rs:893,991` splices; expansion by `DisclosureId` | |
| HD83 | Scroll position per session (§20) | BUILT | `transcript.rs:424`, `scroll.rs:29` | Ranked #8 **Audit 11-04:** 5e0d3d3e: pinned by a test |
| HD84 | A live-text store separate from history rows; tokens don't rebuild old rows (§20) | BUILT | `rich.rs:1` (a stable parser per answer run); `rows.rs:893` Rc reuse | |
| HD85 | §24 diagnostics and telemetry | OUT OF SCOPE | Handoff §24 ("do not add transcript features … no need to copy Desktop's telemetry") | The stderr tail on exit is HD35 |
