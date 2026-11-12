export const navigationGroups = [
 {label:'工作台',items:[
  {id:'overview',label:'运行概览',description:'查看个体、目标与服务的实际运行状态。',icon:'grid'},
  {id:'console',label:'测试沙盒',description:'在隔离边界内检查输入、配置和执行能力。',icon:'flask'}]},
 {label:'数字个体',items:[
  {id:'identity',label:'身份与人格',description:'身份、人格版本、认知与关系分别管理。',icon:'fingerprint'},
  {id:'memory',label:'记忆管理',description:'检索经历，检查内容、来源与人物关联。',icon:'archive'}]},
 {label:'连接与能力',items:[
  {id:'platforms',label:'接入渠道',description:'管理外部入口与个体之间的连接。',icon:'network'},
  {id:'models',label:'模型服务',description:'查看生成与嵌入资源，区分配置就绪和执行结果。',icon:'server'},
  {id:'extensions',label:'扩展管理',description:'管理能力资源，分别检查安装、授权和启用。',icon:'puzzle'},
  {id:'deps',label:'依赖管理',description:'检查构建清单与扩展依赖的可用范围。',icon:'archive'}]},
 {label:'配置与运行',items:[
  {id:'config',label:'配置文件',description:'分区编辑当前运行规则，验证后按版本提交。',icon:'settings'},
  {id:'tasks',label:'自动化任务',description:'管理持久目标、运行条件与停止原因。',icon:'clock'},
  {id:'data',label:'数据与日志',description:'查询运行、交流、事件与诊断数据。',icon:'activity'},
  {id:'governance',label:'权限与审批',description:'检查授权范围、资源约束与审计记录。',icon:'shield'},
  {id:'settings',label:'系统设置',description:'界面、访问与资料管理。',icon:'settings'}]},
];
export const pages = navigationGroups.flatMap(group=>group.items);
export function resolveRoute(value:string):string {
 const path=value.replace(/^#?\/?/,'')||'overview';
 const [root,...rest]=path.split('/');
 const aliases:Record<string,string>={cognition:'identity/cognition',relations:'identity/relations',agency:'tasks',automations:'tasks/schedule',workbench:'console',channels:'platforms',media:rest[0]==='chat'?'platforms/web/chat':'platforms/body',traces:'data/traces',events:'data/sessions',diagnostics:'data/statistics'};
 if(root==='media')return aliases.media;
 return aliases[root]?[aliases[root],...rest].join('/'):path;
}
