# ALIA-P05-07 — 发展来源隐私与导出

Status: **READY，support_schema=29**。现有 P05 Goal 的有界代码切片；P05-06/schema27 接受副本和所有历史失败保留。本切片支持实际 canonical0001..0029，未编辑 P04、worker、HTTP、模型提示词、公共注册或权威规范。无收费调用、部署或生产个体操作。

## 交付行为

- begin 在同一事务先阻断实际闭包，给受影响 target/candidate/revision 标记 `provenance_removed`，从 immutable seed 与仍可证明的各路径当前修订重建 persona，之后才提交202所用回执及重新 ready。保持身份、独立路径、权限、配置和真实费用；不拿私密 before_value 回填，不把 removed 假装成 rejected/reverted。
- 实际 head 值必须等同 actual revision.after_value/version，candidate、转换步骤、trial、独立 review、实际 source 集合/观察/反证结果都保留一致绑定。无 revision 的 head 必须是 seed 目标值（strategy null），版本为同target/scope历史head/revision最大+1。完整 PersonaSeed 校验、相邻 seed/hash 与真实最早 system/direct_observation 出生事件绑定；缺证据拒绝整个投影。
- purge 清除发展 draft/before/after、步骤命令/验证JSON、审查评估、观察正文和可移除关联，保留 opaque FK tombstone、版本及原六态；删除日志重放和 clean 再核验 persona，包括已 completed 的恢复数据及空日志恢复。
- 新一代 head 使用新 object 与递增基线版本，不解封旧 ID，也不把旧 head 作为来源。实际 P04-10 propose→trial→review→accept→新 Store/旧 journal replay→export 通过，旧日志不删除新经历。
- 实际 owner 关系删除后先匿名化旧逻辑key，创建新的空白关系 object（旧 object 永久blocked）；后续合法消息可续接，不引用旧私有 subject 为来源，不恢复旧历史，也不授予 verifier/权限。
- 根 `reference_graph_sql` 增加仅两条 actual FK 来源方向：steps.event_id→development_id、review_records.event_id→development_id。删除独立验证结果也会使对应 persona 修订失效，即使通用/专用派生边缺失。签名和 root 其余提取逻辑保持；按 root 后续明确授权格式化并冻结整个 deletion.rs。
- 内存导出增加实际十张发展表，完整56表/469列；包含所有合法历史，排除被删除行与关联。与 root 共用 actual graph，保留既有结构化脱敏和原始计费；未知表/列/对象kind拒绝，导出同时校验已知约束指纹。仍为64MiB紧凑UTF8 JSON/100000行每表，整份拒绝、不截断。投影检查不继承删除预览10000祖先上限；10001合法来源实际通过。关联标识仅在对应实际属主对象存在时参与来源图。

## 内部接口与主线整合

新 `src/adapters/privacy_development.rs`，四个 `pub(super) async` 事务函数：

- `block_development_in(tx: &mut Transaction<'_, Postgres>, individual: Uuid, ids: &[Uuid]) -> Result<(), StoreError>`：objects 已阻断后、同一TX内标移除并重建投影/关系空档。
- `erase_development_in(tx: &mut Transaction<'_, Postgres>, individual: Uuid, ids: &[Uuid]) -> Result<(), StoreError>`：物理清理与投影再验。
- `development_privacy_clean_in(tx: &mut Transaction<'_, Postgres>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError>`：实际清理与投影一致性，缺证明返回错误。
- `verify_development_projection_in(tx: &mut Transaction<'_, Postgres>, individual: Uuid) -> Result<(), StoreError>`：只读投影核验，export与replay使用。

这些函数不提交、不自行开放ready，不调用模型/网络/文件。Store::export_individual() 及 IndividualExport::to_json_bytes() 沿用06 typed API/format，无传输或下载文件注册。

Root 最小注册：adapters `mod privacy_development; mod export;`、domain `pub mod export;`，并纳入 canonical29。主线保留 runtime/HTTP/模型context与全局整合所有权。canonical `migrations/0029_development_provenance_removal.sql` 已由 root 采用（仅首行注释与本任务初始draft不同），仅应用实际canonical29，不叠加私有draft。三个 provenance_removed 布尔列与非removed target/scope部分唯一索引，不改变六态。

P04-10的live槽、removed读写屏障和新head初始版本由 P04 自有 adapter 实现；本任务未改其源码。最终冻结依赖摘要见报告；后续 P04 变化不能冒用此结果。

## 固定证据与命令结果

[总报告](../../evals/results/p05-development-privacy-summary-v1.json) SHA256 `5c48098cd49f57e167426173eff955f09a2cf8b2250058e9bf6507d46e1e9b63`；[最终定向原报告](../../evals/results/p05-development-privacy-local-v6/report.json) 包含实际命令/退出值、完整构建源与派生注册摘要、独立DB资源身份、source_unchanged/cleanup和锁等待。

| 实际命令/范围 | 退出与结果 |
| --- | --- |
| cargo test --locked --test export export_json_actual_serializer_duplicate_fields_utf8_and_limits -- --test-threads=1 | exit0，1 passed |
| cargo test --locked --test development_privacy -- --ignored --test-threads=1 | exit0，13 passed/0 failed/0 ignored，包含先前失败的新head/来源/版本/大来源图反例 |
| cargo test --locked --test export -- --ignored --test-threads=1 | exit0，11 passed/0 ignored，实际64MiB/100001行拒绝、并发删除/费用写入/恢复generation等原用例 |
| cargo test --locked --test privacy_operations -- --ignored --test-threads=1 | exit0，13 passed/0 ignored，旧日志/清理/回滚/账本/真实依赖回归 |
| cargo clippy --locked --lib --test development_privacy -- -D warnings | exit0 |
| rustfmt --edition 2024 --check 五项最终自有源（含获授权deletion.rs） | exit0 |
| .venv/bin/python scripts/check_phase0.py | exit0，6schemas/60closed_objects/13examples/68negative_cases/38links，见[M0原记录](../../evals/results/p05-development-privacy-m0-v1.json) |

共1普通 + **37项不同真实 PostgreSQL**；没有将默认 ignored 当通过，没有以旧版本报告代替当前通过。最终父驱动全程持 `/tmp/alia-project-heavy.lock` 至自有容器/卷/临时目录清理，独占 `work/p05-development-privacy/target`，RUST_TEST_THREADS=1，无 .env/供应商原始日志。source_unchanged与cleanup实际为true。

[实际canonical29 schema应用](../../evals/results/p05-development-schema29canonical-v1.json) 与清理通过：导出 columns+constraints FP `e0642b8f8ecb874ccc8b3b2497383d84338785d1955de3a06928a76491870848`，privacy columns FP `65922f299c07dc39e37b085b1e0a20d057bef26c2ac8d2511d400602a41cfa44`。未知表/列不静默忽略；canonical29迁移哈希绑定该实际SQL记录。

| 最终自有源 | SHA256 |
| --- | --- |
| src/adapters/privacy_development.rs | 0d2cff4d5e0641c31ca3e79382a045d45e8c2dcf442b1cf7fe875eed5e8d4cfe |
| src/adapters/privacy_operations.rs | 4bd5c0e64a83191b7d278f3bb027b77343c90963fcc432c557122bb005972dfb |
| src/adapters/deletion.rs | 4b928a86a148bc9f16b258bf5311f19ecd1979f6597d1f1ec20b5288d07b7933 |
| src/adapters/export.rs | d29863c5483e0bab99fbed2fc3e5d25881a5ac68464f65a3c2b70e0699c77cd7 |
| tests/development_privacy.rs | 46d8ffe705aad8df3fb8279de980199e790956821d69f1c85ca8339a96072756 |

既有 tests/export.rs 仅46→56表数断言同步；其最终 SHA、P04/domain 依赖与实际29迁移都保存在 [final-inputs manifest](../../evals/results/p05-development-privacy-final-inputs-v1/manifest.json)（38文件独立源副本，SHA `ed4967b593b1624d187caf1d8d0d8d6af7a222260443b48305dcfbfdb38bb8a5`）。独立固定基线为 root-history-context-v1 的381源，叠加不可变 privacy primitives/accepted05 与 root非创建式journal副本；公共模块注册仅派生到测试副本，未改live入口。

[固定重放驱动](../../evals/results/p05-development-privacy-runner-v1.py)：`python3 evals/results/p05-development-privacy-runner-v1.py --extra-regression --output /absolute/new/result-directory`。出版副本改为读取上述已保存inputs，避免未来live P04/P05源码变化；该源路径绑定改动仅AST/静态核验，未因发布脚本再重跑37项。依赖当前 scripts/qa/common.py/database.py，其最终观察摘要列于总报告，不承诺自动恢复未来QA辅助模块。自有TCP readiness等待规避临时初始化socket竞态；shared helper归P03未编辑。

## 范围与下一依赖

这是已执行的 schema29 内部代码与定向验证，不是完整M5工程、行为质量、HTTP/浏览器、下载/备份/恢复发布或14天观察通过。模型调用0、部署0、生产初始化0。Structured operational字段按明确策略剔除，人类正文/外部来源原样保留，不猜测自然语言秘密。

Root整合模块和固定候选；P03执行完整工程/HTTP/浏览器矩阵；P01依原门槛独立行为验收。受管导出文件注册/删除失效、鉴权下载和备份恢复仍属主线后继，不在这里自动启用。主线可继续下一独立切片，不必等本窗口重复完整矩阵。

## 历史失败与逐步记录（原样保留）

Execution evidence (preserved): owner-baseline-v1 failed before product execution because the new snapshot runner registered export modules without copying them; no owner result inferred. Fixed snapshot registration for baseline-v2. schema29draft-v1/v2 failed at first SQL wrapper; the retained reports contain only RuntimeError, so their exact SQL reason is not proved. Owned cleanup passed. v3 actually applied0001..28 plus proposed29 and passed; v4 adds explicit synthetic SQL stderr evidence if failure recurs.

Actual baseline owner-v2: 0 passed/1 failed, new legal submit returned Database(RowNotFound) after real delete+purge; not a speculative defect. Actual schema29draft-v3 passed with cleanup; export FP `e0642b8f8ecb874ccc8b3b2497383d84338785d1955de3a06928a76491870848`, privacy column FP `65922f299c07dc39e37b085b1e0a20d057bef26c2ac8d2511d400602a41cfa44`. Canonical29 adoption still pending.

Schema-v4 captured actual infrastructure race: Docker image temporary init socket disappeared before first psql (no private SQL/body in error). `database.provision` currently polls socket pg_isready, so a fast schema consumer can observe the temporary init server. Own runner now waits `pg_isready -h127.0.0.1` for final TCP server. P03 owns any shared helper correction; no shared source edited. v3 successful migration proof remains valid; v1/v2 exact error unrecorded.

local-v1 actual PG4 passed/1 failed: owner renewal, completed restore, late rollback/replay, failclosed proofs passed; the full-family assertion failed because its fixture deleted *all* dependency rows and then required a nonempty legal dependency table. Changed fixture to remove only private-family deps and retain independent family rows, keeping the assertion. Removed unused copied owner helper (strict Clippy failure). fmt failure was root-owned unformatted reference_graph extraction; local format check now excludes that unchanged shared file rather than modifying its ownership. Added actual strategy and next-generation cases; P04 command adapter must support the latter.

local-v2 actual PG6 passed/1 failed, strictClippy/fmt0, source+cleanup true. Remaining failure is actual P04 new-generation propose NotFound: current adapter still selects removed historical head, cannot create live head with max(history)+1. No P04 source edited. The internal migration/API draft is saved above for root adoption; root/P04 must integrate it before declaring this slice fully ready. Validation-source FK counterexample now being executed separately.

Actual FK negative `p05-development-fk-baseline-v1`: observed validation source removed with derivations+development_dependencies absent, begin returned but persona kept applied0.31 instead of seed0.8 (0/1 fail). Added only two actual source→candidate FK reverse paths in root-owned development reference fragment: steps.event_id→development_id and review_records.event_id→development_id; signature/root extraction untouched. Shared export uses same graph, no divergent policy. No old05 proof covers this change.

FK-fixed-v1 actual targeted PG1 passed/0 ignored; strictClippy/fmt0, source/cleanup true. Both two-line FK additions are necessary: validation observation→review event→candidate→revision→head now clears persona before begin returns despite missing general/development dependency edges.

Root已正式采用 canonical `migrations/0029_development_provenance_removal.sql`（与draft仅首行注释不同），正式P04-10兼容已释放。07后续快照改用实际canonical29，另跑实际schema应用绑定新migration哈希，不重复叠加私有draft29。local-v3实际8新PG+11旧导出PG+13旧删除控制器PG全部通过，Clippy/fmt/source/cleanup真；唯一新代head用例显式未运行（旧失败保留，依赖P04-10）。正在补独立“审查source关联缺失”反例，避免只因缓存head/revision吻合就信任无证据persona。

Missing-proof-baseline-v1 actual1 fail: removing review_source associations still allowed cached persona export. Added exact nonempty validation-command source set = actual review_source set, stored assessment equality and actual observation/counter-result binding; no semantic/model judgment shortcut. Canonical29 actual application proof passed with TCP readiness and cleanup; structural FP matches earlier draft while migration hash now binds real canonical file. P04-10 compatibility source now saved (IMPLEMENTING); next fixed snapshot takes its exact hash and reruns new generation rather than excluding it.

local-v4 uses actual canonical29/P04-10 source (exact dependency hashes in report), ordinary1 + all10 new PG + original export11 PG + original deletion controller13 PG passed, strictClippy/fmt0, source/cleanup true. New-generation actual propose→trial→review→accept→new Store/replay→export now passed; prior NotFound failure retained. Source-set proof actual negative also fixed. Final projection alignment adds explicit no-revision baseline version=max prior head/revision+1 and immutable seed/birth binding checks, tested independently; no broader product scope.

Anchor-baseline-v1实际1失败（无revision head被篡改99仍导出）；修复投影与P04同契约：版本=同target/scope其他全部head及revision最大+1，seed/hash必须匹配真实最早system/direct_observation出生事件。额外核验draft证据/反证集合=实际关联，审查实际结果存在。Root明确授权07最终格式化deletion.rs；现在纳入定向fmt和最终源码哈希，之前两条FK失败/修复证据保留，不扩大完整矩阵。

local-v5 all12 new PG +11 original export+13 original deletion PG (36 distinct) +1ordinary passed, Clippy/fmt includingdeletion/source/cleanup true. Additional explicit export ancestry-boundary baseline1 failed on10001 owned source events despite <64MiB/<100000 per table. Removed the privacy projection helper arbitrary10000 cap (preview deletion cap unchanged), and aligned owned_refs filtering with root graph: opaque nonobject correlation IDs are not treated as missing required sources. New fixed case also checks this legal metadata distinction. Final full directed v6 covers the changed helper, not reusing v5 as final proof.
