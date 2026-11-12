import {createContext,useContext} from 'react';
import type {DeleteIntention,DeleteState,PrivacyOperation,PrivacyReadiness} from './types';
export type PrivacyController={ready:boolean;readiness:PrivacyReadiness|null;readinessLoading:boolean;readinessError:string;refreshReadiness:()=>void;open:(targetId:string)=>void;hidden:boolean;state:DeleteState;canReplay:boolean;replay:()=>void;operationId:string|null;progress:PrivacyOperation|null;progressLoading:boolean;progressError:string;query:(id:string)=>boolean;refreshProgress:()=>void;knownOperations:string[];canQuery:boolean;recoveryNotice:string;pendingRequests:DeleteIntention[];restorePending:(intention:DeleteIntention)=>void};
export const PrivacyContext=createContext<PrivacyController|null>(null);
export function usePrivacy(){const value=useContext(PrivacyContext);if(!value)throw new Error('Privacy provider missing');return value;}
