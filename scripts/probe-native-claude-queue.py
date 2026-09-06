#!/usr/bin/env python3
"""Opt-in live Claude queue probe (uses auth/model tokens, tools disabled).

Run: python3 scripts/probe-native-claude-queue.py --mode cancel
Modes: cancel, batch, interrupt, persistence, image-cancel. Output is sanitized JSONL;
account, filesystem, initialization commands, and arbitrary tool data are omitted.
Persistence mode creates only its own disposable provider conversation.
"""
import argparse
import json
import queue
import subprocess
import tempfile
import threading
import time
import uuid


def emit(value):
    print(json.dumps(value), flush=True)


def sanitized(frame):
    kind = frame.get("type")
    if kind == "command_lifecycle":
        return {key: frame[key] for key in ("type", "command_uuid", "state") if key in frame}
    if kind == "system" and frame.get("subtype") == "init":
        return {key: frame[key] for key in ("type", "subtype", "claude_code_version", "capabilities") if key in frame}
    if kind == "result":
        return {key: frame[key] for key in ("type", "subtype", "terminal_reason", "user_message_uuid", "user_message_uuids", "queued_turn_count") if key in frame}
    if kind == "control_response" and frame.get("response", {}).get("request_id") != "init":
        response = frame.get("response", {})
        payload = response.get("response", {})
        return {"type": kind, "request_id": response.get("request_id"), "subtype": response.get("subtype"), "response": {key: payload[key] for key in ("cancelled", "still_queued") if key in payload}}


class Session:
    def __init__(self, cwd, persist=False, resume=None):
        args = ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--safe-mode", "--strict-mcp-config", "--tools", "", "--model", "haiku", "--system-prompt", "Reply exactly as requested. No tools."]
        if not persist:
            args += ["--no-session-persistence"]
        if resume:
            args += ["--resume", resume]
        self.process = subprocess.Popen(args, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
        self.messages = queue.Queue()
        self.session_id = resume
        threading.Thread(target=self.read, daemon=True).start()
        self.control("init", {"subtype": "initialize"})

    def read(self):
        for line in self.process.stdout:
            self.messages.put(json.loads(line))
        self.messages.put(None)

    def send(self, frame):
        self.process.stdin.write(json.dumps(frame) + "\n")
        self.process.stdin.flush()

    def prompt(self, text, content=None):
        command_id = str(uuid.uuid4())
        emit({"submitted_uuid": command_id, "prompt": text})
        self.send({"type": "user", "uuid": command_id, "isAsync": True, "message": {"role": "user", "content": text if content is None else content}})
        return command_id

    def control(self, request_id, request):
        self.send({"type": "control_request", "request_id": request_id, "request": request})

    def until(self, predicate, timeout=45):
        deadline = time.monotonic() + timeout
        while True:
            frame = self.messages.get(timeout=max(0, deadline - time.monotonic()))
            if frame is None:
                raise RuntimeError("Claude exited before expected frame")
            if frame.get("type") == "system" and frame.get("subtype") == "init":
                self.session_id = frame["session_id"]
            value = sanitized(frame)
            if value:
                emit(value)
            if predicate(frame):
                return frame

    def close(self, crash=False):
        if self.process.poll() is None:
            (self.process.kill if crash else self.process.terminate)()
        self.process.wait(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=["cancel", "batch", "interrupt", "persistence", "image-cancel"], default="cancel")
    mode = parser.parse_args().mode
    emit({"mode": mode, "version": subprocess.check_output(["claude", "--version"], text=True).strip()})
    with tempfile.TemporaryDirectory(prefix="ferrite-claude-queue-") as cwd:
        session = Session(cwd, persist=mode == "persistence")
        try:
            first = session.prompt("Print numbers 1 through 100, one per line.")
            session.until(lambda x: x.get("type") == "stream_event")
            if mode == "image-cancel":
                # Valid 1x1 opaque RGB PNG; fixture output excludes these bytes.
                content = [{"type": "text", "text": "Describe this image."},
                           {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC'}}]
                image_id = session.prompt("Describe this image.", content=content)
                emit({"image_uuid": image_id, "content_types": ["text", "image"], "image_dimensions": [1, 1]})
                session.until(lambda x: x.get("command_uuid") == image_id and x.get("state") == "queued")
                session.control("cancel_image", {"subtype": "cancel_async_message", "message_uuid": image_id})
                receipt = session.until(lambda x: x.get("type") == "control_response" and x.get("response", {}).get("request_id") == "cancel_image")
                if receipt["response"].get("response", {}).get("cancelled") is not True:
                    raise RuntimeError("Image already started; cancellation race lost")
                session.until(lambda x: x.get("command_uuid") == first and x.get("state") == "completed")
                return
            second = session.prompt("Reply exactly SECOND.")
            if mode == "persistence":
                session.until(lambda x: x.get("command_uuid") == second and x.get("state") == "queued")
                native_id = session.session_id
                session.close(crash=True)
                session = Session(cwd, persist=True, resume=native_id)
                try:
                    session.until(lambda x: x.get("type") in ("result", "command_lifecycle"), timeout=8)
                    emit({"resume_automatically_emitted_work": True})
                except queue.Empty:
                    emit({"resume_automatically_emitted_work": False, "observation_seconds": 8})
                session.prompt("Did a previous user request say Reply exactly SECOND? Reply YES or NO only.")
                result = session.until(lambda x: x.get("type") == "result")
                emit({"resumed_answer": result.get("result")})
                return
            third = session.prompt("Reply exactly THIRD.")
            if mode == "interrupt":
                session.control("interrupt", {"subtype": "interrupt", "cancel_queued": True})
                session.until(lambda x: x.get("type") == "control_response" and x.get("response", {}).get("request_id") == "interrupt")
                return
            target = third if mode == "cancel" else first
            session.control("cancel", {"subtype": "cancel_async_message", "message_uuid": target})
            session.until(lambda x: x.get("type") == "control_response" and x.get("response", {}).get("request_id") == "cancel")
            last = second if mode == "cancel" else third
            session.until(lambda x: x.get("command_uuid") == last and x.get("state") in ("completed", "failed", "cancelled"))
        finally:
            session.close()


if __name__ == "__main__":
    main()
