# ALIA-P04｜可追溯记忆纠正 API

**当前状态：P04-01 READY，持续Goal已complete。8项真实TCP HTTP/PostgreSQL测试全部通过；定向编译、严格Clippy、定点fmt和M0退出0。输入源码前后SHA一致，独立测试资源清理通过。未部署；完整工程和真实行为仍交P03/P01。**

主线已实际合入公共注册：`src/adapters.rs`加`pub mod memory_commands;`；`src/service.rs`加`mod memory_commands;`，原`/memories/{id}`改`get(memory_commands::memory).delete(unavailable)`，原`/memories/{id}/revisions`改`post(memory_commands::revise)`。`key/version`已经pub(super)，无其他helper需求。

## 初始检查点（2026-11-12）

- 任务：P04-01；窗口 `01a1cf39-aa98-7801-93af-12ac3758bb5c`。持续 Goal active，无 token_budget。
- 主线唯一领域接口协调者：`01a1cbe8-e270-75e1-9535-d3f2e0ea81c4` / local。本文件供主线/P03读取，不向其他聊天回信。
- 已读取根 AGENTS、BLUEPRINT/CONTRACTS/IMPLEMENTATION 与并行登记；初次检查尚无 P04 release，仅写专属 handoff。现已接收Six-Goal正式release并实现获准新文件，未改迁移或公共注册。
- 首项仅实现已有契约中的 owner 记忆纠正及必要单记忆读取，不声称完整信念/关系/情绪/删除或 M2 验收。能力、来源、预算和隐私标准保持。

## 输入 SHA256

| 文件 | SHA256 |
| --- | --- |
| AGENTS.md | 79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4 |
| docs/BLUEPRINT.md | 30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943 |
| docs/CONTRACTS.md | 621e28cd088169c3194d4e28933ea660b66950ad77d04c86a8a5f0d07e3b4005 |
| docs/IMPLEMENTATION.md | fcd43b8a9265c4b0ac7fd6f618d35d5f0e2397bd940218d971df3f3c2163cba6 |
| src/adapters/store.rs（只读参考） | 451c1c20dd9670e533be069e125fe46682b8aad52acc895a4ef28b98b0e5e36b |
| src/service/api.rs（只读参考） | d118017ca504e54507b6415c70f1f63cb33e2efab97286ed653e0738c08987b4 |

## 一次性释放与公共注册需求

- 待主线登记允许新增 `src/adapters/memory_commands.rs`、`src/service/memory_commands.rs`、`tests/memory_revisions.rs`；均应为 ABSENT 基线。不修改 store/indexing/entities/runtime 或其他任务文件。
- 预期存储入口：`Store::revise_owner_memory(key: &str, memory: Uuid, expected_memory_version: i64, revision: &MemoryRevision) -> Result<MemoryRevisionReceipt, StoreError>`；类型置于新 adapter 模块，可由主线以 `pub mod memory_commands` 或精确 re-export 对外测试。
- 预期 HTTP 新模块 handlers：`memory_commands::memory`（GET `/memories/{id}`）与 `memory_commands::revise`（POST `/memories/{id}/revisions`）。主线统一注册新模块和两个已有路由的方法，不改变 DELETE 501。
- 复用 `service::api::Strict`；现 `key`/`version` 为 sibling 不可见，需要主线把这两个函数改为 `pub(super)`（或授权新模块复用同一严格解析 helper），避免重复设计头语义。
- 候选 receipt 延续现有约定：`{individual_id,memory_id,previous_memory_id,event_id,version,memory_version}`，version为全局版本、memory_version为新对象版本；ETag返回新 memory_version。GET返回实际记忆投影与全局state_version/privacy_epoch，ETag是目标memory.version。精确 shape 以主线登记为准。

## 实施要点（等待接口冻结）

- 复用 Store::write/replay/state/apply_memories/advance/outbox，短事务内创建 owner/reported_claim 纠正事件，保留原事实；原记忆 superseded，新 ID继承kind/同scope；纠正是新陈述，不自动核实外部事实。
- 新记忆直接来源为本次纠正事件，旧记忆/其原来源通过完整派生图保持删除和撤销闭包；不能把旧错误正文当成新事实依据。
- 幂等先于版本仍复验回执、新旧记忆及纠正事件祖先；全局版本递增使旧认知提交失效。旧记忆 version+digest 绑定的向量/人物断言不能复用到新 ID；索引异步状态如实显示未建立。
- 定向真实 PG/HTTP 覆盖同scope修订/源事件不变、旧版本竞争、幂等与撤销、晚期outbox故障全回滚、非法输入/生命周期、旧索引/人物关系不复活。无模型、不部署；完整回归交P03。

## 实际命令与状态

- 只读 `cat/sed/rg/sha256sum` 已执行，exit0；一次大文档输出被工具截断，继续按相关段落读取。
- Goal 建立成功；尚未运行编译、Rust/数据库/HTTP/M0测试，无通过声明。
- 下一依赖：主线一次性 release/接口 shape/公共模块与头 helper 注册。等待期间继续研究与准备精确测试，不要求用户中转。

## Release 已接收 / 首版模块保存

- 主线Six-Goal登记已释放并固定接口，本窗口已保存并定点rustfmt新adapter/service模块。仍未执行编译或数据库测试。
- 请求主线现在整合：`src/adapters.rs` 增 `pub mod memory_commands;`；`src/service.rs` 增 `mod memory_commands;`，把现有 `/memories/{id}` 改为 `get(memory_commands::memory).delete(unavailable)`，`/memories/{id}/revisions` 改为 `post(memory_commands::revise)`。DELETE仍501。handler/store签名与上节一致；service key/version已读取主线pub(super)修改。
- 响应按主线冻结shape实现；POST201，receipt另含previous_memory_version。GET无查询字段。reason最大1000字符，已按登记收窄。源纠正事件kind为`memory.corrected`、owner/reported_claim，payload.text为本次用户陈述；旧记忆→纠正事件→新记忆构成来源闭包。
- 正在自有 `tests/memory_revisions.rs` 准备真实HTTP/SQLx、回滚、旧模型/向量/人物断言测试。公共注册合入后按总锁做定向编译及运行；不会改已有N1/N2。

## 八组定向测试已保存 / 等待注册后编译

- `tests/memory_revisions.rs` 已保存并定点rustfmt，8项SQLx真实TCP用例：纠正链与原事件不变/幂等；两种并发；严格HTTP/来源/只读状态；生命周期；多代撤销及回执；晚期outbox故障回滚；旧向量/异步索引/person断言失效与新记忆可词项检索；旧模型PreparedDecision晚到拒绝且保留fixture账本。
- 模型用例只显式FixtureProvider，绝无外部调用。其usage_unknown必须保留，不当真实模型验收。
- 此处是测试代码保存，不是通过声明。root尚未注册新module/route时不能编译验证；请求的公共注册仍为上节3处，不需扩大领域或helper所有权。

## 公共注册完成 / 唯一定向父任务

- 主线已经确认公共模块、GET/POST方法和key/version helper整合，DELETE仍501。
- 唯一父任务为本窗口tool session `92027`，证据目录 `evals/results/p04-targeted-20261112-v1`，report当前queued_heavy_lock。先等待既有P01总锁，不另起重复任务；取得后持锁贯穿显式P03工程测试库启动、定向编译/8项真实HTTP/SQLx、局部严格Clippy、定点fmt检查、M0与清理。
- 使用P03当前资源工具和固定镜像，随机独立owner/回环端口，DATABASE_URL仅注入子进程环境，不读生产.env，不调用真实模型。源码输入前后全量摘要将记录；若共享源码发生变化，测试结果保持unverified，不假称稳定版本通过。
- 现有内部来源屏障与新belief迁移由主线维护，本窗口不改它们。未修改已有N1/N2或其他任务文件。

## P04-01 最终工程交付 / READY

### 已实现

- GET `/api/v1/memories/{id}`：现有Memory归档投影、来源与人物注记、state_version；强ETag为记忆对象version；同快照递归祖先检查，未知查询字段422、来源不可用404。
- POST `/memories/{id}/revisions`：严格content/scope/reason、既有owner头语义；If-Match为目标memory.version，作用域必须相同。active/paused可更正，archived/terminated拒绝，superseded/旧版本409。
- 一事务追加owner/reported_claim `memory.corrected`，原事实事件和旧内容不动；旧记忆superseded/version递增，新ID/version1、继承kind、同scope、纠正事件直接来源，旧记忆→纠正事件/新记忆完整派生链。全局版本、两条memory.changed、state.changed、幂等回执同事务。
- receipt按主线固定字段返回，`previous_memory_version`为此次提交后的旧记忆版本。幂等重放先于版本仍重查祖先和回执对象；重复请求不重复写入。新记忆不继承旧向量或人物断言，旧model/index晚到受现有版本屏障拒绝，已结算用量不抹掉。

### 实际命令与结果

唯一父任务 session92027 自然终态exit0，总锁从独立测试库准备持续到清理。证据：`evals/results/p04-targeted-20261112-v1/report.json` 与同目录1—5.log。源码集合含当时root的0022 belief迁移，仅作为共享编译输入；本任务未改该迁移。

| 命令 | 实际结果 |
| --- | --- |
| `cargo check --locked --test memory_revisions` | exit0 |
| `cargo test --locked --test memory_revisions -- --ignored` | exit0；8 passed / 0 failed / 0 ignored |
| `cargo clippy --locked --lib --test memory_revisions -- -D warnings` | exit0，无warning |
| `rustfmt --check --edition 2024 src/adapters/memory_commands.rs src/service/memory_commands.rs tests/memory_revisions.rs` | exit0 |
| `.venv/bin/python scripts/check_phase0.py` | exit0；5 schemas/47闭合对象/9示例/52负例/111追溯/136依赖/9文档/34链接/2来源 |

实际HTTP/PG覆盖：原事件完整相等、两代纠正链/历史与当前列表、Unicode最大边界、不同scope拒绝、两请求同版本唯一生效/同key并发重放、JSON/头/鉴权/同源/no-store、paused允许与archive/terminate拒绝、两代祖先/纠正事件/输出对象被阻断后的读取与新请求/回执拒绝、晚期state通知故障全部回滚及同key恢复、旧索引lease与snapshot拒绝/人物断言失效/新记忆词项可见但未伪造索引、真正观察旧记忆的PreparedDecision在更正后拒绝且fixture账本原样保留。

源码输入前后全量SHA相符（report.sources_stable=true）；P03资源工具创建的专属工程测试容器/卷身份经核验且cleanup.passed=true。真实模型调用0；一个显式FixtureProvider用例仅验证提交版本与账本，不是行为验收。没有N1/N2源、产物或报告修改。

### 最终SHA256

```json
{
  "src/adapters/memory_commands.rs": "307ba5c872ae59fc1a2ad88e806da86b59476153f77656732b86cfba4361cd4a",
  "src/service/memory_commands.rs": "d41baf8462d9c16a64e658b2c7f98a305961376ea3681d01e2ece7399aaf7dc8",
  "tests/memory_revisions.rs": "51894cb8ee7bdd4c3dd03dcc6e02cd60558896d2dfa75bc6db5e74533f59b8c7",
  "evals/results/p04-targeted-20261112-v1/report.json": "d359cc5f618c73ef66e716bb90f1ad573ad2096b6942c2d1f6331c92e7ef05ad"
}
```

### 限制与下一依赖

- 此为owner记忆纠正API的完整局部工程交付。没有Memory前端、自动信念/人格/关系修订、完整隐私删除、备份恢复或真实模型行为验收；不将其计作完整M2完成。
- 旧人物断言保留其历史事件，当前版本不再投影；新记忆必须以新来源重新形成断言。旧索引历史行未物理删除，版本/摘要失配使其不能继续作为当前有效索引；后续worker按既有策略重新建立。
- 本窗口只新增三个获准源码/测试文件及本handoff。公共module/route/key/version可见性由主线整合，权威文档仍由主线独占维护。
- 主线可据最终SHA接受下个候选叠加；P03需显式把 `--test memory_revisions -- --ignored` 纳入真实DB矩阵，普通cargo test的ignored不算执行。P01在固定新版本上验证实际认知纠错语义；不能引用本fixture当行为通过。
- 已运行检查全部通过，无本轮已知失败。冻结本任务最终源码，后续继续任务由主线分配，不自主争改他人文件或扩展领域。
