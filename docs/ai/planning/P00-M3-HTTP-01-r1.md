# P00-M3-HTTP-01｜发展读取与 owner 撤回 HTTP 接入

| 字段 | 草案值 |
| --- | --- |
| task_id / revision | P00-M3-HTTP-01 / 1 |
| 状态 | 已定义；待 Root 审查，不构成实施授权 |
| 建议实施 owner | Root（共享路由和服务入口所有者） |
| reviewer | P04：发展事务／版本；P05：来源、隐私和回执重放；均须审查精确实施版本 |
| 工程验证 | P03 对固定候选验收；P02 沿原 P02-08 接相关浏览器检查，不另造同职责任务 |
| 本版增量 | 首版；提出 API 薄接入及释放条件，没有设计新的发展状态机 |

## 实际基线与依赖

- 当前路由事实：`src/service.rs` 的 GET `/developments`、POST `/developments/{id}/revert` 为 unavailable；未注册 GET `/developments/{id}`。
- 已有 Store 方法：`developments`、`development`、`owner_revert_development`。P04-10 固定 v4 的 adapter SHA `cc6df7c97362cd7dd645af6cb06ae005594b230e1b4712ebc5d23d25bf18aca4`；报告 SHA `f52c12efc0fe45a0717a532f53d5456d9a827752ad415d77e9f50e295446ac4c`，见 [P04-10](../ALIA_P04_10_HANDOFF.md)。属于已交付、待 Root 对实施基线确认的依赖，不能直接采用后续 live 哈希。
- canonical0028／0029 与 P05-07 的隐私投影、删除重放一起由 Root 选定精确可兼容版本。P04 的旧固定报告不覆盖 P05 后续容量修复版本。
- P02-08 已有 DTO、未知结果同意图重放及隐私屏障，见 [P02-08](../ALIA_P02_08_HANDOFF.md)。前端开发通过和完整 HTTP 功能验收分别记录。
- 不依赖 Root 当前社会状态模型写入完成、P04-11 兴趣修订、P05-08 备份、收费模型试验或完整工程矩阵才开始本切片。实施顺序由 Root 决定。

## 目标与非目标

目标：用现有领域／存储校验接通真实发展列表、详情与 owner 撤回；鉴权、一致性读取、版本、幂等、审计和来源失效继续由程序执行。撤回生成新事件／修订，实际恢复受影响目标，不擦除历史或覆盖后继 head。

非目标：本卡不实现模型提出发展、trial overlay、生产验证器、独立评审生成或 Accept 入口；不修改人格允许路径、发展六态、权限、模型预算、导出备份和导航。工程测试中的可信合成评审记录不能证明现实评审独立或成长质量。

## Root 释放前裁决

1. 确认此范围未被另一活跃任务持有；如已有计划，合并本草案，不重复派工。记录最终 owner/reviewer、源码基线与接口版本。
2. 冻结列表／详情 DTO、query 和 management 的 `development.read_enabled`、`revert_enabled` 语义。现有前端使用 `include_history`、`has_more` 及全局／隐私版本元数据；CONTRACTS 全局列表另有 `{items,next_cursor}`。由 Root 明确兼容关系，不由实现者各自发明协议。
3. 确认 POST reason-only、逐 development 强 If-Match、原幂等键及现有七字段回执；不要因旧文字数量笔误增加字段。新增／补充协议由 Root 同步 CONTRACTS、schema、示例与验收。
4. 确认真实浏览器所需记录如何由受信任的测试／运行时路径产生；无生产验证器时保留该项依赖，不用伪造成功 JSON 或直接 SQL 状态冒充完整生命周期。

## 允许改动范围（接受时再冻结）

Root 新服务模块（建议 `src/service/developments.rs`）、共享路由注册及实际 management 投影的最小接入；对应独立 HTTP/PG 测试和必要协议配套。复用已接受 adapter，不并行重写其状态机；发现必须修改领域／存储时修订本卡并交相关 owner 审查。P00 不编辑上述文件。

## 验收与交接

- 实际 PG/HTTP 验证列表、详情、真实 owner 撤回和新的修订／事件；auth、严格 query/JSON/header、不可见对象及 no-store 与既有 API 一致。
- 同意图未知结果重放只提交一次；同键异意图、旧 development 版本、竞争 head／撤回和事务后段故障不能留下部分人格／策略更新。重放仍检查来源与隐私，不能借缓存绕过 schema29 移除状态。
- 祖先来源撤销／缺失通用边、删除后旧回执、恢复门关闭及迟到读取均不得返回失效内容；不得解除旧 ID 或重置 generation 版本。
- P02-08 消费真实接口执行对应读写浏览器检查；尚缺可信完整试用记录的分支明确 not_run。不得用 501、合成 DTO 检查或有限接口通过补判完整 M3。
- 按影响范围运行 Rust／Clippy、实际 PG/HTTP、M0 与相关浏览器；不为本薄接入反复触发完整矩阵。完整矩阵仍按 Root/P03 固定候选节奏执行。
- 交接绑定 task_id/revision、源码与接口摘要、实际命令／退出码、非作者审查结论及处置、失败／未运行／清理、残余依赖。审查后相关源码改变则重新确认。

交付后的依赖：P02 完整发展生命周期验收仍需真实 trial／验证／接受运行时；本切片不把该缺口转为通过。M5 工程候选及正式行为／14 日观察完成条件保持。
