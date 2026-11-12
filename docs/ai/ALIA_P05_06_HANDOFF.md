# ALIA-P05-06 — 有界个人数据内存导出

Status: **READY，support_schema=27**。本任务为现有 P05 Goal 的代码切片，P05-01..05 已接受；未新增任务窗口，未编辑旧05五源、其他窗口文件或权威设计。交付范围仅 `src/domain/export.rs`、`src/adapters/export.rs`、`tests/export.rs`、本交接及独立证据。完整工程回归归 P03，行为验收归 P01，领域和发布裁决归 root。

## 行为与接口

- `pub async Store::export_individual(&self) -> Result<IndividualExport, StoreError>`：构建纯内存个人导出，无文件、网络、模型调用或生产路径。
- `IndividualExport` 字段：format、individual_id、state_version、privacy_epoch、observed_at、schema_fingerprint、`BTreeMap<String, Vec<Value>>` tables、`Vec<ExportOmission>` omissions。`ExportOmission` 为 table、columns、reason。
- 格式 `alia-individual-export/1`。`IndividualExport::to_json_bytes()` 实际有界序列化并校验重复键/尾随输入；UTF8 紧凑 JSON 总上限64MiB，每表导出行上限100000；任一超限整份拒绝，不截断。`json_size` 计数不保留第二份正文；`validate_json_document` 只验证，不导入数据库。
- 按0001..0027实际46表/398列及约束指纹逐表声明，保留可见的旧版/撤回/退休历史、原始账本与未知用量 null；按实际主键排序，文本主键使用 C 排序。明确标识排除表与列，不虚构发展数据占位。
- 排除原始命令键、请求/日志键关联、租约、部署连接、结构化凭证和供应商原始错误；删除审计另去除已删除目标/对象/来源ID。人类文本与外部资料出处保持原文，不用关键词猜测语义秘密。
- 同属主和真实 FK/来源/manifest/历史前身/稳定兴趣映射闭包控制输出；缺少 derivations 边也不能导出私密后代。已完成删除账本亦提供屏障种子；pending 隐私任务拒绝整份输出。原始计费行保留，但隐藏运行关联移除，不重算费用。
- 正文为 REPEATABLE READ READ ONLY 快照；实际序列化之后重新检查 schema、身份/版本/epoch、隐私待办、所有表的只读行修订聚合、进程 generation/ready。实际费用写入即使不提升认知版本也会被检测；恢复重新 ready 不能放行旧 generation。

Root 最小注册为 domain `pub mod export;`、adapters `mod export;`，共享入口仍由 root 维护。截至交接只读检查未发现这两个注册，不自行编辑入口。本任务不接生产下载、导出文件注册、删除失效、备份、恢复或导入。

## 固定基线与源码

基线为 `root-history-context-v1/source` 的381项哈希核验清单，manifest `5777aa7b26810a4ef06044bbc15c49d1029626d913fa026b1a10706e5e7b742c`。叠加不可变 `p05-privacy-root-primitives-v1` 三源及 accepted-slices/p05-05 五源，逐项验证旧副本未变；不是当前不断变化的 checkout 全量构建。快照中仅追加 privacy/export 模块及 PrivacyUnavailable 静态错误映射，无 HTTP/启动验收声明。

| 自有源 | SHA256 |
| --- | --- |
| src/domain/export.rs | 070c733cf1d943ab8a4f2f5d49a42cca92bc7e0ed0bed1d5a55fcff31015714b |
| src/adapters/export.rs | 186fd02177cc41cd775da42033de7a1bec81811859b908a3edd0ed4230bf9739 |
| tests/export.rs | 605db58a89ec944aef58fb71b32c87bd6f8de3108837523e6b3e60ecb4a3a533 |

实际0001..27逐迁移应用与清理见 [schema证据](../../evals/results/p05-export-schema-v1.json)，schema fingerprint `10da3c5ad232b9ad9ae554f6c159dd14f3dc36a2fb64223870d1395d7ba6b276`。可见性引用片段逐字来自 accepted05 deletion.rs 的5段，不修改它，也不继承删除预览10000对象上限；root 可后续统一查询构造。

## 实际验证与证据

总报告 [p05-export-summary-v1.json](../../evals/results/p05-export-summary-v1.json)，SHA256 `f5c5536d1422805ae605b0627514c6453e63dd6d00b04db7c656644d22de0c41`。其中绑定每份报告/日志、源清单、配置/迁移、数据库镜像与资源身份、实际命令和清理。

| 固定运行 | 实际结果 |
| --- | --- |
| [v4](../../evals/results/p05-export-local-v4/report.json) | 普通序列化1 passed；真实PG9 passed/0 failed/0 ignored；严格Clippy/fmt exit0，源码稳定，清理通过 |
| [v6](../../evals/results/p05-export-local-v6/report.json) | 新增 generation恢复、隐藏运行保留账本两项真实PG passed/0 ignored；严格Clippy/fmt exit0，源码稳定，清理通过 |
| [v7](../../evals/results/p05-export-local-v7/report.json) | generation用例新增 paused/terminated/archived 完整历史检查，真实PG1 passed/0 ignored；严格Clippy/fmt exit0，源码稳定，清理通过 |
| 当前M0 | `.venv/bin/python scripts/check_phase0.py` exit0：6 schemas、53 closed objects、13 examples、68 negatives、111 traceability、136 task edges |

合计1项普通与11项不同真实PG用例。没有把 v7 已重跑的 generation 重复计数。生产两源在 v4/v6/v7 与当前逐字一致；当前测试前缀可精确还原 v4 文件 SHA，移除唯一生命周期新增块可精确还原 v6 测试 SHA，因此复用未受影响用例证据，不重跑64MiB/100001行用例。所有PG通过均显式 `--ignored --test-threads=1` 实际执行，未把默认 ignored 当通过。

覆盖真实删除 pending/completed、无派生边的反证/关系/兴趣源阻断、异Store真实并发删除、无state_version变化的实际费用写入、恢复ready后的generation变化、各非active生命周期、重启排序/历史稳定、只读全表指纹、结构化脱敏/保留自然文本、未知列/约束/下划线表拒绝、实际64MiB正文与100001行拒绝、隐藏run时精确保留charged48/unknown/null原账本。

可复现入口 [p05-export-local-runner.py](../../evals/results/p05-export-local-runner.py)：`python3 evals/results/p05-export-local-runner.py --output /absolute/new/report-directory`；会核验并组合固定基线、隐私副本、27迁移和三项自有源。当前 `scripts/qa/common.py`、`database.py` 为依赖，其哈希已记总报告；驱动不自动恢复以后变化的辅助模块。整个父驱动直至专属容器/卷/临时目录清理均持 `/tmp/alia-project-heavy.lock`，300秒子进程上限，清洁环境，不读取 .env。

## 失败、限制与后继依赖

保留 v1编译失败、v2/v3真实缺陷/无效fixture/Clippy失败、v5新fixture自锁中断，以下历史不覆盖。已修复无全局版本变化的费用快照漏检，以及删除审计目标关联残留。v5仅精确中断自有cargo测试进程组，父驱动继续持锁清理，无生产/模型中断。

**support_schema=27**：root 已新增 canonical 0028发展迁移，本构建器会因真实schema不匹配整份拒绝；本次未应用28/未声称兼容28，不删除或隐藏live28来制造完整导出。后继由 root 冻结发展语义后同步删除/导出的 source、dependency、head、revision、verifier、validation 等实际数据。accepted发展影响当前persona，来源闭包删除后必须从 immutable seed 按未阻断修订重建，不只清历史留私密persona，也不盲用带删除来源的 before 值。待 root 给出清理API后实施下一切片，当前不并行改旧05五源。

当前基线使用已接受05原journal API，未包含 root 后增 `open_existing` 的启动/HTTP/runtime整合；本地通过不能冒充该后续切片验证。调用者日后持有导出对象或注册下载文件仍须遵守新的隐私失效屏障，内存返回不构成长时下载授权。完整工程矩阵由 P03 运行；行为结论由 P01 独立评审。真实模型调用0、部署0、生产个体初始化0；不宣称M5/完整删除备份恢复或14天观察完成。

## 原始执行记录

v1 actual build failure preserved: String::replace replacement argument used char instead of &str in structured key normalizer; no domain/PG test ran. Fixed own compile error. No production endpoint/backup/export pass claimed. v2 will collect full directed cases, including an explicit concurrent billing update that does not raise global version so the post-capture guard requirement is tested rather than assumed.

v2 actual domain PASS; PG6passed/3failed/0ignored; Clippy unused own import failure. Actual bugs were an undetected concurrent billing write (no global state bump) and erased target ID left in privacy audit payload. Credential fixture also violated existing64char body_digest check before export. A patch anchor mismatch prevented source save, so v3 reran the same failed snapshot; both reports remain failed, not counted as new proof. Fixed sources now save via checked step. Added constant-memory per-table MVCC revision aggregates (count/xmin sum/two PK-xmin checksums) and final fresh readonly schema/row/version/epoch/generation checks after bounded serialization. These internal guard hashes never enter archive; no shared counters/migrations changed. Privacy audit erased-target/object/source associations are omitted explicitly, credential fixture uses valid digest, unused import removed. v4 runs actual directed suite.

v4 actual domain PASS and all9PG PASS/0ignored; strictClippy/fmt/source/cleanup PASS. The original billing concurrency and privacy-audit association cases now pass; size64MiB and100001-row rejection are actual PG cases, not simulated counters. Added two final isolated cases without production edits: same-clone generation invalidation after actual empty-journal recovery reopens readiness (database fingerprints unchanged), and deletion-hidden run association while preserving exact stored ledger48/unknownnull usage. v5 runs only these2 +strictClippy/fmt; v4 full suite reused only after final domain/adapter source hashes match. No paid calls/HTTP/persistence/backup claim.

v5 extra fixture self-lock: generation test attempted whole-table fingerprint while its own separate connection still held contact_windows ACCESS EXCLUSIVE. Confirmed only the private cargo test process group from its exact /proc command and interrupted that group; parent kept global lock through actual PG/container/volume/private temp cleanup, database exit-2, Clippy/fmt0. No product failure/pass inferred; other filtered case was not reached. Fixed fixture release-before-fingerprint order; v6 reruns both newcases only. Original v5 partial log/report retained, no production process/model interrupted.

v6 both extra actualPG cases PASS/0ignored, Clippy/fmt/source/cleanup PASS: actual same-clone generation recovery reopened readiness while DB fingerprints stayed unchanged, yet old export was refused; hidden run/source IDs stayed absent while stored charged48/usage_unknown/null actual_total remained exact. Final generation case now additionally checks paused/terminated/archived identities still export all legal history before recovery, satisfying ordinary archived-state coverage; v7 reruns this single affected case. Export domain/adapter production sources remain byte-for-byte v4 pass, so no large-body/100001-row repeat is needed.
