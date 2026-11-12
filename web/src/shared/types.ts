import personaSeed from '../../../config/persona.example.json';
import runtimeSeed from '../../../config/runtime.example.json';
export type Message = { id: string; sequence: number; role: 'user' | 'assistant'; text: string; occurred_at: string; operation_id: string };
export type PersonAssertion = {entity_id:string;label:string;role:'subject'|'mentioned';assertion_event_id:string;source_event_id:string;actor_id:string;source_kind:string;occurred_at:string};
export type Memory = { id: string; content: string; kind: string; scope: string; validity: string; source_event_ids: string[];person_assertions:PersonAssertion[];person_assertions_more:boolean };
export type Goal = { id: string; version: number; origin_ids: string[]; no_progress_cycles: number; description: string; status: string; next_step: string; waiting_reason: string | null; success_criterion: string };
export type Page<T> = {items: T[]; next_cursor: string | null};
export type State = {
  individual_id: string; event_cursor: string; version: number; config_version: number; created_at: string; lifecycle: string; autonomy_paused: boolean;
  persona: typeof personaSeed; runtime: typeof runtimeSeed; goals: Record<string, number>; budget_frozen: boolean;
  budget_window: null | {charged_total: number; charged_background: number; reserved_total: number; ends_at: string};
  indexing: {enabled:boolean;profile:string|null;retrieval_mode:string};
  model: {mode: string; readiness: string}; capabilities: {id: string; available: boolean; authorized: boolean}[];
};
export type Operation = {operation_id: string; status: string; error_code: string | null};

export type GoalDraft={description:string;success_criterion:string;next_step:string;stop_condition:string};
export type GoalAction='activate'|'wait'|'abandon';
export type GoalReceipt={individual_id:string;goal_id:string;event_id:string;version:number;goal_version:number;status:string};
