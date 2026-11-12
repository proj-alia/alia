# P00-M3-RUNTIME-PLAN｜真实发展运行接入任务卡

| 字段 | r1 |
| --- | --- |
| plan task / revision | P00-M3-RUNTIME-PLAN / 1 |
| plan owner / 技术裁决 | P00 / Root；本方案待 Root 审查，不是实施释放 |
| 建议实施 owner | Root；下列按序有界切片一次仅释放一项，不重复当前社会写入、兴趣或备份任务 |
| 非作者 reviewer | P04：发展状态、事务与试用；P05：来源、人格投影及恢复。P01补行为判据，P03做固定候选工程验证 |
| 基线 | 已接受 P04-10、P05-07 与 canonical0028/0029；共享运行时须由 Root 冻结精确后继，不借旧报告覆盖 live 代码 |
| 增量 | 首版：定位缺失调用链、接口冲突和最小接入顺序；不重建发展架构 |

## 现有入口与真正缺口

| 已有代码／数据契约 | 已做到 | 尚未接通 |
| --- | --- | --- |
| `DevelopmentDraft`：target/before_version/proposed_value/proposed_description/evidence_ids/counterevidence_ids/validation_plan/deadline；`TargetSnapshot`含目标值、版本、head、scope | `DevelopmentCandidate::prepare`按四个人格路径或独立 task_strategy 类型校验，完整 PersonaSeed 再验证 | 模型只有自然语言意图，未获得真实 target/head/catalog，不能细化后直接覆盖 persona |
| `Store::propose_development(key,ExpectedState,manifest,proposer_id,&draft)` | 真实 proposed、初始 head、来源、步骤、回执 | `DecisionProposal::parse`仍拒绝所有非空 development；model规则仍要求 null；`commit_decision`无发展原子写入 |
| `Store::transition_development(...,id,expected_development_version,&DevelopmentCommand)` | StartTrial/RecordValidation/Accept/Reject/Expire/Revert；从真实步骤严格重建，accept/revert实际应用单目标；私有 token 不可从模型反序列化 | 无生产调度者驱动 trial/过期/验证/接受；task_strategy虽有持久值，尚不参与实际对应任务认知 |
| `TrustedValidator::{authorize_trial,review}`、`Development::{start_trial,record_validation,accept}` | 绑定候选、trial、前身、支持/反证、案例及结果；跨情境路线至少两个不同 context，重大事件路线独立处理，不机械要求三次试用 | 没有真实受控试用上下文；已有 accepted 测试不能证明 trial 改变了认知输入或产生合理成长 |
| 0028 的 `development_verifiers/observations/review_records/review_sources`；adapter私有 `verifier/observation/review_proof` | 验证 system注册、直接观察事件、实际trial、reviewer、完整来源集、scenario/counter结果；verifier仅 human/measurement | 搜索生产 `src/` 未找到这些注册／观察／评审记录的 INSERT 写入器；现有 SQL INSERT 在测试夹具中，不能当生产评估器 |
| `privacy_development::verify_development_projection_in` 与 `block_development_in` | 删除/导出路径核查 seed/birth、当前head、修订及实际评审依据；删除先重建人格，新generation与旧日志隔离 | `model_context`当前复制 persona；`public_state`、`console_persona`直接投影当前persona，未接发展head来源；预留/派发也没有发展上下文校验 |

源码依据：`src/domain/development.rs`、`src/adapters/development.rs`、`src/domain/cognition.rs`、`schemas/decision.schema.json`、`src/adapters/{context,scheduling,decisions,api,console,privacy_development}.rs`、`src/runtime/{execution,model,review,worker}.rs`。这是代码链路检查，没有新增运行测试或复现泄漏的声明。

## 释放前必须裁决的接口

1. **意图→值**：模型schema的development缺 proposed_value，before_version还允许0；存储要求正目标版本及严格类型值，task_strategy必须实际goal/task scope。Root冻结细化输入/输出与错误；不靠字符串直接赋值，也不把全局版本当目标版本。若增加细化模型调用，走原预留/派发/记账，不提高预算，工程阶段仅fixture。
2. **兴趣兼容**：schema/CONTRACTS列 persona.interests，`DevelopmentTarget`与0028没有它；P04-11正在独立实现稳定兴趣修订。Root选择兼容映射／协议演进并同步schema、示例、验收；保留兴趣能力及稳定ID，不为对齐删除能力、不新增一份人格兴趣副本。本计划不改P04-11接口。
3. **真实独立性**：当前域接受 IndependentHuman/IndependentMeasurement，不存在 IndependentModel。不能把同一作者换UUID、另一次生成调用或自评分写成measurement“已通过”；Root明确可信执行器、独立判据／评审身份及语义未验证的表示。无法证明则 Unverified，不自动接受，也不把每次发展改为请求用户批准。
4. **事务边界**：现有两个public Store命令自开写事务并推进state。模型提案与回复原子提交需抽共用in-tx校验/持久内核；不得在已持个体行锁的 `commit_decision` 中调用另开事务的Store方法。一次决策的版本推进、事件、通知及回执由Root统一，失败整体回滚，已发生费用不退款。

## 输入输出与依赖顺序（供 Root 拆成唯一实施卡）

| 顺序／有界范围 | 输入 → 输出与结束边界 |
| --- | --- |
| A：当前人格来源与类型化提案 | 已接受head/实际persona、当前run/目标scope、原事件/完整反证 → 来源可验证的当前target视图及类型化candidate；向context/公开读取绑定当前发展head/revision，预留/派发/提交复查。提案与回复同事务只产生 proposed，不宣称已改变人格。此项可先实施，不等后续trial或收费验收。 |
| B：可信记录写入与trial执行 | 真实注册身份、实际候选/StartTrial事件、固定case与判据、实际run/执行回执 → 原子写verifier注册、observations及review_records/sources对应的事件和FK；同身份幂等、来源复验。试用仅在明确受控run/scope的认知副本应用candidate，持久persona与公开current不变；实际请求/结果绑定候选、trial、基线head、scope及判据版本。 |
| C：独立验证、接受及恢复 | B实际观察、完整支持/反证与独立评审 → RecordValidation；满足原域条件才Accept，受控调度处理Unverified/失败/到期。accepted persona进入以后普通认知；accepted strategy仅在匹配goal/task使用。重启重建trial/待处理工作与来源；拒绝、过期、撤回、来源移除或head冲突后不能继续试用。 |

A不让identity派生于私人发展来源；保留birth/身份连续性。上下文应带实际使用persona/strategy的当前head来源，不能只塞cached persona；超出既有容量显式失败或按既定完整条目策略处理，不截掉反证却保留结论。public读取在同一快照核对实际投影和可见性，并沿现有隐私generation返回屏障；不能顺带恢复旧removed对象。

GET/detail/owner revert沿已提交 [P00-M3-HTTP-01 r2](P00-M3-HTTP-01-r2.md)独立接入，不等A—C全部结束；其中 `has_more`/全局分页兼容仍待Root裁决。P02完整试用/验证界面依赖B/C可信记录；P03可先准备集成判据。P04-11、P05-08与Root当前cached receipt修复照常，不能由本卡重新调度或重跑。

## 新链路的失败与验收（不重复旧已过领域测试）

- **A**：通过实际模型fixture→原账目路径→prepare→commit，仅对应目标成为proposed；typed provider直构、未知/受保护路径、错head/scope、理由偷渡权限均拒绝。事务后段故障无部分reply/candidate/outbox，原人格/权限不变。
- **B**：同一case实际对照base/trial发送内容，证明候选值真的影响指定请求且其他run/current persona不变；仅存trial状态或提示词说明不算。观察/审查由真实写入器产生，不用测试直接INSERT替代；假注册、他人trial、重用结果、缺反证搜索及作者自证均拒绝。
- **C**：至少一条真实执行器→观察→独立判据/评审→RecordValidation→Accept→下一次读取/调用→Revert闭环；另验证失败/未验证/到期/竞争head不接受，已接受策略确实用于对应scope。工程fixture闭环只证明接线，成长语义仍未验证。
- **贯穿**：trial在预留后/派发前失效、祖先来源撤销且通用边缺失、缓存回执重放、崩溃重启、旧备份日志重放及新generation保持来源屏障。复用Root现有缓存回执修复，仅补新增development引用组合，不重复整个旧套件。已派发结果未知先恢复账目，不自动再次收费。
- 按切片做定向Rust/Clippy、实际PG/HTTP及相关浏览器；固定候选由P03统一完整矩阵，独立审查后改源重新确认。工程阶段无新增收费模型调用；100条检索、60×3行为、20组ABC（C≥16）及14真实日历日不由此补判通过。

交接必须绑定task/revision、唯一owner/reviewer、实施/接口基线、实际命令和退出码、审查发现/处置、失败/not_run、资源清理及后继依赖。Root接受本方案后再冻结精确文件范围和有界实施卡；本稿没有自批或派工。
