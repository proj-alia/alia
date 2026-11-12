# ALIA-P04-11｜兴趣完整上下文与同事务模型修订

状态：**r2工程就绪 / 等待非作者P05及Root审查**。7项新PG、6项既有PG、1项内部helper PG及5项领域测试通过，定向编译/严格Clippy/fmt/固定源M0全部exit0，固定源/QA辅助摘要稳定、独立PG清理通过，0模型。仅改domain/interests.rs、adapters/interests.rs、新tests/interest_context.rs及本交接；未改live共享ModelContext/schema/decisions/reviewer/context。10与09B接受副本保留。

typed `InterestContext {items,has_more}` 与 `CurrentInterestView`已保存。view含派发九字段，另显式`revisions:Vec<InterestRevisionView>`保留完整旧before/after/version/status/basis/event/previous链；不是只拿IDs冒称已提供历史。root接可选ModelContext.interest_context时直接使用此类型；现有InterestRef仅作active动机目录，retired只放typed可见项供合法恢复，不作为新目标来源。记录事实/推断来源标签继续保留。

计划实际helper为pub async enrich_context(tx,&mut ModelContext)与pub async apply_model_changes(tx,&ModelContext,&[InterestDraft],decision,version)->Vec<Uuid>，仅受信任Rust/DB调用，无HTTP注册或不可信Deserialize写入口。采用pub可供独立integration test验证实际helper（调用者仍需真实DB事务），root内部使用不变。typed context最多32完整项，每项1024来源，全ModelContext96KiB；超限不截旧链/事件依据，整项不带入并标has_more。基线本身连空标记都超96KiB时明确拒绝。

下一步提取旧trusted写入为validate/insert内核，trusted与模型同TX复用；模型仅当前typed已观察id/object/version/完整链可修订，new程序分配，无变化零写。固定副本最小追加ModelContext可选字段与必要构造处None及出生helper挂钩，准确记录私有bridge，不假称live runtime已接通。实际通过范围见最终报告；作者不作非作者审查或里程碑通过声明。

## COLLAB-02 / 当前review边界

owner=P04；非作者reviewer=P05（隐私/删除边界）及Root接受。后续每revision记录源码/依赖/接口版本、审查结论/精确证据与delta；作者仅报告实际命令和就绪状态，不自批。相关代码变化须重新审查。当前review结论=pending；本次冻结revision=r2，精确三源/接口/依赖及delta见下文；无变化不重复转发/全矩阵。Root事务状态变更的非作者review任务收到具体固定差异/证据后由本窗口定向审查，不混同本切片作者自测。

Root已实际增加live ModelContext.interest_context及构造None，但未接helper。当前测试计划仍采用10/owner固定旧源再做最小private字段桥接，不复制正在变化的live cognition/social/schema；若切换为已带字段的新固定源会先检查避免重复shim。所有shared live文件由Root维护，P04未写。

## r2固定交付：owner / reviewers / 接口版本 / delta

owner=P04；reviewers=P05（隐私/删除）+Root接受；review_status=**pending_non_author_review**。本段作者仅声明工程就绪与实际证据，不自批。接口版本为本次Rust `interest-context/v1`（不是新增产品JSON版本字段）。Root事务非作者review任务ROOT-SOCIAL-WRITES-01/r3已收到，排在本切片交付后独立只读审查，不与11并行另开编码切片。

相对固定10源的delta：新增typed当前兴趣/完整修订链；按完整FK/来源图选择可见active与retired；提取旧trusted record_interest为共享validate_interest/insert_interest，旧严格输入、幂等/状态/advance逻辑保持；新增模型分支在caller TX中复查实际state/config/privacy、decision.accepted与run绑定，<=2项且不可重复改同logical ID，先校验全部再写；旧context_sources/active_object仅扩为pub供同库integration调用，签名和输入/拒绝行为不变。没有新HTTP接口、权限、模型调用或空模块。

最终三源：

| 文件 | SHA256 |
| --- | --- |
| src/domain/interests.rs | 9364e7bcd944050f113bb6ad1858b109f5ce469e6cd23ce736f64519e322ee23 |
| src/adapters/interests.rs | ae1c058a3f3e58a39835de634ae0750df26fdcc3a90480e03671782f54022533 |
| tests/interest_context.rs | ee2ab6ee7a566fc88d617dd1f53b744d740962b7e062db034d8a06507fd43c81 |

public helper精确签名（Tx=sqlx Transaction<Postgres>）：

- `enrich_context(tx:&mut Tx<'_>,context:&mut ModelContext)->Result<(),StoreError>`：只读，原调用者RR快照内执行；最多32项，按updated_at DESC/id稳定选取active/retired，每项最多1024完整provenance。view.revisions显式带全部before/after/version/status/basis/actualevent/previous链。每个basis事件原样带入c.events，provenance全入manifest；旧事件内容若与已选事件同ID不一致拒绝。超源数/整包实际字节预算则整项不带入并has_more，不裁历史/来源/原文；基础context连空标记都超96KiB则明确拒绝。legacy c.interests和interest类型goal_origin_catalog仅来自已完整带入active项，retired可观察恢复但不获新目标来源资格。
- `apply_model_changes(tx:&mut Tx<'_>,context:&ModelContext,changes:&[InterestDraft],decision:Uuid,version:i64)->Result<Vec<Uuid>,StoreError>`：空changes立即零写；非空要求actual active/state/privacy/config及version=state+1、实际decision.accepted/generated/alia且同owned run；现有修订必须在typed已观察条目匹配logical id/version/完整内容，并重载actual DB整包与观察值相等。new id/expected均None才由程序分配；返回**interest object IDs**，不是revision或逻辑字符串。保存当前兴趣/不改origin_kind/全部不可变revision/来源/interest.changed及decision关联边；不commit、不advance全局、不保存独立回执、不等待网络。caller继续拥有租约、预算、独立内容审查、整轮幂等/state.changed及TX rollback；本helper不替代它们。

初始seed兴趣从immutable seed而非当前persona数组按稳定legacy索引验证，完整PersonaSeed、digest和真实最早system/direct_observation birth.seed_digest一致；修订链第一before描述/状态须为出生值。experience第一before_version0，其后严格连续+1/前身/首尾值与当前row一致。revision事件实际kind/actor/source/object/logical/revision ID与其1..32实际声明来源一并校验，interest_sources累计全集必须等历史声明并集（seed另含birth）。Root稳定reference_graph_sql(tx)加真实修订event.payload声明来源，故删除通用边及旧source关联后，撤销的旧祖先仍阻断完整链；不把兴趣解释/本地保存当外部事实。

## 固定输入、私有bridge与实际命令

最终报告`evals/results/p04-interest-context-20261112-v2/report.json` SHA256 **6bb80c68b17d484f6dfdad2e940dd7148d37eeb4e5d319438497f91f1ce48f3c**；source-manifest SHA256 **c9fd1729c6010e8cacd8991862071c9037415bd51924c1d7dd31647bdb545ab9**。evidence.json绑定r2/owner/reviewers/pending状态、三个源码、真实测试二进制副本及实际计数。

基线为不变P04-10/v4清单d8acbd07d8534fe2fff06287fc0b0d7b2148762e4fa069ece99f76dacb51f416，逐文件比SHA后叠加owned三源。仅私有副本：domain/adapters登记interests，旧ModelContext加serde default/omitNone的Option<InterestContext>，context构造None/legacy active动机调用for_context并截32，出生事务接seed_interests。五项bridge的before/after SHA均在manifest/report；没有把它们写入live shared或声称root当前social/兴趣runtime已验。Root已另行加live字段，本次旧固定源补的是同一字段，不重复叠加最新live。

8项实际命令均exit0：

1. `cargo check --locked --test interest_context --test interests`：6.78秒。
2. `cargo test --locked --lib domain::interests::tests`：**5 passed/0 failed/0 ignored**，75 filtered out未运行。
3. `cargo test --locked --test interest_context -- --ignored`：**7 passed/0 failed/0 ignored/0 filtered**，实际6.87秒。
4. `cargo test --locked --test interests -- --ignored`：**6 passed/0 failed/0 ignored/0 filtered**，实际7.22秒；旧M27升级、错误owner/版本/来源/manifest、同键回执/晚期rollback、读无副作用等原断言均保留。
5. `cargo test --locked --lib adapters::interests::tests -- --ignored`：**1实际PG passed/0 failed/0 ignored**，79 filtered out未运行；出生幂等及33 active旧接口通过。
6. `cargo clippy --locked --lib --test interest_context --test interests -- -D warnings`：exit0，7.63秒。
7. owned三源rustfmt --check --edition2024：exit0。
8. 用现有.venv Python运行固定副本scripts/check_phase0.py：exit0，schemas5/closed49/examples10/negative56；不代替Root新interest_changes schema/packet审查或实际行为。

新7项实际PG：重命名→退休→新Store重启→按新依据恢复，logical/object ID与旧goal.origin_ids保持，完整4次revision及实际来源均在context；retired不出legacy/goal catalogue且actual active_object拒绝；RR read-only enrich及空apply全public表指纹不变；未观察ID/版本/事件、伪context、新id非法组合、全局/对象CAS、重复和超2项拒绝无写；new ID由程序分配；通用边与旧interest_source关联均移除后旧祖先撤销仍阻断，独立兴趣继续可见；late state.changed故障回滚decision+两项兴趣所有写入，成功时仅一次全局advance；1025额外来源整项省略、120KiB原事件整包省略无截文/留错manifest、超过32项has_more与完整shape均实测。

首轮v1：编译与5领域通过，7新PG中4过3失败，均在fixture创建新run时撞实际one_running_individual唯一索引，尚未进入相应后续断言；另有unused import warning，旧回归/Clippy/fmt/M0未运行。v2仅删除unused import、测试driver在开启下一合成run前将前一运行真实标committed，未改约束/验收判据；原失败及日志保留，最终14实际PG/5域与全部八命令通过。context/decision驱动明确是synthetic，无真实模型/审查语义通过声明。

资源按已授权workspace/独占target/p04-interest-context-v1/独立pgvector17容器持锁，provision及cleanup用database-admin锁，子进程继承FD/map；总排队约0.000019秒，cleanup/source_unchanged/owned_live_unchanged/qa_helpers_unchanged全部true，error=null。0真实/fixture模型、0模型费用、未部署或生产初始化，不重跑全矩阵。

测试产物副本：v2/bin/interest_context SHA5614e024187d1c66473510c6930462215da4a1710da685b4a1853c782ef81941；v2/bin/interests SHA33b9acc9b25628f3fa2992495b05a9b3578a7edd8614456f366cf307d2276e5f；v2/bin/library SHAeaa2d91f99d51bdee6bc22e215e43f2e37f9f8689eed18467348ef2ea5a3f02f。

## 审查与下一依赖

本r2交P05隐私跨审及Root接受；当前pending，无作者自批。P05重点核对缺边/缺关联旧来源、完整链事件、超限整包与不可复活privacy tombstone；Root重点核对共享TX接口、<=2 typed changes、调用顺序及active源再检查。相关三源变动须按COLLAB-02再审，不沿用本revision意见。

Root后续接enrich与apply、DecisionProposal.interest_changes/ReviewTarget::Interest和独立语义审查、同TX回执/新目标active_object复查；enrich须在实际请求计数/最终manifest冻结前，保留96KiB完整context检查。没有兴趣写入字段/审查目标/runtime注册/HTTP或浏览器验收声明。M3全链/实际行为/成长阈值及M1—M5完整目标未变。本窗口接下来按已指定固定r3审Root事务差异，不扩大P04-11代码范围。
