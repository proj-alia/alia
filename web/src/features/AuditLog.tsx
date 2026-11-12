import {useState} from 'react';
import {Panel,Badge,Empty,ResourceState} from '../design-system/components';
import {DataFilters,useDataResource} from './Traces';
import type {AuditEvent} from '../shared/consoleTypes';
import type {Page} from '../shared/types';
import {when} from '../shared/labels';
export function AuditLog(){const [cursor,setCursor]=useState<string|null>(null),[filters,setFilters]=useState('');const params=new URLSearchParams(filters);if(cursor)params.set('cursor',cursor);const r=useDataResource<Page<AuditEvent>>(`/audit?${params}`);const rows=r.data?.items??[];
 return <Panel title="系统事件元数据" subtitle="按事件序号分页；只展示白名单字段，不包含原始payload或服务器日志"><DataFilters onApply={params=>{setFilters(params.toString());setCursor(null);}}/><div className="filterbar"><button onClick={r.reload} disabled={r.loading}>刷新事件</button>{cursor&&<button onClick={()=>setCursor(null)}>首批事件</button>}<span>{r.connection}</span></div><p>事件成功刷新：{r.refreshedAt?<time dateTime={r.refreshedAt}>{when(r.refreshedAt)}</time>:'尚无记录'}</p><ResourceState loading={r.loading} error={r.error} retry={r.reload}/>{r.data&&(rows.length?rows.map(e=><div className="event-row" key={e.id}><div><b>{e.kind}</b>{' '}<Badge>{e.source_kind}</Badge><p>{e.actor_id} · {when(e.occurred_at)} · 序号 {e.sequence}</p><details><summary>事件标识</summary><code className="block">{e.id}</code>{e.correlation_id&&<code className="block">关联 {e.correlation_id}</code>}</details></div></div>):<Empty>没有符合服务端筛选的系统事件。</Empty>)}{r.data?.next_cursor&&<button onClick={()=>setCursor(r.data!.next_cursor)}>下一批系统事件</button>}</Panel>;
}
