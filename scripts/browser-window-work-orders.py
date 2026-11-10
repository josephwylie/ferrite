"""One prescribed task driven only by public browser tool requests/responses."""
import json
import re
import time


def plan(name, number=0):
    def click(label): return ('click', 'button', label, {})
    def fill(label, value): return ('fill', 'textbox', label, {'value':value})
    def select(label, value): return ('select', 'combobox', label, {'values':[value]})
    def check(label, value): return ('check', 'checkbox', label, {'checked':value})
    def wait(selector): return ('wait', None, None, {'selector':selector, 'state':'attached'})
    return [('navigate',None,None,{}), click('Work orders'), wait('#create'), click('Create order'), wait('#name'),
        fill('Name', name), fill('Owner email', 'owner@example.test'),
        fill('Description', 'Inspect subsystem and confirm quality.'),
        select('Team', 'Design'), select('Priority', 'High'), check('Notify owner', True),
        click('Add custom field'), wait('#custom-name'), fill('Custom field name', 'Tracking code'),
        fill('Custom field value', 'initial'), fill('Custom field value', f'custom-{number}'),
        ('fill', 'spinbutton', 'Budget', {'value':'2400'}), check('Approve this order', True),
        click('Save order'), wait('#status[data-saved]'), click('Work orders'), wait('#search'),
        fill('Search orders', name), wait('[data-id]'), click('Open order'), wait('#name'),
        fill('Description', ''), ('click','textbox','Description',{}), ('type','textbox','Description',{'text':'Reviewed and approved.'}),
        select('Team','Engineering'), select('Priority','Normal'), check('Notify owner',False),
        ('fill','spinbutton','Budget',{'value':'3200'}), fill('Custom field value',f'final-{number}'),
        click('Save order'), wait('#status[data-saved]'), click('Overview'), click('Work orders'),
        wait('#search'), fill('Search orders',name), wait('[data-id]'), click('Open order'), wait('#name')]


def expected(name):
    return {'name':name,'email':'owner@example.test','description':'Reviewed and approved.',
        'team':'Engineering','priority':'Normal','budget':'3200','customName':'Tracking code',
        'customValue':'final-0','notify':False,'approved':True}


class WorkOrder:
    def __init__(self, name, call, finish, output, provider="claude", tasks=3):
        self.name, self.call, self.finish, self.output = name, call, finish, output
        self.provider=provider
        self.prefix=name.rsplit("-",1)[0]
        self.tasks=tasks
        self.task_index=0
        self.results=[]
        self.task_started=time.monotonic()
        self.response_count=0
        self.response_bytes=0
        self.steps = plan(name)
        self.index = 0
        self.trace = []
        self.loading_polls = 0

    def begin(self):
        self.task_started=time.monotonic()
        temporary=self.output/'phase.tmp'
        temporary.write_text(json.dumps({'phase':f'work-order-{self.task_index}','emitted_monotonic':self.task_started}),encoding='utf-8')
        temporary.replace(self.output/'phase.json')
        self.call('work-state', {'kind':'state'})

    def receive(self, request, answer):
        self.response_count+=1
        self.response_bytes+=len(json.dumps(answer).encode())
        self.trace.append({'task_index':self.task_index,'received_monotonic':time.monotonic(),'request':request,'answer':answer})
        (self.output/'work-order-trace.json').write_text(json.dumps(self.trace,indent=2),encoding='utf-8')
        try:
            if (self.provider=='claude' and answer.get('isError') is not False) or (self.provider=='codex' and answer.get('success') is not True):
                raise ValueError('Actual work-order tool failed')
            state = json.loads(next(item['text'] for item in answer.get('content',answer.get('contentItems',[])) if 'text' in item))
            tab = next(tab for tab in state['state']['tabs'] if tab['id']==state['state']['active'])
            identity = {'document':{'tab':tab['id'],'generation':tab['generation']}, 'revision':state['revision']}
            if request == 'work-action':
                self.index += 1
            if tab['loading'] or not tab['live']:
                self.loading_polls += 1
                if self.loading_polls > 20:
                    raise ValueError('The public service did not acknowledge the navigated live page')
                self.call('work-state', {'kind':'state'})
                return
            self.loading_polls = 0
            if request == 'work-observe':
                kind, role, label, fields = self.steps[self.index]
                receipt = state['observation']
                if not isinstance(receipt,str) or not receipt:
                    raise ValueError('Work-order Observe did not authorize this action')
                if kind == 'navigate':
                    self.call('work-action', {'kind':'action','observation':receipt,
                        'action':{'kind':'navigate','document':identity['document'],'url':tab['url']}})
                    return
                matches = [ref for ref, data in state['value']['refs'].items()
                           if data.get('role')==role and data.get('name')==label]
                if len(matches)!=1:
                    raise ValueError(f'Expected one current {role} {label!r}; got {len(matches)}')
                self.call('work-action', {'kind':'control',**identity,'observation':receipt,
                    'control':{'kind':kind,'reference':matches[0],**fields}})
                return
            if request == 'work-final':
                self.validate(state)
                self.record_task(True)
                if self.task_index+1<self.tasks:
                    self.task_index+=1
                    self.name=self.prefix+'-'+str(self.task_index)
                    self.steps=plan(self.name,self.task_index)
                    self.index=0;self.response_count=0;self.response_bytes=0
                    self.begin()
                else:self.finish(answer)
                return
            if self.index == len(self.steps):
                self.call('work-final', {'kind':'control',**identity,'control':{'kind':'observe'}})
            else:
                kind, _, _, fields = self.steps[self.index]
                if kind == 'wait':
                    self.call('work-action', {'kind':'control',**identity,'control':{'kind':'wait',**fields}})
                else:
                    self.call('work-observe', {'kind':'control',**identity,'control':{'kind':'observe'}})
        except (ValueError, KeyError, TypeError, StopIteration, AttributeError) as error:
            self.record_task(False,str(error))
            self.finish({'isError':True,'tool_answer':answer,'error':str(error),'step':self.index})

    def record_task(self,success,error=None):
        self.results.append({'task_name':self.name,'task_index':self.task_index,'warmup':self.task_index==0,
            'success':success,'error':error,'elapsed_ms':(time.monotonic()-self.task_started)*1000,
            'logical_steps':32,'readiness_waits':11,'responses':self.response_count,'response_bytes':self.response_bytes})
        (self.output/'work-order-results.json').write_text(json.dumps(self.results,indent=2),encoding='utf-8')

    def validate(self, state):
        # Read the actual readable observation. Missing full values are a product
        # observation failure; no evaluate/DOM side channel supplies them.
        snapshot = state['value']['snapshot']
        refs = state['value']['refs']
        lines = snapshot.splitlines()
        def field(role, label):
            matches = [ref for ref,data in refs.items() if data.get('role')==role and data.get('name')==label]
            if len(matches)!=1:
                raise ValueError(f'Expected one reopened {role} {label!r}')
            matching_lines = [line for line in lines if re.search(r'\bref='+re.escape(matches[0])+r'(?=[,\]])',line)]
            if len(matching_lines)!=1:
                raise ValueError(f'Readable observation does not print the offered opaque ref for {label!r}')
            return matching_lines[0]
        for label,value in [('Name',self.name),('Owner email','owner@example.test'),
                            ('Description','Reviewed and approved.'),('Custom field name','Tracking code'),
                            ('Custom field value',f'final-{self.task_index}')]:
            if field('textbox',label).split(']: ',1)[-1] != value:
                raise ValueError(f'Reopened textbox mismatch: {label}')
        if field('spinbutton','Budget').split(']: ',1)[-1] != '3200':
            raise ValueError('Reopened Budget mismatch')
        for label,checked in [('Notify owner',False),('Approve this order',True)]:
            line = field('checkbox',label)
            if ('checked='+str(checked).lower()) not in line:
                raise ValueError(f'Reopened checkbox mismatch: {label}: {line}')
        for label,value in [('Team','Engineering'),('Priority','Normal')]:
            if field('combobox',label).split(']: ',1)[-1] != value:
                raise ValueError(f'Reopened combobox mismatch: {label}')
            if not any(value in line and re.search(r'\bselected(?=[,\]])',line) for line in lines):
                raise ValueError(f'Reopened selected option absent: {value}')
