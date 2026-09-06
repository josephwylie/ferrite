#!/usr/bin/env python3
"""Probe installed Codex's native queue with an isolated home and local fake model.
No credentials or production sessions; no model tools are requested.
Usage: python3 scripts/probe-native-codex-queue.py > results.json
"""
import struct
import zlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Model(BaseHTTPRequestHandler):
    gate = threading.Event()
    requests = 0
    lock = threading.Lock()

    def log_message(self, *args):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length', 0)))
        with self.lock:
            type(self).requests += 1
            n = type(self).requests
        if n == 1:
            self.gate.wait(25)
        events = [
            {'type': 'response.created', 'response': {'id': f'resp-{n}'}},
            {'type': 'response.output_item.done', 'output_index': 0, 'item': {
                'id': f'msg-{n}', 'type': 'message', 'role': 'assistant',
                'content': [{'type': 'output_text', 'text': 'probe complete'}]}},
            {'type': 'response.completed', 'response': {'id': f'resp-{n}',
                'usage': {'input_tokens': 1, 'output_tokens': 1, 'total_tokens': 2}}},
        ]
        data = ''.join(f'event: {e["type"]}\ndata: {json.dumps(e)}\n\n' for e in events).encode()
        try:
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


class Server:
    def __init__(self, home, experimental=True):
        env = dict(os.environ, CODEX_HOME=str(home))
        # Ensure no API credentials are forwarded to the local mock.
        for key in list(env):
            if 'API_KEY' in key or key in ('OPENAI_ACCESS_TOKEN', 'CODEX_AUTH_JSON'):
                env.pop(key)
        self.proc = subprocess.Popen(['codex', 'app-server'], cwd=home, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.messages = []
        self.cv = threading.Condition()
        self.counter = 0
        threading.Thread(target=self.read, daemon=True).start()
        self.request('initialize', {'clientInfo': {'name': 'ferrite-queue-probe', 'version': '0.1'},
            'capabilities': {'experimentalApi': experimental}})
        self.write({'method': 'initialized'})

    def read(self):
        for line in self.proc.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                continue
            with self.cv:
                self.messages.append(message)
                self.cv.notify_all()

    def write(self, packet):
        self.proc.stdin.write(json.dumps(packet) + '\n')
        self.proc.stdin.flush()

    def wait(self, predicate, timeout=20):
        deadline = time.monotonic() + timeout
        with self.cv:
            while True:
                for packet in self.messages:
                    if predicate(packet):
                        return packet
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(self.messages[-5:])
                self.cv.wait(remaining)

    def request(self, method, params, allow_error=False):
        self.counter += 1
        request_id = self.counter
        self.write({'id': request_id, 'method': method, 'params': params})
        reply = self.wait(lambda p: p.get('id') == request_id)
        if 'error' in reply and not allow_error:
            raise RuntimeError(method, reply)
        return reply.get('error') if 'error' in reply else reply['result']

    def close(self, kill=False):
        if kill:
            self.proc.kill()
        else:
            self.proc.terminate()
        try:
            self.proc.wait(5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def run(crash=False):
    result = {'crash': crash, 'version': subprocess.check_output(['codex', '--version'], text=True).strip()}
    http = ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=http.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='ferrite-native-queue-') as path:
        home = Path(path)
        (home / 'config.toml').write_text(f'''
model = "gpt-5.4"
model_provider = "queue_probe"
approval_policy = "never"
sandbox_mode = "read-only"
[model_providers.queue_probe]
name = "Local queue probe"
base_url = "http://127.0.0.1:{http.server_port}/v1"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0
''')
        s = Server(home, experimental=False)
        try:
            tid = s.request('thread/start', {'cwd': path})['thread']['id']
            result['without_experimental'] = s.request('thread/queue/list', {'threadId': tid}, True)
        finally:
            s.close()
        s = Server(home)
        try:
            tid = s.request('thread/start', {'cwd': path})['thread']['id']
            turn = s.request('turn/start', {'threadId': tid, 'input': [{'type': 'text', 'text': 'first'}]})['turn']['id']
            s.wait(lambda p: p.get('method') == 'turn/started')
            def add(text, client=None, inputs=None):
                return s.request('thread/queue/add', {'threadId': tid,
                    'input': inputs or [{'type': 'text', 'text': text}],
                    'clientUserMessageId': client or str(uuid.uuid4())})['queuedSubmission']
            def listed():
                return s.request('thread/queue/list', {'threadId': tid})['data']
            a = add('second')
            b = add('third')
            c = add('delete me')
            result['while_busy'] = [x['input'] for x in listed()]
            result['busy_start'] = s.request('thread/queue/start', {'threadId': tid}, True)
            updated = s.request('thread/queue/update', {'threadId': tid, 'queuedSubmissionId': a['id'],
                'input': [{'type': 'text', 'text': 'second edited'}]})['queuedSubmission']
            result['update_preserves_id'] = updated['id'] == a['id']
            result['deleted'] = s.request('thread/queue/delete', {'threadId': tid, 'queuedSubmissionId': c['id']})
            s.request('thread/queue/reorder', {'threadId': tid, 'queuedSubmissionIds': [b['id'], a['id']]})
            result['reordered'] = [x['input'] for x in listed()]
            result['delete_again'] = s.request('thread/queue/delete', {'threadId': tid, 'queuedSubmissionId': c['id']}, True)
            result['invalid_reorder'] = s.request('thread/queue/reorder', {'threadId': tid, 'queuedSubmissionIds': [a['id']]}, True)
            image = home / 'one.png'
            def chunk(kind, data):
                return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
            image.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', 1, 1, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\0\xff\0\0')) + chunk(b'IEND', b''))
            image_item = add('', inputs=[{'type': 'localImage', 'path': str(image)}])
            image.unlink()
            result['attachment_snapshot_type'] = image_item['input'][0]['type']
            result['attachment_snapshot_is_data_url'] = image_item['input'][0].get('url', '').startswith('data:')
            s.request('thread/queue/delete', {'threadId': tid, 'queuedSubmissionId': image_item['id']})
            duplicate1 = add('duplicate probe', client='same-client-id')
            duplicate2 = add('duplicate probe', client='same-client-id')
            result['same_client_id_deduplicates'] = duplicate1['id'] == duplicate2['id']
            for item_id in {duplicate1['id'], duplicate2['id']}:
                s.request('thread/queue/delete', {'threadId': tid, 'queuedSubmissionId': item_id})
            result['invalid_attachment'] = s.request('thread/queue/add', {'threadId': tid,
                'clientUserMessageId': str(uuid.uuid4()), 'input': [{'type': 'localImage', 'path': str(home/'missing.png')}]}, True)
            # Provider interrupt should keep the native queue paused and durable.
            s.request('turn/interrupt', {'threadId': tid, 'turnId': turn})
            s.wait(lambda p: p.get('method') == 'turn/completed' and p['params']['turn']['id'] == turn)
            result['after_interrupt'] = [x['input'] for x in listed()]
            # Stop the process with queued entries still owned by Codex.
        finally:
            s.close(kill=crash)
            Model.gate.set()
        s = Server(home)
        try:
            result['after_process_restart'] = s.request('thread/queue/list', {'threadId': tid})['data']
            resume = s.request('thread/resume', {'threadId': tid, 'cwd': path}, True)
            if 'code' in resume:
                result['resume_error'] = resume
                http.shutdown()
                return result
            # An interrupted thread may remain paused on resume. Observe first,
            # then explicitly start its head; later entries must dispatch natively.
            time.sleep(2)
            result['after_resume_before_start'] = s.request('thread/queue/list', {'threadId': tid})['data']
            result['explicit_start_after_interrupt'] = s.request('thread/queue/start', {'threadId': tid}, True)
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                turns = [p['params']['turn'] for p in s.messages if p.get('method') == 'turn/completed']
                if len(turns) >= 2:
                    break
                time.sleep(.05)
            result['turns_after_start'] = [{'id': t['id'], 'status': t['status']} for t in turns]
            result['after_auto_drain'] = s.request('thread/queue/list', {'threadId': tid})['data']
            result['queue_change_notifications'] = sum(p.get('method') == 'thread/queue/changed' for p in s.messages)
            result['user_items'] = [p['params']['item'] for p in s.messages
                if p.get('method') == 'item/completed' and p['params']['item'].get('type') == 'userMessage']
        finally:
            s.close()
    http.shutdown()
    return result


if __name__ == '__main__':
    print(json.dumps(run(crash='--crash' in sys.argv), indent=2))
