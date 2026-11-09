#!/usr/bin/env python3
"""Deterministic external CLI peer; exercises Ferrite's real provider adapters."""
import json
import os
from pathlib import Path
import sys
import time
import importlib.util

provider = sys.argv[sys.argv.index('--provider') + 1]
if '--version' in sys.argv:
    print('2.1.318 (Claude Code)' if provider == 'claude' else 'codex-cli 0.167.0')
    sys.exit(0)

output = Path(os.environ['FERRITE_BROWSER_WINDOW_OUTPUT'])
probe = os.environ['FERRITE_BROWSER_WINDOW_PROBE']
work_orders = os.environ.get('FERRITE_BROWSER_WINDOW_SCENARIO') == 'work-orders'
task_name = 'order-'+provider+'-'+probe[:8]+'-0'


def emit(value):
    print(json.dumps(value, separators=(',', ':')), flush=True)


def call(name, arguments):
    with (output/'provider-requests.jsonl').open('a',encoding='utf-8') as log:
        log.write(json.dumps({'sent_monotonic':time.monotonic(),'request':name,'arguments':arguments})+'\n')
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
    content = answer.get('content', []) if provider == 'claude' else answer.get('contentItems', [])
    observed_fixture = any(('Work orders' if work_orders else 'Window fixture') in item.get('text', '') for item in content)
    result = {'probe':probe,'provider':provider,'peer_pid':os.getpid(),
              'observed_fixture':observed_fixture,
              'success':answer.get('isError') is False if provider == 'claude'
                        else answer.get('success') is True,
              'answer':answer, 'task_name':task_name if work_orders else None}
    result['success'] = result['success'] and observed_fixture
    temporary = output/'provider-result.tmp'
    temporary.write_text(json.dumps(result, indent=2), encoding='utf-8')
    temporary.replace(output/'provider-result.json')


started = False
trace = []
receipt_polls = 0
if work_orders:
    spec = importlib.util.spec_from_file_location('work_orders', Path(__file__).with_name('browser-window-work-orders.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    scenario = module.WorkOrder(task_name, call, record, output)
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
            emit({'type':'control_request','request_id':'browser-definitions','request':{
                'subtype':'mcp_message','server_name':'ferrite','message':{
                    'jsonrpc':'2.0','id':'browser-definitions','method':'tools/list','params':{}}}})
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
    if name == 'browser-definitions' and answer is not None:
        tools = {tool['name']:tool for tool in answer.get('tools', [])}
        (output/'provider-definitions.json').write_text(json.dumps(answer, indent=2), encoding='utf-8')
        if not {'browser','update_plan','wrap_up'} <= tools.keys() or not tools['browser'].get('inputSchema'):
            record({'isError':True, 'success':False, 'error':'Browser capability was not advertised beside plan/wrap_up'})
        else:
            scenario.begin() if work_orders else call('browser-state', {'kind':'state'})
    if work_orders and isinstance(name,str) and name.startswith('work-') and answer is not None:
        scenario.receive(name, answer)
        continue
    if name in ('browser-state', 'browser-observe', 'browser-click', 'browser-after') and answer:
        trace.append({'name':name, 'answer':answer, 'task_name':task_name if work_orders else None})
        try:
            if (provider == 'claude' and answer.get('isError') is not False) or (provider != 'claude' and answer.get('success') is not True):
                raise ValueError('Actual provider tool returned an error')
            state = json.loads(next(item['text'] for item in items if 'text' in item))
            active = next(tab for tab in state['state']['tabs'] if tab['id'] == state['state']['active'])
            identity = {'document':{'tab':active['id'],'generation':active['generation']},
                        'revision':state['revision']}
            if name in ('browser-state', 'browser-click'):
                call('browser-observe' if name == 'browser-state' else 'browser-after',
                     {'kind':'control', **identity, 'control':{'kind':'observe'}})
            elif name == 'browser-observe':
                refs = state['value']['refs']
                reference = next(ref for ref, element in refs.items()
                                 if element.get('role') == 'button' and element.get('name') == 'Window fixture')
                receipt = state['observation']
                if not isinstance(receipt, str) or not receipt:
                    # Native operator input has a bounded quiet interval. A read
                    # during the active gesture cannot authorize a later write.
                    # Request fresh readable state until the real service mints
                    # evidence; never synthesize a receipt or retry a write.
                    receipt_polls += 1
                    if receipt_polls > 10:
                        raise ValueError('Readable Observe did not provide its receipt after operator input settled')
                    time.sleep(0.1)
                    call('browser-observe', {'kind':'control', **identity, 'control':{'kind':'observe'}})
                    continue
                call('browser-click', {'kind':'control', **identity,
                     'observation':receipt, 'control':{'kind':'click','reference':reference}})
            else:
                record(answer)
        except (KeyError, StopIteration, ValueError, TypeError) as error:
            record({'isError':True,'success':False,'tool_answer':answer,'error':str(error)})
        (output/'provider-trace.json').write_text(json.dumps(trace, indent=2), encoding='utf-8')
