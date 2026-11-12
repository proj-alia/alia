import {useCallback,useEffect,useRef,useState} from 'react';
import {resolveRoute} from './routes';
type Pending={path:string;delta:number|null;action?:()=>void};
const currentRoute=()=>resolveRoute(window.location.hash);
export function useConsoleNavigation(dirty:boolean){
 const [path,setPath]=useState(currentRoute),[pending,setPending]=useState<Pending|null>(null),[settling,setSettling]=useState(false);
 const active=useRef(path),index=useRef(0),dirtyRef=useRef(dirty),restoring=useRef(false),allowed=useRef(false);
 dirtyRef.current=dirty;
 const pendingRef=useRef(pending);pendingRef.current=pending;
 function accept(next:string,nextIndex:number){active.current=next;index.current=nextIndex;setPath(next);setPending(null);window.history.replaceState({...window.history.state,aliaConsoleIndex:nextIndex},'','#/'+next);}
 function commit(next:string){const i=index.current+1;window.history.pushState({aliaConsoleIndex:i},'','#/'+next);accept(next,i);}
 const actions=useRef({navigate:(_value:string)=>{},request:(_action:()=>void)=>{},proceed:()=>{}});
 actions.current={
  navigate(value){const next=resolveRoute(value);if(next===active.current)return;if(dirtyRef.current){setPending({path:next,delta:null});return;}commit(next);},
  request(action){if(dirtyRef.current){setPending({path:active.current,delta:null,action});return;}action();},
  proceed(){const target=pendingRef.current;if(!target||restoring.current)return;if(target.action){setPending(null);target.action();return;}if(target.delta!==null){allowed.current=true;window.history.go(target.delta);}else commit(target.path);},
 };
 useEffect(()=>{
  index.current=window.history.state?.aliaConsoleIndex??0;
  window.history.replaceState({...window.history.state,aliaConsoleIndex:index.current},'','#/'+active.current);
  const change=()=>{
   const next=currentRoute(),i=window.history.state?.aliaConsoleIndex??index.current+1;
   if(restoring.current){restoring.current=false;setSettling(false);return;}
   if(allowed.current){allowed.current=false;accept(next,i);return;}
   if(next===active.current){index.current=i;return;}
   if(dirtyRef.current){
    // Restore the current history entry before asking; only an explicit choice may leave.
    window.history.replaceState({...window.history.state,aliaConsoleIndex:i},'');
    const delta=i-index.current;setPending({path:next,delta:delta||null});
    if(delta){restoring.current=true;setSettling(true);window.history.go(-delta);}
    else window.history.replaceState({...window.history.state,aliaConsoleIndex:index.current},'','#/'+active.current);
   }else accept(next,i);
  };
  const click=(e:MouseEvent)=>{
   if(e.defaultPrevented||e.button!==0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;
   const a=(e.target as Element).closest?.('a[href^="#/"]') as HTMLAnchorElement|null;
   if(!a||a.target||a.hasAttribute('download'))return;
   e.preventDefault();actions.current.navigate(a.hash);
  };
  const unload=(e:BeforeUnloadEvent)=>{if(dirtyRef.current){e.preventDefault();e.returnValue='';}};
  window.addEventListener('hashchange',change);document.addEventListener('click',click);window.addEventListener('beforeunload',unload);
  return()=>{window.removeEventListener('hashchange',change);document.removeEventListener('click',click);window.removeEventListener('beforeunload',unload);};
 },[]);
 const request=useCallback((action:()=>void)=>actions.current.request(action),[]);
 return {path,pending,settling,request,navigate:(value:string)=>actions.current.navigate(value),proceed:()=>actions.current.proceed(),cancel:()=>setPending(null)};
}
