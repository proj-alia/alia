// SSE carries committed resource notifications, never model tokens or credentials.
export type Notice = {id:string; type:string; data:string};
export function parser(emit:(event:Notice)=>void) {
  let line='', data:string[]=[], type='', id='', sawCR=false;
  function finishLine() {
    if(line==='') { if(id||data.length)emit({id,type:type||'message',data:data.join('\n')});data=[];type='';id=''; }
    else if(!line.startsWith(':')) {
      const colon=line.indexOf(':');const key=colon<0?line:line.slice(0,colon);let value=colon<0?'':line.slice(colon+1);if(value.startsWith(' '))value=value.slice(1);
      if(key==='data')data.push(value);else if(key==='event')type=value;else if(key==='id'&&!value.includes('\0'))id=value;
    }
    line='';
  }
  return (chunk:string)=>{for(const ch of chunk){if(sawCR&&ch==='\n'){sawCR=false;continue;}sawCR=false;if(ch==='\r'||ch==='\n'){finishLine();sawCR=ch==='\r';}else line+=ch;if(line.length+data.join('').length>65536)throw new Error('事件帧超出限制');}};
}
const types=new Set(['operation.updated','message.created','state.changed','memory.changed','goal.changed','development.changed','privacy.changed','budget.changed']);
const validId=(id:string)=>/^\d{1,19}$/.test(id)&&BigInt(id)<=9223372036854775807n;
function delay(ms:number,signal:AbortSignal){return new Promise<void>((resolve,reject)=>{
  if(signal.aborted){reject(signal.reason);return;}
  const cancel=()=>{clearTimeout(timer);reject(signal.reason);};const timer=setTimeout(()=>{signal.removeEventListener('abort',cancel);resolve();},ms);signal.addEventListener('abort',cancel,{once:true});
});}
export async function watchEvents(options:{token:string;cursor:string;signal:AbortSignal;refresh:()=>Promise<string>;unauthorized:()=>void;status:(text:string)=>void}) {
  let cursor=options.cursor, backoff=1000;
  while(!options.signal.aborted){
    const controller=new AbortController();const signal=AbortSignal.any([options.signal,controller.signal]);let timer:ReturnType<typeof setTimeout>|undefined;
    const deadline=()=>{clearTimeout(timer);timer=setTimeout(()=>controller.abort(),45000);};
    try {
      deadline();
      const response=await fetch('/api/v1/events',{headers:{Authorization:`Bearer ${options.token}`,'Last-Event-ID':cursor},signal,cache:'no-store'});
      if(response.status===401||response.status===403){options.unauthorized();return;}
      if(response.status===410){cursor=await options.refresh();continue;}
      if(!response.ok||!response.body||!response.headers.get('content-type')?.startsWith('text/event-stream'))throw new Error('实时订阅暂不可用');
      options.status('实时同步中');const reader=response.body.getReader();const decoder=new TextDecoder('utf-8',{fatal:true});let pending:Notice[]=[];const feed=parser(n=>pending.push(n));
      try {while(!signal.aborted){
        deadline();const {value,done}=await reader.read();if(done)throw new Error('实时连接已关闭');
        feed(decoder.decode(value,{stream:true}));let next=cursor, changed=false;
        for(const n of pending){if(!validId(n.id))throw new Error('事件游标无效');if(BigInt(n.id)<=BigInt(cursor))continue;next=n.id;if(n.data&&types.has(n.type))changed=true;}
        pending=[];
        // Acknowledge only after guarded projections have refreshed. A failed read
        // reconnects with the previous cursor instead of silently losing the change.
        if(changed)await options.refresh();cursor=next;backoff=1000;
      }}finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
    }catch{if(options.signal.aborted)return;options.status('连接中断，正在重连');}
    finally{clearTimeout(timer);controller.abort();}
    if(options.signal.aborted)return;
    try{await delay(backoff,options.signal);}catch{return;}backoff=Math.min(backoff*2,30000);
  }
}
