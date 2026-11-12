# ALIA-P04-09B｜发展实际持久化

状态：**READY（同P04-09B，含owner撤回追加）**。最新owner-v1实际16项PG及15项领域测试、严格Clippy/fmt/固定源M0均exit0，源码/QA辅助摘要稳定，独立PG清理通过，0模型。GET projection与读取代码、09A源码及PascalCase序列化逐字保持v4。当前adapter/test须用文末owner追加新摘要；已接受v4/12PG原副本证据保留，不覆盖。canonical0028由root维护，29仍待后继兼容。

接口固定：propose_development(key,expected,manifest,proposer_id,&DevelopmentDraft)；transition_development(key,expected,manifest,id,expected_development_version,&DevelopmentCommand)；developments(include_history,limit)；development(id)。前两者返回DevelopmentReceipt；读返回Value，RR只读，list limit1..100。expected为全局ExpectedState，与独立target/scope before_version分开。人格初始target版本1，task_strategy仅实际同owner goal或持久run(task)，初始值null；无读取初始化。

DDL提案10个表：target当前投影、候选、实际事件证据、不可变步骤、target修订、完整objects来源FK、可信verifier注册、实际观察/执行结果、可信评审记录与其实际来源。target+scope使用UNIQUE NULLS NOT DISTINCT。只存target before/after，不复制整份私人persona。所有新表名称/具体字段已可审查；root保留canonical0028及共享入口最终裁决。

可信边界：本adapter不开放HTTP/模型命令Deserialize，也不提供注册/观察/评审认证写入。StartTrial从真实事件/实际action结果/受信任观察记录派生SourceClass。RecordValidation必须对应实际development_review_records、trial事件、registered verifier及完整source FK；不能只提交IndependentAssessment或approved bool通过。verifier registration须system/direct_observation实际登记，human须actual owner关系，measurement须实际development_verifier对象；后续真实执行器/root产出这些server记录，合成PG测试仅证明边界，不能冒充行为通过。

严格恢复：从DB候选和按version排列实际步骤逐步重构纯域状态；每步重载来源/可信评审，重新构造opaque tokens，比较保存的最小transition/validation记录/修订，不Deserialize accepted状态或模型bool。真实应用在accept/revert同一TX以当前persona仅替换一个白名单路径，写projection/head/revision/event/outbox/state/幂等回执；撤回检查实际当前accepted head/version/value。

实际propose→trial→validation→accept→revert及重启、隐私无边屏障、幂等/回滚、错误来源/独立性/过期/失败洗白、分页/只读均已在独立PG验证。0模型，未部署；0028由P05严格schema屏障阻断直至root同步清理/导出规则，不能绕过。

root已采用canonical migrations/0028_individual_development.sql，SHA256 e61da144fa25a887d5a1c686c8dccd554c44981051608e732a1d8d791fc37f12；仅开头注释区别于提案，约束与case_id等语义一致。实际固定PG只采用此canonical28，不以草案替代。首轮固定输入使用root-privacy-http-v2源清单893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72和P04-08完整不变源，隔离构建/PG最终通过，详见最终报告。

## 已实现并定向验证

真实四个Store方法均已保存，不再是只有类型：propose实际创建目标基线/候选/来源/步骤/事件/状态/回执；transition从DB严格重建后执行typed命令，accept/revert在同一TX实际写individuals.persona的单路径完整投影或独立strategy目标，并追加revision/head/version、步骤、完整来源、outbox/全局版本及幂等回执。保存的revision只有该target的before/after，没有整份persona副本。09A domain/test保持接受摘要未改。

恢复不需要增加领域Deserialize后门：adapter私有closed DTO仅解码存储command/assessment；候选重新prepare并核查原proposed事件摘要，按1..version连续步骤重放；逐步重载同owner实际来源/注册/观察/评审FK，重新经TrustedValidator构造token；对比保存的transition/validation及真实revision字段和事件摘要。当前target值/head/version也与实际revision、当前persona路径相互核查。幂等重放再次完整重建及递归隐私复查，不仅信任receipt JSON。

StartTrial按实际events及action receipt派生source class；tool.result没有实际执行receipt、executed不为true或标fixture时，不能伪装成外部报告证明已执行。reported_claim/external_document仍是陈述/资料类别，元数据不证明已阅读；本切片不把本地保存回执提升为外部送达或世界事实。actual observer/reviewer后续必须检查完整原事件与执行上下文，而不是只看SourceClass标签。

来源屏障同时沿development实际FK/dependencies和core实际FK/引用（目标原始来源/稳定兴趣、run输入/trigger/goal、action run/result/decision、关系修订前身、情绪对象/目标、认识/自传/人物等）；去掉general derivation_edges仍拒绝被撤销祖先。读RR、先完整过滤再limit+1、无来源裁剪；include_history=false列proposed/trial及仍为当前target head的accepted，true含历史终态和旧accepted。detail响应元数据加development对象，含transitions/validations；list加include_history/items/has_more，false不附完整历史。原始身份/seed/runtime不写。

最后补全的实际并发expected-state竞争与TX中途privacy fence也已通过：Store.write入口之外，在提交前后复查同一privacy generation；读和私有重放同样复查。测试真实观察到写事务PgSleep等待后才触发fence，证明被封锁的在途写入回滚，不以单纯前置布尔断言代替。

## root整合边界

- root自行登记domain/adapters的pub mod development；本任务只在固定副本登记，不改live共享入口。
- canonical28已采用且固定SHA如上。P05必须将十表加入清理、导出、恢复与重建规则；strict未知schema拒绝保持，不能绕过或生产启用旧规则。
- 接受后当前persona是带来源的投影；root的ModelContext、公开persona读取和删除重建必须将相应development_target/head来源纳入可见性及input_manifest。此adapter没有偷偷把整个identity挂到每项发展来源上以永久阻断无关内容，也没有修改共享context。缺这项接入不能宣称来源连续性全链完成。
- verifier注册/独立实际观察/可信review记录的生产写入器尚未实现，本adapter没有为验收而自注册或把模型assessment写成可信proof；测试SQL fixture明确为synthetic。root后继实际评估器需实现这些写入路径并维持真实独立性，不可用空registry或伪approved bool替代。
- 本轮是内部持久化切片，无模型/HTTP提案接受入口、trial影响认知的overlay、UI或真实20组成长结论，不宣称M3完整工程/行为通过。P03整合工程、P01行为判据与后续集中真实验收保持。

## 最终实际命令、证据与源摘要

最终父任务v4 session97962终态exit0；`evals/results/p04-development-storage-20261112-v4/report.json` SHA256 **969b29c124378a3cf133dd1d5d1c0a3ae844893b811441730824f4ac191f5c98**；source-manifest SHA256 **cc90ad49928d2b537feadf297159e9df281e2107edad36a8711d9d745efe1d60**。evidence.json绑定固定输入、原报告、最终两项测试产物副本及实际计数。

固定输入：P04-08不可变完整source-manifest `26121d1a9cddbdb8dac8908ee6c50346641f417c8ba309ed8f798d801d735c66`，逐源核对；叠加root-privacy-http-v2不变182源清单`893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72`，逐源核对；加入实际canonical28及本任务四项代码/测试（09A两项摘要不变），仅在私有副本给domain/adapters追加pub mod development。未从正在变化的live共享代码拼装，不修改原accepted快照。完整文件集合和摘要在manifest。

所有实际命令RUST_TEST_THREADS=1，自己Cargo target/p04-development-storage-v1，使用当前固定pgvector/pg17镜像的独立工程测试实例/独立数据库/凭据/清理范围；一个父任务持总锁覆盖冻结、编译、实际PG和清理，没有重复父作业。v4锁等待约0秒。生产配置、实际个体、模型和其他窗口资源未操作。

1. `cargo check --locked --test development_storage --test development_domain`：exit0，7.03秒。
2. `cargo test --locked --test development_domain`：exit0，**15 passed / 0 failed / 0 ignored / 0 filtered out**，实际测试0.02秒。
3. `cargo test --locked --test development_storage -- --ignored`：exit0，**12 passed / 0 failed / 0 ignored / 0 filtered out**，实际PG测试15.33秒。测试源码ignore是独立PG保护入口；本次显式--ignored全部实际执行，没有将未运行项目计通过。
4. `cargo clippy --locked --lib --test development_storage --test development_domain -- -D warnings`：exit0，6.93秒。
5. `rustfmt --check --edition 2024 src/domain/development.rs src/adapters/development.rs tests/development_domain.rs tests/development_storage.rs`：exit0。
6. 用现有.venv Python运行固定副本`scripts/check_phase0.py`：exit0，schemas5/closed_objects49/examples10/negative_cases56；这是固定基线M0，不替代root尚未实施的development生产schema同步或M3行为门槛。

十二组实际PG覆盖：

1. propose/trial/record/accept真实人格更新；独立表达路径后继不被撤回traits覆盖；新连接和新Store重启读取/撤回，seed/runtime/origin/digest保持；四条领域transition和三项实际revision完整。
2. proposal及accept同键exactly once；晚期state.changed outbox触发失败时persona/head/revision/event/步骤/outbox/状态/回执全表回滚；去触发后同键只提交一次，旧全局版本拒绝。
3. 两项相同expected并发proposal仅一项成功、另一项StaleState；真实观察PG写事务进入PgSleep后触发同Store共享privacy fence，在途proposal回滚，全public表指纹不变。
4. Failed/Unverified实际结果均保存；后续Passed不能洗白，接受无副作用拒绝；reject保留历史；实际UTC deadline到期expire及终态接受拒绝。
5. 未知/nil proposer、generated/背景/审查反馈/短情绪/无执行回执的tool.result拒绝；缺review事件、错reviewer/self origin、篡改outcome/source/反证搜索、伪注册actor均不能认证，指纹不变。
6. proposal晚期故障连新target/对象均回滚；同key修复后可完成；隐私代次、对象version和manifest缺漏无副作用拒绝。
7. A→B→A目标值的真实不同revision/head/version后，旧A撤回仍冲突；当前列表只保留最新accepted，历史列表保留三次accepted。
8. 103可见+3隐藏提案，去通用派生边后先过滤再limit+1/has_more；detail/list/非法limit全public表指纹不变；伪accepted状态与修改draft均在严格重建时拒绝；全局fence拒绝读取。
9. goal原始来源与run.input_manifest实际祖先撤销且去通用边，公开primary不能洗白该scope，策略proposal拒绝且无写入。
10. 实际goal/run scope与独立strategy存储，persona不变；撤回首次策略为null；只有objects中的goal标记而无实际goal行拒绝。
11. server实际significant观察登记+单个独立场景及真实反证搜索即可接受重要价值顺序变化，不机械要求三次。
12. 去通用derivation_edges后，实际原来源或review注册来源撤销阻断detail/list/新写/私有幂等重放；其他独立表达提案仍可见。

四轮原始记录全部保留：v1两项PG/15域通过，但严格Clippy因未使用CandidateRow.id退出101，之后fmt/M0未运行；已改用真实读取id重建，未压制警告。v2十项PG和六命令全通过；随后补scope祖先来源、工具回执保护，v3十一项PG和六命令全通过；最后补真实并发与TX中途privacy fence，v4十二项PG和六命令全通过。每轮独立容器清理passed、固定source稳定；v1 live在同时补场景所以owned_live_unchanged=false如实保留，其余每轮采样时为true。没有隐藏失败或收费重试。最终source_unchanged/owned_live_unchanged/cleanup.passed全部true，error=null。

| 最终源或产物 | SHA256 |
| --- | --- |
| src/adapters/development.rs | b8c4779ffad2f8de431236c43eb9be19ee0afdaad8488e7842e4b7e9680e4fda |
| tests/development_storage.rs | 33241c98c9b741736fee19a48bb7420a7549a803f5421f6e991182c20aac0b21 |
| src/domain/development.rs（09A不变） | 93b36aad9e668ef2348be790f0dfbb9b1abaa8237de08256770596500492f1fc |
| tests/development_domain.rs（09A不变） | 04980a4c4564c70c9577bdc4f1f07a9aa0eae4d83f729cb1a80d5114db08ddd9 |
| root canonical0028（只读使用） | e61da144fa25a887d5a1c686c8dccd554c44981051608e732a1d8d791fc37f12 |
| v4/bin/development_storage | a3f37b083949ede8b59d91ebc4ead5ef73098522c46917643473f83620d2cd69 |
| v4/bin/development_domain | 0da9bc79c60dc2e589edddaf7a67f9c2fa9e944b7230fa39cb9d7181433a1cb6 |

本有界09B交付已READY，0真实/fixture模型、0模型费用，不初始化生产个体/部署。下一依赖为root接公共入口/真实来源评估器、P05同步十表清理导出/投影重建，以及后续认知trial overlay与P03/P01完整工程/行为。不能据此标M3、AT-05/06/16整体通过。

## 后续0029隐私兼容边界（不改变已冻结09B/28）

root后续通知已记录：P05-07将提案provenance_removed及current-head部分唯一约束，canonical29仍归root。本09B实际已经完成并固定canonical28的12项PG/15项域及严格检查；不为尚未冻结的29修改accepted28、重复运行或把未来兼容记成已完成。已冻结v4副本及其两源摘要/原始证据和09A接受副本保持；同09B的owner追加live源码采用文末新摘要，不将其冒称为已接受旧v4。

当前28明确行为：所有普通读取/恢复/新写/私有重放通过objects.blocked与实际FK祖先拒绝旧受阻断head，返回不可用而不改成主观rejected、再次accepted或解除旧ID阻断。28尚无provenance_removed字段，因此不能声称已支持该字段或删除后新head的版本分配；同逻辑target仍有全唯一约束，被阻断旧head会拒绝新proposal，正是29要补齐的兼容点。

待root冻结29后，明确改动位置：

1. propose的逻辑target/scope查找及list/detail/head恢复加入非provenance_removed/current的条件；旧candidate/head历史继续不可用，不解除旧object阻断。只有P05完成投影重建后，允许创建新object head。
2. 新head基线版本取同individual/target/scope全部旧head/修订历史最大版本+1，不能复用初始1或仅查未移除head；首次普通初始化仍为1。需与root确定该无revision新基线的可信记录绑定，再调整当前verify_head中无revision仅允许version1的28规则，不能简单删除校验。
3. 保留candidate的真实before snapshot、target_id及修订前身，旧accepted不能通过相同target/value或新的head对象执行撤回，继续按head对象/version/revision拒绝ABA。隐私移除不是主观拒绝/接受。
4. P05先从seed+存活未阻断target revisions重建当前persona和strategy，并在ready/202前完成；P04不从旧私有head恢复内容，不修改P05源或canonical28/29。root稳定reference_graph_sql(tx)可作为后续统一来源接口，28固定验证不混入当前social/shared变化。

后继实际验证须独立检查：移除→新合法proposal得到新object与递增基线；旧ID永久blocked；当前读不露移除状态；旧accepted撤回不能覆盖新head；重启及幂等不复活旧私有内容。此处仅登记依赖与准确差异，尚未实施29或运行这些用例。

## 同09B追加：owner撤回入口（READY）

root追加的`Store::owner_revert_development(key:&str,id:Uuid,expected_development_version:i64,reason:&str)->Result<DevelopmentReceipt,StoreError>`已保存。旧v4原始证据保持，当前live adapter/test将有新摘要，须以本追加最终报告为准；不是0029兼容实现。

采用私有共享transition核心，不另建第二个发展写入器：原public transition仍以individual/internal命名空间、原expected/manifest/body摘要调用同一核心；owner以owner、固定`/owner/developments/revert`命名空间调用，幂等摘要**仅**绑定id/expected_development_version/reason，不绑定读取时可能变化的globalExpected或manifest。owner receipt与原子状态/投影/事件/回执在同一write TX提交；没有“先低层提交，再另存owner映射”的响应丢失窗口。两个不同id复用同key也冲突，内部命名空间同key不互相冒充。

owner先开RR/READ ONLY并优先replay稳定意图的真实receipt；校验实际reverted步骤、完整重建和当前来源隐私，允许dev已reverted/version已增加或其他全局变化时重放相同结果。新写才在同RR读取同owner真实候选/目标、devversion、当前ExpectedState/epoch/完整persona及FK/dependency完整祖先，最多4096个来源object，超额明确拒绝，不能截断。对真实domain/head做revert预检，检查privacy generation，关闭RR后再调用共享write核心CAS；源或全局版本变动不自动重试，旧head/ABA/已非可撤回状态映射StaleState。UI没有can_revert输入或可信manifest构造权。

GET envelope/item字段及09A PascalCase记录枚举保持；root仍独占HTTP route、ETag/schema和29flags。丢失响应重放、同key参数冲突、隐私撤销拒绝、晚期回滚、限额、并发及CAS竞争已通过下面的实际PG；本追加READY。

### owner追加最终证据（接收新源码时以此为准）

首轮owner-v1父任务session33584终态exit0，无失败或重试。报告`evals/results/p04-development-owner-20261112-v1/report.json` SHA256 **9354d819e40a9204366ba7de0328de7037217cd32d7b4de3d82799ffc8a18841**；source-manifest SHA256 **77084c7d15e47c617f3aba82ba2b513adf5843b276c5e94d6c072f21b69017ee**。固定输入为已通过v4完整源清单cc90ad49928d2b537feadf297159e9df281e2107edad36a8711d9d745efe1d60，逐文件核对后只叠加本次owned代码/测试，仍固定canonical28，不混入29或当前root社会认知/路由变化。09A两项摘要不变；额外逐字核对v4的projection、GET读实现及verify_head片段完全一致，证据见evidence.json。

实际六命令与上述v4一致，全部exit0：定向cargo check（7.38秒）；15项普通development_domain（15 passed/0 failed/0 ignored/0 filtered）；实际development_storage --ignored（**16 passed/0 failed/0 ignored/0 filtered**，实际执行15.32秒）；--lib/两项test严格Clippy（7.38秒）；owned四源rustfmt --check；固定基线M0（schemas5/closed49/examples10/negative56）。原12项真实PG全部保留通过，新增四组实际owner PG：

1. 与internal命名空间同key共存；owner撤回后dev已reverted/version增加，再有真实消息导致global state变化，原id/devversion/reason仍重放**同一真实receipt**且全表指纹不变；改id、目标devversion或reason的同key均IdempotencyConflict；去通用边后原来源撤销，缓存重放仍NotFound且无写。
2. 晚期state.changed outbox失败，owner mutation与owner receipt同时回滚；随后持真实advisory写门，观测两个owner请求都完成RR规划进入写门等待，释放后即使第一个推进全局版本，第二个仍精确重放同一receipt，只有一次revert revision和一次owner回执。
3. 实际观察owner结束RR并等待写门后，另一个事务推进全局版本；owner仅返回StaleState、无回执或投影副作用，不自动重规划/重试；真实后继A→B→A后旧owner撤回也StaleState且不写。
4. nil/未知ID、非法version、空/NUL/2001字符reason拒绝；为实际候选追加4097个真实owned事件的FK依赖，完整来源超4096时明确拒绝，全表不变，不截断或信UI can_revert。

本追加按P03已释放协议持资源锁：独占输出workspace、固定target、自己实际DB容器；provision与cleanup另持database-admin:shared，cleanup同时持本DB锁。显式向子进程传resource lock map及FD。全部锁等待合计不足1毫秒，没有占共享model预算锁或完整candidate锁；独立PG清理passed、source_unchanged/owned_live_unchanged/qa_helpers_unchanged全部true。QA helper摘要也绑定报告，未重新运行P03整合矩阵。0真实/fixture模型、0模型费用，未部署。

| 最新接收源/产物 | SHA256 |
| --- | --- |
| src/adapters/development.rs（含owner helper） | 6c20b737894bc820e792e8204b8087409b1939d9168899f7da4776232dacd2f8 |
| tests/development_storage.rs（16实际PG） | 19a5dd55ba9dfac04012d9d8b5804fb719ad0bcc27899ad0ed304309557b869f |
| owner-v1/bin/development_storage | 51b81e1a8de04adf35c7bbd3d4b91d56246958f5c065526da6aced5c96e4c2a1 |
| owner-v1/bin/development_domain | 0ce341070eb12d219fdbbd09a7e9cedc58fb860b75c35a884f9e32f3196491bd |

root route仅需传真实已鉴权owner请求的key/id/强ETag解析出的expected_development_version/reason，不预先把globalExpected或浏览器manifest作为意图散列，不在此helper之外再异步存另一个非原子回执。现有GET envelope/item和PascalCase记录不变。尚未运行HTTP或浏览器；其路由/schema/测试仍root/P02/P03负责。29的provenance_removed/新head基线仍未加入本追加，原兼容依赖保持。
