import {useState,type FormEvent} from 'react';
import {Badge,Confirm,Data,Panel} from '../design-system/components';
import {useIndividualContext} from '../shared/context';
import type {Validation} from '../shared/consoleTypes';
import type {State} from '../shared/types';
import runtimeSchema from '../../../schemas/runtime.schema.json';

const sections=[['autonomy','自主与预算'],['contact','主动联系'],['capabilities','能力与访问'],['memory','记忆与发展'],['json','完整 JSON']];
export function Config(){
 const c=useIndividualContext(),s=c.state;const [section,setSection]=useState('autonomy'),[confirm,setConfirm]=useState(false),[validating,setValidating]=useState(false),[feedback,setFeedback]=useState('');
 if(!s)return null;
 const editing=c.budgetVersion!==null;let doc:State['runtime']|null=null;
 try{doc=JSON.parse(editing?c.budget:JSON.stringify(s.runtime));}catch{/* Raw draft is preserved for strict validation. */}
 const current=doc;
 function change(group:string,key:string,value:unknown){
  if(!current||!editing)return;
  const next=structuredClone(current) as unknown as Record<string,Record<string,unknown>>;
  next[group][key]=value;c.setBudget(JSON.stringify(next,null,2));setFeedback('');
 }
 async function validate(){
  setValidating(true);setFeedback('');
  try{const r=await c.read<Validation>('/sandbox/validate','POST',{kind:'runtime',document_text:editing?c.budget:JSON.stringify(s!.runtime)});setFeedback(r.valid?'服务端校验通过，尚未保存。':'配置未通过服务端校验，请检查 JSON、字段及能力范围。');return r.valid;}
  catch{setFeedback('未能完成服务端校验，请检查连接和授权后重试。');return false;}finally{setValidating(false);}
 }
 async function pick(next:string){if(next===section)return;if(section==='json'&&editing&&!(await validate()))return;setSection(next);}
 function number(label:string,group:string,key:string,value:number|null){const schema=runtimeSchema.properties as unknown as Record<string,{properties:Record<string,{minimum:number;maximum:number}>}>;const {minimum:min,maximum:max}=schema[group].properties[key];return <div className="config-field" key={key}><div><label htmlFor={'cfg-'+key}>{label}</label><small>范围 {min}—{max}，最终由服务端验证</small></div><input id={'cfg-'+key} type="number" value={value??''} min={min} max={max} step={1} disabled={!editing||c.busy} onChange={e=>{if(e.target.value==='')change(group,key,null);else if(Number.isInteger(e.target.valueAsNumber))change(group,key,e.target.valueAsNumber);}}/></div>;}
 return <>
  <div className="config-toolbar"><label>配置档案<select aria-label="配置档案" value="current" disabled><option value="current">当前运行配置</option></select></label><Badge>配置版本 {s.config_version}</Badge><small>仅一个已持久化配置；多档案切换尚未接入。</small></div>
  <div className="config-workspace"><nav className="section-directory" aria-label="配置分区">{sections.map(([id,label])=><button key={id} aria-pressed={section===id} disabled={validating} onClick={()=>void pick(id)}>{label}</button>)}</nav><Panel title={sections.find(([id])=>id===section)![1]} subtitle={editing?`正在编辑 · 状态基线 ${c.budgetVersion}`:'服务端当前生效规则'} action={!editing?<button disabled={c.busy} onClick={()=>{c.beginSettingsEdit();setFeedback('');}}>编辑运行配置</button>:<Badge tone={c.settingsDirty?'warning':'neutral'}>{c.settingsDirty?'未保存修改':'未修改'}</Badge>}>
   {section==='json'?<><label htmlFor="rules">完整运行配置</label><textarea id="rules" className="code-editor" rows={22} value={editing?c.budget:JSON.stringify(s.runtime,null,2)} readOnly={!editing} disabled={c.busy} onChange={e=>{c.setBudget(e.target.value);setFeedback('');}}/><p>原文提交给严格解析器，重复字段不会被客户端整理后掩盖。切回分区表单前先进行服务端校验。</p></>:!current?<p role="alert" className="resource-error">草稿不是有效 JSON。切换到完整 JSON 修正后重试。</p>:section==='autonomy'?<>
    <div className="config-field"><div><label htmlFor="cfg-enabled">自主活动配置</label><small>与临时暂停分开；受预算、能力和生命周期共同约束</small></div><select id="cfg-enabled" disabled={!editing||c.busy} value={String(current.autonomy.enabled)} onChange={e=>change('autonomy','enabled',e.target.value==='true')}><option value="true">启用</option><option value="false">停用</option></select></div>
    {number('每循环模型调用上限','autonomy','max_model_calls_per_cycle',current.autonomy.max_model_calls_per_cycle)}
    {number('每循环工具调用上限','autonomy','max_tool_calls_per_cycle',current.autonomy.max_tool_calls_per_cycle)}
    {number('循环超时（秒）','autonomy','cycle_timeout_seconds',current.autonomy.cycle_timeout_seconds)}
    {number('空闲检查间隔（分钟）','autonomy','idle_check_minutes',current.autonomy.idle_check_minutes)}
    {number('后台每日 token 上限','budgets','background_tokens_per_day',current.budgets.background_tokens_per_day)}
    {number('全部每日 token 上限','budgets','total_tokens_per_day',current.budgets.total_tokens_per_day)}
   </>:section==='contact'?<>
    {number('每日主动联系次数','contact','max_proactive_per_day',current.contact.max_proactive_per_day)}
    {number('联系间隔（小时）','contact','minimum_interval_hours',current.contact.minimum_interval_hours)}
    <div className="config-field"><div><label htmlFor="cfg-timezone">实例时区</label><small>IANA 时区；包含夏令时规则</small></div><input id="cfg-timezone" disabled={!editing||c.busy} value={current.contact.timezone} onChange={e=>change('contact','timezone',e.target.value)}/></div>
    {(['quiet_start','quiet_end'] as const).map(key=><div className="config-field" key={key}><label htmlFor={'cfg-'+key}>{key==='quiet_start'?'安静时段开始':'安静时段结束'}</label><input id={'cfg-'+key} type="time" value={current.contact[key]} disabled={!editing||c.busy} onChange={e=>change('contact',key,e.target.value)}/></div>)}
   </>:section==='capabilities'?<>
    <p>变更能力授权会影响下一轮执行，提交前请检查范围。</p>
    {s.capabilities.map(cap=><div className="config-field" key={cap.id}><div><label htmlFor={'cap-'+cap.id}>{cap.id}</label><small>{cap.available?'已注册能力':'当前不可用'}</small></div><input className="checkbox" id={'cap-'+cap.id} type="checkbox" disabled={!editing||c.busy||!cap.available} checked={current.capabilities.enabled.includes(cap.id)} onChange={e=>change('capabilities','enabled',e.target.checked?[...current.capabilities.enabled,cap.id]:current.capabilities.enabled.filter(id=>id!==cap.id))}/></div>)}
    <label htmlFor="cfg-hosts">允许读取的网络主机（每行一个）</label><textarea id="cfg-hosts" rows={4} disabled={!editing||c.busy} value={current.capabilities.allowed_web_hosts.join('\n')} onChange={e=>change('capabilities','allowed_web_hosts',e.target.value===''?[]:e.target.value.split('\n'))}/><p>空列表不开放任意网络访问。主机和能力仍需服务端执行策略验证。</p>
   </>:<><h3>记忆规则</h3><Data value={current.memory}/><h3>发展规则</h3><Data value={current.development}/><p>当前规则由版本化运行契约定义。人格引用指向同一个体的持久人格，配置页不复制人格 Prompt。</p><a href="#/identity">检查人格与版本 →</a></>}
   {feedback&&<p role="status" className="resource-feedback">{feedback}</p>}
   <div className="savebar"><small>{editing?`编辑基线 ${c.budgetVersion}，当前状态 ${s.version}`:'修改模板不会重置个体'}</small><div className="actions"><button disabled={validating||c.busy} onClick={()=>void validate()}>{validating?'校验中…':'验证配置'}</button>{editing&&<><button disabled={c.busy} onClick={()=>{c.discardSettingsEdit();setFeedback('');}}>取消编辑</button><button className="primary" disabled={!c.settingsDirty||c.busy||validating} onClick={()=>{c.setError('');setConfirm(true);}}>保存配置</button></>}</div></div>
  </Panel></div>
  {confirm&&<Confirm title="确认运行规则变更" busy={c.busy} error={c.error} onClose={()=>setConfirm(false)} action={()=>c.saveSettings({preventDefault(){}} as FormEvent)}><p>提交基线 {c.budgetVersion}；变更可能影响预算、联系和能力授权。服务端验证成功后才更新状态。</p><div className="two-columns"><div><h3>编辑前</h3><pre>{c.budgetOriginal}</pre></div><div><h3>拟提交</h3><pre>{c.budget}</pre></div></div></Confirm>}
 </>;
}
