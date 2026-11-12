export type NoteItem={id:string;title:string;created_at:string;run_id:string;result_event_id:string;capability_id:string;capability_version:string};
export type NotePage={items:NoteItem[];next_cursor:string|null};
export type NoteSlice=Omit<NoteItem,'id'>&{individual_id:string;note_id:string;action_id:string;offset:number;max_chars:number;total_chars:number;text:string;next_offset:number|null;observed_at:string};
