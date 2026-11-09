import {spawn,execFileSync} from 'node:child_process';
import {createServer} from 'node:http';
import {mkdtempSync,writeFileSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {performance} from 'node:perf_hooks';
const ROOT=process.env.BENCH_ROOT||'/tmp/ferrite-controller-bench';
const chrome=process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port=Number(process.env.CDP_PORT||19483), webport=Number(process.env.FIXTURE_PORT||19484);
const url=`http://127.0.0.1:${webport}`;
const observe=`JSON.stringify(Array.from(document.querySelectorAll('input,button,[role="status"]')).map(e=>({tag:e.tagName,id:e.id,text:e.textContent,value:e.value??null,disabled:e.disabled??false})))`;
const reset=`document.querySelector('input').value='';document.querySelector('[role="status"]').textContent='Ready';true`;
const verify=`document.querySelector('[role="status"]').textContent`;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
function command(bin,args){return new Promise((resolve,reject)=>{const p=spawn(bin,args,{env:{...process.env,AGENT_BROWSER_HOME:ROOT+'/ab-home'}});let out='',err='';p.stdout.on('data',x=>out+=x);p.stderr.on('data',x=>err+=x);p.on('error',reject);p.on('exit',c=>c?reject(Error(`${c}: ${err} ${out}`)):resolve(out));});}
const rss=pid=>Number(execFileSync('ps',['-o','rss=','-p',String(pid)],{encoding:'utf8'}).trim())/1024;
if(process.argv[2]==='worker'){
 const kind=process.argv[3]; const samples=[];const timings={observe:[],fill:[],click:[],verify:[]};let browser,page,ab,session;
 if(kind.startsWith('ab')){ab=kind==='ab-installed'?process.env.AB_INSTALLED:(kind==='ab-npm'?ROOT+'/node_modules/.bin/agent-browser':(process.env.AB_NATIVE||ROOT+'/node_modules/agent-browser/bin/agent-browser-darwin-arm64'));session='ferrite-bench-'+kind;await command(ab,['--session',session,'--cdp',String(port),'open',url]);}
 else if(kind==='playwright'){const {chromium}=await import('playwright-core');browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);page=browser.contexts()[0].pages()[0];await page.goto(url);}
 else {const {default:puppeteer}=await import('puppeteer-core');browser=await puppeteer.connect({browserURL:`http://127.0.0.1:${port}`});page=(await browser.pages())[0];await page.goto(url);}
 const cli=(...args)=>command(ab,['--session',session,...args]);
 const ev=js=>ab?cli('eval',js):page.evaluate(js);
 const errors=[];
 for(let i=0;i<35;i++){
  await ev(reset);const expected='Saved: value-'+i;const start=performance.now();let outcome;
  try{
   if(kind==='ab-batch'){
    outcome=await cli('batch','--bail',`eval ${JSON.stringify(observe)}`,`fill #name value-${i}`,`click #save`,`eval ${JSON.stringify(verify)}`);
    if(!outcome.includes(expected)||!outcome.includes('Ready'))throw Error('batch correctness failed: '+outcome);
   }else{
    let t=performance.now();const snapshot=await ev(observe);if(!snapshot.includes('Ready')||!snapshot.includes('save'))throw Error('observation mismatch');if(i>=5)timings.observe.push(performance.now()-t);
    t=performance.now();if(ab)await cli('fill','#name','value-'+i);else await page.locator('#name').fill('value-'+i);if(i>=5)timings.fill.push(performance.now()-t);
    t=performance.now();if(ab)await cli('click','#save');else await page.locator('#save').click();if(i>=5)timings.click.push(performance.now()-t);
    t=performance.now();outcome=await ev(verify);if(!outcome.includes(expected))throw Error('result mismatch: '+outcome);if(i>=5)timings.verify.push(performance.now()-t);
   }
   if(i>=5)samples.push(performance.now()-start);
  }catch(e){errors.push({i,error:String(e)});}
 }
 let daemon=null;if(ab){try{daemon=await cli('session','info','--json');}catch(e){daemon=String(e);}}
 const daemonPid=daemon?JSON.parse(daemon).data.pid:null;
 const result={kind,daemonRSSMiB:daemonPid?rss(daemonPid):null,samples,timings,errors,workerRSSMiB:rss(process.pid),daemon};
 writeFileSync(ROOT+'/'+(process.env.BENCH_PASS||'forward')+'-'+kind+'.json',JSON.stringify(result,null,2));
 if(daemonPid)process.kill(daemonPid);
 if(browser)await browser.disconnect?.();
 process.exit(0);
}else{
 const server=createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end(`<!doctype html><title>Controller fixture</title><style>body{font:16px sans-serif}button,input{margin:12px;padding:8px}</style><label>Name<input id="name"></label><button id="save" onclick="document.getElementById('status').textContent='Saved: '+document.getElementById('name').value">Save</button><p id="status" role="status">Ready</p><main>${Array.from({length:200},(_,i)=>`<article><h2>Card ${i}</h2><p>Stable noninteractive fixture text ${i}</p></article>`).join('')}</main>`);});
 await new Promise(r=>server.listen(webport,'127.0.0.1',r));
 const profile=mkdtempSync(ROOT+'/profile-');const proc=spawn(chrome,['--headless=new',`--remote-debugging-port=${port}`,`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','--window-size=1280,800','about:blank'],{stdio:'ignore'});
 try{
  let metadata;for(let i=0;i<100;i++){try{metadata=await(await fetch(`http://127.0.0.1:${port}/json/version`)).json();break;}catch{await sleep(100);}}
  if(!metadata)throw Error('Chrome unavailable');writeFileSync(ROOT+'/environment.json',JSON.stringify({metadata,profile,node:process.version,chromeRSSMiB:rss(proc.pid)},null,2));
  for(const kind of (process.env.BENCH_PASS==='reverse'?['ab-batch','ab-latest','puppeteer','playwright','ab-npm','ab-installed']:['ab-installed','ab-npm','playwright','puppeteer','ab-latest','ab-batch'])){console.log('RUN',kind);const output=await command(process.execPath,[import.meta.filename,'worker',kind]);console.log(output);}
 }finally{proc.kill();server.close();}
}
