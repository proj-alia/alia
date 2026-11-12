import {OverviewHealth} from './OverviewHealth';
import {Panel,Badge,Empty,ResourceState} from '../design-system/components';
import {useIndividualContext} from '../shared/context';
import {statusNames,when} from '../shared/labels';
import {useOverview} from './overview/useOverview';
import './overview/overview.css';
const validityNames:Record<string,string>={active:'有效',disputed:'有争议',superseded:'已替代',stale:'已失效'};
export function Overview(){
 const c=useIndividualContext(),r=useOverview(),s=c.state,data=r.data;if(!s)return null;
 const pending=c.goals.items.filter(g=>['proposed','active','waiting'].includes(g.status));
 return <>
 <div className="overview-snapshot"><b>全量可见元数据</b><span>服务端同一只读快照 · 不以当前分页代替总数</span><button onClick={r.reload} disabled={r.loading}>刷新概览统计</button></div>
 <div className={r.loading?'overview-reading':''}><ResourceState loading={r.loading} error={r.error} retry={r.reload}/></div>
 {data&&<>
 <p className="footnote">统计观测（浏览器本地时间）<time dateTime={data.observed_at}>{when(data.observed_at)}</time> · 状态版本 {data.state_version} · 隐私代次 {data.privacy_epoch}。来源不可用的记录不计入。</p>
 <div className="metrics">{[['个体状态',statusNames[s.lifecycle]??s.lifecycle,`当前状态版本 ${s.version}`],['全量可见记忆',data.totals.memories.total,'含有效、争议、替代和失效记录'],['全量可见目标',data.totals.goals.total,'全部持久目标状态'],['全量可见运行',data.totals.runs.total,'全部持久运行状态']].map(([label,value,note],i)=><section className="metric" key={label}><span>{label}</span><strong data-testid={['overview-lifecycle','overview-memories-total','overview-goals-total','overview-runs-total'][i]}>{value}</strong><small>{note}</small></section>)}</div>
 <div className="overview-count-groups">{([['记忆状态',data.totals.memories,validityNames],['目标状态',data.totals.goals,statusNames],['运行状态',data.totals.runs,statusNames]] as const).map(([title,counts,names])=><Panel key={title} title={title} subtitle="全量可见历史"><dl>{Object.entries(counts).filter(([key])=>key!=='total').map(([key,n])=><div key={key}><dt>{names[key]??key}</dt><dd>{n}</dd></div>)}</dl></Panel>)}</div>
 <Panel title="近 7 日运行趋势（UTC）" subtitle="按运行创建日期分桶；失败数是该日创建且当前状态为失败的运行数，并非失败发生日">
  <div className="table-scroll" role="region" tabIndex={0} aria-label="UTC 七日运行趋势，可水平滚动"><table className="overview-trend-table"><caption>连续 7 个 UTC 日期，今日仅截至本次观测。0 只表示无可见记录，不证明全天在线。</caption><thead><tr><th scope="col">UTC 创建日期</th><th scope="col">创建数</th><th scope="col">当前失败</th><th scope="col">前台</th><th scope="col">后台</th></tr></thead><tbody>{data.trend.days.map(day=><tr key={day.date}><th scope="row"><time dateTime={day.date}>{day.date}</time>{day.partial&&<><br/><Badge>今日 · 部分日期</Badge></>}</th><td>{day.created}</td><td>{day.failed}</td><td>{day.foreground}</td><td>{day.background}</td></tr>)}</tbody></table></div>
 </Panel>
 <OverviewHealth data={data}/>
 <div className="two-columns"><Panel title="待办目标" subtitle="当前已加载分页摘要，最多 4 条" action={<a href="#/tasks">管理目标 →</a>}>{pending.length?pending.slice(0,4).map(g=><div className="event-row" key={g.id}><div><Badge>{statusNames[g.status]??g.status}</Badge><h3>{g.description}</h3><p>{g.next_step}</p></div></div>):<Empty>已加载记录中没有待办目标；完整列表可在任务页查看。</Empty>}<p className="footnote">外部行动审批尚未接入，待审批数量未知。</p></Panel><Panel title="近期交流" subtitle="当前已加载分页摘要，最多 4 条" action={<a href="#/data/sessions">查看记录 →</a>}>{c.messages.items.length?c.messages.items.slice(0,4).map(m=><div className="event-row" key={m.id}><div><b>{m.role==='user'?'用户输入':'已保存回复'}</b><p className="clamp">{m.text}</p><small>{when(m.occurred_at)}</small></div></div>):<Empty>已加载分页中没有交流事件。<a href="#/platforms/web/chat">打开 Web 交流</a></Empty>}</Panel></div>
 </>}
 <Panel title="资源入口"><div className="filterbar">{[['memory','记忆管理'],['tasks','管理目标'],['data/sessions','交流记录'],['config','运行配置'],['models','模型服务'],['extensions','扩展管理'],['data/traces','执行追踪'],['platforms/web/chat','Web 交流']].map(([route,label])=><a className="button" key={route} href={'#/'+route}>{label}</a>)}</div>{data&&<small>{s.persona.identity.display_name} · <code>{s.individual_id}</code> · 创建于 {when(s.created_at)} · 当前可用授权能力 {s.capabilities.filter(c=>c.authorized&&c.available).length}</small>}</Panel>
 </>;
}
