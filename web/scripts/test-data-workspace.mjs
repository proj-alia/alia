// Run the ENTIRE driver under: flock -x /tmp/alia-project-heavy.lock env ALIA_P02_LOCK_HELD=1 node web/scripts/test-data-workspace.mjs
// External frozen mode requires ALIA_TEST_URL/TOKEN/OUTPUT, ALIA_TEST_BINARY, ALIA_P02_DATABASE and ALIA_P02_PSQL_COMMAND (JSON argv without secrets). No .env reads/build/service lifecycle in that mode.
// Real HTTP/PostgreSQL + synthetic fixture. Never loads model credentials into the service.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {mkdir,readFile,writeFile,mkdtemp,copyFile,chmod,rm,open} from 'node:fs/promises';
import {createHash,randomBytes,randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import os from 'node:os';
import net from 'node:net';
import {once} from 'node:events';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
assert.equal(process.env.ALIA_P02_LOCK_HELD,'1','Acquire the shared flock around the entire driver first.');
const external=!!process.env.ALIA_TEST_URL;
if(external){for(const key of ['ALIA_TEST_TOKEN','ALIA_TEST_OUTPUT','ALIA_TEST_BINARY','ALIA_P02_DATABASE','ALIA_P02_PSQL_COMMAND'])assert(process.env[key],`Frozen test mode requires ${key}`);const u=new URL(process.env.ALIA_TEST_URL);assert(['127.0.0.1','localhost','[::1]'].includes(u.hostname),'Test service must use loopback');}
const output=path.resolve(process.env.ALIA_TEST_OUTPUT??path.join(root,'evals/results/p02-data-workspace-v1'));
await mkdir(path.dirname(output),{recursive:true});await mkdir(output); // evidence is never overwritten
const report={task:'ALIA-P02',fixture:'synthetic; real HTTP/PostgreSQL; zero model calls',started_at:new Date().toISOString(),checks:[],passed:false,cleanup:false};
const record=(name,details={})=>{report.checks.push({name,...details});console.log(`PASS ${name}`);};
const settings=external?{}:Object.fromEntries((await readFile(path.join(root,'.env'),'utf8')).split('\n').filter(l=>l.trim()&&!l.startsWith('#')).map(l=>{const i=l.indexOf('=');return [l.slice(0,i),l.slice(i+1)];}));
const database=external?process.env.ALIA_P02_DATABASE:'alia_p02_'+randomUUID().replaceAll('-',''),token=external?process.env.ALIA_TEST_TOKEN:randomBytes(40).toString('base64url');
assert.match(database,/^alia_(p02|p03|web_test)_[a-z0-9_]+$/);
const psqlArgs=['-n','docker','compose','exec','-T','db','psql','-U','alia','-d'];
const externalSql=external?JSON.parse(process.env.ALIA_P02_PSQL_COMMAND):null;
if(external)assert(Array.isArray(externalSql)&&externalSql.length>1&&externalSql.every(v=>typeof v==='string'),'SQL command must be explicit argv');
function sqlCommand(db=database){if(external){assert.equal(db,database);return externalSql;}return ['sudo',...psqlArgs,db];}
function sql(text,db=database){const [command,...args]=sqlCommand(db);const r=spawnSync(command,[...args,'-v','ON_ERROR_STOP=1','-c',text],{cwd:root,encoding:'utf8'});if(r.status!==0)throw new Error('Isolated fixture SQL failed; inspect retained non-secret test command.');return r.stdout;}
const temporary=external?null:await mkdtemp(path.join(os.tmpdir(),'alia-p02-browser-'));let service,browser,context,page,created=false,log;
const environment={...process.env};for(const key of Object.keys(environment))if(/KEY|TOKEN|DATABASE_URL|ALIA_MODEL|ALIA_EMBEDDING/.test(key))delete environment[key];
let base=process.env.ALIA_TEST_URL;
if(!external){
 environment.ALIA_INSTANCE_TOKEN=token;environment.DATABASE_URL=`postgres://alia:${settings.ALIA_DB_PASSWORD}@127.0.0.1:${settings.ALIA_DB_PORT??'55433'}/${database}`;environment.ALIA_WEB_DIR=path.join(root,'web/dist');environment.ALIA_DATA_DIR=temporary;
 const reserved=net.createServer();reserved.listen(0,'127.0.0.1');await once(reserved,'listening');const port=reserved.address().port;await new Promise(resolve=>reserved.close(resolve));environment.ALIA_BIND=`127.0.0.1:${port}`;base=`http://127.0.0.1:${port}`;
}
async function api(route,body){const response=await fetch(base+'/api/v1'+route,{method:body?'POST':'GET',headers:{Authorization:'Bearer '+token,...(body?{'Content-Type':'application/json','Idempotency-Key':randomUUID()}:{})},body:body?JSON.stringify(body):undefined});assert(response.ok,`fixture HTTP ${response.status}`);return response.json();}
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
try{
 if(!external){
 log=await open(path.join(output,'build.log'),'wx');
 const build=spawn('cargo',['build','--locked','--bin','alia'],{cwd:root,stdio:['ignore',log.fd,log.fd]});const [buildExit]=await once(build,'exit');assert.equal(buildExit,0,'service build');await log.close();log=null;
 const binary=path.join(temporary,'alia');await copyFile(path.join(root,'target/debug/alia'),binary);await chmod(binary,0o700);
 report.binary_sha256=createHash('sha256').update(await readFile(binary)).digest('hex');
 report.sources={};for(const file of ['src/service/api.rs','src/adapters/api.rs','web/src/features/Traces.tsx','web/src/features/AuditLog.tsx'])report.sources[file]=createHash('sha256').update(await readFile(path.join(root,file))).digest('hex');
 sql(`CREATE DATABASE ${database}`,'postgres');created=true;
 service=spawn(binary,[],{cwd:root,env:environment,stdio:'ignore'});
 let ready=false;for(let i=0;i<100;i++){if(service.exitCode!==null)throw new Error('Isolated service exited');try{if((await fetch(base+'/healthz')).ok){ready=true;break;}}catch{}await delay(100);}assert(ready,'service ready');
 // Reuse the same external-only browser path that P03 invokes against frozen artifacts.
 const childEnv={...process.env,ALIA_TEST_URL:base,ALIA_TEST_TOKEN:token,ALIA_TEST_BINARY:binary,ALIA_TEST_OUTPUT:path.join(output,'browser'),ALIA_P02_DATABASE:database,ALIA_P02_PSQL_COMMAND:JSON.stringify(sqlCommand())};
 const child=spawn(process.execPath,[fileURLToPath(import.meta.url)],{cwd:root,env:childEnv,stdio:'inherit'});const [code]=await once(child,'exit');
 const childReport=JSON.parse(await readFile(path.join(output,'browser/report.json'),'utf8'));report.checks=childReport.checks;report.external_report='browser/report.json';report.max_concurrent_reads=childReport.max_concurrent_reads;report.request_count=childReport.request_count;
 for(const file of ['traces-390.png','traces-1440.png','audit-390.png','audit-1440.png']){try{await copyFile(path.join(output,'browser',file),path.join(output,file));}catch{}}
 assert.equal(code,0,'external frozen-mode browser driver');assert.equal(childReport.passed,true);report.passed=true;
 }else{
 report.binary_sha256=createHash('sha256').update(await readFile(process.env.ALIA_TEST_BINARY)).digest('hex');
 assert(sql('SELECT current_database()').includes(database),'SQL command must target the declared disposable test DB');
 const initial=await fetch(base+'/api/v1/state',{headers:{Authorization:'Bearer '+token}});assert.equal(initial.status,409,'P02 fixture requires its own fresh isolated test instance');
 const persona=JSON.parse(await readFile(path.join(root,'config/persona.example.json'),'utf8')),runtime=JSON.parse(await readFile(path.join(root,'config/runtime.example.json'),'utf8'));
 persona.identity.display_name='P02 隔离数据测试';await api('/initialize',{persona,runtime});
 sql(`DO $$ DECLARE ind uuid; e uuid; r uuid; n integer; BEGIN SELECT id INTO ind FROM individuals;
 FOR n IN 0..71 LOOP e:=gen_random_uuid();r:=gen_random_uuid();
 INSERT INTO objects(individual_id,id,kind) VALUES(ind,e,'event'),(ind,r,'run');
 INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id)
 VALUES(ind,e,'p02.query_%','reported_claim','Actor'||n,'2026-01-01T00:00:00Z'::timestamptz+n*interval '1 second','{"text":"p02-private-body-marker"}',r);
 INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,status,created_at,error_code)
 VALUES(ind,r,e,CASE WHEN n%2=0 THEN 'background' ELSE 'foreground' END,1,0,1,CASE WHEN n<66 THEN 'failed' ELSE 'committed' END,'2026-01-01T00:00:00Z'::timestamptz+n*interval '1 second',CASE WHEN n<66 THEN 'model_failure' ELSE NULL END);
 INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES(ind,e,r);
 END LOOP;END $$;`);
 record('isolated service and 72 terminal synthetic runs; no worker/model work');
 browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});
 context=await browser.newContext({viewport:{width:1440,height:1000}});page=await context.newPage();const errors=[],requests=[],active=new Map(),maxActive=new Map();
 page.on('pageerror',e=>errors.push(e.message));
 page.on('request',r=>{const u=new URL(r.url());if(u.pathname.startsWith('/api/v1/runs')){requests.push({url:u.pathname+u.search,at:Date.now()});const key=u.pathname;active.set(key,(active.get(key)??0)+1);maxActive.set(key,Math.max(maxActive.get(key)??0,active.get(key)));}});
 const finished=r=>{const key=new URL(r.url()).pathname;if(active.has(key))active.set(key,Math.max(0,active.get(key)-1));};page.on('requestfinished',finished);page.on('requestfailed',finished);
 const go=async route=>{await page.evaluate(route=>{location.hash='#/'+route;},route);};
 const list=page.locator('.panel').filter({has:page.getByRole('heading',{name:'执行记录',exact:true})});
 const detail=page.locator('.panel').filter({has:page.getByRole('heading',{name:'运行详情与步骤',exact:true})});
 const success=()=>list.locator('time').getAttribute('datetime');
 const waitList=()=>page.waitForFunction(()=>!!document.querySelector('.table-scroll tbody tr'));
 await page.goto(base);await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();await page.getByText('P02 隔离数据测试',{exact:true}).first().waitFor();
 await go('traces');await waitList();await page.waitForFunction(()=>location.hash==='#/data/traces');
 const follow=page.getByLabel('自动跟随执行');assert.equal(await follow.isChecked(),false);
 const initialCount=requests.length;await delay(5400);assert.equal(requests.length,initialCount,'follow defaults off');record('legacy traces route and follow defaults off');
 const selected=await list.locator('tbody a').first().textContent();await list.locator('tbody a').first().click();await detail.getByText(selected,{exact:true}).waitFor();record('selected run follows legacy detail link');
 await page.getByRole('searchbox',{name:'搜索公开元数据'}).fill('MODEL_FAILURE');await page.getByLabel('运行状态',{exact:true}).selectOption('failed');await page.getByLabel('运行模式',{exact:true}).selectOption('background');
 await page.getByLabel('开始时间（UTC，包含）').fill('2026-01-01T00:00:10');await page.getByLabel('结束时间（UTC，不包含）').fill('2026-01-01T00:00:30');
 await Promise.all([page.waitForResponse(r=>r.url().includes('/api/v1/runs?')&&r.url().includes('q=MODEL_FAILURE')),page.getByRole('button',{name:'应用筛选',exact:true}).click()]);
 await page.waitForFunction(()=>document.querySelectorAll('.table-scroll tbody tr').length===10);
 assert((await list.locator('tbody').textContent()).includes('后台'));await page.getByText('所选运行不在当前页或筛选结果中；详情仍按原选择显示。',{exact:true}).waitFor();await detail.getByText(selected,{exact:true}).waitFor();
 assert(requests.at(-1).url.includes('from=')&&requests.at(-1).url.includes('to='));record('server combined search, mode, status, half-open UTC range; detail retained outside filter');
 await page.getByLabel('开始时间（UTC，包含）').fill('');await page.getByLabel('结束时间（UTC，不包含）').fill('');
 await Promise.all([page.waitForResponse(r=>r.url().includes('/api/v1/runs?')&&!r.url().includes('from=')),page.getByRole('button',{name:'应用筛选',exact:true}).click()]);await page.waitForFunction(()=>document.querySelectorAll('.table-scroll tbody tr').length===30);
 await follow.check();await page.getByText('跟随中，每 5 秒刷新',{exact:false}).waitFor();
 const beforeFollow=await success();await page.waitForFunction(previous=>{const t=document.querySelector('.panel time');return t&&t.getAttribute('datetime')!==previous;},beforeFollow,{timeout:7000});
 await page.getByRole('button',{name:'下一页运行',exact:true}).click();assert.equal(await follow.isChecked(),false);await page.getByText('已进入历史页，自动跟随已关闭；保留当前筛选和所选详情。',{exact:true}).waitFor();
 await page.waitForFunction(()=>document.querySelectorAll('.table-scroll tbody tr').length===3);assert(requests.at(-1).url.includes('cursor='));await detail.getByText(selected,{exact:true}).waitFor();record('stable filtered paging switches follow off and retains selected detail');
 await follow.check();await page.waitForFunction(()=>document.querySelectorAll('.table-scroll tbody tr').length===30);
 // Controlled browser visibility signal; network reads and refresh timers remain real.
 await page.evaluate(()=>{Object.defineProperty(document,'hidden',{configurable:true,get:()=>true});document.dispatchEvent(new Event('visibilitychange'));});await delay(400);const hiddenCount=requests.length;const hiddenTime=await success();await delay(5600);assert.equal(requests.length,hiddenCount);assert.equal(await success(),hiddenTime);
 await page.getByText('页面隐藏，跟随已暂停',{exact:false}).waitFor();
 await page.evaluate(()=>{delete document.hidden;document.dispatchEvent(new Event('visibilitychange'));});await page.waitForFunction(previous=>document.querySelector('.panel time')?.getAttribute('datetime')!==previous,hiddenTime);
 record('document.hidden pauses timer/SSE refresh; visibility resumes',{visibility_signal:'controlled document.hidden override; real browser timers and HTTP'});
 // Lock the real PostgreSQL runs table across two timer signals and inject an SSE notification.
 const [sqlExecutable,...sqlArguments]=sqlCommand();const lock=spawn(sqlExecutable,[...sqlArguments,'-v','ON_ERROR_STOP=1','-c',"BEGIN; LOCK TABLE runs IN ACCESS EXCLUSIVE MODE; SELECT pg_sleep(8); COMMIT;"],{cwd:root,stdio:'ignore'});
 await delay(350);await page.getByRole('button',{name:'重新加载运行',exact:true}).click();
 sql("INSERT INTO outbox(individual_id,event_type,resource_id,version) SELECT id,'state.changed',id,state_version FROM individuals");
 await once(lock,'exit');await delay(1000);assert.equal(maxActive.get('/api/v1/runs'),1);record('real slow DB read coalesces SSE/timer with maximum one concurrent list request');
 const lastSuccess=await success();await context.setOffline(true);await page.getByRole('button',{name:'重新加载运行',exact:true}).click();await list.getByRole('alert').waitFor();assert.equal(await success(),lastSuccess);await page.getByText(/网络已断开|连接中断，正在重连/).first().waitFor();record('offline failure visible and success time preserved');
 await context.setOffline(false);await page.getByRole('button',{name:'重新加载运行',exact:true}).click();await waitList();await page.waitForFunction(previous=>document.querySelector('.panel time')?.getAttribute('datetime')!==previous,lastSuccess);record('failure retry and reconnection restore real data');
 await follow.uncheck();
 for(const width of [390,1440]){await page.setViewportSize({width,height:1000});await delay(200);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no trace overflow at ${width}`);await page.screenshot({path:path.join(output,`traces-${width}.png`),fullPage:true});}
 await go('data/logs');await page.getByRole('heading',{name:'系统事件元数据',exact:true}).waitFor();
 await page.getByRole('searchbox',{name:'搜索公开元数据'}).fill('Actor71');await Promise.all([page.waitForResponse(r=>r.url().includes('/api/v1/audit?')&&r.url().includes('q=Actor71')),page.getByRole('button',{name:'应用筛选',exact:true}).click()]);await page.locator('.event-row').getByText(/Actor71/).waitFor();assert.equal(await page.locator('.event-row').count(),1);
 await page.getByRole('searchbox',{name:'搜索公开元数据'}).fill('p02-private-body-marker');await page.getByRole('button',{name:'应用筛选',exact:true}).click();await page.getByText('没有符合服务端筛选的系统事件。',{exact:true}).waitFor();
 await page.getByRole('button',{name:'清除筛选',exact:true}).click();await page.locator('.event-row').first().waitFor();await page.getByRole('button',{name:'下一批系统事件',exact:true}).click();await page.getByRole('button',{name:'首批事件',exact:true}).waitFor();
 for(const width of [390,1440]){await page.setViewportSize({width,height:1000});await delay(150);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no audit overflow at ${width}`);await page.screenshot({path:path.join(output,`audit-${width}.png`),fullPage:true});}
 record('audit server search reaches beyond first page; body is unsearchable; paging and 390/1440 layouts');
 await page.getByRole('button',{name:'断开',exact:true}).click();await page.getByLabel('实例访问令牌').waitFor();assert.equal(await page.locator('.event-row,.span-row,.table-scroll').count(),0);assert.deepEqual(await page.evaluate(()=>({local:Object.keys(localStorage),session:Object.keys(sessionStorage)})),{local:[],session:[]});assert.equal((await page.content()).includes(token),false);
 record('session exit clears data and leaves browser storage empty');assert.deepEqual(errors,[]);report.max_concurrent_reads=Object.fromEntries(maxActive);report.request_count=requests.length;
 assert.match(sql('SELECT count(*) FROM usage_reservations'),/^\s*0\s*$/m);record('no model reservations or calls');report.passed=true;report.lifecycle='externally managed frozen service/database';
 }
}catch(error){report.error=error instanceof Error?error.message:'unknown test failure';process.exitCode=1;console.error(report.error);if(page&&!(await page.getByLabel('实例访问令牌').count())){await page.screenshot({path:path.join(output,'failure.png'),fullPage:true}).catch(()=>{});report.visible_text=(await page.locator('main').innerText().catch(()=>'' )).slice(0,8000);}}
finally{
 const cleanupErrors=[];const clean=async(label,action)=>{try{await action();}catch(error){cleanupErrors.push({label,error:error instanceof Error?error.message:'cleanup failed'});}};
 await clean('browser',async()=>{if(context)await context.close();if(browser)await browser.close();});
 await clean('service',async()=>{if(service){if(service.exitCode===null){service.kill('SIGTERM');const ended=await Promise.race([once(service,'exit').then(([code])=>code),delay(5000).then(()=>null)]);if(ended===null){service.kill('SIGKILL');await once(service,'exit');throw new Error('Service graceful shutdown timed out');}assert.equal(ended,0,'service clean shutdown');}else assert.equal(service.exitCode,0,'service exited early');}});
 await clean('database',async()=>{if(created)sql(`DROP DATABASE ${database} WITH (FORCE)`,'postgres');});
 await clean('temporary files',async()=>{if(temporary)await rm(temporary,{recursive:true,force:true});});
 report.cleanup=cleanupErrors.length===0;if(cleanupErrors.length){report.cleanup_errors=cleanupErrors;report.passed=false;process.exitCode=1;}
 if(log)await log.close();report.finished_at=new Date().toISOString();await writeFile(path.join(output,'report.json'),JSON.stringify(report,null,2),{flag:'wx'});console.log(`Report: ${path.join(output,'report.json')}`);
}
