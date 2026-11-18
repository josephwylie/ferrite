# Installed T3 Files, Terminal and provider refresh source match

The live checks establish useful completed workflows: real file navigation, source/rendered Markdown switching, source-editor Find, project search, edit persistence, terminal execution, interruption and pane cleanup. Two qualifications matter: a 500 ms autosave path prevents attributing disk writes to Command-S alone, and rendered Markdown content Find was not certified. The provider banner later disappeared after Refresh, but its timeout cause remains unknown.

Source identity: **1264c206aee0dce805d6279809118bc091f29d2f**, installed version **0.1.10-nightly.20261111.4544**. See [installed provenance](t3-installed-source-match.md). Every implementation link below targets that immutable commit. The newer checkout at 74573fc is not used to explain the installed application.

Runtime evidence belongs to the parent's [live observation log](../evidence/t3-live/observations.md#files-project-search-editor-and-terminal), including the later [provider recovery and conversation Find observations](../evidence/t3-live/observations.md#conversation-find-and-turn-navigation). This worker performed only source inspection. It did not operate T3, launch models or applications, run tests/builds, or start persistent processes. No Ferrite application code changed.

The acceptance criteria below are concrete checks to apply to Ferrite. They are not additional Ferrite defect findings. A fresh native state or capture must follow each action; an acknowledgement or stale accessibility snapshot is not proof of success or failure.

throughput checkpoint: n/a, read-only investigation.

## FILES-01: Open a real project file in a retained panel tab

**Observed:** Files listed the disposable project. README.md opened as source. The rendered long-guide.md link opened that actual file in another tab. Observation log: line 109, line 110.

**State transition:** Files surface → selected file path → file:<relativePath> surface → active file tab, panel open.

**Installed implementation**

- FileBrowserPanel selection takes the last selected non-folder entry and calls onOpenFile with its panel path. [FileBrowserPanel.tsx · useFileTree onSelectionChange, L274–284 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FileBrowserPanel.tsx#L274-L284)
- RightPanelSurfaceContent passes the workspace root, selected path, line, request id and opening callback into FilePreviewPanel. [RightPanelSurfaceContent.tsx · case files/file, L203–234 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/RightPanelSurfaceContent.tsx#L203-L234)
- openFile removes the standalone Files surface, reuses a matching file tab or appends one, increments its reveal request and activates it. [rightPanelStore.ts · openFile, L812–845 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/rightPanelStore.ts#L812-L845)
- Rendered Markdown file links call the same openFile path with optional line metadata; asynchronous basename lookup rejects superseded results. [ChatMarkdown.tsx · openFileInPanel, L800–818 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatMarkdown.tsx#L800-L818)

**Evidence limits**

- This verifies the inspected fixture and navigation path, not every filename, inaccessible host path, rename or deletion case.

**Ferrite acceptance**

- Open Files, click README.md, then activate long-guide.md from its rendered link. Confirm the actual file contents and active tab name agree.
- Reopen the same path and confirm it activates its existing tab; another path opens its own tab. The conversation and draft remain present.
- A pointer click and keyboard activation must reach the same file; an underlined link that only changes appearance fails the journey.

## FILES-02: Switch Markdown between source and rendered content

**Observed:** Show rendered markdown displayed real headings, code, quotations and links after README.md source had been visible. Observation log: line 109, line 110.

**State transition:** Markdown source → persisted rendered preference true → RenderedMarkdownSurface → ChatMarkdown; pending line navigation keeps the file in source until handled.

**Installed implementation**

- The Markdown preference defaults to false. Rendering additionally requires a handled reveal request, so a newly requested line can override the rendered preference. [FilePreviewPanel.tsx · renderMarkdownPreferred / revealHandled / renderMarkdown, L1098–1133 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1098-L1133)
- The toolbar flips the appropriate rendered preference and records whether the current path/reveal request has been handled. [FilePreviewPanel.tsx · FileSurfaceAction rendered toggle, L1357–1378 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1357-L1378)
- The render branch mounts RenderedMarkdownSurface or the source/editor branch using the same file contents. [FilePreviewPanel.tsx · file content branches, L1566–1615 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L1566-L1615)
- FileMarkdownPreview passes frontmatter-masked text to ChatMarkdown, with the file directory as image base and a max-w-4xl, px-6, py-5 content layout. [FileMarkdownPreview.tsx · FileMarkdownPreview, L7–36 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FileMarkdownPreview.tsx#L7-L36)

**Evidence limits**

- The live capture proves visible content and the toggle, not exact typography dimensions or rendering of every Markdown extension.
- A search-result line opening source while rendered preference is on is supported behavior, not a failed preference restore.

**Ferrite acceptance**

- Toggle both ways without duplicating tabs or losing the file identity. Render headings, quoted paragraphs, code and links as distinct content.
- Activate a relative file link from rendered Markdown and verify the destination path and contents.
- Opening a search result must prioritize the requested source line over an incompatible rendered view.

## FILES-03: Route Find according to the surface that owns focus

**Observed:** With the source editor focused, Command-F opened content Find with case, whole-word and regex controls; COBALT_LANTERN navigated and displayed 18 of 80. In an earlier rendered mode, Command-F focused the explorer search. With the chat composer focused and a PR panel open, Command-F later opened Find in thread. Observation log: line 110, line 121.

**State transition:** Focused source editor → dependency-owned editor Find; focused composer → chat.find if enabled. Rendered Markdown is a separate ChatMarkdown surface and has no content-Find handler in its wrapper.

**Installed implementation**

- T3 imports its editable file editor from @pierre/diffs/edit; the search UI internals are owned by that dependency. [FilePreviewPanel.tsx · Editor import and createFileEditor, L33–40 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L33-L40)
- createFileEditor constructs the dependency's Editor rather than a separate T3 editor implementation. [FilePreviewPanel.tsx · createFileEditor, L598–603 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L598-L603)
- EditableFileSurface mounts EditProvider, Virtualizer and File with editing enabled and editor callbacks. [FilePreviewPanel.tsx · EditableFileSurface, L874–902 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L874-L902)
- Default Command-F bindings select preview.find when previewFocus is true and chat.find otherwise outside terminal focus. [keybindings.ts · DEFAULT_KEYBINDINGS, L79–116 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/keybindings.ts#L79-L116)
- Browser preview focus is a connected webview or an ancestor with data-preview-panel-mode. File-rendering mode alone is not this condition. [previewFocus.ts · isPreviewFocused, L1–6 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/previewFocus.ts#L1-L6)
- ChatView computes focus context and opens thread Find only for a server thread with threadFind enabled. [ChatView.tsx · getShortcutContext and chat.find, L2667–2719 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L2667-L2719)
- useChatShortcuts leaves an already-prevented non-terminal event alone and only consumes a successfully handled command. [useChatShortcuts.ts · handleShortcut, L67–93 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatShortcuts.ts#L67-L93)
- The installed explorer supplies its own search input/useFileTreeSearch. Its root has no newer preview-action Find subscription or explicit Command-F capture handler. [FileBrowserPanel.tsx · FileSearchField and FileBrowserPanel root, L474–499 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FileBrowserPanel.tsx#L474-L499)
- Rendered file content uses ChatMarkdown; this wrapper does not install a rendered-content Find handler. [FileMarkdownPreview.tsx · FileMarkdownPreview, L7–36 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FileMarkdownPreview.tsx#L7-L36)
- The installed dependency and patch selection both target @pierre/diffs 1.7.0-beta.3. [pnpm-workspace.yaml · catalog and patched dependency, L240 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/pnpm-workspace.yaml#L240-L240)

**Evidence limits**

- Source-editor Find is runtime verified, but the dependency's exact keyboard/search algorithm was not independently traced here. T3 pins @pierre/diffs 1.7.0-beta.3.
- The explorer landing in the rendered-mode attempt is recorded behavior. The exact active element/event path was not captured, so its cause is unresolved.
- The immediately subsequent commit eceeab4d5c5bb02b58906978ff9e50bbd595ec19 is titled fix(web): Cmd+F in the Files panel focuses its search (#34346). Its new handlers must not be cited as the implementation installed in this audit.
- Neither this source nor the live attempt certifies Find within rendered Markdown contents. It must not be counted as a T3 runtime pass.

**Ferrite acceptance**

- With editor focus, Command-F searches the file and Enter moves the visible content match; include a match outside the initial viewport.
- With composer focus and a right panel open, Command-F searches the conversation and leaves the panel intact.
- Define and visibly expose the intended rendered-Markdown Find scope. Test source, rendered text, explorer, browser and composer separately, retaining an unsent draft in all cases.
- Capture the focused surface before each shortcut and a fresh state afterward. Do not attribute a stale accessibility snapshot to failed routing.

## FILES-04: Project search opens the matching file and requested line

**Observed:** Command-Shift-F opened project content search. COBALT_LANTERN returned 82 matches in 3 files. Clicking README line 8 opened README.md source. Observation log: line 111.

**State transition:** Command-Shift-F → content search overlay → query scoped by environment and cwd → result path+line → file tab with new reveal id → source line reveal.

**Installed implementation**

- The default project search shortcut is mod+shift+f outside terminal focus. [keybindings.ts · DEFAULT_KEYBINDINGS, L86–88 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/keybindings.ts#L86-L88)
- The command scope dispatches ToggleMode; CommandPalette's content mode renders ProjectContentSearchDialog. [commandPaletteCommands.ts · toggleOverlay / projectSearch.toggle, L23–29 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/shortcuts/commandPaletteCommands.ts#L23-L29)
- The active content mode renders the search dialog rather than the filename picker. [CommandPalette.tsx · content mode, L387–390 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/CommandPalette.tsx#L387-L390)
- The hook preserves query whitespace, debounces it and sends cwd, query, limit and matching options; it exposes pending, error, truncation and regex-fallback state. [queries.ts · useProjectContentSearch, L359–404 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/state/queries.ts#L359-L404)
- openMatch checks readable/current results, closes the search overlay and opens the result path at lineNumber. [ProjectContentSearchDialog.tsx · openMatch, L119–157 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/search/ProjectContentSearchDialog.tsx#L119-L157)
- The server dispatches content search to workspaceEntries.searchContents. [ws.ts · projectsSearchContents, L2870–2881 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/ws.ts#L2870-L2881)
- File line reveal clamps the requested line, keys requests by path/id, computes centered scrolling and cancels its short stabilization guard on wheel, touch, pointer or keyboard input. [FilePreviewPanel.tsx · useFileLineReveal, L435–565 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L435-L565)

**Evidence limits**

- The reported counts are fixture-specific. Search-result highlight geometry was not independently measured.
- The stabilization guard and cancellation behavior are source-supported; the live check did not exercise every manual-scroll interruption or late-layout case.

**Ferrite acceptance**

- Use a fixture with a known match count across several files. The count, file grouping and opened file must agree with disk.
- Activate a result outside the initial viewport. The requested line must become visible in the intended file even if rendered Markdown had been preferred.
- Once the user scrolls or interacts, reveal stabilization must stop fighting their input.
- Exercise no results, invalid regex and denied/unavailable workspace states without leaving stale clickable results.

## FILES-05: Edits persist and undo restores disk contents

**Observed:** Appending EDITOR_SAVE_SENTINEL and pressing Command-S produced exactly the expected disk diff. Undo and Command-S restored an empty diff. Observation log: line 112.

**State transition:** Editor change → optimistic file-query contents → per-file ordered save coordinator → 500 ms debounce → projectsWriteFile RPC → filesystem overwrite → confirmed cache state.

**Installed implementation**

- handleEditChange updates edited contents and the project file cache, then changes the held save session. [FilePreviewPanel.tsx · handleEditChange, L700–732 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/FilePreviewPanel.tsx#L700-L732)
- The per-file session uses a 500 ms debounce and calls projectEnvironment.writeFile with environment, cwd, path and contents; releasing the final lease flushes. [fileSaveSessions.ts · fileSaveSessions, L119–159 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/fileSaveSessions.ts#L119-L159)
- FileSaveCoordinator serializes persistence, tracks revisions and schedules the newest draft after an in-flight write. It clears pending only after confirmation. [fileSaveCoordinator.ts · change / persistLatest, L26–99 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/files/fileSaveCoordinator.ts#L26-L99)
- The server write endpoint delegates to WorkspaceFileSystem. [ws.ts · projectsWriteFile, L2916–2926 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/ws.ts#L2916-L2926)
- WorkspaceFileSystem checks workspace membership, resolves the write target, writes the contents and avoids rescanning existing-file autosaves. [WorkspaceFileSystem.ts · writeFile, L543–592 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/workspace/WorkspaceFileSystem.ts#L543-L592)
- The app default mod+s binding names composer.stash; the observed write must not be treated as proof of an editor-specific Save shortcut. [keybindings.ts · DEFAULT_KEYBINDINGS, L88–94 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/shared/src/keybindings.ts#L88-L94)

**Evidence limits**

- The live diff establishes persistence and undo restoration. Autosave can explain both writes; Command-S causality is not established.
- No source claim is made that a save-specific Command-S handler exists inside the editor dependency.
- The fixture restored cleanly. Save failure, permission loss, rapid edit/close and concurrent edits remain additional acceptance cases.

**Ferrite acceptance**

- Append a unique sentinel, observe persistence on disk, undo, and verify the disk content returns exactly to its original state.
- Verify edit focus separately: Command-S must not unexpectedly send or stash the chat draft.
- Close a file immediately after editing and reopen it; the last accepted contents must survive.
- A failed write must retain the unsaved content and expose a recoverable pending/error state rather than presenting an unearned saved state.

## TERMINAL-01: A panel terminal executes in the intended workspace and displays actual output

**Observed:** A real Python command printed TERMINAL_UI_SENTINEL and the fixture's canonical cwd in the panel terminal. Observation log: line 113.

**State transition:** Add Terminal → allocate terminal id → place panel tab → server open with workspace/worktree cwd → attached session output → Ghostty surface.

**Installed implementation**

- addTerminalSurface places the new terminal in the right-panel store and focuses the launched id. [useChatTerminalActions.ts · addTerminalSurface, L330–338 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L330-L338)
- launchTerminal computes the project/worktree location, places the destination before opening, and sends thread, terminal id, cwd and runtime environment. [useThreadTerminalActions.ts · defaultLaunchLocation / launchTerminal, L73–110 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/terminal/useThreadTerminalActions.ts#L73-L110)
- ThreadTerminalDrawer attaches a session; writes include the same environment/thread/terminal identity. [ThreadTerminalDrawer.tsx · terminal session and writeTerminal, L624–636 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L624-L636)
- Output reset and append frames call resetAndWrite or write on the actual terminal surface. [ThreadTerminalDrawer.tsx · writeTerminalOutputUpdate, L163–171 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L163-L171)
- The terminalOpen and terminalWrite RPC handlers call TerminalManager, whose write path requires a running session and writes input to its PTY process. [Manager.ts · write, L3333–3354 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/terminal/Manager.ts#L3333-L3354)

**Evidence limits**

- The shell's missing local powerlevel10k theme messages are observed environment output, not evidence of a T3 terminal defect.
- This check did not certify reconnect, very large output, shell restart or every keyboard chord.

**Ferrite acceptance**

- Print a unique sentinel and canonical cwd from a real bounded command. Confirm both in the terminal viewport and against the intended project.
- Keep a chat draft while entering terminal input; keystrokes must reach only the active terminal.
- Output must update in the active pane without opening an unrelated background-work route.

## TERMINAL-02: Ctrl-C interrupts foreground work and returns a usable prompt

**Observed:** Ctrl-C interrupted a 120-second command within the same 3.2-second tool call, producing KeyboardInterrupt and a fresh prompt. Observation log: line 114.

**State transition:** Focused terminal key → Ghostty encodeKey → onData → terminal write RPC → running PTY input → foreground interruption → output/prompt.

**Installed implementation**

- GhosttyTerminalSurface checks local input ownership and clipboard shortcuts, then encodes the key and forwards nonempty data to onData. [surface.ts · onKeyDown, L1110–1220 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/terminal/ghostty/surface.ts#L1110-L1220)
- The terminal surface's onData calls handleData, which permission-checks and calls writeTerminal; failures render a terminal system message. [ThreadTerminalDrawer.tsx · handleData, L1093–1103 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L1093-L1103)
- TerminalManager writes the received data to the running PTY and NodePtyProcess forwards it to node-pty. [NodePtyAdapter.ts · write, L193–195 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/terminal/NodePtyAdapter.ts#L193-L195)

**Evidence limits**

- The live timing establishes interruption rather than natural completion. This is distinct from the close-pane termination path.
- The source trace does not assert a captured literal Ctrl-C byte or that close escalation was exercised.
- Copy behavior is platform/selection dependent. The test used Ctrl-C as interrupt in the observed macOS session.

**Ferrite acceptance**

- Run a bounded command long enough that natural completion cannot explain the result, interrupt it, and show a new usable shell prompt.
- Typing a subsequent harmless command must work in the same pane.
- Retain the other panes and chat draft; interruption must affect the intended terminal process.

## TERMINAL-03: Split creates a separate terminal with clear pane ownership

**Observed:** Split Terminal Horizontally created a second terminal input/pane. Observation log: line 115.

**State transition:** Split active right-panel terminal → allocate/open new terminal id → add to same terminal surface with horizontal direction → focus new id.

**Installed implementation**

- splitPanelTerminal validates the active terminal surface and group limit, launches a terminal placed by store.splitTerminal, and focuses the new id. [useChatTerminalActions.ts · splitPanelTerminal, L341–358 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L341-L358)
- TerminalSplitPanes maps horizontal to equal grid columns and vertical to equal grid rows, with an independently keyed terminal for each id. [ThreadTerminalDrawer.tsx · TerminalSplitPanes, L1300–1347 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ThreadTerminalDrawer.tsx#L1300-L1347)
- Keyboard split/new/close commands route to the right-panel terminal when it owns focus and otherwise to the drawer. [useChatTerminalActions.ts · terminal shortcuts, L409–442 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L409-L442)

**Evidence limits**

- The source resolves the horizontal/vertical naming precisely. Live verification covered horizontal creation, not vertical splitting or the maximum-pane limit.

**Ferrite acceptance**

- Split horizontally and confirm two side-by-side usable inputs. Print a different sentinel in each and verify output isolation.
- Focus either pane, then use split/close shortcuts. Only the owning terminal group should change.
- Tab order, pointer focus and the active-pane indication must agree.

## TERMINAL-04: Closing a pane stops its session and removes it without orphaning work

**Observed:** Closing each audit-owned terminal showed the stop-process/history-clear confirmation. Confirmation removed both panes and returned to the file surface. Process inspection found no remaining bounded audit Python command. Observation log: line 115.

**State transition:** Close request → confirmation → terminal close command with deleteHistory → server stop/unregister/persist/flush/remove → closed event → history deletion → remaining surface/focus.

**Installed implementation**

- requestClosePanelTerminal waits for confirmTerminalClose and rechecks permission. closePanelTerminal closes the session, removes both UI representations and focuses the surviving panel terminal. [useChatTerminalActions.ts · closePanelTerminal / requestClosePanelTerminal, L373–403 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useChatTerminalActions.ts#L373-L403)
- The local confirmation explicitly describes process stopping and history clearing; failed dialog access resolves false. [terminalCloseConfirm.ts · confirmTerminalClose, L9–34 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/terminalCloseConfirm.ts#L9-L34)
- closeTerminalSession requests deleteHistory:true, with an exit-line fallback after a non-interrupted close failure when permission remains. [useThreadTerminalActions.ts · closeTerminalSession, L117–135 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/terminal/useThreadTerminalActions.ts#L117-L135)
- TerminalManager closeSession stops the process, unregisters and removes the session, publishes closed and conditionally deletes history. [Manager.ts · closeSession, L2589–2627 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/terminal/Manager.ts#L2589-L2627)
- Process termination subscribes to exit before SIGTERM and escalates to SIGKILL only after the configured grace period. [Manager.ts · runKillEscalation, L1846–1907 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/terminal/Manager.ts#L1846-L1907)

**Evidence limits**

- The confirmation text and visible cleanup were exercised. Persisted history deletion and forced SIGKILL escalation were not independently verified.
- The independent process inspection covers the bounded audit command, not every possible descendant process tree.

**Ferrite acceptance**

- Create two disposable terminals, start owned bounded work, close one, and confirm the other remains usable.
- After confirming the final close, return to the expected surviving surface and verify the owned process is gone.
- Cancellation must retain the pane and running process. Do not claim cleanup solely because a tab disappears.

## PROVIDER-01: Show a provider failure, refresh it, and distinguish visible recovery from diagnosis

**Observed:** A Codex banner reported a provider probe failure with workspace routing discovery timed out. Refresh entered a disabled in-flight state. A later fresh observation found the banner absent. Observation log: line 116, line 120.

**State transition:** Provider probe failure → error snapshot with wrapped error.message → active provider banner → guarded refresh with instanceId/refreshModels → RPC instance refresh/model refresh → updated provider publication → banner eligible to disappear.

**Installed implementation**

- probeCodexAppServerProvider initializes the client, reads the account, then performs skills, model and configuration discovery with deadlines. [CodexProvider.ts · probeCodexAppServerProvider, L596–660 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/provider/CodexProvider.ts#L596-L660)
- The failure branch formats Codex app-server provider probe failed from error.message. The separate outer timeout branch uses different text. [CodexProvider.ts · probeResult failure and timeout branches, L840–899 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/provider/CodexProvider.ts#L840-L899)
- A protocol request error can preserve the upstream error message and retain method/request id metadata. The screenshot did not expose that metadata. [errors.ts · CodexAppServerRequestError, L250–282 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/effect-codex-app-server/src/errors.ts#L250-L282)
- The banner displays status.message. Ready/disabled states have no banner key, while a dismissed matching key can also suppress the banner. [ProviderStatusBanner.tsx · getProviderStatusBannerKey / shouldShowProviderStatusBanner / getProviderStatusMessage, L26–76 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderStatusBanner.tsx#L26-L76)
- Refresh guards concurrent activations, sets pending, requests the selected instance with refreshModels:true and resets pending after the command returns. [ProviderStatusBanner.tsx · refresh, L107–123 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderStatusBanner.tsx#L107-L123)
- The Refresh button is disabled while pending or unauthorized, and changes its label to Refreshing. [ProviderStatusBanner.tsx · Refresh button, L166–175 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderStatusBanner.tsx#L166-L175)
- The client refresh command uses single-flight concurrency. Because the banner sends no cwd, the server chooses instance refresh rather than the workspace snapshot branch. [server.ts · refreshProviders, L1472–1508 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/state/server.ts#L1472-L1508)
- The RPC invalidates requested model caches, awaits instance refresh, optionally refreshes instance models and refreshes the instance again. [ws.ts · serverRefreshProviders, L2391–2449 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/ws.ts#L2391-L2449)
- refreshOneSource refreshes and reads the latest provider snapshot before synchronizing it, rejecting stale replaced instances. [ProviderRegistry.ts · refreshOneSource / refreshInstance, L613–628 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/provider/ProviderRegistry.ts#L613-L628)
- refreshInstance resolves the current instance and delegates to refreshOneSource. [ProviderRegistry.ts · refreshInstance, L693–702 ](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/provider/ProviderRegistry.ts#L693-L702)

**Evidence limits**

- The observed phrase matches the propagated-error branch, not T3's generic outer-timeout message. The originating RPC, response payload, provider route and timeout cause remain unknown.
- The banner appeared during terminal inspection; no source/runtime evidence links terminal keystrokes to this provider failure.
- A later absent banner establishes visible recovery. It does not alone prove a ready snapshot, successful model execution or the cause of recovery.
- Do not identify the workspace snapshot refresh path merely from the words workspace routing in the error. The clicked banner refresh passes an instance id and no cwd.

**Ferrite acceptance**

- Display the actionable provider error without replacing or discarding the user's thread, terminal state or draft.
- Refresh must visibly enter one pending state, reject repeat activation, then leave pending and show the new status or a recoverable error.
- Verify the final status and a bounded provider action separately if claiming functional recovery.
- For diagnosis, retain the correlated request method/id, error code/message, timing and relevant route metadata without collecting credentials.

## What remains unresolved

- Rendered Markdown content Find and exact focus route of the explorer-search attempt.
- Editor dependency internals of Find and any save-specific Command-S handling.
- Exact source of the provider discovery error, including RPC metadata and runtime payload.
- Independent provider ready-state/model action after refresh.
- Persisted terminal history deletion, forced termination escalation and all descendant cleanup.

The newer Files-panel shortcut change is explicitly outside this installed build: [eceeab4d, Cmd+F in the Files panel focuses its search](https://github.com/pingdotgg/t3code/commit/eceeab4d5c5bb02b58906978ff9e50bbd595ec19). Its existence cannot turn the rendered-mode observation into a certified content-search feature or prove the cause of that focus result.

## Applied principles and verification

- **Experience First:** Acceptance criteria require completed file navigation, focused search, persisted edits and owned process cleanup rather than visible controls.
- **Prove It Works:** Kept parent runtime evidence separate from source support; qualified Command-S causality, rendered Find and provider recovery.
- **Poteto mode / How:** Pinned every installed-path anchor to the app's source SHA and followed UI actions into stores, RPC and backend side effects.

Documentation validation checks the JSON structure, installed SHA on every source URL, source-anchor bounds against fetched raw files and the two output paths. No application tests were appropriate or run for this read-only report.

