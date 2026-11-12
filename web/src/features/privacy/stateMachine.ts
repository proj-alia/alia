import type {DeleteBaseline,DeleteEvent,DeleteIntention,DeleteReceipt,DeleteState,PrivacyOperation,PrivacyReadiness} from './types';
export const uuid=(v:unknown):v is string=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v)&&v!=='00000000-0000-0000-0000-000000000000';
const integer=(v:unknown,min:number)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
export const emptyDeleteState:DeleteState={phase:'idle',intention:null,operationId:null,receipt:null,error:''};
export function privacyReadiness(value:unknown,individual:string):PrivacyReadiness|null{
 if(!value||typeof value!=='object')return null;const v=value as Record<string,unknown>,p=v.privacy as Record<string,unknown>|undefined;
 return v.individual_id===individual&&!!p&&typeof p==='object'&&['deletion_enabled','journal_ready','status_read_available'].every(k=>typeof p[k]==='boolean')?{deletion_enabled:p.deletion_enabled as boolean,journal_ready:p.journal_ready as boolean,status_read_available:p.status_read_available as boolean}:null;
}
export const deletionReady=(v:PrivacyReadiness|null)=>!!v&&v.deletion_enabled===true&&v.journal_ready===true&&v.status_read_available===true;
export function deletionBaseline(value:unknown,etag:string|null,individual:string,target:string):DeleteBaseline{
 const v=value as {individual_id?:unknown;memory?:{id?:unknown;version?:unknown}}|null;const version=v?.memory?.version;
 if(v?.individual_id!==individual||v?.memory?.id!==target||!uuid(target)||!integer(version,1)||etag!==`"${version}"`)throw new Error('未取得有效的目标记忆强 ETag，删除尚未提交。');
 return {individualId:individual,targetId:target,memoryVersion:version as number,etag};
}
export function receipt(value:unknown,individual:string,expected?:{operationId?:string;targetId?:string}):DeleteReceipt{
 const v=value as DeleteReceipt|null;
 if(!v||v.individual_id!==individual||!uuid(v.operation_id)||!uuid(v.target_id)||!integer(v.state_version,1)||!integer(v.privacy_epoch,1)||!['blocked','purging','retry_pending','completed'].includes(v.status)||expected?.operationId&&v.operation_id!==expected.operationId||expected?.targetId&&v.target_id!==expected.targetId)throw new Error('删除回执未能验证，不能据此报告受理或完成。');
 return {individual_id:v.individual_id,operation_id:v.operation_id,target_id:v.target_id,state_version:v.state_version,privacy_epoch:v.privacy_epoch,status:v.status};
}
export function operation(value:unknown,individual:string,id:string):PrivacyOperation{
 const base=receipt(value,individual,{operationId:id}),v=value as PrivacyOperation;
 if(base.status==='completed'&&v.last_error!==null||typeof v.observed_at!=='string'||!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(v.observed_at)||!Number.isFinite(Date.parse(v.observed_at))||!(v.last_error===null||typeof v.last_error==='string'&&/^[a-z][a-z0-9_]{0,79}$/.test(v.last_error)))throw new Error('进度响应未能验证，仍需重新查询。');
 return {...base,observed_at:v.observed_at,last_error:v.last_error};
}
export function sameIntention(a:DeleteIntention,b:DeleteIntention){return a.key===b.key&&a.individualId===b.individualId&&a.targetId===b.targetId&&a.memoryVersion===b.memoryVersion&&a.etag===b.etag;}
export function deletionTransition(state:DeleteState,event:DeleteEvent):DeleteState{
 switch(event.type){
  case'restore':return {...emptyDeleteState,phase:'unknown',intention:event.intention};
  case'confirm':return state.phase==='unknown'||state.phase==='submitting'?state:{...state,phase:'confirming',intention:null,operationId:null,receipt:null,error:''};
  case'dispatch':if(state.intention&&(state.phase==='unknown'||state.phase==='submitting')&&!sameIntention(state.intention,event.intention))throw new Error('未知删除意图不能被替换。');return {...state,phase:'submitting',intention:event.intention,receipt:null,error:''};
  case'unknown':return {...state,phase:'unknown',receipt:null,error:'请求结果未知；只能显式重放原 key、目标与版本，或查询已知操作 ID。'};
  case'rejected':return {...state,phase:'rejected',receipt:null,error:event.error};
  case'receipt':if(state.intention&&(event.receipt.individual_id!==state.intention.individualId||event.receipt.target_id!==state.intention.targetId))throw new Error('删除回执与当前意图不一致。');if(state.operationId&&state.receipt&&event.receipt.operation_id!==state.operationId)throw new Error('进度不能切换到另一操作。');return {...state,phase:'tracking',operationId:event.receipt.operation_id,receipt:event.receipt,error:''};
  case'recover':return {...emptyDeleteState,phase:'tracking',operationId:event.operationId};
  // Keep only the opaque, body-free intention/operation reference across a session
  // boundary. Authorization and fresh reads are still required before showing it.
  case'invalidate':return {...state,phase:state.phase==='submitting'?'unknown':state.phase==='confirming'?'idle':state.phase,receipt:null,error:''};
  case'close':return state.phase==='confirming'?{...state,phase:'idle'}:state;
 }
}
