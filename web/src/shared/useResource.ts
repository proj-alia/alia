import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from './context';
import {ApiError} from './repository';
export function useResource<T>(path:string|null){
 const c=useIndividualContext(),latest=useRef(c);latest.current=c;
 const [nonce,setNonce]=useState(0),[result,setResult]=useState<{data:T|null;loading:boolean;error:string}>({data:null,loading:false,error:''});
 useEffect(()=>{
  let active=true;
  if(!path||!c.token||!c.state){setResult({data:null,loading:false,error:''});return;}
  setResult({data:null,loading:true,error:''});
  void latest.current.read<T>(path).then(data=>{if(active)setResult({data,loading:false,error:''});}).catch(error=>{
   if(!active)return;
   if(error instanceof ApiError&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.clearSession();return;}
   setResult({data:null,loading:false,error:error instanceof ApiError?error.message:'读取失败，请检查服务连接后重试。'});
  });
  return()=>{active=false;};
 },[path,c.token,c.state?.version,nonce]);
 return {...result,reload:()=>setNonce(n=>n+1)};
}
