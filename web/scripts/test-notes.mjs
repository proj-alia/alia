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
function sql(statement){const r=spawnSync(sqlArgv[0],[...sqlArgv.slice(1),'-A','-t','-v','ON_ERROR_STOP=1','-c',statement],{cwd:root,encoding:'utf8',timeout:20000});if(r.status!==0)throw new Error('P05 isolated fixture SQL failed: '+(r.stderr.split('\n').find(line=>line.startsWith('ERROR:'))??'unknown SQL error').slice(0,240));return r.stdout.trim();}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function exited(child){if(child.exitCode!==null||child.signalCode!==null)return child.exitCode;let timer;try{return await Promise.race([once(child,'exit').then(([code])=>code),new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('P05 fixture child exit timeout')),12000);})]);}finally{clearTimeout(timer);}}
await mkdir(output,{recursive:false});
const report={task:'ALIA-P05-02',started_at:new Date().toISOString(),fixture:'synthetic succeeded notes.write relational fixtures; real HTTP/PostgreSQL/browser; Rust separately executes actual local capability; no HTTP mocks',checks:[],passed:false,model_calls:0};
const pass=(name,details={})=>{report.checks.push({name,...details});console.log(`PASS ${name}`);};
let browser,context,page,slow;
async function api(route,body){const r=await fetch(base+'/api/v1'+route,{method:body?'POST':'GET',headers:{Authorization:'Bearer '+token,...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{})},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(15000)});assert(r.ok,`fixture HTTP ${r.status}`);return r.json();}
async function locked(){for(let i=0;i<50;i++){if(sql("SELECT count(*) FROM pg_locks WHERE relation='notes'::regclass AND mode='AccessExclusiveLock' AND granted")!=='0')return;await delay(100);}throw new Error('fixture table lock not observed');}
try{
 assert(sql('SELECT current_database()').includes(database));
 const initial=await fetch(base+'/api/v1/state',{headers:{Authorization:'Bearer '+token},signal:AbortSignal.timeout(15000)});assert.equal(initial.status,409,'fresh disposable instance required');
 report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');
 report.sources={};for(const file of ['src/adapters/notes_view.rs','src/service/notes_view.rs','web/src/features/Notes.tsx','web/src/features/notes/useNotesRead.ts','web/src/features/Settings.tsx'])report.sources[file]=createHash('sha256').update(await readFile(path.join(root,file))).digest('hex');
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'),'utf8')),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json'),'utf8'));
 runtime.autonomy.enabled=false;persona.identity.display_name='P05 隔离笔记检查';await api('/initialize',{persona,runtime});
 const body='甲🙂e\u0301乙\n<img src="https://p05.invalid/resource" onerror="window.__p05Injected=1">\nPRIVATE-P05-ONLY\n'+'文'.repeat(3000);
 function seedNote(title,body){const id=randomUUID(),run=randomUUID(),decision=randomUUID(),result=randomUUID();const literal=v=>"'"+String(v).replaceAll("'","''")+"'";
 sql(`INSERT INTO objects(individual_id,id,kind) SELECT id,'${run}'::uuid,'run' FROM individuals UNION ALL SELECT id,'${decision}'::uuid,'event' FROM individuals UNION ALL SELECT id,'${id}'::uuid,'action' FROM individuals UNION ALL SELECT id,'${result}'::uuid,'event' FROM individuals;
 INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) SELECT id,'${decision}'::uuid,'decision.committed','generated','fixture',now(),'{}','${run}'::uuid FROM individuals UNION ALL SELECT id,'${result}'::uuid,'tool.result','direct_observation','capability',now(),jsonb_build_object('action_id','${id}'::uuid,'capability_id','notes.write','capability_version','1','status','succeeded','executed',true,'output',jsonb_build_object('note_id','${id}'::uuid)),'${run}'::uuid FROM individuals;
 INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,status) SELECT id,'${run}'::uuid,'${decision}'::uuid,'foreground',state_version,privacy_epoch,config_version,'committed' FROM individuals;
 INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments,status,result_event_id) SELECT id,'${id}'::uuid,'${run}'::uuid,'${decision}'::uuid,1,'notes.write','1','{}','succeeded','${result}'::uuid FROM individuals;
 INSERT INTO notes(individual_id,action_id,title,body) SELECT id,'${id}'::uuid,${literal(title)},${literal(body)} FROM individuals;`);return {event_id:id,result_id:result,run_id:run};}
 const target=seedNote('目标笔记🙂%_',body),parent=seedNote('上游笔记','ancestor fixture'),middle=seedNote('中间笔记','middle fixture'),safe=seedNote('独立安全笔记','safe-text');
 for(let n=0;n<35;n++)seedNote(`分页笔记 ${String(n).padStart(2,'0')}`,'page fixture');
 sql(`UPDATE notes SET created_at='2026-01-01T00:00:00Z' WHERE action_id='${target.event_id}'; INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT id,'${parent.event_id}'::uuid,'${middle.event_id}'::uuid FROM individuals UNION ALL SELECT id,'${middle.event_id}'::uuid,'${target.result_id}'::uuid FROM individuals;`);
 const initialToolResults=sql("SELECT count(*) FROM events WHERE kind='tool.result'");

 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});context=await browser.newContext({viewport:{width:1440,height:1000}});page=await context.newPage();page.setDefaultTimeout(12000);
 const errors=[],requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.url().includes('/api/v1/notes'))requests.push(new URL(r.url()).pathname+new URL(r.url()).search);});
 await page.goto(base+'/#/settings');await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();
 const directory=page.getByRole('region',{name:'笔记列表'}),inspector=page.getByRole('region',{name:'笔记阅读检查器'}),text=page.getByLabel('笔记正文片段');
 await page.getByText('实时同步中',{exact:true}).waitFor();await page.waitForFunction(()=>document.querySelectorAll('[aria-label="笔记列表"] button.list-item').length===30);assert.equal(await directory.locator('button.list-item').count(),30);assert.equal(await text.count(),0,'metadata listing must not auto-read body');
 await page.getByRole('button',{name:'下一页笔记',exact:true}).click();await directory.getByRole('button',{name:/目标笔记/}).waitFor();await page.waitForFunction(()=>document.querySelectorAll('[aria-label="笔记列表"] button.list-item').length===9);assert.equal(await directory.locator('button.list-item').count(),9);await page.getByRole('button',{name:'返回笔记首页',exact:true}).click();await directory.locator('button.list-item').first().waitFor();
 const search=page.getByRole('searchbox',{name:'搜索笔记标题或动作标识'});await search.fill('%_');await page.getByRole('button',{name:'检索笔记',exact:true}).click();await directory.getByRole('button',{name:/目标笔记/}).waitFor();await page.waitForFunction(()=>document.querySelectorAll('[aria-label="笔记列表"] button.list-item').length===1);assert.equal(await directory.locator('button.list-item').count(),1);
 await directory.getByRole('button',{name:/目标笔记/}).focus();await page.keyboard.press('Enter');await text.waitFor();assert.equal(await text.innerText(),Array.from(body).slice(0,2000).join(''));assert.equal(await inspector.locator('img,script,iframe').count(),0);assert.equal(await page.evaluate(()=>window.__p05Injected),undefined);assert.equal(requests.filter(x=>x.includes('/'+target.event_id)).length>=1,true);
 assert(!requests.some(x=>x.includes('max_chars=')&&!x.includes('max_chars=2000')));assert.equal(await inspector.getByRole('link').getAttribute('href'),`#/traces/${target.run_id}`);assert((await inspector.innerText()).includes(target.result_id));pass('full directory beyond 30/20, literal search, keyboard selection, bounded Unicode, escaped HTML and actual save provenance links');
 await page.getByRole('button',{name:'下一段正文',exact:true}).click();await page.waitForFunction(expected=>document.querySelector('[aria-label="笔记正文片段"]')?.textContent===expected,Array.from(body).slice(2000,4000).join(''));assert(requests.at(-1).includes('offset=2000'));
 await page.getByRole('button',{name:'上一段正文',exact:true}).click();await page.waitForFunction(()=>document.querySelector('[aria-label="笔记正文片段"]')?.textContent?.startsWith('甲🙂'));
 for(const width of [360,390,1440]){await page.setViewportSize({width,height:1000});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no page overflow ${width}`);await text.evaluate(el=>{el.scrollTop=0;el.scrollLeft=0;});await text.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,`notes-${width}.png`)});await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:path.join(output,`notes-${width}-top.png`)});}await page.getByRole('button',{name:'切换显示主题',exact:true}).click();await text.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(output,'notes-dark.png')});await page.getByRole('button',{name:'切换显示主题',exact:true}).click();pass('fragment paging, dark theme and 360/390/1440 real layouts');
 const snapshot=()=>sql("SELECT jsonb_build_object('state',(SELECT state_version FROM individuals),'events',(SELECT count(*) FROM events),'runs',(SELECT count(*) FROM runs),'actions',(SELECT count(*) FROM actions),'usage',(SELECT count(*) FROM usage_reservations))");
 const before=snapshot();for(let i=0;i<3;i++)await page.getByRole('button',{name:'重新读取片段',exact:true}).click();await text.waitFor();assert.equal(snapshot(),before);pass('owner browser reads do not create events, tool receipts, runs or usage');
 // Real delayed database read with a two-generation privacy revocation and SSE invalidation.
 slow=spawn(sqlArgv[0],[...sqlArgv.slice(1),'-v','ON_ERROR_STOP=1','-c',"BEGIN; LOCK TABLE notes IN ACCESS EXCLUSIVE MODE; SELECT pg_sleep(5); COMMIT;"],{cwd:root,stdio:'ignore'});await locked();
 await page.getByRole('button',{name:'重新读取片段',exact:true}).click();await delay(200);
 await page.evaluate(()=>{window.__p05Seen=[];window.__p05Watch=new MutationObserver(()=>window.__p05Seen.push(document.querySelector('[aria-label="笔记正文片段"]')?.textContent??''));window.__p05Watch.observe(document.querySelector('main'),{subtree:true,childList:true,characterData:true});});
 sql(`UPDATE objects SET blocked=true WHERE id='${parent.event_id}'; UPDATE individuals SET state_version=state_version+1,privacy_epoch=privacy_epoch+1; INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'privacy.changed',id,state_version FROM individuals;`);
 assert.equal(await exited(slow),0);slow=null;await page.getByText('当前可见笔记中没有匹配项。',{exact:true}).waitFor();assert.equal(await text.count(),0);const seen=await page.evaluate(()=>{window.__p05Watch.disconnect();return window.__p05Seen;});assert(!seen.some(v=>v.includes('PRIVATE-P05-ONLY')));pass('two-generation revocation and real slow response cannot restore private body after SSE');
 await page.getByRole('button',{name:'清除笔记筛选',exact:true}).click();await directory.locator('button.list-item').first().waitFor();
 assert.equal(await inspector.getByText('生成内容 · 已保存',{exact:true}).count(),0); // revoked detail is absent
 assert.equal(await page.getByText('生成笔记可能包含推断或错误；保存成功只证明内容已保存，不代表事实已核实。页面查看不会产生模型阅读或工具执行记录。',{exact:true}).count(),1);
 pass('generated content is labeled honestly; no browser-read receipt');
 await search.fill('独立安全笔记');await page.getByRole('button',{name:'检索笔记',exact:true}).click();await directory.getByRole('button',{name:/独立安全笔记/}).click();await text.waitFor();assert.equal(await text.innerText(),'safe-text');
 sql('ALTER TABLE notes RENAME TO notes_p05_fault');await page.getByRole('button',{name:'刷新笔记目录',exact:true}).click();await page.locator('.resource-error[role="alert"]').waitFor();assert.equal(await text.count(),0);assert.equal(await directory.locator('button.list-item').count(),0);
 sql('ALTER TABLE notes_p05_fault RENAME TO notes');await page.locator('.resource-error').getByRole('button',{name:'重新加载',exact:true}).click();await directory.getByRole('button',{name:/独立安全笔记/}).waitFor();pass('real SQL query failure clears old directory/body; retry restores real data');
 await directory.getByRole('button',{name:/独立安全笔记/}).click();await text.waitFor();await context.setOffline(true);await page.getByText('笔记连接不可用，已清除页面中的笔记内容。',{exact:true}).waitFor();assert.equal(await text.count(),0);assert.equal(await directory.locator('button.list-item').count(),0);
 await context.setOffline(false);await directory.getByRole('button',{name:/独立安全笔记/}).waitFor();await directory.getByRole('button',{name:/独立安全笔记/}).click();await text.waitFor();pass('real browser offline clears private views; online reconnect recovers');
 await page.getByRole('button',{name:'断开',exact:true}).click();await page.getByLabel('实例访问令牌').waitFor();assert.equal(await text.count(),0);assert.equal(await directory.count(),0);assert.deepEqual(await page.evaluate(()=>({local:Object.keys(localStorage),session:Object.keys(sessionStorage)})),{local:[],session:[]});assert.equal((await page.content()).includes(token),false);assert.equal((await page.content()).includes('PRIVATE-P05-ONLY'),false);
 assert.equal(sql('SELECT count(*) FROM usage_reservations'),'0');assert.equal(sql("SELECT count(*) FROM events WHERE kind='tool.result'"),initialToolResults);assert.deepEqual(errors,[]);pass('session exit clears data; no model/tool reservations or fake reading receipts');report.passed=true;
}catch(error){report.error=error instanceof Error?error.message:'unknown failure';process.exitCode=1;console.error(report.error);if(page)await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});}
finally{
 const cleanup=[];
 try{if(slow){if(slow.exitCode===null&&slow.signalCode===null)slow.kill('SIGTERM');await exited(slow);}}catch{cleanup.push('slow fixture process cleanup failed');}
 try{if(context)await context.close();if(browser)await browser.close();}catch{cleanup.push('browser cleanup failed');}
 report.cleanup=cleanup.length===0;report.cleanup_errors=cleanup;if(cleanup.length){report.passed=false;process.exitCode=1;}report.finished_at=new Date().toISOString();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2),{flag:'wx'});console.log(`P05 browser report: ${path.join(output,'report.json')}`);
}
