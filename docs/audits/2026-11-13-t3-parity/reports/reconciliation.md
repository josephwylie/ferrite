# Surface lifecycle reconciliation

Parent reconciliation note: this source slice retains its original priorities and evidence labels. The [consolidated inventory](../findings.md) applies subsequent review, deduplication, and scoped runtime results; [adjudications](../adjudications.json) record each override.

The user wants reliable complete workflows, including recovery after closing a panel tab. This supplement resolves the independent review's question about whether the Agent blank-reopen defect also affects SideChat and File tabs.

Ferrite source is `62947f91d5cd6ee494aaae9ed09d794a853e5bab`. This is a direct source inspection. No application code, tests, live UI, or user state changed. The parent owns runtime verification and the consolidated tally.

## reconciliation-001. Reopen closed tab restores an empty SideChat tab

Priority P2. Category defect. Evidence source-proven.

Closing a SideChat discards the local tab record and retires its fork, but the panel offers Reopen closed tab for that identity. Reopening restores the outer tab without the content needed to render it. The result is a source-defined empty body, not a recovered conversation.

Trigger. Open a SideChat, close its outer tab, and invoke Reopen closed tab in the same owning thread.

Expected. Reopen restores the same side conversation and a usable view, or the application clearly excludes this terminally released resource from its reopen operation. A revived shell must never imply that conversation restoration succeeded.

Actual. `SurfaceId::reopenable` excludes only Terminal and Browser. `Command::ReopenSurface` calls only `reopen_closed` and notifies. `close_side_tab` removes the SideChat record and retires its fork. `side_chat_body` returns an empty `div` when that record is absent. `tend_panel` does not hydrate SideChat records.

Ferrite evidence:

- `crates/ferrite-cockpit/src/cockpit/surfaces.rs:48-53` admits SideChat to the closed-tab history.
- `crates/ferrite-cockpit/src/cockpit/panel.rs:290-293,314-319` closes the identity and releases the resource.
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:308-324` removes the tab, removes pending fork bookkeeping, and retires its conversation.
- `crates/ferrite-cockpit/src/cockpit/palette.rs:1294-1298` restores only the panel identity.
- `crates/ferrite-cockpit/src/cockpit/panel.rs:384-390` hydrates Files, Changes, and PR only in this dispatch.
- `crates/ferrite-cockpit/src/cockpit/side_chat.rs:607-615` renders an empty body for the missing tab.

This defect does not require a T3 parity claim. The contradiction is between Ferrite's own offered reopen operation and its resource lifetime.

Recommendation. Define reopen policy alongside the resource's close and restore operations. Prefer restoring the same conversation when its durable identity exists. If the product intentionally destroys side conversations on close, mark this kind non-reopenable and distinguish that destructive action from hiding its tab. Do not create a new fork silently when the user requests the old one.

Acceptance. Through the rendered panel and production commands, create a side conversation with a unique marker, close it, and reopen it. The same identity and marker return with working controls, or the deliberately unsupported action is unavailable with clear lifecycle copy. Repeating close/reopen must not create duplicate provider work. Include close while the fork is starting and a failed restore, which must show a recoverable error rather than a blank body.

## File previews do not share this finding

File previews have a separate hydration path. `sync_previews` resolves the active File identity, restores a cached document, or reopens the path from disk. See `crates/ferrite-cockpit/src/cockpit/panel/preview.rs:298-319`. Therefore removing a preview resource on close does not by itself prove blank File reopen. This distinction does not establish successful runtime file loading or address Files' separate inner-tab persistence defects.

The existing Agent finding remains `subagents-background-03`. SideChat and Agent have distinct resources but share a missing lifecycle contract. The parent may group them under that architectural root while retaining their separate observable acceptance checks.
