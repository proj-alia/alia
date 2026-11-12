import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from '../../shared/context';
export function useSourceSnapshot(){
 const c=useIndividualContext(),[online,setOnline]=useState(navigator.onLine);
 useEffect(()=>{const update=()=>setOnline(navigator.onLine);window.addEventListener('online',update);window.addEventListener('offline',update);return()=>{window.removeEventListener('online',update);window.removeEventListener('offline',update);};},[]);
 const connected=online&&['实时同步中','服务已连接'].includes(c.connection),individual=c.state?.individual_id;
 // State version plus the completed SSE snapshot covers privacy epoch changes;
 // the public /state projection does not expose privacy_epoch separately.
 const identity=JSON.stringify([c.token,individual,c.state?.version,connected]);
 return {token:c.token,individual,connected,identity,stamp:JSON.stringify([identity,c.lastSyncedAt]),clearSession:c.clearSession,refresh:c.refresh};
}
type Snapshot=ReturnType<typeof useSourceSnapshot>;
export class SourceReadError extends Error{constructor(public status:number,public code:string){super(status===404?'记录或来源当前不可用，旧内容已清除。':'读取未完成，请检查连接后重试。');}}
/** Abort and a render-time key fence suppress data from old selections/snapshots. */
export function useSourceRead<T>(path:string|null,snapshot:Snapshot,onUnavailable?:()=>void){
 const [nonce,setNonce]=useState(0),[result,setResult]=useState<{key:string;data:T|null;loading:boolean;error:string}>({key:'',data:null,loading:false,error:''});
 const latest=useRef({snapshot,onUnavailable});latest.current={snapshot,onUnavailable};
 const key=JSON.stringify([path,snapshot.stamp,nonce]),allowed=!!path&&!!snapshot.token&&!!snapshot.individual&&snapshot.connected;
 useEffect(()=>{
  const controller=new AbortController();let active=true;
  setResult({key,data:null,loading:allowed,error:''});
  if(allowed)void fetch('/api/v1'+path,{headers:{Authorization:'Bearer '+snapshot.token},cache:'no-store',signal:AbortSignal.any([controller.signal,AbortSignal.timeout(15000)])}).then(async response=>{
   const value=await response.json();if(!response.ok)throw new SourceReadError(response.status,value.error?.code??'request_failed');return value as T;
  }).then(data=>{if(active)setResult({key,data,loading:false,error:''});}).catch(error=>{
   if(!active)return;
   if(error instanceof SourceReadError&&[401,403].includes(error.status)&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.snapshot.clearSession();return;}
   if(error instanceof SourceReadError&&error.status===404)latest.current.onUnavailable?.();
   setResult({key,data:null,loading:false,error:error instanceof SourceReadError?error.message:'读取未完成，请检查连接后重试。'});
  });
  return()=>{active=false;controller.abort();};
 },[key,allowed]);
 const current=result.key===key&&allowed;
 return {data:current?result.data:null,loading:allowed&&(!current||result.loading),error:!allowed&&path&&snapshot.token&&!snapshot.connected?'连接不可用，来源与关联正文已隐藏；恢复后重新验证。':current?result.error:'',reload:()=>{setNonce(n=>n+1);if(!snapshot.connected)void latest.current.snapshot.refresh();}};
}
