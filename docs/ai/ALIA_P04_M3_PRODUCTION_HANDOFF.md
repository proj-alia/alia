# P04-M3-PRODUCTION-01｜生产内核与受控测量实施交接

## 暂停恢复点：2026-11-12 20:35 UTC后

已直接核对P00原始用户消息（20:35:37.763Z）：“完成手上的任务就结束项目，等待codex额度重置再开始跑项目goal，现在codex没有额度了”。本窗口已结束自己的检查进程并保存检查点，按用户要求暂停原Goal；不是项目完成或M3验收通过，不创建/重置/归档任务，不自动续跑。实际Goal状态由本轮update_goal返回确认。

最新待审为r2固定`evals/results/p04-m3-positive-protocol-20261112-v2/source`497源，manifest `970bcbadfe325512d70f2e53ba5fe962a29acf819059b62290f0d988d3c11c58`，report `af8833e649ac0443ca84d2cb7f5b93d6bbdffc628971fca6453a1d7e69acc5bb`，measurement adapter SHA `8796842aab3b74c2c9901def11be350ed02eae1904ef654ffdbebace2515737e`。其余五owned源沿v6摘要，live已核一致。

实际新生产实现：单一`close_queued_work_in`由review/finalize/expiry复用，原账本close_pending保留未知费用，只取消actual plan映射的queued sibling，保留他人/普通run及原Goal，operation.updated和领域通知同事务。`expire_development_trial_plans(1..20)`在整个individual任意running时返回空；0表空/partial拒绝；真实candidate期限到期Expire、较短measurement窗口到期Reject，不延长期限。已Expired的finalize绑定原真实expiry step保存稳定回执，不重复转换/advance。Root独占worker/claim接线，不在本片批准范围。

固定`p04-m3-expiry-20261112-v2`实际13项owner PG、9项P02原driver PG、生产编译和strictlib均exit0，source/cleanup通过；manifest `755432b5fbe0b0494c763f3c2868cff88de60b04b77a25b05de6f040e3733204`，report `194b476c45cb1af5508ee68cba710b99845804e90b4db91d2748e8affd19f56a`。新反例覆盖未排队到期、较短窗口/queued取消、任意running保护、原scheduler恢复的Unknown150token不退款、晚期通知失败全回滚、仅映射sibling取消及普通前台保留。P02权限伪造原RED已在原9用例实际通过，不再列为此固定链路未修复。

最终positive-v2仅增加/修正cfg(test)正向协议测试，1项PG实际通过；同目录production-equivalence.json已证明全部非test字节与expiry-v2完全相同，合计23项实际执行。六次Live-mode ABI调用是明确进程内模拟，真实网络模型调用0；不能当供应商、行为或成长通过。最终固定M0及5个owned Rust文件格式检查均exit0。新增positive-v1首次在接受已经成功后误读persona顶层traits而失败；改为完整人格文档比较，仅正确的personality.traits变化。旧失败报告保留。expiry原RED重复Expire失败、expiry-v1错误地把plan期限当lease期限导致11/12失败、全部前序失败均保留，未补判通过。

审查状态：P05已对旧v6给限定privacy批准，独立2新来源反例+8原用例通过；该批准不能自动覆盖r2新增expiry/queued终态。Root已开始事务审查，r2固定源码及增量已交Root/P05，最终非作者批准仍待核。当前不可写局部READY或complete。

恢复后第一项：先读取Root/P05对上述r2的固定审查结论，按真实发现修复；如原源码仍一致只补受影响检查，不重跑旧矩阵。Root新worker维护hook须使用自己的固定报告。此前P04-11兴趣来源F1仍是独立旧缺口，本片不能关闭。当前没有P04测试/构建进程在途；无收费、生产初始化或部署。恢复必须沿原Goal和检查点。

Owner P04；reviewers Root事务/模型、P05来源/恢复。model Sol/high，reason=真实事务、来源和可信评估。Root已冻结下列唯一接口及所有权，P04已写入生产代码；局部状态仍为implementing/review_pending，完整闭环和最新严格检查尚未全通过。Goal继续active，不重建窗口，不等收费对照或完整工程矩阵才开发。

## 第一段已落盘（implementing，非accepted）

- `src/adapters/development.rs`已有Root冻结的`propose_in_tx(tx, individual, input_manifest, proposer_id, &DevelopmentDraft, event_key) -> ProposedDevelopment`，返回四字段；无commit/advance/global outbox/顶层receipt。原public Store提案入口保留自身幂等、CAS、来源/生命周期/隐私代次、一次版本/通知/回执，并复用相同内核。
- 同文件已有`target_catalog_in(tx, individual, run_id, goal_id) -> Vec<DevelopmentTargetView>`；domain/development.rs闭合五字段DTO。只读同快照，实际run→goal、完整投影/出生/FK与scope验证；四persona和精确Task/可选Goal，无head不创建，removed历史用真实generation_baseline，不复制persona JSON或返回旧私有ID。
- 新`tests/development_proposals.rs`用实际PG证明内核没有全局副作用、外层后段错误/第二候选失败整体回滚、旧包装器仍可幂等恢复；目录在READ ONLY事务实际运行，检查当前head、removed历史单调版本、无旧ID和错scope拒绝。removed目录前置明确为合成投影，真实删除整合另有原P04/P05报告，不能冒充本次新删除试验。
- `p04-m3-proposed-kernel-20261112-v1`实际编译/2新PG/19旧storage PG/严格Clippy/格式全通过。之后目录最终源`p04-m3-proposed-catalog-20261112-v1`实际编译/4PG/格式通过，但运行器漏带旧development_domain.rs导致该target命令和后续Clippy命令退出101；失败保留，补原测试资料后运行v2，不为打包错误重跑同4PG。
- Root人格来源r3复审已在本片作者编辑前完成；2原反例+10相关PG独立通过，结论在原persona review handoff，不据此自批此新作者切片。Root JOURNAL-NONBLOCK及P01 UI producer审查已入稳定检查点队列，不中断本片在途编译/PG。

## 第二段单一typed API（已冻结并实现）

所有DTO唯一放`domain/development_trials.rs`，SQL/可信写入唯一放`adapters/development_measurements.rs`，P04唯一编辑0030。P02 runtime/受控adapter引用同一DTO，不复制SQL；Root负责共享接线。下面为接口摘要，精确完整字段以代码为准，不能据此反序列化构造授权。

```rust
enum TrialSide { Baseline, Trial }
enum TrialCaseRole { Scenario, CounterSearch }
enum TrialCriterion { JsonEquals { pointer: String, expected: Value } }
struct TrialCase { id: Uuid, context: String, prompt: String,
    role: TrialCaseRole, criterion: TrialCriterion, source_event_ids: Vec<Uuid> }
struct TrialPlanDraft { cases: Vec<TrialCase>, deadline: DateTime<Utc> }
struct TrialPlan { id: Uuid, development_id: Uuid, development_version: i64,
    trial_event_id: Uuid, target: TargetSnapshot,
    original_scope: Option<StrategyScope>, evaluator_id: Uuid,
    criterion_version: String, cases_digest: String,
    cases: Vec<TrialCase>, deadline: DateTime<Utc> }
struct TrialBinding { plan_id: Uuid, case_id: Uuid, side: TrialSide,
    attempt_id: Uuid, evaluation_run_id: Uuid, lease_generation: i64 }
struct TrialWork { plan: TrialPlan, case: TrialCase,
    evaluation_run_id: Uuid, baseline: TrialBinding, trial: TrialBinding }
// 实际phase/context/allowance/reservation/result/unknown均持久化，非caller bool。
enum TrialAttemptState { Pending, Reserved, Dispatched, Recorded, Unknown, NotRun }
```

`TrialBinding`是可序列化关联元数据，不是授权token；预留/派发仍须同事务核对真实work/plan/head/scope/source/lease。`TargetSnapshot`仅是观测快照，DB另验，不把反序列化变授权。真实model响应通过既有ModelResult及accounted_call返回，不能输入Passed或自报measurement身份。

实际Store入口：

```rust
register_development_measurement(key, program: MeasurementProgram) -> Uuid;
freeze_development_trial_plan(key, expected, manifest, development_id,
    expected_development_version, evaluator_id, route, &TrialPlanDraft) -> TrialPlan;
pending_development_trial_cases(limit) -> Vec<PendingTrialCase>;
enqueue_development_trial_case(key, plan_id, case_id) -> Uuid; // 实际queued run
claim_development_trial_work(&RunLease) -> Option<TrialWork>;
bind_development_trial_request(&RunLease, &TrialBinding, &ModelContext,
    TokenAllowance) -> Uuid; // 预先持久化并返回attempt_id稳定请求键
complete_development_trial_attempt(&RunLease, &TrialBinding,
    reservation_id, &ModelContext, &ModelResult) -> TrialAttempt;
recover_development_trial_work(work_or_run_id) -> TrialWorkStatus;
review_development_trial_plan(key, plan_id) -> TrialReviewStatus;
finish_development_trial_work(&RunLease) -> TrialWorkStatus;
finalize_development_trial_plan(key, plan_id, TrialDisposition) -> DevelopmentReceipt;
```

同事务校验helper将给P02受控adapter使用，名字/出参随Root冻结，不另造普通context旁路。Root已接`accounted_call_with_request_key`的在测变更；稳定attempt_id必须进入其request_key，恢复按actual individual/run/key查账，不因丢响应另造UUID。claim不替代原claim_run：enqueue仅在前台优先/空闲检查后放入原真实run队列，实际lease由原独占路径获得；case的base/trial最多两调用，跨case另建真实有来源评估run，原3call/3tool及日预算保持。

### 已落实的可信边界与验证限制

- MeasurementProgram限定实际程序和代码/判据版本，注册为独立measurement而非作者换UUID。case集合及期望值来自受信控制器/已登记程序，随plan冻结并摘要绑定；不接受模型提交的criterion/Passed作为认证。本地Fixture产物单列非生产证据，不把合成调用当真实成长。
- JsonEquals只测确定性结构条件，未覆盖语义保持Unverified；Scenario要求trial满足冻结判据且baseline未满足才视为该项可观测改善，两者都通过不能凭重复保存制造成长；CounterSearch测冻结反例/事实权限回归，两侧均满足才通过。至少两个不同Scenario标签和去空白后不同prompt，另有CounterSearch；最多八case。Root已冻结此初始程序，不将它当完整语义或成长通过。
- 每case两侧记录先存在；缺失、失败、NotRun、Unknown全部持久，评审覆盖全部冻结case及候选完整支持/反证，不筛成功项。没有有效结果的已派发账目先恢复结算，不自动再收费。未知金额不记零；已结算费用不随状态失败回滚。
- Root已冻结original_scope与evaluation_run_id分离；context.run_id/current_goal_id仍为真实执行run/goal。仅受信binding允许原Task subject进入该受控评估副本，普通其他run不继承；trial不写生产记忆/目标/公开消息，不执行生产工具。plan/case/side、原head、actual run、版本/epoch/config/generation全部绑定，后续撤回/删除/过期停止推进。
- StartTrial和plan创建需复用同事务transition内核，一次全局CAS；claim/绑定/attempt/观察账目为run-local不随意advance导致自身lease失效。RecordValidation/Accept/Expire仍复用原TrustedValidator及状态机；unknown先恢复，不依据runtime bool直接Accept。
- 0030已写五表及真实FK，精确来源经development_dependencies及共享实际FK片段保留；片段每个分支均限制individual_id。只有0表允许旧入口为空/普通路径，5表执行完整校验，partial拒绝。`request_sources`保留预绑但尚未预留费用的实际来源；P05须在原清理事务调用purge helper并适配导出/恢复，P04不替其声明完成。

### 实际事务与费用边界

- `transition_in_tx`已抽出，原Store包装器保留intent/CAS/一次advance/outbox/receipt；freeze、review、finalize复用同一内核。finish只结束实际评估run，不推进全局版本、不修改原目标或公开回复。
- Root实际accounted调用仍是唯一预算/记账引擎。dispatch同事务调用`attest_development_trial_dispatch_in`，真实运行时mode和usage发送标记一起保存；模型自报mode不能升级Fixture。Result同事务调用P02唯一受控overlay门，再查实际settled/sent账本、请求digest、generation和绑定。
- 恢复不重新调用模型，不生成第二request key；实际usage与保存context/allowance摘要、run/side/lease generation必须一致。已派发缺结果保留Unknown，未绑定未调用的另一侧为NotRun；未知费用不写零，原费用不删除。真正费用结算仍由原accounted/reaper完成。
- finalize稳定intent仅绑定plan/disposition/原reason；旧回执优先并复查完整来源。新Accept复查当前head/epoch/config、实际review版本、全部case双侧原始结果及criterion、isolated、真实mode与settled账本，不能只读cached Passed。
- private purge helper不自开事务、不advance、不退款。旧development图漏掉development→target的实际FK已补；Root将该单行独立应用于旧schema29候选，原P05反例保留。

### 现有定向证据（不混用版本）

- `p04-m3-transaction-kernels-20261112-v1`：5新PG、19旧storage、13privacy、7HTTP，共44实际PG及严格检查通过；不覆盖后来0030和measurement控制器。
- `p04-m3-measurements-20261112-v1`：编译和2领域单测通过，6PG中5通过/1失败，review manifest遗漏注册event；原报告保留。v2补该真实来源后，2领域和6PG全通过，清理及源码校验通过。
- `p04-m3-measurements-integrated-20261112-v1`：生成manifest重复写入提前FileExistsError，零测试；v2生产编译通过，测试资料漏包导致libtest未编译，Clippy待真实worker/purge接线；v3生产编译通过，新增测试缺import和status类型比较错误导致libtest未编译，Clippy只余purge未消费。均不能计测试通过。
- v4生产编译、2领域及8实际PG通过；dispatch命令过滤器写错导致0项执行，不计通过。旧support仍用同题不同标签，9个driver用例全部在freeze前拒绝；Clippy只剩尚未纳入的P05 purge消费，失败保留。
- v5纳入实际新support和P05 purge接线，真实dispatch2项、driver7项通过，driver2项真实失败：旧租约部分结果恢复Stale，以及预绑权限上下文伪造未被拒绝。严格Clippy通过。后者连原反例/固定source交P02唯一context owner，不能把整批报通过。
- v6修改实际P04恢复控制：原调度先结清旧reservation，核旧body/allowance摘要、run/key/generation；旧request和Recorded结果保留，未发侧NotRun、已发缺结果Unknown。新租约仅用于中断结案，不新造key或重发。原restart断言保持不变并实际通过；同版8项owner PG和2项真实accounted dispatch复验均通过，共11实际PG，strictlib通过。source_unchanged/root_input_still_verified/cleanup均true。497源manifest SHA256 `35dfffa26bc403cf2975b591c0ec06fc9f40442a943642427f1bfd1cc621327d`，report `fc738ca9297613cf3f3b225e349b4ae87c502cc78dae6458b5f87a47d598a3ed`。
- v6固定源码M0 exit0：7schema/68closed/16example/86negative/111trace/136edges/9docs/39links/2sources；5个owned Rust文件rustfmt --check exit0。首次校验摘要命令误在source子目录使用相对report路径而失败，随后以绝对路径重新核6源全部匹配live；不是产品或测试通过记录。
- v6本片6源为development adapter `f3f4108b9ea60e9365ecdc46c45f549f280c04227ffeb74a8e2dae0c3d28548d`、domain development `609f4546d9787df6f9e00f1b20100713df233624adce92ce21c96ffaba320d86`、domain trials `93aa5d31d6da056d3f16e6f52f005f39256445e926d704645ab072bd9bcf64f0`、measurement adapter `de39a3ec3d40c6362ea7273c8b6aed212c468bb6d2dae5f3c07f5976330d8948`、0030 `214822c170b5fcd374b9cb1521d25ceb9d69147379eac526a108bc1fd346d255`、proposal tests `63c0b6d45a76d7a09e7dd14912f129c1be2969b1ea1f210b1a331e99140b7626`。

无收费模型、无生产初始化/部署，非作者评审尚未结束。v6只重跑受P04恢复修改影响检查，不冒用旧44PG或将P02权限原RED补判通过。所有历史失败保持原文件。各运行器持有独立target、workspace和测试DB锁，未证明跨所有窗口的重型编译串行协调；不能以本报告宣称全局排队优化已完成。

下一依赖：完成最新定向结果、交Root/P05固定哈希非作者审查；P05完成schema30清理/导出/恢复实际接线，Root保有共享接口与完整候选裁决。P01行为与P03工程不能被本地Fixture替代。
