#!/usr/bin/env python3
"""Run one real packaged Cockpit window against an isolated local page."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import struct
import tempfile
import threading
import shlex
import sys
import uuid
import time


def windows_descendants(roots):
    """Account for owned children even after their bootstrap parent exits."""
    query = "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name | ConvertTo-Json -Compress"
    raw = subprocess.check_output(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", query],
        text=True, encoding="utf-8", timeout=20, creationflags=subprocess.CREATE_NO_WINDOW)
    processes = json.loads(raw.lstrip("\ufeff"))
    if isinstance(processes, dict):
        processes = [processes]
    owned = set(roots)
    while True:
        expanded = owned | {int(item["ProcessId"]) for item in processes if int(item["ParentProcessId"]) in owned}
        if expanded == owned:
            return [item for item in processes if int(item["ProcessId"]) in owned]
        owned = expanded


def pe_stack(path):
    with path.open('rb') as executable:
        executable.seek(0x3c)
        header = struct.unpack('<I', executable.read(4))[0]
        executable.seek(header)
        if executable.read(4) != b'PE\0\0':
            raise ValueError(f'Not a PE executable: {path}')
        optional = header + 24
        executable.seek(optional)
        magic = struct.unpack('<H', executable.read(2))[0]
        if magic not in (0x10b, 0x20b):
            raise ValueError(f'Unknown PE optional header: {magic:#x}')
        executable.seek(optional + 72)
        size = 8 if magic == 0x20b else 4
        reserve = int.from_bytes(executable.read(size), 'little')
        commit = int.from_bytes(executable.read(size), 'little')
        return {'path':str(path), 'stack_reserve':reserve, 'stack_commit':commit}

PAGE = b'''<!doctype html><meta charset="utf-8"><style>
body{margin:0;background:white}button{display:block;margin:40px;width:240px;height:160px;
border:0;background:rgb(213,37,59);color:white;font:24px sans-serif}
</style><button onclick="fetch('/clicked',{method:'POST'}).then(()=>{this.dataset.clicks=Number(this.dataset.clicks||0)+1;this.style.background=this.dataset.clicks==='1'?'rgb(23,173,91)':'rgb(29,97,219)'})">Window fixture</button>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--provider', choices=['claude'])
    parser.add_argument('--compare-executable', type=Path, help='retain PE header diagnostics for a matching naked Windows build')
    parser.add_argument('--startup-only', action='store_true', help='diagnose native startup only; never browser acceptance')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    if os.name == 'nt':
        headers = [pe_stack(args.executable.resolve())]
        if args.compare_executable:
            headers.append(pe_stack(args.compare_executable.resolve()))
        (output/'pe-headers.json').write_text(json.dumps(headers, indent=2), encoding='utf-8')
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
        (root/'settings.json').write_text(json.dumps({'check_for_updates':False, 'start_at_login':False, 'auto_update_clis':False}), encoding='utf-8')
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
        server_log = (output/'server-process.log').open('w', encoding='utf-8')
        flags = subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
        server_process = subprocess.Popen([executable, 'server'], env=env, cwd=root,
            stdout=server_log, stderr=subprocess.STDOUT, creationflags=flags)
        descendants = []
        try:
            with (output/'app.log').open('w', encoding='utf-8') as log:
                process = subprocess.Popen([executable, '--browser-window-smoke', '--import', str(history)],
                    env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT, creationflags=flags)
                if args.startup_only:
                    deadline = time.monotonic() + 15
                    while process.poll() is None and not (output/'window-start.json').exists() and time.monotonic() < deadline:
                        time.sleep(0.1)
                    if (output/'window-start.json').exists() and process.poll() is None:
                        time.sleep(2)
                    (output/'startup-comparison.json').write_text(json.dumps({
                        'diagnostic_only':True, 'browser_acceptance':False,
                        'driver_reached':(output/'window-start.json').exists(),
                        'exit_code':process.poll(), 'pid':process.pid,
                        'alive_after_settle':process.poll() is None,
                    }, indent=2), encoding='utf-8')
                    return
                try:
                    code = process.wait(timeout=90)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise RuntimeError('Packaged window did not complete its fixture within 90s')
            (output/'process-exit.json').write_text(json.dumps({'pid':process.pid, 'exit_code':code, 'exit_hex':hex(code & 0xffffffff), 'server_pid':server_process.pid}), encoding='utf-8')
            if os.name == 'nt':
                descendants = windows_descendants([process.pid, server_process.pid])
                (output/'owned-processes.json').write_text(json.dumps(descendants, indent=2), encoding='utf-8')
            if not (output/'window-result.json').exists():
                raise RuntimeError(f'Packaged app exited before the window scenario result: exit={code} ({code & 0xffffffff:#x}); inspect app.log and startup evidence')
            result = json.loads((output/'window-result.json').read_text(encoding='utf-8'))
            if code != 0 or result.get('success') is not True:
                raise RuntimeError(f'Packaged window failed: exit={code}, result={result}')
            if args.provider:
                peer_result = json.loads((output/'provider-result.json').read_text(encoding='utf-8'))
                if peer_result.get('probe') != env['FERRITE_BROWSER_WINDOW_PROBE'] or peer_result.get('provider') != args.provider:
                    raise RuntimeError('The intended isolated provider peer did not produce the response')
                if peer_result.get('success') is not True:
                    raise RuntimeError(f'Actual provider browser Observe failed: {peer_result}')
            expected_clicks = 2 if args.provider else 1
            if len(clicked) != expected_clicks:
                raise RuntimeError(f'Independent fixture oracle saw {len(clicked)} clicks, expected {expected_clicks}')
            screenshots = ['window-before.png', 'window-after.png']
            if args.provider:
                screenshots.append('window-provider.png')
            for name in screenshots:
                if (output/name).stat().st_size < 100:
                    raise RuntimeError(f'Missing window screenshot: {name}')
            (output/'oracle.json').write_text(json.dumps({'clicks':len(clicked),
                'elapsed_seconds':time.monotonic()-started, 'window_result':result}, indent=2))
        finally:
            # Stop this launch's actual Server before deleting its isolated store.
            # Both handles belong exclusively to this isolated probe.
            if os.name == 'nt' and process is not None and not descendants:
                descendants = windows_descendants([process.pid, server_process.pid])
                (output/'owned-processes.json').write_text(json.dumps(descendants, indent=2), encoding='utf-8')
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            stopped = subprocess.run([executable, '--browser-window-stop'], env=env, cwd=root,
                stdout=server_log, stderr=subprocess.STDOUT, timeout=20, creationflags=flags)
            cleanup_error = None
            if stopped.returncode != 0:
                cleanup_error = f'Isolated Server stop failed: exit={stopped.returncode}'
            try:
                server_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server_process.terminate()
                server_process.wait(timeout=10)
                cleanup_error = 'Isolated Server required forced termination'
            server_log.close()
            if os.name == 'nt':
                for child in descendants:
                    pid = int(child['ProcessId'])
                    if pid not in (process.pid if process else None, server_process.pid, os.getpid()):
                        subprocess.run(['taskkill.exe','/PID',str(pid),'/T','/F'],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                            timeout=10, creationflags=flags, check=False)
                query = "Get-WinEvent -FilterHashtable @{LogName='Application'; Id=1000,1001; StartTime=(Get-Date).AddMinutes(-5)} -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'ferrite' } | Select-Object -First 10 TimeCreated,Id,Message | ConvertTo-Json -Depth 3"
                with (output/'windows-crash-events.json').open('w', encoding='utf-8') as events:
                    subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-Command',query],
                        stdout=events, stderr=subprocess.STDOUT, timeout=20,
                        creationflags=flags, check=False)
            import shutil
            if (root/'logs').exists():
                shutil.copytree(root/'logs', output/'diagnostics', dirs_exist_ok=True)
            server.shutdown()
            if cleanup_error:
                (output/'cleanup-error.txt').write_text(cleanup_error, encoding='utf-8')
                if sys.exc_info()[0] is None:
                    raise RuntimeError(cleanup_error)


if __name__ == '__main__':
    main()
