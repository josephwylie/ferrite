"""One prescribed task driven only by public browser tool requests/responses."""
import json
import re
import time


def plan(name):
    def click(label): return ('click', 'button', label, {})
    def fill(label, value): return ('fill', 'textbox', label, {'value':value})
    def select(label, value): return ('select', 'combobox', label, {'values':[value]})
    def check(label, value): return ('check', 'checkbox', label, {'checked':value})
    def wait(selector): return ('wait', None, None, {'selector':selector, 'state':'attached'})
    return [click('Work orders'), wait('#create'), click('Create order'), wait('#name'),
        fill('Name', name), fill('Owner email', 'owner@example.test'),
        fill('Description', 'Inspect subsystem and confirm quality.'),
        select('Team', 'Design'), select('Priority', 'High'), check('Notify owner', True),
        click('Add custom field'), wait('#custom-name'), fill('Custom field name', 'Tracking code'),
        fill('Custom field value', 'initial'), fill('Custom field value', 'custom-0'),
        ('fill', 'spinbutton', 'Budget', {'value':'2400'}), check('Approve this order', True),
        click('Save order'), wait('#status[data-saved]'), click('Work orders'), wait('#search'),
        fill('Search orders', name), wait('[data-id]'), click('Open order'), wait('#name'),
        fill('Description', ''), ('type','textbox','Description',{'text':'Reviewed and approved.'}),
        select('Team','Engineering'), select('Priority','Normal'), check('Notify owner',False),
        ('fill','spinbutton','Budget',{'value':'3200'}), fill('Custom field value','final-0'),
        click('Save order'), wait('#status[data-saved]'), click('Overview'), click('Work orders'),
        wait('#search'), fill('Search orders',name), wait('[data-id]'), click('Open order'), wait('#name')]


def expected(name):
    return {'name':name,'email':'owner@example.test','description':'Reviewed and approved.',
        'team':'Engineering','priority':'Normal','budget':'3200','customName':'Tracking code',
        'customValue':'final-0','notify':False,'approved':True}


class WorkOrder:
    def __init__(self, name, call, finish, output):
        self.name, self.call, self.finish, self.output = name, call, finish, output
        self.steps = plan(name)
        self.index = 0
        self.trace = []

    def begin(self):
        self.call('work-state', {'kind':'state'})

    def receive(self, request, answer):
        self.trace.append({'received_monotonic':time.monotonic(),'request':request,'answer':answer})
        (self.output/'work-order-trace.json').write_text(json.dumps(self.trace,indent=2),encoding='utf-8')
        try:
            if answer.get('isError') is not False:
                raise ValueError('Actual work-order tool failed')
            state = json.loads(next(item['text'] for item in answer['content'] if 'text' in item))
            tab = next(tab for tab in state['state']['tabs'] if tab['id']==state['state']['active'])
            identity = {'document':{'tab':tab['id'],'generation':tab['generation']}, 'revision':state['revision']}
            if request == 'work-observe':
                kind, role, label, fields = self.steps[self.index]
                matches = [ref for ref, data in state['value']['refs'].items()
                           if data.get('role')==role and data.get('name')==label]
                if len(matches)!=1:
                    raise ValueError(f'Expected one current {role} {label!r}; got {len(matches)}')
                receipt = state['observation']
                if not isinstance(receipt,str) or not receipt:
                    raise ValueError('Work-order Observe did not authorize this action')
                self.call('work-action', {'kind':'control',**identity,'observation':receipt,
                    'control':{'kind':kind,'reference':matches[0],**fields}})
                return
            if request == 'work-final':
                self.validate(state)
                self.finish(answer)
                return
            if request == 'work-action':
                self.index += 1
            if self.index == len(self.steps):
                self.call('work-final', {'kind':'control',**identity,'control':{'kind':'observe'}})
            else:
                kind, _, _, fields = self.steps[self.index]
                if kind == 'wait':
                    self.call('work-action', {'kind':'control',**identity,'control':{'kind':'wait',**fields}})
                else:
                    self.call('work-observe', {'kind':'control',**identity,'control':{'kind':'observe'}})
        except (ValueError, KeyError, TypeError, StopIteration, AttributeError) as error:
            self.finish({'isError':True,'tool_answer':answer,'error':str(error),'step':self.index})

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
                            ('Custom field value','final-0')]:
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
