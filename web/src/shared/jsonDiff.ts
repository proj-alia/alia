export type JsonDifference={path:string;before:unknown;after:unknown};
// Configuration documents are bounded by server schemas. Arrays are compared as
// values to preserve order; absent values remain distinct from explicit null.
export function jsonDiff(before:unknown,after:unknown,path='$'):JsonDifference[]{
 if(JSON.stringify(before)===JSON.stringify(after))return [];
 const object=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==='object'&&!Array.isArray(value);
 if(object(before)&&object(after))return [...new Set([...Object.keys(before),...Object.keys(after)])].sort().flatMap(key=>jsonDiff(before[key],after[key],`${path}.${key}`));
 return [{path,before,after}];
}
