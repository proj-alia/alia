# Agent 端到端落地路径

## 建议工作分支

在现有仓库创建与主干隔离的 UI/UX 开发分支（遵守仓库贡献规范）。优先增量提交，不批量替换已有业务实现。

| 里程碑 | 产物 | 技术验证 | 退出条件 |
|---|---|---|---|
| M0 · 现状侦察 | `ALIA_UI_RECON.md`、页面/接口差异矩阵 | 运行项目已有构建命令 | 不破坏现有模块的集成路径已确认 |
| M1 · UI 基座 | Theme/Tokens/AppShell/共享组件 | layout 与主题切换 E2E | 桌面/手机均能完整导航 |
| M2 · 页面实现 | 10 页面真实组件 + 交互 | 每页截图、功能测试 | 不能有空白/死导航页 |
| M3 · 领域数据 | DTO, validation, repositories | Mock/Real 分离单测 | 无“真数据失败→假数据成功”的伪装 |
| M4 · 风险流程 | Approval/Policy/Pause + Audit | 权限错误、409、双击、超时测试 | 真副作用必须等待服务端回执 |
| M5 · 质量 | A11y、响应式、状态覆盖 | lint/typecheck/build/unit/E2E | 主要场景全部通过且异常有反馈 |
| M6 · 交接 | 截图、部署说明、风险与状态报告 | 重现测试命令 | 真实完成范围可审计 |

## 具体实现顺序

1. `src/app`：主路由、应用壳、Sidebar 与 Topbar；先验证 390px、1440px。
2. `src/design-system`：Color Tokens、基础卡片、按钮、Tab、搜索框、StatusBadge、ConfirmDialog。
3. `src/features/overview`：用同一份稳定 Entity ID 拉取各子域摘要，注意多个接口独立 loading/error。
4. `identity`, `memory`, `cognition`, `relationships`：阅读类工作流+对修订操作的状态机。
5. `agency`, `media`, `governance`, `audit`, `settings`：审批、授权、审计与运行治理。
6. `src/shared/repositories`：先可控 mock，再接真实 API；强制标识部署模式。
7. 端到端测试、性能检查、验收并制作交接文件。

## 与原型的关系

原型是**行为与布局参考**，不是可直接复制的生产源代码。`prototype/assets/app.js` 把状态和数据集中用于离线演示，生产系统应拆分至真实页面组件、独立业务 Hook 与 Repository。

## 质量约束

- 不引入没有实际价值的 UI 库和动画框架；已有组件库可直接复用。
- 复用现有项目目录、别名、路由与测试命令；不要在仓库根目录另外生成一个几乎一样的项目。
- UI 核心页面不得依赖外部字体/随机图片/远程截图服务才能展示。
- 任何被后端阻塞的功能，完成能完成的 UI 状态设计后，在 RISKS 中写明阻塞与推荐 API 合同，不静默填充假数据。
