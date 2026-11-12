# 工程落地方案与验收

## 仓库集成前的审计

1. 检查前端框架、路由体系、样式方案、已有组件、主题与用户权限。
2. 检查 API 契约（provider、plugins、platforms、config profiles、traces、memory、approval）。
3. 生成 `docs/ai/ALIA_CONSOLE_AUDIT.md`：映射 v4 页面到现有组件 / 缺失 API / 重用策略。
4. 不要直接将单文件 HTML 当成生产代码，也不要为了重写 UI 替换或修改整个业务领域模型。

## 推荐任务切片

- M0：仓库审计、页面到 API 对照、风险清单、技术栈确认。
- M1：设计变量、侧栏与 Topbar、面包屑、分栏工作区、通用表格/表单/抽屉/确认框。
- M2：扩展管理（已安装/市场/Skills/MCP/指令），状态与配置 schema，对接真实 API。
- M3：配置文件和模型服务、渠道、依赖管理，含凭据安全与校验。
- M4：沙盒、日志与 Trace 数据工作区，服务端筛选分页和详情懒加载。
- M5：数字身份、记忆、任务、审批以及治理与审计记录。
- M6：移动端、空/错/载状态、无障碍、视觉回归、构建与测试。

## 前端 API Adapter 形状（示例，不是现有服务接口）

- `listExtensions({status,query,page}) → {items,total}`
- `getExtension(id) → {id,version,manifest,capabilities,configSchema,installed,enabled,diagnostic}`
- `planExtensionInstall(id,version) → {dependencies,permissions,sourceIntegrity,risks}`
- `installExtension({id,version,planRevision}) → {operationId,status}`
- `changeExtensionState(id,{enabled,expectedRevision}) → {revision,status}`
- `listConfigProfiles()`, `getConfigProfile(id)`, `validateConfigProfile(draft)`, `saveConfigProfile({draft,expectedRevision})`
- `listConnectors()`, `testConnector(id)`, `setConnectorState(id,command)`
- `listModelProviders()`, `updateProvider(id,{keyReplacement,...})`, `testProvider(id)`
- `listTraces({query,filters,cursor})`, `getTrace(id)`
- `requestApproval(action)`, `decideApproval({requestId,expectedVersion,decision,scope})`

**禁止**在浏览器侧相信 `enabled`、`approved` 和 `installed` 的本地临时值；以服务端结果与版本号为唯一可信依据。

## 核心验收（V4 UI 方向 Gate）

- [ ] 界面视觉与 v3 有明显区别：无紫色渐变 Hero、无装饰插画、无全屏大圆角统计墙。
- [ ] 左侧全局导航 + 内部二级导航逻辑清楚、适合管理工作流。
- [ ] 扩展：检索/排序/已安装/市场/配置/安装审查/停用卸载安全操作均能完整走通。
- [ ] 配置：分区、schema、脏表单、验证、保存、错误、撤销、离开保护齐全。
- [ ] 依赖：表格、版本差异、确认、安装状态、失败回滚入口存在。
- [ ] Trace：实时/历史独立，服务端过滤分页，选中加载详情，不用卡片代替记录。
- [ ] 模型与渠道：凭据不回显、连接状态区别于配置状态。
- [ ] 身份/记忆/关系不与运行时 Provider/Plugin 混淆。
- [ ] 所有危险操作服务端鉴权与审计，前端明确等待返回结果。
- [ ] 响应式：360、390、768、1280、1440px 可交互，桌面与移动端信息结构一致。
- [ ] 状态：loading / empty / error / retry / success / unauthorized / unsaved changes。
- [ ] 用实际 Playwright/E2E 记录验证，不以 CSS 截图代替功能测试。

## 报告

在仓库 `docs/ai` 维护：`ALIA_CONSOLE_STATUS.md`、`ALIA_CONSOLE_TEST_REPORT.md`、`ALIA_CONSOLE_BLOCKERS.md`、`ALIA_CONSOLE_DECISIONS.md`。每个页面须有截图、完成状态及 API 对接情况。不得报告未实际验证的项目为完成。
