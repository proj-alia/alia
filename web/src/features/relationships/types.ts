export type RelationshipMeta={individual_id:string;state_version:number;privacy_epoch:number;observed_at:string};
export type RelationshipItem={subject_id:string;subject_key:string;stage:string;version:number};
export type RelationshipPage=RelationshipMeta&{items:RelationshipItem[];next_cursor:string|null};
export type RelationshipDetail=RelationshipMeta&{relationship:RelationshipItem&{expectations:unknown;boundaries:unknown;commitments:unknown;unresolved:unknown}};
export type RelationshipEvent={id:string;sequence:number;kind:string;source_kind:string;actor_id:string;occurred_at:string;text:string};
export type RelationshipEvents=RelationshipMeta&{subject_id:string;items:RelationshipEvent[];next_cursor:string|null};
