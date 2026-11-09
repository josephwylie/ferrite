"""Required browser boundaries, using actual provider tool calls and native UI input."""
import base64
import json
import threading
import time
from urllib.parse import urlsplit, parse_qs, urlunsplit
from urllib.request import urlopen

PAGE = b'''<!doctype html><meta charset="utf-8"><title>Browser boundaries</title>
<style>body{margin:0;background:white}button{margin:12px}#baseline{display:block;margin:40px;width:240px;height:160px;border:0;background:rgb(213,37,59);color:white;font:24px sans-serif}</style>
<button id="baseline" onclick="fetch('/clicked',{method:'POST'}).then(()=>{this.dataset.clicks=Number(this.dataset.clicks||0)+1;this.style.background=this.dataset.clicks==='1'?'rgb(23,173,91)':'rgb(29,97,219)'})">Window fixture</button>
<button id="target" onclick="targetClick()">Boundary target</button>
<button onclick="document.querySelector('#target').outerHTML='<button id=target onclick=targetClick()>Boundary target</button>'">Replace target</button>
<button onclick="document.querySelector('#concurrent-ready')?.remove();setTimeout(()=>{let n=document.createElement('div');n.id='concurrent-ready';document.body.append(n)},1500)">Start concurrent wait</button>
<button onclick="fetch('/boundaries/save'+location.search,{method:'POST'}).then(()=>{let n=document.querySelector('#boundary-save');n.dataset.saved='true';n.textContent='Boundary write saved'})">Commit boundary write</button>
<button style="position:absolute;left:12px;top:calc(100vh - 10px);height:80px;width:240px" onclick="fetch('/boundaries/recovery?mode=success',{method:'POST'})">Recoverable boundary target</button>
<button style="position:fixed;left:300px;top:calc(100vh - 10px);height:80px;width:240px" onclick="fetch('/boundaries/recovery?mode=failed',{method:'POST'})">Fixed partial boundary target</button>
<output id="boundary-save">No boundary write yet</output>
<script>function targetClick(){fetch('/boundaries/click'+location.search,{method:'POST'})}</script>'''

class Fixture:
    def __init__(self):
        self.lock=threading.Lock();self.clicks=[];self.saves=[];self.recoveries=[]
    def snapshot(self):
        with self.lock:return {'clicks':list(self.clicks),'saves':list(self.saves),'recoveries':list(self.recoveries)}
    def get(self,path):
        route=urlsplit(path).path
        if route=='/boundaries-oracle':return 200,'application/json',json.dumps(self.snapshot()).encode()
        if route=='/boundaries':return 200,'text/html',PAGE
        return None
    def post(self,path,body):
        parsed=urlsplit(path)
        if parsed.path not in ('/boundaries/click','/boundaries/save','/boundaries/recovery'):return None
        event={'tab':parse_qs(parsed.query).get('tab',['A'])[0],'mode':parse_qs(parsed.query).get('mode',[None])[0],'monotonic':time.monotonic()}
        with self.lock:(self.clicks if parsed.path.endswith('click') else self.recoveries if parsed.path.endswith('recovery') else self.saves).append(event)
        return 204,'application/json',b''

class Boundary:
    def __init__(self,call,finish,output,provider='claude'):
        self.call,self.finish,self.output,self.provider=call,finish,output,provider
        self.handlers={};self.number=0;self.ui=0;self.trace=[];self.checks=[];self.polls=0
        self.concurrent={};self.last=None;self.failed=False;self.baseline_settled=False
    def begin(self):self.observe(self.manual)
    def send(self,operation,callback,error=False):
        self.number+=1;name='boundary-'+str(self.number)
        self.handlers[name]=(callback,error,operation)
        self.call(name,operation)
    def receive(self,name,answer):
        if self.failed or name not in self.handlers:return
        callback,expected_error,operation=self.handlers.pop(name)
        text='\n'.join(item.get('text','') for item in answer.get('content',answer.get('contentItems',[])))
        failed=answer.get('isError') is not False if self.provider=='claude' else answer.get('success') is not True
        self.trace.append({'request':operation,'answer':answer,'error_expected':expected_error,'monotonic':time.monotonic()})
        (self.output/'boundary-trace.json').write_text(json.dumps(self.trace,indent=2),encoding='utf-8')
        try:
            if expected_error is None:
                if failed and not any(word in text.lower() for word in ('another agent','wait for','in use','busy')):raise ValueError('Concurrent call failed for an unrelated reason: '+text)
            elif failed and not expected_error:raise ValueError('Unexpected boundary tool failure: '+text)
            elif expected_error and not failed:raise ValueError('Rejected boundary operation unexpectedly succeeded')
            state=None if failed else json.loads(text)
            self.last=answer
            callback(state,text,answer)
        except (ValueError,KeyError,TypeError,StopIteration,OSError,AssertionError) as error:self.fail(str(error))
    @staticmethod
    def identity(state):
        tab=next(tab for tab in state['state']['tabs'] if tab['id']==state['state']['active'])
        return {'document':{'tab':tab['id'],'generation':tab['generation']},'revision':state['revision']}
    def state(self,callback):
        def ready(state,text,answer):
            tab=next(tab for tab in state['state']['tabs'] if tab['id']==state['state']['active'])
            if tab['loading'] or not tab['live'] or (not self.baseline_settled and tab['protected']):
                self.polls+=1
                if self.polls>60:raise ValueError('Public browser state did not settle')
                time.sleep(.1);self.state(callback);return
            self.baseline_settled=True
            self.polls=0;callback(state,text,answer)
        self.send({'kind':'state'},ready)
    def observe(self,callback):
        def ready(state,_,answer):
            self.send({'kind':'control',**self.identity(state),'control':{'kind':'observe'}},observed)
        def observed(state,text,answer):
            if not state.get('observation') or not isinstance(state['value']['refs'],dict):raise ValueError('Fresh Observe did not supply usable evidence')
            callback(state,text,answer)
        self.state(ready)
    def control(self,state,control,callback,error=False):
        self.send({'kind':'control',**self.identity(state),'observation':state.get('observation'),'control':control},callback,error)
    def action(self,state,action,callback):
        self.send({'kind':'action','observation':state.get('observation'),'action':action},callback)
    @staticmethod
    def ref(state,name):
        matches=[key for key,value in state['value']['refs'].items() if value.get('role')=='button' and value.get('name')==name]
        if len(matches)!=1:raise ValueError('Missing exact observed button '+name)
        return matches[0]
    def operator(self,action):
        self.ui+=1
        path=self.output/'boundary-ui-request.json';temporary=self.output/'boundary-ui-request.tmp'
        temporary.write_text(json.dumps({'id':self.ui,'action':action}));temporary.replace(path)
        until=time.monotonic()+12
        while time.monotonic()<until:
            answer=self.output/'boundary-ui-answer.json'
            if answer.exists():
                value=json.loads(answer.read_text())
                if value.get('id')==self.ui:
                    if value.get('success') is not True:raise ValueError('Actual operator action failed')
                    return value
            time.sleep(.05)
        raise ValueError('Native operator handshake timed out: '+action)
    def manual(self,state,_,answer):
        self.ref(state,'Window fixture');self.a=state
        self.base=next(tab['url'] for tab in state['state']['tabs'] if tab['id']==state['state']['active'])
        self.target=self.ref(state,'Boundary target')
        self.operator('hold')
        self.control(state,{'kind':'screenshot'},self.held_image)
    def held_image(self,state,_,answer):
        images=[item for item in answer.get('content',answer.get('contentItems',[])) if item.get('type')=='image']
        if len(images)!=1 or not base64.b64decode(images[0]['data'],validate=True).startswith(b'\x89PNG\r\n\x1a\n'):raise ValueError('Held operator screenshot was not one real PNG')
        if state.get('observation') is not None:raise ValueError('Screenshot incorrectly minted readable Observe permission')
        self.control(self.a,{'kind':'click','reference':self.target},self.held_rejected,True)
    def held_rejected(self,*_):
        self.checks.append('held operator input refuses old agent write; screenshot does not discharge barrier')
        self.operator('release')
        self.control(self.a,{'kind':'click','reference':self.target},self.after_release,True)
    def after_release(self,*_):
        self.checks.append('released gesture still requires fresh Observe')
        self.observe(self.replace)
    def replace(self,state,*_):
        self.a=state;self.target=self.ref(state,'Boundary target')
        self.control(state,{'kind':'click','reference':self.ref(state,'Replace target')},self.replaced)
    def replaced(self,*_):self.control(self.a,{'kind':'click','reference':self.target},self.stale_rejected,True)
    def stale_rejected(self,*_):
        self.checks.append('same-label DOM replacement refuses old exact reference')
        self.observe(self.wrong_tab)
    def wrong_tab(self,state,*_):
        self.a=state;wrong={**self.identity(state),'document':{'tab':900000000,'generation':1}}
        self.send({'kind':'control',**wrong,'observation':state['observation'],'control':{'kind':'click','reference':self.ref(state,'Boundary target')}},self.wrong_rejected,True)
    def wrong_rejected(self,*_):
        self.checks.append('unowned tab id refused')
        self.control(self.a,{'kind':'click','reference':self.ref(self.a,'Boundary target')},self.open_b)
    def open_b(self,*_):
        self.a_tab=self.identity(self.a)['document']['tab'];self.old_a=self.a
        self.action(self.a,{'kind':'open','url':self.base.split('?',1)[0]+'?tab=B'},lambda *_:self.observe(self.b_observed))
    def b_observed(self,state,*_):
        self.b=state
        self.control(state,{'kind':'click','reference':self.ref(self.old_a,'Boundary target')},self.cross_tab_refused,True)
    def cross_tab_refused(self,*_):
        self.checks.append('other-tab opaque reference cannot target current tab')
        self.control(self.b,{'kind':'click','reference':self.ref(self.b,'Start concurrent wait')},self.concurrent_start)
    def concurrent_start(self,*_):
        def waited(state,text,answer):
            self.concurrent['wait']=True;self.concurrent_done()
        def scrolled(state,text,answer):
            self.concurrent['scroll']=True;self.concurrent_done()
        # Either serialized completion or an explicit busy refusal is valid;
        # neither caller may redirect the other to another page.
        self.control(self.b,{'kind':'wait','selector':'#concurrent-ready','state':'attached'},waited)
        self.number+=1;name='boundary-'+str(self.number)
        op={'kind':'control',**self.identity(self.b),'observation':self.b['observation'],'control':{'kind':'scroll','x':0,'y':1}}
        self.handlers[name]=(scrolled,None,op);self.call(name,op)
    def concurrent_done(self):
        if len(self.concurrent)!=2:return
        self.checks.append('concurrent callers finish serially or refuse busy work without retargeting')
        self.action(self.b,{'kind':'select','tab':self.a_tab},self.selected_a)
    def selected_a(self,*_):self.control(self.old_a,{'kind':'click','reference':self.ref(self.old_a,'Boundary target')},self.old_a_refused,True)
    def old_a_refused(self,*_):
        self.checks.append('Observe A / Observe B / Act A rejects stale evidence rather than acting on B')
        self.observe(self.reload)
    def reload(self,state,*_):
        self.a=state;self.operator('reload')
        self.control(state,{'kind':'click','reference':self.ref(state,'Boundary target')},self.reload_refused,True)
    def reload_refused(self,*_):
        self.checks.append('reload rejects the previous document and reference')
        self.observe(self.recovery_success)
    def recovery_success(self,state,*_):
        self.control(state,{'kind':'click','reference':self.ref(state,'Recoverable boundary target')},self.recovered)
    def recovered(self,state,*_):
        if state['value'].get('initial_failures')!=1 or state['value'].get('recovery_attempts')!=1:raise ValueError('Partially visible click did not report its bounded recovery')
        self.checks.append('outside viewport click scrolls and retries exactly once')
        self.observe(self.recovery_failure)
    def recovery_failure(self,state,*_):
        self.control(state,{'kind':'click','reference':self.ref(state,'Fixed partial boundary target')},self.failed_recovery,True)
    def failed_recovery(self,state,text,answer):
        if 'initial_failures=1' not in text or 'recovery_attempts=1' not in text:raise ValueError('Failed click lost its bounded recovery counters')
        self.checks.append('failed recovery retains both attempt counters and does not dispatch a click')
        self.observe(self.uncertain_begin)
    def uncertain_begin(self,state,*_):
        self.a=state
        for suffix in ('arm','dispatched','release'):(self.output/('completion-gate-'+suffix)).unlink(missing_ok=True)
        (self.output/'completion-gate-arm').write_text('one actual successful native write')
        self.control(state,{'kind':'click','reference':self.ref(state,'Commit boundary write')},self.uncertain,True)
    def uncertain(self,state,text,answer):
        if 'uncertain' not in text.lower() or not (self.output/'completion-gate-dispatched').exists():raise ValueError('Expected an actual dispatched write with an uncertain reply')
        self.control(self.a,{'kind':'click','reference':self.ref(self.a,'Commit boundary write')},self.no_replay,True)
    def no_replay(self,*_):
        (self.output/'completion-gate-release').write_text('allow late completion')
        self.observe(self.reconciled)
    def reconciled(self,state,*_):
        self.checks.append('committed write timeout blocks blind replay and fresh Observe reconciles')
        self.control(state,{'kind':'wait','selector':'#boundary-save[data-saved]','state':'attached'},self.finished)
    def finished(self,state,text,answer):
        parsed=urlsplit(self.base);url=urlunsplit((parsed.scheme,parsed.netloc,'/boundaries-oracle','',''))
        oracle=json.loads(urlopen(url,timeout=5).read())
        if len(oracle['clicks'])!=1 or oracle['clicks'][0]['tab']!='A':raise ValueError('Independent target oracle detected wrong-tab or stale clicks')
        if len(oracle['recoveries'])!=1 or oracle['recoveries'][0]['mode']!='success':raise ValueError('Recovery replayed or dispatched its known outside-viewport failure')
        if len(oracle['saves'])!=1 or oracle['saves'][0]['tab']!='A':raise ValueError('Independent save oracle detected a lost or duplicate committed write')
        (self.output/'boundary-results.json').write_text(json.dumps({'success':True,'checks':self.checks,'oracle':oracle},indent=2))
        self.finish(answer)
    def fail(self,error):
        if self.failed:return
        self.failed=True
        (self.output/'completion-gate-release').write_text('failure cleanup')
        (self.output/'boundary-results.json').write_text(json.dumps({'success':False,'error':error,'checks':self.checks},indent=2))
        self.finish({'isError':True,'success':False,'content':[{'type':'text','text':error}],'contentItems':[{'type':'text','text':error}]})
