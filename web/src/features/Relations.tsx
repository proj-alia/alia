import {useState} from 'react';
import {Panel,Empty,Badge,ResourceState} from '../design-system/components';
import {useIndividualContext} from '../shared/context';
import {when} from '../shared/labels';
import {useRelationshipsRead} from './relationships/useRelationshipsRead';
import type {RelationshipPage,RelationshipDetail,RelationshipEvents} from './relationships/types';
import './relationships/relationships.css';
const stages:Record<string,string>={unfamiliar:'初识'};
const sources:Record<string,string>={reported_claim:'对方陈述',generated:'模型生成内容',direct_observation:'直接观察记录',inference:'推断',external_document:'外部资料',fictional_background:'背景设定'};
function SavedValue({value}:{value:unknown}){
 if(value===null||value===undefined||(Array.isArray(value)&&value.length===0))return <p className="context-note">未保存相关记录</p>;
 if(Array.isArray(value))return <ul>{value.map((entry,index)=><li key={index}><SavedValue value={entry}/></li>)}</ul>;
 if(typeof value==='object')return <dl className="relationship-metadata">{Object.entries(value).map(([key,entry])=><div key={key}><dt>{key}</dt><dd><SavedValue value={entry}/></dd></div>)}</dl>;
 return <p className="relationship-value">{String(value)}</p>;
}
export function Relations(){
 const c=useIndividualContext(),[cursor,setCursor]=useState<string|null>(null),[selected,setSelected]=useState<string|null>(null),[eventCursor,setEventCursor]=useState<string|null>(null);
 const params=new URLSearchParams({limit:'30'});if(cursor)params.set('cursor',cursor);
 const list=useRelationshipsRead<RelationshipPage>('/relationships?'+params);
 const item=list.data?.items.find(r=>r.subject_id===selected);
 const detail=useRelationshipsRead<RelationshipDetail>(item?`/relationships/${item.subject_id}`:null);
 const eventsParams=new URLSearchParams({limit:'30'});if(eventCursor)eventsParams.set('cursor',eventCursor);
 const events=useRelationshipsRead<RelationshipEvents>(item&&detail.data?`/relationships/${item.subject_id}/events?${eventsParams}`:null);
 const relation=detail.data?.relationship;
 function directoryPage(next:string|null){setCursor(next);setSelected(null);setEventCursor(null);}
 function choose(id:string){setSelected(id);setEventCursor(null);}
 return <>
  <Panel title="关系与初始设定" subtitle="共同经历与当前理解分开保存">
   <p>初始设定阶段：{c.state?.persona.relationship.initial_stage}。这项出生设定不代表当前关系。</p>
   <p>当前档案来自实际保存的关系记录；期待、边界与承诺表达本个体的认识，不代表对方已认可，也不证明对方的感情。离开不会自动扣减关系。</p>
   <a href="#/media/chat">通过 Web 交流 →</a>
  </Panel>
  <div className="relationships-workspace">
   <Panel title="关系对象" subtitle="仅列出当前可见的真实关系记录，按主体标识排序">
    <button onClick={list.reload}>刷新关系目录</button><ResourceState loading={list.loading} error={list.error} retry={list.reload}/>
    <section aria-label="关系对象列表">{list.data&&(list.data.items.length?<div className="relationship-list">{list.data.items.map(r=><button className={`list-item${selected===r.subject_id?' selected':''}`} key={r.subject_id} aria-pressed={selected===r.subject_id} onClick={()=>choose(r.subject_id)}><b>{r.subject_key==='owner'?'实例使用者':r.subject_key}</b><small>当前保存阶段：{stages[r.stage]??r.stage}</small><small>关系版本 {r.version}</small><small><code>{r.subject_id}</code></small></button>)}</div>:<Empty>当前没有可见的关系档案。</Empty>)}</section>
    <div className="relationship-pages">{cursor&&<button onClick={()=>directoryPage(null)}>返回关系首页</button>}{list.data?.next_cursor&&<button onClick={()=>directoryPage(list.data!.next_cursor)}>下一页关系</button>}</div>
   </Panel>
   <Panel title="关系档案与共同交流" subtitle="只展示所选主体实际关联的已收到或已保存表达">
    <section aria-label="当前关系档案"><ResourceState loading={detail.loading} error={detail.error} retry={detail.reload}/>
     {!item&&!list.loading&&<Empty>{selected?'所选关系已不在当前可见页面，请重新选择。':'选择一个关系对象后读取其档案与关联交流。'}</Empty>}
     {relation&&<><h3>{relation.subject_key==='owner'?'实例使用者':relation.subject_key}</h3><Badge>{stages[relation.stage]??relation.stage}</Badge><dl className="relationship-metadata"><dt>关系主体标识</dt><dd><code>{relation.subject_id}</code></dd><dt>Alia 个体标识</dt><dd><code>{detail.data!.individual_id}</code></dd><dt>关系版本 / 全局状态版本</dt><dd>{relation.version} / {detail.data!.state_version}</dd><dt>观测时间</dt><dd>{when(detail.data!.observed_at)}</dd></dl>
      <h4>当前期待</h4><SavedValue value={relation.expectations}/><h4>关系边界</h4><SavedValue value={relation.boundaries}/><h4>已保存承诺</h4><SavedValue value={relation.commitments}/><h4>未解决事项</h4><SavedValue value={relation.unresolved}/>
      <button onClick={()=>{detail.reload();events.reload();}}>刷新所选关系</button>
     </>}
    </section>
    <section aria-label="关联交流时间线">{relation&&<><h3>关联交流时间线</h3><p>表达内容可能是陈述或生成结果，不能单凭记录判定为已核实事实；未投递草稿不在此列。</p><ResourceState loading={events.loading} error={events.error} retry={events.reload}/>
     {events.data&&(events.data.items.length?events.data.items.map(e=><article className="relationship-event" key={e.id}><b>{e.kind==='message.received'?'收到的表达':'已保存回复'}</b><Badge>{sources[e.source_kind]??e.source_kind}</Badge><p className="relationship-text">{e.text}</p><small>{when(e.occurred_at)} · 记录者 {e.actor_id} · 顺序 {e.sequence}</small><details><summary>来源事件</summary><code>{e.id}</code><p>来源类型：{e.source_kind} · 事件类型：{e.kind}</p></details></article>):<Empty>所选主体尚无可见的关联交流。</Empty>)}
     <div className="relationship-pages">{eventCursor&&<button onClick={()=>setEventCursor(null)}>最新关联交流</button>}{events.data?.next_cursor&&<button onClick={()=>setEventCursor(events.data!.next_cursor)}>更早的关联交流</button>}</div>
    </>}</section>
   </Panel>
  </div>
 </>;
}
