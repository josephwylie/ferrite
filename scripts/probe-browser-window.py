#!/usr/bin/env python3
"""Run one real packaged Cockpit window against an isolated local page."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import struct
import signal
import tempfile
import threading
import shlex
import sys
import uuid
import hashlib
import urllib.request
import urllib.parse
import platform
from datetime import datetime, timezone
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


def unix_group_processes(group):
    sample_started = time.monotonic()
    output = subprocess.check_output(['ps', '-axww', '-o', 'pid=,ppid=,pgid=,state=,etime=,comm='], text=True, timeout=15)
    processes = []
    for line in output.splitlines():
        pid, parent, process_group, state, elapsed, executable = line.strip().split(None, 5)
        if int(process_group) == group:
            processes.append({'pid':int(pid), 'parent_pid':int(parent), 'process_group':int(process_group),
                'state':state, 'elapsed_process_time':elapsed, 'executable':executable,
                'sample_started_monotonic':sample_started})
    return processes


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
    parser.add_argument('--work-orders', action='store_true', help='complete one real work order through the provider')
    parser.add_argument('--compare-executable', type=Path, help='retain PE header diagnostics for a matching naked Windows build')
    parser.add_argument('--startup-only', action='store_true', help='diagnose native startup only; never browser acceptance')
    args = parser.parse_args()
    if args.work_orders and args.provider != 'claude':
        parser.error('--work-orders currently requires --provider claude')
    args.output.mkdir(parents=True, exist_ok=True)
    output = args.output.resolve()
    # Reusing an artifact directory must not reuse a previous run's completion
    # or startup marker, including directories restored by compiler caches.
    for name in ('window-start.json', 'input-ready.json', 'window-result.json', 'provider-result.json',
                 'provider-result.tmp', 'provider-trace.json', 'provider-definitions.json',
                 'provider-host.jsonl', 'startup-comparison.json', 'oracle.json',
                 'work-order-oracle.json', 'work-order-oracle-at-exit.json', 'work-order-trace.json',
                 'window-work-order.png', 'window-work-order-before.png',
                 'cleanup-error.txt', 'cleanup-processes.json', 'shutdown-samples.json', 'oracle-at-exit.json', 'window-before.png', 'window-after.png',
                 'window-provider.png', 'window-current.png'):
        (output/name).unlink(missing_ok=True)
    with args.executable.open('rb') as binary:
        executable_digest = hashlib.file_digest(binary, 'sha256').hexdigest()
    components = []
    for component in (args.executable.resolve(), args.executable.resolve().with_name('ferrite.dll'),
                      args.executable.resolve().with_name('agent-browser.exe' if os.name == 'nt' else 'agent-browser')):
        if component.is_file():
            with component.open('rb') as binary:
                components.append({'path':str(component), 'sha256':hashlib.file_digest(binary, 'sha256').hexdigest()})
    controller_metadata = args.executable.resolve().with_name('agent-browser-build.json')
    if sys.platform == 'darwin':
        controller_metadata = args.executable.resolve().parents[1]/'Resources'/'agent-browser'/'agent-browser-build.json'
    native_build = json.loads(controller_metadata.read_text()) if controller_metadata.is_file() else None
    (output/'provenance.json').write_text(json.dumps({
        'started_utc':datetime.now(timezone.utc).isoformat(),
        'executable':str(args.executable.resolve()), 'sha256':executable_digest,
        'application_components':components, 'native_build':native_build,
        'platform':platform.platform(), 'architecture':platform.machine(),
        'provider':args.provider, 'work_orders':args.work_orders, 'diagnostic_only':args.startup_only,
        'profile_mode':'isolated temporary production profile',
    }, indent=2), encoding='utf-8')
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
        if args.work_orders:
            env['FERRITE_BROWSER_WINDOW_SCENARIO'] = 'work-orders'
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
        work_fixture = None
        shutdown_samples = []
        try:
            if args.work_orders:
                ready = root/'fixture-ready.json'
                fixture_script = Path(__file__).parent/'bench'/'browser-controllers'/'fixture-server.mjs'
                fixture_env = {**env, 'FIXTURE_READY_FILE':str(ready)}
                work_fixture = subprocess.Popen(['node',str(fixture_script.resolve())], env=fixture_env,
                    stdout=server_log, stderr=subprocess.STDOUT, creationflags=flags)
                deadline = time.monotonic()+10
                while not ready.exists():
                    if work_fixture.poll() is not None or time.monotonic()>deadline:
                        raise RuntimeError('Standalone work-order fixture did not become ready')
                    time.sleep(0.05)
                url = json.loads(ready.read_text())['url']
                env['FERRITE_BROWSER_WINDOW_URL'] = url
            with (output/'app.log').open('w', encoding='utf-8') as log:
                process = subprocess.Popen([executable, '--browser-window-smoke', '--import', str(history)],
                    env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT, creationflags=flags,
                    start_new_session=os.name != "nt")
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
                    code = process.wait(timeout=210 if args.work_orders else 90)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    raise RuntimeError('Packaged window did not complete its fixture within 90s')
            (output/'process-exit.json').write_text(json.dumps({'pid':process.pid, 'exit_code':code, 'exit_hex':hex(code & 0xffffffff), 'server_pid':server_process.pid}), encoding='utf-8')
            if os.name == 'nt':
                descendants = windows_descendants([process.pid, server_process.pid])
                (output/'owned-processes.json').write_text(json.dumps(descendants, indent=2), encoding='utf-8')
                surviving_controllers = [item for item in descendants if item['Name'].casefold() == 'agent-browser.exe']
            else:
                remaining = unix_group_processes(process.pid)
                shutdown_samples.append({'stage':'after-app-wait','seconds_since_launch':time.monotonic()-started,'processes':remaining})
                (output/'owned-processes.json').write_text(json.dumps(remaining, indent=2), encoding='utf-8')
                surviving_controllers = [item for item in remaining if Path(item['executable']).name == 'agent-browser']
            if surviving_controllers:
                raise RuntimeError(f'Native controller survived ordinary application exit: {surviving_controllers}')
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
            if args.work_orders:
                task = peer_result['task_name']
                with urllib.request.urlopen(url+'oracle?name='+urllib.parse.quote(task), timeout=5) as response:
                    persisted = json.load(response)
                with urllib.request.urlopen(url+'audit', timeout=5) as response:
                    audit = json.load(response)
                expected = {'name':task,'email':'owner@example.test','description':'Reviewed and approved.',
                    'team':'Engineering','priority':'Normal','budget':'3200','customName':'Tracking code',
                    'customValue':'final-0','notify':False,'approved':True}
                (output/'work-order-oracle.json').write_text(json.dumps({'record':persisted,'audit':audit},indent=2),encoding='utf-8')
                for key,value in expected.items():
                    if persisted.get(key) != value:
                        raise RuntimeError(f'Independent work-order oracle mismatch {key}: {persisted}')
                if len([entry for entry in audit if entry['name']==task]) != 2:
                    raise RuntimeError('Independent audit did not record exactly two saves')
            expected_clicks = 0 if args.work_orders else (2 if args.provider else 1)
            if len(clicked) != expected_clicks:
                raise RuntimeError(f'Independent fixture oracle saw {len(clicked)} clicks, expected {expected_clicks}')
            screenshots = ['window-work-order.png'] if args.work_orders else ['window-before.png', 'window-after.png']
            if args.provider and not args.work_orders:
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
            if os.name != 'nt' and process is not None:
                settle_started = time.monotonic()
                while True:
                    remaining = unix_group_processes(process.pid)
                    shutdown_samples.append({'stage':'before-forced-cleanup','seconds_since_launch':time.monotonic()-started,
                        'settle_seconds':time.monotonic()-settle_started,'processes':remaining})
                    if not remaining or time.monotonic()-settle_started >= 2:
                        break
                    time.sleep(0.2)
                (output/'shutdown-samples.json').write_text(json.dumps(shutdown_samples,indent=2),encoding='utf-8')
                if remaining:
                    (output/'cleanup-processes.json').write_text(json.dumps(remaining, indent=2), encoding='utf-8')
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
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
            if work_fixture is not None:
                try:
                    with urllib.request.urlopen(url+'api/records', timeout=5) as response:
                        records = json.load(response)
                    with urllib.request.urlopen(url+'audit', timeout=5) as response:
                        audit = json.load(response)
                    (output/'work-order-oracle-at-exit.json').write_text(json.dumps({'records':records,'audit':audit},indent=2),encoding='utf-8')
                finally:
                    work_fixture.terminate()
                    work_fixture.wait(timeout=10)
            server.shutdown()
            # Retain independent page activation even when rendering/input or
            # provider setup fails before the success-only oracle is written.
            (output/'oracle-at-exit.json').write_text(json.dumps({
                'clicks':len(clicked), 'click_seconds':[stamp-started for stamp in clicked],
                'elapsed_seconds':time.monotonic()-started,
            }, indent=2), encoding='utf-8')
            if cleanup_error:
                (output/'cleanup-error.txt').write_text(cleanup_error, encoding='utf-8')
                if sys.exc_info()[0] is None:
                    raise RuntimeError(cleanup_error)


if __name__ == '__main__':
    main()
