#!/usr/bin/env python3
"""Deterministic external CLI peer; exercises Ferrite's real provider adapters."""
import json
import os
from pathlib import Path
import sys

provider = sys.argv[sys.argv.index('--provider') + 1]
if '--version' in sys.argv:
    print('2.1.318 (Claude Code)' if provider == 'claude' else 'codex-cli 0.167.0')
    sys.exit(0)

output = Path(os.environ['FERRITE_BROWSER_WINDOW_OUTPUT'])
probe = os.environ['FERRITE_BROWSER_WINDOW_PROBE']


def emit(value):
    print(json.dumps(value, separators=(',', ':')), flush=True)


def call(name, arguments):
    if provider == 'claude':
        emit({'type':'control_request','request_id':name,'request':{
            'subtype':'mcp_message','server_name':'ferrite','message':{
                'jsonrpc':'2.0','id':name,'method':'tools/call',
                'params':{'name':'browser','arguments':arguments}}}})
    else:
        emit({'jsonrpc':'2.0','id':name,'method':'item/tool/call','params':{
            'threadId':'browser-window-peer','namespace':'ferrite','tool':'browser',
            'callId':name,'arguments':arguments}})


def record(answer):
    # Only an actual tool response can produce this evidence. Discovery/version
    # processes cannot claim the Session reached the browser boundary.
    result = {'probe':probe,'provider':provider,'peer_pid':os.getpid(),
              'success':answer.get('isError') is False if provider == 'claude'
                        else answer.get('success') is True,
              'answer':answer}
    temporary = output/'provider-result.tmp'
    temporary.write_text(json.dumps(result, indent=2), encoding='utf-8')
    temporary.replace(output/'provider-result.json')


started = False
for line in sys.stdin:
    try:
        frame = json.loads(line)
    except json.JSONDecodeError:
        continue
    with (output/'provider-host.jsonl').open('a', encoding='utf-8') as log:
        log.write(json.dumps({'probe':probe,'provider':provider,'frame':frame})+'\n')
    if provider == 'claude':
        if frame.get('type') == 'control_request':
            emit({'type':'control_response','response':{'subtype':'success',
                'request_id':frame['request_id'],'response':{}}})
            continue
        if frame.get('type') == 'user' and not started:
            started = True
            call('browser-state', {'kind':'state'})
        response = frame.get('response', {})
        name = response.get('request_id')
        answer = response.get('response', {}).get('mcp_response', {}).get('result')
        items = (answer or {}).get('content', [])
    else:
        method = frame.get('method')
        if method in ('initialize','skills/list','model/list','thread/start','thread/resume','turn/start'):
            result = {}
            if method in ('skills/list','model/list'):
                result = {'data':[]}
            elif method in ('thread/start','thread/resume'):
                result = {'thread':{'id':'browser-window-peer'},'model':'fixture'}
            elif method == 'turn/start':
                result = {'turn':{'id':'browser-window-turn'}}
            emit({'jsonrpc':'2.0','id':frame['id'],'result':result})
            if method == 'turn/start' and not started:
                started = True
                call('browser-state', {'kind':'state'})
            continue
        name = frame.get('id')
        answer = frame.get('result')
        items = (answer or {}).get('contentItems', [])
    if name == 'browser-state' and answer:
        try:
            state = json.loads(next(item['text'] for item in items if 'text' in item))
            active = next(tab for tab in state['state']['tabs'] if tab['id'] == state['state']['active'])
            call('browser-observe', {'kind':'control',
                'document':{'tab':active['id'],'generation':active['generation']},
                'revision':state['revision'],'control':{'kind':'observe'}})
        except (KeyError, StopIteration, ValueError) as error:
            record({'isError':True,'success':False,'state_answer':answer,'error':str(error)})
    elif name == 'browser-observe' and answer:
        record(answer)
