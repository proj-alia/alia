# P01-SOCIAL-REVIEW-01｜READY：固定v2新增3PG通过，已知Clippy101保留

仅独立审查 `evals/results/root-social-context-v2/source`，拥有新 `tests/social_boundary_regression.rs` 与本交接。生产、旧测试、collector和付费材料不改；不纳入后续社会状态模型写字段，不运行真实模型/收费/生产部署。最终仅向root交接，等待其限定后继，不自行切collector。

## 精确输入与已有结果

已全量核对411源，base manifest SHA `4b4bdaf369f48b65f3dd22c031cac35bd5ac719399cb5fb0fff080ef28ab8604`。v2原报告 SHA `c174a0c645b47cdd7902ed6556bf47f7ee906fd511915d1e9467a8320b6240e0`：social原3纯/6PG已过，scheduling原14也已实际exit0；overall passed=false，严格Clippy因 `src/domain/social.rs:70` 的question_mark lint退出101。原v1的scheduling 13过/1错误码兼容失败保留，本次不把v2已修正的检查顺序重新列为漏洞。

root随后通知最终v3通过并作等价Option/?及格式修正，同时明确本次继续固定v2；本次没有重启、换源、冒用v3检查结果或扩展范围。v2的既有Clippy失败作为精确输入限制单独报告。

## 控制流与原覆盖核对

social_context把appraisal、长期attitude、relationship分型，在同一RR快照通过实际reference_graph_sql求完整闭包；含旧来源/前身、显式event全文和manifest，超1024对象/96KiB整包省略并has_more。appraisal按真实时效选取；关系与长期态度不因短期状态到期自动惩罚。回复依据packet8标为social_interpretation，只支持当前自我立场，不能认证世界事实、对方感情或主观体验。

DecisionProposal把选中状态ID加入回复visible集合，但memory/belief/person/autobiography的新事件字段仍分别检查真实events；审查非reply目标也拒绝把social ID当事件。预留与提交复查appraisal有效期，租约/来源/权限/预算继续程序控制；稳定兴趣由当前active且完整来源的持久记录选择，目标提交再次校验，不由当前persona数组重排重建。

读取原social_context测试源码与实际记录后，原覆盖包括：真实重启三层恢复/读取不写身份，过期appraisal拒绝预留且不降低长期态度/关系，单来源撤销去一般边后派发拒绝，超大来源包整包省略，有限注意排序不改持久priority，稳定兴趣重命名/退休不重新初始化。原纯测试只直接覆盖人工上下文的appraisal→memory错误引用；本次不重跑上述全套。

## 三个新增有界组合

1. 从实际PG上下文读取appraisal/attitude/relationship，合成Live模式provider的自我立场reply可通过packet8独立程序审查并提交；实际packet断言3项均为social_interpretation及限定proof_scope。随后分别先验证使用真实event的合法memory/belief/person/autobiography候选，再只将事件引用换成实际选中状态UUID，通过model_step断言作者校验拒绝、无review调用、无语义写入。
2. 通过真实Store输入8条各12000字节的有界文本，record_affect绑定完整来源，再用12条小资料移出最近事件窗口；不改写保存的服务器事件正文。实际上下文须整包省略appraisal/attitude，has_more=true、无截断来源或状态ID残留。提案引用记住的省略状态UUID仍须拒绝，不能升级为事实依据。
3. 正常Store建立两代关系修订，再创建无关来源的appraisal/attitude；预留时完整manifest包含旧来源/两代revision。仅模拟已有定义的来源撤销并移除一般derivation_edges，实际previous revision/FK必须继续阻断旧预留派发并从新上下文隐藏该关系；无关评价/长期态度仍可见，runtime规则/persona不变，无派发标记或模型次数增加。

只操纵不可信proposal和允许的来源撤销/缺索引QA；不篡改私有PreparedDecision、ModelContext或可信服务器记录来制造模型攻击。ProviderMode::Live与Supported均为明确合成工程意见，不能证明自然语言忠实度或现实情绪/关系质量。

## 执行状态

使用411固定源加新test共412源，执行manifest SHA `ef162f4852b0710b19b017ae6ff31f87a0c7eb2525ba29c5259263bd850efc80`；test SHA `6fdec23d0521663e090c69314505c24304054f1704c58b88f0d7cd67f970eb08`。`work/p01-social-review-01/target-private`在运行前不存在，禁用wrapper，不复用共享rmeta/rlib；单父任务总锁覆盖首次编译、独立私有PG与精确清理。新test格式、固定源M0及新增PG已实际终态；严格Clippy保留已知v2失败，结果如下。固定411源和新412执行源完整SHA保持。


## 实际命令与结果

在这份固定v2与明确合成输入范围内，未复现状态UUID洗为新事件写入、整包省略后借旧ID绕过，或关系前身旧来源撤销后派发/选择失守的新漏洞。不得将以下3PG通过覆盖严格Clippy失败，或写成语义、安全完整/全部工程通过。

| 命令（格式之外cwd为独立412源） | 实际终态 |
| --- | --- |
| `cargo test --locked --test social_boundary_regression --no-run` | exit0，新空独占target完整编译63.81秒，之后才创建独立PG。 |
| `cargo test --locked --test social_boundary_regression -- --ignored --test-threads=1` | exit0，3 passed / 0 failed / 0 ignored，实际测试3.61秒；未重跑原social6或scheduling14。 |
| `cargo clippy --locked --test social_boundary_regression -- -D warnings` | **exit101**，27.00秒，固定生产`src/domain/social.rs:70`同一question_mark lint；没有压制warning、放宽阈值、改源或换用v3。library失败使newtest的完整严格lint未获通过。 |
| `rustfmt --edition 2024 --check tests/social_boundary_regression.rs` | exit0。 |
| `/workapps/project-alia/alia/.venv/bin/python scripts/check_phase0.py`（固定412源） | exit0，6 schema、53 closed objects、13 examples、68 negative cases；不代表当前在变live整体通过。 |
| 父driver与资源 | exit0仅表示harness完成及3PG通过，**不是所有检查通过**。总锁覆盖全部重操作；cleanup=true，之后按原label/ID独立inspect确认容器/volume均不存在，私有state password已移除。 |

第1组实际阳性：来自实际PG三层状态ID的回复生成/逐目标审查/提交均执行，packet8明确三条social_interpretation及限定proof_scope。4个反例各先在实际上下文中验证真实event版本候选合法，再仅替换相应引用为选中保存状态UUID；memory、belief、person、autobiography分别在作者校验拒绝，review次数0，语义表及persona前后相同。每个反例仅1条settled/2假token，不因拒绝缩任务、伪造通过或额外重试。

第2组用实际输入而非改写DB正文产生超大完整依据，实际has_more=true、两状态ID从selection/manifest完全省略，8条原始全文没有部分混入，整包≤96KiB。引用被省略长期态度ID的作者提案被拒绝，review0、无语义写入，仅1条settled/2假token。

第3组实际旧关系来源、两代revision和subject都先进入完整manifest；移除一般边并撤销旧来源后，旧预留dispatch返回LeaseLost，无dispatched_at、model_calls仍0。新context通过valid_for且has_more，旧关系及其ID从两种关系投影/manifest移除；无关appraisal/attitude及其真实来源保留。runtime_config和persona与撤销前相等，未以权限、预算或人格变更绕过屏障。

合计仅7次合成provider程序调用/14假token（第1组阳性2＋反例4，第2组反例1）；第3组只预留而未派发，合成bound不是提供方真实token准确性证明。真实云模型/收费/部署均0。新增3PG首次执行即通过；唯一新执行非零为上述固定v2原有Clippy警告，不将其计为新隐私漏洞或抹去。root通知的v3通过是另一源范围，本报告不验证也不冒用。

原始证据在 `evals/results/p01-social-review01-v1/`：report.json/输入核对/412源manifest/三份实际执行日志、格式/M0日志及原v2报告/Clippy引用，均有SHA。实际新bin SHA `2b9b14afefedc78f89e7ad0690bd1e54565ac9d2efb57c78b9c9f7a83ebc7dd5`，执行日志确认来自本任务target-private/debug/deps，运行后再次核同SHA；未使用共享旧产物。原SSE RED、后继通过、HISTORY/M3审查与EVAL-03交付保持。

## 限制与下一依赖

这三个有界工程组合不认证自然语言表述是否正确、真实他人感情、持续心理体验或关系质量。合成Supported不是客观事实证据，真实模型语义仍unverified。只检查了指定类型ID替换、实际超大包及一个完整旧关系来源链，未穷举全部并发/过期/计数/权限组合或硬崩溃。当前无社会状态模型新写字段；本测试没有将明确尚未开放入口制造成线上回归。

严格Clippy未通过的具体固定输入保持，不以可见v3或live小改修订旧SHA。本次没有需要root实施的新生产修复建议；root可在其后继命名整合源采用这三个新增用例，并另行绑定实际检查结果。M1—M5、20组成长与十四真实日观察不据此通过，正式100/60x3/20ABC与collector均未改。完成本任务后等待root限定后继，不自行切换collector或追加收费任务。
