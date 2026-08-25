"""Drive one `claude` stream-json Session and record both directions.

Called by capture.sh, which documents the scenarios; this file is only the
driver. It exists in Python rather than in the shell because the control
protocol runs both ways — answering a permission request means reading the
CLI's stdout and writing to its stdin while the turn is still running, which
`sh` cannot do without a fifo dance that would obscure the wire shapes this
script is here to record.

  --initialize      send the initialize control request and stop at its answer
  --prompt TEXT     send one user turn and stop at its result line
  --answer allow    answer any can_use_tool request by allowing the tool
  --answer deny     ... or by denying it with a message
  -- ARGS...        extra arguments for the CLI
"""

import json
import os
import re
import subprocess
import sys
import threading
import time

OUT = os.environ["CAPTURE_OUT"]
BIN = os.environ["CAPTURE_BIN"]
CWD = os.environ["CAPTURE_CWD"]
MODEL = os.environ["CAPTURE_MODEL"]

DENY_MESSAGE = "Ferrite operator denied this tool"
TIMEOUT = 300

# These fixtures ship in a public repository, and the CLI is chatty about the
# machine it ran on: the initialize response names the signed-in account, and
# `system:init` lists plugin paths under the recording user's home directory.
# Redaction is textual so the rest of every line stays byte-for-byte what the
# CLI printed — a fixture that had been through a JSON round-trip would no
# longer be evidence of what the wire looks like.
IDENTIFYING = re.compile(r'"(email|organization)":"[^"]*"')
# Both spellings of each directory: the CLI reports paths resolved, so on macOS
# a scratch dir handed in as /var/... comes back as /private/var/....
PATHS = [(path, mask)
         for directory, mask in ((CWD, "/workspace"), (os.path.expanduser("~"), "/home/operator"))
         for path in {directory, os.path.realpath(directory)}]


def redact(line):
    for path, mask in PATHS:
        line = line.replace(path, mask)
    return IDENTIFYING.sub(r'"\1":"operator@example.invalid"', line)


def parse_args(argv):
    opts = {"initialize": False, "prompt": None, "answer": None, "extra": []}
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--initialize":
            opts["initialize"] = True
        elif arg == "--prompt":
            i += 1
            opts["prompt"] = argv[i]
        elif arg == "--answer":
            i += 1
            opts["answer"] = argv[i]
        elif arg == "--":
            opts["extra"] = argv[i + 1:]
            break
        else:
            sys.exit(f"capture.py: unknown argument {arg}")
        i += 1
    return opts


opts = parse_args(sys.argv[1:])

args = [BIN, "-p", "--input-format", "stream-json", "--output-format", "stream-json",
        "--include-partial-messages", "--verbose", "--model", MODEL] + opts["extra"]

cli_log = open(OUT + ".jsonl", "w")
host_log = open(OUT + ".host.jsonl", "w")
p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.PIPE, text=True, bufsize=1, cwd=CWD)

lines = []
lock = threading.Lock()


def send(obj):
    text = json.dumps(obj)
    host_log.write(redact(text) + "\n")
    host_log.flush()
    p.stdin.write(text + "\n")
    p.stdin.flush()


def reader():
    for line in p.stdout:
        cli_log.write(redact(line))
        cli_log.flush()
        try:
            value = json.loads(line)
        except ValueError:
            continue
        with lock:
            lines.append(value)


threading.Thread(target=reader, daemon=True).start()


def drain(handle):
    """Feed every line the CLI has printed since last time to `handle`.

    Returns as soon as `handle` says the scenario is finished, or when the CLI
    exits — a fixture of a CLI that died is still a fixture, and hanging here
    would be worse.
    """
    deadline = time.time() + TIMEOUT
    seen = 0
    while time.time() < deadline:
        with lock:
            fresh, seen = lines[seen:], len(lines)
        for value in fresh:
            if handle(value):
                return True
        if p.poll() is not None and seen == len(lines):
            return False
        time.sleep(0.05)
    sys.exit("capture.py: scenario did not finish within %ds" % TIMEOUT)


if opts["initialize"]:
    send({"type": "control_request", "request_id": "req_1",
          "request": {"subtype": "initialize"}})
    drain(lambda v: v.get("type") == "control_response")
else:
    send({"type": "user", "message": {"role": "user", "content": [
        {"type": "text", "text": opts["prompt"]}]}})

    def handle(value):
        if value.get("type") == "result":
            return True
        if value.get("type") != "control_request":
            return False
        request = value.get("request", {})
        if request.get("subtype") != "can_use_tool" or not opts["answer"]:
            return False
        if opts["answer"] == "allow":
            body = {"behavior": "allow", "updatedInput": request.get("input", {})}
        else:
            body = {"behavior": "deny", "message": DENY_MESSAGE}
        send({"type": "control_response", "response": {
            "subtype": "success", "request_id": value["request_id"],
            "response": body}})
        return False

    drain(handle)

p.stdin.close()
try:
    p.wait(timeout=20)
except subprocess.TimeoutExpired:
    p.kill()

if os.path.getsize(OUT + ".host.jsonl") == 0:
    os.remove(OUT + ".host.jsonl")
print("    %s.jsonl" % os.path.basename(OUT))
