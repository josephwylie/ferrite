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
import time

PAGE = b'''<!doctype html><meta charset="utf-8"><style>
body{margin:0;background:white}button{display:block;margin:40px;width:240px;height:160px;
border:0;background:rgb(213,37,59);color:white;font:24px sans-serif}
</style><button onclick="this.style.background='rgb(23,173,91)';fetch('/clicked',{method:'POST'})">Window fixture</button>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--output', type=Path, required=True)
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
        env = os.environ.copy()
        env.update(FERRITE_STORE=str(root/'threads'), FERRITE_BROWSER_PROFILE=str(root/'profile'),
                   FERRITE_PROJECTS_BIN=str(root/'no-projects'), TERM='dumb',
                   FERRITE_BROWSER_WINDOW_URL=url, FERRITE_BROWSER_WINDOW_OUTPUT=str(output))
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
            if len(clicked) != 1:
                raise RuntimeError(f'Independent fixture oracle saw {len(clicked)} clicks, expected one')
            for name in ('window-before.png', 'window-after.png'):
                if (output/name).stat().st_size < 100:
                    raise RuntimeError(f'Missing window screenshot: {name}')
            (output/'oracle.json').write_text(json.dumps({'clicks':len(clicked),
                'elapsed_seconds':time.monotonic()-started, 'window_result':result}, indent=2))
        finally:
            # Stop this launch's actual Server before deleting its isolated store.
            subprocess.run([executable, 'server', 'stop'], env=env, cwd=root,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
            server.shutdown()


if __name__ == '__main__':
    main()
