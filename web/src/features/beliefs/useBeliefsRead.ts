import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from '../../shared/context';
import {ApiError} from '../../shared/repository';

/** Session/snapshot keyed data: old responses never repopulate the inspector. */
export function useBeliefsRead<T>(path:string|null){
 const c=useIndividualContext(),latest=useRef(c);latest.current=c;
 const [nonce,setNonce]=useState(0),[online,setOnline]=useState(navigator.onLine);
 const [result,setResult]=useState<{key:string;data:T|null;loading:boolean;error:string}>({key:'',data:null,loading:false,error:''});
 useEffect(()=>{const update=()=>setOnline(navigator.onLine);window.addEventListener('online',update);window.addEventListener('offline',update);return()=>{window.removeEventListener('online',update);window.removeEventListener('offline',update);};},[]);
 const connected=online&&['实时同步中','服务已连接'].includes(c.connection);
 const key=JSON.stringify([path,c.token,c.state?.individual_id,c.state?.version,c.lastSyncedAt,connected,nonce]);
 const allowed=!!path&&!!c.token&&!!c.state&&connected;
 useEffect(()=>{
  let active=true;
  if(!allowed){setResult({key,data:null,loading:false,error:path&&c.token&&!connected?'认识连接不可用，已清除页面中的认识内容。':''});return;}
  setResult({key,data:null,loading:true,error:''});
  void latest.current.read<T>(path!).then(data=>{if(active)setResult({key,data,loading:false,error:''});}).catch(error=>{
   if(!active)return;
   if(error instanceof ApiError&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.clearSession();return;}
   setResult({key,data:null,loading:false,error:error instanceof ApiError?error.message:'认识读取失败，请检查连接后重试。'});
  });
  return()=>{active=false;};
 },[key,allowed,path]);
 const current=result.key===key&&allowed;
 return {data:current?result.data:null,loading:allowed&&(!current||result.loading),error:!allowed&&path&&c.token&&!connected?'认识连接不可用，已清除页面中的认识内容。':current?result.error:'',reload:()=>setNonce(n=>n+1)};
}
