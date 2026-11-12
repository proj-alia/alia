# ROOT-SOCIAL-CONTEXT-01：有来源的状态进入认知

本切片完成稳定兴趣与三层社会状态的**读取、注意和回复依据链路**，没有新增模型情绪/态度/关系写字段，不声明M2/M3整体或真实情感、成长通过。

## 实际变化

- 出生事务初始化稳定兴趣；上下文读取实际active记录及完整来源，不再按当前人格数组重新分配ID。新目标提交复查active对象和manifest，退休不重置出生种子。
- 同一RR快照带入当前短期评价、最多4项长期态度和4个关系。来源沿共享结构图及实际关联取得，最多1024对象/项、整体96KiB；失效或超限整包省略并标has_more。关系原始快捷投影不能绕过较强的来源检查。
- 短期评价按真实有效期选择，预留和提交拒绝过期快照；到期不降低长期态度/关系。注意排序保留当前运行目标，其余原priority仅临时增加最多10分相关权重，不改持久priority、人格、权限或预算。
- 审查包升至target-citations/8。保存状态只可为回复中的自我理解提供依据，附完整basis_events，明确不能证明他人感情、世界事实或主观意识；不能当新记忆/信念/人物/自传的实际事件。实际作者请求和审查器均加入该规则，不增加调用数或原审查响应上限。

## 固定输入与实际测试

固定目录`evals/results/root-social-context-v3/source`共411文件，从root-autobiography-write-v3加22项明确overlay组成。**范围是schema27**；没有删除live的0028/0029，也没有将尚在实施的发展隐私代码混入验收。

- 主报告`checks/report.json`：16阶段全部exit0，102普通测试通过，17个默认ignored未计通过；89项实际PG全部通过，0 ignored。
- 同源补验`history-regression-checks/report.json`：原独立历史3项实际PG通过、定向编译和严格Clippy通过。三项只更新协议版本断言，原目标/前身遗漏、65项完整历史上限和晚期事务回滚断言保留。
- 新`tests/social_context.rs`：3项纯测试及6项实际PG，覆盖状态ID的证明范围、完整来源/权重/有效期校验、当前目标优先与有界排序、重启和只读不改状态、短期到期保留长期关系、无一般派生边的撤销/派发屏障、整包大小、真实目标priority及兴趣重命名/退休与种子不变。
- 实际PG还回归affect6、relationship_revisions6、interests6、autobiography_context5、autobiography_decisions5、belief_decisions4、grounding9、context_privacy3、scheduling14、persistence17、contact8。合计主89+补3=92项，不与普通测试重复计数。
- 固定源M0、定向严格Clippy、专属PG清理及全部411源前后摘要一致。0云端模型调用、0生产初始化/部署。合成模型/审查工程夹具不证明语义质量。

主source manifest SHA：`e8848cc0e08a13a93075589f2a1edcc472155e6b11f4d6b9049ea3c5ff13f92c`。主report SHA：`f895450ad39264cc5d887cfa75c1454a58437e1c49c469d37ca0d69953453d78`。补验report、实际日志和测试可执行摘要绑定在同目录`evidence.json`。

22源接受包`accepted-slices/root-social-context-01`，manifest SHA `76f60fc69c5da7dd8bdedbc7870893a492c08376ad9338e150f69173ecbff4ac`。之后仅修正adapters.rs模块声明排序和proposals.rs三处空格，另包`root-social-context-format-01` SHA `67ad0520270ce1ff1ddd21fcf9cfc40f602a0772e044c26441537434e9aba82f`；程序逐字验证它是对上述实测源码执行rustfmt后的输出。没有声称另跑格式版数据库测试。当前root自有改动定向rustfmt检查exit0；deletion.rs归P05后续冻结与格式检查，不覆盖其在途FK修复。

## 保留的失败

- v1：新6PG及多数回归通过，但原scheduling14中1项错误码兼容失败。新的social校验抢先返回StaleState，原缺关系manifest契约应为Invalid。v2把完整social校验放在原manifest校验之后，过期检查仍在预留重放之前；原断言未改，v2/v3原14项全部通过。
- v2：102普通和89实际PG通过，但严格Clippy因Option查询可用`?`而失败。v3等价改写，未禁用lint或降低门槛。
- 新单测准备时一次括号格式错误由rustfmt发现，补正后才冻结v1；没有伪称该准备检查通过。所有实际失败报告保持。

## 下一依赖

继续实现appraisal/attitude/relationship的结构化变化建议、逐目标独立审查和同事务提交；原受信任存储不能在主决策内嵌套新事务。再完成稳定兴趣模型修订、人格trial与真实评估器，以及schema29发展来源/删除重建接入。P01正在固定v2上作独立社会来源负例审查，结果另行接收；本报告不预判。

新0029仅将隐私移除与发展状态分开、允许合法新head占用原逻辑槽；P04/P05仍在验证单调版本、seed基线、真实投影与旧日志重放。当前92项schema27证据不覆盖0028/0029、新API、UI、受管备份或真实成长。既有完整N5R3只覆盖其固定源码，集中收费验收和十四天真实观察仍未开始。
