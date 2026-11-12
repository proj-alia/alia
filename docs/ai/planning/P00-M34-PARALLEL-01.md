# P00-M34-PARALLEL-01｜已释放并行任务的接口差异

revision: 5；2026-11-12；Root已采纳关键约束，未采纳的接口名字以协调页实际冻结为准。Root已释放M34并行实施；本卡不重新审批或阻塞已释放任务。

直接核对协调聊天原始用户消息 `01a1d0fb-fd64-7cc1-ab59-159746deba45`，要求剩余M3/M4运行闭环分给现有Goal并行实施。随后收到Root实际冻结通知，以下服从该通知，替换 [原运行卡](P00-M3-RUNTIME-PLAN-r1.md) 集中Root实施的归属建议；保留 [Astra边界复核](P00-M3-RUNTIME-REVIEW-r1.md) 的未解决项。当前人格/匹配策略来源、发展HTTP已交付，不重做。

## 已冻结归属与入口

| 窗口 | 唯一归属与完成边界 |
| --- | --- |
| P04 | `src/domain/development_trials.rs`、`src/adapters/development_measurements.rs`、既有development领域/adapter、Root指定唯一0030：typed candidate、同事务内核、target catalog、可信评估及trial持久类型/Store接口 |
| P02 | 新 `src/runtime/development_trials.rs`、Root追加授权的 `src/adapters/development_trial_context.rs`及自有UI/测试：受控副本和强来源复验归P02，DTO仍P04唯一；通过P04真实Store接口驱动受控trial、到期/恢复/接受，不复制SQL或领域DTO；P02-09稳定检查点后转入，当前09 r2 review_pending不能算accepted |
| P05 | 新learned_capabilities生产桥与专属测试；P05-08备份保持在途，稳定检查点后转入，不同时持两个实施任务；SDK共享修改仍归Root |
| Root | 模型schema/review/decision接线、context统一入口最小分流、模块/worker/registry最小委托、规范与迁移最终整合；不承包上述领域内部 |
| P01 | 独立负例及真实UI synthetic producer；synthetic只证明工程接线，不作为真实成长/独立行为通过 |
| P03 | 固定候选与真实PG/HTTP/浏览器/故障恢复整合，独立失败全部保留，依赖未运行单列 |

具体新桥文件名由Root/P05现有实施卡固定，本卡不另划重叠路径。Root已指定P04唯一0030，不再要求另等迁移编号。Root共享接入由P04/P05交叉审查；P02运行由Root/P04审契约、P01审证据；P03工程不能代替领域独立评审。

Root冻结的P04入口（不是P00另设计）：

```rust
propose_in_tx(tx, individual, input_manifest, proposer_id,
    &DevelopmentDraft, event_key: Option<&str>)
    -> ProposedDevelopment {
        development_id, event_id, target_id, development_version
    }
```

P04不commit/advance/写顶层receipt或outbox；Root持有decision事务并统一推进一次，原public Store wrapper复用内核并保留旧幂等。Root typed draft采用现有五target及proposed_value；兴趣经P04-11稳定InterestDraft另路，不造persona.interests目标。P04 `target_catalog_in`返回含removed历史的generation_baseline实际版本，Root接context，不能让模型猜版本1。以上随Root实际冻结继续实现，不等待本卡补充建议。

## 只需补齐的runtime／measurement边界

1. **类型和函数归属**：TrialWork、TrialDriveOutcome及可信记录输入只由P04领域模块定义。建议P02唯一入口 `drive_trial(store: &Store, provider: &dyn ModelProvider, lease: &RunLease, work: &TrialWork, limits: CallLimits) -> Result<TrialDriveOutcome, TrialDriveError>`，恢复入口 `recover_trial(store: &Store, work_id: Uuid) -> Result<TrialDriveOutcome, TrialDriveError>`；这是待双方对齐的签名，不宣称已有实现。P04提供真实Store写入器、持久步骤及CAS，P02只编排，不能在runtime另存一套观察或直接SQL。Store方法的准确命名由P04接口交付确定，不由P00造第二份。
2. **可信输入**：P04写入器必须绑定evaluator注册/版本/授权scope、criterion版本、固定required-case集合/digest、candidate/trial/head/scope/generation、实际run及已结算reservation/执行回执、source/result事件和完整支持/反证。模型JSON或synthetic producer不能构造可信验证token；作者自评不能变成IndependentMeasurement。P02产执行事实，P04核对并持久化，独立评审核对必需案例全部完成，缺测保持Unverified，不能只挑成功项。
3. **trial工作与结果**：TrialWork至少绑定work_id、development及版本、StartTrial事件、TargetSnapshot、scope、case/criterion版本、route、deadline、来源与generation。Outcome区分recorded/awaiting_review/accepted/rejected/expired/stale/unknown，含实际case/观察/评审事件ID和reservation，不把已发请求未知记为通过。RecordValidation/Accept/Expire沿P04现有域校验，跨情境与重大事件保持原门槛，不机械要求固定三次。
4. **真实context接入**：P02复用现有 `execution::accounted_call(store, lease, context, allowance, limits, mode, work, usage_of) -> (T, reservation_id)`。P02在获授权的adapter实现受控副本及强来源复验，Root仅接统一入口最小分流；当前 `development_context::require_context`按普通当前人格验证，不能直接塞overlay或绕过校验。P02构造base/trial实际请求，P02分支验证仅目标值变化、匹配scope/原head/有效trial及完整来源，并在预留、派发和结果提交复查。无DB事务跨越模型调用，无新计费旁路。
5. **副作用与停止**：trial仅受控认知副本与评估证据，不写生产记忆/目标、不调用生产工具、不产生公开回复；模拟案例不能成为共同经历。只有接受后才进入普通人格/匹配策略。到期、撤回、head冲突、lease取消、来源移除、预算不足停止相应推进。恢复读取持久步骤和账目；已派发未知先核对结算，不能自动再次收费。实际费用保留，工程阶段0新增收费调用。
6. **事务与回执**：candidate事件与Root决策同事务，后段失败全部回滚；propose_in_tx不得嵌套调用自开事务wrapper。Root总回执纳入新增development/观察/评审IDs、来源及稳定幂等关联；同key异内容拒绝、重放复验新增引用。继续使用 `development_context::require_sources(tx, individual, sources)`的强FK/removed校验，不能退回只查通用派生边。P04 wrapper及trial命令自己作为外层时也只能推进一次。

## P05桥接的实际接口缺口

现有SDK：`RegistrationGrant::issue(&ValidatedSkill, expires_at, SkillScope::SandboxPython)`、`SkillVault::register(grant)`、`SkillVault::invoke(&SkillVersion, input_json: &str, timeout_ms: u64) -> InvocationAttempt`。ValidatedSkill不可从不可信JSON构造，grant本身不授予运行权限。

P05桥固定请求individual/run/action/幂等key、skill/version/source hash、grant/scope/expiry、输入hash、来源/generation与预算上限。Root仅由现行权限/真实验证创建授权并委托桥；注册、验证、权限、实际执行分别记录，未验证生产路径仍unavailable。DB准备/预留后释放事务，再有界阻塞调用SDK，随后新事务复验并结算/提交；不持DB锁等Docker。

**必须明确的恢复缺口**：invoke内部才生成operation ID，不能用输入hash假设vault和DB可唯一关联，更不能假设两者原子提交。Root已确认后续与P05实现最小SDK持久action关联，P05桥消费；若暂未提供，相关崩溃窗口只能unknown且禁止重发，不能宣布生产恢复通过。P05不得因此独立改共享SDK；这不阻止其先实现其余桥逻辑/负例。

结果/回执包含action、operation、execution IDs、sandbox_calls、attested_executions、durable、验证/输入输出hash、isolation/cleanup、reservation和终态。验证按 `sandbox_calls_upper_bound()`预留，按实际次数结算；失败/超时/撤销/未知不计作零消耗。派发前失效不执行，在途失效丢弃认知结果但保留必要审计/费用。清理未知、审计损坏、关联不唯一保持unavailable。进程/时间/内存/文件/网络隔离及无生产凭证/数据/Docker socket约束不变。

平台已停止的协议复验维持not_run，不换窗口、措辞或命令重启；现有SDK局部通过不代表生产桥或全部协议通过。

## 接续与定向验证

接口已冻结部分立即并行编码，未定细节由相关owner与Root一次对齐；每窗口一个有界任务，P02/P05不打断在途检查，交付后接后继，审查/工程依赖单列。不等收费行为验收才编码，不重排全套M3。

P01/P03新增证据聚焦：decision后段失败无部分提交、typed provider绕schema、缺必需案例/假独立身份、base/trial实际请求差异且普通run不变、到期/撤回/祖先来源删除、scope错配/旧回执、vault/DB两侧崩溃、重复action/旧grant、额度耗尽和在途撤销。汇合候选跑proposed→trial→可信观察/评审→accept→普通上下文→revert及注册→执行→撤销→恢复。日常只跑受影响领域/实际PG/HTTP/界面；固定节点完整矩阵，遵循现有锁。工程fixture不证明成长语义。

交接记录精确源码/版本、实际命令退出码、失败/not_run、独立审查、清理及后继。本卡仅做源码/原始指令核对和本地链接检查，未运行Cargo/PG/浏览器/收费模型。P00交付即结束，不恢复轮询、不另行调度。M5工程候选、正式行为、成长对照、14真实日历日均未因此判过。

Root后续通知：P04 r3独立复审approved，Root已核对6份证据并保存ack，共用内核正式开始。此项仅记录Root通知，P00未独立重跑或复审这6份证据。后续仅针对实际DTO差异定向复核，不重复轮询或重写蓝图。

## scope／预算冻结的只读核对（r3）

Root新冻结：`target_catalog_in(tx, individual, run_id: Uuid, goal_id: Option<Uuid>) -> Vec<DevelopmentTargetView>`，唯一P04 DTO字段target/scope/before_version/target_id?/head_revision_id?。original_scope与evaluation_run_id分离；ModelContext保留实际执行run/goal，受信trial_binding才映射原subject。一个case的base/trial可同run，各case另有真实run；3模型/3工具、背景50k/全部200k原限额不变。case/side及账目观察写入不得无谓advance导致自身lease失效，trial/accept全局变化仍一次CAS。原Task完成不能把accepted策略推广到新task。

源码核对：`reserve_model_call`强制context.run_id等于lease.run_id，按当前配置计每run调用和日预算；`claim_run`全局单running、foreground优先，`require_lease`强制state/config/privacy版本一致；`finish_run`可结束运行而不advance。这些与冻结一致。自主评估必须按background排队，不能标foreground绕过50k/暂停门或与用户输入争优先；实际goal若已关闭，现有claim_run会将该goal的queued run置stale，不能通过伪装current_goal_id规避。

两个具体接入缺口（静态核对，未声称已复现故障）：

- `runtime/execution.rs::accounted_call`每次用`Uuid::new_v4()`预留，只在末端成功返回reservation ID。P04/P02尚需在派发前持久关联plan/case/side与reservation；若仅在函数成功返回后补观察记录，派发后崩溃/错误的side无法由现有函数返回值定位，重跑会使用新key。必须查询恢复原账目或保守unknown，不能自动再收费；这属于原冻结的实现要求，不另设计费协议。
- `adapters/scheduling.rs::dispatch_model_call`调用require_lease，后者已有来源/人格复验，但尚无trial plan/case/side/deadline的专属复验。仅预留前检查不覆盖预留后到派发前的trial到期。P02受信绑定校验须由Root最小入口在派发事务内调用，并在提交评估证据前再次核对；不能仅依赖run.deadline（其与trial/plan deadline不是同一字段）。

本次读取时三个新模块尚未出现在共享路径，故不能评价其实际DTO或实现是否已经消除上述缺口；Root接口是冻结输入，不是已完成源码。未运行Rust/PG/模型测试；P00只向Root反馈具体增量，不派工、不重复轮询。

## 请求键接线增量（r4）

ROOT-M3-REQUEST-KEY-01通知后已只读核对 `src/runtime/execution.rs`：新增 `pub(crate) accounted_call_with_request_key(store, lease, context, request_key: Uuid, allowance, limits, mode, work, usage_of) -> Result<(T, Uuid), StepError>`。原accounted_call生成UUID后委托，未复制计费/心跳流程；nil key拒绝，dispatch返回false即AlreadyDispatched，work future尚未被poll。r3的随机键入口缺口已有代码应对，不能继续记为完全未实现。

仍由P04/P02在调用前持久化plan/case/side→request_key、原context/allowance摘要及实际evaluation_run，并实现账目定位/恢复；错误不更换key。AlreadyDispatched须先通过原lease及幂等检查，跨lease generation重放还可能LeaseLost/IdempotencyConflict，不能把所有已发恢复都假定为这一错误；终态账目通过实际individual/run/key查询，不伪造新lease或重复执行。future的构造也不得提前发起外部请求。

Root报告固定root-m3-request-key-v1在compile/83普通/新增1PG及scheduling14/development_context10验证中；P00未读取最终退出证据，状态仍在测，不标通过或已发布。派发事务trial绑定/deadline复验仍待P04唯一DTO和P02实际adapter校验函数接入；本卡不替owner命名。background预算/暂停/独占claim约束继续。此次仅静态核对和文档链接检查，无运行测试。

## Measurement API冻结与请求键证据（r5）

Root采纳 [P04生产交接](../ALIA_P04_M3_PRODUCTION_HANDOFF.md) 第二段DTO及Store入口命名，P04立即实现，P02直接引用；该交接仍标“待冻结”属于通知与文档更新时差，不是继续等审批的理由。唯一DTO源不变。Root指定MeasurementProgram::JsonComparisonV1、最多8case、预建全部case×两侧attempt，至少两个实际不同context的Scenario及CounterSearch，判据/期望来自受信程序/controller，不能由模型自选。Scenario须有效trial满足且有效baseline不满足；CounterSearch须两侧有效满足；缺/失败/unknown/NotRun完整保留。

JsonEquals仅证明覆盖的结构条件，语义/人格/真实成长未测即Unverified，不能扩大为M3行为或成长通过。register绑定系统固定程序版本与权限；complete_attempt复验真实settled且dispatched账目、individual/run/generation/key、context/allowance摘要及实际结果来源/判据，不能信任ModelResult自报provider_mode/usage。Fixture必须保留来源标签，不能升格生产测量。StartTrial+plan同事务一次全局CAS；claim/bind/attempt/observation run-local；最终审查/Accept单独全局CAS。P00只读核对未发现这次冻结与既有边界的新冲突；具体require_context_in/bind_reservation_in参数仍由P02和P04唯一DTO对齐提供，P00不另命名。

已读取 `evals/results/root-m3-request-key-v3/evidence.json` 及v1失败记录，核对11份引用日志哈希和当前execution SHA `b6fdd8a8d3b4f372ee4974a82cd174b32e8f87bf17583d2f326d949e81efe3e3`一致。据该有界证据，v3 compile/M0/新增1PG/strictClippy及owned清理通过；v1同字节83普通/25PG/Clippy阶段通过但父runner收尾exit1，保留失败，不能称v1整体通过；v2解析失败亦保留。v3没有重跑14/10，未扩大覆盖。P00未重跑这些测试，哈希核对不替代独立产品审查。

请求键切片有界工程证据已有；persisted plan/case/side、dispatch专属校验、unknown自动核对、typed decision/worker整合及完整M5仍在该证据not_run范围。本次只更新规划/交接，不改业务、Goal或原失败报告。
