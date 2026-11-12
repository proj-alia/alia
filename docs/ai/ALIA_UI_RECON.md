# ALIA UI/UX v2 集成记录

2026-11-11。输入为 `tmp/ALIA_UIUX`，完整设计副本保存在 `docs/design/ALIA_UIUX`。包内模拟数据、操作和验收记录仅是参考，不是当前系统事实。此次采用无角色形象版本，替代早期插画概念图。

现有工程为 Rust/Axum/SQLx/PostgreSQL + React/TypeScript/Vite；npm lock；Bearer 令牌只在页面内存；REST 写入使用幂等键和版本，SSE 使用持久 outbox。沿用现有 web 工程，不部署静态模拟原型。生产前端只有真实 API Repository，无 Mock 回退。

| 路由 | 真实数据与操作 | 差距处理 |
| --- | --- | --- |
| #/overview | /state、/messages、/goals、/memories | 只显示已加载记录数，无健康/意识分数 |
| #/identity | /state.persona、身份锚点 | 人格修订和版本历史未开放，明确提示 |
| #/memory | /memories，来源/人物注记查看、筛选、分页 | 更正删除待 M2；不模拟成功 |
| #/cognition | 当前人格认知规则、目标、记忆来源 | 信念/自传接口未开放，不伪造观点 |
| #/relations | 共同消息事件、初始关系规则 | 对象关系档案接口未开放 |
| #/agency | /goals创建/转换/列表、/operations、/autonomy | 无外部审批服务，不展示假审批卡 |
| #/media | /state.capabilities；#/media/chat 真实消息 | 视觉/语音/设备标明未接入 |
| #/governance | 真实能力授权矩阵、运行规则 | 不引入后端不支持的 auto/approve/deny |
| #/events | 真实消息事件及关联运行 ID | 当前筛选结果可导出；不宣称完整审计接口 |
| #/settings | /state、PUT /settings、POST /documents | 主题仅影响页面；可导出当前状态摘要，不导出凭证 |

组件边界：app 壳和导航、design-system tokens/组件、shared 真实 API 与个体状态、features 独立页面。哈希路由保留刷新/后退，不增加路由库。保留同一个体的全部服务端状态。高风险治理操作先展示影响/版本，再发送原有 API，等实际回执后刷新。

验证使用现有 Cargo/SQLx 测试及真实 HTTP 的 Playwright 浏览器恢复测试，扩展十页、五种宽度、主题与键盘测试。未实现业务能力不得以静态设计包的模拟检查结果计为通过。

## v3.0 审计与实施基线

以用户最新17页清单为准，保留v2设计原包不改。现有React19/TypeScript/Vite8、自建tokens/原生dialog/SVG功能图标、内存令牌、REST/SSE与SQLx事务直接复用。原10路由保留，settings拆出diagnostics，新增automations/workbench/channels/models/extensions/traces，共17页；media/chat仍为媒介子页。四组导航和256/76px可收起侧栏不创建第二后台。

已有真实写入仅初始化、消息、资料、目标、运行配置和暂停；人格修订/回滚、完整记忆修订删除、外部审批、插件安装/MCP运行、渠道注册和模型凭证管理无服务端契约，不能伪装接通。本阶段增加白名单运行/步骤/事件元数据读取、诊断投影及隔离配置校验；不返回原始模型请求、工具输入输出、服务端日志或部署密钥。测试工作台的静态校验明确不调用模型/工具，不改变个体状态。

模式参考的一手来源（只借鉴分组导航、管理资源列表、详情追踪与显式反馈，不移植业务架构）：
- AstrBot WebUI：https://docs.astrbot.app/en/use/webui.html
- Koishi控制台：https://koishi.chat/zh-CN/guide/console/client
- LangBot扩展机制：https://langbot.app/docs/en/usage/sandbox/runtime-relationships/
- Dify工作流运行追踪：https://www.dify.ai/workflows
- Open WebUI实例/用户设置分离：https://docs.openwebui.com/getting-started/quick-start/settings/

实施顺序：审计→导航/tokens/组件→17页→真实Repository/服务投影→权限与脱敏→原有回归+新增API/组件/浏览器验收。业务能力状态与页面是否可访问分别记录。
