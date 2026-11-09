# Conversation browser Provider adaptation

Both Provider adapters accept the same `browser::Operation` JSON through the
shared `browser::tools` definition and parser. A `ThreadClient` comes from the
trusted `SpawnRequest.thread`; tool arguments cannot supply ownership. Browser
capability does not depend on Visual Level. Missing windows or bound capability
produce an unavailable tool result without launching a browser.

Claude appends the tool to its existing Ferrite SDK MCP server alongside
wrap-up, plan and optional visual tools. Codex appends it to the existing
`ferrite` dynamic-tool namespace and preserves visual and plan configuration.
Both return screenshot bytes as native PNG image content, with textual browser
state separately. The shared contract owns bounded asynchronous completion.

## Codex native Thread compatibility

Verified against the installed official Codex CLI 0.167.0 using:

```sh
codex app-server generate-json-schema --experimental --out /tmp/ferrite-194-codex-schema
```

The generated `v2/ThreadStartParams.json` has `dynamicTools`.
`v2/ThreadResumeParams.json` and `v2/ThreadForkParams.json` do not. The official
[App Server documentation](https://learn.chatgpt.com/docs/app-server) describes
persisted dynamic tools restored on resume. The generated installed-version
schema is the compatibility boundary used here.

New native Threads receive the browser definition. Previously capable resumed
Threads retain their definition and the adapter answers their calls using the
current bound client. A Thread created before browser support may lack the tool.
Resume instructions explicitly state that limitation and direct the operator to
start a new Thread for browser work if the tool is absent. Ferrite preserves the
existing native conversation; it does not silently fork, replace or discard it.
This is an explicit compatibility limitation, not a seamless tool upgrade.

## Verification scope

`server::tests::browser_providers` drives the actual Claude and Codex Session
protocol adapters with scripted external CLIs, through bound public Thread
operations and the real Server/window protocol. It verifies Thread-owned opens
with Visuals off, native PNG response content, closed routing arguments, visual
and plan configuration coexistence, and resumed-call handling/guidance. The
window fixture supplies capture bytes; these tests do not establish actual CEF
pixel agreement or real native CLI persistence. Packaged displayed-page tests
and sustained workflows remain separate release acceptance evidence for #194.
