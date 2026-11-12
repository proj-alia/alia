import type {RevertIntention} from './types';
export class DevelopmentHttpError extends Error{
 constructor(public status:number,public code:string){super(status===401?'认证已失效，请重新连接。':status===403?'访问被拒绝。':status===404?'发展记录或来源当前不可用，不能据此推断撤回成功。':status===409?'409：版本或目标当前修订已变化，说明已保留；请重新读取，不能覆盖后继修改。':status===422?'422：请求未通过服务端字段校验。':status===501?'服务端尚未开放此发展接口。':status===503?'服务暂不可用，未取得成功回执；已发出写入的结果仍需确认。':'请求未完成，请重新读取。');}
}
export async function request(token:string,path:string,signal:AbortSignal,intention?:RevertIntention){
 const r=await fetch('/api/v1'+path,{method:intention?'POST':'GET',headers:{Authorization:'Bearer '+token,...(intention?{'Content-Type':'application/json','Idempotency-Key':intention.key,'If-Match':intention.etag}:{})},body:intention?JSON.stringify({reason:intention.reason}):undefined,cache:'no-store',signal:AbortSignal.any([signal,AbortSignal.timeout(15000)])});
 let data:unknown;try{data=await r.json();}catch{throw new Error('响应未能读取，请重新确认实际结果。');}
 if(!r.ok){const v=data as {error?:{code?:unknown}};throw new DevelopmentHttpError(r.status,typeof v?.error?.code==='string'&&/^[a-z][a-z0-9_]{0,79}$/.test(v.error.code)?v.error.code:'request_failed');}
 return {status:r.status,data,etag:r.headers.get('etag')};
}
