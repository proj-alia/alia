import {useEffect,useRef,useState,type FormEvent} from 'react';
import {Panel,Badge,Empty,ResourceState,Data} from '../design-system/components';
import {useIndividualContext} from '../shared/context';
import {ApiError} from '../shared/repository';
import type {RunSummary,RunDetail} from '../shared/consoleTypes';
import type {Page} from '../shared/types';
import {statusNames,failureNames,when} from '../shared/labels';
// Dedicated read-only query lifecycle. SSE and timer signals share one in-flight
// request, and a short scheduling window coalesces events at a timer boundary.
export function useDataResource<T>(path:string|null,follow=false){
 const c=useIndividualContext(),latest=useRef(c);latest.current=c;
 const [result,setResult]=useState<{data:T|null;loading:boolean;error:string;refreshedAt:string|null}>({data:null,loading:false,error:'',refreshedAt:null});
 const request=useRef<(automatic?:boolean)=>void>(()=>{}),previousSync=useRef(c.lastSyncedAt),pending=useRef<Promise<T>|null>(null);
 useEffect(()=>{
  let active=true,inFlight=false;let scheduled:ReturnType<typeof setTimeout>|undefined;
  setResult({data:null,loading:!!path,error:'',refreshedAt:null});
  const load=async()=>{
   if(!active||inFlight||!path||!latest.current.token)return;
   inFlight=true;setResult(r=>({...r,loading:true,error:''}));
   try{
    // A changed filter/selection waits for the previous HTTP read to settle.
    // The previous effect cannot publish into the new query or session.
    if(pending.current)await pending.current.catch(()=>{});
    if(!active)return;
    const read=latest.current.read<T>(path);pending.current=read;
    let data:T;try{data=await read;}finally{if(pending.current===read)pending.current=null;}
    if(active)setResult({data,loading:false,error:'',refreshedAt:new Date().toISOString()});
   }
   catch(error){if(!active)return;if(error instanceof ApiError&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.clearSession();return;}
    setResult(r=>({...r,data:null,loading:false,error:error instanceof ApiError?error.message:'读取失败，请检查连接后重试。'}));
   }finally{inFlight=false;}
  };
  request.current=(automatic=false)=>{
   if(automatic&&document.hidden)return;
   if(scheduled||inFlight)return;
   if(automatic)scheduled=setTimeout(()=>{scheduled=undefined;if(!document.hidden)void load();},150);else void load();
  };
  void load();
  return()=>{active=false;if(scheduled)clearTimeout(scheduled);};
 },[path,c.token]);
 useEffect(()=>{
  if(previousSync.current!==c.lastSyncedAt){previousSync.current=c.lastSyncedAt;if(follow)request.current(true);}
 },[c.lastSyncedAt,follow]);
 useEffect(()=>{
  if(!follow||!path)return;
  const timer=setInterval(()=>request.current(true),5000);
  const visible=()=>{if(!document.hidden)request.current(true);};
  document.addEventListener('visibilitychange',visible);
  return()=>{clearInterval(timer);document.removeEventListener('visibilitychange',visible);};
 },[follow,path]);
 return {...result,reload:()=>request.current(),connection:c.connection};
}
type Filters={q:string;status:string;mode:string;from:string;to:string};
const emptyFilters:Filters={q:'',status:'',mode:'',from:'',to:''};
export function DataFilters({runs=false,onApply}:{runs?:boolean;onApply:(params:URLSearchParams)=>void}){
 const [draft,setDraft]=useState(emptyFilters),[error,setError]=useState('');
 function apply(e:FormEvent){e.preventDefault();const params=new URLSearchParams();
  if([...draft.q].length>200){setError('搜索最多 200 个字符。');return;}
  const from=draft.from?Date.parse(draft.from+'Z'):null,to=draft.to?Date.parse(draft.to+'Z'):null;
  if((from!==null&&!Number.isFinite(from))||(to!==null&&!Number.isFinite(to))||(from!==null&&to!==null&&from>=to)){setError('请填写有效 UTC 时间，且开始时间早于结束时间。');return;}
  if(draft.q.trim())params.set('q',draft.q);if(runs&&draft.status)params.set('status',draft.status);if(runs&&draft.mode)params.set('mode',draft.mode);
  if(from!==null)params.set('from',new Date(from).toISOString());if(to!==null)params.set('to',new Date(to).toISOString());setError('');onApply(params);
 }
 const field=(key:keyof Filters,value:string)=>setDraft(d=>({...d,[key]:value}));
 return <form className="filterbar" onSubmit={apply} style={{gridColumn:'1 / -1'}}><label className="search">搜索公开元数据<input type="search" value={draft.q} onChange={e=>field('q',e.target.value)} placeholder={runs?'运行 ID、公开错误码':'事件类型、记录者、事件或可见关联 ID'}/></label>
  {runs&&<><label>运行状态<select aria-label="运行状态" value={draft.status} onChange={e=>field('status',e.target.value)}><option value="">全部状态</option>{['queued','running','committed','failed','cancelled','stale'].map(s=><option key={s} value={s}>{statusNames[s]??s}</option>)}</select></label><label>运行模式<select aria-label="运行模式" value={draft.mode} onChange={e=>field('mode',e.target.value)}><option value="">全部模式</option><option value="foreground">前台</option><option value="background">后台</option></select></label></>}
  <label>开始时间（UTC，包含）<input type="datetime-local" step="1" value={draft.from} onChange={e=>field('from',e.target.value)}/></label><label>结束时间（UTC，不包含）<input type="datetime-local" step="1" value={draft.to} onChange={e=>field('to',e.target.value)}/></label><button type="submit">应用筛选</button><button type="button" onClick={()=>{setDraft(emptyFilters);setError('');onApply(new URLSearchParams());}}>清除筛选</button>{error&&<p role="alert">{error}</p>}
 </form>;
}
export function Traces({selectedId}:{selectedId?:string}){
 const [filters,setFilters]=useState(''),[cursor,setCursor]=useState<string|null>(null),[follow,setFollow]=useState(false),[notice,setNotice]=useState(''),[hidden,setHidden]=useState(document.hidden);
 useEffect(()=>{const changed=()=>setHidden(document.hidden);document.addEventListener('visibilitychange',changed);return()=>document.removeEventListener('visibilitychange',changed);},[]);
 const params=new URLSearchParams(filters);if(cursor)params.set('cursor',cursor);
 const runs=useDataResource<Page<RunSummary>>(`/runs?${params}`,follow),detail=useDataResource<RunDetail>(selectedId?`/runs/${encodeURIComponent(selectedId)}`:null,follow);
 const apply=(params:URLSearchParams)=>{setFilters(params.toString());setCursor(null);setNotice('已应用服务端筛选，返回第一页。');};
 const historical=(next:string)=>{setCursor(next);setFollow(false);setNotice('已进入历史页，自动跟随已关闭；保留当前筛选和所选详情。');};
 return <><DataFilters runs onApply={apply}/><div className="filterbar"><button onClick={runs.reload} disabled={runs.loading}>重新加载运行</button><label className="inline"><input type="checkbox" checked={follow} onChange={e=>{setFollow(e.target.checked);if(e.target.checked){setCursor(null);setNotice('自动跟随第一页，每 5 秒刷新；当前筛选与详情保留。');if(!cursor)runs.reload();detail.reload();}}}/>自动跟随执行</label>{cursor&&<button onClick={()=>setCursor(null)}>返回第一页</button>}<span role="status">{follow?(hidden?'页面隐藏，跟随已暂停':'跟随中，每 5 秒刷新'):'自动跟随已关闭'} · {runs.connection}</span>{notice&&<p role="status">{notice}</p>}</div><Panel title="执行记录" subtitle="按创建时间倒序；搜索、时间、状态与模式由服务端筛选"><p>列表成功刷新：{runs.refreshedAt?<time dateTime={runs.refreshedAt}>{when(runs.refreshedAt)}</time>:'尚无记录'}</p><ResourceState loading={runs.loading} error={runs.error} retry={runs.reload}/>{runs.data&&(runs.data.items.length?<div className="table-scroll"><table style={{tableLayout:'fixed',minWidth:560}}><colgroup><col style={{width:'40%'}}/><col style={{width:'25%'}}/><col style={{width:'9%'}}/><col style={{width:'11%'}}/><col style={{width:'15%'}}/></colgroup><thead><tr><th>运行 / Trace</th><th>状态</th><th>来源</th><th>模型 / 工具</th><th>创建时间</th></tr></thead><tbody>{runs.data.items.map(r=><tr key={r.id}><td><a href={`#/traces/${r.id}`}><code>{r.id}</code></a></td><td><Badge tone={r.status==='failed'?'danger':'neutral'}>{statusNames[r.status]??r.status}</Badge>{r.error_code&&<small className="block">{failureNames[r.error_code]??r.error_code}</small>}</td><td>{r.mode==='foreground'?'前台':'后台'}</td><td>{r.model_calls} / {r.tool_calls}</td><td>{when(r.created_at)}</td></tr>)}</tbody></table></div>:<Empty>没有符合条件的运行。</Empty>)}{runs.data?.next_cursor&&<button onClick={()=>historical(runs.data!.next_cursor!)}>下一页运行</button>}</Panel><Panel title="运行详情与步骤" subtitle="真实调用记录；输入输出正文不在追踪接口中暴露"><p>详情成功刷新：{detail.refreshedAt?<time dateTime={detail.refreshedAt}>{when(detail.refreshedAt)}</time>: '尚无记录'}</p>{selectedId&&runs.data&&!runs.data.items.some(r=>r.id===selectedId)&&<p className="context-note">所选运行不在当前页或筛选结果中；详情仍按原选择显示。</p>}<ResourceState loading={detail.loading} error={detail.error} retry={detail.reload}/>{!selectedId&&<Empty>选择一条运行查看步骤、用量和结果来源。</Empty>}{detail.data&&<><div className="inline"><Badge>{statusNames[detail.data.run.status]??detail.data.run.status}</Badge><code>{detail.data.run.id}</code></div><p>临时重试 {detail.data.run.model_retries} 次 · 提案修复 {detail.data.run.repair_attempts} 次。手动重放旧运行未开放；失败不会显示为完成。</p>{detail.data.spans.length?detail.data.spans.map(span=><article className="span-row" key={span.id}><div className="inline"><Badge>{span.kind==='model'?'模型调用':'工具步骤'}</Badge><b>{span.capability_id??'生成模型'}</b><Badge tone={['failed','unknown'].includes(span.status)?'warning':'neutral'}>{statusNames[span.status]??span.status}</Badge></div><p>耗时：{span.duration_ms===null?'未记录或尚未结束':`${span.duration_ms.toFixed(1)} ms`}{span.execution_mode?` · ${span.execution_mode}`:''}</p>{span.kind==='model'&&<p>计费 {span.charged_tokens??0} token · {span.usage_unknown?'用量未知，按预算契约记账':`输入 ${span.input_tokens??'未收到'} / 输出 ${span.output_tokens??'未收到'}`}</p>}<details><summary>步骤元数据</summary><Data value={span}/></details></article>):<Empty>尚无模型或工具步骤，排队和未执行的运行可以为空。</Empty>}{detail.data.spans_more&&<p>步骤超过当前50项展示上限。</p>}<p>历史工具没有记录开始时间时保持未知。该视图不显示隐藏推理、请求头或部署凭证。</p></>}</Panel></>;
}
