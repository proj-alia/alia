// Component semantics via React server rendering; interaction is tested in Chromium.
import assert from 'node:assert/strict';import {readFile,mkdir,mkdtemp,writeFile,rm} from 'node:fs/promises';import path from 'node:path';import {pathToFileURL,fileURLToPath} from 'node:url';import ts from 'typescript';import React from 'react';import {renderToStaticMarkup} from 'react-dom/server';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');await mkdir(path.join(root,'node_modules/.cache'),{recursive:true});const temp=await mkdtemp(path.join(root,'node_modules/.cache/alia-components-'));
try{
 async function load(relative,name){const source=await readFile(path.join(root,relative),'utf8');const output=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;const target=path.join(temp,name+'.mjs');await writeFile(target,output);return import(pathToFileURL(target).href);}
 const {Panel,Badge,Empty,ResourceState,Tabs,Confirm,ResourceWorkspace}=await load('src/design-system/components.tsx','components');
 const render=(type,props,...children)=>renderToStaticMarkup(React.createElement(type,props,...children));
 const error=render(ResourceState,{loading:false,error:'<script>private test</script>',retry(){}});assert(error.includes('role="alert"'));assert(error.includes('重新加载'));assert(!error.includes('<script>'));
 const loading=render(ResourceState,{loading:true,error:'',retry(){}});assert(loading.includes('role="status"'));assert(loading.includes('aria-live="polite"'));
 const tabs=render(Tabs,{label:'类型',items:[{id:'a',label:'甲'},{id:'b',label:'乙'}],value:'a',onChange(){}});assert(tabs.includes('role="tablist"'));assert(tabs.includes('aria-selected="true" tabindex="0"'));assert(tabs.includes('aria-selected="false" tabindex="-1"'));
 assert(render(Panel,{title:'检查'},renderToStaticMarkup(React.createElement(Badge,null,'状态'))).includes('<h2>检查</h2>'));
 assert(render(Empty,null,'没有记录').includes('没有记录'));
 const confirm=render(Confirm,{title:'确认变更',busy:true,action:async()=>false,onClose(){},error:'服务端拒绝'},'具体范围');assert(confirm.includes('disabled=""'));assert(confirm.includes('等待服务端'));assert(confirm.includes('role="alert"'));
 const {pages,navigationGroups,resolveRoute}=await load('src/app/routes.ts','routes');assert.equal(pages.length,13);assert.equal(new Set(pages.map(p=>p.id)).size,13);assert.equal(navigationGroups.length,4);
 assert.equal(resolveRoute('#/traces/abc'),'data/traces/abc');assert.equal(resolveRoute('cognition'),'identity/cognition');assert.equal(resolveRoute('media/chat'),'platforms/web/chat');assert.equal(resolveRoute('settings'),'settings');
 const workspace=render(ResourceWorkspace,{title:'资源',items:[{id:'one',label:'实际资源',description:'已注册'}],selected:'one',onSelect(){}},'真实详情');assert(workspace.includes('aria-pressed="true"'));assert(workspace.includes('aria-label="资源"'));assert(workspace.includes('真实详情'));
 const {jsonDiff}=await load('src/shared/jsonDiff.ts','diff');assert.deepEqual(jsonDiff({a:1,b:2},{b:2,a:1}),[]);assert.equal(jsonDiff({}, {a:null})[0].after,null);assert.equal(jsonDiff({a:[1,2]},{a:[2,1]})[0].path,'$.a');assert.equal(jsonDiff({a:{b:1}},{a:{b:2}})[0].path,'$.a.b');
 console.log('Components PASS: feedback semantics, escaped content, tab focus contract, pending confirmation, 13 unique routes, legacy route aliases, resource selection semantics, configuration differences');
}finally{await rm(temp,{recursive:true,force:true})}
