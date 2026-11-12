# ROOT-M3-RUN-ISOLATION-01：实际评估运行与原账目接线

Owner Root，reviewers P04（事务/状态）和P05（来源/隐私），Sol/high。状态review_pending；最新固定v8定向验证通过，依用户请求收尾并暂停进一步实施，不能据此判整个项目完成。上游模块继续由P02/P04/P05各自维护，Root不覆盖其实现。

## 实际共享接线

- 依据数据库中实际evaluation_run映射识别试用，不依靠可移除的trial_binding。普通reserve、dispatch、commit（包含旧回执）及普通retry拒绝评估任务，防止候选写进日常记忆、目标或回复。
- 注册唯一P04 measurement及P02 trial context/runtime模块。专用reserve消费P02 Reservation校验；原usage_reservation插入/原键恢复与P04 attempt关联处于同事务，不增加第二套预算。只有真实prebound case/side/key/context可进入。
- 新内部dispatch_model_call_with_context消费P02 Dispatch和P04实际ProviderMode证明，与原dispatched_at/model_calls写入同事务；旧公开dispatch保持拒绝评估运行。原accounted_call_with_request_key根据真实受控context调用这个入口；时限、心跳、已知/未知用量和原三次上限不变。
- 评估budget stop只更新本run及明确trial诊断，不推进全局状态、不暂停原目标。goal_cycles排除实际评估run，即使旧数据错误带有goal_due_at，也不增加目标无进展次数。
- P04 Result阶段同事务消费、终态finish/finalize、P02完整driver及worker选择仍需接完并验证；缺失时不通过普通路径临时执行或伪造成功。

## 实际证据及失败

固定`evals/results/root-m3-run-isolation-v3/source`496文件，是Root atomic-v4的明确叠加，不是当前全部checkout。P02/P04作者输入按固定SHA记录，未据此给予独立接受。新共享fixture仅用于工程，模型输出/注册观察明确合成，不代表成长。

1. v1编译通过，4项新PG均在setup因无注册观察被原分类器拒绝，尚未执行目标断言。修正新fixture为实际submit_message的reported_claim来源，未修改分类规则或旧测试。原失败、清理记录保留。
2. v2实际4新PG+scheduling14+goals13+worker12+atomic4，共47项全过；strict失败：P04两未消费函数与三处复杂类型。没有把PG绿色补判整体通过。
3. v3接入真实专用请求/dispatch路径后，新增2项内部PG与原47项全部通过（49）；源码摘要、封存执行文件与owned数据库/卷清理均复验通过。普通测试没有重跑，日志中filtered_out不计通过。
4. 两项新PG实际证明：同run基线/候选两次原账目结算共4合成token，原键重放不再poll；Fixture状态Recorded但production_measurement=false/outcome=Unverified，当前人格/全局版本和公开消息均不变。另一项在mode attestation之后、usage dispatch更新时注入失败，整事务回滚且provider未poll；移除故障后原键只派发一次。
5. v3 strict仍失败六项：P04新增purge未消费、新finalizer五元组复杂度；P02 Result未消费、prepare_case_context未接driver、bound_case_context未消费、ordinary_in嵌套if。错误已分别交原owner，不添加生产allow或无意义引用来掩盖未接入。

v3 manifest SHA `88fc76b230a59e355d0a42304b8aa1798ca6a7b6e3567cf09587c2e18a040b73`。原始命令/退出/实际二进制在同目录checks/report.json及bin；v2 task-card/evidence明确validation=failed、implementing，不是accepted。后续修复必须冻结新版本，不回写原失败。

## 尚需验收的具体闭口

- prepare_case_context需要考虑心跳续约：原RunLease结构的expires_at不会随DB续约自动改变；第二side不能用过期旧字段误拒真实有效lease。P02已收到实际复现要求。
- Result校验在P04结果写入同事务执行；finish只有两侧真实终态、账目已恢复后释放run，review不抢占仍running的评估；finalize稳定原意图，不重取ExpectedState造成重启换意图。
- P04新purge由P05原删除事务消费，清理plan/criterion/context/output副本；来源图、导出、旧备份恢复仍需完整证明，不能只靠引用边或编译通过。
- 完整driver使用实际案例/原账目，无生产tool/reply/memory写入；等待、过期、未知、失败、跨进程恢复和前台优先需要真实运行证据。
- 继续固定候选全工程、非作者审查、真实模型发展与正式60×3/20组成长/十四真实日门槛。0个Root收费调用，无生产初始化或部署。

## r3实际接线与新增反例（固定v8定向通过，待审）

Root已接原Worker::run_once：真实claim后优先委派P02受控driver；空闲且模型就绪时仅入队有来源case，仍由原claim_run领取。普通前台输入优先。评估输出不走普通commit、不执行生产工具。P02已接实际权限复核（预留/派发/结果）、P04已接Result/finish/finalize及旧generation保守恢复，P05已接原删除事务的schema30清理与独立就绪检查。

- v4普通92过/3失败，21 ignored：P02移动的纯投影fixture仍使用同prompt，被新“不同实际Scenario输入”规则正确拒绝。P02修fixture而不放宽领域约束，失败保留。
- v5普通95过/26 ignored；worker2中1过1失败，失败发生在前台已经正确commit后，Root测试又请求已结束run的model_context。原生命周期正确拒绝；Root仅改cfg(test)为记录provider实际收到的context，断言真实run/mode及无trial绑定，不修改生产生命周期。
- v6实际72PG通过/11失败：完整worker2、driver11、measurement8、dispatch2、isolation4、scheduling14、goals13、worker12、atomic4通过；development_privacy原13中2过11失败，明确为Root尚未采用schema30隐私/导出guard及级联断言。后续HTTP/独立反例与strict未运行；不能将此版报整体通过。
- 独立`root-m3-expiry-red-v1`新增Root真实Worker反例0过/1失败：已排队评测窗口到期、provider不可用时，旧路径重新claim后返回StaleState，不能收敛。案例使用明确合成的已过期窗口，不作为真实时间观察；旧失败与清理保留。
- Root共享修正：claim候选按实际plan排除已终态/已到期评估；无lease时调用P04原子expiry维护，与provider readiness无关。Store重新核全个体无running、恢复原费用、关闭关联队列并原状态机结案；维护有结果时Worker返回Maintained，重复无工作返回Idle。较短评测窗口只能Reject，不能提前Expire候选。
- Root采用P05逐实际0001..0030迁移的精确列摘要证明：删除controller仅白名单schema29/30，未知仍拒绝。固定`root-m3-schema30-privacy-v1`用P05 portable-v3四源，33实际PG（development privacy17/HTTP5/export11）、compile/M0/strict及清理全部通过；manifest`4e1326d8559fde709d4b5c72211f4e71ed02c8aa60d21207c85ad8610e79a9fe`。这是此固定范围，不覆盖后续expiry controller或受管备份。

最新`root-m3-run-isolation-v8`499源manifest`2b94c40f00ceb68579d1a4e5691b4882a38e98b58dabf1ed787a75f9b0d57bb5`，基于上述隐私固定版+P04 expiry-v2固定controller+Root两个共享钩子/新测试。17阶段实际全部exit0：普通95通过、33 ignored不计通过；实际PG/HTTP112通过、0失败/0 ignored；compile/M0/strict通过，10个实际执行二进制封存校验、499源逐字及完整集合校验、owned数据库/卷清理通过。report SHA`10cbe54dff7da5f57b7cdc5e73a4d863ff1ca07225afda5f936c5e319b6f39c6`；evidence SHA`0ffe9122decfab530afe0b28bea33d4e4bd2213287dbd778cc0f96e04431fca7`。没有重新运行前端/browser、完整checkout矩阵或本版schema29双guard兼容套件，不跨版本外推。

新增native scripted Live分支完整三case接受测试只是工程ABI/事务覆盖，不发网络、不证明真实模型或成长；前两case不接受，完整三case后只更新原scope策略及head，无生产回复/记忆/目标写入，判据expected不进入模型context。P04对应native positive也不是收费行为验收。原Fixture分支仍Unverified/Reject。原实际到期RED逐字保留，新相同Worker反例通过：provider不可用仍清理已到期队列、6侧NotRun、零新收费，重复维护不推进全局、不改变原goal。

Root/P04/P05针对本版的非作者审查、完整工程候选、真实发展、20组成长及十四真实日仍未完成，没有新收费或部署。

## 用户暂停与恢复入口（2026-11-12T20:35Z请求，20:40Z当前运行收尾）

Root直接核对用户原始消息`msg_01a1d179-1ca3-79a2-a09c-9ac14f6f716a`：完成手上任务后停下，等待Codex额度重置。既有session42762已终态exit0，资源清理完成；不启动补跑、后继切片、收费或部署。保存[暂停检查点](../../evals/results/root-m3-run-isolation-v8/pause-checkpoint.json)，SHA`2f18fb60d38f00c9e4b403f6b66b73e1eb3241a9a5ffec637bd43ea4845e39ea`。原完整M1—M5 Goal按用户请求暂停，不complete/重建/归档；实际状态以Goal工具结果为准。

用户明确恢复后，先核固定v8全部摘要及P04/P05非作者审查。P04最终positive-protocol-v2相对Root采用的expiry-v2仅增加cfg(test)正向ABI用例，其完整文件hash不同，需保留生产等价证明及原新源，不混称同一源。再衔接P05 schema30个人导出/受管备份与Root原子hook、P02有记录UI及P03固定整合。M4注册/SDK每次派发/动作关联、完整导出HTTP/备份恢复、正式行为与十四天仍有剩余；不得因本次定向通过缩减愿景。VISIBLE-NATURAL原campaign/身份/账本和满额未知费用保留，额度重置本身不自动授权新调用或恢复Goal。
