import {useEffect,useRef,useState,type FormEvent} from 'react';
import personaSeed from '../../../config/persona.example.json';
import runtimeSeed from '../../../config/runtime.example.json';
import type {Message,Memory,Goal,Page,State,Operation,GoalDraft,GoalAction,GoalReceipt} from './types';
import {watchEvents} from '../events';
import {ApiError,createRepository,RawJsonDocument} from './repository';
export function useIndividual() {
  const [token,setToken]=useState(''); const [credential,setCredential]=useState('');
  const [state,setState]=useState<State|null>(null); const [needsInit,setNeedsInit]=useState(false); const [name,setName]=useState('Alia');
  const [messages,setMessages]=useState<Page<Message>>({items:[],next_cursor:null});
  const [memories,setMemories]=useState<Page<Memory>>({items:[],next_cursor:null}); const [goals,setGoals]=useState<Page<Goal>>({items:[],next_cursor:null});
  const [receipt,setReceipt]=useState<{version:number;individual_id:string;event_id?:string;goal_id?:string}|null>(null);
  const [error,setError]=useState(''); const [busy,setBusy]=useState(false); const [connection,setConnection]=useState('尚未连接');
  const [lastSyncedAt,setLastSyncedAt]=useState<string|null>(null);
  const [draft,setDraft]=useState(''); const [operation,setOperation]=useState<Operation|null>(null); const [search,setSearch]=useState('');
  const [docTitle,setDocTitle]=useState(''); const [docText,setDocText]=useState(''); const [showImport,setShowImport]=useState(false);
  const [budget,setBudget]=useState(''); const [budgetOriginal,setBudgetOriginal]=useState(''); const [budgetVersion,setBudgetVersion]=useState<number|null>(null);
  function beginSettingsEdit(){if(!state)return;const text=JSON.stringify(state.runtime,null,2);setBudget(text);setBudgetOriginal(text);setBudgetVersion(state.version);}
  function discardSettingsEdit(){setBudget('');setBudgetOriginal('');setBudgetVersion(null);}
  const session=useRef(0); const published=useRef(false); const abort=useRef(new AbortController()); const pendingMessage=useRef<{text:string;key:string}|null>(null);
  const loaded=useRef({messages:1,goals:1,memories:1}); const refreshSerial=useRef(0); const commands=useRef(new Map<string,{signature:string;key:string}>());
  const recovery=useRef<AbortController|null>(null);
  const initKey=useRef(crypto.randomUUID()); const end=useRef<HTMLDivElement>(null);
  function clearSession(){recovery.current?.abort();recovery.current=null;published.current=false;session.current++;abort.current.abort();abort.current=new AbortController();setToken('');setCredential('');setState(null);setNeedsInit(false);setMessages({items:[],next_cursor:null});setMemories({items:[],next_cursor:null});setGoals({items:[],next_cursor:null});setOperation(null);setDraft('');setDocText('');setDocTitle('');setSearch('');setBudget('');setBudgetOriginal('');setBudgetVersion(null);setBusy(false);setError('');setConnection('尚未连接');setLastSyncedAt(null);setReceipt(null);pendingMessage.current=null;loaded.current={messages:1,goals:1,memories:1};commands.current.clear();}
  // A stream can close because a privacy generation changed, even while the
  // server is healthy again. Retire every old read before hiding private views.
  // Runtime configuration drafts/baselines stay owned by the existing guard.
  function retirePrivate(connection:string,preserveIntentions=false){
    recovery.current?.abort();recovery.current=null;session.current++;refreshSerial.current++;
    published.current=false;
    abort.current.abort();abort.current=new AbortController();
    setState(null);setMessages({items:[],next_cursor:null});setMemories({items:[],next_cursor:null});setGoals({items:[],next_cursor:null});
    setOperation(null);setReceipt(null);setDraft('');setDocText('');setDocTitle('');setSearch('');setBusy(false);setLastSyncedAt(null);setConnection(connection);
    loaded.current={messages:1,goals:1,memories:1};
    // A failed read cannot establish whether an in-flight write committed.
    // Keep its original key for explicit same-intention recovery, never retry it.
    if(!preserveIntentions){pendingMessage.current=null;for(const path of commands.current.keys())if(path!=='/settings')commands.current.delete(path);}
    return session.current;
  }
  function reconnect(access:string){
    if(recovery.current||!access)return;
    const generation=retirePrivate(navigator.onLine?'连接中断，正在重连':'网络已断开');
    if(!navigator.onLine)return;
    const controller=new AbortController();recovery.current=controller;
    void (async()=>{
      let backoff=1000;
      try{while(!controller.signal.aborted&&generation===session.current){
        const snapshot=await latestRefresh.current(access,generation);
        if(snapshot||controller.signal.aborted||generation!==session.current)return;
        await new Promise<void>(resolve=>{
          const done=()=>{clearTimeout(timer);controller.signal.removeEventListener('abort',done);resolve();};
          const timer=setTimeout(done,backoff);controller.signal.addEventListener('abort',done,{once:true});if(controller.signal.aborted)done();
        });
        backoff=Math.min(backoff*2,30000);
      }}finally{if(recovery.current===controller)recovery.current=null;}
    })();
  }
  const api=createRepository(token,abort.current.signal);
  function failed(e:unknown,generation:number){if(generation!==session.current||(e instanceof DOMException&&e.name==='AbortError'))return;
    if(e instanceof ApiError&&(e.code==='unauthorized'||e.code==='cross_origin_denied')){clearSession();setError('令牌无效或已失效，请重新连接。');return;}setError(e instanceof Error?e.message:'暂时无法完成请求');}
  async function mutate<T>(path:string,method:string,body:unknown,version?:number):Promise<T>{
    const generation=session.current;const signature=JSON.stringify({method,body,version});let command=commands.current.get(path);
    if(command?.signature!==signature){command={signature,key:crypto.randomUUID()};commands.current.set(path,command);}
    const result=await api<T>(path,method,body,command.key,version);
    if(generation!==session.current)throw new DOMException('Session ended','AbortError');
    if(version!==undefined)setReceipt(result as {version:number;individual_id:string});
    if(commands.current.get(path)?.key===command.key)commands.current.delete(path);return result;
  }
  async function pages<T>(kind:'messages'|'goals'|'memories',access:string,read:ReturnType<typeof createRepository>):Promise<Page<T>>{
    let cursor:string|null=null;const items:T[]=[];const count=loaded.current[kind];
    for(let i=0;i<count;i++){const params=new URLSearchParams();if(kind==='messages'){params.set('order','desc');if(i===0)params.set('limit','50');}if(cursor)params.set('cursor',cursor);
      const result:Page<T>=await read(`/${kind}?${params}`,'GET',undefined,undefined,undefined,access);items.push(...result.items);cursor=result.next_cursor;if(!cursor)break;}
    return {items,next_cursor:cursor};
  }
  async function refresh(access=token,generation=session.current){
    const serial=++refreshSerial.current;
    try {
      const read=createRepository(access,abort.current.signal);
      const s=await read<State>('/state');if(generation!==session.current||serial!==refreshSerial.current)return;
      const results=await Promise.all([pages<Message>('messages',access,read),pages<Goal>('goals',access,read),pages<Memory>('memories',access,read)]);
      const op=operation?await read<Operation>(`/operations/${operation.operation_id}`):null;
      if(generation!==session.current||serial!==refreshSerial.current)return;
      // Publish the snapshot atomically. state.individual_id starts the stream
      // effect, so it must not become visible before all guarded pages arrive.
      published.current=true;setState(s);setNeedsInit(false);setMessages(results[0]);setGoals(results[1]);setMemories(results[2]);if(op)setOperation(op);
      setLastSyncedAt(new Date().toISOString());setConnection('服务已连接');return s;
    } catch(e){
      if(generation!==session.current||serial!==refreshSerial.current)return;
      if(e instanceof ApiError&&['unauthorized','cross_origin_denied'].includes(e.code)){failed(e,generation);return;}
      // Every failed projection invalidates the published private snapshot,
      // including source-validation422. Do not retire an already-empty recovery
      // generation: the existing EOF reconnect loop must retain its retry handle.
      const current=published.current?retirePrivate('暂时无法同步',true):generation;
      if(e instanceof ApiError&&e.code==='not_initialized'){setNeedsInit(true);setConnection('等待第一次相遇');}
      else{setConnection('暂时无法同步');failed(e,current);}
    }
  }
  const latestRefresh=useRef(refresh);latestRefresh.current=refresh;
  useEffect(()=>{if(!token)return;const offline=()=>retirePrivate('网络已断开');const online=()=>reconnect(token);window.addEventListener('offline',offline);window.addEventListener('online',online);return()=>{window.removeEventListener('offline',offline);window.removeEventListener('online',online);};},[token]);
  useEffect(()=>{if(token)void latestRefresh.current();},[token,operation?.operation_id]);
  useEffect(()=>{
    if(!token||!state)return;
    const controller=new AbortController();const signal=AbortSignal.any([controller.signal,abort.current.signal]);const generation=session.current;
    const sync=async()=>{const snapshot=await latestRefresh.current(token,generation);if(!snapshot||signal.aborted)throw new Error('快照尚未同步');return snapshot.event_cursor;};
    void watchEvents({token,cursor:state.event_cursor,signal,refresh:sync,unauthorized:()=>{if(generation===session.current){clearSession();setError('实时连接鉴权失败，请重新连接。');}},status:text=>{if(generation===session.current&&!signal.aborted){if(text==='连接中断，正在重连')reconnect(token);else setConnection(text);}}});
    const visible=()=>{if(!document.hidden)void latestRefresh.current(token,generation);};document.addEventListener('visibilitychange',visible);
    return()=>{controller.abort();document.removeEventListener('visibilitychange',visible);};
  },[token,state?.individual_id]);
  async function connect(e:FormEvent){e.preventDefault();const generation=session.current;setBusy(true);setError('');
    try{await api<State>('/state','GET',undefined,undefined,undefined,credential);if(generation===session.current){setToken(credential);setCredential('');}}
    catch(e){if(generation!==session.current)return;if(e instanceof ApiError&&e.code==='not_initialized'){setToken(credential);setCredential('');setNeedsInit(true);}else failed(e,generation);}finally{if(generation===session.current)setBusy(false);}}
  async function initialize(e:FormEvent){e.preventDefault();const generation=session.current;setBusy(true);setError('');
    try{const persona=structuredClone(personaSeed);persona.identity.display_name=name.trim();await api('/initialize','POST',{persona,runtime:runtimeSeed},initKey.current);if(generation===session.current)await refresh();}catch(e){failed(e,generation);}finally{if(generation===session.current)setBusy(false);}}
  async function send(e:FormEvent){e.preventDefault();if(!draft.trim())return;const generation=session.current;setBusy(true);setError('');
    const pending=pendingMessage.current?.text===draft?pendingMessage.current:{text:draft,key:crypto.randomUUID()};pendingMessage.current=pending;
    try{const r=await api<{operation_id:string}>('/messages','POST',{text:pending.text},pending.key);if(generation!==session.current)return;setOperation({operation_id:r.operation_id,status:'queued',error_code:null});setDraft('');pendingMessage.current=null;await refresh();}catch(e){failed(e,generation);}finally{if(generation===session.current)setBusy(false);}}
  async function toggleAutonomy():Promise<boolean>{if(!state)return false;const generation=session.current;setBusy(true);setError('');try{await mutate(`/autonomy/${state.autonomy_paused?'resume':'pause'}`,'POST',{},state.version);if(generation!==session.current)return false;await refresh();return true;}catch(e){failed(e,generation);return false;}finally{if(generation===session.current)setBusy(false);}}
  async function importDocument(e:FormEvent){e.preventDefault();const generation=session.current;setBusy(true);setError('');try{await mutate('/documents','POST',{title:docTitle,text:docText,source_label:'WebUI 手动导入'});if(generation===session.current){setDocText('');setDocTitle('');setShowImport(false);await refresh();}}catch(e){failed(e,generation);}finally{if(generation===session.current)setBusy(false);}}
  async function saveSettings(e:FormEvent):Promise<boolean>{e.preventDefault();if(!state||budgetVersion===null)return false;const generation=session.current;setBusy(true);setError('');try{JSON.parse(budget);await mutate('/settings','PUT',new RawJsonDocument(budget),budgetVersion);if(generation!==session.current)return false;setBudget('');setBudgetOriginal('');setBudgetVersion(null);await refresh();return true;}catch(e){failed(e,generation);return false;}finally{if(generation===session.current)setBusy(false);}}
  async function createGoal(draft:GoalDraft):Promise<boolean>{const generation=session.current;setBusy(true);setError('');try{const r=await mutate<GoalReceipt>('/goals','POST',draft);if(generation!==session.current)return false;setReceipt(r);await refresh();return true;}catch(e){failed(e,generation);return false;}finally{if(generation===session.current)setBusy(false);}}
  async function changeGoal(goal:Goal,action:GoalAction,reason:string,evidence_ids:string[]):Promise<boolean>{const generation=session.current;setBusy(true);setError('');try{const r=await mutate<GoalReceipt>(`/goals/${goal.id}/transitions`,'POST',{action,reason,evidence_ids},goal.version);if(generation!==session.current)return false;setReceipt(r);await refresh();return true;}catch(e){failed(e,generation);await refresh();return false;}finally{if(generation===session.current)setBusy(false);}}
  async function more(kind:'messages'|'goals'|'memories'){const page=kind==='messages'?messages:kind==='goals'?goals:memories;if(!page.next_cursor)return;const generation=session.current;setBusy(true);loaded.current[kind]++;try{await refresh();}finally{if(generation===session.current)setBusy(false);}}
  return {read:api,state,token,credential,setCredential,needsInit,name,setName,initKey,messages,memories,goals,error,setError,busy,connection,lastSyncedAt,draft,setDraft,operation,search,setSearch,docTitle,setDocTitle,docText,setDocText,showImport,setShowImport,budget,setBudget,budgetVersion,setBudgetVersion,budgetOriginal,beginSettingsEdit,discardSettingsEdit,settingsDirty:budgetVersion!==null&&budget!==budgetOriginal,receipt,clearSession,refresh,connect,initialize,send,toggleAutonomy,importDocument,saveSettings,createGoal,changeGoal,more};
}
export type IndividualController=ReturnType<typeof useIndividual>;
