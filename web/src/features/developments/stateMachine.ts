import type {RevertIntention,RevertReceipt,RevertState} from './types';
export const emptyRevert:RevertState={phase:'idle',intention:null,receipt:null,error:''};
export type RevertEvent={type:'submit';intention:RevertIntention}|{type:'unknown';error:string}|{type:'reject';error:string}|{type:'commit';receipt:RevertReceipt}|{type:'clear'};
export function sameIntention(a:RevertIntention,b:RevertIntention){return a.individualId===b.individualId&&a.developmentId===b.developmentId&&a.version===b.version&&a.etag===b.etag&&a.key===b.key&&a.reason===b.reason&&a.acceptedHead===b.acceptedHead;}
export function transition(state:RevertState,event:RevertEvent):RevertState{
 if(event.type==='clear')return {...emptyRevert};
 if(event.type==='submit'){if((state.phase==='unknown'||state.phase==='submitting')&&state.intention&&!sameIntention(state.intention,event.intention))throw new Error('未知撤回意图不可替换。');return {phase:'submitting',intention:event.intention,receipt:null,error:''};}
 if(event.type==='commit'){if(!state.intention||event.receipt.individual_id!==state.intention.individualId||event.receipt.development_id!==state.intention.developmentId||event.receipt.development_version!==state.intention.version+1)throw new Error('回执不属于原撤回意图。');return {phase:'confirmed',intention:null,receipt:event.receipt,error:''};}
 return {...state,phase:event.type==='unknown'?'unknown':'rejected',receipt:null,error:event.error};
}
