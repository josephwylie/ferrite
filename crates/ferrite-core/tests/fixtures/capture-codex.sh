#!/bin/sh
# Record the `codex app-server` JSON-RPC wire protocol into committed fixtures.
#
# The fixtures next to this script are the protocol contract: the replay tests
# in `tests/codex_session.rs` and `src/providers/codex/wire.rs` assert against
# them, so a vendor release that changes the wire breaks the build loudly.
# Re-run this after a CLI upgrade, diff the result, and fix the parser — never
# the other way round.
#
#   usage: ./capture-codex.sh [scenario...]   (no arguments: every scenario)
#   env:   CODEX_BIN    path to the CLI       (default: codex)
#          FIXTURE_TAG  version suffix        (default: the CLI's own --version)
#
# Each scenario writes `codex-<scenario>-<tag>.jsonl` — every line the server
# printed on stdout — and `codex-<scenario>-<tag>.host.jsonl` with every line
# Ferrite wrote to its stdin. Both directions are recorded because JSON-RPC
# runs both ways: approvals are server requests that block the turn until the
# host answers them.
#
# Privacy: these fixtures ship in a public repository, so the driver redacts
# the recording machine out of every line — the home directory (the server
# reports $CODEX_HOME and rollout paths under it), the capture's scratch cwd,
# the machine's hostname and installation id (the remoteControl notification
# carries both), and any signed-in email/organization. Everything else stays
# byte-for-byte what the server printed.
#
# Hygiene: captures run in a scratch cwd, and every scenario overrides the
# recording machine's `notify` hook (which would launch a desktop app per
# turn). Unlike Claude's --safe-mode there is no one flag that isolates the
# rest of the config; instead each scenario pins its own approval policy,
# sandbox, model and reasoning effort explicitly, so the recording machine's
# config.toml cannot leak a posture into a fixture. The server's bundled MCP
# servers still announce themselves in the stream — kept, because those lines
# are the committed evidence that unknown methods are ignored, not fatal.
#
# The `error` scenario points CODEX_HOME at an empty directory, which is what
# makes the turn fail: an isolated home has no credentials, so the API refuses
# the model call (observed: five retries, a final error, a failed turn). The
# other scenarios use the machine's real credentials.
#
# Deliberately /tmp and not $TMPDIR: on macOS $TMPDIR names the recording
# account, and the model can quote its cwd back split across deltas, where a
# path survives any textual redaction. Recording somewhere anonymous is the
# only fix that holds.

set -eu

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CODEX_BIN=${CODEX_BIN:-codex}
# `codex --version` prints `codex-cli 0.149.1`: the version is the last token.
FIXTURE_TAG=${FIXTURE_TAG:-$("$CODEX_BIN" --version | awk '{print $NF}')}
WORK=/tmp/ferrite-codex-capture-$$
mkdir -p "$WORK/cwd" "$WORK/emptyhome"
trap 'rm -rf "$WORK"' EXIT

# Model: the cheapest listed model everywhere, prompts kept to one sentence —
# a full re-capture must cost cents, or it stops being run.
MODEL=gpt-5.4-mini

capture() {
    scenario=$1
    shift
    echo "==> $scenario"
    CAPTURE_OUT="$DIR/codex-$scenario-$FIXTURE_TAG" \
    CAPTURE_BIN="$CODEX_BIN" \
    CAPTURE_CWD="$WORK/cwd" \
    CAPTURE_MODEL="$MODEL" \
        python3 "$DIR/capture_codex.py" "$@"
}

WANTED=${*:-initialize hello tool approval-allow approval-deny approval-patch interrupt resume error}

for scenario in $WANTED; do
    case $scenario in
    # (a) The handshake alone: initialize, initialized, thread/start. No turn,
    # no model call, no cost — this is the capability fixture.
    initialize)
        capture initialize --approval-policy on-request --sandbox workspace-write \
            -- -c 'notify=[]'
        ;;
    # (b) A plain text turn that also carries reasoning summaries. The default
    # summary setting produced none on this model, so the scenario pins
    # `detailed` + `high` — the arithmetic detour is what makes the model emit
    # a summary at all, and the reply keeps the fixture's text assertable.
    hello)
        capture hello --approval-policy never --sandbox read-only \
            --turn 'Work out 17*23 in your head, then reply with exactly this and nothing else: hello ferrite' \
            -- -c 'notify=[]' -c 'model_reasoning_summary=detailed' -c 'model_reasoning_effort=high'
        ;;
    # (c) Tool use end to end, no approval gate: a workspace-write sandbox
    # runs a harmless echo without asking. This fixture is about the
    # commandExecution item lifecycle.
    tool)
        capture tool --approval-policy never --sandbox workspace-write \
            --turn 'Run the shell command `echo ferrite-tool-ok` and tell me its output.' \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    # (d) A Decision. A read-only sandbox plus on-request approvals makes the
    # file write ask the host: item/commandExecution/requestApproval blocks the
    # turn until the host's decision lands.
    approval-allow)
        rm -f "$WORK/cwd/ferrite-perm.txt"
        capture approval-allow --answer allow \
            --approval-policy on-request --sandbox read-only \
            --turn 'Create a file named ferrite-perm.txt containing exactly the word ok. Then say done.' \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    approval-deny)
        rm -f "$WORK/cwd/ferrite-perm.txt"
        capture approval-deny --answer deny \
            --approval-policy on-request --sandbox read-only \
            --turn 'Create a file named ferrite-perm.txt containing exactly the word ok. Then say done.' \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    # (d') The other Decision shape: steering the model to apply_patch makes
    # the same gate arrive as item/fileChange/requestApproval, whose params
    # carry no command — the changes live on the fileChange item it blocks.
    approval-patch)
        rm -f "$WORK/cwd/ferrite-patch.txt"
        capture approval-patch --answer allow \
            --approval-policy on-request --sandbox read-only \
            --turn 'Use the apply_patch tool (not shell) to create a file named ferrite-patch.txt containing exactly the word ok. Then say done.' \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    # (e) Interrupting a streaming turn: turn/interrupt answers with an empty
    # result and the turn completes with status "interrupted".
    interrupt)
        capture interrupt --interrupt \
            --approval-policy never --sandbox read-only \
            --turn 'Count slowly from 1 to 200, one number per line. Do not stop early.' \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    # (f) Resume across processes: an unrecorded setup server plants a
    # codeword and exits; the recorded server thread/resume-s the same thread
    # from its rollout file and the model answers from history.
    resume)
        capture resume --resume \
            --approval-policy never --sandbox read-only \
            -- -c 'notify=[]' -c 'model_reasoning_effort=low'
        ;;
    # (g) A failing turn. An empty CODEX_HOME has no credentials, so the API
    # refuses the model call — reliable and free. The subshell keeps the
    # override out of every later scenario.
    error)
        (
            CODEX_HOME="$WORK/emptyhome"
            export CODEX_HOME
            capture error --approval-policy never --sandbox read-only \
                --turn 'Say exactly: hi' -- -c 'notify=[]'
        )
        ;;
    *)
        echo "unknown scenario: $scenario" >&2
        exit 2
        ;;
    esac
done

echo "==> wrote fixtures tagged $FIXTURE_TAG into $DIR"
