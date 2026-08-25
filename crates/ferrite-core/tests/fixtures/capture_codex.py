"""Drive one `codex app-server` JSON-RPC session and record both directions.

Called by capture-codex.sh, which documents the scenarios; this file is only
the driver. It mirrors capture.py (the Claude driver): the protocol runs both
ways — answering an approval request means reading the server's stdout and
writing to its stdin while the turn is still running.

  (no mode flag)    initialize + thread/start only: the capability handshake
  --turn TEXT       start a thread and send one user turn, stop at turn/completed
  --answer allow    answer any approval request by accepting it
  --answer always   ... or by accepting with the execpolicy amendment the
                    server offered, so the same command should not ask twice
  --answer deny     ... or by declining it
  --interrupt       send turn/interrupt after the first agent-message delta
  --resume          two phases: a setup turn that plants a codeword, then a
                    fresh server process that thread/resume-s the same thread
                    and asks for the codeword back. Only phase two is recorded:
                    the fixture is the resume, not the setup.
  --approval-policy X, --sandbox Y   thread/start parameters
  -- ARGS...        extra arguments for `codex app-server` (config overrides)

The request-id sequence is deliberately the one CodexSession uses — 1 for
initialize, 2 for thread/start or thread/resume, 3 for the turn — so a replay
stub can `cat` a fixture and the recorded responses answer the session's own
requests.
"""

import json
import os
import re
import socket
import subprocess
import sys
import threading
import time

OUT = os.environ["CAPTURE_OUT"]
BIN = os.environ["CAPTURE_BIN"]
CWD = os.environ["CAPTURE_CWD"]
MODEL = os.environ["CAPTURE_MODEL"]

TIMEOUT = 300

# These fixtures ship in a public repository, and the app-server is chatty
# about the machine it ran on: the initialize response carries $CODEX_HOME,
# thread objects carry rollout paths under the home directory, and the
# remoteControl notification names the machine and its installation id.
# Redaction is textual so the rest of every line stays byte-for-byte what the
# server printed. The masks match the Claude fixtures' conventions.
IDENTIFYING = re.compile(r'"(email|organization)":"[^"]*"')
INSTALLATION = re.compile(r'"installationId":"[0-9a-f-]{36}"')
HOSTS = {socket.gethostname(), socket.gethostname().split(".")[0] + ".local"}
PATHS = [(path, mask)
         for directory, mask in ((CWD, "/workspace"), (os.path.expanduser("~"), "/home/operator"))
         for path in {directory, os.path.realpath(directory)}]


def redact(line):
    for path, mask in PATHS:
        line = line.replace(path, mask)
    for host in HOSTS:
        line = line.replace(host, "operator.local")
    line = INSTALLATION.sub('"installationId":"00000000-0000-0000-0000-000000000000"', line)
    return IDENTIFYING.sub(r'"\1":"operator@example.invalid"', line)


def parse_args(argv):
    opts = {"turn": None, "answer": None, "interrupt": False, "resume": False,
            "approval_policy": None, "sandbox": None, "extra": []}
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--turn":
            i += 1
            opts["turn"] = argv[i]
        elif arg == "--answer":
            i += 1
            opts["answer"] = argv[i]
        elif arg == "--interrupt":
            opts["interrupt"] = True
        elif arg == "--resume":
            opts["resume"] = True
        elif arg == "--approval-policy":
            i += 1
            opts["approval_policy"] = argv[i]
        elif arg == "--sandbox":
            i += 1
            opts["sandbox"] = argv[i]
        elif arg == "--":
            opts["extra"] = argv[i + 1:]
            break
        else:
            sys.exit("capture_codex.py: unknown argument %s" % arg)
        i += 1
    return opts


opts = parse_args(sys.argv[1:])


class Server(object):
    """One `codex app-server` process with both directions logged."""

    def __init__(self, extra, cli_log, host_log):
        self.cli_log = cli_log
        self.host_log = host_log
        self.lines = []
        self.lock = threading.Lock()
        self.p = subprocess.Popen(
            [BIN, "app-server"] + extra,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, bufsize=1, cwd=CWD)
        threading.Thread(target=self._reader, daemon=True).start()

    def _reader(self):
        for line in self.p.stdout:
            if self.cli_log:
                self.cli_log.write(redact(line))
                self.cli_log.flush()
            try:
                value = json.loads(line)
            except ValueError:
                continue
            with self.lock:
                self.lines.append(value)

    def send(self, obj):
        text = json.dumps(obj)
        if self.host_log:
            self.host_log.write(redact(text) + "\n")
            self.host_log.flush()
        self.p.stdin.write(text + "\n")
        self.p.stdin.flush()

    def drain(self, handle):
        """Feed every line printed since last time to `handle`.

        Returns when `handle` says the scenario is finished, or when the
        server exits — a fixture of a server that died is still a fixture.
        """
        deadline = time.time() + TIMEOUT
        seen = 0
        while time.time() < deadline:
            with self.lock:
                fresh, seen = self.lines[seen:], len(self.lines)
            for value in fresh:
                if handle(value):
                    return True
            if self.p.poll() is not None and seen == len(self.lines):
                return False
            time.sleep(0.05)
        sys.exit("capture_codex.py: scenario did not finish within %ds" % TIMEOUT)

    def request(self, req_id, method, params):
        self.send({"jsonrpc": "2.0", "id": req_id, "method": method, "params": params})
        box = {}

        def take(value):
            if value.get("id") == req_id and ("result" in value or "error" in value):
                box["response"] = value
                return True
            return False

        self.drain(take)
        return box.get("response")

    def close(self):
        self.p.stdin.close()
        try:
            self.p.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.p.kill()


def handshake(server, start_method, start_params):
    """The session's own opening: initialize, initialized, thread/start."""
    server.request(1, "initialize", {
        "clientInfo": {"name": "ferrite", "version": "0.1.0"}})
    server.send({"jsonrpc": "2.0", "method": "initialized"})
    response = server.request(2, start_method, start_params)
    if response is None or "error" in response:
        sys.exit("capture_codex.py: %s failed: %r" % (start_method, response))
    return response["result"]["thread"]["id"]


def thread_params(extra_params=None):
    params = {"cwd": CWD, "model": MODEL}
    if opts["approval_policy"]:
        params["approvalPolicy"] = opts["approval_policy"]
    if opts["sandbox"]:
        params["sandbox"] = opts["sandbox"]
    if extra_params:
        params.update(extra_params)
    return params


def run_turn(server, thread_id, prompt):
    """One user turn, answering approvals and interrupting per the flags."""
    server.send({"jsonrpc": "2.0", "id": 3, "method": "turn/start", "params": {
        "threadId": thread_id,
        "input": [{"type": "text", "text": prompt}]}})
    state = {"turn_id": None, "interrupted": False, "next_id": 4}

    def handle(value):
        method = value.get("method")
        if method == "turn/started":
            state["turn_id"] = value["params"]["turn"]["id"]
        if method in ("item/commandExecution/requestApproval",
                      "item/fileChange/requestApproval") and opts["answer"]:
            decision = "accept" if opts["answer"] == "allow" else "decline"
            if opts["answer"] == "always":
                # Echo back the standing answer the server offered, rather
                # than a bare accept: the amendment is what makes the next
                # identical command run unasked.
                offered = (value.get("params") or {}).get("availableDecisions") or []
                standing = next((d for d in offered if isinstance(d, dict)), None)
                decision = standing if standing is not None else "accept"
            server.send({"jsonrpc": "2.0", "id": value["id"],
                         "result": {"decision": decision}})
        if (method == "item/agentMessage/delta" and opts["interrupt"]
                and not state["interrupted"] and state["turn_id"]):
            state["interrupted"] = True
            server.send({"jsonrpc": "2.0", "id": state["next_id"],
                         "method": "turn/interrupt", "params": {
                             "threadId": thread_id, "turnId": state["turn_id"]}})
        return method == "turn/completed"

    server.drain(handle)
    # The stragglers that follow a completed turn (token usage arrives around
    # it); a bounded grace keeps them in the fixture without hanging on a quiet
    # stream.
    time.sleep(1.0)


cli_log = open(OUT + ".jsonl", "w")
host_log = open(OUT + ".host.jsonl", "w")

if opts["resume"]:
    # Phase one is scaffolding, not fixture: an unrecorded server plants a
    # codeword in a thread and exits, leaving its rollout on disk.
    setup = Server(opts["extra"], None, None)
    thread_id = handshake(setup, "thread/start", thread_params())
    saved_answer, opts["answer"] = opts["answer"], None
    run_turn(setup, thread_id,
             "Remember the codeword: ferrite-resume-ok. Reply with exactly: saved")
    opts["answer"] = saved_answer
    setup.close()

    server = Server(opts["extra"], cli_log, host_log)
    handshake(server, "thread/resume", thread_params({"threadId": thread_id}))
    run_turn(server, thread_id,
             "What is the codeword? Reply with the codeword only.")
    server.close()
else:
    server = Server(opts["extra"], cli_log, host_log)
    thread_id = handshake(server, "thread/start", thread_params())
    if opts["turn"]:
        run_turn(server, thread_id, opts["turn"])
    server.close()

print("    %s.jsonl" % os.path.basename(OUT))
