#!/usr/bin/env python3
"""Drive Ferrite's native window against an isolated, real localhost SSH server.

prepare and ssh-smoke need no Ferrite build. After the final build, launch,
run, shot and flow use the production headless Driver. stop only signals PIDs
whose start identity still matches this run. Provider responses are fixtures.

verify runs the three-size flow. files-recapture reuses one stopped fixture
for a rendering change; recovery checks file/terminal failure and recovery
surfaces. summarize checks owned-process cleanup and preserves phase metadata.
"""

import argparse
import base64
import getpass
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time


REPO = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = REPO / "docs/audits/2026-11-16-remote-connections/evidence/native"


def write(path, content, mode=0o600):
    path.write_text(content)
    path.chmod(mode)


def save(output, state):
    write(output / "state.json", json.dumps(state, indent=2) + "\n")


def record(output, kind, **values):
    with (output / "commands.jsonl").open("a") as log:
        log.write(json.dumps({"time": time.time(), "kind": kind, **values}) + "\n")


def environment(state, remote=False, size="1440x900"):
    env = {key: value for key, value in os.environ.items() if not key.startswith("FERRITE_")}
    env.pop("SSH_AUTH_SOCK", None)
    if not remote:
        for key in ("SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"):
            env.pop(key, None)
    env.update(FERRITE_STORE=state["remote_store" if remote else "store"],
               FERRITE_SSH_HOSTS_DIR=state["root"] + "/ssh-hosts",
               FERRITE_HEADLESS_SIZE=size, FERRITE_LOOPS_OVERLAY="0", TERM="dumb",
               PATH=state["root"] + "/bin:" + env.get("PATH", ""))
    return env


def command(output, argv, *, env=None, cwd=None, timeout=35, check=True):
    result = subprocess.run([str(part) for part in argv], env=env, cwd=cwd,
                            text=True, capture_output=True, timeout=timeout)
    record(output, "command", argv=[str(part) for part in argv],
           returncode=result.returncode, stdout=result.stdout, stderr=result.stderr)
    if check and result.returncode:
        raise RuntimeError(f"{argv[0]} exited {result.returncode}\n{result.stderr}\n{result.stdout}")
    return result


def identity(pid):
    result = subprocess.run(["ps", "-p", str(pid), "-o", "lstart="],
                            capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else None


def launch_process(output, state, name, argv, *, env=None, cwd=None):
    with (output / f"{name}.log").open("a") as log:
        process = subprocess.Popen([str(part) for part in argv], env=env, cwd=cwd,
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    entry = {"name": name, "pid": process.pid, "identity": identity(process.pid)}
    state["processes"].append(entry)
    save(output, state)
    return process


def stop_processes(output, state, names=None):
    for process in reversed(state["processes"]):
        if process.get("stopped") or (names is not None and process["name"] not in names):
            continue
        if identity(process["pid"]) == process["identity"]:
            os.killpg(process["pid"], signal.SIGTERM)
            process["termination_sent"] = True
            deadline = time.monotonic() + 3
            while identity(process["pid"]) and time.monotonic() < deadline:
                time.sleep(.05)
        process["stopped"] = True
    for control in (Path(state["store"]).parent / "machines").glob("*/c"):
        command(output, [Path(state["root"]) / "bin/ssh", "-S", control,
                         "-O", "exit", "ferrite-audit"], timeout=5, check=False)
    save(output, state)


def prepare(output, binary, server):
    output.mkdir(parents=True, exist_ok=True)
    if (output / "state.json").exists():
        raise RuntimeError("This evidence directory already owns a run. Use a new --output.")
    root = Path(tempfile.mkdtemp(prefix="ferrite-remote-", dir="/tmp")).resolve()
    for name in ("bin", "client", "remote", "remote-project", "runtime", "ssh-hosts"):
        (root / name).mkdir(mode=0o700)
    state = {"root": str(root), "store": str(root / "client/threads"),
             "remote_store": str(root / "remote/threads"), "binary": str(binary),
             "server": str(server), "processes": [], "ssh_transport": "actual OpenSSH localhost",
             "filesystem": "localhost SSH fixture shares the host filesystem; SSH request logs prove routing",
             "credentials": "fixture-passphrase is a synthetic, task-only test credential",
             "provider": "simulated Codex app-server, no model API calls"}
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        state["port"] = sock.getsockname()[1]
    for name, passphrase in (("host", ""), ("client-key", ""), ("auth-key", "fixture-passphrase")):
        command(output, ["/usr/bin/ssh-keygen", "-q", "-t", "ed25519", "-N", passphrase,
                         "-f", root / name])
    write(output / "ssh_authorized_keys", (root / "client-key.pub").read_text() +
          (root / "auth-key.pub").read_text())
    host_key = (root / "host.pub").read_text().split()
    write(root / "known_hosts", f"[127.0.0.1]:{state['port']} {host_key[0]} {host_key[1]}\n")
    config = []
    for alias, key in (("ferrite-audit", "client-key"), ("ferrite-auth", "auth-key"),
                       ("ferrite-hostkey", "client-key")):
        known_hosts = root / ("new_known_hosts" if alias == "ferrite-hostkey" else "known_hosts")
        strict = "ask" if alias == "ferrite-hostkey" else "yes"
        config.append(f"""Host {alias}
    HostName 127.0.0.1
    Port {state['port']}
    User {getpass.getuser()}
    IdentityFile {root / key}
    IdentitiesOnly yes
    IdentityAgent none
    UserKnownHostsFile {known_hosts}
    GlobalKnownHostsFile /dev/null
    StrictHostKeyChecking {strict}
    ConnectTimeout 3
""")
    config.append("Host ferrite-unreachable\n    HostName 127.0.0.1\n    Port 1\n"
                  "    ConnectTimeout 5\n"
                  "    ProxyCommand /bin/sh -c 'sleep 2; exec /usr/bin/nc %h %p'\n")
    write(root / "ssh_config", "\n".join(config))
    write(root / "ssh-hosts/config", "\n".join(config))
    write(root / "ssh-hosts/known_hosts", (root / "known_hosts").read_text())
    write(root / "bin/ssh", "#!/bin/sh\nexec /usr/bin/ssh -F " +
          shlex.quote(str(root / "ssh_config")) + ' "$@"\n', 0o700)
    write(root / "bin/fixture-shell", "#!/bin/sh\nexec /bin/sh -i\n", 0o700)
    remote_entry = " ".join(shlex.quote(str(value)) for value in
                            (sys.executable, Path(__file__).resolve(), "--output", output, "remote-entry"))
    proxy = " ".join(shlex.quote(str(value)) for value in
                     (sys.executable, Path(__file__).resolve(), "--output", output, "bridge-proxy"))
    write(root / "bin/ferrited", "#!/bin/sh\nexec " + proxy + ' "$@"\n', 0o700)
    write(root / "sshd_config", f"""Port {state['port']}
ListenAddress 127.0.0.1
HostKey {root / 'host'}
PidFile {root / 'sshd.pid'}
AuthorizedKeysFile {output / 'ssh_authorized_keys'}
StrictModes yes
PasswordAuthentication no
KbdInteractiveAuthentication no
UsePAM no
PermitRootLogin no
AllowUsers {getpass.getuser()}
AllowAgentForwarding no
AllowTcpForwarding no
PermitTunnel no
X11Forwarding no
ForceCommand {remote_entry}
LogLevel VERBOSE
""")
    settings = {"check_for_updates": False, "start_at_login": False, "auto_update_clis": False,
                "default_provider": "codex", "machines": []}
    for directory in ("client", "remote"):
        write(root / directory / "settings.json", json.dumps(settings))
    write(root / "remote-project/REMOTE_ONLY.md", "# Remote fixture\n\nThis file belongs to the SSH server.\n")
    write(root / "remote-project/REMOTE_EDIT.rs", "// Remote Files fixture before edit.\n")
    for provider in ("codex", "claude"):
        if provider == "codex":
            invocation = " ".join(shlex.quote(str(value)) for value in
                                  (sys.executable, Path(__file__).resolve(), "--output", output, "provider"))
            body = "#!/bin/sh\nexec " + invocation + ' "$@"\n'
        else:
            body = ('#!/bin/sh\nif [ "$1" = "--version" ]; then echo "999999.0.0 (Claude Code)"; exit 0; fi\n'
                    'echo "Anthropic is disabled in this audit" >&2\nexit 126\n')
        write(root / "bin" / provider, body, 0o700)
    save(output, state)
    print(json.dumps(state, indent=2))


def ssh_smoke(output, state):
    root = Path(state["root"])
    command(output, ["/usr/sbin/sshd", "-t", "-f", root / "sshd_config"])
    process = launch_process(output, state, "sshd", ["/usr/sbin/sshd", "-D", "-e", "-f", root / "sshd_config"])
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError((output / "sshd.log").read_text())
        try:
            with socket.create_connection(("127.0.0.1", state["port"]), timeout=.2):
                break
        except OSError:
            time.sleep(.1)
    result = command(output, [root / "bin/ssh", "-o", "BatchMode=yes", "ferrite-audit", "fixture-smoke"])
    assert result.stdout.strip() == "isolated-real-sshd-ok", result.stdout
    state["ssh_smoke"] = {"passed": True, "nonroot": os.geteuid() != 0,
                          "strict_modes": True, "pid": process.pid}
    save(output, state)
    print(json.dumps(state["ssh_smoke"]))


def remote_entry(output, state):
    request = os.environ.get("SSH_ORIGINAL_COMMAND", "")
    record(output, "remote-command", command=request)
    if request == "fixture-smoke":
        print("isolated-real-sshd-ok")
        return
    if request == "true":
        return
    words = shlex.split(request)
    if words in (["ferrited", "bridge"], ["ferrited", "api-bridge"]):
        bridge_proxy(output, state, words[1])
        return
    if len(words) == 3 and words[:2] == ["sh", "-c"]:
        terminal = words[2].startswith("cd ")
        record(output, "ssh-terminal" if terminal else "preinstalled-runtime", transport="actual SSH",
               runtime="fixture PATH ferrited", original=request,
               ssh_connection=os.environ.get("SSH_CONNECTION"))
        env = environment(state, remote=True)
        if terminal:
            env.update(SHELL=state["root"] + "/bin/fixture-shell", ENV="/dev/null", BASH_ENV="/dev/null")
        elif (Path(state["root"]) / "inject-setup-warning").exists():
            print("FERRITE_WARNING Synthetic audit warning: verify provider sign-in on the remote machine.",
                  file=sys.stderr, flush=True)
        os.execve("/bin/sh", ["/bin/sh", "-c", words[2]], env)
    raise RuntimeError(f"Remote fixture refused an unrecognized command: {request}")


def bridge_proxy(output, state, mode):
    if mode not in ("bridge", "api-bridge"):
        raise RuntimeError(f"Unexpected ferrited fixture mode: {mode}")
    child = subprocess.Popen([state["server"], mode], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, env=environment(state, remote=True))

    def requests():
        try:
            for line in sys.stdin.buffer:
                request = json.loads(line)
                record(output, "ssh-request", mode=mode, request=request,
                       ssh_connection=os.environ.get("SSH_CONNECTION"))
                child.stdin.write(line)
                child.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass
        finally:
            child.stdin.close()

    threading.Thread(target=requests, daemon=True).start()
    try:
        while chunk := os.read(child.stdout.fileno(), 65536):
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
    finally:
        if child.poll() is None:
            child.terminate()
        child.wait()


def provider(output):
    if "--version" in sys.argv:
        print("codex-cli 999999.0.0")
        return
    def emit(value):
        print(json.dumps(value), flush=True)
    for line in sys.stdin:
        request = json.loads(line)
        record(output, "provider-fixture", request=request, cwd=os.getcwd(),
               store=os.environ.get("FERRITE_STORE"))
        method, request_id = request.get("method"), request.get("id")
        if request_id is None:
            continue
        result = {}
        if method in ("skills/list", "model/list"):
            result = {"data": []}
        elif method in ("thread/start", "thread/resume"):
            result = {"thread": {"id": "fixture-thread"}, "model": "fixture-codex"}
        elif method == "turn/start":
            result = {"turn": {"id": "fixture-turn", "status": "inProgress"}}
        emit({"id": request_id, "result": result})
        if method == "turn/start":
            params = {"threadId": "fixture-thread", "turnId": "fixture-turn"}
            emit({"method": "turn/started", "params": {**params, "turn": result["turn"]}})
            emit({"method": "item/completed", "params": {**params, "item": {
                "type": "agentMessage", "id": "fixture-message",
                "text": "Remote Codex protocol fixture completed. No model API was called."}}})
            emit({"method": "turn/completed", "params": {**params, "turn": {
                "id": "fixture-turn", "status": "completed"}}})


def cli(output, state, args, remote=False, check=True):
    return command(output, [state["binary"], *args], env=environment(state, remote),
                   cwd=Path(state["root"]) / "remote-project", check=check)


def launch(output, state, size):
    for name in ("binary", "server"):
        digest = hashlib.sha256()
        with Path(state[name]).open("rb") as binary:
            while chunk := binary.read(1024 * 1024):
                digest.update(chunk)
        state[name + "_sha256"] = digest.hexdigest()
    state["source_commit"] = command(output, ["git", "rev-parse", "HEAD"], cwd=REPO).stdout.strip()
    save(output, state)
    for name, binary, args, remote in (("remote-server", state["server"], ["server"], True),
                                       ("server", state["binary"], ["server"], False),
                                       ("headless", state["binary"], ["--headless"], False)):
        process = launch_process(output, state, name, [binary, *args],
                                 env=environment(state, remote, size), cwd=state["root"])
        time.sleep(.5)
        if process.poll() is not None:
            raise RuntimeError(f"{name} exited: {(output / (name + '.log')).read_text()}")
    print("Launched isolated local/remote Servers and native headless window.")


def flow(output, state, size):
    def window(*args, remote=False, check=True):
        return cli(output, state, ["window", *args], remote=remote, check=check)

    def click(selector, remote=False):
        window("click", "--selector", selector, remote=remote)

    def field(selector, text, remote=False):
        if selector == "editor":
            bounds = json.loads(window("find", selector, remote=remote).stdout)["result"]
            window("click", "--x", str(int(bounds["x"] + 60)),
                   "--y", str(int(bounds["y"] + 18)), remote=remote)
        else:
            click(selector, remote=remote)
        window("key", "cmd-a", remote=remote)
        window("type", text, remote=remote)

    def wait(selector, remote=False, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = window("find", selector, remote=remote, check=False)
            if result.returncode == 0:
                return
            time.sleep(.1)
        raise RuntimeError(f"Window never drew {selector}")

    def shot(name, remote=False):
        path = output / f"{name}-{size}.png"
        window("shot", str(path), remote=remote)
        record(output, "screenshot", name=name, path=str(path), size=size,
               source="production native headless Driver")

    def add(host):
        click("connection-add")
        field("connection-host", host)
        click("connection-submit")

    def saved():
        return json.loads((Path(state["store"]).parent / "settings.json").read_text())["machines"]

    wait("board")
    window("key", "escape")
    window("key", "cmd-,")
    click("settings-page-connections")
    wait("connection-add")
    shot("01-connections-empty")
    click("connection-add")
    shot("02-add-form")
    field("connection-host", "ferrite-audit")
    field("connection-port", "0")
    click("connection-submit")
    wait("connection-error")
    assert not saved(), "Invalid connection must not be saved"
    shot("03-invalid-input-retained")
    click("connection-cancel")
    add("ferrite-hostkey")
    wait("connection-verification")
    shot("03b-host-key-confirmation")
    click("connection-cancel")
    assert not saved(), "Cancelled host confirmation must not save a connection"
    add("ferrite-unreachable")
    shot("04-connecting")
    wait("connection-error")
    assert not saved(), "Failed connection must not be saved"
    shot("05-connection-failure")
    click("connection-cancel")
    add("ferrite-auth")
    wait("connection-verification")
    shot("06-native-auth-prompt")
    field("connection-response", "fixture-passphrase")
    shot("06b-masked-auth-response")
    click("connection-verify-submit")
    wait("connection-open-1", timeout=40)
    assert len(saved()) == 1, "Only the verified connection should persist"
    shot("07-connected-row")
    click("connection-toggle-1")
    assert not saved()[0]["enabled"], "Disable must persist"
    shot("08-disabled-row")
    click("connection-toggle-1")
    deadline = time.monotonic() + 20
    while not saved()[0]["enabled"]:
        assert time.monotonic() < deadline, "Reconnect did not persist an enabled connection"
        time.sleep(.1)
    shot("08b-reconnected")
    click("connection-open-1")
    wait("board", remote=True)
    shot("09-remote-selected", remote=True)
    window("key", "escape", remote=True)
    project = Path(state["root"]) / "remote-project"
    opened = cli(output, state, ["--machine", "ferrite-auth", "project", "open", "--cwd", str(project)])
    opened_project = json.loads(opened.stdout)["result"]["project"]
    assert opened_project["label"] == "remote-project", opened.stdout
    local_projects = cli(output, state, ["project", "list"])
    assert json.loads(local_projects.stdout)["result"]["projects"] == [], "Remote Project leaked into the local registry"
    remote_projects = cli(output, state, ["--machine", "ferrite-auth", "project", "list"])
    assert opened_project in json.loads(remote_projects.stdout)["result"]["projects"], "Remote Project was not registered remotely"
    record(output, "check", name="remote-project-ownership", passed=True)
    started = cli(output, state, ["--machine", "ferrite-auth", "thread", "start", "--cwd", str(project),
                                  "--provider", "codex", "--title", "Remote fixture", "--ready", "--focus",
                                  "--launcher", state["root"] + "/bin/codex"])
    response = json.loads(started.stdout)
    thread = response.get("thread", response.get("result", {}).get("thread"))
    if isinstance(thread, dict):
        thread = thread.get("thread", thread.get("id"))
    assert thread is not None, started.stdout
    startup_records = [json.loads(line) for line in (output / "commands.jsonl").read_text().splitlines()]
    assert any(row.get("kind") == "provider-fixture" and row.get("request", {}).get("method") == "thread/start"
               and row.get("store") == state["remote_store"] and row.get("cwd") == str(project)
               for row in startup_records), "Refusing to prompt a provider not proven to be the isolated fixture"
    cli(output, state, ["--machine", "ferrite-auth", "agent", "prompt", str(thread),
                        "--", "Confirm this remote fixture"])
    cli(output, state, ["--machine", "ferrite-auth", "agent", "wait", str(thread),
                        "--until", "done", "--timeout-ms", "10000"])
    transcript = cli(output, state, ["--machine", "ferrite-auth", "agent", "read", str(thread)])
    assert "No model API was called" in transcript.stdout, transcript.stdout
    local_threads = json.loads(cli(output, state, ["thread", "list"]).stdout)
    assert "Remote fixture" not in json.dumps(local_threads), "Remote thread leaked into local Store"
    provider_records = [json.loads(line) for line in (output / "commands.jsonl").read_text().splitlines()
                        if json.loads(line).get("kind") == "provider-fixture"]
    assert any(row.get("store") == state["remote_store"] and row.get("cwd") == str(project)
               for row in provider_records), "Codex fixture did not run in the remote-owned Store/project"
    record(output, "check", name="remote-codex-provider-ownership", passed=True,
           provider="simulated Codex app-server")
    shot("09b-remote-thread", remote=True)
    window("key", "cmd-shift-f", remote=True)
    wait("files-row-0", remote=True)
    shot("09c-remote-files-tree", remote=True)
    click("files-row-0", remote=True)
    wait("editor", remote=True)
    shot("09d-remote-file-read", remote=True)
    edited = "// Remote Files fixture saved through SSH."
    field("editor", edited, remote=True)
    window("key", "cmd-s", remote=True)
    deadline = time.monotonic() + 10
    while (project / "REMOTE_EDIT.rs").read_text() != edited:
        assert time.monotonic() < deadline, "Remote editor did not save the fixture"
        time.sleep(.1)
    requests = [json.loads(line) for line in (output / "commands.jsonl").read_text().splitlines()
                if json.loads(line).get("kind") == "ssh-request"]
    file_requests = [row for row in requests if '"file_operation"' in json.dumps(row)]
    for operation in ("snapshot", "read", "write"):
        assert any(f'"kind": "{operation}"' in json.dumps(row) and row["ssh_connection"]
                   for row in file_requests), f"No {operation} request crossed SSH"
    record(output, "check", name="remote-files-read-edit-save", passed=True,
           evidence="actual SSH file_operation snapshot/read/write requests and saved text",
           limitation=state["filesystem"])
    shot("09e-remote-file-saved", remote=True)
    window("key", "cmd-j", remote=True)
    wait("terminal", remote=True)
    marker = "FERRITE_SSH_TERMINAL_" + str(state["port"])
    terminal_output = Path(state["root"]) / "terminal-output.txt"
    terminal_command = ("printf '%s\\n' \"$SSH_CONNECTION\" \"$PWD\" " +
                        shlex.quote(marker) + " | tee " + shlex.quote(str(terminal_output)))
    click("terminal", remote=True)
    window("type", terminal_command, remote=True)
    window("key", "enter", remote=True)
    deadline = time.monotonic() + 10
    while not terminal_output.exists():
        assert time.monotonic() < deadline, "Remote terminal produced no command output"
        time.sleep(.1)
    terminal_lines = terminal_output.read_text().splitlines()
    assert len(terminal_lines) == 3, terminal_lines
    assert terminal_lines[0].split()[-1] == str(state["port"]), terminal_lines
    assert terminal_lines[1:] == [str(project), marker], terminal_lines
    write(output / "terminal-output.txt", terminal_output.read_text())
    record(output, "check", name="remote-terminal-command", passed=True,
           ssh_connection=terminal_lines[0], cwd=terminal_lines[1], marker=marker)
    shot("09f-remote-terminal-output", remote=True)
    window("key", "cmd-,", remote=True)
    click("settings-page-connections", remote=True)
    shot("10-remote-connections", remote=True)
    click("connection-toggle-1", remote=True)
    wait("board")
    window("key", "cmd-,")
    click("settings-page-connections")
    click("connection-remove-1")
    shot("11-remove-confirmation")
    click("connection-remove-cancel")
    assert len(saved()) == 1, "Cancel removal must preserve the connection"
    click("connection-remove-1")
    click("connection-remove-confirm")
    assert not saved(), "Confirmed removal must remove the saved connection"
    shot("12-removed")
    stop_processes(output, state, {"headless"})
    server = next(process for process in state["processes"] if process["name"] == "remote-server")
    assert identity(server["pid"]) == server["identity"], "Remote Server died with its window"
    cli(output, state, ["project", "list"], remote=True)
    record(output, "check", name="remote-server-survives-window-disconnect", passed=True)
    state["flow"] = {"passed": True, "size": size}
    save(output, state)


def recoveries(output, state, window, wait, size):
    started = time.time()
    root = Path(state["root"])
    project = root / "remote-project"

    def click(selector):
        window("click", "--selector", selector, remote=True)

    def shot(name):
        window("shot", str(output / f"{name}-{size}.png"), remote=True)

    def choose(name, before_open=None):
        while window("find", "files-tab-close-0", remote=True, check=False).returncode == 0:
            click("files-tab-close-0")
        if window("find", "files-tree-column", remote=True, check=False).returncode:
            click("files-tree-toggle")
        wait("files-tree", remote=True)
        bounds = json.loads(window("find", "files-tree", remote=True).stdout)["result"]
        window("click", "--x", str(int(bounds["x"] + 60)),
               "--y", str(int(bounds["y"] - 18)), remote=True)
        window("key", "cmd-a", remote=True)
        window("type", name, remote=True)
        wait("files-row-0", remote=True)
        if before_open:
            before_open()
        click("files-row-0")

    def replace(text):
        bounds = json.loads(window("find", "editor", remote=True).stdout)["result"]
        window("click", "--x", str(int(bounds["x"] + 60)),
               "--y", str(int(bounds["y"] + 18)), remote=True)
        window("key", "cmd-a", remote=True)
        window("type", text, remote=True)
        window("key", "cmd-s", remote=True)

    def saved(path, text):
        deadline = time.monotonic() + 10
        while path.read_text() != text:
            assert time.monotonic() < deadline, f"Recovery save failed: {path.name}"
            time.sleep(.1)

    for name, capture in (("RECOVERY_BINARY.bin", "14-remote-binary-guidance"),
                          ("RECOVERY_IMAGE.png", "15-remote-image-guidance")):
        choose(name)
        wait("files-unshown", remote=True)
        assert window("find", "files-open-original", remote=True, check=False).returncode
        shot(capture)

    missing = project / "RECOVERY_MISSING.rs"
    choose(missing.name, missing.unlink)
    wait("files-retry", remote=True)
    shot("16-remote-file-read-failure")
    write(missing, "// Restored remotely. Retry succeeded.")
    click("files-retry")
    wait("editor", remote=True)
    shot("17-remote-file-retry-recovered")

    conflict = project / "RECOVERY_CONFLICT.rs"
    choose(conflict.name)
    wait("editor", remote=True)
    write(conflict, "// Another remote writer changed this file.")
    replace("// My edit before Reload.")
    wait("editor-reload", remote=True)
    wait("editor-overwrite", remote=True)
    assert conflict.read_text() == "// Another remote writer changed this file."
    shot("18-remote-save-conflict")
    click("editor-reload")
    wait("editor", remote=True)
    shot("19-remote-conflict-reloaded")
    replace("// Saved after Reload.")
    saved(conflict, "// Saved after Reload.")
    write(conflict, "// A second remote writer changed this file.")
    replace("// Explicit Overwrite recovery.")
    wait("editor-overwrite", remote=True)
    click("editor-overwrite")
    saved(conflict, "// Explicit Overwrite recovery.")
    shot("20-remote-conflict-overwritten")

    window("key", "cmd-j", remote=True)
    wait("terminal", remote=True)
    click("terminal")
    window("type", "exit 23", remote=True)
    window("key", "enter", remote=True)
    wait("terminal-failed", remote=True)
    shot("21-remote-terminal-failed")
    click("terminal-retry")
    wait("terminal", remote=True)
    proof = root / "terminal-recovery.txt"
    proof.unlink(missing_ok=True)
    click("terminal")
    window("type", "printf 'SSH=%s\\nREMOTE_TERMINAL_RETRY_OK\\n' \"$SSH_CONNECTION\" | tee " + shlex.quote(str(proof)), remote=True)
    window("key", "enter", remote=True)
    deadline = time.monotonic() + 10
    while not proof.exists() or "REMOTE_TERMINAL_RETRY_OK" not in proof.read_text():
        assert time.monotonic() < deadline, "Terminal Retry did not execute marker"
        time.sleep(.1)
    assert str(state["port"]) in proof.read_text(), "Terminal Retry did not run across fixture SSH"
    shot("22-remote-terminal-retry-recovered")
    rows = [row for line in (output / "commands.jsonl").read_text().splitlines()
            if (row := json.loads(line))["time"] >= started]
    assert any(row.get("kind") == "ssh-request" and row["request"].get("op") == "file_operation"
               and row["request"]["request"].get("text") == "// Explicit Overwrite recovery."
               and row["request"]["request"].get("expected") is None and row["ssh_connection"]
               for row in rows), "Overwrite did not reach the remote file protocol"
    if state.get("recovery"):
        state.setdefault("recovery_history", []).append(state["recovery"])
    state["recovery"] = {"passed": True, "binary_sha256": state["binary_sha256"],
                         "source_commit": state["source_commit"],
                         "setup_warning": "synthetic stderr diagnostic through actual setup SSH",
                         "files": "real remote binary/read failure/retry/conflict/reload/overwrite",
                         "terminal": "actual SSH shell exit 23 then Retry and SSH_CONNECTION marker"}
    record(output, "check", name="remote-recovery-surfaces", **state["recovery"])
    save(output, state)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--binary", type=Path, default=REPO / "target/debug/ferrite")
    parser.add_argument("--server", type=Path, default=REPO / "target/debug/ferrited")
    parser.add_argument("--size", default="1440x900")
    parser.add_argument("--keep-on-failure", action="store_true")
    parser.add_argument("mode", choices=["prepare", "ssh-smoke", "remote-entry", "provider",
                                        "bridge-proxy", "launch", "flow", "verify", "files-recapture", "recovery", "summarize", "run", "remote-run", "shot", "stop", "stop-window"])
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    output = args.output.resolve()
    if args.mode in ("files-recapture", "recovery"):
        state = json.loads((output / "state.json").read_text())
        state["flow"].setdefault("binary_sha256", state["binary_sha256"])
        state["flow"].setdefault("source_commit", state["source_commit"])
        settings_path = Path(state["store"]).parent / "settings.json"
        settings = json.loads(settings_path.read_text())
        settings["machines"] = [{"name": "ferrite-audit", "target": "ferrite-audit", "enabled": True}]
        if args.mode == "recovery":
            settings["machines"] = []
            root = Path(state["root"])
            write(root / "inject-setup-warning", "Synthetic setup diagnostic for native rendering verification.")
            (root / "remote-project/RECOVERY_BINARY.bin").write_bytes(b"fixture\xff\xfebinary")
            (root / "remote-project/RECOVERY_IMAGE.png").write_bytes(base64.b64decode(
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jVS0AAAAASUVORK5CYII="))
            write(root / "remote-project/RECOVERY_MISSING.rs", "// Present in snapshot, removed before opening.")
            write(root / "remote-project/RECOVERY_CONFLICT.rs", "// Original remote text.")
        write(settings_path, json.dumps(settings))
        save(output, state)

        def window(*arguments, remote=False, check=True):
            return cli(output, state, ["window", *arguments], remote=remote, check=check)

        def wait(selector, remote=False):
            deadline = time.monotonic() + 20
            while window("find", selector, remote=remote, check=False).returncode:
                assert time.monotonic() < deadline, f"No {selector} during Files recapture"
                time.sleep(.1)

        try:
            ssh_smoke(output, state)
            launch(output, state, args.size)
            wait("board")
            window("key", "escape", "cmd-,")
            window("click", "--selector", "settings-page-connections")
            if args.mode == "recovery":
                wait("connection-add")
                window("click", "--selector", "connection-add")
                window("click", "--selector", "connection-host")
                window("type", "ferrite-audit")
                window("click", "--selector", "connection-submit")
            wait("connection-open-1")
            if args.mode == "recovery":
                window("shot", str(output / f"13-setup-warning-{args.size}.png"))
                (Path(state["root"]) / "inject-setup-warning").unlink()
            window("click", "--selector", "connection-open-1")
            wait("board", remote=True)
            window("key", "escape", remote=True)
            cli(output, state, ["--machine", "ferrite-audit", "thread", "focus", "1"])
            wait("board", remote=True)
            window("key", "cmd-shift-f", remote=True)
            wait("files", remote=True)
            if args.mode == "recovery":
                recoveries(output, state, window, wait, args.size)
                return
            while window("find", "files-tab-close-0", remote=True, check=False).returncode == 0:
                window("click", "--selector", "files-tab-close-0", remote=True)
            wait("files-empty", remote=True)
            if window("find", "files-row-0", remote=True, check=False).returncode:
                window("shot", str(output / f"09c-empty-editor-{args.size}.png"), remote=True)
                window("click", "--selector", "files-tree-toggle", remote=True)
            wait("files-row-0", remote=True)
            for name, selector in (("09c-remote-files-tree", None), ("09d-remote-file-read", "files-row-0")):
                if selector:
                    window("click", "--selector", selector, remote=True)
                    wait("editor", remote=True)
                window("shot", str(output / f"{name}-{args.size}.png"), remote=True)
            bounds = json.loads(window("find", "editor", remote=True).stdout)["result"]
            window("click", "--x", str(int(bounds["x"] + 60)),
                   "--y", str(int(bounds["y"] + 18)), remote=True)
            marker = "// Remote Files saved over SSH: " + time.strftime("%H:%M:%S") + "."
            window("key", "cmd-a", remote=True)
            window("type", marker, remote=True)
            window("key", "cmd-s", remote=True)
            deadline = time.monotonic() + 10
            path = Path(state["root"]) / "remote-project/REMOTE_EDIT.rs"
            while path.read_text() != marker:
                assert time.monotonic() < deadline, "Files recapture did not save selected replacement"
                time.sleep(.1)
            rows = [json.loads(line) for line in (output / "commands.jsonl").read_text().splitlines()]
            assert any(row.get("kind") == "ssh-request" and row["request"].get("op") == "file_operation"
                       and row["request"]["request"].get("text") == marker and row["ssh_connection"]
                       for row in rows), "Files recapture did not write across SSH"
            window("shot", str(output / f"09e-remote-file-saved-{args.size}.png"), remote=True)
            state["files_recapture"] = {"passed": True, "size": args.size,
                                        "binary_sha256": state["binary_sha256"],
                                        "source_commit": state["source_commit"]}
            record(output, "check", name="remote-files-final-layout-read-write", passed=True,
                   binary_sha256=state["binary_sha256"])
            save(output, state)
        finally:
            stop_processes(output, state)
        return
    if args.mode == "summarize":
        summary = {"passed": True, "transport": "actual nonroot OpenSSH localhost",
                   "provider": "simulated Codex app-server, explicit fixture launcher checked before prompting",
                   "filesystem": "Client and SSH server share this Mac's filesystem; recorded SSH requests prove routing. Separate collision unit tests prove no local fallback.",
                   "bootstrap": "Preinstalled task-owned ferrited; cold installer/archive/checksum checks are separate unit fixtures, not a published-asset download.",
                   "sizes": {}}
        for size in ("1440x900", "1000x700", "800x600"):
            directory = output / size
            state = json.loads((directory / "state.json").read_text())
            assert state.get("flow", {}).get("passed"), f"No passing flow for {size}"
            assert all(process.get("stopped") and identity(process["pid"]) != process["identity"]
                       for process in state["processes"]), f"Owned processes remain for {size}"
            rows = [json.loads(line) for line in (directory / "commands.jsonl").read_text().splitlines()]
            file_requests = [row["request"]["request"]["kind"] for row in rows
                             if row.get("kind") == "ssh-request" and row["request"].get("op") == "file_operation"]
            summary["sizes"][size] = {
                "passed": True, "owned_processes_stopped": True,
                "binary_sha256": state["binary_sha256"], "server_sha256": state["server_sha256"],
                "full_flow_binary_sha256": state["flow"].get("binary_sha256", state["binary_sha256"]),
                "full_flow_source_commit": state["flow"].get("source_commit", state["source_commit"]),
                "files_recapture": state.get("files_recapture"),
                "recovery": state.get("recovery"),
                "checks": [row for row in rows if row.get("kind") == "check"],
                "file_requests": {kind: file_requests.count(kind) for kind in ("snapshot", "read", "write")},
                "simulated_turns": sum(row.get("kind") == "provider-fixture" and
                                       row.get("request", {}).get("method") == "turn/start" for row in rows),
                "screenshots": [str(path.relative_to(output)) for path in sorted(directory.glob("*.png"))
                                if path.name[0].isdigit()],
            }
        write(output / "summary.json", json.dumps(summary, indent=2) + "\n")
        print(json.dumps({"passed": True, "sizes": list(summary["sizes"]),
                          "screenshots": sum(len(size["screenshots"]) for size in summary["sizes"].values())}))
        return
    if args.mode == "verify":
        for size in ("1440x900", "1000x700", "800x600"):
            sized_output = output / size
            prepare(sized_output, args.binary.resolve(), args.server.resolve())
            state = json.loads((sized_output / "state.json").read_text())
            failed = False
            try:
                ssh_smoke(sized_output, state)
                launch(sized_output, state, size)
                flow(sized_output, state, size)
            except Exception as error:
                failed = True
                record(sized_output, "verification-failed", error=str(error))
                for remote in (False, True):
                    try:
                        cli(sized_output, state, ["window", "shot", str(sized_output /
                            f"failure-{'remote' if remote else 'local'}-{size}.png")],
                            remote=remote, check=False)
                    except (OSError, subprocess.TimeoutExpired):
                        pass
                raise
            finally:
                if not (failed and args.keep_on_failure):
                    stop_processes(sized_output, state)
        return
    if args.mode == "prepare":
        prepare(output, args.binary.resolve(), args.server.resolve())
        return
    if args.mode == "provider":
        provider(output)
        return
    state = json.loads((output / "state.json").read_text())
    if args.mode == "ssh-smoke":
        ssh_smoke(output, state)
    elif args.mode == "remote-entry":
        remote_entry(output, state)
    elif args.mode == "bridge-proxy":
        bridge_proxy(output, state, args.arguments[0])
    elif args.mode == "launch":
        launch(output, state, args.size)
    elif args.mode == "flow":
        flow(output, state, args.size)
    elif args.mode in ("run", "remote-run"):
        result = cli(output, state, args.arguments, remote=args.mode == "remote-run", check=False)
        print(result.stdout, end="")
        print(result.stderr, end="", file=sys.stderr)
        raise SystemExit(result.returncode)
    elif args.mode == "shot":
        if len(args.arguments) != 1 or Path(args.arguments[0]).name != args.arguments[0]:
            raise RuntimeError("shot requires one filename within the evidence directory")
        result = cli(output, state, ["window", "shot", str(output / args.arguments[0])])
        print(result.stdout, end="")
    elif args.mode in ("stop", "stop-window"):
        stop_processes(output, state, {"headless"} if args.mode == "stop-window" else None)


if __name__ == "__main__":
    main()
