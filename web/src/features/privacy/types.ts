export type PrivacyStatus='blocked'|'purging'|'retry_pending'|'completed';
export type PrivacyReadiness={deletion_enabled:boolean;journal_ready:boolean;status_read_available:boolean};
export type DeleteBaseline={individualId:string;targetId:string;memoryVersion:number;etag:string};
/** No body, token, source text or user draft is retained in a deletion intention. */
export type DeleteIntention=DeleteBaseline&{key:string};
export type DeleteReceipt={individual_id:string;operation_id:string;target_id:string;state_version:number;privacy_epoch:number;status:PrivacyStatus};
export type PrivacyOperation=DeleteReceipt&{observed_at:string;last_error:string|null};
export type DeletePhase='idle'|'confirming'|'submitting'|'unknown'|'rejected'|'tracking';
export type DeleteState={phase:DeletePhase;intention:DeleteIntention|null;operationId:string|null;receipt:DeleteReceipt|null;error:string};
export type DeleteEvent={type:'confirm'}|{type:'restore';intention:DeleteIntention}|{type:'dispatch';intention:DeleteIntention}|{type:'unknown'}|{type:'rejected';error:string}|{type:'receipt';receipt:DeleteReceipt}|{type:'recover';operationId:string}|{type:'invalidate'}|{type:'close'};
