#!/bin/sh
# Record the `claude` CLI's stream-json wire protocol into committed fixtures.
#
# The fixtures next to this script are the protocol contract: the replay tests
# in `tests/claude_session.rs` and `src/providers/claude/wire.rs` assert against
# them, so a vendor release that changes the wire breaks the build loudly. Re-run
# this after a CLI upgrade, diff the result, and fix the parser — never the
# other way round.
#
#   usage: ./capture.sh [scenario...]      (no arguments: every scenario)
#   env:   CLAUDE_BIN   path to the CLI      (default: the pinned 2.1.243)
#          FIXTURE_TAG  version suffix       (default: the CLI's own --version)
#
# Each scenario writes `claude-<scenario>-<tag>.jsonl` — every line the CLI
# printed on stdout — and, when the host had to talk back over the control
# protocol, `claude-<scenario>-<tag>.host.jsonl` with every line Ferrite wrote
# to the CLI's stdin. Both directions are recorded because the control protocol
# runs both ways: a replay harness that only knows one side cannot prove
# `respond_to_decision` or the initialize handshake.
#
# Privacy: these fixtures ship in a public repository, so the driver redacts
# the recording machine out of every line — the signed-in account's address,
# the home directory (which the CLI prints in plugin paths), and the capture's
# own scratch cwd. Everything else stays byte-for-byte what the CLI printed.
#
# Hygiene: captures run in a scratch cwd with `--safe-mode`, so the recording
# machine's hooks, MCP servers, plugins and CLAUDE.md stay out of the fixture.
# The `error` scenario additionally points CLAUDE_CONFIG_DIR at an empty
# directory, which is what makes the CLI fail: an isolated config has no
# credentials. That is also why the other scenarios do NOT isolate the config —
# doing so costs authentication, so they use the recording machine's real
# credentials and accept its slash-command list in the `system:init` line.
#
# The `hello` fixture predates this script and is captured differently (real
# config, hooks enabled). It is deliberately left alone: its hook chatter is the
# only committed evidence that unknown event types are ignored, not fatal.

set -eu

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CLAUDE_BIN=${CLAUDE_BIN:-$HOME/.local/share/claude/versions/2.1.243}
FIXTURE_TAG=${FIXTURE_TAG:-$("$CLAUDE_BIN" --version | cut -d' ' -f1)}
# Deliberately /tmp and not $TMPDIR: on macOS $TMPDIR is a per-user directory
# whose name identifies the recording account, and the model quotes its cwd
# back inside thinking deltas — where a path split across two deltas survives
# any amount of textual redaction. Recording somewhere anonymous is the only
# fix that holds.
WORK=/tmp/ferrite-capture-$$
mkdir -p "$WORK/cwd" "$WORK/emptyconfig"
trap 'rm -rf "$WORK"' EXIT

# Model: haiku everywhere, prompts kept to one sentence — a full re-capture must
# cost cents, or it stops being run.
MODEL=haiku

capture() {
    scenario=$1
    shift
    echo "==> $scenario"
    CAPTURE_OUT="$DIR/claude-$scenario-$FIXTURE_TAG" \
    CAPTURE_BIN="$CLAUDE_BIN" \
    CAPTURE_CWD="$WORK/cwd" \
    CAPTURE_MODEL="$MODEL" \
        python3 "$DIR/capture.py" "$@"
}

want() {
    [ $# -eq 0 ] && return 1
    case " $WANTED " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

WANTED=${*:-tool edit permission-allow permission-deny error initialize}

for scenario in $WANTED; do
    case $scenario in
    # (d) The capability handshake, alone: no prompt, no model call, no cost.
    initialize)
        capture initialize --initialize -- --safe-mode
        ;;
    # (a) Tool use end to end. --allowedTools scopes the one command so no
    # permission gate fires: this fixture is about tool_use/tool_result shapes.
    tool)
        capture tool \
            --prompt 'Run the shell command `echo ferrite-tool-ok` with Bash and tell me its output.' \
            -- --safe-mode --allowedTools 'Bash(echo *)'
        ;;
    # (e) An edit to a file that already exists. The permission-allow fixture
    # only proves a *create*, whose `structuredPatch` is empty by definition —
    # so it is the one capture that carries real patch hunks, and diff cards
    # are typed from them. Read is allowed alongside Edit because the CLI
    # refuses to edit a file this session has not read.
    edit)
        printf 'alpha\nbravo\ncharlie\n' >"$WORK/cwd/ferrite-edit.txt"
        capture edit \
            --prompt 'Use the Edit tool to change the word bravo to delta in ferrite-edit.txt, then say done.' \
            -- --safe-mode --allowedTools 'Read Edit'
        ;;
    # (b) A Decision. No --allowedTools, and --permission-prompt-tool stdio is
    # what makes the CLI ask over the control protocol instead of refusing
    # outright. Write always prompts in the default permission mode, whereas a
    # bare `echo` is pre-approved on most machines — hence the file, not a
    # command. --permission-mode default overrides a recording machine that
    # normally runs in bypassPermissions.
    permission-allow)
        rm -f "$WORK/cwd/ferrite-perm.txt"
        capture permission-allow --answer allow \
            --prompt 'Create a file named ferrite-perm.txt containing exactly the word ok, using the Write tool. Then say done.' \
            -- --safe-mode --permission-mode default --permission-prompt-tool stdio
        ;;
    permission-deny)
        rm -f "$WORK/cwd/ferrite-perm.txt"
        capture permission-deny --answer deny \
            --prompt 'Create a file named ferrite-perm.txt containing exactly the word ok, using the Write tool. Then say done.' \
            -- --safe-mode --permission-mode default --permission-prompt-tool stdio
        ;;
    # (c) A failing turn. An empty CLAUDE_CONFIG_DIR has no credentials, so the
    # CLI ends the turn with terminal_reason "api_error" — reliable, offline,
    # and free, unlike provoking a real API failure.
    # The subshell matters: `VAR=x func` leaks VAR into the calling shell for
    # a function, which would silently unauthenticate every later scenario.
    error)
        (
            CLAUDE_CONFIG_DIR="$WORK/emptyconfig"
            export CLAUDE_CONFIG_DIR
            capture error --prompt 'Say exactly: hi' -- --safe-mode
        )
        ;;
    *)
        echo "unknown scenario: $scenario" >&2
        exit 2
        ;;
    esac
done

echo "==> wrote fixtures tagged $FIXTURE_TAG into $DIR"
