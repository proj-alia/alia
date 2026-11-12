import {useEffect,useMemo,useReducer,useRef,useState,type ReactNode} from 'react';
import {Dialog,ResourceState} from '../../design-system/components';
import {useMemoryEditorGuard} from '../memory/EditorGuard';
import {readRecovery,rememberPending,rememberOperation,forgetPending} from './recovery';
import {PrivacyContext} from './PrivacyContext';
import {privacyRequest,PrivacyHttpError} from './client';
import {deletionBaseline,deletionReady,deletionTransition,emptyDeleteState,operation,privacyReadiness,receipt,uuid} from './stateMachine';
import {usePrivacyRead,usePrivacySession} from './usePrivacyRead';
import type {DeleteIntention} from './types';
import './privacy.css';
export function PrivacyProvider({route,children}:{route:string;children:ReactNode}){
 const session=usePrivacySession(),c=session.c,guard=useMemoryEditorGuard(),latest=useRef(session);latest.current=session;
 const [state,dispatch]=useReducer(deletionTransition,emptyDeleteState),stateRef=useRef(state);stateRef.current=state;
 const [target,setTarget]=useState<string|null>(null),[agreed,setAgreed]=useState(false),[hidden,setHidden]=useState(false),[known,setKnown]=useState<{individual:string;id:string}[]>([]);
 const [pendingRequests,setPendingRequests]=useState<DeleteIntention[]>([]),[recoveryNotice,setRecoveryNotice]=useState('');
 const serial=useRef(0),posting=useRef(false),refreshFence=useRef<string|null>(null),controller=useRef<AbortController|null>(null),live=useRef(true),previousRoute=useRef(route);
 const management=usePrivacyRead(route.split('/')[0]==='memory'?'/management':null,session,data=>privacyReadiness(data,session.individual!));
 const ready=deletionReady(management.data),baseline=usePrivacyRead(target?'/memories/'+encodeURIComponent(target):null,session,(data,etag)=>deletionBaseline(data,etag,session.individual!,target!));
 const owns=!state.intention||state.intention.individualId===session.individual;
 const operationId=owns?state.operationId:null,canQuery=!!session.individual&&session.connected&&management.data?.status_read_available===true;
 const progress=usePrivacyRead(operationId&&canQuery&&route.split('/')[0]==='memory'?'/privacy-operations/'+encodeURIComponent(operationId):null,session,data=>operation(data,session.individual!,operationId!));
 useEffect(()=>{if(!progress.data||progress.data.status==='completed'||!canQuery)return;const timer=setTimeout(progress.reload,2500);return()=>clearTimeout(timer);},[progress.data,progress.loading,progress.error,canQuery]);
 useEffect(()=>{if(!hidden||!progress.data||!session.connected||state.phase!=='tracking')return;const individual=progress.data.individual_id,version=progress.data.state_version;void latest.current.c.refresh().then(snapshot=>{if(live.current&&latest.current.connected&&latest.current.individual===individual&&snapshot&&snapshot.individual_id===individual&&snapshot.version>=version)setHidden(false);});},[hidden,progress.data,session.connected,state.phase]);
 useEffect(()=>{if(hidden&&state.phase==='rejected'&&session.connected&&c.lastSyncedAt&&c.lastSyncedAt!==refreshFence.current)setHidden(false);},[hidden,state.phase,session.connected,c.lastSyncedAt]);
 function syncRecovery(restore=false){const individual=latest.current.individual;if(!individual)return;try{const saved=readRecovery(individual);setPendingRequests(saved.pending);setKnown(saved.operations.map(id=>({individual,id})));if(restore&&stateRef.current.phase==='idle'&&saved.pending[0]){dispatch({type:'restore',intention:saved.pending[0]});setHidden(true);}else if(restore&&stateRef.current.phase==='idle'&&!stateRef.current.operationId&&saved.operations.length===1)dispatch({type:'recover',operationId:saved.operations[0]});}catch{setRecoveryNotice('本地恢复标识无法读取；请使用已保存的操作 ID 认证查询。');}}
 useEffect(()=>{if(!progress.data)return;try{rememberOperation(progress.data.individual_id,progress.data.operation_id);}catch{setRecoveryNotice('已认证的操作 ID 未能保存在此浏览器，请保留它以便重新查询。');}remember(progress.data.individual_id,progress.data.operation_id);},[progress.data]);
 function remember(individual:string,id:string){setKnown(items=>items.some(v=>v.individual===individual&&v.id===id)?items:[...items,{individual,id}]);}
 function invalidate(){serial.current++;controller.current?.abort();posting.current=false;setTarget(null);setAgreed(false);dispatch({type:'invalidate'});}
 useEffect(()=>{invalidate();},[session.identity]);
 useEffect(()=>{syncRecovery(true);const change=()=>syncRecovery();window.addEventListener('storage',change);return()=>window.removeEventListener('storage',change);},[c.token,session.individual]);
 useEffect(()=>{if(route!==previousRoute.current){previousRoute.current=route;invalidate();}},[route]);
 useEffect(()=>{live.current=true;return()=>{live.current=false;serial.current++;controller.current?.abort();};},[]);
 // A new snapshot hides the confirmation baseline and removes its acknowledgement.
 useEffect(()=>setAgreed(false),[target,session.stamp]);
 async function refreshPrivate(intention:DeleteIntention,minimumVersion?:number){
  const snapshot=await latest.current.c.refresh();if(live.current&&latest.current.individual===intention.individualId&&latest.current.connected&&snapshot&&snapshot.individual_id===intention.individualId&&(minimumVersion===undefined||snapshot.version>=minimumVersion))setHidden(false);
 }
 async function submit(existing?:DeleteIntention){
  if(posting.current||!ready||!latest.current.connected||!latest.current.c.token)return;
  const intention=existing??(agreed&&baseline.data?{...baseline.data,key:crypto.randomUUID()}:null);if(!intention||intention.individualId!==latest.current.individual)return;
  if(existing&&stateRef.current.phase!=='unknown')return;
  try{rememberPending(intention);syncRecovery();}catch{dispatch({type:'rejected',error:'无法保存本地恢复标识，删除请求未发送。'});setTarget(null);return;}
  refreshFence.current=latest.current.c.lastSyncedAt;posting.current=true;dispatch({type:'dispatch',intention});setHidden(true);setAgreed(false);
  const generation=serial.current,token=latest.current.c.token,abort=new AbortController();controller.current=abort;
  try{const response=await privacyRequest(token,'/memories/'+encodeURIComponent(intention.targetId),abort.signal,intention);
   if(!live.current||generation!==serial.current||token!==latest.current.c.token)return;
   if(response.status!==202)throw new Error('删除回执必须为 HTTP 202。');const value=receipt(response.data,intention.individualId,{targetId:intention.targetId});
   try{rememberOperation(value.individual_id,value.operation_id);forgetPending(intention);syncRecovery();}catch{setRecoveryNotice('已取得受理回执，但本地恢复索引更新失败；请保存操作 ID，重新连接后可认证查询。');}remember(value.individual_id,value.operation_id);dispatch({type:'receipt',receipt:value});setTarget(null);void refreshPrivate(intention,value.state_version);
  }catch(error){if(!live.current||generation!==serial.current||token!==latest.current.c.token)return;
   if(error instanceof PrivacyHttpError&&[401,403].includes(error.status)&&['unauthorized','cross_origin_denied'].includes(error.code)){dispatch({type:'unknown'});latest.current.c.clearSession();return;}
   if(error instanceof PrivacyHttpError&&[400,403,404,409,422].includes(error.status)){try{forgetPending(intention);syncRecovery();}catch{setRecoveryNotice('本地原请求标识未能清理，请保留其恢复信息。');}dispatch({type:'rejected',error:error.message});void refreshPrivate(intention);baseline.reload();return;}
   dispatch({type:'unknown'});setTarget(null);void latest.current.c.refresh();
  }finally{if(live.current&&generation===serial.current){posting.current=false;}}
 }
 function open(id:string){if(!uuid(id)||!ready||stateRef.current.phase==='unknown'||posting.current)return;guard.request(()=>{if(!latest.current.connected)return;dispatch({type:'confirm'});setTarget(id);setAgreed(false);baseline.reload();});}
 function restorePending(intention:DeleteIntention){if(posting.current||stateRef.current.phase==='unknown'||intention.individualId!==latest.current.individual)return;guard.request(()=>{if(posting.current||stateRef.current.phase==='unknown'||intention.individualId!==latest.current.individual)return;dispatch({type:'restore',intention});setHidden(true);});}
 function query(id:string){if(!uuid(id)||!canQuery||stateRef.current.phase==='unknown'||posting.current)return false;if(id===state.operationId)progress.reload();else dispatch({type:'recover',operationId:id});return true;}
 const exposedState=owns?state:{...emptyDeleteState};
 const value=useMemo(()=>({ready,readiness:management.data,readinessLoading:management.loading,readinessError:management.error,refreshReadiness:management.reload,open,hidden,state:exposedState,canReplay:ready&&owns&&state.phase==='unknown'&&!!state.intention,replay:()=>void submit(stateRef.current.intention??undefined),operationId,progress:progress.data,progressLoading:progress.loading,progressError:progress.error,query,refreshProgress:progress.reload,knownOperations:known.filter(v=>v.individual===session.individual).map(v=>v.id),canQuery,recoveryNotice,pendingRequests:pendingRequests.filter(v=>v.individualId===session.individual),restorePending}),[ready,management.data,management.loading,management.error,hidden,state,owns,operationId,progress.data,progress.loading,progress.error,known,session.stamp,agreed,baseline.data,recoveryNotice,pendingRequests]);
 return <PrivacyContext.Provider value={value}>{children}{c.token&&c.state&&target&&state.phase==='confirming'&&<Dialog title="确认删除记忆来源与派生内容" onClose={()=>{setTarget(null);dispatch({type:'close'});}}><p>删除从此记忆的实际来源开始，保守覆盖受影响的记忆、陈述及派生内容。这不是仅从列表移除此条记录。</p><p>无法撤回已经下载的副本，也不能保证撤回云端提供方已经收到的数据。</p><ResourceState loading={baseline.loading} error={baseline.error} retry={baseline.reload}/>{baseline.data&&<dl className="privacy-metadata"><dt>实际目标 ID</dt><dd><code>{baseline.data.targetId}</code></dd><dt>记忆对象版本 / If-Match</dt><dd>{baseline.data.memoryVersion} / <code>{baseline.data.etag}</code>（不是全局状态版本）</dd></dl>}<p>202 仅代表已阻断受理。只有实际进度返回 completed 才显示完成；失败可能保持待重试。</p><label className="privacy-ack"><input type="checkbox" checked={agreed} disabled={!baseline.data||!ready} onChange={e=>setAgreed(e.target.checked)}/>我理解删除来源及派生内容的范围与限制</label><div className="form-actions"><button autoFocus onClick={()=>{setTarget(null);dispatch({type:'close'});}}>取消删除</button><button className="privacy-danger" disabled={!ready||!baseline.data||!agreed} onClick={()=>void submit()}>确认删除来源及派生内容</button></div></Dialog>}</PrivacyContext.Provider>;
}
