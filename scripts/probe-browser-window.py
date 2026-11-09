#!/usr/bin/env python3
"""Run one real packaged Cockpit window against an isolated local page."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import shlex
import sys
import uuid
import time

PAGE = b'''<!doctype html><meta charset="utf-8"><style>
body{margin:0;background:white}button{display:block;margin:40px;width:240px;height:160px;
border:0;background:rgb(213,37,59);color:white;font:24px sans-serif}
</style><button onclick="this.style.background='rgb(23,173,91)';fetch('/clicked',{method:'POST'})">Window fixture</button>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--provider', choices=['claude'])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    clicked = []

    class Fixture(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.send_header('Content-Type', 'text/html')
            self.end_headers()
            self.wfile.write(PAGE)

        def do_POST(self):
            if self.path != '/clicked':
                self.send_error(404)
                return
            clicked.append(time.monotonic())
            self.send_response(204)
            self.end_headers()

        def log_message(self, *_):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{server.server_port}/'
    with tempfile.TemporaryDirectory(prefix='ferrite-window-') as temporary:
        root = Path(temporary)
        (root/'settings.json').write_text(json.dumps({'check_for_updates':False, 'start_at_login':False}), encoding='utf-8')
        env = os.environ.copy()
        env.update(FERRITE_STORE=str(root/'threads'), FERRITE_BROWSER_PROFILE=str(root/'profile'),
                   FERRITE_PROJECTS_BIN=str(root/'no-projects'), TERM='dumb',
                   FERRITE_BROWSER_WINDOW_URL=url, FERRITE_BROWSER_WINDOW_OUTPUT=str(output))
        if args.provider:
            peer = Path(__file__).with_name('browser-window-provider.py').resolve()
            binaries = root/'bin'
            binaries.mkdir()
            if os.name == 'nt':
                shim = binaries/(args.provider+'.cmd')
                shim.write_text(f'@echo off\n"{sys.executable}" "{peer}" --provider {args.provider} %*\n')
            else:
                shim = binaries/args.provider
                shim.write_text(f'#!/bin/sh\nexec {shlex.quote(sys.executable)} {shlex.quote(str(peer))} --provider {args.provider} "$@"\n')
                shim.chmod(0o755)
            env.update(PATH=str(binaries)+os.pathsep+env.get('PATH',''),
                FERRITE_BROWSER_WINDOW_PROVIDER=args.provider,
                FERRITE_BROWSER_WINDOW_PROBE=uuid.uuid4().hex)
        # An imported, idle Thread exercises the real Server without calling a model.
        history = root/'thread.jsonl'
        history.write_text(json.dumps({'type':'user','sessionId':'browser-window-fixture',
            'cwd':str(root),'message':{'role':'user','content':'Packaged window fixture'},
            'uuid':'browser-window-prompt','timestamp':'2026-11-09T00:00:00Z'})+'\n'+
            json.dumps({'type':'assistant','sessionId':'browser-window-fixture',
            'message':{'model':'fixture','role':'assistant','content':[{'type':'text','text':'Ready.'}],
                       'stop_reason':'end_turn'},'uuid':'browser-window-answer',
            'timestamp':'2026-11-09T00:00:01Z'})+'\n', encoding='utf-8')
        executable = str(args.executable.resolve())
        started = time.monotonic()
        process = None
        server_process = subprocess.Popen([executable, 'server'], env=env, cwd=root,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            with (output/'app.log').open('w', encoding='utf-8') as log:
                process = subprocess.Popen([executable, '--browser-window-smoke', '--import', str(history)],
                    env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT)
                try:
                    code = process.wait(timeout=90)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise RuntimeError('Packaged window did not complete its fixture within 90s')
            result = json.loads((output/'window-result.json').read_text(encoding='utf-8'))
            if code != 0 or result.get('success') is not True:
                raise RuntimeError(f'Packaged window failed: exit={code}, result={result}')
            if args.provider:
                peer_result = json.loads((output/'provider-result.json').read_text(encoding='utf-8'))
                if peer_result.get('probe') != env['FERRITE_BROWSER_WINDOW_PROBE'] or peer_result.get('provider') != args.provider:
                    raise RuntimeError('The intended isolated provider peer did not produce the response')
                if peer_result.get('success') is not True:
                    raise RuntimeError(f'Actual provider browser Observe failed: {peer_result}')
            if len(clicked) != 1:
                raise RuntimeError(f'Independent fixture oracle saw {len(clicked)} clicks, expected one')
            for name in ('window-before.png', 'window-after.png'):
                if (output/name).stat().st_size < 100:
                    raise RuntimeError(f'Missing window screenshot: {name}')
            (output/'oracle.json').write_text(json.dumps({'clicks':len(clicked),
                'elapsed_seconds':time.monotonic()-started, 'window_result':result}, indent=2))
        finally:
            # Stop this launch's actual Server before deleting its isolated store.
            # Both handles belong exclusively to this isolated probe.
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            if server_process.poll() is None:
                server_process.terminate()
                server_process.wait(timeout=20)
            import shutil
            if (root/'logs').exists():
                shutil.copytree(root/'logs', output/'diagnostics', dirs_exist_ok=True)
            server.shutdown()


if __name__ == '__main__':
    main()
