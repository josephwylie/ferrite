# Claude inline skill mentions (`skill_mention`)

Investigated 2026-10-20 against the installed **Claude Code 2.1.304**
(`~/.local/share/claude/versions/2.1.304`, a Bun-compiled Mach-O read with
`strings`) and the official **Agent SDK 0.3.285** package (`sdk.d.ts`,
`sdk.mjs`). Live probes used `claude -p --input-format stream-json
--output-format stream-json --verbose --model haiku`; scripts and captures stay
in `/tmp/ferrite-skill-probe/` and are not copied into the repo.

## Conclusion

**No stdin field, flag or env var makes the CLI attach `skill_mention` for a
user-only skill (`disable-model-invocation: true`) named mid-message.** The
terminal and stdin paths do share one architecture (`ovt` → `xn` → `Bn` →
`Wwe` → `$wn`). The one thing that differs is the `preExpansionInput` argument.
The terminal composer always passes it, and it is the only source of
`typed: true` for user-only skills. The headless stdin dispatcher never passes
it, and the stdin message schema has no field that becomes it.

What a host *can* get, proven by probe:

1. **`"origin":{"kind":"human"}`** on the stdin user line gives the same
   `skill_mention` as the terminal for every **model-invocable** skill.
   Without it, no skill gets a mention.
2. **User-only skills still run headless when the model calls them.** The Skill
   tool's own check (`Pe`) scans this turn's user text for `/name` and does not
   look at `preExpansionInput`. So "go ahead and /grill-with-docs" over stdin
   lets the model run the skill. Only the reminder telling the model to do so
   is missing. Probe p10 ran `mattpocock-skills:grill-with-docs` through the
   Skill tool.
3. **A leading `/skill args`** dispatches directly, as in the terminal. The docs
   state that stacked leading skills load only the first over SDK/`-p`.

The user's example would not match in the terminal either: "…any
/mattpocock-skills:grill-with-docs?" has `?` attached. Both paths need the name
to be a whitespace-delimited word (`Nwn`; probe p08; the
[skills docs](https://code.claude.com/docs/en/skills) say "as a separate word
with no punctuation attached").

## Mechanism (2.1.304 bundle, minified names)

### The collector

`Wwe` builds the turn's attachments. Its `skill_mentions` entry:

```js
$d("skill_mentions",()=>Promise.resolve(b?.isHumanTypedPrompt&&ve&&!Ele(n)&&!jy()
  &&n.options.tools.some((Et)=>At(Et,vo))&&Fwn()
  ?$wn(b.preExpansionInput??e,nee(n.options.commands),
       {typed:b.preExpansionInput!==void 0,pastedContents:b.pastedContents,permissionContext:ce(n)}):[]))
```

- `Fwn()` is the GrowthBook gate `k("tengu_humble_emerson",!0)`. `jy()` is
  `launchOptions.disableSlashCommands()`. The Skill tool (`vo`) must be in the
  tool list.
- **`typed` is exactly `preExpansionInput !== undefined`.**

### `$wn`: who gets a mention

```js
for(let{start:V,end:Q}of jUe(e)){ if(H&&e.slice(q,V).trim()===""){q=Q;continue}  // skip leading /cmd run
  if(H=!1,!Nwn(e,V,Q))continue;                                                    // whitespace-delimited only
  ... let we=(ve)=>J5e(ke,{commandName:_e,userTypedThisTurn:ve,isMainSession:!0,...});
  if(we(!0)!==null)Se=null; else{let ve=we(!1)?.reason;
    Se=ve==="disable_model_invocation"||ve==="override_disabled"}                   // Se = userOnly
  if(Se===null)continue;
  if(Se&&(!r.typed||eMr(V,Q,s)||h.some((we)=>we.includes(`/${_e}`))))continue;    // user-only gate
  y.set(ke,{skillName:_e,userOnly:Se}) }
```

- `jUe` finds `/[a-zA-Z][a-zA-Z0-9.:\-_]*` after start, whitespace or CJK
  punctuation, and trims trailing dots. `Nwn` then requires whitespace or the
  end of the text on **both** sides. So `/x?` and `/x.` do not match.
- A user-only skill is dropped when **not typed**, **or** when it lies inside a
  paste range (`eMr`), **or** when it appears inside pasted content. Paste ranges
  never bypass the typed check. They are a second exclusion. Model-invocable
  skills inside pastes still get a mention (probes p05, p06).

### `Bn`: `isHumanTypedPrompt`

```js
let be=oe!=="all"&&(Ue||!se?.startsWith("/")),Ke=oe===void 0,Be=_t&&!Q&&Z!==!1,Le=Be&&cf(J)&&!Ie;
... Wwe(..., {isRegularUserPrompt:Be,isHumanTypedPrompt:Le, ..., preExpansionInput:q,pastedContents:n, ...})
```

Mapped against `xn`'s call `Bn(e,o,n,qe,f,…,_e,bt,s,r,me,ke,…,ve===!0,…)`:
`_t` = mode is prompt, `Q` = isMeta, `Z` = shouldQuery, `J` = origin,
`Ie` = textNotTyped, `q` = `preExpansionInput`, `n` = pastedContents. So
`isHumanTypedPrompt` needs a human origin, no `text_not_typed`, not meta, and
`shouldQuery !== false`. Empirically, an absent origin fails it (p01). That is
everything `origin:{kind:"human"}` turns on. **`preExpansionInput` is a
separate argument.**

### Where `preExpansionInput` comes from

`ovt` (exported from the processUserInput chunk) spreads its argument into
`xn({input:e,preExpansionInput:s,…})`. It has exactly three call sites:

| Caller | Passes `preExpansionInput`? |
|---|---|
| Terminal queue drain: `ovt({input:Dr.value,preExpansionInput:Dr.preExpansionValue,promptSource:wr,…,pastedContents:Dr.pastedContents,…})` | **Yes.** The composer sets `preExpansionValue: h.inputSource==="suggestion_accepted"?void 0:to` (the raw typed line) on every submit, so terminal transcripts show `"promptSource":"typed"`. |
| Headless/SDK turn: `ovt({input:pt,mode:"prompt",…,querySource:"sdk",origin:je?.origin,skipSlashCommands:…,localStdinOrigin:…,sessionChannelPersonTyped:…,textNotTyped:…,…})` | **No.** It also passes no `pastedContents`. |
| Deferred slash `zn`: `ovt({input:e.text,mode:"prompt",…,origin:e.origin,…})` | No |

The headless per-turn options builder (`Sb`) carries no `preExpansionValue` and
no `pastedContents` either. Its fields include `skipSlashCommands`,
`bridgeOrigin`, `localStdinOrigin`, `sessionChannelPersonTyped`, `ownWordUuids`,
`hostInitiator`, `inboundOrigin`, `origin`, `clientPlatform`, `textNotTyped`,
`ownFilePaths` and `noPersonArrived`. The stdin frame builder sets
`clientPlatform`, `origin`, `skipSlashCommands` (for `client_composed`), and
spreads `oJt(H,…)` (`text_not_typed` → `{textNotTyped:!0,skipSlashCommands:!0}`)
and `MXr(…)` (→ `localStdinOrigin` / `skipAttachments`). None of these reach
`typed`.

The only other `preExpansionValue` writer is the terminal REPL's concurrent-turn
re-enqueue (`preExpansionValue:Uo?.preExpansionValue??Po.stackedOriginalInput`).
That is REPL host state, not reachable from stdin.

### Every candidate checked

| Candidate | Effect on user-only `skill_mention` |
|---|---|
| Stdin schema (`Ty`/`un` zod): `type, message, parent_tool_use_id, isSynthetic, tool_use_result, priority, origin, client_platform, inbound_origin, server_received_wall_ms, historical, shouldQuery, timestamp, is_meta, seeded_summon, client_composed, text_not_typed, initiator, is_visible_in_transcript_only, is_virtual, is_compact_summary, summarize_metadata, mcp_meta, tool_result_meta, source_tool_use_id, source_tool_assistant_uuid, image_paste_ids, plan_content, permission_mode, interrupted_message_id, refused_message_id, uuid, pasted_content, inline_pastes, session_id, agent_id, subagent_type, task_description, file_attachments` | None of them maps to `preExpansionInput`. |
| `origin:{kind:"human"}` | Turns on `isHumanTypedPrompt`. Model-invocable skills only. |
| `client_platform` (`claude_code_cli`/`claude_code_vscode` pass `tLn`; `ios`/`android`/`web_claude_ai`/`desktop_app` pass `jbe`) | Only shapes queued origin (`Suo`) and `localStdinOrigin` in `MXr`. No change (p03, p04). |
| `inbound_origin` | "Server-asserted… Not settable by clients"; also disables `localStdinOrigin`. |
| `pasted_content` / `inline_pastes` | Merged into the text by `jKe` (wrapped in `<pasted_content>`). Paste ranges *exclude* user-only skills (p05, p06). |
| `text_not_typed` | Documented: "a skill named in it does not count as typed by the user". Kills all mentions (p07) and Skill-tool typed credit (p13). |
| `client_composed` / SDK `verbatimPrompts` | Skips the whole turn-start attachment pass. |
| `localStdinOrigin`, `sessionChannelPersonTyped`, `ownWords` | Internal, derived. Feed `/ultraplan` and ownership checks, not `$wn`. |
| `stackedOriginalInput`, `promptSource`/`inputSource:"typed"` | Internal UserMessage / REPL-queue fields, absent from the stdin schema. |
| Agent SDK 0.3.285 | No `preExpansion`/typed field. Its only `"typed"` is a `forwardedIntent.source` label on `queued_command`. String prompts and `streamInput` write plain `SDKUserMessage`s. |

### The Skill tool's own gate (why execution still works)

```js
U=J5e(y,{commandName:…,userTypedThisTurn:!Ele(n)&&Pe(a,{agentId:n.agentId,messages:n.messages(),turnStartIndex:n.turnStartIndex})&&W,…})
function Pe(e,n){ if(n.agentId!==void 0)return!1; let s=nCs(e);
  for(…this turn's messages…){ if(d.type!=="user"||d.isMeta||d.isCompactSummary||!u_(d.origin)
      ||d.verifiedSlackHumanTurn===!0||d.handedOff===!0||d.textNotTyped===!0)continue; …
    if(s.test(Zy(d)??""))return!0 } return!1 }
function nCs(e){return new RegExp(`(?<!\\S)/${Ma(e)}(?=$|\\s)`)}
function u_(e){return e===void 0||e.kind==="human"}
```

`J5e` returns `disable_model_invocation` only when `!userTypedThisTurn`. So a
user-only skill runs if this turn's user text contains `/name` as a word and the
message is human or unattributed (not `text_not_typed`). This matches the docs'
"the name counts as your permission for that message".

## Probes

All used `--model haiku`. "Mention" means a `skill_mention` attachment in
`~/.claude/projects/*/<sid>.jsonl`. Text named both
`/mattpocock-skills:grill-with-docs` (user-only) and
`/mattpocock-skills:grilling` (model-invocable) unless noted.

| # | Stdin line (beyond `type`/`message`) | Result |
|---|---|---|
| p01 | none | No mentions. `turnOrigin:"sdk"` |
| p02 | `origin:{kind:"human"}` | Mention: grilling only |
| p03 | + `client_platform:"claude_code_cli"` | Mention: grilling only |
| p04 | + `client_platform:"claude_code_vscode"` | Mention: grilling only |
| p05 | human, both names only in `pasted_content` | Mention: grilling only |
| p06 | human, names in text **and** `inline_pastes` | Mention: grilling only |
| p07 | human + `text_not_typed:true` | No mentions |
| p08 | human, `…/mattpocock-skills:grilling? Or /mattpocock-skills:grilling.` | No mentions (punctuation attached) |
| p09 | human, `message.content` as a plain string | Mention: grilling only |
| p10 | human, `bypassPermissions`. Text asks the model to call Skill with grill-with-docs and contains `/mattpocock-skills:grill-with-docs` | Skill ran: `Launching skill: mattpocock-skills:grill-with-docs` |
| p11 | as p10, **no origin** | Skill ran (`u_` accepts an absent origin) |
| p12 | human, asks for the Skill call but the text has no `/name` | `disable-model-invocation` error |
| p13 | as p10 + `text_not_typed:true` | `disable-model-invocation` error |

The reference terminal transcript
(`~/.claude/projects/-Users-josephwylie/cac85aaf-….jsonl`, line 108) is a user
row with `"origin":{"kind":"human"},"promptSource":"typed"`, followed by
`{"type":"skill_mention","skillName":"mattpocock-skills:grill-with-docs"}`.
Every stdin row above has `"promptSource":"sdk"`.

## Recommendation for Ferrite

Stamp the human origin on both writers in `providers/claude.rs`. Nothing else
in the shape changes:

```json
{"type":"user","uuid":"<client id>","isAsync":true,
 "origin":{"kind":"human"},
 "message":{"role":"user","content":[{"type":"text","text":"…"}]}}
```

`send` writes the same line without `uuid`/`isAsync`. The CLI's own schema
description asks for exactly this: "A host wrapping keyboard input must stamp
{kind:'human'} explicitly — absent origin is treated as unattributed and fails
closed at strict isHuman() trust gates."

This yields terminal-identical mentions for model-invocable skills. For
user-only skills, Ferrite gets the terminal's *permission* (p10) but not its
*reminder*. Full parity would need the CLI to accept a typed marker on stdin.
That is CLI-side work (feedback to Anthropic). The only host-side stand-in is
to add the reminder text itself, and it is not identical: it lands in the user
message, not as an attachment, and Ferrite would have to know which skills are
user-only. I would not build that (YAGNI) unless the gap bites in practice.

### What `origin:{kind:"human"}` also turns on

- Turn bookkeeping: `turnOrigin:"human"`, a human `turnPosition` (promptIndex
  advances), and "own words" tracking (`Zkn({ownWords})`).
- Attachments gated on `isHumanTypedPrompt`: `skill_mentions`, and
  `workflow_keyword_request` (keyword reminders read from the text). The
  terminal-only `peer_mentions` still needs `preExpansionInput`.
- `isHuman()` trust gates that fail closed on an absent origin now pass. That
  is the documented intent for keyboard input.
- Leading `/skill` dispatch and the Skill tool's typed credit behave the same
  either way (p11).
- Never stamp it on text the person did not type (restored drafts, generated
  prompts). Use `text_not_typed:true` or `client_composed:true` for those.
