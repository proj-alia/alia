// Real feature-enabled isolated service; no HTTP mocks, fake indexing, or model calls.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';
import path from 'node:path';
const base=process.env.ALIA_TEST_URL,token=process.env.ALIA_TEST_TOKEN,output=process.env.ALIA_TEST_OUTPUT;
assert(base&&token&&output);await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true,...(process.env.CHROMIUM_EXECUTABLE?{executablePath:process.env.CHROMIUM_EXECUTABLE}:{})});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/#/memory');await page.getByLabel('实例访问令牌').fill(token);await page.getByRole('button',{name:'连接 Alia',exact:true}).click();
 await page.getByRole('heading',{name:'索引与检索',exact:true}).waitFor();
 await page.getByText('语义与词项混合检索',{exact:true}).waitFor();
 await page.getByText(/本地索引维护：已启用/).waitFor();
 await page.getByText('编码版本',{exact:true}).click();
 const state=await page.request.get(base+'/api/v1/state',{headers:{Authorization:`Bearer ${token}`}});assert.equal(state.status(),200);
 const json=await state.json();assert.equal(json.indexing.enabled,true);await page.getByText(json.indexing.profile,{exact:true}).waitFor();
 const memories=await page.request.get(base+'/api/v1/memories',{headers:{Authorization:`Bearer ${token}`}});assert.equal(memories.status(),200);
 const long=(await memories.json()).items.find(m=>m.content.includes('磁盘同步'));assert(long);
 const row=page.getByRole('button',{name:/^选择记忆：可靠保存文件/});await row.click();
 const inspector=page.locator('section.panel').filter({has:page.getByRole('heading',{name:'记忆检查器',exact:true})});
 await inspector.getByText(long.content,{exact:true}).waitFor();
 for(const width of [1440,1280,1080,768,390,360]){
  await page.setViewportSize({width,height:900});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`index profile overflow ${width}`);
  assert(await row.locator('p').evaluate(el=>el.scrollHeight>el.clientHeight),'long memory list must be a preview, with full text in inspector');
  if(width===1440||width===390){await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:path.join(output,`index-enabled-${width}.png`),fullPage:true});}
 }
 assert.deepEqual(errors,[]);assert.equal(await page.locator('img,canvas,video').count(),0);
 assert.equal(await page.evaluate(()=>localStorage.length+sessionStorage.length),0);
 await page.getByRole('tab',{name:'服务端检索',exact:true}).click();await page.getByLabel('检索问题',{exact:true}).fill('怎样将文件可靠地保存在磁盘上？');
 const response=page.waitForResponse(r=>r.url().includes('/api/v1/memories/retrieve?')&&r.status()===200);await page.getByRole('button',{name:'检索记忆',exact:true}).click();const retrieved=await(await response).json();assert.equal(retrieved.retrieval.mode,'hybrid');assert.equal(retrieved.retrieval.indexed,2);assert.equal(retrieved.items[0].id,long.id);
 await page.getByText(/实际模式：语义与词项混合检索/).waitFor();await inspector.getByText(long.content,{exact:true}).waitFor();
 for(const width of [1440,390,360]){await page.setViewportSize({width,height:900});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`shared retrieval overflow ${width}`);await page.evaluate(()=>window.scrollTo(0,0));await page.screenshot({path:path.join(output,`shared-retrieval-${width}.png`),fullPage:true});}
 const after=await page.request.get(base+'/api/v1/state',{headers:{Authorization:`Bearer ${token}`}});assert.deepEqual(await after.json(),json,'read-only search changed the individual');assert.deepEqual(errors,[]);
 console.log('Index UI PASS: actual hybrid service and search, six widths, full source inspector, no character assets');
}finally{await browser.close()}
