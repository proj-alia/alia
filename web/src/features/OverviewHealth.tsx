import {Panel,Badge,Empty,Unavailable} from '../design-system/components';
import type {OverviewSnapshot} from './overview/types';
import {failureNames,when} from '../shared/labels';
export function OverviewHealth({data}:{data:OverviewSnapshot}){return <div className="two-columns"><Panel title="近期失败运行" action={<a href="#/data/traces">全部执行追踪 →</a>}>
 <p>全量可见失败运行 {data.totals.runs.failed} 条；以下最多 5 条，按创建时间和 ID 倒序。统计快照 {when(data.observed_at)}。创建、结束与观测时间按浏览器本地时区显示。</p>
 {data.recent_failed_runs.length?data.recent_failed_runs.map(run=><article className="event-row overview-failure" key={run.id}><div><Badge tone="danger">失败</Badge>{' '}<Badge>{run.mode==='foreground'?'前台':'后台'}</Badge><p><a href={`#/traces/${run.id}`} aria-label={`查看失败运行 ${run.id}`}><code>{run.id}</code></a></p><p>{run.error_code?(failureNames[run.error_code]??run.error_code):'未记录公开错误码'}</p><small>创建 {when(run.created_at)} · 结束 {run.finished_at?when(run.finished_at):'未记录'}</small></div></article>):<Empty>没有当前可见的失败运行；这不表示模型或工具已通过健康检查。</Empty>}
 <p className="footnote">只展示公开元数据。零记录不代表全天在线或模型健康；实际执行结果请查看追踪。</p>
 </Panel><Panel title="待审批事项" action={<a href="#/governance">查看治理 →</a>}><Unavailable>审批服务尚未接入，待审批数量未知。</Unavailable><p>已生效的能力授权仍由服务端验证；不会以模拟批准按钮放行行动。</p></Panel></div>;}
