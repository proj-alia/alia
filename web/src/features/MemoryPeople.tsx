import {Badge,Empty} from '../design-system/components';
import type {Memory} from '../shared/types';
import {when} from '../shared/labels';

export function MemoryPeople({memory}:{memory:Memory}) {
  return <section aria-label="人物引用">
    <h3>人物引用</h3>
    <p>关联是带来源的判断，不表示身份已核实，也不代表关系或亲密程度。</p>
    {memory.person_assertions.length?memory.person_assertions.map(item=><div className="event-row" key={item.entity_id}>
      <div>
        <strong>{item.label}</strong>{' '}<Badge>{item.role==='subject'?'事实主体':'仅被提及'}</Badge>{' '}
        <Badge tone={item.source_kind==='inference'?'warning':'neutral'}>{item.source_kind==='inference'?'模型推断':item.source_kind==='reported_claim'?'陈述注记':item.source_kind}</Badge>
        <details><summary>查看关联依据</summary><dl>
          <dt>人物引用 ID</dt><dd><code>{item.entity_id}</code></dd>
          <dt>来源事件</dt><dd><code>{item.source_event_id}</code></dd>
          <dt>关联事件</dt><dd><code>{item.assertion_event_id}</code></dd>
          <dt>记录者</dt><dd>{item.actor_id}</dd>
          <dt>记录时间</dt><dd>{when(item.occurred_at)}</dd>
        </dl></details>
      </div>
    </div>):<Empty>没有已保存的人物关联。提到名字不等于已完成关联。</Empty>}
    {memory.person_assertions_more&&<p role="status">仅显示前 20 项人物关联，此列表不完整。</p>}
  </section>;
}
