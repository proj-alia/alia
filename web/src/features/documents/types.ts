export type DocumentItem={id:string;title:string;source_label:string;source_kind:string;created_at:string};
export type DocumentPage={items:DocumentItem[];next_cursor:string|null};
export type DocumentSlice={individual_id:string;document_id:string;event_id:string;title:string;source_label:string;source_kind:string;offset:number;max_chars:number;total_chars:number;text:string;next_offset:number|null;observed_at:string};
