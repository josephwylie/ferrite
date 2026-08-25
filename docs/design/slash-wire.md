# Slash-command & @-mention wire study (ferrite#23)

Live-probed 2026-08-25 against the installed CLIs, in a throwaway scratch cwd
(`…/scratchpad/slash-probe/work`) containing a project command
(`.claude/commands/pingpong.md`), project skills
(`.claude/skills/probe-skill/`, `.codex/skills/probe-codex-skill/`,
`.codex/skills/probe-body/`), and `notes.txt` containing `MAGIC-WORD: zanzibar77`.

- claude **2.1.245** (Ferrite pin window 2.1.224 ≤ v < 3.0.0 — in range), spawned with
  Ferrite's exact args: `-p --input-format stream-json --output-format stream-json
  --include-partial-messages --verbose --permission-prompt-tool stdio`
  (probe variants added `--replay-user-messages`, `--tools ""`).
- codex **0.149.1**, `codex app-server` JSON-RPC over stdio, Ferrite's handshake
  (initialize → initialized → thread/start).

Raw captures: `…/scratchpad/slash-probe/cap1-init.txt` … `cap7-edges.txt`
(+ `codex-schema/` from `codex app-server generate-json-schema --out`).

---

## 1. Claude Code: enumeration WORKS over the wire

The `initialize` control_request Ferrite **already sends at spawn** answers with the
full command menu. `providers/claude.rs` even notes it ("the response also carries the
CLI's slash commands… which nothing reads yet") — confirmed, and it is richer than the
comment suggests. cap1, abridged but verbatim fields:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"req_1","response":{
  "commands":[
    {"name":"probe-skill","description":"Probe skill; reply MARKER-SKILL-LOADED when invoked (project)","argumentHint":""},
    {"name":"pingpong","description":"Probe command that makes the reply deterministic (project)","argumentHint":"anything"},
    {"name":"clear","description":"Start a new session with empty context; previous session stays on disk (resumable with /resume)","argumentHint":"[name]","aliases":["reset","new"]},
    {"name":"compact","description":"Free up context by summarizing the conversation so far","argumentHint":"<optional custom summarization instructions>"},
    {"name":"code-review","description":"Review the current diff…","argumentHint":"[low|medium|high|…]","aliases":["review"]}
  ],
  "...":"56 commands total; other keys: account, agents, analytics_disabled, available_output_styles, current_permission_mode, fast_mode_state, models, output_style, pid, session_state, …"}}}
```

- Entry shape: `{name, description, argumentHint, aliases?}`. Sources are mixed into one
  list: built-ins (compact/clear/context/model/usage/mcp/init/…), user+project **skills**,
  project **commands** (`.claude/commands/*.md`), and **plugin** commands
  (`impeccable:impeccable` with `aliases:["impeccable"]`).
- The list is the CLI's **effective** config, not a disk scan: this machine has 41
  user skills on disk but 37 are `"off"` under `skillOverrides` in
  `~/.claude/settings.json` — exactly those are absent from `commands`. A disk-scan
  fallback would over-report; don't build one.
- Names-only duplicate: every turn's `system:init` line carries
  `"slash_commands":["firecrawl","…","pingpong",…]` (cap2) — useful for refresh but the
  handshake list has the descriptions/hints a menu needs.
- No `claude commands list` subcommand exists (checked `--help`); the wire is the only
  structured source. `/reload-skills` exists as a command if Ferrite ever needs a
  mid-session rescan.

## 2. Claude Code: sending "/name args" as user text INVOKES the command

All sent as ordinary `{"type":"user","message":{content:[{type:"text",text:"/…"}]}}` lines.

- `/pingpong hello` → `{"type":"result","subtype":"success","result":"PONG-FROM-COMMAND hello"}` (cap2).
  With `--replay-user-messages` the CLI shows what it did to the text (cap3):
  ```json
  {"type":"user","message":{"role":"user","content":"<command-message>pingpong</command-message>\n<command-name>/pingpong</command-name>\n<command-args>again</command-args>"},…,"isReplay":true}
  ```
  and the session transcript shows the command body injected as a follow-up `isMeta`
  user message. Skills invoke identically: `/probe-skill` → `"MARKER-SKILL-LOADED"` reply.
- **Unknown** command short-circuits **locally, no model call**: `/definitely-not-a-command foo` →
  synthetic assistant message + result in ~4 ms:
  ```json
  {"type":"assistant","message":{…,"model":"<synthetic>",…"content":[{"type":"text","text":"Unknown command: /definitely-not-a-command"}]…}}
  {"type":"result","is_error":false,"duration_api_ms":0,"num_turns":0,…}
  ```
  (`is_error:false` — benign, session continues.)
- Matching is **case-sensitive** (`/PINGPONG` → "Unknown command: /PINGPONG", cap7) and
  only applies to a leading token that parses as a command name: `"/etc/hosts is a path.
  Say ok."` was NOT intercepted — replayed verbatim, model answered "Ok." (cap7). So an
  interior `/` stops command parsing; plain prose starting with a path is safe.
- Built-ins verified over stream-json (cap2):
  - `/context` → local, `duration_api_ms:0`, result = the markdown context report.
  - `/compact` → `system:status {"status":"compacting"}` → `system:compact_boundary
    {"compact_metadata":{"trigger":"manual","pre_tokens":19147,"post_tokens":2609,…}}` →
    summary replay + result.
  - `/clear` → `{"type":"conversation_reset","new_conversation_id":"1e783b7a-…"}` +
    `SessionStart:clear` hook + fresh `system:init`; the SAME process keeps serving turns
    afterwards ("say ok" → "ok"). Ferrite could get "new thread, same Session" from this.

## 3. Claude Code: `@path` is NOT plain text — the CLI attaches the file itself

`"What is the magic word in @notes.txt? Reply with only the word."` sent with
`--tools ""` (every built-in tool disabled; init `tools` list shows only MCP tools):
answer `"zanzibar77"` in 2.2 s, **zero tool_use blocks** (cap3). The session transcript
shows what the wire hides — the CLI parsed the mention and injected the content:

```json
{"type":"attachment","attachment":{"type":"file","filename":"…/work/notes.txt",
 "content":{"type":"text","file":{"filePath":"…/notes.txt","content":"MAGIC-WORD: zanzibar77\n","numLines":2,…}},
 "displayPath":"notes.txt"}}
```

- The attachment never appears on the stream-json output; the replayed user message keeps
  the literal `@notes.txt` text. Resolution is against the process cwd (the Thread's
  workspace binding), relative paths fine.
- Nonexistent `@nope-does-not-exist.txt` → harmless, treated as plain text, turn succeeds (cap7).
- Consequence: Ferrite's composer only needs client-side path completion; inserting
  `@rel/path` into the text buys real attachment semantics for free.

## 4. Codex app-server: enumeration WORKS (`skills/list`), plus server-side file search

Method inventory from `codex app-server generate-json-schema --out` (95 client
requests): relevant ones are `skills/list`, `fuzzyFileSearch`, `plugin/skill/read`,
`skills/config/write`, `skills/extraRoots/set`, `model/list`, `thread/compact/start`,
`turn/steer`. There is no customPrompts method in 0.149.x — skills are the mechanism
(`~/.codex/prompts` legacy is gone; this machine's is empty).

- `skills/list` (params `{cwds?: string[], forceReload?: bool}`) → per-cwd entries with
  `{name, description, shortDescription?, path, scope: user|repo|system|admin, enabled,
  interface?: {displayName, brandColor, icons…}}`. Live (cap4):
  ```json
  {"id":3,"result":{"data":[{"cwd":"…/slash-probe/work","skills":[
    {"name":"probe-codex-skill","description":"Probe skill for wire test; reply MARKER-CODEX-SKILL when invoked","path":"…/work/.codex/skills/probe-codex-skill/SKILL.md","scope":"repo","enabled":true},
    {"name":"browser:control-in-app-browser","description":"Control the in-app Browser…","interface":{"displayName":"Browser",…}}, …]}]}}
  ```
- `fuzzyFileSearch` `{query, roots[], cancellationToken?}` → ranked matches **with match
  indices for highlighting** (cap4):
  ```json
  {"id":4,"result":{"files":[{"root":"…/work","path":"notes.txt","match_type":"file","file_name":"notes.txt","score":109,"indices":[0,1,2,3]}]}}
  ```
  This is the IDE extension's @-menu backend; Ferrite can use it instead of building its
  own walker for Codex threads (or for both, if it wants one implementation, client-side).

## 5. Codex: slash text is NOT intercepted; typed input items carry skills/mentions

- `turn/start` with text `"/definitely-not-a-command foo"` → goes straight to the model
  as a `userMessage` item (`text_elements:[]`, unchanged); ~54 s later the **model**
  (after a reasoning item) replies `"Unknown command: `/definitely-not-a-command`."`
  (cap5, thread C). No server interception, no error — the TUI's slash menu is purely
  client-side. A Ferrite Codex composer must dispatch skills itself.
- `UserInput` variants (v2 schema): `text` (+`text_elements` byte-range spans),
  `image`/`localImage`/`audio`/`localAudio`, **`{"type":"skill","name","path"}`**,
  **`{"type":"mention","name","path"}`**.
- **Skill item works end-to-end**: `[{"type":"skill","name":"probe-body","path":"…/SKILL.md"},
  {"type":"text","text":"follow the skill"}]` → model ran
  `/bin/zsh -lc 'cat .codex/skills/probe-body/SKILL.md'` then answered the marker that
  exists ONLY in the body:
  ```json
  {"method":"item/completed","params":{"item":{"type":"agentMessage",…,"text":"MARKER-BODY-XYZZY","phase":"final_answer"…}
  ```
  (cap6, thread E). So the item is a *pointer* — server does not expand the body; the
  model reads the file (needs a sandbox that permits reads; probes used
  `sandbox:"read-only"`, `approvalPolicy:"never"`).
- **Skill descriptions are ambient** in every turn's context: a fresh thread asked to
  "list the names of any skills you can currently see" enumerated all of them, including
  both scratch repo skills, with no skill item ever sent (cap6, thread D:
  `"imagegen\nopenai-docs\nprobe-body\nprobe-codex-skill\nbrowser:control-in-app-browser…"`).
  (This explains cap5's stray "MARKER-CODEX-SKILL" answer — the marker sat in a skill
  *description*; no cross-thread contamination on re-check.)
- **Mention item does NOT inject content**: alone in a fresh thread it produced an answer
  with no file read and no knowledge of `zanzibar77` (cap5, thread B); when the model
  wants the content it reads it itself (cap4: `sed -n '1,120p' notes.txt` then
  `"zanzibar77"`). Plain-text `@notes.txt` behaves the same — literal text, model reads
  via shell. So for Codex, `mention`/`text_elements` are UI/persistence decoration, and
  file access rides on the sandbox.
- Operational note (cap4): `turn/start` while a turn is active does not error — inputs
  queue into the running turn as additional userMessage items.

## 6. Recommended v1 mechanism for Ferrite's Composer

**Claude threads**
1. Populate the `/` menu from the initialize response Ferrite already receives:
   parse `commands[] {name, description, argumentHint, aliases}` in
   `claude/wire.rs::parse_capabilities` alongside `models`. No new request needed.
   (`system:init.slash_commands` can refresh names mid-session if ever wanted.)
2. On submit, send the text unchanged — the CLI does dispatch, expansion, and even the
   unknown-command error (benign local result). No client-side execution logic.
3. `@` menu: client-side completion over the Thread's workspace binding; insert
   `@rel/path` literally — the CLI attaches file content itself. No wire attachment API
   exists or is needed.

**Codex threads**
1. Populate the `/` (skills) menu from `skills/list` (cwd of the Thread; re-call on
   `skills/changed` notification, `forceReload` on demand).
2. On "/skill args" submit, translate to
   `input:[{"type":"skill","name","path"},{"type":"text","text":args}]` — never send raw
   slash text expecting dispatch (the model just sees prose). Show only wire-backed
   entries; there are no built-in slash equivalents to fake (compact = `thread/compact/start`
   request, interrupt = `turn/interrupt`, etc. — Session methods, not message text).
3. `@` menu: either the same client-side completer, or `fuzzyFileSearch` for
   server-ranked results; append a `{"type":"mention"}` item (decoration + persistence)
   and rely on the model/sandbox for actual reading.

**Shared:** the menu is provider-fed either way, so the Composer needs one UI and two
small adapters; nothing static to maintain except perhaps pinning which Claude built-ins
Ferrite surfaces prominently (`compact`, `clear`, `context` verified working; everything
else in `commands[]` is offered as-is and fails soft).

---

### Capture index (all under `…/scratchpad/slash-probe/`)
| file | what |
|---|---|
| cap1-init.txt | claude initialize handshake, full `commands` list |
| cap2-turns.txt | /pingpong, /probe-skill, @notes.txt, /context, /compact, /clear, post-clear turn |
| cap3-replay.txt | `--replay-user-messages --tools ""`: @ with no tools, unknown cmd, expansion replay |
| cap7-edges.txt | "/etc/hosts …" passthrough, missing @file, case sensitivity |
| cap4-codex.txt | codex initialize, thread/start, skills/list, fuzzyFileSearch, queueing |
| cap5-codex-clean.txt | codex per-thread: mention-only, slash-text-only |
| cap6-codex-decisive.txt | codex ambient-skill-descriptions proof; body-marker skill invocation |
| codex-schema/ | `generate-json-schema` output (ClientRequest/ServerNotification/UserInput/…) |

Claude transcripts (attachment evidence): `~/.claude/projects/-private-tmp-…-slash-probe-work/*.jsonl`.
