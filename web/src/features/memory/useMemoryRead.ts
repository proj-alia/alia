import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from '../../shared/context';
import {memoryRequest,MemoryHttpError,type MemoryResponse} from './client';
import {memoryEditorStamp} from './EditorGuard';
export function useMemoryRead<T>(path:string|null,onUnavailable?:()=>void){
 const c=useIndividualContext(),latest=useRef({c,onUnavailable});latest.current={c,onUnavailable};
 const [nonce,setNonce]=useState(0),[result,setResult]=useState<{key:string;value:MemoryResponse<T>|null;loading:boolean;error:string}>({key:'',value:null,loading:false,error:''});
 const stamp=memoryEditorStamp(c),key=JSON.stringify([path,stamp,nonce]);const connected=['实时同步中','服务已连接'].includes(c.connection);
 useEffect(()=>{
  const controller=new AbortController();let active=true;
  setResult({key,value:null,loading:!!path&&connected,error:path&&!connected?'连接中断，私人记录已隐藏；恢复后重新读取。':''});
  if(path&&connected&&c.token&&c.state)void memoryRequest<T>(c.token,path,controller.signal).then(value=>{if(active)setResult({key,value,loading:false,error:''});}).catch(error=>{
   if(!active)return;
   if(error instanceof MemoryHttpError&&[401,403].includes(error.status)&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.c.clearSession();return;}
   if(error instanceof MemoryHttpError&&error.status===404)latest.current.onUnavailable?.();
   setResult({key,value:null,loading:false,error:error instanceof Error?error.message:'读取失败，请重试。'});
  });
  return()=>{active=false;controller.abort();};
 },[key,c.token]);
 const current=result.key===key&&connected;
 return {data:current?result.value?.data??null:null,etag:current?result.value?.etag??null:null,loading:!!path&&connected&&(!current||result.loading),error:result.key===key?result.error:'',reload:()=>{setNonce(n=>n+1);if(!connected)void latest.current.c.refresh();},stamp};
}
