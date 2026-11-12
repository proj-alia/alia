// Real-only adapter to the existing single-individual API. No demonstration fallback.
export class RawJsonDocument {constructor(public readonly text:string){}}
export class ApiError extends Error { constructor(public code:string,message:string){super(message);} }
export function createRepository(token:string,sessionSignal:AbortSignal) {
  return async function request<T>(path:string,method='GET',body?:unknown,key?:string,version?:number,access=token):Promise<T>{
    const headers:Record<string,string>={Authorization:`Bearer ${access}`};
    if(body!==undefined)headers['Content-Type']='application/json';if(key)headers['Idempotency-Key']=key;if(version!==undefined)headers['If-Match']=`"${version}"`;
    const response=await fetch(`/api/v1${path}`,{method,headers,body:body===undefined?undefined:body instanceof RawJsonDocument?body.text:JSON.stringify(body),signal:AbortSignal.any([sessionSignal,AbortSignal.timeout(15000)]),cache:'no-store'});
    const value=await response.json();if(!response.ok)throw new ApiError(value.error?.code??'request_failed',value.error?.message??'请求未完成');return value as T;
  };
}
