import {createContext,useContext} from 'react';
import type {IndividualController} from './useIndividual';
export const IndividualContext=createContext<IndividualController|null>(null);
export function useIndividualContext(){const value=useContext(IndividualContext);if(!value)throw new Error('Individual provider missing');return value;}
