import ts from 'typescript';
import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const source=await readFile(new URL('../src/events.ts',import.meta.url),'utf8');
const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
const {parser,watchEvents}=await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
const bytes=new TextEncoder().encode(': heartbeat\r\nid: 9007199254740993\r\nevent: state.changed\r\ndata: 中文\r\ndata: 第二行\r\n\r\n');
for(let split=1;split<bytes.length;split++){
 const events=[];const feed=parser(n=>events.push(n));const decoder=new TextDecoder();feed(decoder.decode(bytes.slice(0,split),{stream:true}));feed(decoder.decode(bytes.slice(split),{stream:true}));assert.deepEqual(events,[{id:'9007199254740993',type:'state.changed',data:'中文\n第二行'}]);
}
const events=[];const feed=parser(n=>events.push(n));feed(': connected\nid: 5\n\nid: 6\n: cursor\n\n');assert.deepEqual(events.map(n=>n.id),['5','6']);feed('id: invalid\0id\ndata: text\n\n');assert.equal(events.at(-1).id,'');feed('id: 7\ndata: incomplete');assert.equal(events.length,3);assert.throws(()=>feed('a'.repeat(65537)),/超出限制/);
const original=globalThis.fetch;
try{
 const controller=new AbortController();let refreshed=0;let unauthorized=0;let calls=0;
 globalThis.fetch=async(url,init)=>{assert.equal(url,'/api/v1/events');assert.equal(init.headers.Authorization,'Bearer test-only');assert.equal(init.headers['Last-Event-ID'],calls===0?'0':'12');calls++;return new Response('',{status:calls===1?410:401});};
 await watchEvents({token:'test-only',cursor:'0',signal:controller.signal,refresh:async()=>{refreshed++;return '12';},unauthorized:()=>unauthorized++,status:()=>{}});assert.equal(calls,2);assert.equal(refreshed,1);assert.equal(unauthorized,1);
 const stop=new AbortController();refreshed=0;
 globalThis.fetch=async()=>new Response(new ReadableStream({start(c){c.enqueue(new TextEncoder().encode('id: 10\nevent: state.changed\ndata: {}\n\nid: 10\nevent: state.changed\ndata: {}\n\nid: 11\n: cursor\n\n'));}}),{headers:{'Content-Type':'text/event-stream'}});
 await watchEvents({token:'test-only',cursor:'9',signal:stop.signal,refresh:async()=>{refreshed++;stop.abort();return '11';},unauthorized:()=>assert.fail(),status:()=>{}});assert.equal(refreshed,1);
}finally{globalThis.fetch=original;}
console.log('PASS: fragmented UTF-8/CRLF, multiline data, id-only checkpoints, bounds, snapshot reset, auth expiry, deduplicated refresh, abort');
