# Agent live-output runtime correction

This correction verifies the user journey through rendered controls. It found **four broken behaviors**, alongside six working steps and four explicitly unverified areas. A subagent screen exists and can update, but adjacent controls lead to a different, empty output surface; nested-agent controls are dead; reopening produces a blank tab; and ordinary child prose is cut off at the default panel width. That is an integration-quality failure, not a missing-feature checkbox.

The original [subagents/background source report](subagents-background.md) remains useful for additional hypotheses. This report upgrades only the cases actually exercised. It does not imply all nine original findings were reproduced.

## Environment and fidelity

- Ferrite source: `62947f91d5cd6ee494aaae9ed09d794a853e5bab`.
- Existing warm headless binary SHA-256: `8f47873505f48c90a267ef5b02913a345b72e390d17b78cd3a72c552497a2b98`. No new build or application edit.
- Fresh isolated Store: `/tmp/ferrite-audit-runtime-q_vpcgd9/threads`; one local fixture thread.
- Real GPUI rendering at 1440 × 900 logical pixels (2880 × 1800 PNG, scale 2).
- Public `ferrite window click`, `window key`, `window find`, and `window shot` commands drive actual drawn elements. No direct invocation of UI event handlers or injection into the UI model.
- A local fake Claude CLI emits controlled protocol JSON through Ferrite's normal provider process/adapter, engine, event transport, and renderer. The Agent/task/child-message shapes are based on the repository's captured Claude subagent fixture. Stage gates keep each live state open until the next release. Full inbound and outbound protocol records are preserved.
- Fake CLI version 9999 wins discovery; fake Codex refuses non-version commands. No real model or network prompt was sent.

This is a faithful check of the adapter-to-rendered-control path for these supported frames. It is not native installed-app verification or validation of every real provider's event sequence. Claude's current adapter explicitly limits `stream_event` to Main (`crates/ferrite-core/src/providers/claude/activity.rs:342`); child live checks therefore use supported forwarded child assistant messages while the task remains Working.

**Fixture caveat:** the synthetic Main stream omitted message lifecycle frames and then supplied a final assistant snapshot with a new message ID. The later screenshots contain a duplicated Main sentence. That is excluded from defects here; it needs a valid captured lifecycle replay to assess. Main incremental visibility before that snapshot is still directly observed.

## What breaks

### agent-live-04 — “View live output” shows “No output yet” despite visible child output

**P1, runtime reproduced.** Click Alpha's row label and its child transcript opens correctly. Release its next output: `ALPHA_LIVE_TWO` appears. Click the separate **View live output** text on exactly that same Main row: the selected surface becomes Background, with `Running 1`, `Alpha live fixture`, and **No output yet**.

Evidence: [child already has output](../evidence/agent-live-correction/06-agent-open-live-two-nested.png), then [explicit live-output link opens an empty output card](../evidence/agent-live-correction/09-main-live-output-click.png).

The route is explicit in `crates/ferrite-pane/src/transcript.rs:349`: every background call's link emits `OpenBackground`, including Agent calls. The Background card reads the Main `ToolBlock.output`, not the child's transcript (`crates/ferrite-cockpit/src/cockpit/panel/background.rs:264`). The two controls therefore disagree about where an agent's output lives. This directly explains a way the user can ask for agent live output and get sent to Background instead.

Acceptance: both agent inspection affordances must reveal the same child's actual latest output. If Background retains an agent card, it must link to or render that child and never label known output as absent. This extends the source report's Main-only Background finding; deduplicate in the aggregate.

### agent-live-05 — nested agent actions are inert

**P1, runtime reproduced.** Alpha spawns Grandchild through real protocol frames. Its row appears inside Alpha with Running status and **View live output**. Clicking the row at its measured bounds, then clicking the explicit live-output text, causes no navigation and no error. Alpha remains selected; Grandchild's emitted live output cannot be reached through this normal inspection path.

Evidence: [nested row exists](../evidence/agent-live-correction/06-agent-open-live-two-nested.png), [row click](../evidence/agent-live-correction/07-nested-row-click.png), [live-output click](../evidence/agent-live-correction/08-nested-live-output-click.png). Row bounds: x1038, y186, width384, height46 logical pixels. Explicit text click: x1341, y209.

Cause: compact transcript inputs provide empty agent mappings (`crates/ferrite-cockpit/src/cockpit/side_chat.rs:527` and `:564`), and the subscribed handler returns for every event except ToggleDisclosure (`crates/ferrite-cockpit/src/cockpit/agent_pane.rs:127`). This confirms the normal-drill part of `subagents-background-02`, not every alternate ancestry route.

Acceptance: Main → Alpha → Grandchild works through each displayed opener; Grandchild's own progress is visible; parent and root navigation remain available. The same flow must work after completion.

### agent-live-06 — reopening a closed Agent tab restores an empty panel

**P1, runtime reproduced.** Close Alpha's tab, use the actual command palette to choose **Reopen closed surface**, and the Alpha tab returns selected with a wholly blank body.

Evidence: [palette command](../evidence/agent-live-correction/11-reopen-palette.png), [blank reopened tab](../evidence/agent-live-correction/12-reopened-agent-blank.png). The public drawn-element check fails reproducibly:

```text
window find agent-drill
exit 1: nothing drawn as `agent-drill`
window find panel-tab-Agent
found x1220,y10,width126,height24
```

Closing releases the Drill (`crates/ferrite-cockpit/src/cockpit/panel.rs:290`, `:318`; `agent_pane.rs:113`). Reopen only restores the Surface ID (`palette.rs:1294`), and the body returns an empty div without its Drill (`agent_pane.rs:203`). Clicking the original Main row reconstructs the missing resource and recovers the transcript. Runtime confirmation of `subagents-background-03`.

Acceptance: reopen restores tab identity and resource/content together, including retained history and current live updates. Check Close all as well as individual close.

### agent-live-07 — child prose is cut off at normal panel width

**P1, visual/runtime reproduced.** At the ordinary 420px panel, `ALPHA_LIVE_TWO: second progress arrived while this tab is open.` displays only through `is`; the final `open.` disappears beyond the right edge. `ALPHA_LIVE_THREE` is also cut through its final word. Clicking Maximize changes the panel to 960px and reveals both complete sentences. The text exists; the ordinary view fails to wrap it into the visible area.

Evidence: [420px completed view with missing endings](../evidence/agent-live-correction/13-completed-agent.png) versus [960px view with complete endings](../evidence/agent-live-correction/14-agent-maximized-output.png). `window find agent-drill` measured normal x1020,y44,w420,h855 and maximized x480,y44,w960,h855.

The route is `agent_pane.rs:203` → compact TranscriptView → answer and list-column layout (`crates/ferrite-pane/src/transcript.rs:1527`, `:2955`). The exact text-measurement/layout root cause was not isolated; do not present those locations as a proven fix. This is an additional visible defect, independent of whether output events arrive.

Acceptance: ordinary prose wraps at the actual panel width with no missing characters. Verify open, maximize, restore and resize without requiring a new output event.

## Working and unverified states

| Step | Observed result | Evidence |
| --- | --- | --- |
| Main incremental stream | `MAIN_LIVE_ONE`, then `MAIN_LIVE_TWO` appears before completion | [first](../evidence/agent-live-correction/02-main-first-live-clear.png), [second](../evidence/agent-live-correction/03-main-second-live.png) |
| Child creation and direct row opening | Alpha Running row opens the correct child transcript | [created](../evidence/agent-live-correction/04-agent-created.png), [opened](../evidence/agent-live-correction/05-agent-open-live-one.png) |
| Open child receives updates | Second and third forwarded messages arrive without reopening/refresh | [second](../evidence/agent-live-correction/06-agent-open-live-two-nested.png), [third](../evidence/agent-live-correction/10-agent-open-live-three.png) |
| Completion | Grandchild row becomes Completed; Alpha's final text and Main Done appear | [completed](../evidence/agent-live-correction/13-completed-agent.png) |
| Blank reopen recovery | Clicking Main's Alpha row reconstructs the transcript | [recovered/completed](../evidence/agent-live-correction/13-completed-agent.png) |
| Return to Main | Restore and collapse panel restores Main transcript/composer | [Main](../evidence/agent-live-correction/15-return-main.png) |
| Eviction/restart history | Not exercised; only two retained children | Source hypotheses remain unverified at runtime |
| Real provider sessions | Not exercised | No paid model/network prompts |
| Native focus/motion/CEF | Not exercised by this headless run | Parent owns installed-app verification |
| Child token deltas / Grandchild transcript UI | Child token deltas are outside this Claude adapter contract; nested UI blocked Grandchild inspection | Do not infer that all live streaming is absent |

T3 relationship-navigation comparison remains pinned in the [original report](subagents-background.md): `ThreadRelationshipsControl.tsx` and `ProviderSubagentBar.tsx` at commit `74573fc29b901bafbf8f1dbb2eb29606da1fa448`. Blank reopen, false no-output state, and clipping are internal correctness defects and do not require a T3 equivalent to establish.

## Reproduction and evidence

- [Machine-readable cases](../evidence/agent-live-correction/cases.json)
- [All public commands and return codes](../evidence/agent-live-correction/commands.jsonl)
- [Controlled provider](../evidence/agent-live-correction/provider-peer.py)
- [Provider-emitted frames with timestamps](../evidence/agent-live-correction/provider-emitted.jsonl)
- [Host-to-provider frames](../evidence/agent-live-correction/provider-host.jsonl)
- [Driver and process isolation](../evidence/agent-live-correction/drive.py)

The driver records every public command. Gates 1 through 5 are advanced by writing the integer to `evidence/agent-live-correction/stage`; the provider records `waiting-stage`. Each screenshot is captured after a public UI operation settles. This permits independent reproduction with a new isolated Store; the existing state file intentionally prevents overwriting a running fixture. No application tests were run because this was a runtime audit with no app diff. No Rust build occurred.

## Cleanup

The fixture thread was closed. Server PID 52664, headless PID 52998, and fake-provider PIDs 52985/55588 are absent; no members remain in either tracked process group. [cleanup.json](../evidence/agent-live-correction/cleanup.json) records the process-table check. The temporary Store is retained only as inert audit data. No installed GUI was opened or modified by this worker.
