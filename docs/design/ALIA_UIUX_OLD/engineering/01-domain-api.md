# ALIA Console · 前端领域与 API 契约建议

**设计假设而非现成后端事实。** Agent 实施前必须读取真实仓库的 API；如果已有协议，按真实接口做 Adapter，而不是要求后端迁就本文件。以下 `entityId` 是稳定身份键，其他对象由资源 ID 引用它。

## TypeScript 领域模型

```ts
type EntityId = string;
type ISODateTime = string;
type EvidenceStatus = 'confirmed' | 'unverified' | 'contested';
type RuntimeState = 'running' | 'paused' | 'degraded' | 'offline' | 'unknown';
type ApprovalState = 'pending' | 'approved' | 'denied' | 'expired' | 'cancelled';
type PermissionMode = 'auto' | 'approve' | 'deny';

interface DigitalEntity {
  id: EntityId;                // identity anchor, never inferred from model/session
  displayName: string;
  createdAt: ISODateTime;
  personaVersionId: string;
  runtime: { state: RuntimeState; checkedAt: ISODateTime | null };
  revision: number;           // optimistic concurrency
}

interface PersonaVersion {
  versionId: string;
  entityId: EntityId;
  createdAt: ISODateTime;
  createdBy: string;
  reason: string;
  traits: Record<string, string>;
  style: Record<string, string>;
  constraints: string[];      // constraints are not capabilities / grants
  previousVersionId?: string;
}

interface MemoryRecord {
  id: string;
  entityId: EntityId;
  kind: 'core' | 'semantic' | 'episode';
  title: string;
  content: string;
  evidenceStatus: EvidenceStatus;
  provenance: { sourceType: string; sourceId: string; sourceExcerptId?: string }[];
  createdAt: ISODateTime;
  revisedAt?: ISODateTime;
  supersedesId?: string;
  version: number;
}

interface RelationshipRecord {
  id: string;
  entityId: EntityId;
  peerId: string;
  label: string;
  consentScope: string[];
  createdAt: ISODateTime;
  lastInteractionAt?: ISODateTime;
}

interface AgentAction {
  id: string;
  entityId: EntityId;
  goalId: string;
  adapterId: string;
  target: string;
  payloadSummary: string;       // sanitized; privileged payload server-side
  status: 'queued' | 'awaiting_approval' | 'running' | 'completed' | 'failed';
  createdAt: ISODateTime;
}

interface ApprovalRequest {
  id: string;
  entityId: EntityId;
  actionId: string;
  target: string;
  scope: string;
  risk: 'low' | 'medium' | 'high';
  payloadPreview: string;       // sanitized preview for human review
  expiresAt: ISODateTime;
  state: ApprovalState;
  version: number;
}

interface Channel {
  id: string;
  entityId: EntityId;
  kind: 'web' | 'browser' | 'api' | 'email' | 'voice' | 'device' | 'other';
  state: 'connected' | 'degraded' | 'disconnected' | 'planned';
  capabilities: string[];
  lastCheckedAt: ISODateTime | null;
}

interface PermissionPolicy {
  id: string;
  entityId: EntityId;
  capability: string;
  mode: PermissionMode;
  resourceScope: string[];
  revision: number;
}

interface AuditEvent {
  id: string;
  entityId: EntityId;
  occurredAt: ISODateTime;
  actorId: string;
  operation: string;
  resourceType: string;
  resourceId: string;
  traceId: string;
  outcome: 'ok' | 'denied' | 'error';
}
```

## Repository 隔离

```ts
interface EntityRepository {
  getEntity(id: EntityId, signal?: AbortSignal): Promise<DigitalEntity>;
  listMemories(id: EntityId, filter: { q?: string; kind?: string; cursor?: string }): Promise<{ items: MemoryRecord[]; nextCursor?: string }>;
  proposeMemoryRevision(id: EntityId, memoryId: string, data: { reason: string; proposal: string; expectedVersion: number }): Promise<{ proposalId: string; auditId: string }>;
  listApprovals(id: EntityId): Promise<ApprovalRequest[]>;
  decideApproval(id: EntityId, approvalId: string, decision: 'approve' | 'deny', params: { expectedVersion: number; idempotencyKey: string }): Promise<{ state: ApprovalState; auditId: string }>;
  requestRuntimeState(id: EntityId, next: 'paused' | 'running'): Promise<{ state: RuntimeState; auditId: string }>;
  updatePermission(id: EntityId, policy: PermissionPolicy, nextMode: PermissionMode): Promise<{ revision: number; auditId: string }>;
  listAuditEvents(id: EntityId, cursor?: string): Promise<{ items: AuditEvent[]; nextCursor?: string }>;
}
```

## HTTP API 路由示例（可替换）

| 方法 | 路由 | 所需能力 |
|---|---|---|
| GET | `/api/v1/entities/{id}` | entity.read |
| GET | `/api/v1/entities/{id}/persona/versions` | persona.read |
| POST | `/api/v1/entities/{id}/persona/proposals` | persona.propose |
| GET | `/api/v1/entities/{id}/memories?q=&kind=&cursor=` | memory.read |
| POST | `/api/v1/entities/{id}/memories/{memoryId}/revision-proposals` | memory.propose |
| GET | `/api/v1/entities/{id}/relationships` | relation.read |
| GET | `/api/v1/entities/{id}/actions` | action.read |
| GET | `/api/v1/entities/{id}/approvals` | approval.read |
| POST | `/api/v1/entities/{id}/approvals/{approvalId}/decision` | approval.decide |
| GET | `/api/v1/entities/{id}/channels` | media.read |
| GET/PATCH | `/api/v1/entities/{id}/policies/{policyId}` | policy.read/write |
| POST | `/api/v1/entities/{id}/runtime/transitions` | runtime.manage |
| GET | `/api/v1/entities/{id}/audit-events` | audit.read |

## HTTP 状态约定

- 200/201：使用服务端回执更新 UI（含 resource version、时间、auditId）。
- 202：仅表示已受理；UI 展示处理中，不得写成完成。
- 401：身份认证失效；引导重新验证。403：缺少操作权限，不展示敏感资源。
- 404：目标不存在或不可见；409：版本冲突；412：前置条件不满足。
- 422：校验错误，展示字段级提示；429：节流与恢复时间；5xx：可重试但不能重复发起危险操作。

## 安全要求

- 鉴权与权限判定在服务端执行，客户端只能提示，不可成为事实来源。
- 审批请求包含明确 target/scope/payload preview/expiry/version，必须能抵抗过期、重复执行和并发更新。
- AuditEvent 是服务端写入，不接受 UI 任意覆写；敏感字段做脱敏。
- 不把权限提升写入 Persona 或系统 Prompt，避免被叙事性配置绕过。
- MockRepository 只在明确的演示环境启用，页面必须显示 `DEMO DATA`；真实接口失败不能自动回落 Mock。
