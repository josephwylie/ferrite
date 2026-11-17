# Navigation runtime correction

Runtime evidence for NAV-05/06/07 from a fresh scratch store using `target/debug/ferrite` and the copied fake Claude peer. No application code, build, tests, or native UI were used. Setup commands created disposable titled threads; user-facing interactions used actual headless window clicks, key events, and screenshots. No real model was invoked.

## Results

### NAV-05 — Search, select, open

**Reproduced: the highlighted result can be outside the visible search list.** After a 44-thread broad search was opened from a scrolled sidebar, the result list retained its previous scroll offset while the search cursor reset to result 0. The visible rows began at “Nav Sentinel 09”; the selected result “Nav Sentinel 02” was above the viewport. Pressing Enter opened thread 02. Evidence: [13-search-44-results.png](../evidence/navigation-correction/13-search-44-results.png), [14-search-offscreen-zero.png](../evidence/navigation-correction/14-search-offscreen-zero.png), and [15-enter-offscreen-zero-opens-02.png](../evidence/navigation-correction/15-enter-offscreen-zero-opens-02.png).

A separate 14-result check moved down twelve times, visibly highlighted result 14, and Enter opened “Nav Sentinel 14.” This confirms the normal keyboard path when the target is already visible; it does not resolve the above visibility failure. Evidence: [11-search-down-12.png](../evidence/navigation-correction/11-search-down-12.png) and [12-search-enter-12.png](../evidence/navigation-correction/12-search-enter-12.png).

### NAV-06 — Rename and lifecycle rows

- **Working row:** The rendered context-menu action was followed by a `rename-hint` selector at logical bounds x=70, y=226, w=160, h=20; `rename-thread-2` was absent, consistent with the title element having been replaced by an editor. `window type PRIMARY_RENAME_SENTINEL` returned `typed`; `window key Enter` returned `pressed` with no unhandled keys. However, screenshots 17–19 do not clearly show the full field or hint, and the title cell only shows a clipped tail in screenshot 18. `thread get 2` ran immediately after Enter, without a settle interval, and still showed the original title. No later state read was made. The provider log contains no sentinel prompt; its only matching entry is a normal startup `rename_session` control request for the original title. **Inconclusive:** editor state was likely active, but the retained evidence cannot establish what text the field held or whether Enter committed before the state read. Do not count this as a failed-save defect. Evidence: [16-row2-menu.png](../evidence/navigation-correction/16-row2-menu.png), [17-working-rename-editor.png](../evidence/navigation-correction/17-working-rename-editor.png), [18-working-rename-edited.png](../evidence/navigation-correction/18-working-rename-edited.png), [19-working-rename-saved.png](../evidence/navigation-correction/19-working-rename-saved.png), and [provider-host.jsonl](../evidence/navigation-correction/provider-host.jsonl).
- **Settled row:** A fake-provider exchange settled thread 4 (`attention: done`). The rendered row menu's Rename action displayed the inline editor hint. Typing and saving were not completed. Evidence: [22-settled-row-rename.png](../evidence/navigation-correction/22-settled-row-rename.png).
- **Snoozed row:** Snooze for one hour was selected through the rendered context menu. The expanded Snoozed shelf showed thread 5 as a slim row. Its offered Rename action closed the menu, but no `rename-hint` was drawn and the stored title stayed “Nav Sentinel 05.” Evidence: [24-snoozed-row.png](../evidence/navigation-correction/24-snoozed-row.png), [26-snoozed-row-menu.png](../evidence/navigation-correction/26-snoozed-row-menu.png), [27-snoozed-rename-attempt.png](../evidence/navigation-correction/27-snoozed-rename-attempt.png).
- **Parked row:** The shelf was expanded and paged, but the target was outside the currently drawn slice. No rendered parked-row Rename action was selected. **Untested.**

The Snoozed observation matches the existing source-backed finding `projects-navigation-001`; this run adds runtime confirmation for Snoozed only. It does not certify the Parked branch. Working rename completion is inconclusive; Settled editor visibility was observed, but completion was not checked.

### NAV-07 — Project and draft continuity

Draft preservation while switching threads, project-filter changes, and New thread retargeting were not exercised. **Untested.**

## Limits and cleanup

The search finding is bounded to a broad query opened after the sidebar had been scrolled; no live result reordering or disappearing-result case was induced. No rename persistence result is asserted for Working or Settled. No tests were run, as requested. `cases.json` records the case boundaries and event commands. `cleanup.json` records shutdown and PID/group verification.
