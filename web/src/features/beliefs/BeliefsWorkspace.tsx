import {useEffect,useState,type FormEvent} from 'react';
import {Panel,Badge,Empty,ResourceState} from '../../design-system/components';
import {useIndividualContext} from '../../shared/context';
import {when} from '../../shared/labels';
import {useBeliefsRead} from './useBeliefsRead';
import {SourceEventInspector} from '../sources/SourceEventInspector';
import './beliefs.css';
type Belief={id:string;version:number;statement:string;scope:string;position:'tentative'|'held'|'questioned';status:'current'|'superseded';brief_basis:string;created_at:string;superseded_at:string|null;supersedes:string|null;support_event_ids:string[];counter_event_ids:string[]};
type Page={items:Belief[];next_cursor:string|null};
const positions={tentative:'暂定',held:'持有',questioned:'存疑'};
const valid=(s:string,max:number,empty:boolean)=>Array.from(s).length<=max&&!/[\u0000-\u001f\u007f-\u009f]/.test(s)&&(empty&&s===''||s.trim()!=='');
export function BeliefsWorkspace(){
 const c=useIndividualContext(),[online,setOnline]=useState(navigator.onLine);
 useEffect(()=>{const update=()=>setOnline(navigator.onLine);window.addEventListener('online',update);window.addEventListener('offline',update);return()=>{window.removeEventListener('online',update);window.removeEventListener('offline',update);};},[]);
 // A new observed epoch unmounts selection and source state synchronously.
 const epoch=JSON.stringify([c.token,c.state?.individual_id,c.state?.version,c.lastSyncedAt,c.connection,online]);
 return <Workspace key={epoch}/>;
}
function Workspace(){
 const [draft,setDraft]=useState(''),[scopeDraft,setScopeDraft]=useState(''),[statusDraft,setStatusDraft]=useState('current');
 const [filter,setFilter]=useState({q:'',scope:'',status:'current'}),[cursor,setCursor]=useState<string|null>(null),[selected,setSelected]=useState<string|null>(null),[source,setSource]=useState<string|null>(null),[error,setError]=useState('');
 const params=new URLSearchParams({limit:'30',status:filter.status});if(filter.q)params.set('q',filter.q);if(filter.scope)params.set('scope',filter.scope);if(cursor)params.set('cursor',cursor);
 const list=useBeliefsRead<Page>('/beliefs?'+params);
 const detail=useBeliefsRead<{belief:Belief}>(list.data&&selected?'/beliefs/'+encodeURIComponent(selected):null);
 const b=detail.data?.belief;
 const sourceAllowed=!!b&&!!source&&[...b.support_event_ids,...b.counter_event_ids].includes(source);
 function clear(){setSelected(null);setSource(null);}
 function search(e:FormEvent){e.preventDefault();if(!valid(draft,200,true)||!valid(scopeDraft,200,true)){setError('检索和作用域最多 200 个 Unicode 字符，不能仅为空白或包含控制字符。');return;}setError('');clear();setCursor(null);setFilter({q:draft,scope:scopeDraft,status:statusDraft});list.reload();}
 function choose(id:string){setSelected(id);setSource(null);}
 function page(next:string|null){clear();setCursor(next);}
 return <Panel title="信念与认识修正" subtitle="实际保存的认识及修订历史，只读检查">
  <p>持有是一种立场，不是事实认证。支持与反证是实际来源记录；简要依据是已保存的解释，不是模型隐藏推理。打开检查器不代表模型读过来源。</p>
  <form className="belief-filters" onSubmit={search}><label>搜索认识或简要依据<input type="search" value={draft} onChange={e=>setDraft(e.target.value)}/></label><label>精确作用域<input value={scopeDraft} onChange={e=>setScopeDraft(e.target.value)} placeholder="留空查看全部作用域"/></label><label>认识记录状态<select aria-label="认识记录状态" value={statusDraft} onChange={e=>setStatusDraft(e.target.value)}><option value="current">当前认识</option><option value="superseded">已被替代</option><option value="all">全部历史</option></select></label><button type="submit">应用认识筛选</button><button type="button" onClick={()=>{setDraft('');setScopeDraft('');setStatusDraft('current');setFilter({q:'',scope:'',status:'current'});page(null);setError('');list.reload();}}>清除认识筛选</button></form>
  {error&&<p role="alert" className="resource-error">{error}</p>}
  <button onClick={()=>{clear();list.reload();}}>刷新认识目录</button><ResourceState loading={list.loading} error={list.error} retry={()=>{clear();list.reload();}}/>
  <div className="belief-workspace"><section aria-label="认识列表"><p>已应用：{filter.status==='all'?'全部历史':filter.status==='current'?'当前认识':'已被替代'} · {filter.scope||'全部作用域'}{filter.q&&<> · 字面检索：{filter.q}</>}</p>
   {list.data&&(list.data.items.length?<div className="belief-list">{list.data.items.map(item=><button key={item.id} className="list-item" aria-pressed={item.id===selected} onClick={()=>choose(item.id)}><b>{item.statement}</b><small>{positions[item.position]} · {item.status==='current'?'当前':'已被替代'} · {item.scope} · v{item.version}</small></button>)}</div>:<Empty>当前可见认识中没有匹配项。</Empty>)}
   <div className="belief-actions">{cursor&&<button onClick={()=>page(null)}>返回认识首页</button>}{list.data?.next_cursor&&<button onClick={()=>page(list.data!.next_cursor)}>下一页认识</button>}</div>
  </section><section aria-label="认识详情"><h3>认识详情</h3><ResourceState loading={detail.loading} error={detail.error} retry={detail.reload}/>
   {!selected&&<Empty>选择实际认识记录后检查详情。</Empty>}
   {b&&<><h4>{b.statement}</h4><Badge>{positions[b.position]} · {b.status==='current'?'当前记录':'历史记录'}</Badge><dl className="belief-meta"><dt>认识标识</dt><dd><code>{b.id}</code></dd><dt>版本与作用域</dt><dd>v{b.version} · {b.scope}</dd><dt>形成时间</dt><dd>{when(b.created_at)}</dd><dt>被替代时间</dt><dd>{b.superseded_at?when(b.superseded_at):'尚未被替代'}</dd><dt>简要依据</dt><dd>{b.brief_basis}</dd><dt>前身记录</dt><dd>{b.supersedes?<><code>{b.supersedes}</code> <button onClick={()=>choose(b.supersedes!)}>检查前身认识</button></>:'无前身记录'}</dd></dl>
    <p>历史记录保留原认识及来源；这不是人格修订或撤回操作。</p>
    {(['support','counter'] as const).map(role=><section key={role} aria-label={role==='support'?'支持来源':'反对来源'}><h4>{role==='support'?'支持来源':'反对来源'}</h4>{(role==='support'?b.support_event_ids:b.counter_event_ids).length?(role==='support'?b.support_event_ids:b.counter_event_ids).map(id=><button key={id} className="belief-source" onClick={()=>{setSource(id);}}><span>检查{role==='support'?'支持':'反对'}来源</span> <code>{id}</code></button>):<p>没有保存此角色的来源。</p>}</section>)}
   </>}
  </section></div>
  <section aria-label="认识来源检查器"><SourceEventInspector key={b?.id??'none'} eventId={sourceAllowed?source:null} onClose={()=>setSource(null)}/>
  </section><a href="#/memory">检查记忆来源 →</a>
 </Panel>;
}
