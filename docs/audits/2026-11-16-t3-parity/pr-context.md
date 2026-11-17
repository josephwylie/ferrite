# PR and context card source comparison

> PR status correction: [pr-current.md](pr-current.md) supersedes this report’s PR data, remote Code, and scope claims. The paragraphs below preserve the earlier investigation; they are not a description of the current PR implementation.

Reference checkout HEAD verified as `4da2bc520c38cd6b36014d2888774168fbf4c77f`.
Previous audit reports were treated as hypotheses. This comparison reads the components directly.

## Source rules

- `apps/web/src/rightPanelStore.ts` lines 211–214 initializes inline context visible and popover closed. Lines 763–780 opens a PR as a per-thread right-panel surface, retaining the chat route.
- `apps/web/src/components/chat/threadDetailsCardLayout.ts` lines 4–14 specifies a 12px gap, 280px width, and measured full, compact, essential density fallback. Lines 37–60 requires the card, gap, 32px clearance, lane padding, and minimum chat width to fit. Available height must reach 160px, including Find clearance.
- `apps/web/src/components/chat/chatCanvasLayout.ts` lines 22–54 defaults to 48px lane padding, 768px maximum and 640px minimum chat width. The lane starts at the canvas center until the card pushes it left.
- `apps/web/src/components/chat/ThreadDetailsCard.tsx` lines 43–68 separates inline/popover state and closes stale popover state when entering inline mode. Lines 117–126 uses a 16px rounded glass frame. Inline positioning is absolute with measured available height. Popup dismissal does not dismiss inline state.
- `apps/web/src/components/chat/ThreadDetailsPanel.tsx` lines 167–180 restricts environment to full density and Open In to full/compact. Branch, scripts and context survive essential density. Lines 238–283 removes the version-control separator in reduced density and hides servers, automation and relationships outside full density.
- `apps/web/src/components/chat/threadDetailsPanelStyles.ts` lines 12–20 defines 32px controls, 13px medium type, 10px row gap and horizontal padding, 16px icons.
- `apps/web/src/index.css` lines 454–466 defines glass from the popover token and a 10% contrast border. The no-backdrop fallback is opaque popover. Lines 1190–1198 defines the default dark popover from 97% neutral-950 plus 3% white, approximately #111111.

## Confirmed Ferrite causes and changes

`open_named_pr` formerly routed non-branch PRs through the full PR destination. This hides the conversation. Same-repository PRs now use a thread Panel surface. Other repository links retain the browser route. The repository is established from the checkout origin or the known branch PR URL.

`read_pr_snapshot` formerly passed a number only for Project sources. A Thread surface therefore requested the current branch PR even when its tab named another number. It now passes the requested number explicitly. The forge fixture boundary returns a branch fixture only when its number matches; an unmatched fixture returns NoPr rather than consulting a real CLI.

PR snapshot selection now prioritizes the active PR surface. Branch metadata changes retain other explicit PR surfaces. A PR that does not match the checkout branch has no checkout Code tab or branch actions. Its header link reads the snapshot URL.

The context card formerly used one transient visibility flag and a 584px threshold. Narrow opening substituted the Panel Overview. It now has separate default-on inline visibility and default-off popover visibility. Inline requires 1012px body width with the default T3 lane values and 160px usable card height. Find adds its top clearance to that height test. Outside presses and Escape leave inline context visible. Explicit toggling changes inline visibility. Narrow opening leaves the Panel tool intact.

The card uses measured full and compact content heights before essential fallback. The cache invalidates on thread, width, branch, and context availability. Full-only machine and nonessential folder rows map existing Ferrite controls to the corresponding T3 density categories. Branch and context remain. The generic Environment header, thread title and Open overview footer have been removed from this card. The Overview keeps its full section presentation.

The lane uses the clamped-center formula. It retains Ferrite's existing 736px reading-column maximum. It reserves only the necessary trailing space, so a very wide canvas does not shift the conversation unnecessarily.

## Explicit differences

This is not exact parity. Ferrite's existing context domain does not have T3's scripts, Open In editor picker, automation, lineage, or side-chat card controls. No controls were invented for this scoped correction. The folder row still opens the existing system folder action. T3's full card may contain those sections when available.

GPUI's available surface primitive does not supply CSS backdrop blur and saturation. The card uses T3's specified opaque fallback color and border instead. This is a concrete rendering difference, not a pixel-parity claim.

Ferrite keeps its existing 736px transcript maximum versus T3's 768px default. Context visibility is retained by the existing per-thread Pane lifetime, rather than persisted in a right-panel store across app restarts. Measured content changes while reduced density is already active can retain the last full-density height until the cache key changes.

## Verification ownership

The parent captures the built before/after interfaces and exercises the changed UI. The new PR regression in `cockpit/tests/prs.rs` opens PR #7 while branch metadata names #48 and checks that #7 loads, the conversation destination stays visible, and checkout actions and Code do not appear.

The details tests cover narrow popover dismissal, preserved Panel tools, default-on inline context, explicit hide/restore, Escape persistence, width, Find separation, and focus movement between transient popovers. Final commands and results are recorded in the parent report.

Model the Domain shaped the separate inline and popover state. Fix Root Causes shaped the PR routing and explicit numbered read, rather than button-specific presentation fixes. Both leaf skills were read in this session.

## Parent correction pass

Independent review found that PR identity appeared below tabs and vanished outside Summary. The existing title and branch fields now form a persistent header above the tabs. The PR body and header use the black background specified by T3's `PullRequestDetailPanel` root.

Bare numbered references use `/issues/N`, so same-repository recognition accepts that form as well as `/pull/N`. A matching number in a foreign repository cannot open the checkout's PR. A response for the wrong requested number becomes a read failure and never replaces the requested snapshot. Explicit PR tabs survive branch changes, including restoration from saved state. The old retirement helper was removed from runtime and fixture paths.

The wire snapshot lacks T3's author, update time and changed-file statistics. Those fields were not invented or added across repository boundaries. A PR for another branch has no checkout Code view or checkout actions because those operate on Ferrite's local branch. Remote PR diff fetching remains outside this change.
