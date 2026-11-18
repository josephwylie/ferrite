# Installed T3 source provenance

The audit goal is to compare the real installed T3 Nightly with Ferrite and give each source trace the correct revision. This report establishes provenance and checks a bounded set of material differences. It does not repeat the parent's live audit.

throughput checkpoint: n/a, read-only investigation.

## Overview

The installed `/Applications/T3 Code (Nightly).app` identifies **`1264c206aee0dce805d6279809118bc091f29d2f`** as its source commit. Its version is `0.1.10-nightly.20261111.4544`. The [published release](https://github.com/pingdotgg/t3code/releases/tag/v0.1.10-nightly.20261111.4544) targets that same full SHA. The [commit](https://github.com/pingdotgg/t3code/commit/1264c206aee0dce805d6279809118bc091f29d2f) is dated `2026-11-11T22:13:05Z`; the release was published at `2026-11-11T22:22:16Z`.

The pinned source checkout `/tmp/ferrite-t3-audit-source-20261113` remains at `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. GitHub's [comparison](https://github.com/pingdotgg/t3code/compare/1264c206aee0dce805d6279809118bc091f29d2f...74573fc29b901bafbf8f1dbb2eb29606da1fa448) reports 746 commits ahead and zero behind. All 746 unique commit records were collected through eight paginated compare requests. The JSON includes their SHAs, timestamps, subjects, and links.

Use the installed SHA for source citations attached to live app observations. The newer checkout is useful for additional design comparison only when its version is stated. A newer source implementation does not establish installed behavior.

## Provenance evidence

| Evidence | Observed value |
| --- | --- |
| `Contents/Info.plist` | `CFBundleShortVersionString` and `CFBundleVersion` both equal `0.1.10-nightly.20261111.4544`. |
| `Contents/Resources/app.asar`, internal `/package.json` | `version` and `buildVersion` equal that version; `t3codeCommitHash` equals `1264c206aee0`. |
| Same package metadata | Main entry is `apps/desktop/dist-electron/boot.cjs`; homepage is `https://t3.codes`. |
| `Contents/Resources/app-update.yml` | GitHub owner `pingdotgg`, repository `t3code`, channel `nightly`, release type `prerelease`. |
| [Release API](https://api.github.com/repos/pingdotgg/t3code/releases/tags/v0.1.10-nightly.20261111.4544) | `target_commitish` equals `1264c206aee0dce805d6279809118bc091f29d2f`. |
| [Commit API](https://api.github.com/repos/pingdotgg/t3code/commits/1264c206aee0) | Resolves the embedded 12-character hash to `1264c206aee0dce805d6279809118bc091f29d2f`. |
| Installed `app.asar` whole-file SHA-256 | `d250da790f086e622f59220e5abaccfebd8e2fb92f129cdd506482f611d646af`. |
| ASAR file index | No source maps outside dependencies. Original TypeScript source was fetched from the exact public commit. |

The ASAR was read in place using its header and offsets. No extraction or app execution was required. The whole-file SHA-256 above is a local artifact fingerprint. It is distinct from the Electron ASAR header integrity hash stored in the plist. Agreement between package metadata and release target establishes the source identity used for this audit. It does not prove a reproducible build or exclude downstream packaging changes.

## Key concepts

Passage citations and whole-response replies are different features. The installed code already captures selected assistant text, adds a comment, serializes the citation, and navigates back to the source. The newer checkout adds a separate hover action that replies to an entire response with a response context record.

Command text controls and live command output are also different components. `ShellCommandBlock` renders command actions. `CommandOutputPanel` subscribes to running output. A successful command output check must cite the adapter, output hub, and panel rather than command text controls alone.

Provider behavior must stay qualified. An installed Codex stream can work while a later commit fixes Cursor shell output or Claude assistant text streaming. The later fixes are not evidence against the parent's successful Sol command stream.

## How the installed paths work

### Citation selection, comments, and sending

The installed [ChatView citeAssistantText handler](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L623) calls the composer ref. The installed [ChatComposer citation insertion](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ChatComposer.tsx#L2939) inserts the citation into the current draft. [AssistantCitationCommentEditor](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantCitationCommentEditor.tsx#L10) already supports comment submission and an optional submit-and-send callback. [useComposerSend](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/useComposerSend.ts#L260) already snapshots image attachments and message context for sending. These are source traces for the installed path, not worker confirmation of a delivered message.

The installed [AssistantCitationEmbed](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantCitationChip.tsx#L53) renders a quoted response and its comment. [AssistantCitationSource](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/AssistantCitationSource.tsx#L335) provides the citation source behavior. [assistantCitationNavigation](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/lib/assistantCitationNavigation.ts#L79) computes navigation to it. Navigation, source highlighting, the citation contract, and shared citation serialization are byte-identical in the inspected revisions. The newer `AssistantCitationChip.tsx` refactors shared card rendering to add `ResponseReplyCard`. The ordinary citation feature predates that addition.

A live check of editable text selection, comment entry, image attachment, send, and provider receipt therefore belongs to the installed SHA. A check for the hover action named `Reply to this response` belongs to the newer source. The installed `TimelineAssistantRows.tsx` has no `AssistantReplyButton`; the comparison diff adds it.

### Child agents and Open parent

The installed [ProviderSubagentBar](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/ProviderSubagentBar.tsx#L14) contains `Open parent` at line 73. It is byte-identical to the newer file. So is [threadSubagents](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/packages/client-runtime/src/state/threadSubagents.ts#L31). The installed [ChatView parent reference](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/ChatView.tsx#L824) resolves the parent's thread and environment reference.

These stable files support child-pill navigation and returning to a parent. They do not make delegation lifecycle behavior identical. Newer source changes delegated result retention, resumed children, process cleanup, and visibility of linked-environment tasks in trimmed shell snapshots. A normal local child route check does not exercise those cases.

### Sol and Codex live command output

The installed [Codex outputDelta handler](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/orchestration-v2/Adapters/CodexAdapterV2.ts#L4553) handles `item/commandExecution/outputDelta`. It finds an active or settled turn context and emits `command_output.delta` with the item ID and chunk. The installed [ProviderSessionManager branch](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/orchestration-v2/ProviderSessionManager.ts#L2397) sends that event directly to `commandOutput.append`. [CommandOutputHub append](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/server/src/orchestration-v2/CommandOutputHub.ts#L145) maintains output tails and serves coalesced append frames.

The installed [V2ItemInspector](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/V2ItemInspector.tsx#L268) mounts [CommandOutputPanel](https://github.com/pingdotgg/t3code/blob/1264c206aee0dce805d6279809118bc091f29d2f/apps/web/src/components/chat/CommandOutputPanel.tsx#L26) for expanded command rows. The panel subscribes to `orchestrationEnvironment.v2.commandOutput` and follows the output until the reader scrolls away. The capability check is at line 16. The hub, `applyCommandOutputFrame` client reducer, inspector, panel, and Codex tool presentation are byte-identical across the compared revisions. The whole Codex adapter and session manager differ elsewhere.

The parent reported actual Sol command output through line 25 before completion. That observation is consistent with this installed source chain. This worker did not rerun it. Cursor's later streaming fix and Claude's later paragraph streaming fix must remain provider-specific notes.

### Shell, browser, and PR boundaries

`ShellCommandBlock.tsx` is unchanged, but `ThreadTerminalDrawer.tsx` has newer focus and keyboard handling. A matching command button does not establish terminal keyboard parity. `ShellStream.ts` concerns lightweight thread sidebar snapshots, not shell command stdout. Its newer linked-environment parent retention should be cited for task visibility only.

`ServerBrowserSurface.tsx` is unchanged, while `BrowserSession.ts` adds certificate approval persistence and the `Trust and Remember` prompt only in newer source. A browser opening an ordinary page on installed T3 does not validate the newer local HTTPS trust flow. Other later commits fix named browser profile persistence and browser snapshots after reconnect.

`usePullRequestLinking.ts` and `openPullRequestLink.ts` are unchanged. The installed `PullRequestWatchReactor.ts` already contains PR watching. The newer source modifies check baselines, queued workflow handling, watch wake aggregation, and check-suite reactions. `apps/server/src/pullRequest/PullRequestWatchService.ts` exists in the pinned checkout but returns 404 at the installed revision and is absent from the complete installed tree listing. Its absence is a module boundary change, not evidence that installed T3 lacks PR watches.

## Material differences for live checks

The following commits all occur after the installed revision and before or at the pinned comparison revision. Exact file diffs are included in the JSON for inspected paths. Conclusions about uninspected supporting paths use commit subjects and are not independent runtime verification.

| Area | Installed versus newer boundary | Later commit |
| --- | --- | --- |
| whole response reply | Whole-response hover Reply action and separate response context are absent at installed SHA. Passage citations and comments are already present. | [c0f85e7f](https://github.com/pingdotgg/t3code/commit/c0f85e7f7940d0243dc47f5ed8d48266302aeace) |
| cursor command streaming | Cursor command-output streaming fix is newer than installed app. Do not generalize a successful Codex command stream to Cursor. | [4e0b5280](https://github.com/pingdotgg/t3code/commit/4e0b52806be83e66f80839e1e372398307346425) |
| claude text streaming | Paragraph-level Claude assistant text delta handling is newer than installed app. This does not establish absence of all Claude updates. | [1a5d3737](https://github.com/pingdotgg/t3code/commit/1a5d3737b7ed60f5372752f0e2607f24c66122ef) |
| provider start label | Provider-specific Starting Claude label is newer. Installed ThinkingTimelineRow falls back to Thinking. | [2b7e975a](https://github.com/pingdotgg/t3code/commit/2b7e975a974f9184b4a212675dd45d3070a789a7) |
| claude background elapsed | Claude background-command elapsed display fix is newer. | [d4b77d24](https://github.com/pingdotgg/t3code/commit/d4b77d244879e2d8388ceadc44e41dd5f091cfcd) |
| local https trust | Local HTTPS certificate Trust and Remember handling in BrowserSession is newer. | [52273b7a](https://github.com/pingdotgg/t3code/commit/52273b7a04dbf482ea83f761479fe9c1573149c0) |
| dictation citation cancel | Read-only composer citation cancellation fix is newer. Ordinary editable citation comments predate it. | [1001bc71](https://github.com/pingdotgg/t3code/commit/1001bc71fcc80ba8dc32c60a0ae4bbe85efea44b) |
| composer paste | CRLF plain-text paste fix is newer. | [4e692475](https://github.com/pingdotgg/t3code/commit/4e6924753cac26b904b76f59a44cff026914a132) |
| composer shortcuts | App-bound Mod+B, Mod+I, Mod+E precedence over composer formatting is newer. | [6a03ef65](https://github.com/pingdotgg/t3code/commit/6a03ef6568da3f962714b1ea8f0de91a947c9014) |
| composer focus | Composer focus restoration when returning to thread is newer. | [05089062](https://github.com/pingdotgg/t3code/commit/05089062d4df32cc9a4f5969911122e9808ac0c5) |
| remote child shell visibility | Linked-environment task parents retained in trimmed sidebar shell snapshots is newer. | [d412871f](https://github.com/pingdotgg/t3code/commit/d412871f27a52709c5978c3f9b0a0433242c50f6) |
| delegation lifecycle | Delegated results across wake turns fix is newer. Basic child navigation predates it. | [7462f234](https://github.com/pingdotgg/t3code/commit/7462f234e89027943cf0d1a73d3ae1f36571d4e3) |
| named browser profiles | Named agent browser profiles surviving server restart fix is newer. | [30a26dbb](https://github.com/pingdotgg/t3code/commit/30a26dbb64f80f288f0b379061a254d4c574d29c) |
| pr watch baseline | Baseline existing PR checks when starting watch fix is newer. | [072254dd](https://github.com/pingdotgg/t3code/commit/072254ddd31dfc01b8ae9336a82d7439125e9ac0) |
| pr watch queued workflows | Waiting for queued GitHub workflow runs fix is newer. | [c501ccad](https://github.com/pingdotgg/t3code/commit/c501ccad573c567a85495b35598f8ec843bf8383) |
| pr watch check suite | GitHub watched PR reaction to completed check suite is newer. | [e0d6aadb](https://github.com/pingdotgg/t3code/commit/e0d6aadb88d229746be9503c510baaccbad84f4f) |
| pr watch combined wake | Coalescing multiple watched PR changes into one thread wake is newer. | [1de3c3c6](https://github.com/pingdotgg/t3code/commit/1de3c3c68c4b886c52ddee49cdb32c4e3150d39b) |

## Where identical source can be reused

Direct raw-source comparisons found 19 byte-identical files among 43 successfully compared paths. One further path is newer-only. This is a bounded selection, not a repository-wide count. The JSON records hashes, exact revision URLs, installed symbol line anchors, and unified diffs for every attempted path.

- `apps/web/src/lib/assistantCitationNavigation.ts`.
- `apps/web/src/components/chat/MarkdownFileLink.tsx`.
- `apps/web/src/components/chat/MarkdownFileLink.logic.ts`.
- `apps/web/src/components/chat/ShellCommandBlock.tsx`.
- `apps/web/src/browser/ServerBrowserSurface.tsx`.
- `apps/web/src/hooks/usePullRequestLinking.ts`.
- `apps/web/src/lib/openPullRequestLink.ts`.
- `apps/web/src/components/chat/AssistantCitationSource.tsx`.
- `apps/web/src/components/chat/useAssistantCitationTarget.ts`.
- `packages/shared/src/assistantCitations.ts`.
- `packages/contracts/src/assistantCitations.ts`.
- `apps/web/src/components/chat/ProviderSubagentBar.tsx`.
- `packages/client-runtime/src/state/threadSubagents.ts`.
- `apps/web/src/components/chat/TimelineRowParts.tsx`.
- `apps/server/src/provider/CodexToolPresentation.ts`.
- `apps/web/src/components/chat/CommandOutputPanel.tsx`.
- `apps/web/src/components/chat/V2ItemInspector.tsx`.
- `packages/client-runtime/src/state/commandOutput.ts`.
- `apps/server/src/orchestration-v2/CommandOutputHub.ts`.

## Gotchas and unresolved boundaries

- The installed app and pinned checkout are deliberately unchanged. No tests, builds, models, or apps were launched by this worker. No UI profiles, credentials, auth state, or conversation store were read.
- Source presence establishes an implementation path, not subscription entitlement, successful execution, or final UX. The parent's real sessions supply those observations.
- The JSON contains 746 unique commit records. GitHub's default unpaginated compare response returned only 250 commits. Its file list is capped at 300 files. This report avoids treating either truncated response as an exhaustive source delta.
- Raw public source was compared against the existing local pinned files. Git status in that checkout was empty at inspection. Neither HEAD nor tracked source files changed.
- Supporting code outside the inspected paths can affect behavior even when a listed file is identical. For example, an unchanged component can consume a changed contract or provider result.
- No byte-for-byte comparison to a downloaded release archive was performed. The app bundle fingerprint and source hash are recorded for later reproducibility.
- All source line numbers in the installed trace point to `1264c206aee0dce805d6279809118bc091f29d2f`. Line numbers in the newer local checkout differ and must not be copied into installed citations.

## Collection method

Read the app plist with `/usr/libexec/PlistBuddy`. Parse the ASAR header using Python `struct` and `json`, then read its internal package metadata at the recorded byte offset. Read the update feed. Resolve the embedded hash through the public GitHub commit and release APIs. Read the compare API with `per_page=100` and pages 1 through 8. Fetch each selected source file from `raw.githubusercontent.com/pingdotgg/t3code/1264c206aee0dce805d6279809118bc091f29d2f/...` and compare it with the local pinned file using Python `difflib` and SHA-256. The complete old tree API reported `truncated=false` with 36,659 entries and confirmed the older PR reactor module.

Only this Markdown report and its adjacent JSON were written. The JSON parsed successfully and its compared path counts match its file entries. Runtime testing is outside this provenance task.
