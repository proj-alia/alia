// Static functional icons from the provided UI/UX handoff; no character assets.
import type {ReactNode} from 'react';
const icons:Record<string,ReactNode>={
  grid:<><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></>,
  fingerprint:<><path d="M12 11a2 2 0 0 0-2 2c0 4-1 6-2 8"/><path d="M14 13c0 4 0 7-1 9"/><path d="M8 17c0 2-.5 4-1 5"/><path d="M16 17c0 2 .2 4 1 5"/><path d="M6 14a6 6 0 0 1 12 0v3"/><path d="M4 15v-1a8 8 0 0 1 16 0v2"/></>,
  archive:<><rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v12h14V8M10 12h4"/></>,
  brain:<><path d="M12 18V5a3 3 0 0 0-5.8-1A4 4 0 0 0 4 11a4 4 0 0 0 .7 7.1A4 4 0 0 0 12 18Zm0 0V5a3 3 0 0 1 5.8-1A4 4 0 0 1 20 11a4 4 0 0 1-.7 7.1A4 4 0 0 1 12 18Z"/><path d="M8 9a3 3 0 0 1-4 2m5 5a3 3 0 0 0-4 2m11-9a3 3 0 0 0 4 2m-5 5a3 3 0 0 1 4 2"/></>,
  users:<><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2m14-17a4 4 0 0 1 0 8m6 9v-2a4 4 0 0 0-3-3.87"/><circle cx="9" cy="7" r="4"/></>,
  compass:<><circle cx="12" cy="12" r="9"/><path d="m15.5 8.5-3 7-4-4 7-3Z"/></>,
  radio:<><path d="M5 16a9 9 0 0 1 0-8m14 0a9 9 0 0 1 0 8M8 13a5 5 0 0 1 0-2m8 0a5 5 0 0 1 0 2"/><circle cx="12" cy="12" r="1"/><path d="M12 14v7"/></>,
  shield:<><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/></>,
  clock:<><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></>,
  flask:<><path d="M9 3h6M10 3v6l-6 10a1 1 0 0 0 1 2h14a1 1 0 0 0 1-2L14 9V3M8 14h8"/></>,
  network:<><rect x="9" y="2" width="6" height="6" rx="1"/><rect x="2" y="16" width="6" height="6" rx="1"/><rect x="16" y="16" width="6" height="6" rx="1"/><path d="M12 8v4H5v4m7-4h7v4"/></>,
  server:<><rect x="3" y="3" width="18" height="7" rx="2"/><rect x="3" y="14" width="18" height="7" rx="2"/><path d="M7 6h.01M7 17h.01M11 6h6M11 17h6"/></>,
  puzzle:<><path d="M9 3h5v4a2 2 0 1 0 4 0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3h6Z"/></>,
  activity:<><path d="M3 12h4l3-8 4 16 3-8h4"/></>,
  settings:<><path d="M12 2v3m0 14v3M4.93 4.93l2.12 2.12m9.9 9.9 2.12 2.12M2 12h3m14 0h3M4.93 19.07l2.12-2.12m9.9-9.9 2.12-2.12"/><circle cx="12" cy="12" r="4"/></>,
};
export function Icon({name}:{name:string}){return <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{icons[name]}</svg>;}
