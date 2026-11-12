import type {DeleteIntention} from './types';
export class PrivacyHttpError extends Error{constructor(public status:number,public code:string){super(status===404?'目标或操作当前不可用，不能据此判断删除完成。':status===409?'409：目标版本或请求标识发生冲突；需要重新读取并明确确认。':status===401||status===403?'认证或访问权限失效，请重新连接。':status===422||status===400?'删除请求被拒绝；请重新读取目标与服务就绪状态。':'服务暂时无法确认请求结果。');}}
export function deletionHeaders(token:string,intention:DeleteIntention){return {Authorization:'Bearer '+token,'Idempotency-Key':intention.key,'If-Match':intention.etag};}
export async function privacyRequest(token:string,path:string,signal:AbortSignal,intention?:DeleteIntention){
 const r=await fetch('/api/v1'+path,{method:intention?'DELETE':'GET',headers:intention?deletionHeaders(token,intention):{Authorization:'Bearer '+token},cache:'no-store',signal:AbortSignal.any([signal,AbortSignal.timeout(15000)])});
 let data:unknown;try{data=await r.json();}catch{throw new Error('服务响应未能验证，请重新读取。');}if(!r.ok){const code=(data as {error?:{code?:unknown}})?.error?.code;throw new PrivacyHttpError(r.status,typeof code==='string'?code:'request_failed');}
 return {status:r.status,etag:r.headers.get('etag'),data};
}
