// P02-09: actual failed projections must retire previously rendered private data.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {randomUUID,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
for(const k of ['ALIA_TEST_URL','ALIA_TEST_TOKEN','ALIA_TEST_OUTPUT','ALIA_TEST_BINARY','ALIA_P02_DATABASE','ALIA_P02_PSQL_COMMAND'])assert(process.env[k],`Missing private parent resource: ${k}`);
assert.equal(process.env.ALIA_P02_LOCK_HELD,'1');
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=process.env.ALIA_TEST_OUTPUT;
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));assert.match(process.env.ALIA_P02_DATABASE,/^alia_p02_[a-z0-9_]+$/);
const args=JSON.parse(process.env.ALIA_P02_PSQL_COMMAND);assert(Array.isArray(args)&&args.length&&args.every(v=>typeof v==='string'));
const sql=text=>{const r=spawnSync(args[0],[...args.slice(1),'-v','ON_ERROR_STOP=1','-c',text],{encoding:'utf8',timeout:15000});assert.equal(r.status,0,'private SQL failed');return r.stdout.trim();};
async function api(route,{method='GET',body,headers={}}={}){const r=await fetch(base+'/api/v1'+route,{method,headers:{Authorization:'Bearer '+token,Connection:'close',...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{}),...headers},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(15000)});return {status:r.status,data:await r.json()};}
await mkdir(output,{recursive:false});
const report={task_id:'ALIA-P02-09',scope:'real state/persona refusal and stale DOM; no development fixture or production model',checks:[],passed:false,cleanup:false};
const pages={},errors=[],refusedPages=new Set();let browser,context,originalPersona,corrupted=false;
const prefix='P02_09_PRIVATE_',pass=name=>{report.checks.push(name);console.log('PASS '+name);};
const releases=new Set(),pause=ms=>new Promise(r=>setTimeout(r,ms));
function gate(){let resolve;const promise=new Promise(r=>{resolve=r;});releases.add(resolve);return {promise,resolve};}
async function bounded(p,label){let timer;try{return await Promise.race([p,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('deadline: '+label)),20000);})]);}finally{clearTimeout(timer);}}
try{
 report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');
 assert.equal((await api('/state')).status,409);
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'))),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json')));runtime.autonomy.enabled=false;persona.identity.self_description=prefix+'PERSONA';
 assert.equal((await api('/initialize',{method:'POST',body:{persona,runtime}})).status,201);
 const initial=await api('/state');assert.equal(initial.status,200);originalPersona=initial.data.persona;report.individual_id=initial.data.individual_id;
 const goal=randomUUID();sql(`INSERT INTO objects(individual_id,id,kind) SELECT id,'${goal}','goal' FROM individuals; INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition) SELECT id,'${goal}','event','${prefix}GOAL','synthetic criterion','synthetic step','synthetic stop' FROM individuals;`);
 browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_EXECUTABLE});context=await browser.newContext();
 for(const [name,route] of Object.entries({identity:'identity',cognition:'identity/cognition',config:'config'})){
  const p=await context.newPage();pages[name]=p;p.setDefaultTimeout(15000);p.on('pageerror',e=>errors.push(e.message));p.on('response',r=>{if(new URL(r.url()).pathname==='/api/v1/state'&&r.status()===422)refusedPages.add(name);});
  await p.goto(base+'/#/'+route);await p.getByLabel('实例访问令牌').fill(token);await p.getByRole('button',{name:'连接 Alia',exact:true}).click();await p.locator('.console-status time').waitFor();
 }
 await pages.identity.getByText(prefix+'PERSONA',{exact:true}).waitFor();await pages.cognition.getByText(prefix+'GOAL',{exact:true}).waitFor();
 await pages.config.getByRole('button',{name:'编辑运行配置',exact:true}).click();await pages.config.getByLabel('后台每日 token 上限',{exact:true}).fill('11013');
 pass('actual private persona and goal render in independent tabs; runtime draft prepared without saving');
 const settings=[];let lost=false;
 const loseSettings=async route=>{settings.push({key:route.request().headers()['idempotency-key'],version:route.request().headers()['if-match'],body:route.request().postData()});const r=await route.fetch();assert.equal(r.status(),200);if(!lost){lost=true;await route.abort('connectionreset');}else await route.fulfill({response:r});};
 await pages.config.route('**/api/v1/settings',loseSettings);await pages.config.getByRole('button',{name:'保存配置',exact:true}).click();let confirm=pages.config.getByRole('dialog',{name:'确认运行规则变更',exact:true});await confirm.getByRole('button',{name:'确认提交',exact:true}).click();await confirm.getByRole('alert').waitFor();await confirm.getByRole('button',{name:'取消',exact:true}).click();assert.equal(settings.length,1);const committedConfigVersion=sql('SELECT config_version FROM individuals');
 pass('actual settings200 is committed but transport-lost; no automatic new-key retry');
 sql("UPDATE individuals SET persona=jsonb_set(persona,'{expression,tone}','\"unproven synthetic corruption\"'::jsonb)");corrupted=true;
 for(const route of ['/state','/persona']){const r=await api(route);assert.equal(r.status,422);assert.equal(r.data.error.code,'invalid_request');assert.equal(r.data.persona,undefined);assert.equal(r.data.current,undefined);}
 pass('actual source validator rejects corrupted projection with422/invalid_request and no private payload');
 report.refusal_observations=[];
 for(const [name,p] of Object.entries(pages)){
  if(!refusedPages.has(name)){const result=p.waitForResponse(r=>new URL(r.url()).pathname==='/api/v1/state'&&r.status()===422);result.catch(()=>{});const refresh=p.getByRole('button',{name:'刷新状态',exact:true});if(await refresh.count())await refresh.click();await result;}
  await p.getByRole('alert').filter({has:p.getByRole('button',{name:'重新读取',exact:true})}).waitFor();
  const stale=await p.locator('main').textContent();report.refusal_observations.push({page:name,private_marker_present:stale.includes(prefix),old_persona_heading_present:await p.getByRole('heading',{name:'人格与表达',exact:true}).count()});
 }
 const stale=report.refusal_observations.filter(x=>x.private_marker_present||x.old_persona_heading_present);
 if(stale.length){report.reproduced_stale_private_dom=true;await pages.identity.screenshot({path:path.join(output,'actual422-stale-identity.png'),fullPage:true});throw new Error('actual422 kept previously rendered private state: '+stale.map(x=>x.page).join(','));}
 for(const p of Object.values(pages)){assert.equal(await p.locator('.console-status time').count(),0);assert.equal(await p.getByRole('button',{name:'刷新状态',exact:true}).count(),0);await p.getByText('未能读取个体状态，请重新读取以恢复连接。',{exact:true}).waitFor();}
 assert(!(await pages.identity.locator('main').textContent()).includes(prefix));assert.equal(settings.length,1);
 pass('actual failed state snapshot removes persona/private children and successful-sync marker in all tabs');
 sql(`UPDATE individuals SET persona='${JSON.stringify(originalPersona).replaceAll("'","''")}'::jsonb`);corrupted=false;
 for(const p of Object.values(pages)){await p.getByRole('button',{name:'重新读取',exact:true}).click();await p.locator('.console-status time').waitFor();}
 await pages.identity.getByText(prefix+'PERSONA',{exact:true}).waitFor();assert.equal(await pages.config.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'11013');
 await pages.config.locator('.sidebar nav a[href="#/models"]').click();const guard=pages.config.getByRole('dialog',{name:'配置尚未保存',exact:true});await guard.waitFor();await guard.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await pages.config.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'11013');
 pass('explicit real200 revalidation restores verified views and unchanged runtime draft/dirty cancellation');
 await pages.config.getByRole('button',{name:'保存配置',exact:true}).click();confirm=pages.config.getByRole('dialog',{name:'确认运行规则变更',exact:true});await confirm.getByRole('button',{name:'确认提交',exact:true}).click();await confirm.waitFor({state:'hidden'});assert.equal(settings.length,2);assert.deepEqual(settings[1],settings[0]);assert.equal(sql('SELECT config_version FROM individuals'),committedConfigVersion);await pages.config.unroute('**/api/v1/settings',loseSettings);
 pass('explicit same-key/body/If-Match settings replay after failed snapshot recovers only the original committed write');
 const recoveryRead=gate();const holdRecovery=async route=>{await recoveryRead.promise;await route.continue();};await context.route('**/api/v1/state',holdRecovery);refusedPages.clear();
 const restarted=spawnSync(JSON.parse(process.env.ALIA_P02_RESTART_COMMAND)[0],JSON.parse(process.env.ALIA_P02_RESTART_COMMAND).slice(1),{encoding:'utf8',timeout:30000});assert.equal(restarted.status,0,'owned service restart failed');
 // The authenticated restart bridge requires a healthy snapshot. Only after it
 // verifies the restarted process do browser recovery reads reach the real422.
 sql("UPDATE individuals SET persona=jsonb_set(persona,'{expression,tone}','\"unproven synthetic corruption\"'::jsonb)");corrupted=true;recoveryRead.resolve();releases.delete(recoveryRead.resolve);
 const recoveryDeadline=Date.now()+20000;while(refusedPages.size<Object.keys(pages).length&&Date.now()<recoveryDeadline)await pause(25);assert.equal(refusedPages.size,Object.keys(pages).length,'actual422 in every EOF recovery loop');
 for(const p of Object.values(pages)){await p.waitForFunction(prefix=>!document.querySelector('main')?.textContent?.includes(prefix),prefix);await p.waitForFunction(()=>!document.querySelector('.console-status time'));}
 await pause(1200);sql(`UPDATE individuals SET persona='${JSON.stringify(originalPersona).replaceAll("'","''")}'::jsonb`);corrupted=false;
 for(const p of Object.values(pages))await p.locator('.console-status time').waitFor({timeout:45000});
 await context.unroute('**/api/v1/state',holdRecovery);
 await pages.identity.getByText(prefix+'PERSONA',{exact:true}).waitFor();
 pass('genuine SSE EOF retires views; failing recovery reads preserve the existing retry loop and recover without a new user request');
 const deny=async route=>{const r=await route.fetch({headers:{...route.request().headers(),authorization:'Bearer invalid-p02-09-token'}});assert.equal(r.status(),401);await route.fulfill({response:r});};await pages.identity.route('**/api/v1/state',deny);await pages.identity.getByRole('button',{name:'刷新状态',exact:true}).click();await pages.identity.getByLabel('实例访问令牌').waitFor();assert(!(await pages.identity.content()).includes(prefix));await pages.identity.unroute('**/api/v1/state',deny);
 pass('actual401 still ends the session and clears private views');
 const late=gate(),gotLate=gate();const lateRead=async route=>{try{const r=await route.fetch();assert.equal(r.status(),200);gotLate.resolve();await late.promise;await route.fulfill({response:r});}catch{await route.abort().catch(()=>{});}};
 await pages.cognition.route('**/api/v1/state',lateRead);await pages.cognition.getByRole('button',{name:'刷新状态',exact:true}).click();await bounded(gotLate.promise,'genuine pre-logout state200');await pages.cognition.getByRole('button',{name:'断开',exact:true}).click();late.resolve();releases.delete(late.resolve);await pause(200);await pages.cognition.getByLabel('实例访问令牌').waitFor();assert(!(await pages.cognition.content()).includes(prefix));assert(!(await pages.cognition.content()).includes(token));await pages.cognition.unroute('**/api/v1/state',lateRead);
 pass('logout blocks a delayed genuine200 from restoring private state');
 await pages.config.screenshot({path:path.join(output,'verified-config-after-recovery.png'),fullPage:true});
 assert.equal(sql('SELECT count(*) FROM usage_reservations'),'0');assert.equal(sql('SELECT count(*) FROM runs'),'0');assert.deepEqual(errors,[]);report.passed=true;
}catch(e){report.error=e instanceof Error?e.message:'browser failed';console.error(report.error);process.exitCode=1;}
finally{
 for(const release of releases)release();
 if(corrupted&&originalPersona)try{sql(`UPDATE individuals SET persona='${JSON.stringify(originalPersona).replaceAll("'","''")}'::jsonb`);report.corruption_restored=true;}catch{report.corruption_restored=false;}
 const failures=[];if(context)try{await context.close();}catch{failures.push('context');}if(browser)try{await browser.close();}catch{failures.push('browser');}report.cleanup=failures.length===0;if(failures.length){report.cleanup_errors=failures;report.passed=false;process.exitCode=1;}
 await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});
}
