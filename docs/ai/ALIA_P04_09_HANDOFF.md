# ALIA-P04-09A｜发展领域机制与候选应用器

状态：**READY（P04-09A纯领域限定切片）**。15项普通领域测试全部通过，定向编译、严格Clippy、局部格式及固定源M0均exit0。仅新src/domain/development.rs、tests/development_domain.rs及本handoff；未写空adapter、未改共享schema/模块/迁移/运行时。08 READY三源保持不变。

## public接口已保存，供root冻结0028

DevelopmentDraft八字段严格遵循派发；DevelopmentTarget白名单、TargetSnapshot、DevelopmentCandidate::prepare、Development::{propose,start_trial,record_validation,accept,reject,expire,revert}、ApplicationIntent及完整before/after/验证/转换历史已保存。root可在domain.rs登记pub mod development，后续0028需要持久化target head/version、candidate与不可变validation/transition记录及实际来源FK。

人格路径按现有真实schema映射：traits→/personality/traits，values→/personality/values，expression→/expression，cognitive_preferences→/personality/cognitive_preferences；没有/persona包装层。每次只替换该路径且完整文档经PersonaSeed::validate，不放宽schema，不覆盖identity/profile/relationship/runtime/seed或initial_interests。

task_strategy独立类型建议精确为{scope:{kind:goal|task,id:UUID},description:非空≤2000}，无全局scope，不塞入persona schema。TargetSnapshot.scope显式绑定实际goal/task；根持久化需按individual+target+scope维护current head/version，before_version是该目标版本，全局expected state/epoch仍由事务另查。scope实际DB归属由root可信adapter验证，纯域不伪称已查库。

TrialAuthorization与VerifiedReview不实现Deserialize、字段私有，只由TrustedValidator结构化校验可信server-loaded来源/独立评审后构造，不接收模型bool授予验证。server输入也不Deserialize，实际DB来源/独立review者身份与执行回执由root可信加载器证明；纯域仅校验这些记录的绑定、owner标签、完整性和时间，不能替代真实来源验证。

跨场景习惯需至少两个不同context及唯一实际结果事件；重大变化允许typed significant_event+独立实际试验/反证审查，不机械要求三次。仅短情绪/模型自述/初始设定不成为人格漂移充分依据。失败/未验证记录留存且不接受；撤回只在accepted head/version/value仍是当前目标时产生新意图，不覆盖后继修订。

实际测试与证据见下文。下一阶段同09持久化接口由root冻结；本阶段不等数据库迁移或付费验收，不宣称M3实际应用/成长已完成。

## 09A 已验证接口冻结细节

候选构造为 `DevelopmentCandidate::prepare(id, individual_id, proposer_id, draft, &TargetSnapshot, &current_persona, now)`，只返回不可变候选，保留完整八字段draft。`propose(candidate)`建proposed状态；`start_trial(&TrialAuthorization,now)`、`record_validation(&VerifiedReview,now)`、`reject(reason,now)`、`expire(now)`返回新状态；`accept(&snapshot,&persona,new_revision,now)`与`revert(&snapshot,&persona,new_revision,reason,now)`返回`(Development,ApplicationIntent)`。原对象不变，不隐藏数据库副作用。

`ApplicationIntent`带target/scope、expected_version/expected_head_revision_id、新revision_id/next_version、before_value/after_value及完整受校验的ApplicationValue。人格应用在**事务当前完整persona**上仅替换白名单路径；因此不能将prepare时旧全文存储为接受结果覆盖其他目标的合法后续变化。task_strategy使用独立的`ApplicationValue::TaskStrategy{scope,strategy:Option<TaskStrategy>}`，撤回首次策略允许None，不往persona塞字段。

候选摘要绑定id/individual/proposer/draft/原目标值和head/scope。trial摘要再绑定实际授权route、完整来源元数据、授权时间和开始时间；评审token绑定二者，防止同一候选在同一时刻切换试用路线/来源后移植凭据。评审来源原始记录须与trial主证据/反证记录完全一致；新结果和反证搜索必须引用本次trial开始后实际ObservedOutcome，所有case/result去重，不能复用旧来源当新执行结果。counter_review Failed/Unverified可以如实记入验证历史，但不能接受；后续Passed也不掩盖已保存失败。跨场景独立性和事件分类真实性仍须实际执行器/可信加载器提供，字符串不同本身不证明独立场景。

`Development`及opaque token不支持Deserialize，不提供状态setter或`verified:true`恢复后门。根持久化下一阶段应增加**显式受信任重放/加载入口**，由真实记录重构候选、授权和评审，逐步重放状态并对比保存的历史/摘要；不能直接从任意JSON反序列化accepted状态。现阶段没有伪造存储adapter或“查库成功”声明。

## 给root的0028与公共契约精确建议（尚未实施）

1. 当前目标表按`individual_id + target + scope_kind/scope_id`唯一；人格scope为空时用partial unique index或NULLS NOT DISTINCT，避免SQL NULL允许重复head。存正version、current_value、current_revision_id；全局state_version/privacy_epoch由现有事务独立校验。
2. 候选存proposer、prepared_at、八字段draft、完整before snapshot与不可变digest。主来源/反证关联真实同owner事件；task/goal scope亦需实际同owner外键或事务检查。significant_event不得由模型枚举自行认证。
3. trial保存route、授权事件/来源快照与trial started time/digest；validation保存独立review者身份、origin、实际反证搜索事件和逐case执行结果、原/反证审查全集、recorded_at；transition按顺序追加before/after/reason/review_ids/revision_id。status增加expired，候选超时不再接受，已accepted允许在deadline后撤回。
4. 写事务先重查当前individual/state/privacy/source闭包/目标head；再以当前persona调用纯域；intent与候选状态、revision、来源边、事件、outbox及state_version原子提交。撤回必须新revision并检查accepted head/version/value，后继冲突拒绝，不能覆盖或删除旧accepted历史。
5. 当前`schemas/decision.schema.json`的`development`实际为七字段，before_version最低0，枚举含persona.interests，缺proposed_value。建议root按本次冻结要求增必填proposed_value成为八字段，将before_version最低改1，暂移除persona.interests；按target校验traits/values/expression/cognitive_preferences真实结构，task_strategy使用上面独立闭合scope/description结构。同步Rust DecisionProposal类型、示例、负例、规范和非空development运行接入，不能只改schema宣称可用。保持完整persona schema，不开放identity/运行权限。root维护共享规范/迁移/API统一，不由P04并行编辑。

下一项依赖仅为root冻结同09后续可信存储接口与迁移，并释放adapter/DB测试文件所有权。本阶段无DB、HTTP、实际模型、20组成长或M3全链通过声明；完整工程回归仍P03、真实行为门槛仍P01。

## 实际验证与固定输入

最终报告：`evals/results/p04-development-domain-20261112-v2/report.json`，SHA256 `065247cc0b3397341d2da5385e852786d7d49fe3afb0f2dec9bec27cc5de2dc8`。最终source-manifest SHA256 `c7ed4f69f26fd11e66812867c943699e427ea0acff149e55a41f237167b27c26`。固定基线为N5R3 manifest `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974`，verify全源集合与摘要通过；叠加仅本任务两源，并在**私有副本**src/domain.rs追加pub mod development。不复制正在变化的live隐私/运行时或未冻结后续迁移，没有live共享注册修改。

1. `cargo check --locked --test development_domain`：exit0。
2. `cargo test --locked --test development_domain`：exit0，**15 passed / 0 failed / 0 ignored / 0 filtered out**。
3. `cargo clippy --locked --lib --test development_domain -- -D warnings`：exit0。
4. `rustfmt --check --edition 2024 src/domain/development.rs tests/development_domain.rs`：exit0。
5. 现有.venv Python执行固定源`scripts/check_phase0.py`：exit0，schemas5/closed_objects49/examples10/negative_cases56。此M0针对固定基线及纯域切片，不冒称覆盖live未冻结隐私schema或新生产development schema。

15组覆盖重大价值顺序/优先级变化的真实结构应用；所有四个人格路径；跨场景习惯及重复context拒绝；短情绪/自述/初始配置不足；unknown/permissions/noop/deadline/来源计数/版本/完整persona异常；无独立审查、反证缺漏及结果伪装；失败和Unverified保留/后续Passed不掩盖；重复/过期/逆序转换；候选payload、同时间不同route/来源及trial时间的token隔离；owner标签、来源时间/类别不可变；跨review重复case/result；accepted应用和完整历史；撤回B→A新意图、原历史与其他人格路径保留、后继/ABA冲突；独立goal/task策略与撤回为无策略。

v1原始报告/日志保留，11项和全部五命令均exit0。v1验收期间live新增了trial摘要隔离及四组测试，因此v1.owned_live_unchanged=false如实保留；其固定副本source_unchanged=true，不作为最终live摘要声明。v2因这些明确改动重新固定验证，source_unchanged与owned_live_unchanged均true。没有失败被覆盖，也没有重复模型试跑。

两个父任务先后终态exit0，无并行重型作业；全命令阶段持有项目总锁，v1等待22.69秒、v2等待52.05秒。Cargo target为本任务独占target/p04-development-v1（第二轮复用本任务编译缓存）；最终测试二进制SHA256 `2030565815d26e581aebcbaf02daf97b1c851ee189485464ca8224bebd774087`，实际执行路径见v2/02.log，另存最终bin并绑定evidence.json。0 DB调用、0真实/fixture模型调用、0费用，不部署或创建实际个体。

| 最终所有权文件 | SHA256 |
| --- | --- |
| src/domain/development.rs | 93b36aad9e668ef2348be790f0dfbb9b1abaa8237de08256770596500492f1fc |
| tests/development_domain.rs | 04980a4c4564c70c9577bdc4f1f07a9aa0eae4d83f729cb1a80d5114db08ddd9 |

## 限制与下一项依赖

主线可接收上述两源并登记pub mod development。本轮已经实现可执行纯领域状态机和候选应用器，**尚无**数据库存储、可信事件/评审加载、启动恢复、HTTP/前端、实际trial overlay和原子提交。ServerSource/IndependentAssessment在普通测试中均是显式合成server事实，不能证明真实场景语义、实际来源归属或评审独立性；这些必须由后续真实adapter/执行器与P01判据验证。当前验收不是M3/AT-05/06/16整体完成。

下一项由root冻结0028和显式可信重建/存储接口，释放P04后续adapter与DB测试文件；后续保持同09任务连续推进，不修改本阶段原始证据。P03负责整合工程回归，P01负责后续集中真实成长/行为判据。权威IMPLEMENTATION及里程碑仍仅root更新。
