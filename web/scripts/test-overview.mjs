// Explicit fixture-only browser child. Parent holds global flock and owns frozen
// service/web artifacts and the fresh disposable test database/lifecycle.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {once} from 'node:events';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
for(const key of ['ALIA_TEST_URL','ALIA_TEST_TOKEN','ALIA_TEST_OUTPUT','ALIA_TEST_BINARY','ALIA_P02_DATABASE','ALIA_P02_PSQL_COMMAND'])assert(process.env[key],`Explicit overview test requires ${key}`);
assert.equal(process.env.ALIA_P02_LOCK_HELD,'1','Parent must hold /tmp/alia-project-heavy.lock through cleanup');
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=path.resolve(process.env.ALIA_TEST_OUTPUT),database=process.env.ALIA_P02_DATABASE;
assert(['127.0.0.1','localhost','[::1]'].includes(new URL(base).hostname));assert.match(database,/^alia_(p02|p03|web_test)_[a-z0-9_]+$/);
const sqlArgv=JSON.parse(process.env.ALIA_P02_PSQL_COMMAND);assert(Array.isArray(sqlArgv)&&sqlArgv.length>1&&sqlArgv.every(v=>typeof v==='string'));
function sql(statement){const r=spawnSync(sqlArgv[0],[...sqlArgv.slice(1),'-v','ON_ERROR_STOP=1','-c',statement],{cwd:root,encoding:'utf8'});assert.equal(r.status,0,'isolated test SQL failed');return r.stdout;}
async function api(route,body){const r=await fetch(base+'/api/v1'+route,{method:body?'POST':'GET',headers:{Authorization:'Bearer '+token,...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{})},body:body?JSON.stringify(body):undefined});assert(r.ok,`fixture HTTP ${r.status}`);return r.json();}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
await mkdir(path.dirname(output),{recursive:true});await mkdir(output);
const report={task:'ALIA-P02-02',started_at:new Date().toISOString(),fixture:'synthetic; actual HTTP/PostgreSQL; no models',checks:[],passed:false,cleanup:false,lifecycle:'parent owns service/database; child owns browser'};
const pass=(name,extra={})=>{report.checks.push({name,...extra});console.log('PASS '+name);};
let browser,context,page,slow,slowExit;const errors=[];
try{
 report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');report.sources={};
 for(const file of ['src/adapters/overview.rs','src/service/overview.rs','web/src/features/Overview.tsx','web/src/features/OverviewHealth.tsx','web/src/features/overview/types.ts','web/src/features/overview/useOverview.ts','web/src/features/overview/overview.css','web/scripts/test-overview.mjs'])report.sources[file]=createHash('sha256').update(await readFile(path.join(root,file))).digest('hex');
 assert(sql('SELECT current_database()').includes(database),'SQL command targets the declared test DB');
 const empty=await fetch(base+'/api/v1/state',{headers:{Authorization:'Bearer '+token}});assert.equal(empty.status,409,'requires fresh isolated instance');
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'),'utf8')),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json'),'utf8'));persona.identity.display_name='P02-02 概览测试';await api('/initialize',{persona,runtime});
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});context=await browser.newContext({viewport:{width:1440,height:1000}});page=await context.newPage();page.on('pageerror',error=>errors.push(error.message));
 const total=page.getByTestId('overview-memories-total');const waitTotal=async expected=>page.waitForFunction(expected=>document.querySelector('[data-testid="overview-memories-total"]')?.textContent===expected,expected);
 await page.goto(base);await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();await page.locator('.console-status time').waitFor();await waitTotal('0');await page.waitForFunction(()=>document.querySelectorAll('.overview-trend-table tbody tr').length===7);await page.getByText('没有当前可见的失败运行；这不表示模型或工具已通过健康检查。',{exact:true}).waitFor();pass('empty initialized state renders actual zero counts and seven UTC buckets');
 assert.equal(await page.locator('.overview-trend-table tbody tr').count(),7);assert.equal(await page.getByText('今日 · 部分日期',{exact:true}).count(),1);
 sql(`DO $$ DECLARE ind uuid; e uuid; m uuid; g uuid; r uuid; n integer; BEGIN SELECT id INTO ind FROM individuals;
 FOR n IN 0..64 LOOP e:=gen_random_uuid();m:=gen_random_uuid();g:=gen_random_uuid();r:=gen_random_uuid();
 INSERT INTO objects(individual_id,id,kind) VALUES(ind,e,'event'),(ind,m,'memory'),(ind,g,'goal'),(ind,r,'run');
 INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES(ind,e,'overview.fixture','reported_claim','fixture-'||n,clock_timestamp(),'{"text":"overview-private-body-marker"}',r);
 INSERT INTO memories(individual_id,id,kind,content,scope,validity) VALUES(ind,m,'semantic','overview-private-memory-marker','general',(ARRAY['active','disputed','superseded','stale'])[n%4+1]);
 INSERT INTO memory_sources(individual_id,memory_id,event_id) VALUES(ind,m,e);
 INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition,status) VALUES(ind,g,'event','synthetic goal '||n,'criterion','next','stop',(ARRAY['proposed','active','waiting','completed','abandoned'])[n%5+1]);
 INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,status,created_at,error_code) VALUES(ind,r,e,CASE WHEN n%2=0 THEN 'background' ELSE 'foreground' END,1,0,1,(ARRAY['failed','committed','cancelled','stale'])[n%4+1],(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')+n*interval '1 second',CASE WHEN n=64 THEN 'private-raw-provider-error-marker' ELSE 'model_failure' END);
 INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES(ind,e,m),(ind,e,g),(ind,e,r);
 END LOOP;INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'state.changed',id,state_version FROM individuals;END $$;`);
 await page.waitForFunction(()=>document.querySelector('[data-testid="overview-memories-total"]')?.textContent==='65',{timeout:10000});
 assert.equal(await page.getByTestId('overview-goals-total').textContent(),'65');assert.equal(await page.getByTestId('overview-runs-total').textContent(),'65');
 const expected=await api('/overview');assert.equal(expected.totals.memories.total,65);assert.equal((await api('/memories')).items.length,30);
 assert.equal(await page.locator('.overview-failure').count(),5);const trendRows=await page.locator('.overview-trend-table tbody tr').allTextContents();assert.equal(trendRows.length,7);assert((await page.locator('.overview-trend-table').textContent()).includes('UTC'));assert((await page.locator('main').textContent()).includes('并非失败发生日'));
 pass('SSE refreshes true full totals beyond first-page 30; trend scope and recent five failures');
 assert.equal((await page.locator('main').textContent()).includes('private-raw-provider-error-marker'),false);assert.equal((await page.locator('main').textContent()).includes('overview-private-body-marker'),false);
 const failure=page.getByRole('link',{name:`查看失败运行 ${expected.recent_failed_runs[0].id}`,exact:true});await failure.focus();await page.keyboard.press('Enter');await page.waitForFunction(id=>location.hash==='#/data/traces/'+id,expected.recent_failed_runs[0].id);await page.getByRole('heading',{name:'运行详情与步骤',exact:true}).waitFor();
 await page.evaluate(()=>{location.hash='#/overview';});await waitTotal('65');pass('keyboard failure entry reaches existing real trace detail; safe public errors only');
 // A delayed real DB read must not republish an older overview after SSE changes state.
 const revokedMemory=sql('SELECT id FROM memories ORDER BY id LIMIT 1').trim();assert.match(revokedMemory,/^[0-9a-f-]{36}$/);
 slow=spawn(sqlArgv[0],[...sqlArgv.slice(1),'-v','ON_ERROR_STOP=1','-c',"BEGIN; LOCK TABLE memories IN ACCESS EXCLUSIVE MODE; SELECT pg_sleep(5); COMMIT;"],{cwd:root,stdio:'ignore'});slowExit=once(slow,'exit');
 let locked=false;for(let i=0;i<40;i++){if(sql("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='memories'::regclass AND mode='AccessExclusiveLock' AND granted)").trim()==='t'){locked=true;break;}await delay(50);}assert(locked,'real slow-read fixture acquired the memory table lock');
 await page.getByRole('button',{name:'刷新概览统计',exact:true}).click();await page.locator('.overview-reading [role="status"]').waitFor();assert.equal(await total.count(),0,'loading clears old private statistics');
 await page.evaluate(()=>{window.__overviewSeen=[];window.__overviewWatch=new MutationObserver(()=>{const value=document.querySelector('[data-testid="overview-memories-total"]')?.textContent;if(value)window.__overviewSeen.push(value);});window.__overviewWatch.observe(document.querySelector('main'),{subtree:true,childList:true,characterData:true});});
 const changedState=page.waitForResponse(async r=>r.url().endsWith('/api/v1/state')&&r.ok()&&(await r.json()).version===2);
 sql(`UPDATE objects SET blocked=true WHERE id='${revokedMemory}'; UPDATE individuals SET state_version=state_version+1,privacy_epoch=privacy_epoch+1; INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'privacy.changed',id,state_version FROM individuals;`);
 await changedState;await delay(80);
 await slowExit;slow=null;await page.waitForFunction(()=>document.querySelector('[data-testid="overview-memories-total"]')?.textContent==='64');
 const seen=await page.evaluate(()=>{window.__overviewWatch.disconnect();return window.__overviewSeen;});assert(!seen.includes('65'),'old response must not restore revoked statistic');pass('real delayed HTTP/SQL response is discarded after SSE state/privacy invalidation');
 sql('ALTER TABLE memories RENAME TO overview_fault_fixture');await page.getByRole('button',{name:'刷新概览统计',exact:true}).click();await page.locator('.resource-error[role="alert"]').waitFor();assert.equal(await total.count(),0);assert.equal(await page.locator('.overview-failure').count(),0);
 sql('ALTER TABLE overview_fault_fixture RENAME TO memories');await page.locator('.resource-error').getByRole('button',{name:'重新加载',exact:true}).click();await waitTotal('64');pass('actual backend query failure is an error without stale/zero-success stats; retry recovers');
 await context.setOffline(true);await page.getByRole('button',{name:'刷新概览统计',exact:true}).click();await page.locator('.resource-error[role="alert"]').waitFor();assert.equal(await total.count(),0);assert.equal(await page.locator('.overview-failure').count(),0);
 await context.setOffline(false);await page.waitForFunction(()=>document.querySelector('[data-testid="overview-memories-total"]')?.textContent==='64');pass('offline clears statistics; reconnection refreshes current visible totals');
 const acknowledge=page.getByRole('button',{name:'关闭提示',exact:true});if(await acknowledge.count())await acknowledge.click();
 // A keyboard user can reach and scroll the trend table at all tested widths.
 for(const width of [360,390,1440]){await page.setViewportSize({width,height:1000});await delay(150);await waitTotal('64');const scroll=page.locator('[aria-label="UTC 七日运行趋势，可水平滚动"]');await scroll.focus();await page.keyboard.press('ArrowRight');assert(await scroll.evaluate(el=>el===document.activeElement));if(await scroll.evaluate(el=>el.scrollWidth>el.clientWidth))await page.waitForFunction(()=>document.querySelector('[aria-label="UTC 七日运行趋势，可水平滚动"]').scrollLeft>0);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no page overflow at ${width}`);await page.evaluate(()=>{document.activeElement?.blur();document.querySelector('[aria-label="UTC 七日运行趋势，可水平滚动"]').scrollLeft=0;window.scrollTo(0,0);});await delay(120);await page.screenshot({path:path.join(output,`overview-${width}.png`),fullPage:true});}
 pass('360/390/1440 layouts, focusable trend scrolling, and screenshots');
 const before=sql("SELECT state_version,privacy_epoch,(SELECT count(*) FROM events),(SELECT count(*) FROM usage_reservations) FROM individuals");await api('/overview');await api('/overview');const after=sql("SELECT state_version,privacy_epoch,(SELECT count(*) FROM events),(SELECT count(*) FROM usage_reservations) FROM individuals");assert.equal(after,before);assert.match(sql('SELECT count(*) FROM usage_reservations'),/^\s*0\s*$/m);pass('overview reads leave state/events/usage unchanged; zero model reservations');
 await page.setViewportSize({width:1440,height:1000});await page.getByRole('button',{name:'断开',exact:true}).click();await page.getByLabel('实例访问令牌').waitFor();assert.equal(await total.count(),0);assert.equal(await page.locator('.overview-failure,.overview-trend-table').count(),0);assert.deepEqual(await page.evaluate(()=>({local:Object.keys(localStorage),session:Object.keys(sessionStorage)})),{local:[],session:[]});assert(!(await page.content()).includes(token));assert.deepEqual(errors,[]);pass('session exit clears all overview data and browser storage');report.passed=true;
}catch(error){process.exitCode=1;report.error=error instanceof Error?error.message:'test failed';console.error(report.error);if(page&&!(await page.getByLabel('实例访问令牌').count())){await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});report.visible_text=(await page.locator('main').innerText().catch(()=>'' )).slice(0,6000);}}
finally{
 const failures=[];if(slow){try{await slowExit;}catch{failures.push('SQL delay process cleanup failed');}}
 try{if(context)await context.close();}catch{failures.push('browser context cleanup failed');}try{if(browser)await browser.close();}catch{failures.push('browser cleanup failed');}
 report.cleanup=failures.length===0;if(failures.length){report.cleanup_errors=failures;report.passed=false;process.exitCode=1;}report.finished_at=new Date().toISOString();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2),{flag:'wx'});console.log('Report: '+path.join(output,'report.json'));
}
