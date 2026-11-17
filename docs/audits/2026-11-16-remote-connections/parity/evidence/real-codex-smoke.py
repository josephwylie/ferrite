#!/usr/bin/env python3
"""One real Codex turn through native Ferrite over isolated localhost SSH.
Preparation and syntax checking do not launch providers. Only `verify` does.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import threading
import time
import uuid

REPO = Path('/Users/josephwylie/Desktop/Projects/ferrite-remote-connections')
SPEC = importlib.util.spec_from_file_location('parity', REPO / 'scripts/verify-remote-parity.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
B = P.BASE
CODEX = Path('/Users/josephwylie/.nvm/versions/node/v25.9.0/bin/codex')
NODE = CODEX.parent / 'node'


def launcher(output, arguments):
    state = json.loads((output / 'state.json').read_text())
    if '--version' in arguments:
        os.execv(str(NODE), [str(NODE), str(CODEX), *arguments])
    child = subprocess.Popen([str(NODE), str(CODEX), *arguments], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, start_new_session=True)
    identity = B.identity(child.pid)
    B.record(output, 'real-provider-start', pid=child.pid, identity=identity,
             executable=str(CODEX), node=str(NODE), arguments=arguments,
             cwd=os.getcwd(), store=os.environ.get('FERRITE_STORE'))
    def stop(*_):
        if child.poll() is None and B.identity(child.pid) == identity:
            os.killpg(child.pid, signal.SIGTERM)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    def requests():
        try:
            for line in sys.stdin.buffer:
                try:
                    B.record(output, 'real-provider-request', request=json.loads(line))
                except json.JSONDecodeError:
                    pass
                child.stdin.write(line)
                child.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass
        finally:
            child.stdin.close()
    threading.Thread(target=requests, daemon=True).start()
    try:
        for line in child.stdout:
            try:
                B.record(output, 'real-provider-response', response=json.loads(line))
            except json.JSONDecodeError:
                pass
            sys.stdout.buffer.write(line)
            sys.stdout.buffer.flush()
    finally:
        stop()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            if B.identity(child.pid) == identity:
                os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=5)


def verify(args):
    output = args.output.resolve()
    state = P.prepare(output, args.binary.resolve(), args.server.resolve())
    state['provider'] = 'Actual installed OpenAI Codex app-server; no simulated provider for this turn'
    state['verification']['actual_model'] = 'not completed'
    state['smoke_harness_sha256'] = P.digest(__file__)
    root = Path(state['root'])
    # The app-server launcher is explicit; the fixture PATH can never select Claude.
    wrapper = root / 'bin/real-codex'
    B.write(wrapper, '#!/bin/sh\nexec ' + shlex.join([
        sys.executable, str(Path(__file__).resolve()), '--output', str(output), 'launcher'
    ]) + ' "$@"\n', 0o700)
    # Codex's node entry point may spawn helpers that need its installed node.
    (root / 'bin/node').symlink_to(NODE)
    token = 'REMOTE-CODEX-' + uuid.uuid4().hex
    B.write(root / 'remote-project/SMOKE.txt', token + '\n')
    B.save(output, state)
    ui = P.Window(output, state, args.size)
    thread = None
    try:
        B.command(output, [NODE, CODEX, '--version'])
        B.ssh_smoke(output, state)
        B.launch(output, state, args.size)
        ui.wait('board')
        ui.command('key', 'escape')
        ui.settings()
        ui.add_ssh('ferrite-audit')
        ui.wait('connection-menu-1')
        ui.click('connection-menu-1')
        ui.click('connection-open-1')
        ui.remote = True
        ui.wait('board')
        ui.command('key', 'escape')
        P.remote_cli(ui, 'project', 'open', '--cwd', str(root / 'remote-project'))
        start = ['thread', 'start', '--cwd', str(root / 'remote-project'), '--provider', 'codex',
                 '--title', 'Actual Codex SSH smoke', '--ready', '--focus',
                 '--launcher', str(wrapper), '--timeout-ms', '30000']
        for argument in ['--model', args.model, '-c', 'model_reasoning_effort="low"',
                         '--sandbox', 'read-only', '--ask-for-approval', 'never']:
            start.append('--args=' + argument)
        thread = P.thread_id(P.remote_cli(ui, *start))
        ui.wait('composer-block')
        screenshot = ui.shot('real-codex-before-prompt')
        ui.command('attach', str(screenshot))
        ui.wait('tray-image-0')
        ui.wait('attachment-upload-progress', visible=False)
        assert not ui.has('attachment-failed')
        ui.shot('real-codex-attachment-ready')
        ui.field('composer-row', 'Read only SMOKE.txt in the current directory and return its exact token. '
                 'Also briefly describe the attached screenshot. Do not modify any files or inspect '
                 'anything outside this synthetic project. This is a remote connection smoke test.')
        ui.click('composer-send')
        request = P.until(lambda: [r for r in P.rows(output, 'real-provider-request')
                          if r['request'].get('method') == 'turn/start'],
                          'Real Codex did not receive the native prompt', timeout=35)[0]['request']
        images = [i for i in request['params'].get('input', []) if i.get('type') == 'localImage']
        assert images, 'Actual Codex input omitted the uploaded image'
        remote_path = Path(images[0]['path'])
        assert remote_path.is_relative_to(Path(state['remote_store']) / 'attachments')
        assert P.digest(remote_path) == P.digest(screenshot)
        ui.shot('real-codex-running')
        completed = P.until(lambda: [r for r in P.rows(output, 'real-provider-response')
                            if r['response'].get('method') == 'turn/completed'],
                            'Actual Codex turn did not complete within the smoke deadline',
                            timeout=args.timeout)[0]['response']
        assert completed['params']['turn']['status'] == 'completed', completed
        transcript = P.remote_cli(ui, 'agent', 'read', thread, '--lines', '120').stdout
        B.write(output / 'transcript.json', transcript)
        assert token in transcript, 'Model did not return the synthetic file token'
        ui.wait('composer-stop', visible=False)
        ui.shot('real-codex-completed')
        providers = P.rows(output, 'real-provider-start')
        assert providers and all(p['store'] == state['remote_store'] and
                                  p['cwd'] == str(root / 'remote-project') for p in providers)
        assert 'Actual Codex SSH smoke' not in B.cli(output, state, ['thread', 'list']).stdout
        assert any(r.get('ssh_connection') for r in P.rows(output, 'ssh-request'))
        ui.check('actual-codex-ssh-native-chat-image-file-read', model=args.model, thread=thread,
                 remote_image=str(remote_path), image_sha256=P.digest(screenshot),
                 provider_executable=str(CODEX), launcher_wrapper=str(wrapper),
                 remote_store=state['remote_store'], completed=completed)
        state['verification'].update(functional='passed', actual_model='passed: actual installed Codex')
    except Exception as error:
        state['verification'].update(functional='failed', actual_model='failed or incomplete')
        B.record(output, 'verification-failed', error=str(error))
        try:
            ui.shot('failure')
        except Exception as shot_error:
            B.record(output, 'failure-screenshot-unavailable', error=str(shot_error))
        raise
    finally:
        if thread:
            try:
                P.remote_cli(ui, 'thread', 'close', thread)
            except Exception as error:
                B.record(output, 'thread-close-failed', error=str(error))
        B.stop_processes(output, state)
        for process in P.rows(output, 'real-provider-start'):
            if B.identity(process['pid']) == process['identity']:
                os.killpg(process['pid'], signal.SIGTERM)
                try:
                    P.until(lambda: B.identity(process['pid']) != process['identity'],
                            'Actual Codex survived SIGTERM', timeout=5)
                except AssertionError:
                    if B.identity(process['pid']) == process['identity']:
                        os.killpg(process['pid'], signal.SIGKILL)
        owned = state['processes'] + P.rows(output, 'real-provider-start')
        state['verification']['owned_processes_stopped'] = all(
            B.identity(p['pid']) != p['identity'] for p in owned)
        B.save(output, state)
        B.write(output / 'summary.json', json.dumps({
            **state['verification'], 'source_commit': state.get('source_commit'),
            'binary_sha256': state.get('binary_sha256'),
            'smoke_harness_sha256': state['smoke_harness_sha256'],
            'checks': P.rows(output, 'check'), 'screenshots': P.rows(output, 'screenshot'),
            'limitations': ['Actual OpenAI model call using existing Codex sign-in.',
                            'Localhost SSH shares the host filesystem; separate Store and request routing are asserted.',
                            'Explicit task launcher wraps the exact installed Codex executable to record provider ownership.',
                            'Uses existing user Codex configuration except explicit model, reasoning, sandbox and approval overrides.',
                            'Does not exercise Remote link; the deterministic parity E2E covers that transport.',
                            'Visual parity requires separate review.']}, indent=2) + '\n')
    assert state['verification']['owned_processes_stopped']
    print(json.dumps(state['verification']))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=REPO / 'target/debug/ferrite')
    parser.add_argument('--server', type=Path, default=REPO / 'target/debug/ferrited')
    parser.add_argument('--size', default='1470x923')
    parser.add_argument('--model', default='gpt-6-astra')
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('mode', choices=['verify', 'launcher'])
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.mode == 'launcher':
        launcher(args.output.resolve(), args.arguments)
    else:
        verify(args)


if __name__ == '__main__':
    main()
