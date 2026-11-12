import type {Development,DevelopmentDetail,DevelopmentList,DevelopmentStatus,Json,Readiness,RevertIntention,RevertReceipt,Scope,Target} from './types';
export const uuid=(x:unknown):x is string=>typeof x==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(x)&&x!=='00000000-0000-0000-0000-000000000000';
const object=(x:unknown):x is Record<string,unknown>=>!!x&&typeof x==='object'&&!Array.isArray(x);
const keys=(x:unknown,k:string[]):x is Record<string,unknown>=>object(x)&&Object.keys(x).length===k.length&&k.every(key=>Object.hasOwn(x,key));
const int=(x:unknown):x is number=>typeof x==='number'&&Number.isSafeInteger(x)&&x>0;
const text=(x:unknown):x is string=>typeof x==='string'&&!x.includes('\0');
const time=(x:unknown):x is string=>typeof x==='string'&&/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(x)&&Number.isFinite(Date.parse(x));
const ids=(x:unknown,min=0,max=1024):x is string[]=>Array.isArray(x)&&x.length>=min&&x.length<=max&&x.every(uuid)&&new Set(x).size===x.length;
const nullableId=(x:unknown)=>x===null||uuid(x);
const status=['proposed','trial','accepted','rejected','expired','reverted'];
const recorded=['Proposed','Trial','Accepted','Rejected','Expired','Reverted'];
const outcomes=['Passed','Failed','Unverified'];
export const statuses=status as DevelopmentStatus[];
export const targets=['persona.traits','persona.values','persona.expression','persona.cognitive_preferences','task_strategy'] as Target[];
function json(x:unknown,depth=0):x is Json{return depth<=30&&(x===null||typeof x==='boolean'||typeof x==='string'||typeof x==='number'&&Number.isFinite(x)||Array.isArray(x)&&x.every(v=>json(v,depth+1))||object(x)&&Object.values(x).every(v=>json(v,depth+1)));}
const scope=(x:unknown):x is Scope=>keys(x,['kind','id'])&&['goal','task'].includes(x.kind as string)&&uuid(x.id);
function value(target:Target,x:unknown,s:Scope|null,before=false):boolean{
 if(!json(x))return false;
 if(target==='task_strategy')return before&&x===null||keys(x,['scope','description'])&&scope(x.scope)&&!!s&&x.scope.kind===s.kind&&x.scope.id===s.id&&text(x.description)&&!!x.description.trim();
 if(s!==null)return false;
 if(target==='persona.traits')return keys(x,['curiosity','caution','sociability','assertiveness','openness_to_revision'])&&Object.values(x).every(v=>typeof v==='number'&&Number.isFinite(v)&&v>=0&&v<=1);
 if(target==='persona.values')return Array.isArray(x)&&x.every(v=>keys(v,['id','description','priority'])&&text(v.id)&&text(v.description)&&typeof v.priority==='number'&&Number.isFinite(v.priority)&&v.priority>=0&&v.priority<=1);
 if(target==='persona.cognitive_preferences')return Array.isArray(x)&&x.every(v=>text(v)&&!!v.trim());
 return keys(x,['language','tone','verbosity','humor','disagreement'])&&Object.values(x).every(text);
}
function transition(x:unknown):boolean{return keys(x,['from','to','occurred_at','before_value','after_value','reason','review_ids','revision_id'])&&(x.from===null||recorded.includes(x.from as string))&&recorded.includes(x.to as string)&&time(x.occurred_at)&&json(x.before_value)&&json(x.after_value)&&text(x.reason)&&ids(x.review_ids)&&nullableId(x.revision_id);}
function validation(x:unknown,individual:string):boolean{
 if(!keys(x,['assessment','sources','recorded_at'])||!time(x.recorded_at)||!Array.isArray(x.sources)||!x.sources.every(s=>keys(s,['id','individual_id','class','recorded_at'])&&uuid(s.id)&&s.individual_id===individual&&['ObservedOutcome','ExternalReport','SignificantEvent','ModelSelfReport','TransientAffect','InitialSetting'].includes(s.class as string)&&time(s.recorded_at)))return false;
 const a=x.assessment;
 return keys(a,['review_id','reviewer_id','origin','reviewed_evidence_ids','reviewed_counterevidence_ids','counter_search_event_ids','counter_review','observations'])&&uuid(a.review_id)&&uuid(a.reviewer_id)&&['IndependentHuman','IndependentMeasurement','AuthorSelfAssessment'].includes(a.origin as string)&&ids(a.reviewed_evidence_ids)&&ids(a.reviewed_counterevidence_ids)&&ids(a.counter_search_event_ids)&&outcomes.includes(a.counter_review as string)&&Array.isArray(a.observations)&&a.observations.every(o=>keys(o,['case_id','context','source_event_ids','result_event_id','outcome'])&&uuid(o.case_id)&&text(o.context)&&ids(o.source_event_ids)&&uuid(o.result_event_id)&&outcomes.includes(o.outcome as string));
}
const itemKeys=['id','version','status','target_id','target','scope','proposer_id','prepared_at','before_version','before_head','before_value','proposed_value','proposed_description','evidence_ids','counterevidence_ids','validation_plan','deadline','current_target_version','current_target_head'];
function item(x:unknown,individual:string,history:boolean):x is Development{
 if(!keys(x,[...itemKeys,...(history?['transitions','validations']:[])])||!uuid(x.id)||!int(x.version)||!status.includes(x.status as string)||!uuid(x.target_id)||!targets.includes(x.target as Target)||!uuid(x.proposer_id)||!time(x.prepared_at)||!int(x.before_version)||!nullableId(x.before_head)||!nullableId(x.current_target_head)||!int(x.current_target_version)||!text(x.proposed_description)||!text(x.validation_plan)||!time(x.deadline)||!ids(x.evidence_ids,1,32)||!ids(x.counterevidence_ids,0,32))return false;
 const s=x.scope;if(!(s===null||scope(s))||x.target==='task_strategy'&&!s||!value(x.target as Target,x.before_value,s,true)||!value(x.target as Target,x.proposed_value,s))return false;
 return !history||Array.isArray(x.transitions)&&x.transitions.length>0&&x.transitions.every(t=>transition(t)&&value(x.target as Target,(t as Record<string,unknown>).before_value,s,true)&&value(x.target as Target,(t as Record<string,unknown>).after_value,s,true))&&((x.transitions.at(-1) as Record<string,unknown>).to as string).toLowerCase()===x.status&&Array.isArray(x.validations)&&x.validations.every(v=>validation(v,individual));
}
const meta=(x:Record<string,unknown>,individual:string)=>x.individual_id===individual&&uuid(individual)&&int(x.state_version)&&typeof x.privacy_epoch==='number'&&Number.isSafeInteger(x.privacy_epoch)&&x.privacy_epoch>=0&&time(x.observed_at);
export function readiness(x:unknown,individual:string):Readiness|null{return object(x)&&x.individual_id===individual&&keys(x.development,['read_enabled','revert_enabled'])&&typeof x.development.read_enabled==='boolean'&&typeof x.development.revert_enabled==='boolean'?x.development as Readiness:null;}
export function list(x:unknown,individual:string,history:boolean,limit:number):DevelopmentList{
 if(!keys(x,['individual_id','state_version','privacy_epoch','observed_at','include_history','items','has_more'])||!meta(x,individual)||x.include_history!==history||!Array.isArray(x.items)||x.items.length>limit||!x.items.every(v=>item(v,individual,history))||new Set(x.items.map(v=>(v as Development).id)).size!==x.items.length||typeof x.has_more!=='boolean')throw new Error('发展目录未通过当前个体与记录合同校验，未显示内容。');return x as DevelopmentList;
}
export function detail(x:unknown,individual:string,id:string,etag:string|null=null):DevelopmentDetail{
 if(!keys(x,['individual_id','state_version','privacy_epoch','observed_at','development'])||!meta(x,individual)||!item(x.development,individual,true)||x.development.id!==id||etag!==null&&etag!==`"${x.development.version}"`)throw new Error('发展详情未通过当前目标、版本或历史合同校验，未显示内容。');return x as DevelopmentDetail;
}
export function canRevert(d:Development|undefined):boolean{const accepted=[...(d?.transitions??[])].reverse().find(t=>t.to==='Accepted');return !!d&&d.status==='accepted'&&uuid(accepted?.revision_id)&&accepted.revision_id===d.current_target_head;}
export const validReason=(x:string)=>!!x.trim()&&!x.includes('\0')&&Array.from(x).length<=1000;
export function receipt(x:unknown,i:RevertIntention):RevertReceipt{
 if(!keys(x,['individual_id','development_id','event_id','state_version','development_version','status','revision_id'])||x.individual_id!==i.individualId||x.development_id!==i.developmentId||!uuid(x.event_id)||!uuid(x.revision_id)||x.revision_id===i.acceptedHead||x.revision_id===i.developmentId||x.event_id===i.developmentId||!int(x.state_version)||x.development_version!==i.version+1||x.status!=='reverted')throw new Error('撤回回执未能验证，仍按结果未知处理。');return x as RevertReceipt;
}
