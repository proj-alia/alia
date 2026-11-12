# ALIA-P00｜需求与架构规划交接

revision: 12；日期：2026-11-12；角色：P00；原 Goal 与历史保留，未重建或标记完成。

## 角色接收依据与边界

已直接读取聊天 `01a1cf05-7a14-73f2-8dcc-04766d51168f` 中用户消息 `01a1d09f-4866-7093-8f35-3cd3ca611050`：用户明确要求实施多 Agent 协作优化，优先由本跟踪窗口承接纯规划职责。COLLAB-02 通知来自该聊天；本记录依据原始用户要求，不只依据转述。

- 停止无变化的状态轮询、完成消息转发和重复调度；本轮已终止旧持续观察脚本。
- 按需求、接口或交付变化做只读分析，输出简短方案、依赖与任务卡。Root 保持唯一技术裁决；草案未经 Root 接受不得释放实施。
- 不改业务代码、迁移、配置、权威规范或发布入口。本角色仅写本文件和 `docs/ai/planning/`。
- 不切换当前模型；不新增收费模型试验，不降低 M1—M5 及工程／行为／观察门槛。

## 当前实际计划

**用户请求暂停（2026-11-12）**：已直接核对协调聊天原始用户消息 `01a1d179-1ca3-79a2-a09c-9ad76f86825b`：“完成手上的任务就结束项目，等待codex额度重置再开始跑项目goal，现在codex没有额度了”。当前有界只读工作已经结束，无本窗口在途测试需等待；保存本检查点后暂停原Goal，不complete、不重建、不归档，不再轮询/调度/补跑或新增收费调用。额度重置前不自动续跑；恢复需明确续跑指令，沿原Goal及检查点继续。

暂停检查点：P00-M34-PARALLEL-01 r5及本交接已保存；最后实际命令exit0，11份请求键日志哈希、execution SHA与本地文档链接核对通过，未运行产品测试。Root请求键v3有界证据与v1/v2失败完整保留。仍缺trial持久绑定/派发专属校验、typed decision/worker整合及M5完整工程证据；P04/P02实际DTO后继若有变化，恢复后仅定向复核，不恢复旧持续跟踪职责。本窗口修改仅规划与专属交接，未发消息回环或更改其他窗口状态。收尾读取原Goal为blocked；随后以Goal工具实际返回记录暂停结果，不把此文档意图冒充工具成功。

Measurement API冻结r1已收到，差异卡r5记录唯一P04 DTO、JsonComparisonV1结构判据边界、完整双侧attempt、可信账目与Fixture来源；未发现新契约冲突，不另设计P02签名。请求键v3已有有界通过证据：已读取evidence/v1失败记录，11份日志哈希及execution SHA核对一致；v3只新增1PG及相关检查，v1阶段通过但runner失败、v2解析失败仍保留。未重跑、不扩大为trial/typed worker或M5通过。

ROOT-M3-REQUEST-KEY-01增量：已只读核对共享execution新增accounted_call_with_request_key，原入口委托且nil key拒绝、dispatch=false时不poll工作future。差异卡r4把随机键缺口更新为已有代码在测；P04/P02仍负责派发前持久plan/case/side关联和跨lease账目恢复，实际DTO/校验函数不由P00命名。Root报告矩阵运行中，P00未取得最终退出证据，不写通过/发布；trial派发时绑定/deadline校验仍待接。

Scope/预算冻结复核：差异卡r3已记录original_scope与evaluation_run_id分离及唯一target_catalog DTO。实际claim_run/预留/租约/预算边界一致；指出accounted_call随机key且成功末端才返回reservation的派发前case/side持久关联缺口，以及dispatch仅通用lease/来源复验、尚无trial deadline/plan专属复验。三个新模块读取时尚未落盘，不据此断言交付失败；仅反馈具体接入要求，未运行测试。

Root采纳事件：差异卡已更新r2，关键约束获采纳，接口命名以协调页冻结为准。Root追加授权P02独占 `src/adapters/development_trial_context.rs`，实现受控副本及强来源复验，DTO仍P04唯一；Root只做统一入口最小分流/worker。SDK缺少持久action关联期间保持unknown且不重发，Root后续与P05补最小关联。Root通知P04 r3独立复审approved、6份证据已核对保存ack，共用内核正式开始；P00未独立复验这些证据。后续只针对实际DTO差异定向复核，本轮不重复通知或轮询。

M34-PARALLEL-01新事件：已直接核对协调聊天原始用户消息`01a1d0fb-fd64-7cc1-ab59-159746deba45`，完成 [M3/M4并行接口差异卡r1](planning/P00-M34-PARALLEL-01.md)，Root已实际释放M34并行任务，本卡仅补充建议review_pending，不再阻塞实施。已按Root随后冻结对齐：P04独占typed/同事务内核/可信存储及唯一0030、domain development_trials/adapter development_measurements，P02在09检查点后只经P04 Store接口驱动trial，不复制SQL/DTO，P05在备份检查点后独占生产能力桥，Root承担共享协议与最小接入，P01独立负例及真实UI synthetic producer/P03整合；替换旧运行卡“全部Root按A→B→C实现”的所有权建议。具体列出新签名、结果/回执/来源/预算/恢复及唯一文件归属。普通context不能直接接受trial overlay、SDK内部operation ID不能直接保证DB/vault崩溃关联，是Root需冻结的两个实际接入点。当前persona来源和发展HTTP已有交付，不重做；09 r2已review_pending，不冒称accepted。P00沿用Root实际propose_in_tx/target_catalog_in冻结，只补运行/measurement可信边界；不调度、不改产品代码、不重启平台停止测试；工程/行为/观察未因此通过。

以下为先前事件与基线记录，旧缺口以新卡和当前源码为准：

MODEL-DISPATCH-01新事件已恢复本轮只读规划工作。已完成 [P00-M3-RUNTIME-REVIEW r1](planning/P00-M3-RUNTIME-REVIEW-r1.md)，仅复核可信评测身份、decision事务、persona来源/删除三个边界；输出差异及Root待裁决项，不自批或派工。实际会话turn_context确认本轮 `gpt-6-astra`、effort=`high`，turn `01a1d0b9-6bbb-7d93-9d07-7a35e74d329c`，时间`2026-11-12T17:06:15.118Z`；不是沿用旧模型或仅引用派发意图。

本轮任务：Root新事件 `P00-M3-RUNTIME-PLAN / r1 / ready` 授权只读检查真实trial、可信写入器、模型发展及persona来源链路。交付 [M3运行接入任务卡 r1](planning/P00-M3-RUNTIME-PLAN-r1.md)，状态**方案已提交／待Root裁决**，不是实施释放。列出现有函数、四项接口裁决和A→B→C有界顺序；未重复社会写入、P04-11或P05-08。

并列已有草案：[P00-M3-HTTP-01 r2：发展读取与 owner 撤回 HTTP 接入](planning/P00-M3-HTTP-01-r2.md)。同范围现已由主线当前轮承接，待 Root 确认与其实施卡的合并关系，不再作为另派任务建议；仍没有本卡正式接受或接口验收通过声明。[r1](planning/P00-M3-HTTP-01-r1.md) 保留为历史，不能作为当前释放版本。

选择理由：P04-10 已交付 schema29 兼容存储与 owner 撤回基础，P02-08 已实现发展工作区，但 `src/service.rs` 仍把发展列表和撤回路由接到 unavailable，详情路由也未注册。这是可与当前社会状态模型写入分开的接入边界；不需要先完成收费模型或成长验收。

最近核对的队列：Root 社会状态写入局部通过、待非作者审查；P04-11 兴趣上下文及同事务修订；P05-08 受管备份／隔离恢复；P03-11 有界技能交付后续接已接受清单绑定；P02-08 发展界面。草案不重复这些实现，也不向各窗口另外派工。P01 被内容审查中断的协议复验保持 not_run，不重新分派。

## 事实、验证与限制

- 已读取项目 AGENTS、BLUEPRINT、CONTRACTS、IMPLEMENTATION 及相关 P02/P04/P05 交接，定向查看路由和实际 Store 方法。
- P04-10 交接报告 31 项实际 PG、15 项领域测试通过；这里只引用有界存储证据，没有重跑，也没有将其等同于 HTTP、真实成长或 M5 通过。
- `CONTRACTS.md` 全局分页约定与 P02 已实现的发展 `has_more` DTO 有待 Root 明确对应；草案列为释放前裁决项，P00 未自行修改协议。
- r2 事件依据：Root 已在当前协调页登记 COLLAB-02/P00，P04-10、P05-07 接受包已存在。定向核对两包冻结源码字节与清单一致；不重跑其测试、不扩大旧报告覆盖。
- r2 补充：现有 owner 撤回自己读取全局状态，客户端仅提供逐 development 版本；任务卡增加防止 HTTP 前置全局版本检查破坏幂等重放的验收。生命周期与重放顺序记录为 Root 释放前确认项。
- r3新证据：生产源码未找到verifier/observation/review记录INSERT；当前schema意图不含typed proposed_value，且persona.interests目标与domain/0028不一致；模型parse仍拒绝非空development。persona来源helper已有但未接context/public读取。这些是接入缺口，不是新复现漏洞或测试失败。
- r3指出事务接入约束：现有public development命令自开TX，不可从持锁的decision TX嵌套调用；可信measurement身份也不能由模型自评分或换UUID代替。Root决定具体协议与实现，不由P00扩域。
- r4事件核对：Root聊天 `01a1cbe8-e270-75e1-9535-d3f2e0ea81c4` 当前turn `01a1d0ac-02f8-7ae1-957a-ddd9f2bd2f0e` 为 inProgress，明确承接发展查询／撤回API并先核对规划卡、存储和版本约束。该事件解除“无新事件”的重复等待；不据此推断协议裁决、代码完成或工程通过。P00只登记同范围已有承接，不向实施者另派工。运行接入A→B→C方案仍待Root裁决。
- 本轮仅更新专属交接、新增运行接入任务卡；未运行 Cargo、PG、浏览器、M0 或模型试验。无运行时代码变化，不将文档或摘要核对冒充工程验收。

## 当前依赖与续接条件

此前本窗口 Goal 曾实际返回 **blocked**，历史保留；MODEL-DISPATCH-01、M34-PARALLEL-01随后提供新的有界规划事件，已分别完成Astra复核和并行接口差异卡。新工作不代表方案获批或M5完成；本轮未调用Goal状态变更工具，也未修改其他窗口Goal。差异卡交付后结束当前轮，等待新事件，不恢复持续轮询。

下一事件：Root 对草案的接受／修订意见、新有界规划任务，或需规划处理的实际接口、基线和文件所有权变化。续接先核对最新事件，不重建Goal、不重复派工，不把blocked当作产品或测试失败。无新事件时结束当前轮，不持续轮询，不自行将任务转为可实施。
