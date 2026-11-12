# ROOT-PERSONA-SOURCES-01/r3 — 当前人格与任务策略的强来源闭环

当前为r3；下列r1/r2及v1—v5是原始交付记录，不能覆盖文末两项独立RED、r2提交修复与r3预留恢复处置。r2的独立结论为changes_requested，不是批准。

Owner Root；非作者reviewer P04（事务/版本）及P05（来源/隐私）；状态review_pending，作者不能自批。model=gpt-6.1-sol/effort=high，reason=跨上下文、预留/派发、提交、历史回执和公开读取的人格来源边界。本切片采用P00运行方案A第一部分及Astra复核，不删除typed候选、生产trial/评测身份、独立验证/接受与恢复等后继工作。

## 实际代码与接口

新增domain/development_context.rs：当前head元数据target_id/target/version/revision_id/scope，以及seed_event_id、effective_task_strategy与完整source_ids。ModelContext可选development_context，无head时省略，出生来源仍入manifest。persona仍唯一持有当前人格值，不再复制四份大JSON；策略仅取同一实际run及其实际goal，精确Task优先Goal，空/撤回Task可回退Goal，不扩大权限、预算、身份或完成条件。未接受candidate的proposed_value不作为当前persona。

新增adapters/development_context.rs，在调用者RR/写事务内复用P05完整投影验证和P04真实LINKS/available/immutable_seed；development.rs只将这三项改pub(super)，算法不改。验证seed摘要/最早出生、完整投影、当前head/revision、FK祖先和removed标志。模型完整来源上限4096、整包96KiB，超限拒绝不截断；公开persona直接完整验证图，不套模型载荷上限，合法大图仍可读取。run→goal绑定从实际DB读，不能由context伪造。

来源接通context组装、reserve、lease/dispatch、fresh commit、审查记录和cached receipt。fresh检查实际persona/head/作用域/来源与context一致；缓存检查原完整来源和全部产物/实际审查凭据，沿真实发展FK及removed复查，不要求与后来合法人格版本相同，不重新执行/计费。public_state与console_persona在同一快照核验投影，并加privacy generation前后门。

作者实际请求体携带新视图及边界说明；review packet升target-citations/10并以development_context_not_experience呈现。旧允许事实引用集合不扩大，不能把版本/来源当共同亲历或工具成功。相关六份旧测试仅同步packet10断言；新实际请求体单测核对作者与review两份真实序列化路径。没有model development字段写入、可信verifier生成、trial scheduler、Accept入口或任何新模型权限。

## 冻结基线与真实验证

最终源`evals/results/root-persona-sources-v3/source`，481文件：HTTP-v5/schema29基线＋Root social r4生产修复＋本切片。P04-11仅引入其固定r2 domain/interests DTO以满足先前已存在interest_context字段；不接其兴趣写入helper，不据此认为11跨审通过。未引入未验SDK/备份；未用当前全部live文件冒充已验版本。

15阶段实际全部exit0：M0、定向cargo check、89普通测试、11组实际PG/HTTP共93项、严格Clippy。数据库组：新development_context7、development_http7、development_privacy13、social_decisions6、scheduling14、grounding9、api20、privacy_http5、belief_decisions4、autobiography_decisions5、history_boundary_regression3。默认36 ignored不计通过，DB实际运行各组ignored才计。来源/QA helper摘要稳定，精确临时容器与卷cleanup通过。

新7项覆盖：合法人格来源进入作者/reviewer、重启一致、出生与合法当前差异及后来正常撤回后旧意图恢复；伪persona/head/缺manifest/错goal拒绝；缺通用边的祖先撤销在reserve/dispatch前无新账目；removed但object未blocked时拒绝派发/提交/旧回执，原已结算合成调用费用不丢；公开state/persona/management拒绝无依据值，privacy gate继续生效；当前Task策略优先Goal且其他目标不入视图、persona不改；4097额外来源使模型明确拒绝，但公开persona仍可完整验证并读取。

这些trusted注册/观察记录是显式synthetic前置，合法状态转换通过真实Store。0云端模型、未部署/未初始化生产；既有回归含FixtureProvider和合成reviewer，未采集其总trait调用数。report.model_calls=0是旧runner占位，evidence.json明确排除为调用总数测量，不据此称零合成调用或行为通过。

## 失败与修复保留

- v1：测试支持文件移动到tests/support后include_bytes路径仍少一层，编译失败，未启动PG。
- v2：修测试路径；新7及相关回归合计56PG先通过，API组19过1失败，后续17PG/Clippy未运行。旧API故障注入断言要求显示无来源的直接persona改值，与本次真实来源约束冲突。
- v3：生产实现不放宽；旧测试改为公开拒绝，并保留实际数据库seed未变/故障值确实写入的检查。另在真实accepted开发场景验证合法initial/current差异可显示；受影响belief/自传/独立历史三组补跑。最终89普通/93实际PG全部通过。旧失败和报告不覆盖。

最终manifest SHA `3f8b02c52333d9884791c529af480a5d0a5b13abdfd71a759779e56b6bb7e36a`；report SHA `61dbb3939495d91f873697ef2d0db18a6c2a69a678c64bb02dd2072a9a7733cd`。evidence.json绑定15日志和12封存执行文件，run-parent.py保留真实运行器。独占target及候选/工作区/数据库资源锁贯穿各运行及清理，不猜旧进程终止后重启，不重派平台拒绝的协议操作。

## 后继与限制

P04/P05独立审查待完成；完整管理UI与全部live组合、正式模型/检索/60×3、20组ABC及十四真实日未验。后继仍需M3-A typed proposed同事务写入及兴趣接线，B冻结真实评测计划/执行器身份/判据/全部case结果与受控trial，C独立验证和接受恢复。不能用本来源接线代替“已经成长”。

Root还静态发现/state投影拒绝时前端旧state可能未清屏，已交P02独立复现后修补（P02-09），不凭推测改UI或报告浏览器通过。Root核心实现不等待该分支完整矩阵；上线前仍需统一工程候选和独立接受。


## r2：实际Task scope链路复现并修复来源集合增长

Root在宣布r1完成前补查发现：TaskStrategy来源引用本run，而reserve合法追加run.input_manifest；后续机械比较重建的全部source_ids，会把本轮新增的合法输入误当旧快照篡改。仅加强原scoped用例从reserve到真实model_step/commit，在原r1生产上运行`root-persona-sources-growth-baseline-v1`，实际0过/1失败（StaleState）、源码/清理稳定，原RED保留。r1的89/93定向通过不覆盖这一组合，不据此批准r1。

r2不移除run.input_manifest真实来源边、不放宽FK/removed或事实引用。首次author reserve仍完整物化并精确比较观测来源和head/persona/scope；之后已绑定review/commit完整复验真实图及原来源，比较相同head/persona/scope但保持原观测列表，不要求它变成后来run祖先全集。review reserve额外调用原grounding::accounted（只提升内部可见性）认证实际settled作者reservation/individual/run/lease及完整原context/allowance摘要；伪review标记不能篡改原观测列表。fresh commit的opaque PreparedDecision及原账目摘要校验保留，已结算费用不撤销。

新增第8PG用例经实际合成作者→独立角色review→记录→commit→同意图replay完整路径；两次provider均实际收到正确scoped策略。另保持manifest/目标/root不变，仅从review.source_ids移除事件，实际settled作者摘要拒绝且全表不变。原scoped第6用例亦从仅reserve加强为实际prepare/commit；这两项工程fixture不证明模型语义质量。

- v4：最终生产代码的89普通/94实际PGHTTP全部通过；严格Clippy因新测试两项unused trait imports失败，报告/警告保留。
- v5：只移除这两项测试局部import，生产481源集合中所有production字节与v4完全相同、测试断言/driver逐字不变（确切变换已验证）。最终定向编译、8项实际PG、严格Clippy/M0全过。没有重跑不受影响的86PG或89普通，也不把v4整体failed报告改成passed。

最终源`root-persona-sources-v5/source`，manifest SHA `a2f73e7108bd4a8771d6b516238b993bfac111698e2acae985bd23351a35d68e`；定向report SHA `06a3a36b58bfb0a116c6b69951338b0b3a5dfd11fba9bec303603af9b4f81d4c`；evidence SHA `29e644dabdb00d585c068e4cc4853ecfc48c48fcfa5ed157705d2265d8681c18`，绑定v4全部日志、v5日志/封存执行文件、等价变换及未运行项。新卡revision2 subject `1aac285f1181262cba6161421e6cc0c172ae19792c7bec384a468004cff39a23`、review元数据门exit0；该元数据门不是非作者审查。

本r2仍P04/P05 review_pending，必须按新源码/phase复验，不能继承r1意见。云端调用0；合成provider总调用数未独立统计，原runner占位model_calls不用于计量。本r2只收敛合法scope时序，不完成typed发展写入、生产trial/评测、真实成长或M1—M5。

## r3：恢复已持久化但尚未派发的原作者预留

Root读取并核对P04 r2独立结论：原F1提交反例已通过，但新增F2同key未派发预留恢复实际失败（StaleState），账目count/reserved/charged=(1,150,0)，失败时未调用provider、没有额外收费。原因是查原request_key之前仍将每次作者调用当首次新预留，自己的run.input_manifest增长使精确来源集合不同。原失败、结论与两份P04反例均保留，不由作者定向通过改判。

本r3相对固定v5唯一生产改动为`scheduling.rs`：在有效lease/来源校验后，先查实际individual/run/request_key账目，比较完整context/allowance摘要和lease generation。只有匹配的已持久化原请求使用既有绑定阶段复验，仍严格校验当前persona/head/scope、原来源及实际FK/removed；不同key仍走首次来源全集精确校验。review仍绑定真实settled作者。返回原receipt，不新增预留、不派发、不改费用。没有把status=reserved伪装成已派发或已结算，也没有新增权限、模型字段或配置。

Root新增两项实际PG：并发恢复reserved/dispatched/settled各阶段时全表不变、同id、仅一次dispatch标记及50合成token的单条结算；改persona/原来源/allowance拒绝、不同key不能借旧快照恢复、旧generation拒绝、缺通用边的来源撤销及removed标志继续拒绝。独立P04两反例一字未改，SHA分别`e825feb074bfd0ba4c61791725c97d7652337ab31b9897d39b5e5ad3e2ba7784`和`135baea75a00eeec5269fbe8f9a43e257cbc477d3823e56552e0678661a0639c`，均在本固定副本实际通过。

固定`evals/results/root-persona-sources-v6/source`483源，仅v5叠加`scheduling.rs`、Root两项测试、契约及P04两反例；未混入当前UI/SDK/备份。14阶段全部exit0：M0、定向compile、83普通测试（37 ignored不计）、10组实际PG/HTTP共86项、严格Clippy。实际PG为development_context10、P04原反例1+1、scheduling14、grounding9、development_privacy13、social_decisions6、api20、development_http7、privacy_http5。局部rustfmt另实际exit0。每组执行日志的精确Running二进制在target锁释放前封存；evidence只绑定这些10个实际执行文件，不以缓存目录里其他匹配文件冒充本次执行。源码稳定、owned容器/卷清理均true。

最终manifest `a965d6b835e029ea987dd2babe60b534b01394dce3f547a53253d811aa482b1c`；report `37e339bdd87dc6a441b9d18e5d49b12e29da08f2ca0a5bdb7b8b65c70b0f4fbc`；evidence `9feacd62a7317d534e643c16511979fcb613836e31437319eabf747aec289528`；r3任务主体`6c31a68f153f6ae2df43be7c8abb721942f130268b788a64b3bebfb74bfa8310`。review元数据门实际通过，不是独立批准。P04/P05须按新r3复审；状态review_pending。

0云端模型调用、未部署、未使用生产个体。合成provider总次数未独立汇总，报告为null；新增账本用例明确无provider，只模拟一次dispatch/settle。未跑不受本改动直接影响的belief/autobiography/history DB组，也没有完整live工程矩阵、有记录WebUI全流程、生产trial/verifier、typed提案、成长ABC或十四真实日。原M1—M5 Goal保持active。

P04已按固定v6/r3限定批准、open_blockers=0：原两反例独立2PG通过，作者原样10PG独立复跑通过，严格Clippy/fmt通过。Root实际核对subject、作者manifest/report及其六份结论所绑证据摘要，保存在`root-persona-sources-v6/root-p04-review-ack.json`。独立report SHA `fffc22b2602827a16e159dcbb994934dfa01bb2928ee8ff98c312a31b8dc58b5`，manifest `e73d344bc172dab2a99db9ac8452b7d5c03330f0fb8c0fc051dfb5a2898d7078`。该批准不代替P05，也不涵盖后续P04作为作者的新M3内核、当前全部live源码或产品发布。
