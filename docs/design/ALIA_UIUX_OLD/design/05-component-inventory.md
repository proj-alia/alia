# 可复用组件清单 · 原型 → 前端实现

| UI 构件 | 建议组件/Hook | 使用范围 | 必须覆盖状态 |
|---|---|---|---|
| 主框架 | `AppShell`, `Sidebar`, `Topbar`, `ResponsiveDrawer` | 全站 | 当前页、移动端开关、断网 |
| 结构导航 | `NavItem`, `Breadcrumb`, `PageHeader` | 全站 | active, focus, disabled |
| 状态标签 | `StatusBadge`, `RiskBadge` | 首页/行动/媒介 | ok/warn/fail/mock/unknown |
| 指标 | `MetricCard` | 总览 | loading/empty/error/unit/source |
| 基础表面 | `Panel`, `PanelHeader`, `Divider` | 全站 | title, action, status |
| 身份信息 | `IdentitySummary`, `IdentityVersionHistory` | 总览/身份 | stableId, version, runtimeDisconnected |
| 记忆 | `MemorySearch`, `MemoryFilters`, `MemoryItem`, `MemoryInspector` | 记忆 | searching, none, selected, pendingReview |
| 认知 | `BeliefCard`, `EvidenceList`, `UncertaintyNotice` | 认知 | hasEvidence/unverified |
| 关系 | `RelationshipList`, `SharedTimeline` | 关系 | selected/empty/consent |
| 行动 | `GoalPanel`, `ActionList`, `ApprovalCard`, `ApprovalDetail` | 总览/行动 | pending/approved/denied/expired/conflict |
| 媒介 | `ChannelCard`, `CapabilityList` | 媒介 | connected/degraded/disconnected/planned |
| 权限 | `PolicyMatrix`, `PolicyChangeDialog` | 治理 | auto/approve/deny/403/conflict |
| 事件 | `AuditTimeline`, `AuditSearch`, `ExportAction` | 事件 | query/empty/error/pagination |
| 设置 | `ThemeToggle`, `RuntimeHealth`, `ExportStatus` | 设置 | mock/real/offline |
| 交互辅助 | `ConfirmDialog`, `Toast`, `EmptyState`, `LoadingSkeleton`, `ErrorNotice` | 全站 | keyboard/focus/error |

**分层要求**：UI 组件不直接执行外部工具、不直接修改权限、不拥有“记忆事实”数据源。通过 `Repository` 获取服务端状态，再将交互请求传至对应服务。
