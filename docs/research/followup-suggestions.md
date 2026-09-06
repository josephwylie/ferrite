# Follow-up suggestions from providers

Researched 2026-09-06 against the two CLIs Ferrite ships with: Codex 0.153.4
and Claude Code 2.1.261. Protocol schemas and shipped binaries only; no model
requests were made.

Question: can Ferrite show, in the Composer's idle line, a follow-up prompt the
provider itself suggested after the last response?

## Neither provider offers one

**Codex.** `codex app-server generate-json-schema -o <dir>` emits the whole
app-server protocol. Nothing in it carries a suggested next user prompt.
`turn/completed`, `item/completed` and the thread notifications have no such
field. The only near-miss identifiers are unrelated: `CollabAgentTool` has a
`followupTask` variant, which is a *tool an agent calls to task another agent*,
and `installSuggestionPluginNames`, which is about plugins.

**Claude Code.** The stream-json output carries `assistant`, `user`,
`system:init`, `result` and control frames; none names a suggestion. A sweep of
the shipped binary's string table for `followup`, `follow_up`, `suggestedPrompt`,
`nextPrompt` and `promptSuggestion` returns only the artifact-comments
subsystem (`codeliveredFollowups`, "the thread follow-up …"), which is about
comment threads on artifacts, not about what the operator should type next.

## The adjacent native surface is the question tool, and Ferrite already has it

Both providers *do* have one way to hand the operator candidate next inputs, and
it is the question tool: Claude's `AskUserQuestion`, and Codex's
`tool/requestUserInput` (marked EXPERIMENTAL; `questions[].options[].label` /
`.description`). Ferrite normalizes both in `ferrite-core::questions` and shows
them as a Decision. That path needs no new work, and a Composer placeholder
would be the wrong surface for it — the Decision card already carries the
options, and its answer goes back over a different transport.

## What Ferrite does instead

`ferrite-core::followup` derives the suggestion from the transcript with no
inference call: no tokens, no latency, no new dependency. It fires only on
signals that are unambiguous in the text, and otherwise falls back to the
generic line — a confidently wrong placeholder costs the operator more than the
generic one it replaced. The rules, in order:

1. a pending Decision, or a closed Session — as before;
2. the response's last sentence is an offer ("want me to …?", "should I …?")
   with no free-standing "or" in it, which becomes an acceptance quoted back in
   the model's own words;
3. any other trailing question, which becomes an invitation to answer;
4. the model's own plan with steps outstanding;
5. otherwise the generic line.

A streaming Thread yields nothing but the generic line: the sentence a
suggestion would be read from is still half-written.

The renderer owns every sentence; `followup` names only what the Thread is
waiting on. It also draws the line while the Composer is focused and empty, not
only at rest — a suggestion the operator cannot read with the cursor in the box
is one they never see.
