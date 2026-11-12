import {useState} from 'react';
import {Badge,Empty,ResourceState} from '../../design-system/components';
import {when} from '../../shared/labels';
import {useSourceRead,useSourceSnapshot} from './useSourceRead';
import {sourceLabels,validFragment,type LinkedDocumentView,type SourceEventView} from './types';
import './sources.css';
export type SourceEventInspectorProps={eventId:string|null;onClose?:()=>void};
/** Stable shared inline reader. Opening a source does not change the host draft. */
export function SourceEventInspector({eventId,onClose}:SourceEventInspectorProps){
 const snapshot=useSourceSnapshot();
 return <section className="source-event-inspector" aria-label="来源事件检查器"><div className="source-inspector-heading"><h3>来源事件检查器</h3>{onClose&&<button onClick={onClose}>关闭来源检查器</button>}</div><p>来源说明记录从何而来，不保证其中陈述为真。页面查看不等于模型已读，也不产生工具阅读回执。</p>{eventId?<SourceReader key={eventId+snapshot.identity} eventId={eventId} snapshot={snapshot}/>:<Empty>选择一个实际来源事件后读取安全投影。</Empty>}</section>;
}
function SourceReader({eventId,snapshot}:{eventId:string;snapshot:ReturnType<typeof useSourceSnapshot>}){
 const [offset,setOffset]=useState(0),[documentOpen,setDocumentOpen]=useState(false),[linkNotice,setLinkNotice]=useState('');
 const r=useSourceRead<SourceEventView>(`/source-events/${encodeURIComponent(eventId)}?offset=${offset}&max_chars=2000`,snapshot);
 const value=r.data,valid=!!value&&value.individual_id===snapshot.individual&&value.event.id===eventId&&((value.content_status==='available'&&!!value.content&&validFragment(value.content,offset))||(value.content_status==='not_exposed'&&value.content===null&&offset===0));
 const data=valid?value:null;
 return <><ResourceState loading={r.loading} error={r.error} retry={r.reload}/>{value&&!valid&&<p role="alert" className="resource-error">来源响应不符合当前记录与片段契约，未显示旧内容。</p>}{linkNotice&&<p role="status">{linkNotice}</p>}{data&&<><div className="inline"><Badge>{sourceLabels[data.event.source_kind]??data.event.source_kind}</Badge><span>{data.event.kind}</span></div><dl className="source-metadata"><dt>事件 / 顺序</dt><dd><code>{data.event.id}</code> / {data.event.sequence}</dd><dt>记录主体</dt><dd>{data.event.actor_id}</dd><dt>发生时间</dt><dd><time dateTime={data.event.occurred_at}>{when(data.event.occurred_at)}</time></dd><dt>保存时间</dt><dd><time dateTime={data.event.recorded_at}>{when(data.event.recorded_at)}</time></dd><dt>本次观测（浏览器本地时间）</dt><dd><time dateTime={data.observed_at}>{when(data.observed_at)}</time></dd><dt>状态版本 / 隐私代次</dt><dd>{data.state_version} / {data.privacy_epoch}</dd></dl>
 {data.content_status==='available'&&data.content?<><p>Unicode 字符区间 [{data.content.offset}, {data.content.offset+Array.from(data.content.text).length}) · 共 {data.content.total_chars} 字符</p><pre className="source-text" aria-label="来源正文片段">{data.content.text}</pre>{!data.content.text&&<p>此来源的正文片段为空。</p>}<div className="source-fragment-actions"><button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-2000))}>上一段来源</button><button disabled={data.content.next_offset===null} onClick={()=>setOffset(data.content!.next_offset!)}>下一段来源</button><button onClick={r.reload}>重新读取来源</button></div>{data.content.next_offset===null&&<p role="status">已到来源正文末尾。</p>}</>:<><p className="source-not-exposed">not_exposed · 此事件存在；安全投影仅公开元数据，不公开正文。</p><button onClick={r.reload}>重新读取来源</button></>}
 <div className="source-fragment-actions">{data.document_id&&<button aria-expanded={documentOpen} onClick={()=>{setDocumentOpen(v=>!v);setLinkNotice('');}}>{documentOpen?'收起关联资料':'读取关联资料片段'}</button>}{data.run_id&&<a className="button" href={'#/data/traces/'+encodeURIComponent(data.run_id)}>查看实际关联运行</a>}</div>
 {data.document_id&&documentOpen&&<LinkedDocument key={data.document_id} id={data.document_id} snapshot={snapshot} onUnavailable={()=>{setDocumentOpen(false);setLinkNotice('关联资料当前不可用，旧片段已清除；正在重新验证来源链接。');r.reload();}}/>}
 </>}</>;
}
function LinkedDocument({id,snapshot,onUnavailable}:{id:string;snapshot:ReturnType<typeof useSourceSnapshot>;onUnavailable:()=>void}){
 const [offset,setOffset]=useState(0);const r=useSourceRead<LinkedDocumentView>(`/documents/${encodeURIComponent(id)}?offset=${offset}&max_chars=2000`,snapshot,onUnavailable);
 const value=r.data,valid=!!value&&value.individual_id===snapshot.individual&&value.document_id===id&&value.event_id===id&&validFragment(value,offset),data=valid?value:null;
 return <section className="source-linked-document" aria-label="关联资料片段"><h4>关联资料片段</h4><p>按真实 document_id 读取一个片段；页面检查不代表模型执行了 documents.read。</p><ResourceState loading={r.loading} error={r.error} retry={r.reload}/>{value&&!valid&&<p role="alert" className="resource-error">关联资料响应未能验证，未显示正文。</p>}{data&&<><h4>{data.title}</h4><Badge>{sourceLabels[data.source_kind]??data.source_kind}</Badge><dl className="source-metadata"><dt>资料 / 来源事件</dt><dd><code>{data.document_id}</code></dd><dt>来源标签</dt><dd>{data.source_label}</dd><dt>观测时间（浏览器本地时间）</dt><dd><time dateTime={data.observed_at}>{when(data.observed_at)}</time></dd></dl><p>Unicode 字符区间 [{data.offset}, {data.offset+Array.from(data.text).length}) · 共 {data.total_chars} 字符</p><pre className="source-text" aria-label="关联资料正文片段">{data.text}</pre>{!data.text&&<p>此资料的正文片段为空。</p>}<div className="source-fragment-actions"><button disabled={offset===0} onClick={()=>setOffset(Math.max(0,offset-2000))}>上一段关联资料</button><button disabled={data.next_offset===null} onClick={()=>setOffset(data.next_offset!)}>下一段关联资料</button><button onClick={r.reload}>重新读取关联资料</button></div>{data.next_offset===null&&<p role="status">已到关联资料末尾。</p>}</>}</section>;
}
