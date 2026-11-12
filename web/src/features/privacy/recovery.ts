import {sameIntention,uuid} from './stateMachine';
import type {DeleteIntention} from './types';
const pendingPrefix=(individual:string)=>'alia.privacy.pending.v1:'+individual+':';
const operationPrefix=(individual:string)=>'alia.privacy.operation.v1:'+individual+':';
const keys=(value:object,expected:string[])=>Object.keys(value).sort().join('|')===expected.sort().join('|');
export function pendingRecord(value:unknown,individual:string):DeleteIntention{
 const v=value as DeleteIntention|null;
 if(!v||typeof v!=='object'||!keys(v,['individualId','targetId','memoryVersion','etag','key'])||v.individualId!==individual||!uuid(individual)||!uuid(v.targetId)||!uuid(v.key)||!Number.isSafeInteger(v.memoryVersion)||v.memoryVersion<1||v.etag!==`"${v.memoryVersion}"`)throw new Error('本地恢复标识未能验证；请用已保存操作 ID 认证查询。');
 return {individualId:v.individualId,targetId:v.targetId,memoryVersion:v.memoryVersion,etag:v.etag,key:v.key};
}
function parse(text:string|null):unknown{if(!text||text.length>2048)throw new Error('本地恢复标识未能验证。');try{return JSON.parse(text);}catch{throw new Error('本地恢复标识未能验证。');}}
/** Each command/operation has an immutable key: concurrent tabs cannot overwrite
 * another intention. No token, body, draft, error text or progress is cached. */
export function readRecovery(individual:string,store:Storage=localStorage){
 const pending:DeleteIntention[]=[],operations:string[]=[];if(!uuid(individual))return {pending,operations};
 for(let i=0;i<store.length;i++){
  const key=store.key(i);if(!key)continue;
  if(key.startsWith(pendingPrefix(individual))){const record=pendingRecord(parse(store.getItem(key)),individual);if(key!==pendingPrefix(individual)+record.key)throw new Error('本地恢复标识未能验证。');pending.push(record);}
  if(key.startsWith(operationPrefix(individual))){const value=parse(store.getItem(key)) as {individualId:string;operationId:string};if(!value||typeof value!=='object'||!keys(value,['individualId','operationId'])||value.individualId!==individual||!uuid(value.operationId)||key!==operationPrefix(individual)+value.operationId)throw new Error('本地操作标识未能验证。');operations.push(value.operationId);}
  if(pending.length+operations.length>1000)throw new Error('本地恢复索引超过读取上限，请使用已保存的操作 ID。');
 }
 return {pending:pending.sort((a,b)=>a.key.localeCompare(b.key)),operations:operations.sort()};
}
export function rememberPending(intention:DeleteIntention,store:Storage=localStorage){
 const value=pendingRecord(intention,intention.individualId),key=pendingPrefix(value.individualId)+value.key,existing=store.getItem(key);
 if(existing&&!sameIntention(pendingRecord(parse(existing),value.individualId),value))throw new Error('本地原删除意图冲突，未发送新请求。');
 store.setItem(key,JSON.stringify(value));if(store.getItem(key)!==JSON.stringify(value))throw new Error('无法保存恢复标识，删除请求未发送。');
}
export function rememberOperation(individual:string,id:string,store:Storage=localStorage){if(!uuid(individual)||!uuid(id))throw new Error('操作标识无效。');store.setItem(operationPrefix(individual)+id,JSON.stringify({individualId:individual,operationId:id}));}
export function forgetPending(intention:DeleteIntention,store:Storage=localStorage){const key=pendingPrefix(intention.individualId)+intention.key,value=store.getItem(key);if(value&&!sameIntention(pendingRecord(parse(value),intention.individualId),intention))throw new Error('本地原删除意图冲突。');store.removeItem(key);}
