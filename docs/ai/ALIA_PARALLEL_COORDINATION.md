# ALIA 并行交付协调

当前暂停交接：Root已核对协调聊天原始用户消息`msg_01a1d179-1ca3-79a2-a09c-9ac14f6f716a`（2026-11-12T20:35:37.763Z）。手上固定验证v8于20:40Z终态通过、owned清理完成后停止推进，保留七角色、原Goal及全部历史；不新增派发、补跑、收费或部署，额度重置不自动恢复。恢复入口在[运行交接](ALIA_ROOT_M3_RUNTIME_HANDOFF.md)文末和IMPLEMENTATION当前状态；112实际PG/HTTP定向通过不等于全项目或真实成长完成，后续审查待用户明确恢复。

本页只记录当前分工、正在交付的切片、依赖和验收链接。项目进度与里程碑状态唯一以 [IMPLEMENTATION.md](../IMPLEMENTATION.md) 为准；详细规格及原始证据放在各任务交接和 `evals/results/`。旧任务记录已冻结在 [ALIA_PARALLEL_COORDINATION_HISTORY.md](ALIA_PARALLEL_COORDINATION_HISTORY.md)，不构成当前授权。

## 当前分工与交付队列

| 窗口 | 固定职责 | 当前交付 / 下一依赖 | 交接与验收 |
| --- | --- | --- | --- |
| P00 | 只读需求/架构规划与任务卡，复用原进度跟踪窗口 | COLLAB-02切换；按需求/接口/交付事件规划，停止无变化轮询转发；Root仍唯一技术裁决 | 本页、COLLAB-02交接 |
| Root | 权威契约、迁移、共享路由、核心认知与原子整合 | persona来源r3修复同key恢复RED，83普通/86PG及严格检查通过、P04限定批准已核对/P05待审；social r4和HTTP r1同样P05待审；现接M3共享schema/decision委派 | [IMPLEMENTATION](../IMPLEMENTATION.md)、对应 `root-*.json` 报告 |
| P01 | 行为、成长与负控评测材料 | 社会读取3PG与8组写入离线场景已交付；协议复验被平台中断/not_run并移出自动发现；纯文件证据索引已交付 | [M3审查](ALIA_P01_M3_REVIEW_HANDOFF.md)、[隐私反例](ALIA_P01_PRIVACY_REVIEW_HANDOFF.md) |
| P02 | 管理界面与数据工作区 | 05删除和07真实SSE清屏已接受；08发展详情/撤回界面实施中，等待真实API后定向验收 | [P02-05 handoff](ALIA_P02_05_HANDOFF.md)、[P02-07 handoff](ALIA_P02_07_HANDOFF.md) |
| P03 | 固定快照、工程回归、QA 资源与报告 | 11已交付后回归QA；05→07有序来源绑定及COLLAB-02门禁接入中，旧FLOW失败保留 | [P03 handoff](ALIA_P03_HANDOFF.md)、[并发隔离证据](ALIA_P03_CONCURRENCY_HANDOFF.md)、[FLOW-01失败报告](../../evals/results/p03/flow-01-candidate-v2/report.json) |
| P04 | 记忆、人格/认知修订及领域存储 | 10/schema29已交付并归档；11兴趣完整来源/修订r2已交付待P05审查；Root社会写入独立反例已复现 | [P04-09B handoff](ALIA_P04_09B_HANDOFF.md)、[P04-10](ALIA_P04_10_HANDOFF.md) |
| P05 | 隐私删除、资料与个人数据生命周期 | 07/schema29删除重建/导出37PG已交付；08真实受管备份/隔离恢复实施中 | [P05-07 handoff](ALIA_P05_07_HANDOFF.md) |

一个窗口一次只保留一个活跃编码切片。当前任务交付后沿同一职责窗口接续已登记的下一项，不新增 Goal 窗口；Goal 数量不作为吞吐指标。Root 不要求并行窗口等待完整工程矩阵后才开始其独立工作，但共享契约及源文件所有权不变。

## COLLAB-02 当前审查与所有权

原始用户批准消息`01a1d09f-4866-7093-8f35-3cd3ca611050`已由Root直接核对；此节覆盖早期职责/交接规则，历史证据保留。首次完整读规范，续接按revision/哈希及变更章节恢复；通知只含ID/revision/state/evidence，按版本去重。P00只读规划，不重复Root派工或改生产代码。P03当前11自然交付后回归QA，不再长期承担产品实现；技能后续维护归Root。

| 切片 | owner | 非作者reviewer | 当前依据与门禁 |
| --- | --- | --- | --- |
| ROOT-SOCIAL-WRITES-01 | Root | P04；隐私部分P05 | r3 changes_requested保留；r4原样反例通过、P04复审批准且Root核对，P05待审，整体review_pending |
| P04-11 兴趣来源/修订 | P04 | P05 | r2工程就绪，待P05及Root；typed helper未据此接通模型写入 |
| P05-08 备份/恢复 | P05 | P04；Root接运行边界 | 实施中；必须真实隔离恢复与独立删除日志，不以个人导出替代 |
| P02-08 发展工作区 | P02 | P03工程，P01来源/语义 | 当前80合同检查/5未就绪浏览器，真实HTTP仍待接入；不计完整功能通过 |
| P03-11 SDK/技能vault | P03 | Root代码，P01能力证据 | 在途尾项自然完成；被平台中断的独立协议复验是not_run，不换窗口重派，不补判安全通过 |
| P03后继绑定/QA门禁 | P03 | Root+P01 | r1 P01发现卡外新源码可复用旧释放，changes_requested；Root释放r2完整候选内容摘要绑定及增删改反例，Sol/high；不改COLLAB helper、不启动完整矩阵 |
| COLLAB-02小型协作/用量工具 | 协作方案窗口 | P01独立审查，Root技术接受 | r4固定五源已核对，P01独立8+8通过并批准，Root真实task-card ready检查exit0并接受四源；证据collab-02/r4/root-acceptance.json。P03仅接前置集成。r1—r3不批准保留；不证明身份鉴权/实际通知/账单或产品验收 |

切片交接需包含task_id/revision/owner/reviewer、baseline/源码及接口版本、依赖、允许修改范围、实际命令/退出码、审查发现与处置、失败/未运行项和本版增量。状态defined→ready→implementing→review_pending→engineering_pending→accepted，依赖阻塞另附解除条件；与App Goal及行为通过分开。复杂切片不能靠拆小免除方案/反例/审查，简单变更允许精简流程。审查后相关源码变化则结论失效，重新确认。

确定性失败先诊断，环境安全瞬时重试至多一次，同故障两次修复未改变即独立审查；结果未知先恢复状态/费用。工程完整矩阵由P03按固定候选统一运行，已有效的旧完整报告不重复跑；真实行为及十四日门槛不变，M5工程候选前不新增收费trial。开发用量只作独立证据口径，不把缓存请求总token或Goal累计数当实际账单。

## READY 与整合节奏

一个切片只有在以下内容齐全时才可登记 READY：精确改动文件/源码哈希、实际命令和退出结果、实际覆盖与未覆盖项、失败及限制、下一依赖、独立报告路径。未运行项写明 `not run`，不能按通过计算。失败结论和旧报告保持不变。

自本流程启用后，每累计 **2 个彼此独立的 READY 切片**，Root 组装一个不可变候选，P03 对受影响依赖运行整合检查；每个候选仍只允许一个完整工程矩阵运行。候选失败按真实失败修复并保留原报告，不用局部检查抵消。未满 2 个 READY 切片时继续开发，不因等待候选阻断其他独立切片。

## 测试资源和锁

- P03 并发隔离证据已通过Root审阅：同父任务下真实独立构建/产物、双PostgreSQL开放事务写入、Alia/Chromium服务和中断清理均有重叠记录；已知限制和原始哈希见隔离交接与报告。
- P03 报告必须覆盖独立且不可覆盖的源码快照/构建目录/结果目录、互不相同的测试数据库容器/卷/端口和数据、随机浏览器服务端口、并发运行不串写，以及中断/部分启动后的精确归属清理。报告记录命令、哈希、实际并发重叠区间、资源身份和清理结果。
- 证明通过后，QA 已改用候选、target、数据库管理、单个数据库、浏览器输出、沙箱和工作区资源锁；嵌套子进程继承并核验锁FD身份。完整候选仍持候选锁至全部阶段和清理结束。未经实证的共享资源仍串行保护；任何缺少隔离证据的类别不得继续拆分。
- 禁止生产数据库/个体、生产凭证、付费模型调用和部署参与隔离测试。当前“不新增收费模型 trial”的规则继续生效。

## 流程指标

每个固定候选在 [IMPLEMENTATION.md](../IMPLEMENTATION.md) 记录 READY 切片数、等待整合时间、锁等待时间、整合返工数及报告链接。FLOW-01在两个READY后启动，但接受源绑定门失败；先补归档7份原有接受清单，Node/npm环境也已配置，第二次仍发现P02-03/04六条哈希记录无已绑定后继（涉及五路径）。Root随后逐项核对05-full-v5和07-final-v3的READY、完整冻结源及实际报告，已归档accepted-slices/p02-05（manifest ee553288cfe5c2437c6b4635d1c40f6bdfdf83617f7b8c1953f303c7bf036b58）和p02-07（234a4608bab9b263db68454c5624b20419f344a0b11aa5fec82f7bb5181b797f）。这是合法有序后继，不是直接接受live哈希；后续仍需P03更新绑定和负例测试、冻结新候选并实际运行。旧FLOW失败不覆盖，也不算完整矩阵周期。至少两个完整周期后再判断提速。


## MODEL-DISPATCH-01 / ROOT-DEVELOPMENT-HTTP-01 当前释放

模型派发按AGENTS的MODEL-DISPATCH-01执行：新切片/阶段工具参数必须显式model/thinking，卡中记录reason，并在回合边界核对实际turn_context。在途进程继续原handle，不能为了切模型重跑。原用户两条消息已核对；工具保存配置与实际回合采用分别报告。具体模型/档位表在AGENTS维护，避免多处独立版本。

ROOT-DEVELOPMENT-HTTP-01/r1：Root owner，P04审事务/对象版本、P05审来源/隐私；model=gpt-6.1-sol，effort=high，reason=涉及真实状态回滚、幂等恢复与删除屏障。采用P00-M3-HTTP-01/r2，列表为include_history/limit/has_more有界快照；详情强ETag与POST If-Match均为development.version，reason≤1000 Unicode字符，回执七字段。当前review_pending，64实际PG/HTTP及81普通定向通过，P04/P05独立审查未完成，不释放为accepted。P02-08继续等待固定真实接口及可信测试记录，不重做原未就绪检查。

ROOT-SOCIAL-WRITES-01/r3独立审查两个反例：直接阻断新输出对象、阻断作者上下文之后生成的审查事件且缺通用边时，旧回执仍返回；原失败保留。Root增加全部回执对象与实际review引用检查并补齐八target规则，最终固定schema27的106普通/17PG通过；非作者审查仍待确认，不因作者修复而自批。

## ROOT-PERSONA-SOURCES-01/r2

状态review_pending，scope-growth真实RED已修；生产89普通/94PG通过、仅测试import清理后的8PG/严格检查通过；owner Root；reviewer P04事务/版本、P05来源/隐私；model=gpt-6.1-sol/effort=high，reason=人格来源跨上下文、预算预留/派发、提交及公开返回。采用P00运行方案A的第一部分与Astra三项差异，B/C及typed proposed写入继续保留，不能以来源接线缩减M3。

唯一修改范围：新domain/adapters development_context.rs、Root共享注册/cognition/context/model/grounding/scheduling/decisions/api/console，development.rs仅LINKS/available/immutable_seed的pub(super)可见性；新development_context实际PG及支持fixture，既有测试只同步packet10断言。采用固定HTTP-v5/schema29与social r4两生产源；P04-11仅为已存在可选字段提供固定r2 domain DTO（不接其新兴趣写helper），P05-08与SDK代码不进入本切片。Root可继续自身实现，不等待其他切片全矩阵；定向结果/独立审查/发布分别记录。

P02-08现有固定r2保持review_pending（全记录browser仍依赖Root有界producer）。P02下一唯一有界P02-09：独立复现并修补/state源投影拒绝后旧private state/DOM可能残留；owner P02，reviewer P03工程/P01来源，Sol/high（异步隐私状态）。先复现，范围web/src/shared/useIndividual.ts及新09测试/自有handoff，其余共享文件需精确接口协调。不能改Root来源校验或绕过unknown写/dirty机制，不把08的未运行分支判通过。

该片r2保留首次作者来源全集精确匹配；review/commit保持原观测快照并强查实际FK，不因run输入合法增长误拒。review reserve必须绑定实际settled作者原context摘要；不是以phase布尔绕过来源。新固定v5/subject1aac285f1181262cba6161421e6cc0c172ae19792c7bec384a468004cff39a23，P04/P05重新审本revision，r1和所有失败保留。

最新r3（替代r2当前结论）：P04 r2为changes_requested，新增F2同key未派发恢复RED；Root在有效lease和来源门后先认证实际individual/run/request_key及context/allowance/generation，同原请求才复验绑定阶段，首次key仍来源全集精确匹配。固定v6/subject6c31a68f153f6ae2df43be7c8abb721942f130268b788a64b3bebfb74bfa8310通过83普通/86实际PGHTTP，两个P04原反例未改且通过，严格检查/清理通过；P04/P05按r3重审，仍review_pending。r2/v5全部失败和独立证据保留，未开始全记录UI/正式成长/十四日。

## M34-PARALLEL-01：实际运行闭环并行释放

ROOT-M3-ATOMIC-01/r1第一段固定v4已交P04/P05审查，109实际PGHTTP及严格检查通过，详见[交接](ALIA_ROOT_M3_ATOMIC_HANDOFF.md)。P01-M3-UI-PRODUCER-01获Root限定非作者审查批准（实际6guards、486源/两个bin摘要核对，原真实Store/catalog/删除报告核验），`evals/results/root-p01-ui-producer-review-v1/review-conclusion.json`；仍待P04，不能当完整UI或生产verifier接受。

当前Root与协调窗口的模型文件边界：用户最新指定opencode_go中的DeepSeek V4.1 Flash；Go profile/对应请求计量及必要tokenizer新增已明确有界交协调窗口实施，Root不并发改这些区域，协调者须保留当前M3 typed schema/review逻辑。Root继续scheduling/decisions/goals/worker等共享试用接线，不等待模型切换大整合。自然测试仍P01唯一执行者、同campaign/DB/identity/原账本不重置，旧两轮Go/GLM误配5次调用28606已结算独立标注，货币未知；不得混称DeepSeek结果或自行再发GLM。

Root已直接读取协调聊天原用户消息`01a1d0fb-fd64-7cc1-ab59-159746deba45`：M3/M4剩余运行闭环和生产接入拆给现有Goal并行实施，不降低验收标准、不只改文档。以下已通过现有聊天实际派发，均显式Sol/high；P00收到Astra/high只读短接口复核。开发依赖不等于发布依赖，不以收费行为门槛阻断编码；在途P05-08自然稳定检查点后接续，不中断进程、不新增窗口或重置Goal。

| 实施切片 | 唯一owner/范围 | 非作者审查与当前边界 |
| --- | --- | --- |
| P04-M3-PRODUCTION-01 | P04：domain/development.rs、development_trials.rs；adapters/development.rs、development_measurements.rs；必要0030迁移及专属PG | Root事务/模型、P05来源/恢复。已开始共用事务内核重构；r3独立复审在稳定检查点进行，不把现有typed候选或SQL夹具当生产评测 |
| P02-M3-TRIAL-RUNTIME-01 | P02：新runtime/development_trials.rs、adapters/development_trial_context.rs及专属测试、原自有development UI | P04状态、P01来源、P03工程。依赖P04真实plan/attempt/observation DTO；新adapter实施受控trial副本及强来源校验，Root在统一入口最小路由与接worker。09已到作者交审检查点，08完整有记录仍未验 |
| P05-M4-PRODUCTION-BRIDGE-01 | P05：新adapters/learned_capabilities.rs、runtime/learned_capabilities.rs及专属测试 | Root生产权限、P04事务；08备份在途先自然交付。复用原SDK，未经验证执行仍disabled；原被平台停止协议保持not_run，不改写/换窗口重试 |
| P01-M3-UI-PRODUCER-01 | P01：独立examples/development_ui_fixture.rs或确认后的专属QA入口、scripts/qa/development_ui_fixture.py及测试 | Root+P04。按P02-08机器契约真实Store转换生成明确synthetic工程记录；拒绝不符/非空/生产DB，不直接写accepted/review通过行；不当成长或生产verifier证据 |
| P03-12/r2 | P03：既有QA候选绑定/后继清单及负例 | Root+P01。卡外源增删改必须使旧候选释放失效；完成后接统一实际工程整合，不自造接受登记 |
| ROOT-M3-ATOMIC-01 | Root：cognition/schema/示例、model/grounding、decisions总回执/原子委派，context/worker/能力registry最小注册 | P04+P05。不接管前三项模块内部实现，第一段只产生真实proposed；试用/接受仍需独立真实评测 |

当前冻结的P04提案接口：`propose_in_tx(tx, individual, input_manifest, proposer_id, &DevelopmentDraft, event_key: Option<&str>) -> ProposedDevelopment { development_id, event_id, target_id, development_version }`。内核不commit、不advance、不写顶层receipt/outbox；外层decision或旧public Store包装器一次统一推进。Root直接用typed proposed_value，目标before_version来自真实catalog，不用全局版本；P04提供只读target_catalog_in，含removed历史generation基线与精确goal/task作用域。稳定兴趣保留P04-11独立ID路径，不新增persona.interests副本。

P00短建议见[并行接口卡](planning/P00-M34-PARALLEL-01.md)，其中尚未采纳的名字/签名不覆盖本节实际Root冻结；评测plan/attempt及runtime context分支仍需按实际DTO短冻结。P04是0030唯一编辑owner，Root负责最后canonical编号/发布注册，其他窗口只提交精确字段需求，不能抢写迁移。SDK调用关联缺口由Root与P05确认；没有真实关联时保留unknown而非重发。

Root共享依赖交付ROOT-JOURNAL-NONBLOCK-01/r1：已采用两个准确非阻塞journal/replay源，原10+新2文件测试及18实际PGHTTP/strict通过，固定root-journal-nonblocking-v2 subject5924e0593cc1843334f1c9842da539505b4993b2220151e547aa270cec692610；P04已限定批准，Root核证记录root-peer-review-acks-20261112-v1（仅2独立文件反例，不是新增PG）。原API缺失这一08依赖条件已消除，P05自行绑定r8 manager并做受影响检查，原子hook和完整08审批仍单列；不能因旧Goal blocked推迟已经正式释放的M4新模块编码。
