import {spawn,execFileSync} from 'node:child_process';
import {createWorkOrderServer} from './fixture-server.mjs';
import {readFileSync,writeFileSync,mkdtempSync,mkdirSync} from 'node:fs';
import {performance} from 'node:perf_hooks';
import {fileURLToPath} from 'node:url';
const ROOT=process.env.BENCH_ROOT||'/tmp/ferrite-controller-bench', PORT=19583, WEB=19584, BASE=`http://127.0.0.1:${WEB}`;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const rss=pid=>Number(execFileSync('ps',['-o','rss=','-p',String(pid)],{encoding:'utf8'}).trim())/1024;
const run=(bin,args)=>new Promise((resolve,reject)=>{const p=spawn(bin,args);let out='',err='';const timer=setTimeout(()=>{p.kill();reject(Error('command timeout'));},15000);p.stdout.on('data',x=>out+=x);p.stderr.on('data',x=>err+=x);p.on('error',reject);p.on('exit',c=>{clearTimeout(timer);c?reject(Error(err+out)):resolve(out)})});
if(process.argv[2]==='worker'){
 const kind=process.argv[3],block=Number(process.argv[4]),session=`sustained-${kind}-${block}`;let browser,page,daemonPid,commands=0,polls=0,recoveries=0,recoveryErrors=[];
 const cli=(...a)=>{commands++;return run(ROOT+'/node_modules/agent-browser/bin/agent-browser-darwin-arm64',['--session',session,...a])};
 if(kind==='ab'){await cli('--cdp',String(PORT),'open',BASE);daemonPid=JSON.parse(await cli('session','info','--json')).data.pid}
 else if(kind==='playwright'){const {chromium}=await import(ROOT+'/node_modules/playwright-core/index.mjs');browser=await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(10000)}
 else{const {default:puppeteer}=await import(ROOT+'/node_modules/puppeteer-core/lib/puppeteer/puppeteer-core.js');browser=await puppeteer.connect({browserURL:`http://127.0.0.1:${PORT}`,defaultViewport:null});page=(await browser.pages())[0];page.setDefaultTimeout(10000)}
 if(kind==='ab')await cli('set','viewport','1280','713');else if(kind==='playwright')await page.setViewportSize({width:1280,height:713});else await page.setViewport({width:1280,height:713});
 const ev=async js=>kind==='ab'?JSON.parse(JSON.parse(await cli('eval',`JSON.stringify(${js})`))):page.evaluate(js);
 const ready=async js=>{const start=performance.now();for(;;){const value=await ev(js);if(typeof value!=='boolean')throw Error('nonboolean readiness '+JSON.stringify(value));if(value)return;polls++;if(performance.now()-start>10000)throw Error('condition timeout '+js);await sleep(50)}};
 const click=async sel=>{commands+=kind==='ab'?0:1;if(kind==='ab'){try{await cli('click',sel)}catch(e){if(!String(e).includes('outside the viewport'))throw e;if(process.env.AB_RECOVERY==='0'){const geometry=await ev(`(()=>{const r=document.querySelector(${JSON.stringify(sel)}).getBoundingClientRect();return {top:r.top,bottom:r.bottom,centerY:r.top+r.height/2,innerHeight}})()`);throw Error(String(e)+' Geometry: '+JSON.stringify(geometry))}recoveries++;recoveryErrors.push({selector:sel,error:String(e)});await cli('scrollintoview',sel);await cli('click',sel)}}else await page.locator(sel).click()};
 const fill=async(sel,value)=>{commands+=kind==='ab'?0:1;if(kind==='ab')await cli('fill',sel,value);else await page.locator(sel).fill(value)};
 const select=async(sel,value)=>{commands+=kind==='ab'?0:1;if(kind==='ab')await cli('select',sel,value);else if(kind==='playwright')await page.selectOption(sel,value);else await page.select(sel,value)};
 const go=async()=>{commands+=kind==='ab'?0:1;if(kind==='ab')await cli('open',BASE);else await page.goto(BASE)};
 const viewport=await ev(`({width:innerWidth,height:innerHeight})`);if(viewport.width!==1280||viewport.height!==713)throw Error('viewport mismatch '+JSON.stringify(viewport));
 const memory=()=>rss(daemonPid||process.pid);const results=[],startAll=performance.now(),memoryBefore=memory();let memoryWarm;
 for(let i=0;i<Number(process.env.TASKS||24);i++){
  const name=`order-${kind}-${block}-${i}`,start=performance.now(),beforeCommands=commands,beforePolls=polls,beforeRecoveries=recoveries;
  try{
   await go();await click('#records');await ready(`!!document.querySelector('#create')`);await click('#create');await ready(`!!document.querySelector('#name')`);
   await fill('#name',name);await fill('#email','owner@example.test');await fill('#description','Inspect subsystem and confirm quality.');await select('#team','Design');await select('#priority','High');await click('#notify');
   await click('#add-field');await ready(`!!document.querySelector('#custom-name')`);await fill('#custom-name','Tracking code');await fill('#custom-value','initial');await fill('#custom-value','custom-'+i);await fill('#budget','2400');await click('#approved');await click('#save');await ready(`document.querySelector('#status')?.textContent.startsWith('Saved ')`);
   await click('#records');await ready(`!!document.querySelector('#search')`);await fill('#search',name);await ready(`document.querySelectorAll('[data-id]').length===1`);await click('[data-id]');await ready(`!!document.querySelector('#name')`);
   await fill('#description','');await click('#description');if(kind==='ab')await cli('keyboard','type','Reviewed and approved.');else{commands++;await page.keyboard.type('Reviewed and approved.')} await select('#team','Engineering');await select('#priority','Normal');await click('#notify');await fill('#budget','3200');await fill('#custom-value','final-'+i);await click('#save');await ready(`document.querySelector('#status')?.textContent.startsWith('Saved ')`);
   await click('#home');await click('#records');await ready(`!!document.querySelector('#search')`);await fill('#search',name);await click('[data-id]');await ready(`!!document.querySelector('#name')`);
   const state=await ev(`Object.fromEntries(['name','email','description','team','priority','budget','custom-name','custom-value','notify','approved'].map(k=>{const e=document.getElementById(k);return [k,e.type==='checkbox'?e.checked:e.value]}))`);
   const expected={name,email:'owner@example.test',description:'Reviewed and approved.',team:'Engineering',priority:'Normal',budget:'3200','custom-name':'Tracking code','custom-value':'final-'+i,notify:false,approved:true};
   if(JSON.stringify(state)!==JSON.stringify(expected))throw Error('UI state mismatch '+JSON.stringify(state));
   const persisted=await(await fetch(BASE+'/oracle?name='+name)).json();const oracle={...expected,customName:expected['custom-name'],customValue:expected['custom-value']};delete oracle['custom-name'];delete oracle['custom-value'];
   for(const[k,v]of Object.entries(oracle))if(persisted[k]!==v)throw Error('Persistence mismatch '+k);
   results.push({i,ms:performance.now()-start,success:true,commands:commands-beforeCommands,polls:polls-beforePolls,recoveries:recoveries-beforeRecoveries,rssMiB:memory()});
   if(i===0)memoryWarm=memory();
   if(i===1){const t=performance.now();if(kind==='ab')await cli('screenshot',ROOT+`/sustained-${kind}.png`);else await page.screenshot({path:ROOT+`/sustained-${kind}.png`});results.at(-1).screenshotMs=performance.now()-t}
  }catch(e){results.push({i,ms:performance.now()-start,success:false,error:String(e),commands:commands-beforeCommands,polls:polls-beforePolls,recoveries:recoveries-beforeRecoveries,rssMiB:memory()})}
 }
 writeFileSync(ROOT+`/${process.env.RESULT_PREFIX||'sustained'}-${block}-${kind}.json`,JSON.stringify({kind,block,viewport,elapsedMs:performance.now()-startAll,memoryBefore,memoryWarm,memoryAfter:memory(),recoveryErrors,results},null,2));
 if(daemonPid)process.kill(daemonPid);if(browser)await browser.disconnect?.();process.exit(0);
}else{
 let records=[];if(process.env.RESUME){for(const kind of ['ab','playwright'])for(let i=0;i<24;i++)records.push({id:String(records.length+1),name:`order-${kind}-0-${i}`,email:'owner@example.test',description:'Reviewed and approved.',team:'Engineering',priority:'Normal',budget:'3200',customName:'Tracking code',customValue:'final-'+i,notify:false,approved:true});console.log('RESUMED server records',records.length)}const server=createWorkOrderServer({records});await new Promise(r=>server.listen(WEB,'127.0.0.1',r));
 try{for(const[block,order]of (process.env.BENCH_PILOT?[[process.env.PILOT_KIND||'ab']]:[['ab','playwright','puppeteer'],['playwright','puppeteer','ab'],['puppeteer','ab','playwright']]).entries())for(const kind of order){
  if(process.env.RESUME&&block===0&&['ab','playwright'].includes(kind))continue;
  const profile=mkdtempSync(ROOT+'/sustained-profile-');const chrome=spawn(process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',['--headless=new',`--remote-debugging-port=${PORT}`,`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','--window-size=1280,800','about:blank'],{stdio:'ignore'});
  try{for(let i=0;i<100;i++){try{await fetch(`http://127.0.0.1:${PORT}/json/version`);break}catch{await sleep(100)}}console.log('START',block,kind);await new Promise((resolve,reject)=>{const p=spawn(process.execPath,[fileURLToPath(import.meta.url),'worker',kind,String(block)],{stdio:'inherit'});p.on('exit',c=>c?reject(Error('worker '+c)):resolve())});console.log('DONE',block,kind)}finally{chrome.kill();await sleep(500)}}}finally{server.close()}
}
