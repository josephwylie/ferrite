#!/usr/bin/env python3
"""Native remote parity evidence. Provider traffic is an explicit protocol fixture.

No app starts during preparation, composition, or syntax checking. `verify` runs
one complete native flow and stops every process it owns, including on failure.
The older SSH audit harness supplies process and transport helpers unchanged.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import struct
import sys
import time

REPO = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ssh_audit", REPO / "scripts/verify-remote-connections.py")
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)
DEFAULT_OUTPUT = REPO / "docs/audits/2026-11-16-remote-connections/parity/native"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def rows(output, kind=None):
    path = output / "commands.jsonl"
    values = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    return [row for row in values if kind is None or row.get("kind") == kind]


def until(test, message, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = test()
        if value:
            return value
        time.sleep(.1)
    raise AssertionError(message)


def png_size(path):
    with Path(path).open("rb") as stream:
        header = stream.read(24)
    assert header[:8] == b"\x89PNG\r\n\x1a\n", f"Not a PNG: {path}"
    return list(struct.unpack(">II", header[16:24]))


def prepare(output, binary, server):
    BASE.prepare(output, binary, server)
    state = json.loads((output / "state.json").read_text())
    root = Path(state["root"])
    script = Path(__file__).resolve()
    for filename, mode in (("codex", "provider"), ("ferrited", "bridge-proxy")):
        invocation = shlex.join([sys.executable, str(script), "--output", str(output), mode])
        BASE.write(root / "bin" / filename, '#!/bin/sh\nexec ' + invocation + ' "$@"\n', 0o700)
    config = (root / "sshd_config").read_text()
    config = config.replace(str(REPO / "scripts/verify-remote-connections.py"), str(script))
    BASE.write(root / "sshd_config", config)
    included = root / "ssh-hosts/included"
    included.mkdir()
    BASE.write(included / "hosts.conf", "Host ferrite-included\n    HostName 127.0.0.1\n    Port 1\n    ConnectTimeout 2\n")
    for config_path in (root / "ssh-hosts/config", root / "ssh_config"):
        with config_path.open("a") as stream:
            stream.write(f"\nInclude {included}/*.conf\n")
    for name in ("remote-project", "local-project"):
        project = root / name
        project.mkdir(exist_ok=True)
        BASE.write(project / "REMOTE_EDIT.rs", f"// {name} untouched.\n")
        BASE.write(project / "REMOTE_ONLY.md", f"# {name}\n")
        for args in (["init", "-q"], ["config", "user.email", "fixture@example.invalid"],
                     ["config", "user.name", "Parity fixture"], ["add", "."],
                     ["commit", "-qm", "Fixture baseline"]):
            BASE.command(output, ["git", *args], cwd=project)
    BASE.write(root / "remote-project/REMOTE_EDIT.rs", "// Remote change for staging.\n")
    BASE.write(root / "local-project/REMOTE_EDIT.rs", "// Local collision must stay untouched.\n")
    state["local_collision_sha256"] = digest(root / "local-project/REMOTE_EDIT.rs")
    state["verification"] = {"functional": "not run", "visual": "requires parent review",
                             "actual_model": "not run; explicit Codex protocol fixture"}
    state["harness_sha256"] = digest(script)
    BASE.save(output, state)
    return state


def provider(output):
    if "--version" in sys.argv:
        print("codex-cli 999999.0.0")
        return
    state = json.loads((output / "state.json").read_text())
    thread = "parity-fixture-" + str(os.getpid())
    turn_number = 0
    BASE.record(output, "provider-start", pid=os.getpid(), identity=BASE.identity(os.getpid()),
                store=os.environ.get("FERRITE_STORE"))

    def emit(value):
        print(json.dumps(value), flush=True)

    for line in sys.stdin:
        request = json.loads(line)
        BASE.record(output, "provider-fixture", request=request, cwd=os.getcwd(),
                    store=os.environ.get("FERRITE_STORE"), pid=os.getpid())
        method, request_id = request.get("method"), request.get("id")
        if request_id is None:
            continue
        result = {}
        if method in ("skills/list", "model/list"):
            result = {"data": []}
        elif method in ("thread/start", "thread/resume"):
            result = {"thread": {"id": thread}, "model": "fixture-codex"}
        elif method == "turn/start":
            turn_number += 1
            result = {"turn": {"id": f"turn-{turn_number}", "status": "inProgress"}}
        emit({"id": request_id, "result": result})
        if method != "turn/start":
            continue
        params = {"threadId": thread, "turnId": result["turn"]["id"]}
        emit({"method": "turn/started", "params": {**params, "turn": result["turn"]}})
        message = "Remote protocol fixture is streaming. "
        emit({"method": "item/started", "params": {**params, "item": {
            "type": "agentMessage", "id": "message", "text": ""}}})
        emit({"method": "item/agentMessage/delta", "params": {
            **params, "itemId": "message", "delta": message}})
        tool = {"type": "commandExecution", "id": "tool", "command": "fixture inspect attachment",
                "status": "inProgress", "aggregatedOutput": ""}
        emit({"method": "item/started", "params": {**params, "item": tool}})
        emit({"method": "item/commandExecution/outputDelta", "params": {
            **params, "itemId": "tool", "delta": "Protocol fixture tool activity.\n"}})
        release = Path(state["root"]) / f"release-{os.getpid()}-{turn_number}"
        BASE.record(output, "provider-await", release=str(release), request=request,
                    store=os.environ.get("FERRITE_STORE"), pid=os.getpid())
        until(release.exists, "Harness did not release the streaming fixture", timeout=45)
        emit({"method": "item/completed", "params": {**params, "item": {
            **tool, "status": "completed", "exitCode": 0,
            "aggregatedOutput": "Protocol fixture tool activity.\n", "durationMs": 1000}}})
        emit({"method": "item/completed", "params": {**params, "item": {
            "type": "agentMessage", "id": "message",
            "text": message + "No model API was called."}}})
        emit({"method": "turn/completed", "params": {**params, "turn": {
            "id": params["turnId"], "status": "completed"}}})


class Window:
    def __init__(self, output, state, size):
        self.output, self.state, self.size = output, state, size
        self.remote = False

    def command(self, *args, check=True):
        return BASE.cli(self.output, self.state, ["window", *args], remote=self.remote, check=check)

    def has(self, selector):
        return self.command("find", selector, check=False).returncode == 0

    def wait(self, selector, visible=True):
        until(lambda: self.has(selector) == visible, f"Expected {selector} visible={visible}")

    def click(self, selector):
        self.wait(selector)
        self.command("click", "--selector", selector)

    def field(self, selector, value):
        if selector in ("editor", "composer-block"):
            bounds = json.loads(self.command("find", selector).stdout)["result"]
            self.command("click", "--x", str(int(bounds["x"] + 45)),
                         "--y", str(int(bounds["y"] + 16)))
        else:
            self.click(selector)
        self.command("key", "cmd-a")
        self.command("type", value)

    def shot(self, name):
        path = self.output / f"{name}-{self.size}.png"
        result = json.loads(self.command("shot", str(path)).stdout)["result"]
        logical = [int(part) for part in self.size.split("x")]
        pixels = png_size(path)
        scale = result["scale"]
        assert pixels == [result["width"], result["height"]]
        assert pixels == [round(value * scale) for value in logical], (pixels, logical, scale)
        metadata = {"path": str(path), "sha256": digest(path), "logical_viewport": logical,
                    "device_pixel_ratio": scale, "pixel_dimensions": pixels, "image_format": "PNG",
                    "crop_px": [0, 0, *pixels], "surface": "Ferrite native headless Driver",
                    "provider": self.state["provider"], "source_commit": self.state["source_commit"],
                    "binary_sha256": self.state["binary_sha256"]}
        BASE.write(path.with_suffix(".json"), json.dumps(metadata, indent=2) + "\n")
        BASE.record(self.output, "screenshot", **metadata)
        return path

    def settings(self):
        self.command("key", "cmd-,")
        self.click("settings-page-connections")
        self.wait("connection-add")

    def saved(self):
        return json.loads((Path(self.state["store"]).parent / "settings.json").read_text())["machines"]

    def check(self, name, **evidence):
        BASE.record(self.output, "check", name=name, passed=True, **evidence)

    def add_ssh(self, host):
        self.click("connection-add")
        self.click("connection-mode-ssh")
        self.field("connection-host", host)
        if self.has("connection-host-suggestions"):
            self.command("key", "escape")
        self.click("connection-submit")


def modal_flow(ui):
    ui.wait("board")
    ui.command("key", "escape")
    ui.settings()
    ui.shot("01-connections-empty")
    ui.click("connection-add")
    ui.wait("connection-remote-host")
    ui.wait("connection-pairing-code")
    assert not ui.has("connection-host"), "New modal must default to Remote link"
    ui.shot("02-add-remote-default")
    ui.field("connection-remote-host", "not a URL")
    ui.field("connection-pairing-code", "invalid")
    ui.click("connection-submit")
    ui.wait("connection-error")
    assert not ui.saved(), "Invalid remote link persisted"
    ui.shot("03-remote-validation")
    ui.click("connection-close")
    ui.wait("connection-dialog", visible=False)
    ui.click("connection-add")
    ui.click("connection-mode-ssh")
    ui.shot("04-add-ssh")
    ui.field("connection-host", "ferrite-incl")
    ui.wait("connection-host-suggestions")
    ui.wait("connection-host-suggestion-0")
    ui.shot("05-ssh-include-autocomplete")
    ui.command("key", "down", "enter")
    ui.wait("connection-host-suggestions", visible=False)
    ui.wait("connection-error")
    ui.shot("06-ssh-autocomplete-selected")
    ui.command("key", "escape")
    ui.wait("connection-dialog", visible=False)
    ui.click("connection-add")
    ui.wait("connection-remote-host")
    ui.click("connection-mode-ssh")
    ui.field("connection-host", "ferrite-audit")
    ui.field("connection-port", "0")
    ui.click("connection-submit")
    ui.wait("connection-error")
    ui.shot("07-ssh-validation")
    assert not ui.saved(), "Invalid SSH connection persisted"
    ui.click("connection-close")
    ui.add_ssh("ferrite-unreachable")
    ui.wait("connection-progress")
    ui.shot("08-ssh-connecting-cancel")
    ui.click("connection-close")
    until(lambda: not ui.saved(), "Cancelled SSH connection persisted")
    ui.wait("connection-dialog", visible=False)
    ui.add_ssh("ferrite-unreachable")
    ui.wait("connection-error")
    ui.shot("09-ssh-failure")
    assert not ui.saved(), "Failed SSH connection persisted"
    ui.click("connection-close")
    ui.add_ssh("ferrite-auth")
    ui.wait("connection-verification")
    ui.shot("10-ssh-authentication")
    ui.field("connection-response", "fixture-passphrase")
    ui.click("connection-verify-submit")
    ui.wait("connection-menu-1")
    assert len(ui.saved()) == 1
    ui.shot("11-ssh-connected")
    ui.check("modal-default-modes-autocomplete-validation-cancel-connect")


def remote_cli(ui, *args):
    machine = ui.saved()[0]["name"]
    return BASE.cli(ui.output, ui.state, ["--machine", machine, *args])


def thread_id(result):
    body = json.loads(result.stdout)
    value = body.get("thread", body.get("result", {}).get("thread"))
    if isinstance(value, dict):
        value = value.get("thread", value.get("id"))
    assert value is not None, body
    return str(value)


def local_collision(ui):
    project = Path(ui.state["root"]) / "local-project"
    result = BASE.command(ui.output, [ui.state["binary"], "thread", "start", "--cwd", project,
                            "--provider", "codex", "--title", "LOCAL COLLISION", "--ready",
                            "--launcher", Path(ui.state["root"]) / "bin/codex"],
                 env=BASE.environment(ui.state), cwd=project)
    ui.state["local_collision_thread"] = thread_id(result)
    ui.check("local-collision-thread-created", thread=ui.state["local_collision_thread"])


def attachment_turn(ui, name, screenshot, new=False):
    before = len(rows(ui.output, "provider-await"))
    if new:
        ui.command("key", "cmd-n")
    ui.wait("composer-block")
    ui.command("attach", str(screenshot))
    ui.wait("tray-image-0")
    ui.wait("attachment-upload-progress", visible=False)
    assert not ui.has("attachment-failed"), "Remote screenshot upload failed"
    ui.shot(name + "-attachment-ready")
    ui.field("composer-block", "Inspect the attached screenshot. PARITY_STREAM")
    ui.click("composer-send")
    pending = until(lambda: rows(ui.output, "provider-await")[before:], "Fixture did not receive submitted prompt")[0]
    assert pending["store"] == ui.state["remote_store"], "Prompt reached the local provider"
    inputs = pending["request"]["params"].get("input", [])
    image_inputs = [item for item in inputs if item.get("type") == "localImage"]
    assert image_inputs, f"Screenshot absent from provider input: {inputs}"
    remote_path = Path(image_inputs[0]["path"])
    attachment_root = Path(ui.state["remote_store"]) / "attachments"
    assert remote_path.is_relative_to(attachment_root), (remote_path, attachment_root)
    assert remote_path != screenshot, "Local screenshot path forwarded without upload"
    assert digest(remote_path) == digest(screenshot), "Remote uploaded bytes differ"
    ui.shot(name + "-streaming-tool")
    BASE.write(Path(pending["release"]), "release\n")
    ui.wait("composer-stop", visible=False)
    ui.shot(name + "-completed")
    ui.check(name, remote_path=str(remote_path), screenshot_sha256=digest(screenshot),
             provider="simulated Codex app-server", input=inputs)


def workspace_flow(ui, protocol, screenshot):
    root = Path(ui.state["root"])
    project = root / "remote-project"
    ui.click("connection-menu-1")
    ui.click("connection-open-1")
    ui.remote = True
    ui.wait("board")
    ui.command("key", "escape")
    opened = remote_cli(ui, "project", "open", "--cwd", str(project))
    assert "remote-project" in opened.stdout
    started = remote_cli(ui, "thread", "start", "--cwd", str(project), "--provider", "codex",
                         "--title", "Remote parity " + protocol, "--ready", "--focus",
                         "--launcher", str(root / "bin/codex"))
    thread = thread_id(started)
    if protocol == "ssh":
        assert thread == ui.state["local_collision_thread"], "Local and remote thread IDs did not collide"
    until(lambda: any(row.get("store") == ui.state["remote_store"] and row.get("cwd") == str(project)
                      and row.get("request", {}).get("method") == "thread/start"
                      for row in rows(ui.output, "provider-fixture")), "Remote fixture launcher was not proven")
    ui.shot(protocol + "-remote-thread")
    attachment_turn(ui, protocol + "-existing-thread", screenshot)
    attachment_turn(ui, protocol + "-new-thread", screenshot, new=True)
    remote_cli(ui, "thread", "focus", thread)
    ui.command("key", "cmd-shift-f")
    ui.wait("files-row-0")
    ui.shot(protocol + "-files-tree")
    ui.click("files-row-0")
    ui.wait("editor")
    ui.shot(protocol + "-file-read")
    edited = "// Saved from native " + protocol + " remote editor."
    ui.field("editor", edited)
    ui.command("key", "cmd-s")
    until(lambda: (project / "REMOTE_EDIT.rs").read_text() == edited, "Remote file save did not land")
    assert digest(root / "local-project/REMOTE_EDIT.rs") == ui.state["local_collision_sha256"]
    ui.shot(protocol + "-file-saved")
    ui.command("key", "cmd-shift-d")
    ui.wait("changes-file-0")
    ui.shot(protocol + "-changes")
    ui.click("changes-stage-0")
    until(lambda: "REMOTE_EDIT.rs" in BASE.command(ui.output, ["git", "diff", "--cached", "--name-only"], cwd=project).stdout,
          "Remote git index did not stage the selected file")
    assert not BASE.command(ui.output, ["git", "diff", "--cached", "--name-only"], cwd=root / "local-project").stdout
    ui.shot(protocol + "-changes-staged")
    ui.click("changes-stage-0")
    until(lambda: not BASE.command(ui.output, ["git", "diff", "--cached", "--name-only"], cwd=project).stdout,
          "Remote git index did not unstage the selected file")
    ui.shot(protocol + "-changes-unstaged")
    ui.command("key", "cmd-j")
    ui.wait("terminal")
    marker = "FERRITE_PARITY_" + protocol.upper()
    destination = root / (protocol + "-terminal.txt")
    text = "printf '%s\\n' \"$PWD\" " + shlex.quote(marker) + " | tee " + shlex.quote(str(destination))
    ui.click("terminal")
    ui.command("type", text)
    ui.command("key", "enter")
    until(destination.exists, "Remote terminal produced no output")
    assert destination.read_text().splitlines() == [str(project), marker]
    ui.shot(protocol + "-terminal")
    assert "Remote parity " + protocol not in BASE.cli(ui.output, ui.state, ["thread", "list"]).stdout
    assert "remote-project" not in BASE.cli(ui.output, ui.state, ["project", "list"]).stdout
    if protocol == "ssh":
        requests = rows(ui.output, "ssh-request")
        for kind in ("snapshot", "read", "write"):
            assert any(row.get("ssh_connection") and row["request"].get("op") == "file_operation"
                       and row["request"]["request"].get("kind") == kind for row in requests), kind
        assert any(row.get("ssh_connection") for row in rows(ui.output, "ssh-terminal"))
        for operation, actions in (("attachment", ("begin", "chunk", "finish")),
                                   ("changes", ("read", "set_staged"))):
            for action in actions:
                assert any(row.get("ssh_connection") and row["request"].get("op") == operation
                           and row["request"]["request"].get("action") == action
                           for row in requests), (operation, action)
    ui.check(protocol + "-workspace-routing", remote_thread=thread,
             local_collision_sha256=ui.state["local_collision_sha256"])
    ui.settings()


def disconnect_remove(ui, protocol):
    ui.click("connection-toggle-1")
    ui.remote = False
    ui.wait("board")
    ui.settings()
    until(lambda: not ui.saved()[0]["enabled"], "Disable did not persist")
    ui.shot(protocol + "-disconnected")
    ui.click("connection-toggle-1")
    until(lambda: ui.saved()[0]["enabled"], "Reconnect did not persist")
    ui.wait("connection-menu-1")
    ui.shot(protocol + "-reconnected")
    ui.click("connection-menu-1")
    ui.click("connection-remove-1")
    ui.shot(protocol + "-remove-confirmation")
    ui.click("connection-remove-cancel")
    assert len(ui.saved()) == 1
    ui.click("connection-menu-1")
    ui.click("connection-remove-1")
    ui.click("connection-remove-confirm")
    until(lambda: not ui.saved(), "Connection removal did not persist")
    ui.shot(protocol + "-removed")
    server = next(item for item in ui.state["processes"] if item["name"] == "remote-server")
    assert BASE.identity(server["pid"]) == server["identity"], "Remote Server died on disconnect"
    assert "Remote parity" in BASE.cli(ui.output, ui.state, ["thread", "list"], remote=True).stdout
    ui.check(protocol + "-disconnect-reconnect-remove-server-survival")


def remote_link_flow(ui):
    import socket
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    endpoint = f"http://127.0.0.1:{port}"
    BASE.launch_process(ui.output, ui.state, "remote-gateway",
                        [ui.state["server"], "remote", "serve", "--listen", f"127.0.0.1:{port}",
                         "--public-url", endpoint, "--allow-loopback-http"],
                        env=BASE.environment(ui.state, remote=True), cwd=ui.state["root"])

    def pairing():
        result = BASE.command(ui.output, [ui.state["server"], "remote", "pairing", "create"],
                              env=BASE.environment(ui.state, remote=True), check=False)
        return json.loads(result.stdout) if result.returncode == 0 else None

    pair = until(pairing, "Remote gateway did not issue a pairing code")
    ui.click("connection-add")
    ui.field("connection-remote-host", pair["url"])
    ui.field("connection-pairing-code", "incorrect-code")
    ui.click("connection-submit")
    ui.wait("connection-error")
    assert not ui.saved()
    ui.shot("20-remote-pairing-rejected")
    ui.field("connection-pairing-code", pair["code"])
    ui.click("connection-submit")
    ui.wait("connection-menu-1")
    assert len(ui.saved()) == 1
    ui.shot("21-remote-link-connected")
    ui.check("remote-link-real-gateway-pairing", endpoint=endpoint, server_id=pair["server_id"],
             expiry_seconds=pair["expires_in"], expiry_runtime="covered by server::remote::access::tests::expired_codes_and_excessive_auth_requests_are_rejected; not asserted in this native run")


def verify(args):
    output = args.output.resolve()
    state = prepare(output, args.binary.resolve(), args.server.resolve())
    ui = Window(output, state, args.size)
    try:
        BASE.ssh_smoke(output, state)
        BASE.launch(output, state, args.size)
        local_collision(ui)
        modal_flow(ui)
        screenshot = output / f"02-add-remote-default-{args.size}.png"
        workspace_flow(ui, "ssh", screenshot)
        disconnect_remove(ui, "ssh")
        remote_link_flow(ui)
        workspace_flow(ui, "link", screenshot)
        disconnect_remove(ui, "link")
        state["verification"]["functional"] = "passed"
    except Exception as error:
        state["verification"]["functional"] = "failed"
        BASE.record(output, "verification-failed", error=str(error))
        try:
            ui.shot("failure")
        except Exception as shot_error:
            BASE.record(output, "failure-screenshot-unavailable", error=str(shot_error))
        raise
    finally:
        BASE.stop_processes(output, state)
        import signal
        for process in rows(output, "provider-start"):
            if BASE.identity(process["pid"]) == process["identity"]:
                os.kill(process["pid"], signal.SIGTERM)
                until(lambda: BASE.identity(process["pid"]) != process["identity"],
                      "Fixture provider survived cleanup", timeout=5)
        state["verification"]["owned_processes_stopped"] = all(
            BASE.identity(item["pid"]) != item["identity"] for item in state["processes"])
        BASE.save(output, state)
        summary = {**state["verification"], "source_commit": state.get("source_commit"),
                   "binary_sha256": state.get("binary_sha256"),
                   "harness_sha256": state["harness_sha256"],
                   "checks": rows(output, "check"), "screenshots": rows(output, "screenshot"),
                   "limitations": [state["provider"], state["filesystem"],
                                   "Localhost fixture uses a preinstalled task-owned runtime.",
                                   "Remote link uses explicit loopback HTTP; production TLS has separate tests.",
                                   "Visual parity is not inferred from functional assertions."]}
        BASE.write(output / "summary.json", json.dumps(summary, indent=2) + "\n")
    assert state["verification"]["owned_processes_stopped"], "Owned processes remain alive"
    print(json.dumps(state["verification"]))


def compose(manifest):
    from PIL import Image, ImageDraw
    entries = json.loads(manifest.read_text())
    for entry in entries:
        images, metadata = [], []
        for side in ("t3", "ferrite"):
            meta_path = (manifest.parent / entry[side]).resolve()
            meta = json.loads(meta_path.read_text())
            path = Path(meta["path"])
            if not path.is_absolute():
                path = meta_path.parent / path
            assert digest(path) == meta["sha256"], f"Source changed: {path}"
            with Image.open(path) as original:
                assert list(original.size) == meta["pixel_dimensions"]
                assert original.format == meta["image_format"]
            x, y, width, height = meta["crop_px"]
            assert x >= 0 and y >= 0 and width > 0 and height > 0
            assert x + width <= meta["pixel_dimensions"][0] and y + height <= meta["pixel_dimensions"][1]
            images.append(Image.open(path).convert("RGB").crop((x, y, x + width, y + height)))
            metadata.append(meta)
        for key in ("logical_viewport", "device_pixel_ratio"):
            assert metadata[0][key] == metadata[1][key], f"Unmatched {key}: {entry['name']}"
        assert images[0].size == images[1].size, "Comparison crops must have identical pixel dimensions"
        width, height = images[0].size
        canvas = Image.new("RGB", (width * 2, height + 60), "#ffffff")
        draw = ImageDraw.Draw(canvas)
        for at, (source, meta) in enumerate(zip(images, metadata)):
            canvas.paste(source, (at * width, 60))
            draw.text((at * width + 12, 10),
                      f"{meta['surface']} | {meta['image_format']} {meta['pixel_dimensions']} | DPR {meta['device_pixel_ratio']}", fill="#000000")
            draw.text((at * width + 12, 30),
                      f"Viewport {meta['logical_viewport']} | crop {meta['crop_px']}", fill="#000000")
        destination = manifest.parent / (entry["name"] + "-comparison.png")
        canvas.save(destination)
        BASE.write(destination.with_suffix(".json"), json.dumps({
            "sources": metadata, "comparison_sha256": digest(destination),
            "composition": "Native pixel crops, side by side, no scaling or retouching",
            "verdict": "requires parent visual review"}, indent=2) + "\n")
        print(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--binary", type=Path, default=REPO / "target/debug/ferrite")
    parser.add_argument("--server", type=Path, default=REPO / "target/debug/ferrited")
    parser.add_argument("--size", default="1440x900")
    parser.add_argument("mode", choices=("verify", "prepare", "provider", "remote-entry", "bridge-proxy",
                                         "run", "remote-run", "stop", "compose"))
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    args.output = args.output.resolve()
    if args.mode == "verify":
        verify(args)
    elif args.mode == "prepare":
        prepare(args.output, args.binary.resolve(), args.server.resolve())
    elif args.mode == "provider":
        provider(args.output)
    elif args.mode == "compose":
        assert len(args.arguments) == 1, "compose requires a comparison manifest JSON"
        compose(Path(args.arguments[0]).resolve())
    else:
        state = json.loads((args.output / "state.json").read_text())
        if args.mode == "remote-entry":
            BASE.remote_entry(args.output, state)
        elif args.mode == "bridge-proxy":
            BASE.bridge_proxy(args.output, state, args.arguments[0])
        elif args.mode == "stop":
            BASE.stop_processes(args.output, state)
        else:
            result = BASE.cli(args.output, state, args.arguments, remote=args.mode == "remote-run", check=False)
            print(result.stdout, end="")
            print(result.stderr, end="", file=sys.stderr)
            raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
