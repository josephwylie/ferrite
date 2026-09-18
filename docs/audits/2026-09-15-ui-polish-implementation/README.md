# Ferrite UI polish implementation

Branch: `fix/ui-polish-2026-09-15` (implementation based on `844fd36`). This work addresses the 22 entries in the UI quality audit. The separate 42 functional UX findings remain a separate backlog; this report does not close that audit wholesale.

The existing neutral desktop design remains. Prose still uses the full available width: no reading-width cap was added. Solo and fullscreen answer text now have a saved Standard / Comfortable / Large size preference; Standard preserves the existing 13px size, and Group answers retain their compact default. Code, diffs and operational metadata keep their existing scale and width.

The [18 September sizing follow-up](#18-september-sizing-follow-up) extends this implementation with adaptive Composer space, scrollable queues/navigation, form proportions, and Markdown alignment. Earlier verification below is retained as the first-pass record.

## Coverage

Product source was verified at `ba6ffd3`. Five subagents implemented the work; the coordinating agent reviewed, integrated and checked it. The original audit branch remains unchanged.

| Audit ID | Final behavior |
|---|---|
| UI-01 | Questions fit their owning Pane, with internal scrolling and fixed actions. Compact or crowded Panes expose Expand to answer in the header. Native measurements restore inline forms when space returns. Expansion preserves answers and the full draft; while answering fullscreen, the Composer shows two scrollable rows, then restores its normal height. |
| UI-02 | L2 measures and retains complete transcript rows in the remaining space, favoring the newest rows instead of compressing glyph baselines. |
| UI-03 | The Composer has pointer Send and Stop actions, Enter/newline guidance and busy queue guidance. Actions route to the owning Pane and preserve existing queue/interrupt semantics, including a submitted prompt awaiting the provider's running event. In compact L2 Panes the actions sit on the existing metadata row, preserving the editor's full width; L1 placement and the two-row idle height remain. |
| UI-04 | Settings has one category heading, separate provider model/effort groups, current-value model menus and bounded choice groups with persistent checkmarks. Menus represent CLI default, discovered models, aliases and saved custom values; hidden option labels remain searchable. |
| UI-05 | Busy model/effort choices retain the current checkmark and explain that changes become available when the turn finishes. Menus update on busy transitions and recheck availability at selection time. |
| UI-06 | Typed and accepted Composer text uses primary ink; unaccepted suggestions remain subordinate. Tab accepts only from the focused empty Composer. Leaving a code action preserves the draft, suggestion and clipboard. |
| UI-07 | The parent subject visibly says Main. Main and the selected child remain visible ahead of other child tabs; provider order remains stable. |
| UI-08 | Behaviour → Reading exposes the persisted Solo answer-size preference. It affects Solo/fullscreen prose and headings without narrowing prose or increasing dense Group text. |
| UI-09 | Form controls use opaque neutral hover surfaces and visible keyboard outlines. Dark controls use a 2px TEXT_2 inset outline; the light primary action uses a dark outline. Focus remains independent of hover and does not alter the global Pane ring. Settings switches use the same focus treatment. |
| UI-10 | L2 completion fades historical body content while keeping its header and editable Composer at normal contrast; completion uses one header label. |
| UI-11 | Pane-scoped image previews offer Open Original using the canonical local file URL. Fit-to-Pane and dismissal remain. In-app 100% zoom/panning was not added. |
| UI-12 | Fenced code has a restrained block treatment, a language label when known, Copy and Copied feedback. Copy preserves exact source whitespace and omits toolbar labels; feedback remains until the source changes. Native keyboard traversal reaches Copy/HTML Preview and returns to the Composer without submitting a draft. |
| UI-13 | The contextual titlebar creation control visibly names New Group, Add Thread or New Thread from the same state as its actual placement action. The compact sidebar creation control remains. |
| UI-14 | Markdown headings use deliberate weight/size without automatic italic/underline decoration; authored emphasis remains supported. The Ferrite gutter mark aligns with the first heading's line height, including larger reading sizes. |
| UI-15 | Grouped failed tools retain one group failure tally and the useful child error, without repeated child failure badges. The Composer's real Stop action carries the interrupt instruction once. |
| UI-16 | Transcript spacing follows content shape: single-paragraph runs use 4px vertical padding and structured runs use 8px, down from 12px. This tightens commentary/tool sequences without adding a new turn separator. |
| UI-17 | The Settings sidebar paints its lower-left corner with the same radius as the outer card. |
| UI-18 | The Composer always reserves its prompt-mark gutter, keeping the text origin fixed through focus changes. |
| UI-19 | Enabled Project Create/Done uses a restrained light filled primary action with dark text; empty/disabled state remains distinct and inert. Add Directory remains secondary. The Name input has a darker neutral surface and a visible border, so its bounds are recognizable before typing; its input behavior is unchanged. |
| UI-20 | The keyboard-targeted disclosure header has a visible selected surface/outline before Enter activates it; the full header remains the pointer target. |
| UI-21 | Alert Panes retain their amber/red perimeter while the neutral focus treatment is inset, preserving both meanings without changing layout. |
| UI-22 | Useful diff line numbers and omitted-line counts use readable metadata ink instead of the decorative separator token. |

## Implementation boundaries

- Settings choices still save through the existing Settings action. Showing an explicit Codex CLI default and preserving alias/custom selection were necessary for a truthful current-value chooser; the broader defaults/effort behavior from the functional audit was not redesigned.
- No provider transport, tool permissions, dependency versions, or general session lifecycle was changed for this polish pass.
- UI-11 adds a concrete inspection path through the user's external application; it does not claim an in-app image editor or zoom system.
- Windows runtime, real-provider busy/startup transitions, and OS Open Original launch need explicit evidence before being described as exercised. Source and headless tests alone do not establish those runtime outcomes.

## Verification

- Production build: `cargo build -p ferrite --locked` **passed**. [Build log](evidence/production-build.log).
- Final application suite: **364 passed, 3 failed, 2 ignored**. All new polish regressions passed. [Final app log](evidence/final-app-tests.log).
- Core suite, including the persisted reading-size API: **802 passed, 22 ignored**. [Core log](evidence/core-tests.log).
- `git diff --check 844fd36 -- crates vendor` passed for all product changes. Evidence files retain verbatim command output and patch context, including their original whitespace. Repository-wide formatting already failed before this work; the original drift is recorded in the [baseline formatting log](evidence/baseline-formatting.log). Owned changes were formatted without unrelated cleanup.

The baseline application suite had **346 passed, 4 failed, 2 ignored** ([baseline log](evidence/baseline-app-tests.log)). The held-Space subject-navigation failure is fixed without weakening its assertions. Three existing failures remain:

| Existing failure | Final disposition |
|---|---|
| `retained_transcript_relative_file_links_use_the_thread_workspace_and_copy_text` | Same final-character selection mismatch as baseline: selected text ends in `next s` instead of `next steps`. |
| `the_caret_is_solid_on_focus_then_blinks_and_typing_resets_it` | Same caret timer assertion as baseline. |
| `the_workspace_chip_scopes_to_the_chosen_project` | Fixture assumes a `master` initial branch; this machine's Xcode Git configuration defaults to `main`. The unchanged test passes with a process-local `init.defaultBranch=master` override. No global configuration was changed. [Environment check](evidence/workspace-environment-check.log). |

One earlier combined run also failed the existing native attachment-drop assertion. It passed unchanged both in isolation and in the complete repeat; the cause is unproven. [Earlier run](evidence/initial-final-app-tests.log), [isolated rerun](evidence/native-drop-rerun.log).

The final app suite includes five Composer ownership/availability tests, five layout tests, ten Question tests, six provider-form tests, model selection/search/persistence flows, native subject navigation and file routes, exact code copying, reading-size selection retention, and Copy-to-Composer traversal with a pending suggestion. These use production GPUI controls with synthetic providers; live-provider and Windows runtime behavior are not claimed.

## Native visual evidence

**28 final frames captured successfully and reviewed:** [gallery with original-audit comparisons](gallery.md). The initial 25-frame inspection was followed by a correction batch and one confirmation round. Confirmed states include cramped and expanded Questions, eight-line draft preservation, compact Composer actions, selected Settings control focus, enabled Create focus, Project Name bounds, heading marks, code-copy feedback, subject tabs and representative screenshot previews. Lower Question content scrolls within the available viewport.

The selected Claude control shows the light inset focus outline; enabled Create shows the dark inset outline. The Copy fixture activates the real control and asserts exact clipboard contents. [Native capture log](evidence/native-capture.log), [image hashes](evidence/capture-manifest.json).

The capture-only [fixture patch](evidence/native-fixture.patch) was applied in a disposable checkout, not to product source. To reproduce, create a disposable checkout of `ba6ffd3`, apply that patch and run `cargo run -p ferrite --locked --features visual-reference -- --visual-reference /tmp/ferrite-polish-capture`. Set `FERRITE_POLISH_IMAGE` to the absolute path of `docs/audits/2026-09-15-ui-quality/screenshots/settings-desktop.png` for the representative preview. The helper lets native Question layout settle before capture; it does not alter component sizing.

The screenshots use disposable stores and synthetic conversations through Ferrite's production renderer. The installed app and its real sessions were not used for this implementation pass. Open Original is tested through its canonical file-URL route; an external viewer was not launched.

## 18 September sizing follow-up

Five subagents reviewed and implemented this pass using the Impeccable polish workflow, with the coordinator integrating changes and checking the native application. The work follows the existing Soft theme, dense desktop layout, and full-width prose.

| Root cause | Result |
|---|---|
| Composer sizing ignored the actual height of each Pane. | Drafts keep their full contents while their visible rows adapt to the Pane. Queues have a bounded viewport, a visible count, and independent scrolling per Pane. Stop and Send remain available in short four-Pane layouts. Compact live captions and elapsed time share one complete row above the queue. |
| The collapsed rail had no independent overflow area. | Thread targets scroll between pinned creation and utility actions; expanded Project headers keep their height. |
| Form sizes and modal proportions were implemented separately. | Shared 32px controls, 48px headers, 16px insets, and 12px gaps establish consistent geometry. Small Settings choices fit their contents; larger sets use the existing current-value menu. |
| Project actions shared the directory list's scroll area. | One-directory dialogs are shorter; additional directories grow the form within the viewport. Create/Done stays fixed, and folder names lead each row above the secondary path. |
| List markers, table headings, and code actions used inconsistent measurements. | Numbered lists share a measured gutter through digit changes, including continuations and nesting. Table headings respect column alignment. Copy/Copied has a stable padded target beside Preview. |

The default sidebar width and global text scale were preserved. The compact progress row is also checked with metadata, test badges, eight queued prompts, and an eight-line draft together; isolated empty fixtures do not establish that state fits.

### Follow-up validation

- Full application suite at `051ed18`: **374 passed, 3 pre-existing failures, 2 ignored**. [Log](evidence/sizing-app-tests.log). The three failures remain the file-link selection endpoint, caret timer, and Git initial-branch assumption listed above.
- Six new native regressions exercise adaptive draft geometry, queue reachability and keyboard behavior, neighboring queues' independent scroll positions, collapsed navigation reachability, fixed Project actions, list alignment, and displaying current reasoning once while preserving its history. Existing Copy checks now verify padded targets and stable confirmation bounds.
- Fresh native screenshots cover 20 states at window widths from 640 to 1440 logical pixels, including one/two/four/six Panes, compact Settings and Projects, Questions, queues, large reading text, tables, and code. Five additional native interaction frames verify scrolling and the complete effort menu. [Gallery](gallery.md#18-september-sizing-pass).
- A real, isolated native window was exercised through computer use at 640×500: open Settings, open the six-choice Claude effort menu, choose High, close/reopen Settings, confirm the selection remains, and scroll to the lower Codex controls. It used synthetic sessions and disposable preferences; the user's live sessions were not modified.

The [independent visual review](evidence/sizing-visual-review.md) records coverage and limitations. Screenshots use the production native renderer with disposable stores. The baseline long-draft frame contains an eight-line draft; the final stress fixture additionally contains eight accepted queue items. The baseline is evidence of draft crowding, not an identical queue comparison. The large-list fixture is an added state with valid continuation indentation.

Capture-only changes are archived in the [fixture patch](evidence/sizing-native-fixture.patch), with image provenance in the [manifest](evidence/sizing-capture-manifest.json). To reproduce, create a disposable checkout of `051ed18`, apply the patch, and run `cargo run -p ferrite --locked --features visual-reference -- --visual-reference /tmp/ferrite-sizing-capture`. The independent review lists the five interaction commands. No capture fixture was added to product source. Windows visual runtime and live-provider behavior remain outside this native macOS review.
