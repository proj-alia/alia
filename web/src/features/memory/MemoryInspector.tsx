import {useEffect,useMemo,useRef,useState} from 'react';
import {Panel,Badge,Empty,ResourceState,Unavailable} from '../../design-system/components';
import {useIndividualContext} from '../../shared/context';
import {MemoryPeople} from '../MemoryPeople';
import {memoryEditorStamp,useMemoryEditorGuard} from './EditorGuard';
import {MemoryHttpError,memoryRequest,validMemoryEtag} from './client';
import {useMemoryRead} from './useMemoryRead';
import {RevisionPreview} from './RevisionPreview';
import {MemoryHistory} from './MemoryHistory';
import type {MemoryView,RevisionDraft,RevisionReceipt} from './types';
import {usePrivacy} from '../privacy/PrivacyContext';
import {SourceEventInspector} from '../sources/SourceEventInspector';
import './memory.css';
const uuid=(value:unknown):value is string=>typeof value==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
const validText=(text:string,max:number)=>!!text.trim()&&!text.includes('\0')&&Array.from(text).length<=max;
export function MemoryInspector({id,onSelect,outsideResults,onUnavailable}:{id:string|null;onSelect:(id:string)=>void;outsideResults:boolean;onUnavailable:()=>void}){
 const privacy=usePrivacy(),c=useIndividualContext(),guard=useMemoryEditorGuard(),latest=useRef(c);latest.current=c;
 const [sourceEvent,setSourceEvent]=useState<string|null>(null);
 const sourceConnected=['服务已连接','实时同步中'].includes(c.connection);
 useEffect(()=>setSourceEvent(null),[id,c.token,c.state?.version,sourceConnected]);
 const [draft,setDraft]=useState<RevisionDraft|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[receipt,setReceipt]=useState<RevisionReceipt|null>(null),[showHistory,setShowHistory]=useState(false);
 const draftRef=useRef(draft);draftRef.current=draft;const serial=useRef(0),live=useRef(true),post=useRef<AbortController|null>(null),following=useRef<string|null>(null),saving=useRef(false);
 function clearPrivate(){following.current=null;serial.current++;post.current?.abort();saving.current=false;setBusy(false);setDraft(null);setReceipt(null);setError('');setNotice('记录或来源已不可用，草稿与保存意图已清除。');guard.register(null);onUnavailable();}
 const r=useMemoryRead<MemoryView>(id?`/memories/${encodeURIComponent(id)}`:null,clearPrivate);
 const validated=!!r.data&&r.data.individual_id===c.state?.individual_id&&validMemoryEtag(r.etag,r.data.memory.version);
 const dirty=!!draft&&(!!draft.intention||draft.content!==draft.base.content||draft.reason.length>0);
 const actions=useRef({save:async()=>false,discard:()=>{}});
 function discard(){serial.current++;post.current?.abort();saving.current=false;setDraft(null);setBusy(false);setError('');setNotice('草稿已放弃；不撤销可能已提交的请求。');}
 useEffect(()=>{if(following.current===id){following.current=null;return;}serial.current++;post.current?.abort();saving.current=false;setBusy(false);setDraft(null);setReceipt(null);setError('');setNotice('');setShowHistory(false);},[id]);
 useEffect(()=>{live.current=true;return()=>{live.current=false;serial.current++;post.current?.abort();guard.register(null);};},[guard.register]);
 useEffect(()=>{
  if(!draft||!validated||busy||draft.status!=='editing'||draft.base.id!==r.data!.memory.id)return;
  if(draft.base.version!==r.data!.memory.version){setDraft(d=>d?{...d,status:'conflict',intention:null}:null);setError('目标版本已变化，草稿保留；请检查最新对照与修订链，不能直接覆盖。');}
 },[draft,validated,r.data,busy]);
 async function save():Promise<boolean>{
  const current=draftRef.current,view=r.data;if(!current||saving.current)return false;
  if(!validated||!view){setError('先重新读取并验证当前来源，再保存更正。');return false;}
  if(!['active','paused'].includes(latest.current.state?.lifecycle??'')){setError('当前生命周期不允许更正；草稿仍保留。');return false;}
  if(!validText(current.content,2000)||!validText(current.reason,1000)){setError('正文须为 1—2000 字符，理由为 1—1000 字符，均不能纯空白或包含 NUL。');return false;}
  if(current.status==='conflict'){setError('版本冲突尚未处理。请明确采用当前记录作为新基线并重新预览，或放弃草稿。');return false;}
  const intention=current.intention??{key:crypto.randomUUID(),target:current.base.id,etag:current.etag,body:{content:current.content,scope:current.base.scope,reason:current.reason}};
  setDraft(d=>d?{...d,preview:true,intention}:null);saving.current=true;setBusy(true);setError('');setReceipt(null);
  const generation=serial.current,token=latest.current.token,individual=latest.current.state?.individual_id;const controller=new AbortController();post.current=controller;
  try{
   const response=await memoryRequest<RevisionReceipt>(token,`/memories/${encodeURIComponent(intention.target)}/revisions`,controller.signal,{body:intention.body,key:intention.key,etag:intention.etag});
   if(!live.current||generation!==serial.current||token!==latest.current.token)return false;
   const value=response.data;
   if(response.status!==201||value.individual_id!==individual||value.previous_memory_id!==intention.target||!uuid(value.memory_id)||value.memory_id===value.previous_memory_id||!uuid(value.event_id)||!Number.isSafeInteger(value.version)||value.version<1||!validMemoryEtag(response.etag,value.memory_version)||value.previous_memory_version!==current.base.version+1)throw new Error('服务端保存回执未能完整验证');
   setDraft(null);setError('');setReceipt(value);setNotice('已收到 HTTP 201 和新记忆回执；原解释保留为历史。');following.current=value.memory_id;onSelect(value.memory_id);void latest.current.refresh();return true;
  }catch(cause){
   if(!live.current||generation!==serial.current||token!==latest.current.token)return false;
   if(cause instanceof MemoryHttpError&&[401,403].includes(cause.status)&&['unauthorized','cross_origin_denied'].includes(cause.code)){latest.current.clearSession();return false;}
   if(cause instanceof MemoryHttpError&&cause.status===404){clearPrivate();r.reload();return false;}
   if(cause instanceof MemoryHttpError&&cause.status===409){setDraft(d=>d?{...d,status:'conflict',intention:null}:null);setError('409：目标版本、状态或请求标识发生冲突。输入已保留，请检查最新对照；不会静默覆盖。');setShowHistory(true);r.reload();return false;}
   if(cause instanceof MemoryHttpError&&[400,403,422].includes(cause.status)){setDraft(d=>d?{...d,status:'editing',intention:null}:null);setError(cause.message);return false;}
   setDraft(d=>d?{...d,status:'unknown'}:null);setError('保存结果未知。草稿和同一保存意图已锁定；请重放原请求确认结果，不能当作未保存重新创建。');return false;
  }finally{if(live.current&&generation===serial.current){saving.current=false;setBusy(false);}}
 }
 actions.current={save,discard};
 const registration=useMemo(()=>draft?{dirty,busy,error,session:c.token,stamp:memoryEditorStamp(c),validated,summary:validated?<RevisionPreview draft={draft}/>:null,save:()=>actions.current.save(),discard:()=>actions.current.discard()}:null,[draft,dirty,busy,error,c.token,c.state?.version,c.lastSyncedAt,c.connection,validated]);
 useEffect(()=>{guard.register(registration);return()=>guard.register(null);},[registration,guard.register]);
 async function verifyHistorySource(){
  if(!id)return;const generation=serial.current,token=latest.current.token;
  try{const response=await memoryRequest<MemoryView>(token,`/memories/${encodeURIComponent(id)}`,new AbortController().signal);
   if(!live.current||generation!==serial.current||token!==latest.current.token)return;
   if(response.data.memory.version!==r.data?.memory.version)r.reload();
  }catch(cause){if(!live.current||generation!==serial.current||token!==latest.current.token)return;if(cause instanceof MemoryHttpError&&cause.status===404){clearPrivate();void latest.current.refresh();}else if(cause instanceof MemoryHttpError&&[401,403].includes(cause.status)&&['unauthorized','cross_origin_denied'].includes(cause.code))latest.current.clearSession();}
 }
 async function carry(target:string){
  const current=draftRef.current;if(!current||busy||current.status==='unknown')return;const generation=serial.current,token=latest.current.token;setBusy(true);setError('');
  const controller=new AbortController();post.current=controller;
  try{const response=await memoryRequest<MemoryView>(token,`/memories/${encodeURIComponent(target)}`,controller.signal);if(!live.current||generation!==serial.current||token!==latest.current.token)return;
   if(!validMemoryEtag(response.etag,response.data.memory.version)||response.data.individual_id!==latest.current.state?.individual_id||response.data.memory.validity==='superseded')throw new Error('此记录当前不能作为更正基线。');
   const next=response.data.memory;setDraft({...current,base:next,etag:response.etag,preview:true,status:'editing',intention:null});setNotice('已明确采用该记录为新基线；草稿保留，请重新确认对照。');if(target!==id){following.current=target;onSelect(target);}
  }catch(cause){if(live.current&&generation===serial.current){if(cause instanceof MemoryHttpError&&cause.status===404)clearPrivate();else setError(cause instanceof Error?cause.message:'新基线读取失败。');}}
  finally{if(live.current&&generation===serial.current)setBusy(false);}
 }
 function edit(){if(!r.data||!validated||!r.etag)return;setDraft({base:r.data.memory,etag:r.etag,content:r.data.memory.content,reason:'',preview:false,status:'editing',intention:null});setError('');setReceipt(null);setNotice('');}
 const canEdit=validated&&['active','paused'].includes(c.state?.lifecycle??'')&&r.data?.memory.validity!=='superseded';
 return <>
 <Panel title="记忆检查器" subtitle="实际单条记录、记忆版本与来源；owner 更正不等于事实已核实">
  <ResourceState loading={r.loading} error={r.error} retry={r.reload}/>{notice&&<p role="status" className="memory-editor-notice">{notice}</p>}
  {!id&&<Empty>选择条目后查看来源。</Empty>}
  {r.data&&!validated&&<p role="alert">缺少有效强 ETag 或版本不匹配，不能编辑此记录。</p>}
  {validated&&r.data&&<>{outsideResults&&<p className="context-note">选中记录不在当前列表结果中，仍按明确选择读取详情。</p>}<Badge>{r.data.memory.validity}</Badge><p className="memory-text">{r.data.memory.content}</p><dl className="memory-inspector-meta"><dt>记忆 ID / 版本</dt><dd><code>{r.data.memory.id}</code> / {r.data.memory.version}</dd><dt>全局状态版本</dt><dd>{r.data.state_version}（不用于 If-Match）</dd><dt>种类 / 适用范围</dt><dd>{r.data.memory.kind} / <code>{r.data.memory.scope}</code></dd><dt>来源事件</dt><dd>{r.data.memory.source_event_ids.map(source=><div key={source}><code className="block">{source}</code><button aria-label={`检查来源 ${source}`} aria-pressed={sourceEvent===source} onClick={()=>setSourceEvent(source)}>检查来源事件</button></div>)}</dd>{r.data.memory.supersedes&&<><dt>前身记忆</dt><dd><code>{r.data.memory.supersedes}</code></dd></>}</dl>{sourceEvent&&r.data.memory.source_event_ids.includes(sourceEvent)&&<SourceEventInspector eventId={sourceEvent} onClose={()=>setSourceEvent(null)}/>}<MemoryPeople memory={r.data.memory}/>
   <div className="memory-inspector-actions"><button className="privacy-danger" disabled={!privacy.ready||busy||privacy.state.phase==='unknown'||privacy.state.phase==='submitting'} onClick={()=>privacy.open(r.data!.memory.id)}>删除此记忆</button>{!draft&&<button disabled={!canEdit} onClick={edit}>更正此记忆</button>}<button aria-expanded={showHistory} onClick={()=>setShowHistory(v=>!v)}>{showHistory?'收起修订历史':'查看修订历史'}</button></div>
   {r.data.memory.validity==='superseded'&&<p>这是已被替代的历史解释；请从修订链选择当前记录后更正。</p>}
   {draft&&<section className="memory-revision-form" aria-label="owner 记忆更正"><h3>更正草稿</h3><p>基线：记忆版本 {draft.base.version}，作用域和种类保持不变。正文和理由只保存在当前内存。</p>
    {!draft.preview?<><label htmlFor="owner-memory-content">更正后的正文</label><textarea id="owner-memory-content" value={draft.content} maxLength={4000} disabled={busy} onChange={e=>setDraft(d=>d?{...d,content:e.target.value}:null)}/><small>{Array.from(draft.content).length}/2000 字符</small><label htmlFor="owner-memory-reason">更正理由</label><textarea className="reason" id="owner-memory-reason" value={draft.reason} maxLength={2000} disabled={busy} onChange={e=>setDraft(d=>d?{...d,reason:e.target.value}:null)}/><small>{Array.from(draft.reason).length}/1000 字符</small></>:<RevisionPreview draft={draft}/>}
    {error&&<p className="resource-error" role="alert">{error}</p>}
    {draft.status==='conflict'&&<div className="memory-unknown"><h3>最新服务端对照</h3><p className="memory-text">{r.data.memory.content}</p><p>最新记忆版本 {r.data.memory.version} · {r.data.memory.validity}；原草稿和原基线未覆盖。</p>{r.data.memory.validity!=='superseded'&&<button disabled={busy} onClick={()=>void carry(r.data!.memory.id)}>明确采用此版本并重新预览</button>}<button onClick={()=>{r.reload();setShowHistory(true);}} disabled={busy}>重新读取最新对照与历史</button></div>}
    <div className="form-actions">{draft.status==='unknown'?<button className="primary" disabled={busy} onClick={()=>void save()}>{busy?'等待原请求回执…':'重放同一保存请求'}</button>:draft.preview?<><button disabled={busy} onClick={()=>setDraft(d=>d?{...d,preview:false}:null)}>返回编辑</button><button className="primary" disabled={busy||draft.status==='conflict'} onClick={()=>void save()}>{busy?'等待服务端回执…':'确认保存更正'}</button></>:<button disabled={busy} onClick={()=>{if(!validText(draft.content,2000)||!validText(draft.reason,1000)){setError('请填写合法正文与更正理由，不能纯空白或包含 NUL。');return;}setError('');setDraft(d=>d?{...d,preview:true}:null);}}>预览更正</button>}<button disabled={busy} onClick={()=>guard.request(discard)}>取消更正</button></div>
   </section>}
   {receipt&&<div role="status" className="memory-revision-receipt" data-testid="memory-revision-receipt"><b>服务端已确认更正 · HTTP 201</b><p>新记忆版本 {receipt.memory_version} · 全局状态版本 {receipt.version} · 原记忆新版本 {receipt.previous_memory_version}</p><code className="block">新记忆 {receipt.memory_id}</code><code className="block">原记忆 {receipt.previous_memory_id}</code><code className="block">更正事件 {receipt.event_id}</code></div>}
  </>}
  <Unavailable>待复核操作未开放；普通更正不会覆盖原始事实事件。</Unavailable>
 </Panel>
 {id&&showHistory&&<MemoryHistory key={id} id={id} onSelect={next=>guard.request(()=>onSelect(next))} onCarry={next=>void carry(next)} canCarry={!!draft&&draft.status==='conflict'&&!busy} onUnavailable={()=>void verifyHistorySource()}/>}
 </>;
}
