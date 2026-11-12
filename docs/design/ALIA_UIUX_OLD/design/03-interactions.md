# 交互流与状态反馈（操作契约）

## 主导航

`Sidebar item click -> route change -> update page title/breadcrumb -> focus page h1 (production) -> load domain data`。
移动端：点击菜单 → 抽屉打开 + 主区遮罩 → 选页/点击遮罩/Escape 后关闭。生产实现需管理焦点并防止背景键盘导航。

## 记忆流

1. 进入“记忆档案”，默认展示全部类别和首条的详细信息；不自动采纳所有“候选记忆”。
2. 切分类 Tab 或搜索输入，列表仅匹配当前过滤条件，空结果明确提示。
3. 点击记忆，右侧 Inspector 展示 `id / provenance / evidence status / history / body`。
4. 点击“提议修订”，打开 Dialog：填写建议+依据，前端校验非空，提交后等待后端反馈；不得直接覆盖当前记忆。
5. 点击“标记待复核”，通过 Repository 新建审核状态；原型显示模拟反馈。

## 行动审批流（高风险）

```text
PENDING
  -> Open approval details (actor / target / scope / payload / impact / expiresAt)
  -> Review (Approve or Deny)
  -> Confirmation dialog
  -> POST approve/deny with request version and idempotency key
  -> Server accepted? ---- no ---> show error & keep PENDING
  | yes
  -> ACKNOWLEDGED (server receipt + auditId) -> refresh list
```

- 审批 UI 不能在没有服务端反馈时写入 `APPROVED`。
- 系统应支持“审批请求已被其他操作者处理”的 409 冲突提示。
- 默认显示拒绝/关闭，批准不是默认焦点。
- 外发邮件、发布、写文件、设备控制等均需明确目标与可撤销性。

## 暂停自主行动流

`Click pause -> show exact impact (queued/running tasks) -> Confirm -> call server -> update runtime status from response -> show auditId`。恢复同理。仅改变 UI 状态不等于真实停止后台。

## 权限矩阵

`Select changed -> show diff of old/new policy -> require confirmation -> PATCH with policyVersion -> refresh server state -> show receipt`。后台若拒绝，UI 恢复服务端原值，保留错误提示。禁止让角色提示词代替权限引擎。

## 样式与输入

- Hover/focus/pressed/disabled/loading 均提供独立反馈。
- 风险按钮遵循语义，不用紫色代替危险色。
- 对话框支持 Escape 关闭（危险操作提交中不能无条件关闭），可读的 ARIA label。
- Toast 用于非关键结果；关键失败必须在原操作上下文内显示。
- 主题切换仅影响 UI，记忆/人格/策略数据不变。
- 所有图表与数字必须具备单位、时间范围和来源；不造可验证性不足的心理/意识分数。
