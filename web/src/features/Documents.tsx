import {useState,type FormEvent} from 'react';
import {Panel,Badge,Empty,ResourceState} from '../design-system/components';
import {when} from '../shared/labels';
import {useDocumentsRead} from './documents/useDocumentsRead';
import type {DocumentPage,DocumentSlice} from './documents/types';
import './documents/documents.css';

export function Documents(){
 const [draft,setDraft]=useState(''),[query,setQuery]=useState(''),[cursor,setCursor]=useState<string|null>(null),[selected,setSelected]=useState<string|null>(null),[offset,setOffset]=useState(0),[inputError,setInputError]=useState('');
 const params=new URLSearchParams({limit:'30'});if(query)params.set('q',query);if(cursor)params.set('cursor',cursor);
 const list=useDocumentsRead<DocumentPage>('/documents?'+params);
 const item=list.data?.items.find(d=>d.id===selected);
 const detail=useDocumentsRead<DocumentSlice>(item?`/documents/${encodeURIComponent(item.id)}?offset=${offset}&max_chars=2000`:null);
 function search(e:FormEvent){e.preventDefault();setInputError('');if(Array.from(draft).length>200||/[\u0000-\u001f\u007f]/.test(draft)){setInputError('检索词最多 200 个字符，不能包含控制字符。');return;}const next=draft.trim()?draft:'';setSelected(null);setOffset(0);setCursor(null);if(next===query)list.reload();else setQuery(next);}
 function page(next:string|null){setSelected(null);setOffset(0);setCursor(next);}
 return <>
  <Panel title="资料目录" subtitle="查询全部当前可见的持久资料，按保存时间倒序；目录不包含正文">
   <p>页面查看不等于模型已读；外部资料不代表亲身经历。只有模型工具的实际执行才形成阅读回执。</p>
   <form className="document-search" onSubmit={search}><label>搜索资料标题或来源<input type="search" value={draft} onChange={e=>setDraft(e.target.value)} maxLength={400} placeholder="按标题或来源字面搜索"/></label><button type="submit" disabled={list.loading}>检索资料</button><button type="button" onClick={()=>{setDraft('');setQuery('');page(null);list.reload();}}>清除资料筛选</button></form>
   {inputError&&<p role="alert" className="resource-error">{inputError}</p>}
   <button onClick={list.reload}>刷新资料目录</button>
   <ResourceState loading={list.loading} error={list.error} retry={list.reload}/>
   <div className="documents-workspace">
    <section aria-label="资料列表"><p>{list.data?`本页 ${list.data.items.length} 条${list.data.next_cursor?'，还有后续资料':'，已到当前查询末页'}`:'尚未取得资料目录。'}{query&&<> · 已应用检索：{query}</>}</p>
     {list.data&&(list.data.items.length?<div className="document-list">{list.data.items.map(d=><button key={d.id} className={`list-item${d.id===selected?' selected':''}`} aria-pressed={d.id===selected} onClick={()=>{setSelected(d.id);setOffset(0);}}><b>{d.title}</b><small>{d.source_label}</small><small>保存于 {when(d.created_at)}</small></button>)}</div>:<Empty>当前可见资料中没有匹配项。</Empty>)}
     <div className="document-pages">{cursor&&<button onClick={()=>page(null)} disabled={list.loading}>返回资料首页</button>}{list.data?.next_cursor&&<button onClick={()=>page(list.data!.next_cursor)} disabled={list.loading}>下一页资料</button>}</div>
    </section>
    <section aria-label="资料阅读检查器">
     <h3>资料阅读检查器</h3><ResourceState loading={detail.loading} error={detail.error} retry={detail.reload}/>
     {!item&&!list.loading&&<Empty>{selected?'所选资料已不在当前可见页面，请重新选择。':'选择一项资料后读取一个正文片段。'}</Empty>}
     {item&&detail.data&&<><h4>{detail.data.title}</h4><Badge>{detail.data.source_kind==='external_document'?'外部资料':detail.data.source_kind}</Badge><dl className="document-metadata"><dt>资料 / 来源事件</dt><dd><code>{detail.data.document_id}</code></dd><dt>来源标签</dt><dd>{detail.data.source_label}</dd><dt>本次实际读取</dt><dd>Unicode 字符区间 [{detail.data.offset}, {detail.data.offset+Array.from(detail.data.text).length}) · 共 {detail.data.total_chars} 字符</dd><dt>观测时间</dt><dd><time dateTime={detail.data.observed_at}>{when(detail.data.observed_at)}</time></dd></dl>
      <pre className="document-text" aria-label="资料正文片段">{detail.data.text||'已到资料末尾，此片段为空。'}</pre>
      <div className="document-pages"><button disabled={detail.loading||offset===0} onClick={()=>setOffset(Math.max(0,offset-2000))}>上一段正文</button><button disabled={detail.loading||detail.data.next_offset===null} onClick={()=>setOffset(detail.data!.next_offset!)}>下一段正文</button><button onClick={detail.reload}>重新读取片段</button></div>
     </>}
    </section>
   </div>
  </Panel>
 </>;
}
