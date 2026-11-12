import type {Memory as PublicMemory} from '../../shared/types';
export type OwnerMemory=PublicMemory&{version:number;created_at:string;valid_from:string;valid_until:string|null;supersedes:string|null};
export type MemoryView={individual_id:string;state_version:number;memory:OwnerMemory};
export type RevisionReceipt={individual_id:string;memory_id:string;previous_memory_id:string;event_id:string;version:number;memory_version:number;previous_memory_version:number};
export type MemoryHistoryPage={individual_id:string;state_version:number;privacy_epoch:number;observed_at:string;target_memory_id:string;items:{memory:OwnerMemory;correction:{event_id:string;reason:string;occurred_at:string}|null}[];next_cursor:string|null};
export type RevisionBody={content:string;scope:string;reason:string};
export type RevisionDraft={base:OwnerMemory;etag:string;content:string;reason:string;preview:boolean;status:'editing'|'conflict'|'unknown';intention:{key:string;body:RevisionBody;etag:string;target:string}|null};
