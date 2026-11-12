// P05 fixture-only child: explicit fresh service/DB/binary, parent owns full lock and cleanup.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {createHash,randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {once} from 'node:events';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
for(const key of ['ALIA_TEST_URL','ALIA_TEST_TOKEN','ALIA_TEST_OUTPUT','ALIA_TEST_BINARY','ALIA_P05_DATABASE','ALIA_P05_PSQL_COMMAND'])assert(process.env[key],`Explicit P05 browser requires ${key}`);
assert.equal(process.env.ALIA_P05_LOCK_HELD,'1','Parent must hold the project flock through cleanup');
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=path.resolve(process.env.ALIA_TEST_OUTPUT),database=process.env.ALIA_P05_DATABASE;
assert(['127.0.0.1','localhost','[::1]'].includes(new URL(base).hostname));assert.match(database,/^alia_(p05|p03|web_test)_[a-z0-9_]+$/);
const sqlArgv=JSON.parse(process.env.ALIA_P05_PSQL_COMMAND);assert(Array.isArray(sqlArgv)&&sqlArgv.length>1&&sqlArgv.every(v=>typeof v==='string'));
function sql(statement){const r=spawnSync(sqlArgv[0],[...sqlArgv.slice(1),'-A','-t','-v','ON_ERROR_STOP=1','-c',statement],{cwd:root,encoding:'utf8',timeout:20000});if(r.status!==0)throw new Error('P05 isolated fixture SQL failed');return r.stdout.trim();}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function exited(child){if(child.exitCode!==null||child.signalCode!==null)return child.exitCode;let timer;try{return await Promise.race([once(child,'exit').then(([code])=>code),new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('P05 fixture child exit timeout')),12000);})]);}finally{clearTimeout(timer);}}
await mkdir(output,{recursive:false});
const report={task:'ALIA-P05-01',started_at:new Date().toISOString(),fixture:'synthetic data; real HTTP/PostgreSQL/browser; no HTTP mocks',checks:[],passed:false,model_calls:0};
const pass=(name,details={})=>{report.checks.push({name,...details});console.log(`PASS ${name}`);};
let browser,context,page,slow;
async function api(route,body){const r=await fetch(base+'/api/v1'+route,{method:body?'POST':'GET',headers:{Authorization:'Bearer '+token,...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{})},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(15000)});assert(r.ok,`fixture HTTP ${r.status}`);return r.json();}
async function locked(){for(let i=0;i<50;i++){if(sql("SELECT count(*) FROM pg_locks WHERE relation='documents'::regclass AND mode='AccessExclusiveLock' AND granted")!=='0')return;await delay(100);}throw new Error('fixture table lock not observed');}
try{
 assert(sql('SELECT current_database()').includes(database));
 const initial=await fetch(base+'/api/v1/state',{headers:{Authorization:'Bearer '+token},signal:AbortSignal.timeout(15000)});assert.equal(initial.status,409,'fresh disposable instance required');
 report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');
 report.sources={};for(const file of ['src/adapters/documents_view.rs','src/service/documents_view.rs','web/src/features/Documents.tsx','web/src/features/documents/useDocumentsRead.ts','web/src/features/Settings.tsx'])report.sources[file]=createHash('sha256').update(await readFile(path.join(root,file))).digest('hex');
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'),'utf8')),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json'),'utf8'));
 runtime.autonomy.enabled=false;persona.identity.display_name='P05 隔离资料检查';await api('/initialize',{persona,runtime});
 const body='甲🙂e\u0301乙\n<img src="https://p05.invalid/resource" onerror="window.__p05Injected=1">\nPRIVATE-P05-ONLY\n'+'文'.repeat(5000);
 const target=await api('/documents',{title:'目标资料🙂%_',text:body,source_label:'<script>window.__p05Injected=1</script>'});
 const parent=await api('/documents',{title:'上游资料',text:'ancestor fixture',source_label:'独立合成资料'});
 const middle=await api('/documents',{title:'中间资料',text:'middle fixture',source_label:'独立合成资料'});
 const safe=await api('/documents',{title:'独立安全资料',text:'safe-text',source_label:'公开来源标签'});
 for(let n=0;n<35;n++)await api('/documents',{title:`分页资料 ${String(n).padStart(2,'0')}`,text:'page fixture',source_label:'目录数据'});
 sql(`UPDATE events SET recorded_at='2026-01-01T00:00:00Z' WHERE id='${target.event_id}'; INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,'${parent.event_id}'::uuid,'${middle.event_id}'::uuid FROM individuals UNION ALL SELECT id,'${middle.event_id}'::uuid,'${target.event_id}'::uuid FROM individuals;`);
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});context=await browser.newContext({viewport:{width:1440,height:1000}});page=await context.newPage();page.setDefaultTimeout(12000);
 const errors=[],requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.url().includes('/api/v1/documents'))requests.push(new URL(r.url()).pathname+new URL(r.url()).search);});
 await page.goto(base+'/#/settings');await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();
 const directory=page.getByRole('region',{name:'资料列表'}),inspector=page.getByRole('region',{name:'资料阅读检查器'}),text=page.getByLabel('资料正文片段');
 await directory.locator('button.list-item').first().waitFor();assert.equal(await directory.locator('button.list-item').count(),30);assert.equal(await text.count(),0,'metadata listing must not auto-read body');
 await page.getByRole('button',{name:'下一页资料',exact:true}).click();await directory.getByRole('button',{name:/目标资料/}).waitFor();assert.equal(await directory.locator('button.list-item').count(),9);await page.getByRole('button',{name:'返回资料首页',exact:true}).click();await directory.locator('button.list-item').first().waitFor();
 const search=page.getByRole('searchbox',{name:'搜索资料标题或来源'});await search.fill('%_');await page.getByRole('button',{name:'检索资料',exact:true}).click();await directory.getByRole('button',{name:/目标资料/}).waitFor();assert.equal(await directory.locator('button.list-item').count(),1);
 await directory.getByRole('button',{name:/目标资料/}).focus();await page.keyboard.press('Enter');await text.waitFor();assert.equal(await text.innerText(),Array.from(body).slice(0,2000).join(''));assert.equal(await inspector.locator('img,script,iframe').count(),0);assert.equal(await page.evaluate(()=>window.__p05Injected),undefined);assert.equal(requests.filter(x=>x.includes('/'+target.event_id)).length>=1,true);
 assert(!requests.some(x=>x.includes('max_chars=')&&!x.includes('max_chars=2000')));pass('full directory beyond 30/20, literal search, keyboard selection, bounded Unicode and safe imported HTML');
 await page.getByRole('button',{name:'下一段正文',exact:true}).click();await page.waitForFunction(()=>document.querySelector('[aria-label="资料正文片段"]')?.textContent==='文'.repeat(2000));assert(requests.at(-1).includes('offset=2000'));
 await page.getByRole('button',{name:'上一段正文',exact:true}).click();await page.waitForFunction(()=>document.querySelector('[aria-label="资料正文片段"]')?.textContent?.startsWith('甲🙂'));
 for(const width of [360,390,1440]){await page.setViewportSize({width,height:1000});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no page overflow ${width}`);await text.evaluate(el=>{el.scrollTop=0;el.scrollLeft=0;});await text.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,`documents-${width}.png`)});await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:path.join(output,`documents-${width}-top.png`)});}pass('fragment paging and 360/390/1440 real layouts');
 const snapshot=()=>sql("SELECT jsonb_build_object('state',(SELECT state_version FROM individuals),'events',(SELECT count(*) FROM events),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions),'usage',(SELECT count(*) FROM usage_reservations))");
 const before=snapshot();for(let i=0;i<3;i++)await page.getByRole('button',{name:'重新读取片段',exact:true}).click();await text.waitFor();assert.equal(snapshot(),before);pass('owner browser reads do not create events, tool receipts, runs or usage');
 // Real delayed database read with a two-generation privacy revocation and SSE invalidation.
 slow=spawn(sqlArgv[0],[...sqlArgv.slice(1),'-v','ON_ERROR_STOP=1','-c',"BEGIN; LOCK TABLE documents IN ACCESS EXCLUSIVE MODE; SELECT pg_sleep(5); COMMIT;"],{cwd:root,stdio:'ignore'});await locked();
 await page.getByRole('button',{name:'重新读取片段',exact:true}).click();await delay(200);
 await page.evaluate(()=>{window.__p05Seen=[];window.__p05Watch=new MutationObserver(()=>window.__p05Seen.push(document.querySelector('[aria-label="资料正文片段"]')?.textContent??''));window.__p05Watch.observe(document.querySelector('main'),{subtree:true,childList:true,characterData:true});});
 sql(`UPDATE objects SET blocked=true WHERE id='${parent.event_id}'; UPDATE individuals SET state_version=state_version+1,privacy_epoch=privacy_epoch+1; INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'privacy.changed',id,state_version FROM individuals;`);
 assert.equal(await exited(slow),0);slow=null;await page.getByText('当前可见资料中没有匹配项。',{exact:true}).waitFor();assert.equal(await text.count(),0);const seen=await page.evaluate(()=>{window.__p05Watch.disconnect();return window.__p05Seen;});assert(!seen.some(v=>v.includes('PRIVATE-P05-ONLY')));pass('two-generation revocation and real slow response cannot restore private body after SSE');
 await page.getByRole('button',{name:'清除资料筛选',exact:true}).click();await directory.locator('button.list-item').first().waitFor();
 // Preserve the original import form and actual write path.
 await page.getByRole('button',{name:'导入文本资料',exact:true}).click();await page.getByLabel('资料标题',{exact:true}).fill('P05 UI 导入');await page.getByLabel('正文',{exact:true}).fill('只在隔离测试中输入的资料正文');await page.getByRole('button',{name:'保存资料',exact:true}).click();await directory.getByRole('button',{name:/P05 UI 导入/}).waitFor();pass('existing import form still persists actual text and refreshes directory');
 await search.fill('独立安全资料');await page.getByRole('button',{name:'检索资料',exact:true}).click();await directory.getByRole('button',{name:/独立安全资料/}).click();await text.waitFor();assert.equal(await text.innerText(),'safe-text');
 sql('ALTER TABLE documents RENAME TO documents_p05_fault');await page.getByRole('button',{name:'刷新资料目录',exact:true}).click();await page.locator('.resource-error[role="alert"]').waitFor();assert.equal(await text.count(),0);assert.equal(await directory.locator('button.list-item').count(),0);
 sql('ALTER TABLE documents_p05_fault RENAME TO documents');await page.locator('.resource-error').getByRole('button',{name:'重新加载',exact:true}).click();await directory.getByRole('button',{name:/独立安全资料/}).waitFor();pass('real SQL query failure clears old directory/body; retry restores real data');
 await directory.getByRole('button',{name:/独立安全资料/}).click();await text.waitFor();await context.setOffline(true);await page.getByText('资料连接不可用，已清除页面中的资料内容。',{exact:true}).waitFor();assert.equal(await text.count(),0);assert.equal(await directory.locator('button.list-item').count(),0);
 await context.setOffline(false);await directory.getByRole('button',{name:/独立安全资料/}).waitFor();await directory.getByRole('button',{name:/独立安全资料/}).click();await text.waitFor();pass('real browser offline clears private views; online reconnect recovers');
 await page.getByRole('button',{name:'断开',exact:true}).click();await page.getByLabel('实例访问令牌').waitFor();assert.equal(await text.count(),0);assert.equal(await directory.count(),0);assert.deepEqual(await page.evaluate(()=>({local:Object.keys(localStorage),session:Object.keys(sessionStorage)})),{local:[],session:[]});assert.equal((await page.content()).includes(token),false);assert.equal((await page.content()).includes('PRIVATE-P05-ONLY'),false);
 assert.equal(sql('SELECT count(*) FROM usage_reservations'),'0');assert.equal(sql("SELECT count(*) FROM events WHERE kind='tool.result'"),'0');assert.deepEqual(errors,[]);pass('session exit clears data; no model/tool reservations or fake reading receipts');report.passed=true;
}catch(error){report.error=error instanceof Error?error.message:'unknown failure';process.exitCode=1;console.error(report.error);if(page)await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});}
finally{
 const cleanup=[];
 try{if(slow){if(slow.exitCode===null&&slow.signalCode===null)slow.kill('SIGTERM');await exited(slow);}}catch{cleanup.push('slow fixture process cleanup failed');}
 try{if(context)await context.close();if(browser)await browser.close();}catch{cleanup.push('browser cleanup failed');}
 report.cleanup=cleanup.length===0;report.cleanup_errors=cleanup;if(cleanup.length){report.passed=false;process.exitCode=1;}report.finished_at=new Date().toISOString();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2),{flag:'wx'});console.log(`P05 browser report: ${path.join(output,'report.json')}`);
}
