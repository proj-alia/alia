# ALIA-P04-07｜实际关系修订持久化

状态：**READY**。首轮独立固定输入5项领域测试、6组实际PostgreSQL测试、定向编译/严格Clippy/局部fmt/快照M0全部exit0。source_stable、旧P04-04接受八源稳定及独立资源cleanup均通过，0真实/fixture模型调用，未部署或初始化实际个体。内部存储不是AT-08或完整M2—M5行为完成证明。

## 保存信号与接口

已读取AGENTS与三份权威文档、P04-07冻结契约、0026真实迁移。仅新增domain/relationships.rs、adapters/relationship_revisions.rs、tests/relationship_revisions.rs及本handoff；不改root注册/迁移/Cargo/schema/storeerror/HTTP/context/UI、旧affect/autobiography或其他任务源。

**root已实际登记，本窗口只读核对通过** domain.rs的 `pub mod relationships;` 及adapters.rs的 `pub mod relationship_revisions;`。typed接口已保存：RelationshipState/RelationshipDelta/RelationshipRevisionDraft/CurrentRelationship/PreviousRelationshipRevision/ValidatedRelationshipRevision/RelationshipRevisionError；Store::revise_relationship/relationship_history与精确六字段RelationshipRevisionReceipt。私有固定副本中最小注册已实际执行，未把它冒称当前服务接入或部署。

Draft严格为subject_id/expected_relationship_version/changes/event_ids/brief_basis。Delta只允许stage/expectations/boundaries/commitments/unresolved：stage非空≤80字符，四列表0..16项、每项非空≤2000字符且去重。省略/null不改变该字段，明确[]清空；全null/空delta/实际无变化拒绝。事件1..32去重非nil实际owned来源，basis非空≤1000，未知字段拒绝；人格/权限/情绪/身份/对方感情不在delta中。

Receipt为individual_id/subject_id/revision_id/event_id/version/relationship_version；version全局，relationship_version是本次提交后的对象版本。history返回冻结六字段metadata及十字段item，不以修订次数代替对象版本、历史数量或亲密程度。

## 实际存储规则

全局版本/privacy与真实关系对象version同时比较，关系完整五字段由程序读取/验证；受信任管理路径仅active/paused允许，archived/terminated拒绝，模型active限制仍归root认知提交路径，不开放HTTP或扩大模型权限。

程序按subject实际最高after_version加载最新修订，显式要求该实际前身ID在manifest，不能由模型选任意前身。消息也推进relationship.version，所以允许latest.after_version<当前version，不错误要求连续；每次新修订的after_version仍严格before_version+1。前身须同subject/同individual且不来自未来版本。

在同一写锁事务中比较/更新当前关系、新建修订对象、保存只含五字段的before_state/after_state及实际事件引用、记录inference/individual的relationship.revised、建立完整manifest来源与修订→当前关系的来源边、relationship.changed/state.changed、全局版本与幂等回执。历史正文与原事件不改，实际晚期失败全回滚；同key同意图返回同一历史回执，不再增版本，不同意图冲突。

所有subject/实际事件/真实前身必须在manifest、同个体且递归可用；fictional_background与decision.reviewed不作来源。隐私同时沿derivation_edges、relationship_events、修订事件关联/previous、当前关系到最新修订的实际关联及既有状态源遍历；UNION终止循环。缺本资源来源边时仍阻断隐藏消息、修订来源或前身，不裁掉争议来源后展示剩余当前状态。读取与私有回执重放也核验，独立关系保留可用。

history单RR/READ ONLY，limit1..100，subject范围和完整来源过滤在limit+1前，按created_at/id倒序、真实has_more；读取不写事件、费用、关系版本或回执，不扣减缺席关系，也不提供永久跨请求快照或完整导出。

## 固定输入与真实命令

唯一父任务session10978已终态exit0，报告`evals/results/p04-relationships-targeted-20261112-v1/report.json`。固定N5R3基线manifest `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974` + 不可变root-history-01-domain/p04-05/p04-06 + root精确Cargo/lock/0024-26/diag26 + 自有三源/私有副本最小注册。未采用并行变化的隐私runtime或最新模型/context集成源码。

接受输入manifest：root-history-domain `0216662e77b8b9968d5e37e19418113e253d3e32d669df4b3af91266c7b31490`；p04-05 `fdf780dbb722a8f9457a6babadd3216679f476232d9623c573c27533b96704fc`；p04-06 `4403f1d35bc529859e769e38759559253d166faa19ea124b21588f59dbcd3fba`。全部接受文件逐项SHA核对，Cargo/lock按已冻结完整SHA强制验证；全输入与最小注册旧新SHA记录在source-manifest。独占target/p04-relationships-v1、自己的工程PG/实际固定镜像、DB串行、全局父锁从固定输入到清理持续持有，无重复作业。

1. `cargo check --locked --test relationship_revisions`：exit0。
2. `cargo test --locked --lib domain::relationships::tests`：exit0，**5 passed/0 failed/0 ignored**；65项filtered out未运行，未计通过。
3. `cargo test --locked --test relationship_revisions -- --ignored`：exit0，**6 passed/0 failed/0 ignored**，6.38秒；显式执行独立PG用例，默认跳过不计通过。
4. `cargo clippy --locked --lib --test relationship_revisions -- -D warnings`：exit0。
5. `rustfmt --check --edition 2024 src/domain/relationships.rs src/adapters/relationship_revisions.rs tests/relationship_revisions.rs`：exit0。
6. 用既有.venv Python运行固定源`scripts/check_phase0.py`：exit0，M0 PASS（schemas5/closed_objects49/examples10/negative_cases56）；不代替产品行为或14天观察。

领域实际测试覆盖省略/明确清空、unknown/allnull/noop、列表/Unicode边界/去重、真实当前版本与owner、旧修订允许消息间隔及错误subject/未来或非正前身版本/无来源拒绝。

六组PG实际覆盖：

1. 实际用户陈述→分歧/边界→按后续明确范围调整expectations/commitments/unresolved，保留旧before/after与原始事件，重启可读，关系事件source_kind=inference；这里只证明记录持久化，未语义认证共同修复成功。
2. 多条真实消息独立递增owner关系版本后，正确前身指向旧修订而非计数推断；stale global/target、noop、missing actual predecessor/source、unknown owner与真实FK、fiction/feedback均拒绝且无副作用。
3. 独立subject不被改写；短情绪/长期attitude原值保持，未来时间只令短状态到期，不降低关系或长期倾向；persona不变，不把三层状态合并。
4. 删除subject/两次revision/发布事件的来源边后，两代实际事件、subject和旧revision撤销仍阻断历史/新写/私有回执；缺边的relationship_events本身也构成屏障，独立subject仍可读。
5. 最晚state.changed outbox注入失败，当前五字段/关系版本、新历史/事件/引用/来源/回执/state全部回滚；移除注入后同key一次成功，后续重放不重复发布，变更意图冲突。
6. 103条选中subject历史与3条其他较新私有subject历史，先范围/可见性过滤再limit、同刻UUID稳定排序/真实has_more、实际只读连接/全public表内容指纹不变、零usage、空结果/非法limit、paused受信任写允许而archived/terminated拒绝。

## 证据、最终SHA与失败限制

本任务首轮即通过，无失败运行或隐藏重试。最终report SHA `a99405aee9b000f5d577691d1df83e72371c37fd402a3abb934d2c94629090f2`；完整source/overlay/注册manifest SHA `2f0486698ce560a4acd533b40e90a355a3623676495ac86f8261c4a341aa8f0a`。实际测试二进制 `target/p04-relationships-v1/debug/deps/relationship_revisions-1d5b1c99eacf6eb2` SHA `9aca19fc60b50d179163eaece0415fa51c76b8c35efe97b8bd7246bc4555e750`。report.passed/sources_stable/previous_accepted_sources_stable/cleanup.passed全部true，error=null，原日志含精确命令退出。未运行P03完整矩阵或任何模型。

自有三源已与实际通过输入再次匹配：

| 文件 | SHA256 |
| --- | --- |
| `src/domain/relationships.rs` | `34665a2cd48a601e4effa4c15fa1ae7d3d0b0a46cf2e8ca03180ea69a24a4144` |
| `src/adapters/relationship_revisions.rs` | `42992fcb110571632250e763a56f62c074773fa209b0810c12ce42da7254f4f4` |
| `tests/relationship_revisions.rs` | `95b349d54a169aa80f916464ccaf9291b9c638f97236a6bf84c10df91b76119b` |

root只读覆盖输入，未修改：

| 文件 | SHA256 |
| --- | --- |
| `Cargo.toml` | `82c84ee0d075498e12cd77063e45579c089c13b6363023827fdd6da179f1866a` |
| `Cargo.lock` | `8e756235d926f7043d6f8801ebf49b170e9c2d0e5ef7891a785112ad43dd595a` |
| `migrations/0024_privacy_operations.sql` | `1debd5d0f8d508ffbec4a9e18b3bad133fd6ff8e6df8faa17325f3585c4dccfe` |
| `migrations/0025_affective_layers.sql` | `a7fc966a842dc80fc2d058e4ac69cb9d7e2cbc9f878ff6fbcf575f038bed57a7` |
| `migrations/0026_relationship_revisions.sql` | `aa5f8196891a899608ff4a39dac067f7a421734d4216e207dd7f2bef95e4e279` |
| `tests/api.rs` | `8d9903eba404bc20174a0fb33272138181022cbd69a2bfeed654466e000127a4` |

## 下一依赖

root核对并接受三源后（双module注册已实际保存），可继续context/model/同decision原子提交整合；P03将稳定接受版本纳入后继固定工程候选、诊断26和全跨模块来源/隐私回归。当前0024/0025随正确迁移顺序建表及接受affect测试代码可用，不表示本轮验证P05 journal/purge或情绪实际认知影响。

这是可信内部管理存储；没有新HTTP/模型授权、实际共同经历/对方感情保证或AT-08通过结论。继续按最新路线先做完整M5工程再集中真实模型验收，不添加付费前置、不降低预算与原阈值。root冻结后续M3发展/人格修订存储任务前，本窗口不扩大所有权或改已接受affect/autobiography/UI。
