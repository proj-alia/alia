import {useEffect,useRef,useState,type ReactNode} from 'react';
export function Panel({title,subtitle,children,action}:{title:string;subtitle?:string;children:ReactNode;action?:ReactNode}){return <section className="panel"><div className="panel-head"><div><h2>{title}</h2>{subtitle&&<p>{subtitle}</p>}</div>{action}</div>{children}</section>;}
export function Badge({children,tone='neutral'}:{children:ReactNode;tone?:'neutral'|'success'|'warning'|'danger'}){return <span className={`badge ${tone}`}>{children}</span>;}
export function Empty({children}:{children:ReactNode}){return <div className="empty">{children}</div>;}
export function Unavailable({children}:{children:ReactNode}){return <p className="unavailable">尚未开放 · {children}</p>;}
export function Data({value}:{value:unknown}){return <pre>{JSON.stringify(value,null,2)}</pre>;}
export function Dialog({title,children,onClose,busy=false}:{title:string;children:ReactNode;onClose:()=>void;busy?:boolean}){
 const ref=useRef<HTMLDialogElement>(null);
 useEffect(()=>{const last=document.activeElement as HTMLElement|null;const dialog=ref.current!;dialog.showModal();return()=>{dialog.close();last?.focus();};},[]);
 return <dialog ref={ref} aria-labelledby="dialog-title" onKeyDown={e=>{
 if(e.key!=='Tab')return;
 const nodes=Array.from(e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled),a[href],input:not(:disabled),select:not(:disabled),textarea:not(:disabled),summary,[tabindex="0"]')).filter(n=>n.getClientRects().length>0);
 const first=nodes[0],last=nodes.at(-1);if(!first){e.preventDefault();e.currentTarget.focus();return;}
 if(e.shiftKey&&document.activeElement===first){e.preventDefault();last?.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first.focus();}
 }} onCancel={e=>{e.preventDefault();if(!busy)onClose();}} onClick={e=>{if(e.target===e.currentTarget&&!busy)onClose();}}><div className="dialog-body"><h2 id="dialog-title">{title}</h2>{children}</div></dialog>;
}
export function Confirm({title,children,action,onClose,busy,error}:{title:string;children:ReactNode;action:()=>Promise<boolean>;onClose:()=>void;busy:boolean;error?:string}){
 const [submitting,setSubmitting]=useState(false);
 const [unexpected,setUnexpected]=useState('');
 const inFlight=useRef(false);
 const waiting=busy||submitting;
 async function submit(){
  if(inFlight.current||busy)return;
  inFlight.current=true;setSubmitting(true);setUnexpected('');
  try{if(await action())onClose();}
  catch{setUnexpected('请求未完成，请检查连接后重试。');}
  finally{inFlight.current=false;setSubmitting(false);}
 }
 return <Dialog title={title} onClose={onClose} busy={waiting}><div>{children}</div>{(unexpected||error)&&<p role="alert" className="alert">{unexpected||error}</p>}<div className="form-actions"><button autoFocus onClick={onClose} disabled={waiting}>取消</button><button className="primary" disabled={waiting} onClick={()=>void submit()}>{waiting?'等待服务端…':'确认提交'}</button></div></Dialog>;
}

export function Tabs({items,value,onChange,label}:{items:{id:string;label:string}[];value:string;onChange:(id:string)=>void;label:string}){
 return <div role="tablist" aria-label={label} className="tabs" onKeyDown={e=>{
  if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key))return;e.preventDefault();
  const index=items.findIndex(item=>item.id===value);const next=e.key==='Home'?0:e.key==='End'?items.length-1:(index+(e.key==='ArrowRight'?1:-1)+items.length)%items.length;
  onChange(items[next].id);(e.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next])?.focus();
 }}>{items.map(item=><button key={item.id} role="tab" aria-selected={item.id===value} tabIndex={item.id===value?0:-1} onClick={()=>onChange(item.id)}>{item.label}</button>)}</div>;
}
export function ResourceState({loading,error,retry}:{loading:boolean;error:string;retry:()=>void}){
 if(loading)return <div className="skeleton compact" role="status" aria-live="polite">正在读取服务端数据…</div>;
 if(error)return <div role="alert" className="resource-error"><p>{error}</p><button onClick={retry}>重新加载</button></div>;
 return null;
}
export function ResourceWorkspace({title,items,selected,onSelect,children}:{title:string;items:{id:string;label:string;description?:string}[];selected:string;onSelect:(id:string)=>void;children:ReactNode}){
 return <div className="resource-workspace"><aside className="resource-directory" aria-label={title}><div className="directory-heading"><h2>{title}</h2><small>{items.length} 项</small></div><div className="resource-options">{items.length?items.map(item=><button key={item.id} type="button" aria-pressed={selected===item.id} onClick={()=>onSelect(item.id)}><b>{item.label}</b>{item.description&&<small>{item.description}</small>}</button>):<Empty>没有匹配资源。</Empty>}</div></aside><div className="resource-detail">{children}</div></div>;
}
