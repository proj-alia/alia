# COLLAB-02：多 Agent 协作实施交接

本任务由原协作方案窗口实施，Root 统一修改权威规范，P01 独立审查工具。复用七个已有角色，不新建窗口、不重置 Goal，不修改产品 API、运行预算、业务 schema、数据库或既有行为门槛。

## 已生效的职责与交接

- Root 是唯一领域裁决与整合者；P00 在原跟踪窗口只读分析、写专属规划和任务卡，不再无变化轮询或转发状态，不写产品代码。
- Root/P02/P04/P05 继续当前有界实现。P03 当前技能切片自然交付后回归工程 QA，后继产品维护归 Root；P01 保持行为与独立反例职责。
- 当前 owner/reviewer/依赖唯一见 [协调页](ALIA_PARALLEL_COORDINATION.md)，里程碑状态唯一见 [IMPLEMENTATION](../IMPLEMENTATION.md)。旧报告不覆盖。P00 已交付 [角色交接](ALIA_P00_HANDOFF.md)，其方案仍需 Root 接受，不自动释放实施。
- 状态：defined → ready（可实施）→ implementing → review_pending → engineering_pending → accepted。依赖阻塞另记条件；这些状态不等于 App Goal 状态，也不等于行为通过。每个角色最多一个编码切片；只读审查须明确固定输入。
- 新任务首次读取规范；续接核对文档和接口版本，只读相关差异。通知传 task_id/revision/state/evidence 增量，无变化不重复发送。简单修改不另写架构长文；高风险变更必须有 Root 接受方案、非作者审查和故障反例。

## 小型工具接口与边界

`scripts/qa/collaboration.py` 在阶段边界检查任务卡、Root 维护的登记表与固定源码。`validate(card, registry, root, phase)` 的 phase 为 implement/review/ready；ready 指交付前置检查，不等于产品接受。CLI：

```bash
.venv/bin/python scripts/qa/collaboration.py --card CARD.json --registry REGISTRY.json --root FROZEN_SOURCE --phase ready
```

卡片 `format=alia-task-card/1`，必需字段：task_id、正整数revision、owner、非作者reviewers、risk(simple/high)、baseline、sources(相对路径→SHA256)、dependencies(任务ID→revision/subject_sha256)、interfaces(名称→版本)、scope(精确文件列表)、goal、acceptance、delta。`subject_digest(card)` 将这些字段规范化后摘要；实施中编辑不逐次要求重新登记，下一正式交接或接口变动才生成新revision。implement检查本次切片的固定起始源，ready检查最终交付源，不能把旧起始摘要当新交付源。

登记表 `format=alia-task-registry/1`，包含 interfaces、tasks。当前任务条目绑定revision、subject_sha256、state及released_by=Root；依赖必须已accepted且版本/摘要精确一致。Root维护登记表，实施者不能自行修改它来放行。本文字符串角色字段不是密码学签名，文件所有权约定也不是操作系统强制隔离；工具不能替代授权和真实审查。

ready另要求每名指定非作者的reviewer、subject_sha256、decision=approved、open_blockers=0、evidence(相对路径→摘要)，以及validation的相同subject_sha256、status=passed、commands(command/实际整数exit_code)、not_run列表和evidence。文件改动、过期接口/依赖、作者自批、缺审查或失败命令均拒绝。工具只校验元数据一致性和证据字节，不理解报告正文，不把作者填写的passed视为独立质量证明。原有真实工程/行为门禁必须继续执行。

`claim_notification(directory,event)` 使用独占原子发布记录通知接收，event含task_id/revision/state/subject_sha256/evidence_sha256。同版本同状态同证据只会有一个claimed；同源码的新证据可以单独通知；同版本冲突源码拒绝。重复返回already_claimed_outcome_unknown，不能重派；若接收后崩溃，先恢复原任务状态。它不发送聊天、不启动进程，也不提供exactly-once外部执行承诺。调用者仅在claimed后处理该通知，重试仍须核对当前登记版本；旧人工通知和已有聊天不会被工具追溯接管。

兼容策略：旧冻结候选与报告保持原样；新任务逐步纳入登记，不编造旧独立审查。P03负责将新任务元数据门接入现有QA入口，并补P02-05→07的合法有序接受链；未知live哈希仍拒绝。历史FLOW失败持续有效，不能由本工具单测抵消。

## 开发用量与模型分级

`scripts/qa/development_usage.py --session THREAD_ID=SESSION.jsonl [--session ...] [--turn-tasks MAP.json]` 只读取显式会话。只取token_usage_record的逐请求usage，全局按response_id去重；跨线程继承记录排除，冲突重复拒绝，累计token_count与Goal.tokensUsed不相加。模型按实际turn_context关联。任务关联仅由显式turn_id→task_id映射提供，缺失保持null；输出task_id_sha256而非原始任务标签。线程/回合仅允许规范UUID，model/effort只输出已知枚举，否则null，避免不可信元数据回显。

输出只含线程/回合/任务/模型计量与时间，不含聊天、代码、命令、凭据和原始供应商响应。known_tokens为已知小计，unknown_records逐项记录缺失，不能把未知解释为0；reasoning属于output、cached属于input，不能重复相加。actual_cost和provider_latency_seconds未获得可靠数据均为null；记录时间跨度不是服务延迟。报告是本地观测基线，不是账户账单。

实际初始基线保存在忽略目录 `evals/results/collab-02/development-usage-baseline-v1.json`：七个明确线程，4331个去重请求、143个分组，初次观测无解析问题。任务历史关联未推断，实际货币费用未知。它不是产品运行用量，不能用于削减产品预算。后续输出新文件，不覆盖基线。

调度规则保持当前在途模型：Root/P04/P05用Sol High；P00一般规划Sol，涉及来源/权限/预算/并发/恢复/沙盒的复杂决策才提Astra；P02普通界面下一独立任务可Terra对照，竞态/隐私用Sol；P01材料Terra，复杂边界独立Sol/Astra；P03正常执行用脚本，失败诊断Sol。Luna只用于有可检查结果的简单提取，不替代高风险裁决。Claude/DeepSeek开发路由尚未接入，不调用产品凭证冒充开发适配。DOC-VISION-02保持原义。

首次模型对照复用五类已接受任务的固定源与隐藏验收：Rust事务、SQL迁移、React异步状态、故障诊断、Code Review各一例。每个候选独立目录和上下文，不向后一个暴露前者答案；记录模型/档位/版本、实际请求用量、耗时、失败、返工和遗漏缺陷。只在授权开发调用范围内进行，不新增产品收费trial；当前尚无受控同题对照结果，不能宣布降级等价或节省比例。

## 验证与后续门槛

首轮定向命令：`PYTHONDONTWRITEBYTECODE=1 .venv/bin/python -m unittest discover -s evals/p03 -p 'test_collaboration.py'` 七项退出0；相同命令pattern改为test_development_usage.py六项退出0。真实并发通知用8个调用验证仅1个claim；其余测试覆盖过期卡片/接口、源码改后审查失效、自批、合法精确后继与未知摘要、失败/未知退出、证据改动、重复请求、继承用量、缺失计量、模型变更、敏感正文排除和截断日志。均无PG、Docker或产品模型调用。

独立审查和最终命令证据见 [P01审查记录](ALIA_COLLABORATION_02_REVIEW.md)；未完成审查前保持review_pending。现行资源锁不改变，本任务不运行完整工程矩阵。

Git评估：已确认尚无首个提交；现阶段继续快照与精确文件所有权。将来首个基线必须从明确白名单逐文件审阅，排除work/私有结果/模型/凭据/运行数据，禁止git add .和顺带全仓提交。本任务未创建提交或worktree。

持续效果至少观察两个完整候选周期：accepted切片耗时、READY到候选等待、审查排队、锁等待、返工/遗漏缺陷、重复执行及已知费用。FLOW失败不是完整周期；目前不能给出提速/节省结论。P00维护计划，P03写实际周期证据，Root确认里程碑。只有实际独立队列持续积压才评估新增Goal。

## r3 修复与固定验证

P01首审不批准记录保留：通知缺证据增量、元数据透传和跨时区排序三项均已修复。r3固定四源及common依赖在`evals/results/collab-02/r3/source-manifest.json`，复制源实际运行7项协作测试及8项用量测试，均exit0，日志和checks.json同目录。新增真实七线程用量快照4362请求/146分组、无解析问题，费用与延迟unknown；该快照随活动线程时间变化，不能与v1累加。M0实际exit0（6schemas/60closed/14examples/78negative/111trace/136edges/9documents/39links/2sources）。等待P01对r3的独立结论，未宣称本次完整工程通过。

## r4 独立审查通过

P01追加发现并实际复现双方缺失依赖版本/摘要会错误相等，r4现对双方及当前登记强制非bool正整数revision和规范SHA。最终8项协作＋8项用量测试在固定副本实际退出0；P01独立再跑同16项并复现旧反例已拒绝，明确批准该五源范围，旧不批准记录保留。最终证据目录`evals/results/collab-02/r4/`含source-manifest、固定源码、checks、日志和p01-review.md。

首份真实卡`task-card.json`已绑定四个自有源、common接口摘要、P01非作者审查副本和实际局部测试，主体摘要`0df6b6b49fc0d37a08e032ec4f9ae110d7b19c7ba264b617b1be7da24100fdcb`。`registry-proposed.json`初始defined且released_by=null，必须Root核对后才能产生正式registry并运行ready检查；作者未自批。P03已自然交付11并开始接受链修复，生产后续维护归Root。两个完整候选周期、模型同题对照和新的完整工程候选仍未完成，不能据本工具通过宣称整个优化方案的效果已经验证。

## Root实际接受与首个门禁使用

Root已独立核对P01最终审查与五源摘要，从proposed仅变更state/released_by生成registry.json，并实际运行本卡`--phase ready`退出0，主体摘要与上述一致。证据`evals/results/collab-02/r4/root-acceptance.json`、`registry-accepted.json`及`evals/results/accepted-slices/collab-02-r4/`。这证明新机制已用于一次真实工具交接；不把它扩大为全产品工程或行为通过。P03后继负责在唯一QA入口接入此固定机制并完成旧接受链修复，旧报告兼容保留。

## MODEL-DISPATCH-01：实际应用模型与推理档位

用户明确要求解决模型/强度只写规则未实际切换的问题。2026-11-12本窗口已对七个现有线程使用send_message_to_thread的显式model/thinking参数，而不是仅发送文字建议。P00新架构复核派发gpt-6-astra/high，P01复杂独立语义审查gpt-6.1-sol/high，P03工程编排/固定接受链及汇总gpt-6.1-sol/medium，Root/P02当前撤回隐私/P04领域事务/P05恢复继续显式gpt-6.1-sol/high。没有重建Goal、重启在途外部进程或修改产品预算。

实际观测见`evals/results/collab-02/model-dispatch-01/observed-v2.json`。P00和P01已出现对应新turn_context；P03 thread_settings_applied已保存medium，但观测时旧在途turn_context仍high，故只记“后续设置已应用、当前回合未换”，不能宣称旧执行被换模。Root/P02/P04/P05也已保存显式设置。模型名称和推理档位分别验证，不能拿本机默认值或聊天文字代替。

Root后续每次释放切片/阶段必须显式填写model和thinking，并在任务卡记录选型原因；复杂架构Astra/high，一般规划Sol/medium，核心/来源/隐私/事务/恢复Sol/high，普通纯UI Terra/medium，材料提取Terra/medium，QA编排Sol/medium、关键门禁和复杂诊断Sol/high，跨域疑难Astra/high。确定性统计用脚本。实际新阶段由Root按任务类别派发，不允许省略参数继承旧high；完成后核对执行配置。当前高风险切片不为降低单价降档。此项是显式派发流程，不冒称平台已具备自动模型路由或已有质量等价对照结果。
