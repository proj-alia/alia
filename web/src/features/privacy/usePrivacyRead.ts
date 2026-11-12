import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from '../../shared/context';
import {privacyRequest,PrivacyHttpError} from './client';
export function usePrivacySession(){
 const c=useIndividualContext(),[online,setOnline]=useState(navigator.onLine);
 useEffect(()=>{const update=()=>setOnline(navigator.onLine);window.addEventListener('online',update);window.addEventListener('offline',update);return()=>{window.removeEventListener('online',update);window.removeEventListener('offline',update);};},[]);
 const connected=online&&['服务已连接','实时同步中'].includes(c.connection),individual=c.state?.individual_id;
 return {c,connected,individual,identity:JSON.stringify([c.token,individual,connected]),stamp:JSON.stringify([c.token,individual,c.state?.version,c.lastSyncedAt,connected])};
}
export function usePrivacyRead<T>(path:string|null,session:ReturnType<typeof usePrivacySession>,decode:(data:unknown,etag:string|null)=>T){
 const [nonce,setNonce]=useState(0),[result,setResult]=useState<{key:string;data:T|null;error:string;loading:boolean}>({key:'',data:null,error:'',loading:false});
 const latest=useRef({session,decode});latest.current={session,decode};const key=JSON.stringify([path,session.stamp,nonce]),allowed=!!path&&!!session.c.token&&!!session.individual&&session.connected;
 useEffect(()=>{const controller=new AbortController();let active=true;setResult({key,data:null,error:'',loading:allowed});
  if(allowed)void privacyRequest(session.c.token,path!,controller.signal).then(response=>{const data=latest.current.decode(response.data,response.etag);if(active)setResult({key,data,error:'',loading:false});}).catch(error=>{
   if(!active)return;if(error instanceof PrivacyHttpError&&[401,403].includes(error.status)&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.session.c.clearSession();return;}
   setResult({key,data:null,error:error instanceof PrivacyHttpError?error.message:error instanceof TypeError||error instanceof DOMException?'读取未完成，请检查连接后重试。':error instanceof Error?error.message:'读取失败，请重新查询。',loading:false});
  });return()=>{active=false;controller.abort();};
 },[key,allowed]);
 const current=allowed&&result.key===key;
 return {data:current?result.data:null,error:current?result.error:!session.connected&&path?'当前未连接，私有状态已隐藏；进度需要重新认证读取。':'',loading:allowed&&(!current||result.loading),reload:()=>{setNonce(n=>n+1);if(!session.connected)void latest.current.session.c.refresh();}};
}
