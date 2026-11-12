import {useState,type FormEvent} from 'react';
import {Panel,Badge,Empty,ResourceState} from '../design-system/components';
import {when} from '../shared/labels';
import {useNotesRead} from './notes/useNotesRead';
import type {NotePage,NoteSlice} from './notes/types';
import './notes/notes.css';

export function Notes(){
 const [draft,setDraft]=useState(''),[query,setQuery]=useState(''),[cursor,setCursor]=useState<string|null>(null),[selected,setSelected]=useState<string|null>(null),[offset,setOffset]=useState(0),[inputError,setInputError]=useState('');
 const params=new URLSearchParams({limit:'30'});if(query)params.set('q',query);if(cursor)params.set('cursor',cursor);
 const list=useNotesRead<NotePage>('/notes?'+params);
 const item=list.data?.items.find(d=>d.id===selected);
 const detail=useNotesRead<NoteSlice>(item?`/notes/${encodeURIComponent(item.id)}?offset=${offset}&max_chars=2000`:null);
 function search(e:FormEvent){e.preventDefault();setInputError('');if(Array.from(draft).length>200||/[\u0000-\u001f\u007f-\u009f]/.test(draft)){setInputError('检索词最多 200 个字符，不能包含控制字符。');return;}const next=draft.trim()?draft:'';setSelected(null);setOffset(0);setCursor(null);if(next===query)list.reload();else setQuery(next);}
 function page(next:string|null){setSelected(null);setOffset(0);setCursor(next);}
 return <>
  <Panel title="笔记目录" subtitle="成功保存的个人生成笔记，按保存时间倒序；目录不包含正文">
   <p>生成笔记可能包含推断或错误；保存成功只证明内容已保存，不代表事实已核实。页面查看不会产生模型阅读或工具执行记录。</p>
   <form className="note-search" onSubmit={search}><label>搜索笔记标题或动作标识<input type="search" value={draft} onChange={e=>setDraft(e.target.value)} maxLength={400} placeholder="按标题或动作 UUID 字面搜索"/></label><button type="submit" disabled={list.loading}>检索笔记</button><button type="button" onClick={()=>{setDraft('');setQuery('');page(null);list.reload();}}>清除笔记筛选</button></form>
   {inputError&&<p role="alert" className="resource-error">{inputError}</p>}
   <button onClick={list.reload}>刷新笔记目录</button>
   <ResourceState loading={list.loading} error={list.error} retry={list.reload}/>
   <div className="notes-workspace">
    <section aria-label="笔记列表"><p>{list.data?`本页 ${list.data.items.length} 条${list.data.next_cursor?'，还有后续笔记':'，已到当前查询末页'}`:'尚未取得笔记目录。'}{query&&<> · 已应用检索：{query}</>}</p>
     {list.data&&(list.data.items.length?<div className="note-list">{list.data.items.map(d=><button key={d.id} className={`list-item${d.id===selected?' selected':''}`} aria-pressed={d.id===selected} onClick={()=>{setSelected(d.id);setOffset(0);}}><b>{d.title}</b><small>{d.capability_id} · {d.capability_version}</small><small>保存于 {when(d.created_at)}</small></button>)}</div>:<Empty>当前可见笔记中没有匹配项。</Empty>)}
     <div className="note-pages">{cursor&&<button onClick={()=>page(null)} disabled={list.loading}>返回笔记首页</button>}{list.data?.next_cursor&&<button onClick={()=>page(list.data!.next_cursor)} disabled={list.loading}>下一页笔记</button>}</div>
    </section>
    <section aria-label="笔记阅读检查器">
     <h3>笔记阅读检查器</h3><ResourceState loading={detail.loading} error={detail.error} retry={detail.reload}/>
     {!item&&!list.loading&&<Empty>{selected?'所选笔记已不在当前可见页面，请重新选择。':'选择一项笔记后读取一个正文片段。'}</Empty>}
     {item&&detail.data&&<><h4>{detail.data.title}</h4><Badge>生成内容 · 已保存</Badge><dl className="note-metadata"><dt>笔记 / 保存动作</dt><dd><code>{detail.data.action_id}</code></dd><dt>保存工具</dt><dd>{detail.data.capability_id} · {detail.data.capability_version}</dd><dt>保存结果事件</dt><dd><code>{detail.data.result_event_id}</code></dd><dt>关联运行</dt><dd><a href={`#/traces/${detail.data.run_id}`}><code>{detail.data.run_id}</code></a></dd><dt>保存时间</dt><dd>{when(detail.data.created_at)}</dd><dt>本次实际读取</dt><dd>Unicode 字符区间 [{detail.data.offset}, {detail.data.offset+Array.from(detail.data.text).length}) · 共 {detail.data.total_chars} 字符</dd><dt>观测时间</dt><dd><time dateTime={detail.data.observed_at}>{when(detail.data.observed_at)}</time></dd></dl>
      <pre className="note-text" aria-label="笔记正文片段">{detail.data.text||'已到笔记末尾，此片段为空。'}</pre>
      <div className="note-pages"><button disabled={detail.loading||offset===0} onClick={()=>setOffset(Math.max(0,offset-2000))}>上一段正文</button><button disabled={detail.loading||detail.data.next_offset===null} onClick={()=>setOffset(detail.data!.next_offset!)}>下一段正文</button><button onClick={detail.reload}>重新读取片段</button></div>
     </>}
    </section>
   </div>
  </Panel>
 </>;
}
