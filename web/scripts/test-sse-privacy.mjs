// Real private HTTP/PG/browser acceptance. Response gates only delay genuine HTTP
// responses; they never replace server data or fake a successful snapshot/SSE.
import assert from 'node:assert/strict';
import {createHash,randomUUID} from 'node:crypto';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {chromium} from 'playwright';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
for(const key of ['ALIA_TEST_URL','ALIA_TEST_TOKEN','ALIA_TEST_OUTPUT','ALIA_TEST_BINARY','ALIA_P02_DATABASE','ALIA_P02_PSQL_COMMAND','ALIA_P02_RESTART_COMMAND'])assert(process.env[key],`Explicit parent resource required: ${key}`);
assert.equal(process.env.ALIA_P02_LOCK_HELD,'1');
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=path.resolve(process.env.ALIA_TEST_OUTPUT);
assert(['127.0.0.1','localhost','[::1]'].includes(new URL(base).hostname));assert.match(process.env.ALIA_P02_DATABASE,/^alia_p02_[a-z0-9_]+$/);
const sqlArgs=JSON.parse(process.env.ALIA_P02_PSQL_COMMAND),restartArgs=JSON.parse(process.env.ALIA_P02_RESTART_COMMAND);
assert([sqlArgs,restartArgs].every(a=>Array.isArray(a)&&a.length&&a.every(v=>typeof v==='string')));
function sql(text){const r=spawnSync(sqlArgs[0],[...sqlArgs.slice(1),'-v','ON_ERROR_STOP=1','-c',text],{encoding:'utf8',timeout:15000});assert.equal(r.status,0,'isolated SSE SQL failed');return r.stdout;}
function restart(){const r=spawnSync(restartArgs[0],restartArgs.slice(1),{encoding:'utf8',timeout:30000});assert.equal(r.status,0,'owned SSE service restart failed');}
async function api(route,{method='GET',body,headers={}}={}){const r=await fetch(base+'/api/v1'+route,{method,headers:{Authorization:'Bearer '+token,Connection:'close',...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{}),...headers},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(15000)});return {status:r.status,data:await r.json(),etag:r.headers.get('etag')};}
const literal=v=>"'"+String(v).replaceAll("'","''")+"'",delay=ms=>new Promise(r=>setTimeout(r,ms));
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
async function bounded(p,label,ms=15000){let timer;try{return await Promise.race([p,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('deadline: '+label)),ms);})]);}finally{clearTimeout(timer);}}
await mkdir(path.dirname(output),{recursive:true});await mkdir(output);
const report={task:'ALIA-P02-07',scope:'real browser SSE privacy invalidation/reconnect; no 05/06 suite or full engineering matrix',started_at:new Date().toISOString(),checks:[],passed:false,cleanup:false,models:0,connection_contexts:process.argv.includes('--shared-context')?'shared-five-tabs':'isolated-client-contexts'};
const releases=new Set(),contexts=[],errors=[];let browser,diagnosticPage,fault=false;
const pass=(name,details={})=>{report.checks.push({name,...details});console.log('PASS '+name);};
function seed(label){
 const prefix=`SSE_${label}_PRIVATE_`,ids={source:randomUUID(),memory:randomUUID(),goal:randomUUID(),belief:randomUUID(),relation:randomUUID(),history:randomUUID()};
 const texts={source:prefix+'SOURCE',memory:prefix+'MEMORY',goal:prefix+'GOAL',belief:prefix+'BELIEF',relation:prefix+'RELATION',history:prefix+'HISTORY',draft:prefix+'DRAFT',reason:prefix+'REASON'};
 const kinds={source:'event',memory:'memory',goal:'goal',belief:'belief',relation:'relationship',history:'autobiography'};
 const statements=Object.entries(ids).map(([k,id])=>`INSERT INTO objects(individual_id,id,kind) SELECT id,'${id}',${literal(kinds[k])} FROM individuals`);
 statements.push(`INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,'${ids.source}','message.received','reported_claim','synthetic-owner',clock_timestamp(),jsonb_build_object('text',${literal(texts.source)}),'${ids.source}' FROM individuals`,
 `INSERT INTO memories(individual_id,id,kind,content,scope) SELECT id,'${ids.memory}','semantic',${literal(texts.memory)},'general' FROM individuals`,
 `INSERT INTO memory_sources(individual_id,memory_id,event_id) SELECT id,'${ids.memory}','${ids.source}' FROM individuals`,
 `INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition) SELECT id,'${ids.goal}','event',${literal(texts.goal)},'synthetic criterion','synthetic step','synthetic stop' FROM individuals`,
 `INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis) SELECT id,'${ids.belief}',${literal(texts.belief)},'general','held','synthetic saved basis' FROM individuals`,
 `INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) SELECT id,'${ids.belief}','${ids.source}','support' FROM individuals`,
 `INSERT INTO relationships(individual_id,subject_id,subject_key,expectations) SELECT id,'${ids.relation}',${literal(texts.relation)},jsonb_build_array(${literal(texts.relation)}) FROM individuals`,
 `INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT id,'${ids.relation}','${ids.source}' FROM individuals`,
 `INSERT INTO autobiographies(individual_id,id,topic,title,interpretation) SELECT id,'${ids.history}',${literal('sse_'+label.toLowerCase())},${literal(texts.history)},${literal(texts.history)} FROM individuals`,
 `INSERT INTO autobiography_events(individual_id,autobiography_id,event_id) SELECT id,'${ids.history}','${ids.source}' FROM individuals`);
 for(const [k,id] of Object.entries(ids))if(k!=='source')statements.push(`INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,'${ids.source}','${id}' FROM individuals`);
 sql('BEGIN;'+statements.join(';')+';COMMIT;');return {ids,texts,prefix};
}
async function views(seed,{config=true}={}){
 const group=[];const context=Object.fromEntries(['route','unroute','setOffline','close'].map(method=>[method,async(...args)=>Promise.all(group.map(c=>c[method](...args)))]));
 const pages={},trace=[];let order=0;
 for(const [name,route] of Object.entries({memory:'memory',cognition:'identity/cognition',relations:'identity/relations',history:'identity/history',...(config?{config:'config'}:{})})){
  let isolated=process.argv.includes('--shared-context')?group[0]:null;if(!isolated){isolated=await browser.newContext({viewport:{width:1440,height:1000}});contexts.push(isolated);group.push(isolated);}const page=await isolated.newPage();pages[name]=page;diagnosticPage=page;page.setDefaultTimeout(15000);
  await page.addInitScript(()=>{globalThis.__sseEOF=0;const original=globalThis.fetch;globalThis.fetch=async(...args)=>{const response=await original(...args);if(new URL(response.url,location.href).pathname==='/api/v1/events'&&response.body){const body=response.body,getReader=body.getReader.bind(body);body.getReader=(...options)=>{const reader=getReader(...options),read=reader.read.bind(reader);reader.read=async(...input)=>{const next=await read(...input);if(next.done)globalThis.__sseEOF++;return next;};return reader;};}return response;};});page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{const url=new URL(r.url());if(url.pathname.startsWith('/api/v1/'))trace.push({n:++order,page:name,event:'request',path:url.pathname,cursor:r.headers()['last-event-id']??null});});
  page.on('requestfinished',r=>{const p=new URL(r.url()).pathname;if(p.startsWith('/api/v1/'))trace.push({n:++order,page:name,event:'finished',path:p});});
  page.on('requestfailed',r=>{const p=new URL(r.url()).pathname;trace.push({n:++order,page:name,event:'aborted',path:p});});
  page.on('response',r=>{if(r.status()>=400)trace.push({n:++order,page:name,event:'response',path:new URL(r.url()).pathname,status:r.status()});});
  await page.goto(base+'/#/'+route);await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();await page.locator('.console-status time').waitFor();await page.waitForFunction(()=>document.querySelector('.connection')?.textContent==='实时同步中');
 }
 const m=pages.memory;await m.getByRole('button',{name:'选择记忆：'+seed.texts.memory,exact:true}).click();await m.getByRole('button',{name:'更正此记忆',exact:true}).click();await m.getByLabel('更正后的正文').fill(seed.texts.draft);await m.getByLabel('更正理由').fill(seed.texts.reason);await m.getByRole('button',{name:'检查来源 '+seed.ids.source,exact:true}).click();await m.getByLabel('来源正文片段').waitFor();assert.equal(await m.getByLabel('来源正文片段').textContent(),seed.texts.source);
 const c=pages.cognition;await c.getByRole('button').filter({hasText:seed.texts.belief}).click();await c.getByRole('button').filter({hasText:'检查支持来源'}).click();await c.getByLabel('来源正文片段').waitFor();assert((await c.locator('main').textContent()).includes(seed.texts.goal));
 const r=pages.relations;await r.getByRole('button').filter({hasText:seed.texts.relation}).click();await r.getByRole('button',{name:'刷新所选关系',exact:true}).waitFor();await r.getByText(seed.texts.source,{exact:true}).waitFor();
 const h=pages.history;await h.getByRole('button',{name:'选择自传解释：'+seed.texts.history,exact:true}).click();await h.getByLabel('所选解释正文').waitFor();await h.getByRole('button',{name:'检查自传来源 '+seed.ids.source,exact:true}).click();await h.getByLabel('来源正文片段').waitFor();
 await m.locator('.sidebar nav a[href="#/models"]').click();await m.getByRole('dialog',{name:'记忆更正尚未保存',exact:true}).waitFor();
 if(config){const p=pages.config;await p.getByRole('button',{name:'编辑运行配置',exact:true}).click();await p.getByLabel('后台每日 token 上限',{exact:true}).fill('11013');await p.locator('.sidebar nav a[href="#/models"]').click();await p.getByRole('dialog',{name:'配置尚未保存',exact:true}).waitFor();}
 return {context,pages,trace};
}
async function absent(page,prefix,timeout=6000){await page.waitForFunction(prefix=>!document.querySelector('main')?.textContent?.includes(prefix)&&!Array.from(document.querySelectorAll('dialog')).some(d=>d.textContent?.includes(prefix)),prefix,{timeout});}
async function invalidation(kind,target){
 const v=await views(target),{context,pages,trace}=v;diagnosticPage=pages.cognition;
 const oldState=deferred(),lateState=deferred(),fresh=deferred(),sourceRead=deferred(),lateSource=deferred();[lateState,fresh,lateSource].forEach(x=>releases.add(x.resolve));let changed=false,oldCaptured=false;const freshRows=[];
 const stateRoute=async route=>{try{const response=await route.fetch();if(!changed&&!oldCaptured&&route.request().frame().page()===pages.cognition){oldCaptured=true;oldState.resolve();await lateState.promise;}else if(changed){const body=await response.json();freshRows.push({page:Object.entries(pages).find(([,p])=>p===route.request().frame().page())?.[0],version:body.version,cursor:body.event_cursor,status:response.status()});await fresh.promise;}await route.fulfill({response});}catch{await route.abort().catch(()=>{});}};
 await context.route('**/api/v1/state',stateRoute);await pages.cognition.getByRole('button',{name:'刷新状态',exact:true}).click();await bounded(oldState.promise,'old genuine state200');
 const sourceRoute=async route=>{try{const response=await route.fetch();assert.equal(response.status(),200);sourceRead.resolve();await lateSource.promise;await route.fulfill({response});}catch{await route.abort().catch(()=>{});}};
 await pages.history.route('**/api/v1/source-events/'+target.ids.source+'?*',sourceRoute);await pages.history.getByRole('button',{name:'重新读取来源',exact:true}).click();await bounded(sourceRead.promise,'old genuine source200');
 const before=trace.length;changed=true;
 if(kind==='unknown'){sql("CREATE FUNCTION p02_sse_accept_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private-sse-db-fault'; END $$; CREATE TRIGGER p02_sse_accept_fault BEFORE INSERT ON privacy_operations FOR EACH ROW EXECUTE FUNCTION p02_sse_accept_fault();");fault=true;}
 const deleted=await api('/memories/'+target.ids.memory,{method:'DELETE',headers:{'If-Match':'"1"','Idempotency-Key':randomUUID()}});assert.equal(deleted.status,kind==='unknown'?503:202);
 if(kind==='unknown'){assert.equal((await api('/state')).status,503);assert.equal((await fetch(base+'/healthz')).status,503);}else assert.equal((await api('/memories/'+target.ids.memory)).status,404);
 // Observe real EOF before checking clearing; no old notification or synthetic
 // reset is injected. Fresh snapshots are genuinely fetched, then held above.
 report.trace=trace;for(const p of Object.values(pages))await p.waitForFunction(()=>globalThis.__sseEOF>0,null,{timeout:15000});
 report.last_invalidation={kind,old_eof_pages:Object.keys(pages).length,trace:trace.slice(before)};
 for(const [name,p] of Object.entries(pages)){diagnosticPage=p;try{await absent(p,target.prefix);}catch{report.private_failure={kind,page:name,main_contains_private:(await p.locator('main').textContent()).includes(target.prefix),open_private_dialog:await p.getByRole('dialog',{name:'记忆更正尚未保存',exact:true}).count()};throw new Error(`private DOM remains after actual SSE EOF: ${kind}/${name}`);}}
 lateState.resolve();lateSource.resolve();releases.delete(lateState.resolve);releases.delete(lateSource.resolve);await delay(250);
 for(const p of Object.values(pages))await absent(p,target.prefix);
 assert(trace.slice(before).some(e=>e.event==='aborted'&&e.path==='/api/v1/state'),'old in-flight snapshot must be cancelled');
 await pages.memory.screenshot({path:path.join(output,kind+'-cleared.png'),fullPage:true});
 pass('actual '+kind+' SSE EOF clears memory/source/cognition/relationship/history and private draft; delayed genuine old200 cannot restore private DOM',{actual_status:deleted.status,old_state_cancelled:true});
 if(kind==='unknown'){assert(!JSON.stringify(await pages.memory.locator('body').textContent()).includes('private-sse-db-fault'));sql('DROP TRIGGER p02_sse_accept_fault ON privacy_operations; DROP FUNCTION p02_sse_accept_fault();');fault=false;restart();}
 const state=await api('/state');assert.equal(state.status,200);fresh.resolve();releases.delete(fresh.resolve);await context.unroute('**/api/v1/state',stateRoute);await pages.history.unroute('**/api/v1/source-events/'+target.ids.source+'?*',sourceRoute);
 for(const [name,p] of Object.entries(pages)){report.recovering_page=name;diagnosticPage=p;await p.locator('.console-status time').waitFor({timeout:45000});await p.waitForFunction(()=>['实时同步中','服务已连接'].includes(document.querySelector('.connection')?.textContent??''));await absent(p,target.prefix);assert.equal(await p.getByRole('dialog',{name:'记忆更正尚未保存',exact:true}).count(),0);
  const tail=trace.slice(before).filter(e=>e.page===name);const event=tail.findIndex(e=>e.path==='/api/v1/events'&&e.event==='request');const snapshot=tail.findIndex(e=>e.path==='/api/v1/state'&&e.event==='finished');assert(snapshot>=0&&event>snapshot,'fresh completed snapshot must precede SSE reconnect: '+name);assert.equal(tail[event].cursor,state.data.event_cursor,'new stream uses refreshed event_cursor');}
 const cfg=pages.config,dialog=cfg.getByRole('dialog',{name:'配置尚未保存',exact:true});await dialog.waitFor();await dialog.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await cfg.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'11013');await cfg.locator('.sidebar nav a[href="#/models"]').click();await dialog.getByRole('button',{name:'保存后离开',exact:true}).click();await dialog.getByRole('alert').waitFor();assert.equal(new URL(cfg.url()).hash,'#/config');await cfg.screenshot({path:path.join(output,kind+'-config-guard.png'),fullPage:true});await dialog.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await cfg.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'11013');assert.notEqual((await api('/state')).data.runtime.budgets.background_tokens_per_day,11013);
 pass('actual '+kind+' ready recovery takes a fresh snapshot before subscribing with its cursor; old selection/draft do not return, Config draft/409 guard remains intact',{fresh_state_version:state.data.version,fresh_rows:freshRows});
 await context.close();return deleted;
}
try{
 assert(sql('SELECT current_database()').includes(process.env.ALIA_P02_DATABASE));assert.equal((await api('/state')).status,409,'new private instance required');report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');report.sources={};for(const f of ['web/scripts/test-sse-privacy.mjs','web/src/shared/useIndividual.ts','web/src/features/privacy/PrivacyProvider.tsx','src/service/events.rs','src/adapters/notifications.rs'])report.sources[f]=createHash('sha256').update(await readFile(path.join(root,f))).digest('hex');
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'),'utf8')),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json'),'utf8'));persona.identity.display_name='P02-07 synthetic SSE';runtime.autonomy.enabled=false;assert.equal((await api('/initialize',{method:'POST',body:{persona,runtime}})).status,201);
 const a=seed('DELETE'),independent=seed('SAFE');browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});
 await invalidation('delete',a);
 if(!process.argv.includes('--probe')){
  restart();const b=seed('UNKNOWN');await invalidation('unknown',b);
  const v=await views(independent,{config:false}),{context,pages}=v;diagnosticPage=pages.memory;await context.setOffline(true);for(const p of Object.values(pages))await absent(p,independent.prefix);await context.setOffline(false);for(const p of Object.values(pages)){await p.locator('.console-status time').waitFor();await p.waitForFunction(()=>document.querySelector('.connection')?.textContent==='实时同步中');assert.equal(await p.getByRole('dialog',{name:'记忆更正尚未保存',exact:true}).count(),0);}assert.equal(await pages.memory.getByLabel('更正后的正文').count(),0);assert.equal(await pages.history.getByLabel('所选解释正文').count(),0);pass('actual browser offline hides private DOM, cancels old state and reconnects through fresh data without reviving the prior draft or history selection');
  // Invalid credentials are sent to the actual server; its real401 is forwarded.
  const deny=async route=>{const r=await route.fetch({headers:{...route.request().headers(),authorization:'Bearer invalid-sse-test-token'}});assert.equal(r.status(),401);await route.fulfill({response:r});};await pages.cognition.route('**/api/v1/state',deny);await pages.cognition.getByRole('button',{name:'刷新状态',exact:true}).click();await pages.cognition.getByLabel('实例访问令牌').waitFor();assert(!(await pages.cognition.content()).includes(independent.prefix));assert(!(await pages.cognition.content()).includes(token));pass('actual authenticated state401 clears token and all private DOM rather than reconnecting an unauthorized stream');
  await pages.cognition.unroute('**/api/v1/state',deny);await pages.cognition.route('**/api/v1/events',deny);const sseUnauthorized=pages.cognition.waitForResponse(r=>new URL(r.url()).pathname==='/api/v1/events'&&r.status()===401);await pages.cognition.getByLabel('实例访问令牌').fill(token);await pages.cognition.getByRole('button',{name:'连接 Alia',exact:true}).click();await sseUnauthorized;await pages.cognition.getByLabel('实例访问令牌').waitFor();assert(!(await pages.cognition.content()).includes(independent.prefix));assert(!(await pages.cognition.content()).includes(token));pass('actual SSE401 clears the newly authenticated session and never falls back to an old cursor or cached private snapshot');
  const logoutRead=deferred(),logoutRelease=deferred();releases.add(logoutRelease.resolve);await pages.memory.route('**/api/v1/state',async route=>{try{const r=await route.fetch();assert.equal(r.status(),200);logoutRead.resolve();await logoutRelease.promise;await route.fulfill({response:r});}catch{await route.abort().catch(()=>{});}});await pages.memory.getByRole('button',{name:'刷新状态',exact:true}).click();await bounded(logoutRead.promise,'logout genuine snapshot200');await pages.memory.getByRole('button',{name:'断开',exact:true}).click();await pages.memory.getByLabel('实例访问令牌').waitFor();logoutRelease.resolve();releases.delete(logoutRelease.resolve);await delay(250);assert(!(await pages.memory.content()).includes(independent.prefix));assert(!(await pages.memory.content()).includes(token));assert.equal(await pages.memory.getByLabel('更正后的正文').count(),0);pass('logout cancels a real in-flight snapshot200; its delayed completion cannot restore token, private projection or the old selection');
  for(const name of ['relations','history']){const p=pages[name];await p.getByRole('button',{name:'断开',exact:true}).click();await p.getByLabel('实例访问令牌').waitFor();assert(!(await p.content()).includes(independent.prefix));assert(!(await p.content()).includes(token));}pass('active logout aborts each actual SSE session and clears private selection, draft and token with no automatic reauthentication');await context.close();
 }
 assert.deepEqual(errors,[]);assert.match(sql('SELECT count(*) FROM usage_reservations'),/^\s*0\s*$/m);assert.match(sql('SELECT count(*) FROM runs'),/^\s*0\s*$/m);pass('zero model usage/runs and no uncaught browser errors');report.passed=true;
}catch(error){report.error=error instanceof Error?error.message:'SSE browser failed';process.exitCode=1;console.error(report.error);if(diagnosticPage&&!diagnosticPage.isClosed())await diagnosticPage.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});}
finally{
 for(const release of releases)release();const failures=[];if(fault)try{sql('DROP TRIGGER IF EXISTS p02_sse_accept_fault ON privacy_operations; DROP FUNCTION IF EXISTS p02_sse_accept_fault();');}catch{failures.push('fault cleanup');}
 for(const context of contexts)try{await context.close();}catch{failures.push('context');}if(browser)try{await browser.close();}catch{failures.push('browser');}report.cleanup=failures.length===0;if(failures.length){report.cleanup_errors=failures;report.passed=false;process.exitCode=1;}report.finished_at=new Date().toISOString();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2),{flag:'wx'});console.log('Report: '+path.join(output,'report.json'));
}
