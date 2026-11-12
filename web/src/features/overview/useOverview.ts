import {useEffect,useRef,useState} from 'react';
import {useIndividualContext} from '../../shared/context';
import {ApiError} from '../../shared/repository';
import type {OverviewSnapshot} from './types';

export function useOverview(){
 const c=useIndividualContext(),latest=useRef(c);latest.current=c;
 const connected=['服务已连接','实时同步中'].includes(c.connection);
 const [nonce,setNonce]=useState(0);
 const key=JSON.stringify([c.state?.individual_id,c.state?.version,c.lastSyncedAt,connected,nonce]);
 const [result,setResult]=useState<{key:string;data:OverviewSnapshot|null;loading:boolean;error:string}>({key:'',data:null,loading:false,error:''});
 const pending=useRef<Promise<OverviewSnapshot>|null>(null);
 useEffect(()=>{
  let active=true;
  setResult({key,data:null,loading:!!c.token&&!!c.state&&connected,error:connected?'':'概览连接中断，统计已清空；恢复连接后重新读取。'});
  if(!c.token||!c.state||!connected)return;
  async function load(){
   try{
    // Serialize across SSE/state invalidations, without publishing an old response.
    if(pending.current)await pending.current.catch(()=>{});
    if(!active)return;
    const read=latest.current.read<OverviewSnapshot>('/overview');pending.current=read;
    let data:OverviewSnapshot;try{data=await read;}finally{if(pending.current===read)pending.current=null;}
    if(!active)return;
    const state=latest.current.state;
    if(!state||data.individual_id!==state.individual_id||data.state_version!==state.version)throw new Error('概览快照已变化，请重新读取当前状态。');
    setResult({key,data,loading:false,error:''});
   }catch(error){
    if(!active)return;
    if(error instanceof ApiError&&['unauthorized','cross_origin_denied'].includes(error.code)){latest.current.clearSession();return;}
    setResult({key,data:null,loading:false,error:error instanceof Error?error.message:'概览读取失败，请重试。'});
   }
  }
  void load();return()=>{active=false;};
 },[key,c.token]);
 const current=result.key===key;
 return {data:current&&connected?result.data:null,loading:connected&&(!current||result.loading),error:current?result.error:'',reload:()=>{setNonce(n=>n+1);if(!connected)void latest.current.refresh();}};
}
