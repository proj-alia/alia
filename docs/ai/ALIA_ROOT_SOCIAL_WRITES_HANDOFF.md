# ROOT-SOCIAL-WRITES-01｜r3 待非作者审查

Owner Root；reviewer P04，隐私交叉部分P05。当前为固定源码的局部工程通过，COLLAB-02的独立审查尚待完成，不声明M2/M3/M5、真实情感或成长通过。

## 实际实现

- 可选`appraisal_changes`≤1、`attitude_changes`≤2个不同topic、`relationship_changes`≤2个不同subject。严格schema、来源、对象、前身与版本验证；不改人格/权限，不让保存状态ID变成新经历事件。
- 作者schema限制实际事件、关系subject/对应版本、目标及态度前身。packet9增加三类逐目标审查，每个supported须覆盖其全部声明事件。旧状态带完整依据作比较，不能借比较资料为本目标补引用。检查上限23，原24KiB审查输出及循环预算不变。
- 原受信任Store方法与模型变化复用事务内校验/写入；原trusted整manifest严格分类保留，模型只检查显式事件分类，允许审查反馈作因果元数据但不能引用为亲历。无嵌套事务。
- 状态、关系版本、记忆、自传、目标、回复、outbox和回执在原决策事务提交；关系修订与随后真实交流分别推进版本。晚期故障全部回滚，已结算调用费用保留。回执新增可省略的三类状态ID，旧回执可读。
- 短期评价到期阻止尚未提交的旧快照；已成功写入的合法幂等回执不因原评价后来到期而失效。但重放必须复查完整原context来源与隐私代次，不能仅依赖可能缺失的通用派生边。

## 固定证据及失败

1. `root-social-writes-v1`：M0通过；新测试误用不存在的StepError::Proposal导致编译101，未启动PG。只修测试为既有StepError::Storage(StoreError::Proposal)，不改生产错误类型。
2. `root-social-writes-v2`：固定413源，16阶段全部通过，**104普通、84实际PG**、严格Clippy/M0/清理/来源稳定。23项默认ignored不计通过。新6PG覆盖共同原子提交/重放/来源与人格不变、重启后态度/关系修订历史、状态独自被拒仍记两次合成调用、晚期全表回滚、审查后来源撤销、到期与缓存重放。
3. `root-social-writes-author-check-v1`：仅增加实际作者schema/body约束单测，1项通过、严格库Clippy/M0通过；未知事件、未知subject、错误版本/前身均被拒。没有重复PG，也不以合成provider证明语义。
4. `root-social-replay-baseline-v1`：同源只加强缓存回执用例，实际**0过/1失败**。删除通用来源边并阻断原事件后，旧实现仍返回已保存状态标识；失败与清理记录保留。这是回执标识来源屏障缺口，不扩大为正文/凭据泄漏声明。
5. `root-social-replay-fixed-v1`：只修重放前完整原context来源检查；固定413源，9阶段通过，**105普通、27受影响实际PG**（social6、belief4、独立history3、grounding9、privacy HTTP5），严格Clippy/M0/清理/来源稳定。原新增断言不改，合法一次性提交、到期重放、重启、晚期回滚和费用保留仍通过。未重复无关完整矩阵。

最终source manifest SHA `ebc03304cb7fa7b857fc958866b2b9feaeb1905ed3c964f646930708ed217378`；report SHA `3a0c710b781134cb8ef10c73a92c2b6592313d0ca066f98da4abd9c23d8966fa`。目录均在`evals/results/`。实际运行采用独占target与候选/工作区/数据库资源锁，0云端模型/生产初始化/部署；合成Live-mode provider仅检验审查、事务与计量门禁。

前序归档：root-social-writes-01（19源）manifest `105642953ebb21e49a859b7fab8ee4635669d06a69f99940d2b66778369a44b5`，其后root-social-writes-author-test-01（仅单测）`e2edbf4b8e4782b7780a1d5e7603054a616d39a1806787703277f716fabf7731`。最终回执修复须在其后覆盖决策与新测试两源。前两包最初附带的是读取切片handoff，此文作为补充澄清，不改写原归档；它们不代替此处写入证据或独立审查。

## 整合与下一步

该固定验证**仍为schema27**，不包含最新development29隐私整合或P04-11新兴趣上下文。live另已登记development/export/privacy_development模块及interest_context可选字段/构造None，属于后继共享整合，尚未据本报告通过。P04-10两源及P05-07六源分别按自身真实29报告归档，不能跨版本拼成完整产品通过。

P04在当前11自然交付后审查此固定r3，P05负责隐私交叉；任何相关生产代码改变需要重新确认。其后推进发展HTTP/撤回、trial实际评估器与认知overlay、兴趣提案、受管备份和隔离恢复，再统一候选工程/正式行为。现有100检索、60×3、20成长及十四真实日门槛、无新增收费trial规则不变。

P01另一个沙盒协议复验被平台内容审查中断，未运行Cargo/Docker，已保留not_run证据并移出自动发现；不重派被拒操作。它不改变本社会状态测试结果，也不补判能力协议安全或执行通过。


## r4：处理P04非作者审查三项发现（待复审）

P04对r3的changes_requested及两项实际RED见ALIA_P04_ROOT_SOCIAL_REVIEW_HANDOFF.md，未覆盖或改写。Root r4 model=gpt-6.1-sol/effort=high，reason=回执来源与已提交状态隐私边界。新增重放检查实际stored DecisionReceipt中全部产物IDs、decision事件绑定以及实际decision_reviews按author reservation/individual/run关联的审查事件；审查事件必须匹配原opaque PreparedDecision，再与完整原context manifest一起检查实际objects归属/阻断。不清空回执字段，不重新执行或付费，不让评价后来过期阻断合法已提交意图。

F3将真实reviewer系统规则输出target清单补齐为全部八类；新增schema分支→实际ReviewTarget反序列化/序列化→RULES字面target说明的交叉契约测试，已有实际请求体测试继续校验发送的system content等于RULES。checker策略指纹自然随规则改变，不改循环预算或费用。

- root-social-output-replay-fixed-v1：10阶段通过，105普通/29实际PG，含P04原样两反例、社会6、信念4、历史3、grounding9、隐私HTTP5，严格Clippy/M0/来源稳定/清理通过。
- v2：新增F3单测误用未导入Value，库单测编译失败；未启动本轮PG。失败保留。
- 最终root-social-output-replay-fixed-v3：仅修测试类型限定，7阶段通过，106普通/17受影响实际PG（社会6、原样独立反例2、grounding9），严格Clippy/M0/来源稳定/清理通过。不重复无关PG。原始独立反例SHA未变。

最终固定manifest SHA `9f8a97f481cca035c28f821f753acf375954271a1324e4b2e2ec3241f3174096`，report SHA `26c96c86c7bd3894dfeee3e1641bc3f67b5b52c116afd117518e05668d826509`。evidence.json逐项绑定日志/实际执行二进制摘要。0云端模型、未部署。当前只是作者修复与定向证据，P04/P05尚未复审，不自批accepted；仍为固定schema27，HTTP schema29另有报告。

计量/二进制澄清：各新运行器report.model_calls=0仅是未接云端的占位，不是合成trait调用总数，metadata-correction.json明确排除。v1在后继复用target后采集的binary hashes不用于v1来源证明；最终v3已持target锁复制其实际日志指向的执行文件到bin，并以binary-capture.json固定摘要。原源码/日志/失败/报告不变。

P04 r4复审已明确批准（open_blockers=0）：独立两项原PG反例及1新协议单测实际exit0，414源/原反例/8封存执行文件核对。Root已逐项比对review-conclusion、独立report/manifest及原测试SHA和两份日志，记录root-p04-review-ack.json。该限定schema27代码意见已接收；P05交叉仍pending，不通过整体READY门，不补判schema29或行为。
