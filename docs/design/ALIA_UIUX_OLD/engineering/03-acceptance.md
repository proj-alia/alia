# ALIA UI/UX 验收标准 · Design DoD v2

> 仅在实际运行源码、测试和截图验证后标记为完成。原型文件中的模拟行为不能被算作真实权限和 Agent Runtime 已实现。

## P0 · 界面与导航

- [ ] 10 个页面均可打开，有真实布局、内容组件和对应交互；无空白页、死链接。
- [ ] 桌面 1440px：256px 深紫左侧导航、白色内容卡、4 KPI、事件/审批双列布局清晰。
- [ ] 1024px：收窄导航后内容不被遮挡；768px：明细改为合理单列。
- [ ] 390px/360px：抽屉可打开、关闭，整体无横向滚动和按钮遮挡。
- [ ] 明暗主题均可读，主题切换不改变业务状态；含键盘焦点与 Escape 支持。
- [ ] **没有任何角色形象资产**：不含立绘、动画头像、人物插画、动漫背景、照片。
- [ ] 采用提供的 `tokens.json` 配色和布局语义，必要差异有文档记录。

## P0 · 数据与连续性

- [ ] stable `entityId` 与 `modelId`, `sessionId`, `channelId` 分离。
- [ ] 记忆列表/详情包含来源、类别、证据状态、修订或复核路径。
- [ ] 人格有版本信息、修订理由、审核/回滚设计，不与权限策略耦合。
- [ ] 关系是对象和事件历史，不伪装成无依据的亲密度/情感测量。
- [ ] 实时运行状态来自可校验的数据源，断线时不会显示“在线”。

## P0 · 权限与审计

- [ ] 审批详情展示具体 target、scope、payload preview、风险、时效。
- [ ] 批准/拒绝经过二次确认且等待服务端回执；点击不直接代表成功。
- [ ] 已处理/过期审批在并发更新时给出冲突反馈，防止误重复提交。
- [ ] 策略变更和暂停行为由服务端决定，失败后界面展示真实状态。
- [ ] 高风险操作日志至少保留服务端 `auditId`, `actor`, `occurredAt`, `resourceId`。
- [ ] 未授权/会话过期/网络错误/409 冲突能被可靠呈现。

## P1 · 体验细节

- [ ] 全站 loading/empty/error/unknown/disconnected 的状态与恢复路径可见。
- [ ] 记忆搜索、类别筛选、条目选择、修订建议、状态反馈完整。
- [ ] 媒介“已连接/未连接/规划中”与能力范围区分清晰。
- [ ] 输入框、按钮、模态框、开关可键盘操作；图标按钮有名称。
- [ ] 尊重 `prefers-reduced-motion`，文本与背景达到可读性要求。

## P1 · 工程交付

- [ ] 仓库项目原有构建、静态检查和测试没有退化。
- [ ] 生成 `docs/ai/ALIA_UI_STATUS.md`、`ALIA_UI_TEST_REPORT.md`、`ALIA_UI_RISKS.md`。
- [ ] 附每页截图、适配器设计、Mock/Real 使用说明与后端依赖列表。
- [ ] 没有声明任何未真实接通的功能为生产能力。

## QA 测试证据模板

```text
Test ID: ALIA-UI-APPROVAL-01
Build / commit: ...
Viewport / browser: 390px / Chromium ...
Steps: open action -> inspect target -> approve -> confirm
Expected: pending until server returns receipt; then approved with audit ID
Actual: ...
Evidence: screenshot / trace / test output
Status: PASS | FAIL | BLOCKED
```
