// Independent review of the supplied offline design package. No product writes.
import {chromium} from 'playwright';import {readFile,writeFile,mkdir,readdir,access} from 'node:fs/promises';import path from 'node:path';import {fileURLToPath} from 'node:url';import {createHash} from 'node:crypto';
const args=process.argv.slice(2);if(args.length&&!(args.length===2&&args[0]==='--output'))throw new Error('Usage: review-uiux-v4.mjs [--output <new results directory>]');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..'),source=path.join(root,'docs/design/ALIA_UIUX'),out=path.resolve(root,args[1]??'evals/results/uiux-v4-review');
let exists=false;try{await access(out);exists=true;}catch(error){if(error.code!=='ENOENT')throw error;}if(exists)throw new Error('Refusing to overwrite previous review evidence; select a new --output directory.');await mkdir(out,{recursive:true});
const html=await readFile(path.join(source,'prototype/index.html'),'utf8');
async function sourceManifest(dir=source){const items={};for(const e of await readdir(dir,{withFileTypes:true})){const file=path.join(dir,e.name);if(e.isDirectory())Object.assign(items,await sourceManifest(file));else if(e.isFile())items[path.relative(source,file)]=createHash('sha256').update(await readFile(file)).digest('hex');}return Object.fromEntries(Object.entries(items).sort(([a],[b])=>a<b?-1:a>b?1:0));}
function luminance(hex){const c=hex.replace('#','').match(/../g).map(v=>parseInt(v,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return c[0]*.2126+c[1]*.7152+c[2]*.0722;}
function contrast(a,b){const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05);}
const manifestBefore=await sourceManifest(),tokens=JSON.parse(await readFile(path.join(source,'design/tokens.json'),'utf8'));
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});
const report={source:'docs/design/ALIA_UIUX',prototype_sha256:createHash('sha256').update(html).digest('hex'),mode:'offline prototype only; no production app, network or model calls',checked_at:new Date().toISOString(),layout:[],findings:{},errors:[]};
report.source_files=Object.keys(manifestBefore).length;
report.source_manifest_sha256=createHash('sha256').update(JSON.stringify(manifestBefore)).digest('hex');
report.source_manifest_format='relative paths in code-point order, compact JSON, SHA256 of each file';
report.token_contrast={secondary_on_surface:contrast(tokens.colors.textSecondary,tokens.colors.surface),secondary_on_background:contrast(tokens.colors.textSecondary,tokens.colors.background),primary_on_surface:contrast(tokens.colors.primary,tokens.colors.surface),scope:'specified color pairs only; not a complete accessibility audit'};
try{
 const p=await browser.newPage({viewport:{width:1440,height:900}});p.on('pageerror',e=>report.errors.push(e.message));await p.route('**/*',r=>r.abort());await p.setContent(html);
 const pages=await p.locator('a[data-page]').evaluateAll(nodes=>nodes.map(a=>a.dataset.page));report.routes=pages;
 for(const width of [360,390,768,1080,1280,1440]){
  await p.setViewportSize({width,height:900});
  for(const page of pages){await p.evaluate(page=>act('nav:'+page),page);await p.waitForTimeout(25);report.layout.push({width,page,heading:await p.locator('.pagehead h2').innerText(),no_page_overflow:await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)});}
 }
 await p.setViewportSize({width:1440,height:900});await p.evaluate(()=>act('nav:extensions'));
 const timeout=p.locator('[data-dirty]').first();await timeout.selectOption({label:'60 秒'});
 report.findings.dirty_before_navigation=await p.evaluate(()=>state.dirty);
 await p.locator('a[data-page=models]').click();report.findings.unsaved_navigation_dialogs=await p.locator('[role=dialog]').count();
 await p.locator('a[data-page=extensions]').click();report.findings.timeout_after_navigation=await p.locator('[data-dirty]').first().inputValue();
 await p.locator('[data-dirty]').first().selectOption({label:'60 秒'});await p.locator('[data-action=saveDirty]').first().click();report.findings.timeout_after_demo_save=await p.locator('[data-dirty]').first().inputValue();
 await p.locator('[data-action="extTab:插件市场"]').click();await p.locator('[data-action="modal:install:web-search"]').click();report.findings.modal_focus_inside=await p.evaluate(()=>!!document.activeElement.closest('[role=dialog]'));
 await p.keyboard.press('Escape');report.findings.modal_remaining_after_escape=await p.locator('[role=dialog]').count();
 await p.locator('[data-action=confirmModal]').focus();await p.keyboard.press('Tab');report.findings.modal_focus_after_tab_from_last=await p.evaluate(()=>!!document.activeElement.closest('[role=dialog]'));
 await p.locator('[data-action=closeModal]').first().click();
 await p.setViewportSize({width:390,height:844});await p.evaluate(()=>{act('nav:extensions');act('extTab:已安装');});await p.waitForTimeout(200);
 report.findings.mobile_buttons=await p.locator('button:visible').evaluateAll(nodes=>nodes.map(b=>({label:b.textContent.trim(),width:b.getBoundingClientRect().width,height:b.getBoundingClientRect().height})).filter(b=>b.width<40||b.height<40));
 report.findings.mobile_notices=await p.locator('.notice:visible').evaluateAll(nodes=>nodes.map(n=>({text:n.textContent.trim(),client_width:n.clientWidth,scroll_width:n.scrollWidth,white_space:getComputedStyle(n).whiteSpace,right:n.getBoundingClientRect().right})));
 report.findings.mobile_fields_outside_viewport=await p.locator('.right-form input,.right-form select,.right-form button').evaluateAll(nodes=>nodes.map(n=>({tag:n.tagName,width:n.getBoundingClientRect().width,right:n.getBoundingClientRect().right})).filter(n=>n.right>390));
 await p.screenshot({path:path.join(out,'extensions-390-recheck.png'),fullPage:true});
 await p.locator('#menuButton').click();await p.locator('a[data-page=config]').click();await p.waitForTimeout(250);report.findings.mobile_drawer_after_route=await p.locator('#sidebar').evaluate(n=>({shown:n.classList.contains('mobile-show'),right:n.getBoundingClientRect().right}));
 await p.setViewportSize({width:1440,height:900});await p.evaluate(()=>{act('nav:extensions');act('extTab:已安装');});await p.locator('#pluginCategory').selectOption('连接器');
 report.findings.installed_category={visible_items:await p.locator('.split-menu .split-item').count(),displayed_count:await p.locator('.toolbar .ml-auto').innerText(),detail:await p.locator('.right-form').locator('..').locator('.panel-head h3').innerText()};
 await p.evaluate(()=>act('extTab:插件市场'));await p.locator('#pluginSearch').fill('Maintainers');report.findings.market_author_search={query:'Maintainers',visible_cards:await p.locator('.plugin-card').count(),expected_demo_author_present:html.includes("author:'Maintainers'")};
 await p.evaluate(()=>act('nav:tasks'));const allTasks=await p.locator('tbody').innerText();await p.locator('[data-action="tasksTab:暂停"]').click();report.findings.tasks_pause_tab_changes_rows=(await p.locator('tbody').innerText())!==allTasks;
 await p.evaluate(()=>{act('nav:data');act('dataTab:日志');});const allLogs=await p.locator('.log').innerText();await p.locator('#logsFilter').selectOption('INFO');report.findings.info_log_filter_changes_rows=(await p.locator('.log').innerText())!==allLogs;
 await p.evaluate(()=>act('dataTab:追踪'));await p.locator('#traceSearch').fill('no-matching-trace');report.findings.empty_trace_search={visible_items:await p.locator('.split-menu .split-item').count(),detail_still_visible:await p.locator('.right-form h3').innerText()};
 // Cross-workspace drafts and previews must refer to the same editable document.
 await p.evaluate(()=>{state.configDirty=false;state.configTab='AI';act('nav:config');});
 const rounds=p.locator('.formrow').filter({hasText:'最大工具调用轮次'}).locator('input');
 const initialRounds=await rounds.inputValue();await rounds.fill('2');
 await p.locator('[data-action="modal:json"]').click();
 report.findings.config_json_draft={initial_value:initialRounds,edited_value:'2',preview:await p.locator('[role=dialog] pre').innerText()};
 await p.locator('[data-action=closeModal]').first().click();
 await p.locator('[data-action="configTab:访问控制"]').click();await p.locator('[data-action="configTab:AI"]').click();
 report.findings.config_section_draft={value_after_return:await rounds.inputValue(),dirty_after_return:await p.evaluate(()=>state.configDirty),dialogs:await p.locator('[role=dialog]').count()};
 await p.evaluate(()=>{act('nav:extensions');act('extTab:已安装');});
 await p.locator('[data-dirty]').first().selectOption({label:'60 秒'});
 await p.locator('[data-action="pickPlugin:web-reader"]').click();
 const resourceDialogs=await p.locator('[role=dialog]').count();
 await p.locator('[data-action="pickPlugin:memory-core"]').click();
 report.findings.resource_switch_draft={dialogs:resourceDialogs,timeout_after_return:await p.locator('[data-dirty]').first().inputValue(),dirty_after_return:await p.evaluate(()=>state.dirty)};
 report.findings.resource_keyboard_semantics=await p.locator('[data-action^="pickPlugin:"]').evaluateAll(nodes=>nodes.map(n=>({text:n.textContent.trim(),tag:n.tagName,role:n.getAttribute('role'),tab_index:n.tabIndex})));
 report.findings.timeout_label_matches=await p.getByLabel('执行超时',{exact:true}).count();
 await p.evaluate(()=>act('nav:memory'));const memoryBefore=await p.locator('#memoryList .split-item:visible').count();
 await p.locator('.toolbar select').selectOption({label:'偏好'});
 report.findings.memory_category={before:memoryBefore,after:await p.locator('#memoryList .split-item:visible').count(),selected:'偏好'};
 await p.locator('#memorySearch').fill('聊天记录');
 report.findings.memory_source_search={query:'聊天记录',visible_items:await p.locator('#memoryList .split-item:visible').count(),source_in_selected_detail:await p.locator('.right-form').innerText()};
 report.findings.native_dialog_count=(html.match(/<dialog\b/gi)||[]).length;report.findings.fetch_calls=(html.match(/\bfetch\s*\(/g)||[]).length;report.findings.dark_theme_rules=(html.match(/prefers-color-scheme|data-theme|dark-mode/g)||[]).length;
 report.layout_passed=report.layout.filter(r=>r.no_page_overflow).length;
}finally{await browser.close();report.source_unchanged=JSON.stringify(manifestBefore)===JSON.stringify(await sourceManifest());await writeFile(path.join(out,'source-manifest.json'),JSON.stringify(manifestBefore,null,2)+'\n');await writeFile(path.join(out,'review.json'),JSON.stringify(report,null,2)+'\n')}
console.log(JSON.stringify({routes:report.routes.length,layouts:report.layout.length,layout_passed:report.layout_passed,errors:report.errors.length,findings:report.findings},null,2));
