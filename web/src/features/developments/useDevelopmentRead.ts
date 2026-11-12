import {useEffect,useRef,useState} from 'react';
import {useSourceSnapshot} from '../sources/useSourceRead';
import {request,DevelopmentHttpError} from './client';
export function useDevelopmentRead<T>(path:string|null,s:ReturnType<typeof useSourceSnapshot>,decode:(data:unknown,etag:string|null)=>T,onUnavailable?:()=>void){
 const [nonce,setNonce]=useState(0),[result,setResult]=useState<{key:string;data:T|null;error:string;loading:boolean}>({key:'',data:null,error:'',loading:false});
 const latest=useRef({s,decode,onUnavailable});latest.current={s,decode,onUnavailable};const key=JSON.stringify([path,s.stamp,nonce]),allowed=!!path&&!!s.token&&!!s.individual&&s.connected;
 useEffect(()=>{const controller=new AbortController();let active=true;setResult({key,data:null,error:'',loading:allowed});if(allowed)void request(s.token,path!,controller.signal).then(response=>{const data=latest.current.decode(response.data,response.etag);if(active)setResult({key,data,error:'',loading:false});}).catch(error=>{if(!active)return;if(error instanceof DevelopmentHttpError&&[401,403].includes(error.status)&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.s.clearSession();return;}if(error instanceof DevelopmentHttpError&&error.status===404)latest.current.onUnavailable?.();setResult({key,data:null,loading:false,error:error instanceof DevelopmentHttpError?error.message:error instanceof TypeError||error instanceof DOMException?'连接读取未完成，请重试。':error instanceof Error?error.message:'读取失败。'});});return()=>{active=false;controller.abort();};},[key,allowed]);
 const current=allowed&&result.key===key;return {data:current?result.data:null,error:current?result.error:'',loading:allowed&&(!current||result.loading),reload:()=>setNonce(n=>n+1)};
}
