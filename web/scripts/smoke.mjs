// Real HTTP and disposable PostgreSQL. No route mocks or live model calls.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=process.env.ALIA_TEST_OUTPUT;
assert(base&&token&&output,'Use scripts/test_web.py');await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});
const context=await browser.newContext({viewport:{width:1440,height:1000}});const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
const headers={Authorization:`Bearer ${token}`};const readState=async()=>{const r=await context.request.get(`${base}/api/v1/state`,{headers});assert.equal(r.status(),200);return r.json();};
const go=async route=>{await page.evaluate(route=>{location.hash=`#/${route}`;},route);await page.waitForTimeout(90);};
try{
 await page.goto(base);if(process.env.ALIA_TEST_PHASE==='create')await page.screenshot({path:path.join(output,'login-desktop.png'),fullPage:true});
 if(process.env.ALIA_TEST_PHASE==='create'){
  await page.getByLabel('实例访问令牌').fill('invalid-token-for-browser-test');
  await page.getByRole('button',{name:'连接 Alia',exact:true}).click();
  await page.getByRole('alert').getByText('令牌无效或已失效，请重新连接。',{exact:true}).waitFor();
  assert.equal(await page.getByText('Alia 测试个体',{exact:true}).count(),0);
 }
 await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();
 if(process.env.ALIA_TEST_PHASE==='create'){
  await page.getByLabel('初始名字').fill('Alia 测试个体');await page.getByRole('button',{name:'建立持续身份'}).click();
  await page.getByText('Alia 测试个体',{exact:true}).first().waitFor();
  await writeFile(path.join(output,'identity.json'),JSON.stringify({individual:(await readState()).individual_id}));
  await go('media/chat');await page.getByLabel('发送给 Alia 的消息').fill('这是隔离浏览器测试的真实输入，不是生产共同经历。');await page.getByRole('button',{name:'发送 ↗',exact:true}).click();
  await page.getByText('未完成 · 模型尚未配置可用凭证',{exact:true}).waitFor({timeout:20000});
  // A second client writes: this page must refresh via persistent SSE, not local mutation.
  await page.getByText('实时同步中',{exact:true}).waitFor({timeout:10000}).catch(()=>{});
  const external=await context.request.post(`${base}/api/v1/messages`,{headers:{...headers,'Idempotency-Key':'ui-sse-external'},data:{text:'另一个窗口提交的真实事件。'}});assert.equal(external.status(),202);
  await page.getByText('另一个窗口提交的真实事件。',{exact:true}).waitFor({timeout:4500});
  await go('agency');await page.getByRole('button',{name:'暂停自主活动',exact:true}).click();await page.getByRole('dialog').waitFor();assert.equal((await readState()).autonomy_paused,false,'confirmation must not mutate');
  await page.keyboard.press('Escape');await page.getByRole('dialog').waitFor({state:'hidden'});assert.equal((await readState()).autonomy_paused,false);
  await page.getByRole('button',{name:'暂停自主活动',exact:true}).click();await page.getByRole('button',{name:'确认提交',exact:true}).click();await page.getByRole('button',{name:'恢复自主活动',exact:true}).waitFor();assert.equal((await readState()).autonomy_paused,true);
  await page.getByRole('button',{name:'创建目标',exact:true}).click();
  for(const [label,value] of [['目标描述','浏览器验收目标'],['完成条件','保留带来源的研究笔记'],['下一步','阅读已经导入的资料'],['停止条件','资料缺失或预算不足时等待']])await page.getByLabel(label,{exact:true}).fill(value);
  await page.getByRole('button',{name:'创建并允许推进',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await page.getByRole('heading',{name:'浏览器验收目标',exact:true}).waitFor();
  const goalRow=page.locator('.goal-row').filter({has:page.getByRole('heading',{name:'浏览器验收目标',exact:true})});
  await goalRow.getByRole('button',{name:'等待目标',exact:true}).click();await page.getByLabel('变更理由',{exact:true}).fill('等待补充资料');await page.getByRole('button',{name:'确认目标变更',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await goalRow.getByText('等待中',{exact:true}).waitFor();
  await goalRow.getByRole('button',{name:'激活目标',exact:true}).click();await page.getByLabel('变更理由',{exact:true}).fill('现在继续研究');await page.getByRole('button',{name:'确认目标变更',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await goalRow.getByText('进行中',{exact:true}).waitFor();
  await goalRow.getByRole('button',{name:'放弃目标',exact:true}).click();await page.getByLabel('变更理由',{exact:true}).fill('测试完成，保留历史');await page.getByRole('button',{name:'确认目标变更',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await goalRow.getByText('已放弃',{exact:true}).waitFor();assert.equal(await goalRow.getByRole('button',{name:'激活目标',exact:true}).count(),0);
  await go('settings');await page.getByRole('button',{name:'导入文本资料',exact:true}).click();await page.getByLabel('资料标题',{exact:true}).fill('浏览器导入测试');await page.getByLabel('正文',{exact:true}).fill('外部资料内容应当明确标注来源，不属于亲身经历。');await page.getByRole('button',{name:'保存资料',exact:true}).click();await page.getByRole('button',{name:'导入文本资料',exact:true}).waitFor();
  await go('config');await page.getByRole('button',{name:'完整 JSON',exact:true}).click();await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();const rules=JSON.parse(await page.getByLabel('完整运行配置').inputValue());rules.budgets.background_tokens_per_day=12000;await page.getByLabel('完整运行配置').fill(JSON.stringify(rules,null,2));await page.getByRole('button',{name:'保存配置',exact:true}).click();
  await page.getByRole('dialog',{name:'确认运行规则变更'}).waitFor();assert.notEqual((await readState()).runtime.budgets.background_tokens_per_day,12000);
  await page.getByRole('button',{name:'确认提交',exact:true}).click();await page.getByRole('dialog',{name:'确认运行规则变更'}).waitFor({state:'hidden'});await page.getByRole('button',{name:'编辑运行配置',exact:true}).waitFor();assert.equal((await readState()).runtime.budgets.background_tokens_per_day,12000);assert.equal(JSON.parse(await page.getByLabel('完整运行配置').inputValue()).budgets.background_tokens_per_day,12000);
  await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();
  const conflictRules=JSON.parse(await page.getByLabel('完整运行配置').inputValue());conflictRules.budgets.background_tokens_per_day=13000;await page.getByLabel('完整运行配置').fill(JSON.stringify(conflictRules));
  const conflictEvent=await context.request.post(`${base}/api/v1/messages`,{headers:{...headers,'Idempotency-Key':'ui-version-conflict'},data:{text:'另一个请求改变当前版本，用于真实冲突测试。'}});assert.equal(conflictEvent.status(),202);
  await page.getByRole('button',{name:'保存配置',exact:true}).click();await page.getByRole('button',{name:'确认提交',exact:true}).click();await page.getByRole('dialog').getByRole('alert').getByText('状态已经变化，请重新读取',{exact:true}).waitFor();assert.equal((await readState()).runtime.budgets.background_tokens_per_day,12000);
  assert.equal(await page.getByRole('dialog').count(),1,'failed confirmation must stay open');
  assert.equal(JSON.parse(await page.getByLabel('完整运行配置').inputValue()).budgets.background_tokens_per_day,13000,'failed edit was lost');
  await page.getByRole('dialog').getByRole('button',{name:'取消',exact:true}).click();
  await page.getByRole('button',{name:'取消编辑',exact:true}).click();await page.getByRole('button',{name:'关闭提示',exact:true}).click();
  await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();
  const strictBefore=await readState();const strictRules=await page.getByLabel('完整运行配置').inputValue();
  await page.getByLabel('完整运行配置').fill(strictRules.replace('{','{"schema_version":1,'));
  await page.getByRole('button',{name:'保存配置',exact:true}).click();await page.getByRole('button',{name:'确认提交',exact:true}).click();
  await page.getByRole('dialog').getByText('JSON 无效或包含重复字段',{exact:true}).waitFor();assert.deepEqual(await readState(),strictBefore);
  await page.getByRole('dialog').getByRole('button',{name:'取消',exact:true}).click();await page.getByRole('button',{name:'取消编辑',exact:true}).click();await page.getByRole('button',{name:'关闭提示',exact:true}).click();
  // The deliberate conflicting foreground message also has a real queued run.
  // Wait for its terminal result and the UI's SSE snapshot before starting a positive save.
  const conflictId=(await conflictEvent.json()).operation_id;
  let terminal=false;for(let i=0;i<100;i++){const result=await context.request.get(`${base}/api/v1/operations/${conflictId}`,{headers});assert.equal(result.status(),200);if((await result.json()).status==='failed'){terminal=true;break;}await page.waitForTimeout(50);}assert(terminal,'conflicting foreground run did not settle');
  const settled=await readState();await go('identity');await page.getByText(new RegExp('当前状态版本 '+settled.version+'$')).waitFor();await go('config');
  // Structured configuration preserves the full document and uses server revisions.
  await page.getByRole('button',{name:'自主与预算',exact:true}).click();
  await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();
  assert.equal(await page.getByLabel('每循环工具调用上限',{exact:true}).getAttribute('min'),'1');
  assert.equal(await page.getByLabel('循环超时（秒）',{exact:true}).getAttribute('max'),'120');
  await page.getByLabel('后台每日 token 上限',{exact:true}).fill('');assert.equal(await page.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'','numeric draft must allow clearing');
  await page.locator('.sidebar nav a[href="#/models"]').click();await page.getByRole('dialog',{name:'配置尚未保存'}).waitFor();await page.getByRole('dialog').getByRole('button',{name:'取消',exact:true}).click();assert.equal(await page.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'','empty numeric draft must survive navigation cancellation');
  await page.getByLabel('后台每日 token 上限',{exact:true}).fill('12001');
  const fullBeforeGuard=await readState();
  await page.locator('.sidebar nav a[href="#/models"]').click();
  const dirtyDialog=page.getByRole('dialog',{name:'配置尚未保存'});await dirtyDialog.waitFor();
  assert.equal(new URL(page.url()).hash,'#/config');assert.deepEqual(await readState(),fullBeforeGuard);
  await dirtyDialog.getByRole('button',{name:'取消',exact:true}).click();
  assert.equal(await page.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'12001');
  await page.locator('.sidebar nav a[href="#/models"]').click();await dirtyDialog.waitFor();
  await dirtyDialog.getByRole('button',{name:'保存后离开',exact:true}).click();await dirtyDialog.waitFor({state:'hidden'});
  assert.equal(new URL(page.url()).hash,'#/models');
  const savedStructured=await readState();assert.equal(savedStructured.runtime.budgets.background_tokens_per_day,12001);
  assert.deepEqual(savedStructured.runtime.memory,fullBeforeGuard.runtime.memory);assert.deepEqual(savedStructured.runtime.development,fullBeforeGuard.runtime.development);
  await page.locator('.sidebar nav a[href="#/config"]').click();
  await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();await page.getByLabel('后台每日 token 上限',{exact:true}).fill('12000');
  await page.getByRole('button',{name:'保存配置',exact:true}).click();await page.getByRole('button',{name:'确认提交',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();await page.getByLabel('后台每日 token 上限',{exact:true}).fill('12003');
  await page.goBack();await dirtyDialog.waitFor();await page.waitForFunction(()=>location.hash==='#/config');
  await dirtyDialog.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await page.getByLabel('后台每日 token 上限',{exact:true}).inputValue(),'12003');
  await page.goBack();await dirtyDialog.waitFor();await dirtyDialog.getByRole('button',{name:'放弃并离开',exact:true}).click();await dirtyDialog.waitFor({state:'hidden'});await page.waitForFunction(()=>location.hash==='#/models');
  await page.goForward();await page.getByRole('button',{name:'编辑运行配置',exact:true}).waitFor();assert.equal(new URL(page.url()).hash,'#/config');assert.equal((await readState()).runtime.budgets.background_tokens_per_day,12000);
  await page.getByRole('button',{name:'完整 JSON',exact:true}).click();await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();
  const guardRaw=await page.getByLabel('完整运行配置').inputValue();await page.getByLabel('完整运行配置').fill(guardRaw.replace('{','{"schema_version":1,'));
  await page.getByRole('button',{name:'主动联系',exact:true}).click();await page.getByText('配置未通过服务端校验，请检查 JSON、字段及能力范围。',{exact:true}).waitFor();
  assert.equal(await page.getByLabel('完整运行配置').inputValue(),guardRaw.replace('{','{"schema_version":1,'),'invalid raw JSON was normalized');
  await page.getByLabel('完整运行配置').fill('');await go('models');await dirtyDialog.waitFor();await dirtyDialog.getByRole('button',{name:'取消',exact:true}).click();assert.equal(await page.getByLabel('完整运行配置').inputValue(),'','empty draft was lost');
  await page.getByRole('button',{name:'取消编辑',exact:true}).click();
  const routes=['overview','console','identity','memory','platforms','models','extensions','deps','config','tasks','data','governance','settings'];
  assert.equal(await page.locator('.sidebar nav a').count(),13);assert.equal(await page.locator('.sidebar .nav-group h2').count(),4);
  assert.equal(await page.locator('.sidebar').evaluate(el=>Math.round(el.getBoundingClientRect().width)),230);
  await page.getByRole('button',{name:'收起侧栏',exact:true}).click();assert.equal(await page.locator('.sidebar').evaluate(el=>Math.round(el.getBoundingClientRect().width)),64);
  await page.getByRole('button',{name:'展开侧栏',exact:true}).click();assert.equal(await page.locator('.sidebar').evaluate(el=>Math.round(el.getBoundingClientRect().width)),230);
  for(const route of routes){await page.locator(`.sidebar nav a[href="#/${route}"]`).click();await page.waitForTimeout(90);await page.waitForFunction(()=>!Array.from(document.querySelectorAll('[role=status]')).some(el=>el.textContent==='正在读取服务端数据…'));assert.equal(await page.locator('.resource-error[role=alert]').count(),0,`resource failed ${route}`);assert.equal(await page.locator('main h1').count(),1);assert(await page.locator('main h1').evaluate(el=>el===document.activeElement),'route must focus heading');assert.equal(await page.locator('img,canvas,video').count(),0,'no character/image assets');await page.screenshot({path:path.join(output,`${route}-desktop.png`),fullPage:true});}
  const aliasCases={cognition:'identity/cognition',relations:'identity/relations',agency:'tasks',automations:'tasks/schedule',workbench:'console',media:'platforms/body','media/chat':'platforms/web/chat',channels:'platforms',traces:'data/traces',events:'data/sessions',diagnostics:'data/statistics'};
  for(const [oldRoute,canonical] of Object.entries(aliasCases)){await go(oldRoute);assert.equal(new URL(page.url()).hash,'#/'+canonical,`legacy route ${oldRoute}`);assert.equal(await page.getByText('页面不存在',{exact:true}).count(),0);}
  await go('workbench');const sandboxBefore=await readState();
  await page.getByRole('button',{name:'运行隔离校验',exact:true}).click();await page.getByText('校验通过 · 未执行',{exact:true}).waitFor();
  const validPersona=await page.getByLabel('隔离校验 JSON',{exact:true}).inputValue();await page.getByLabel('隔离校验 JSON',{exact:true}).fill(validPersona.replace('{','{"schema_version":1,'));
  await page.getByRole('button',{name:'运行隔离校验',exact:true}).click();await page.getByText('校验未通过',{exact:true}).waitFor();assert.deepEqual(await readState(),sandboxBefore,'sandbox modified individual state');
  await page.getByLabel('校验类型',{exact:true}).selectOption('decision_schema');await page.getByRole('button',{name:'运行隔离校验',exact:true}).click();await page.getByText('本次仅验证提案形状，来源、权限、预算和状态版本均未验证，禁止据此执行。',{exact:true}).waitFor();
  await go('extensions');await page.getByRole('tab',{name:'已安装',exact:true}).click();await page.getByRole('tab',{name:'已安装',exact:true}).focus();await page.keyboard.press('ArrowRight');assert.equal(await page.getByRole('tab',{name:'插件市场',exact:true}).getAttribute('aria-selected'),'true');await page.getByText('当前没有已连接的插件市场。',{exact:true}).waitFor();assert(await page.getByRole('button',{name:'连接市场（未接入）',exact:true}).isDisabled());await page.keyboard.press('Home');
  await page.getByLabel('搜索已注册能力',{exact:true}).fill('not-an-installed-capability');await page.getByText('没有匹配的已注册能力。',{exact:true}).waitFor();
  await go('models');await page.getByText('凭证未配置',{exact:true}).waitFor();assert(await page.getByRole('button',{name:'测试推理连通性（未接入）',exact:true}).isDisabled());
  await go('traces');const traceLink=page.locator('a[href^="#/traces/"]').first();await traceLink.waitFor();await traceLink.click();await page.getByText('尚无模型或工具步骤，排队和未执行的运行可以为空。',{exact:true}).waitFor();
  await go('memory');await page.getByLabel('搜索已加载记忆').fill('没有匹配');await page.getByText('没有匹配项。清除搜索或更换类别。',{exact:true}).waitFor();
  await go('events');await page.getByLabel('搜索已加载交流事件').fill('不匹配任何内容');await page.getByText('没有匹配的交流事件。',{exact:true}).waitFor();
  // Export only the selected real UI projection. Cancel has no download or write.
  await page.getByLabel('搜索已加载交流事件').fill('另一个窗口提交的真实事件。');
  const exportBefore=await readState();let downloads=0;page.on('download',()=>downloads++);
  await page.getByRole('button',{name:'导出当前视图',exact:true}).click();
  await page.getByRole('dialog').waitFor();await page.keyboard.press('Escape');
  assert.equal(downloads,0);assert.deepEqual(await readState(),exportBefore);
  await page.getByRole('button',{name:'导出当前视图',exact:true}).click();
  const eventDownload=page.waitForEvent('download');await page.getByRole('button',{name:'确认提交',exact:true}).click();
  const eventFile=await eventDownload;const eventExport=JSON.parse(await readFile(await eventFile.path(),'utf8'));
  assert.equal(eventExport.format,'alia-ui-view/1');assert.equal(eventExport.complete_backup,false);
  assert.equal(eventExport.view.events.length,1);assert.equal(eventExport.view.events[0].text,'另一个窗口提交的真实事件。');
  assert.equal(eventExport.view.individual_id,exportBefore.individual_id);assert(!JSON.stringify(eventExport).includes(token));
  await go('settings');await page.getByRole('button',{name:'导出当前视图',exact:true}).click();
  const stateDownload=page.waitForEvent('download');await page.getByRole('button',{name:'确认提交',exact:true}).click();
  const stateFile=await stateDownload;const stateText=await readFile(await stateFile.path(),'utf8');const stateExport=JSON.parse(stateText);
  assert.equal(stateExport.scope,'runtime-view');assert.equal(stateExport.view.individual_id,exportBefore.individual_id);
  assert(!stateText.includes(token));assert(!stateText.includes('另一个窗口提交的真实事件。'));
  assert(!('token' in stateExport.view));assert(!('runtime' in stateExport.view));assert.deepEqual(await readState(),exportBefore);
  await go('events');
  await page.locator('footer time').waitFor();const syncedBeforeOffline=await page.locator('footer time').getAttribute('datetime');await context.setOffline(true);await page.getByRole('banner').getByText('网络已断开',{exact:true}).waitFor();assert.equal(await page.locator('footer time').getAttribute('datetime'),syncedBeforeOffline,'offline must not claim a new sync');await context.setOffline(false);await page.getByRole('banner').getByText('服务已连接',{exact:true}).waitFor();
  const before=await readState();await page.getByRole('button',{name:'切换主题',exact:true}).click();assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');assert.deepEqual(await readState(),before,'theme changed backend state');await go('overview');await page.screenshot({path:path.join(output,'overview-dark.png'),fullPage:true});for(const route of routes){await go(route);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`dark overflow ${route}`);}await page.getByRole('button',{name:'切换主题',exact:true}).click();
  for(const width of [1440,1280,1080,768,390,360]){
    await page.setViewportSize({width,height:900});
    for(const route of routes){await go(route);assert(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth),`overflow ${width} ${route}`);
    const clipped=await page.locator('.resource-detail input:visible,.resource-detail select:visible,.resource-detail textarea:visible,.config-workspace input:visible,.config-workspace select:visible,.config-workspace textarea:visible').evaluateAll(nodes=>nodes.map(n=>({tag:n.tagName,left:n.getBoundingClientRect().left,right:n.getBoundingClientRect().right})).filter(n=>n.left<0||n.right>innerWidth+1));assert.deepEqual(clipped,[],`clipped control ${width} ${route}`);if(width===390&&['console','models','data','config','extensions'].includes(route))await page.screenshot({path:path.join(output,`${route}-390.png`),fullPage:true});}
    await go('overview');await page.screenshot({path:path.join(output,`overview-${width}.png`),fullPage:true});
  }
  await go('extensions');await page.locator('.resource-options button').last().click();await page.locator('.resource-detail').getByRole('heading',{name:'web.read_allowed',exact:true}).waitFor();
  assert.deepEqual(await page.locator('button:visible').evaluateAll(nodes=>nodes.filter(n=>!n.disabled&&((n.getBoundingClientRect().width<40)||(n.getBoundingClientRect().height<40))).map(n=>n.textContent.trim())),[],'mobile buttons must meet package target size');
  await go('config');await page.getByRole('button',{name:'编辑运行配置',exact:true}).click();await page.getByLabel('后台每日 token 上限',{exact:true}).fill('12004');
  await page.getByRole('button',{name:'验证配置',exact:true}).click();await page.getByText('服务端校验通过，尚未保存。',{exact:true}).waitFor();
  await page.getByRole('button',{name:'保存配置',exact:true}).click();const mobileConfirm=page.getByRole('dialog',{name:'确认运行规则变更'});await mobileConfirm.waitFor();await mobileConfirm.getByRole('button',{name:'取消',exact:true}).click();
  await page.getByRole('button',{name:'取消编辑',exact:true}).click();assert.equal((await readState()).runtime.budgets.background_tokens_per_day,12000,'mobile cancel must not change persisted configuration');
  await page.getByRole('button',{name:'打开导航'}).click();const dialog=page.getByRole('dialog');await dialog.waitFor();await page.keyboard.press('Shift+Tab');assert(await page.evaluate(()=>!!document.activeElement?.closest('dialog')),'drawer focus escaped');await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});await page.getByRole('button',{name:'打开导航'}).click();await dialog.getByRole('link',{name:'记忆管理',exact:true}).click();await dialog.waitFor({state:'hidden'});assert.equal(new URL(page.url()).hash,'#/memory');await page.waitForFunction(()=>document.activeElement===document.querySelector('main h1'));
  await page.getByRole('link',{name:'跳到主要内容',exact:true}).focus();await page.keyboard.press('Enter');assert(await page.locator('main h1').evaluate(el=>el===document.activeElement));assert.equal(new URL(page.url()).hash,'#/memory','skip link must not alter route');
  await page.getByRole('button',{name:'打开导航'}).click();await page.getByRole('dialog').getByRole('link',{name:'系统设置',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});await page.getByRole('button',{name:'打开导航'}).click();await page.getByRole('dialog').getByRole('link',{name:'记忆管理',exact:true}).click();await page.getByRole('dialog').waitFor({state:'hidden'});
  await page.reload();await page.getByLabel('实例访问令牌').waitFor();await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();await page.getByRole('heading',{name:'记忆条目',exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>localStorage.length+sessionStorage.length),0);await go('settings');await page.getByRole('button',{name:'断开当前页面',exact:true}).click();await page.getByLabel('实例访问令牌').waitFor();assert.equal(await page.getByText('这是隔离浏览器测试的真实输入，不是生产共同经历。',{exact:true}).count(),0);
 }else{
  await page.getByText('Alia 测试个体',{exact:true}).first().waitFor();const expected=JSON.parse(await readFile(path.join(output,'identity.json'),'utf8'));const actual=await readState();assert.equal(actual.individual_id,expected.individual);assert.equal(actual.runtime.budgets.background_tokens_per_day,12000);assert.equal(actual.autonomy_paused,true);
  await go('agency');await page.getByRole('heading',{name:'浏览器验收目标',exact:true}).waitFor();await page.getByText('已放弃',{exact:true}).waitFor();
  await go('memory');
  const people=page.getByRole('region',{name:'人物引用'});
  await people.getByText('周岚',{exact:true}).waitFor();
  await people.getByText('事实主体',{exact:true}).waitFor();
  await people.getByText('陈述注记',{exact:true}).waitFor();
  assert.equal(await people.getByText('模型推断',{exact:true}).count(),0,'owner fixture must not masquerade as model inference');
  await people.getByText('查看关联依据',{exact:true}).click();
  await people.getByText('owner',{exact:true}).waitFor();
  const inspected=await context.request.get(`${base}/api/v1/memories`,{headers});
  assert.equal(inspected.status(),200);const memory=(await inspected.json()).items[0];
  assert.equal(memory.person_assertions.length,1);
  await people.getByText(memory.person_assertions[0].assertion_event_id,{exact:true}).waitFor();
  for(const width of [1440,1280,1080,768,390,360]){
    await page.setViewportSize({width,height:900});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`memory inspector overflow ${width}`);
    if(width===1440||width===390)await page.screenshot({path:path.join(output,`memory-person-${width}.png`),fullPage:true});
  }
  await page.getByRole('tab',{name:'服务端检索',exact:true}).click();await page.getByLabel('检索问题',{exact:true}).fill('周岚的认识');await page.getByLabel('人物范围（已加载引用）',{exact:true}).selectOption(memory.person_assertions[0].entity_id);
  const queryBefore=await readState();const found=page.waitForResponse(r=>r.url().includes('/api/v1/memories/retrieve?')&&r.status()===200);await page.getByRole('button',{name:'检索记忆',exact:true}).click();const queried=await(await found).json();assert.equal(queried.retrieval.mode,'lexical');assert.equal(queried.retrieval.person_id,memory.person_assertions[0].entity_id);assert.equal(queried.items[0].id,memory.id);await page.getByText(/实际模式：词项检索/).waitFor();await people.getByText('事实主体',{exact:true}).waitFor();assert.deepEqual(await readState(),queryBefore);
  await page.getByLabel('精确姓名或别名（可选）',{exact:true}).fill('周岚');assert.equal(await page.getByLabel('人物范围（已加载引用）',{exact:true}).inputValue(),'');
  const byName=page.waitForResponse(r=>r.url().includes('/api/v1/memories/retrieve?')&&r.status()===200);await page.getByRole('button',{name:'检索记忆',exact:true}).click();const named=await(await byName).json();assert.equal(named.retrieval.person_filter,'exact_alias');assert.equal(named.retrieval.person_id,memory.person_assertions[0].entity_id);assert.equal(named.items[0].id,memory.id);await page.getByText(/人物定位：已保存姓名或别名/).waitFor();
  for(const width of [1440,1280,1080,768,390,360]){await page.setViewportSize({width,height:900});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`name retrieval overflow ${width}`);if(width===1440||width===390){await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:path.join(output,`memory-name-${width}.png`),fullPage:true});}}
  await page.getByLabel('精确姓名或别名（可选）',{exact:true}).fill('周岚安');const unknownName=page.waitForResponse(r=>r.url().includes('/api/v1/memories/retrieve?')&&r.status()===404);await page.getByRole('button',{name:'检索记忆',exact:true}).click();await unknownName;await page.getByRole('alert').last().waitFor();await page.getByText('尚未取得服务端结果',{exact:true}).waitFor();assert.equal(await page.getByRole('button',{name:/选择记忆：/}).count(),0,'unknown name must not leave a successful old result');assert.deepEqual(await readState(),queryBefore);
  await page.setViewportSize({width:1440,height:1000});
  await go('media/chat');await page.getByText('这是隔离浏览器测试的真实输入，不是生产共同经历。',{exact:true}).waitFor();await page.screenshot({path:path.join(output,'restored-chat.png'),fullPage:true});
 }
 await writeFile(path.join(output,process.env.ALIA_TEST_PHASE+'-report.json'),JSON.stringify({phase:process.env.ALIA_TEST_PHASE,checked_at:new Date().toISOString(),api:'real HTTP and disposable PostgreSQL',models:'not called',routes:13,legacy_aliases:11,widths:[1440,1280,1080,768,390,360],checks:process.env.ALIA_TEST_PHASE==='create'?['navigation','78_light_layouts','13_dark_layouts','control_bounds','mobile_touch_targets','server_write','409_draft','duplicate_json','dirty_navigation','history_back_forward','empty_draft','sse','exports','auth','offline','sandbox_no_write']:['identity_restore','configuration_restore','goal_restore','memory_source','chat_restore'],errors},null,2));
 assert.deepEqual(errors,[]);console.log(`Browser ${process.env.ALIA_TEST_PHASE}: PASS (real HTTP/storage/SSE, 13 v4 routes and legacy aliases, no live model requests)`);
}catch(error){await page.screenshot({path:path.join(output,process.env.ALIA_TEST_PHASE+'-failure.png'),fullPage:true}).catch(()=>{});throw error;}finally{await browser.close();}
