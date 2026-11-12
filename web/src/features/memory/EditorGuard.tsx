import {createContext,useContext,type Dispatch,type SetStateAction,type ReactNode} from 'react';
import type {IndividualController} from '../../shared/useIndividual';
export type MemoryEditorGuard={dirty:boolean;busy:boolean;error:string;session:string;stamp:string;validated:boolean;summary:ReactNode;save:()=>Promise<boolean>;discard:()=>void};
export const memoryEditorStamp=(c:Pick<IndividualController,'state'|'lastSyncedAt'|'connection'>)=>JSON.stringify([c.state?.individual_id,c.state?.version,c.lastSyncedAt,['服务已连接','实时同步中'].includes(c.connection)]);
export const MemoryEditorContext=createContext<{register:Dispatch<SetStateAction<MemoryEditorGuard|null>>;request:(action:()=>void)=>void}|null>(null);
export function useMemoryEditorGuard(){const context=useContext(MemoryEditorContext);if(!context)throw new Error('Memory editor guard missing');return context;}
