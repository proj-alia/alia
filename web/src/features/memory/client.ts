export class MemoryHttpError extends Error{constructor(public status:number,public code:string,message:string){super(message);}}
export type MemoryResponse<T>={data:T;etag:string|null;status:number};
export async function memoryRequest<T>(token:string,path:string,signal:AbortSignal,options?:{body:unknown;etag:string;key:string}):Promise<MemoryResponse<T>>{
 const response=await fetch('/api/v1'+path,{method:options?'POST':'GET',headers:{Authorization:`Bearer ${token}`,...(options?{'Content-Type':'application/json','If-Match':options.etag,'Idempotency-Key':options.key}:{})},body:options?JSON.stringify(options.body):undefined,signal:AbortSignal.any([signal,AbortSignal.timeout(15000)]),cache:'no-store'});
 const data=await response.json();if(!response.ok)throw new MemoryHttpError(response.status,data.error?.code??'request_failed',data.error?.message??'请求失败，请重新读取。');
 return {data,etag:response.headers.get('etag'),status:response.status};
}
export function validMemoryEtag(etag:string|null,version:number):etag is string{return Number.isSafeInteger(version)&&version>0&&etag===`"${version}"`;}
