# P01-HISTORY-REVIEW-01｜READY：有限审查与3项新增PG通过，未复现新绕过

范围：只读审查固定 `root-autobiography-write-v3/source` 的模型自传提案、独立逐目标审查、完整历史/反证/前身与原子提交。仅新增 `tests/history_boundary_regression.rs` 和本交接；不编辑生产源、原测试、规范、阈值、其他worker或在变情绪/关系/导出，不迁移EVAL-03/collector，不调用真实模型或收费接口。

## 固定输入与旧证据

已逐项验证402个冻结源、root-history-03接受包十五源及既有report SHA。base manifest SHA `1ee798cd2d8d0ab6a72727cb0e4542dea2b4ee4d42c26d8c7120642bd876a8a7`；accepted manifest SHA `60138982a7a8dd452bfb121f890ef32742281e1ee7ded90093d6f2b27fba571a`；原报告 SHA `2b9301f6fcbde3e777b5bacb0e004a7b70378741a0e72900e9a02b48e4a7c8b6`。原15阶段、95ordinary/79实际PGHTTP是原定向工程结果，审查意见为合成，不能当语义评测；本次不重复该全套。

原新5组实际覆盖：一条解释写入/修订/重启/幂等；不支持的history-only候选计费拒绝；发布晚期故障回滚history/其他写入；第一条解释读取完整current/previous stance；来源撤销拒绝prepared提交。旧存储6组包含前身、来源、同键及回滚；上下文5组包含旧立场/反证、撤销、超预算整包省略、预留/派发失效与HTTP timeline；grounding9组覆盖实际独立计费/一般目标覆盖/协议修复、原子发布、并发输入、3调用上限及计费绑定。这些均读取原源码与日志核对，未冒充本次运行。

## 控制流审查与新增覆盖

`DecisionProposal::validate_context`逐条验证两项topic唯一与真实选中来源；AutobiographyDraft只接受events中的实际来源与完整autobiographical_context.belief_basis，前身须选中同topic当前解释。上下文以RR读取完整立场前身和支持/反证，64 belief/1024对象/96KiB超限整包省略并has_more，不静默截掉反证。实际DB再次复验来源、前身版本及完整stance basis。

packet7使用historical_belief、autobiography等类型化记录与非世界事实proof_scope；历史立场保留current/superseded、complete_lineage和support/counter。DecisionAssessment要求全部target恰好覆盖，autobiography的显式event/belief/predecessor引用须与supported evidence集合相等。Live model_step调用独立审查并计费，commit再绑定候选/上下文/审查/策略摘要、租约与实际来源。两条解释和belief/memory/reply在同一外层事务，模型不能取得PreparedDecision私有字段。

没有从静态控制流确认绕过；存在下述具体未单独实测组合，因此只新增三个实际PG组：

1. 两条历史都存在时，合成审查分别漏第二个target、或target齐全却漏第二条前身引用，仍声称Supported。实际Live门须拒绝，不落解释/回复/commit；保留3调用上限及已消耗假账目。同时核对实际packet提供完整旧立场/反证/前身类型。
2. 使用正常Store命令产生65个有来源的立场修订，触发完整历史basis因64上限整包省略。提案试图仅凭仍可见的当前stance ID重新引用它，须在作者校验拒绝，不能进入审查或写历史。不是伪造DB状态或篡改context。
3. 两条当前解释同时修订并含belief/memory/reply；QA触发器仅在第二条INSERT失败，须回滚第一条前身替代与全部兄弟写入。清除QA故障后用同一不可变PreparedDecision重试、再并发重放，实际回执/状态不重复。最后撤销旧反证来源，同一prepared回执必须拒绝重放，费用保留。

所有proposal/assessment均为明确合成的不可信输出。ProviderMode::Live仅使程序运行真实Live校验/预算/事务路径，不进行HTTP/云模型调用；合成Supported永不认证客观事实或自然语言忠实度。只操纵公开提案/审查返回、正常Store初始化/修订、既有来源撤销及隔离QA故障；不修改PreparedDecision私有状态。队列取消仅用于选择实际新前台测试run，不伪造审查结果或完成状态。

## 执行状态

独立输入为402原源不变加新test，共403源：`work/p01-history-review-01/source`；manifest SHA `cace3d11135114407c4f4e2154473d2c8fae4d1a6cdb2cb231aa183140c3ca2c`；test SHA `3a2d9cf4e1e1efc8cc000003002149b863306da46f445afacb0f693b2ee65a45`。使用运行前不存在的 `work/p01-history-review-01/target-private`，禁用wrapper，未复用共享rmeta/rlib。全局重资源锁实际覆盖首次编译、私有PG、测试与精确清理；下表列实际终态。原402源与新403执行源均全量哈希保持。


## 实际结果与命令

在以上固定版本和明确合成输入范围内，未复现跳过独立审查、漏完整历史依据后强行引用、两条解释部分提交或撤销后重放成功的新绕过。没有把合成Supported判定当作语义通过，也不宣布隐私/历史安全完整。

| 命令（除格式外cwd为独立403源） | 实际结果 |
| --- | --- |
| `cargo test --locked --test history_boundary_regression --no-run` | exit0；独立空target首次完整编译57.45秒，此后才创建私有PG。 |
| `cargo test --locked --test history_boundary_regression -- --ignored --test-threads=1` | exit0，3 passed / 0 failed / 0 ignored，实际测试5.50秒。只运行这三个新组，没有运行原95ordinary/79PGHTTP。 |
| `cargo clippy --locked --test history_boundary_regression -- -D warnings` | exit0，24.93秒。 |
| `rustfmt --edition 2024 --check tests/history_boundary_regression.rs` | exit0。 |
| `/workapps/project-alia/alia/.venv/bin/python scripts/check_phase0.py`（固定403源） | exit0；6 schema、53 closed objects、13 examples、68 negative cases。不是对当前在变core的通过声明。 |
| 全局锁与精确cleanup | 父进程exit0、cleanup=true；持锁覆盖全部重操作。清理后独立按原label/ID再次inspect，容器和volume均不存在；私有state已移除password。 |

第1组实际执行两种不完整assessment，各1次作者＋2次合成审查，在原3调用上限内拒绝，分别保留3条settled/6假token；作者没有重新生成，合成协议修复重试没有改变候选。旧两条解释仍原样存在，没有commit或回复。实际packet断言每条自传均含historical_belief完整两代lineage、superseded状态、带counter=true的原反证及类型化前身解释。

第2组实际上下文has_more=true、完整历史belief_basis为空，而当前stance仍在普通selected_beliefs；不可信提案试图重新引用该ID，返回ProposalError::Autobiography。仅作者1次/2假token，独立审查0次、历史和commit均0；没有静默截断依据后保存。

第3组实际第二条INSERT故障返回DB错误，比较完整history/belief/memory行及event/outbox/commit/outgoing计数、状态版本和账目，与故障前相等。移除隔离QA故障后，同一未篡改PreparedDecision提交得到2个解释ID/1 belief/1 memory/回复；随后两次并发同候选重放返回相同回执，完整快照不变。两条旧解释均superseded、只有两条current。最后撤销历史反证，回执重放返回StaleState；作者和审查各1次、2条settled/4假token保留，不新增收费。

以上合成provider断言合计9次程序调用/18假token，只说明工程账目与上限，真实云模型/收费调用均0。首次新target执行即3过，无新增编译或断言失败、没有修改原测试或降低阈值后重试。既有root-autobiography v1/v2失败、SSE旧RED/v2通过与EVAL-03材料保持，不重写或跨版本冒用。

原始证据在 `evals/results/p01-history-review01-v1/`：report.json记录实际命令、时长、manifest/test/runner/log SHA、私有PG公开身份和cleanup；三份执行日志、输入核对、完整403源manifest、格式及固定M0日志随新READY交付。实际新测试二进制 SHA `7367831691b4d70ad7e771581f33a8371786254b83149de54ceea4d633d3cd2f`，日志确认由 `work/p01-history-review-01/target-private/debug/deps/` 执行，运行后再次核同SHA；不使用旧共享产物代替新行为断言。

## 限制与下一依赖

审查仅绑定402固定源和上述三个新增组：未执行真实独立模型语义审查，不能证明自然语言理解、当前/旧观点区别或共同经历陈述总是正确；合成意见并不认证世界事实。未穷举所有并发时点、预算/租约组合、历史长度、数据库硬崩溃或恢复产品。snapshot等值不等于证明所有未来schema字段都已纳入；当前实际表/字段由固定输入限定。没有使用任意PreparedDecision私有字段篡改、同uid文件读取或任意源码改写假设。

既有通过记录仍是局部工程，M1—M5行为与十四真实日观察未由本次完成。原100检索/60x3/20ABC门槛与EVAL-03正式unverified/null保持；不适配最终collector，不运行付费模型、自动付费重试、部署或其他worker任务。

下一依赖：root可接受新test与本交接，在后继命名整合候选按其精确源码绑定运行；情绪/关系/09B/导出接入继续由其owner推进。本次无生产修复建议需要实施；最终独立语义评测仍待完整M5候选及明确真实采集/预算释放，不将有限审查写成安全完成。
