export type Interpretation={id:string;version:number;topic:string;title:string;interpretation:string;status:'current'|'superseded';supersedes:string|null;created_at:string;superseded_at:string|null;event_ids:string[];belief_ids:string[]};
export type TimelineView={individual_id:string;state_version:number;privacy_epoch:number;observed_at:string;items:Interpretation[];has_more:boolean;view:'autobiographical_interpretations'};
const uuid=(v:unknown):v is string=>typeof v==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v);
const date=(v:unknown)=>typeof v==='string'&&/^\d{4}-\d{2}-\d{2}T/.test(v)&&Number.isFinite(Date.parse(v));
const text=(v:unknown,max:number)=>typeof v==='string'&&!!v.trim()&&!v.includes('\0')&&Array.from(v).length<=max;
const ids=(v:unknown,min:number,max:number)=>Array.isArray(v)&&v.length>=min&&v.length<=max&&v.every(uuid)&&new Set(v).size===v.length;
export function validTimeline(value:unknown,individual:string|undefined,history:boolean,limit:number):value is TimelineView{
 const v=value as TimelineView|null;
 return !!v&&v.individual_id===individual&&v.view==='autobiographical_interpretations'&&Number.isSafeInteger(v.state_version)&&v.state_version>0&&Number.isSafeInteger(v.privacy_epoch)&&v.privacy_epoch>=0&&date(v.observed_at)&&typeof v.has_more==='boolean'&&Array.isArray(v.items)&&v.items.length<=limit&&new Set(v.items.map(i=>i?.id)).size===v.items.length&&v.items.every(i=>!!i&&uuid(i.id)&&Number.isSafeInteger(i.version)&&i.version>0&&typeof i.topic==='string'&&/^[a-z][a-z0-9_.-]{0,79}$/.test(i.topic)&&text(i.title,160)&&text(i.interpretation,4000)&&(i.status==='current'||history&&i.status==='superseded')&&(i.supersedes===null||uuid(i.supersedes))&&date(i.created_at)&&(i.status==='current'?i.superseded_at===null:date(i.superseded_at))&&ids(i.event_ids,1,32)&&ids(i.belief_ids,0,8));
}
