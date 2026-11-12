# ALIA-P04-06｜短期评价与长期情感分层

状态：**READY**。最终固定版本5项领域测试、6组实际PostgreSQL测试、定向编译/严格Clippy/局部fmt/快照M0全部exit0；源码稳定、独立PG资源清理通过、P04-04旧八源不变。0真实/fixture模型调用，未部署或初始化实际个体。存储与有界权重证明不等于实际模型情绪、主观体验、成长或完整M2—M5验收。

## 保存信号、所有权与字段

已读取AGENTS、三份权威文档、P04-06冻结契约和0024/0025真实迁移。仅新增domain/affect.rs、adapters/affect.rs、tests/affect.rs及本handoff；不改root共享注册/迁移/Cargo/schema/HTTP/context/UI、已接受自传或其他任务源。root可立即在domain.rs和adapters.rs分别登记 `pub mod affect;`；本轮副本中最小双注册已实际测试，不代表已部署当前服务。

**typed接口已保存**：`AppraisalDraft`、`AttitudeDraft`、`PreviousAttitude`、`ValidatedAppraisal`、`ValidatedAttitude`、`AffectError`、`attention_weight(...) -> Result<f64,AffectError>`及`APPRAISAL_LIFETIME_SECONDS=3600`。`Store::record_affect(key,expected,input_manifest,appraisal,attitude)`、`Store::affect_snapshot(at,limit)`和精确六字段`AffectReceipt`全部可用；没有HTTP/model写入口。

字段计数澄清：派发文字写“AppraisalDraft11fields”，而CONTRACTS与0025明确列举的实际是10字段：event_ids/goal_id/subject_id/valence/arousal/goal_relevance/expectation_gap/controllability/uncertainty/brief_basis。本实现遵循明确字段名，没有虚构第11项，未知字段拒绝。AttitudeDraft六字段严格按topic/subject_id/stance/event_ids/supersedes/brief_basis。root后续共享规范计数可按实际接口更正，未擅改其文档。

## 实际行为与边界

- 数值全部有限，valence/expectation_gap[-1,1]、另外四数[0,1]；basis非空且≤1000 Unicode字符。event_ids1..32去重/非nil/真实owned事件。goal/subject除null外必须是manifest中的实际同个体goals/relationships行，不能仅有对象UUID。fictional_background与decision.reviewed不成为实际评价依据，额外混入manifest也拒绝；生成/陈述来源并不自动证明世界事实。
- 短状态每个体一个current，新评价只把旧status变superseded，保留原数字、依据、时间和来源。新created_at/expires_at同一DB时钟取值，差精确3600秒。快照只取current且created_at<=at<expires_at的完整可见状态；过期或隐藏为null，不复活旧状态。
- attention_weight严格 `clamp(goal_relevance*(0.5+0.5*abs(expectation_gap))*(0.5+0.5*uncertainty),0,1)`；有限有界信号不扩大权限、不覆盖明确指令，尚未接入实际注意行为。
- 长期态度每topic一个current，显式同topic/同subject/同个体/current正版本前身才可修订；新ID、旧status/version/superseded_at更新，旧正文及来源保留。独立对象使用独立topic，不根据时间、缺席或调用量自动降低情感；不改人格、关系stage/边界/承诺。
- 至少一个draft，manifest实际owned/非空/去重/非nil，直接事件/目标/对象/前身在manifest；写锁、expected state/privacy、active、幂等/来源与current前身再次比较。一个inference/individual的affect.updated、状态/态度/来源边/affect.changed/state.changed/版本/回执同事务，晚期失败整体回滚。回执重放保留实际历史结果并重新检查隐私，不伪造用户消息或工具成功。
- 隐私闭包同时遍历derivation_edges、affective_sources、实际goal/subject外键、attitude前身、relationship_events、goal.origin_ids中的实际UUID，以及现有记忆/认识/自传/运行/动作真实关联。缺本资源派生边时仍检查其关联事件、目标、对象及所有祖先。循环UNION终止，保留真正独立可用对象。
- snapshot单RR/READ ONLY，limit1..100；current态度按(created_at,id)倒序，先完整可见性过滤再limit+1/has_more。appraisal为实际DB字段去individual_id加event_ids/attention_weight，attitude为冻结11字段，响应精确七字段。只读不写事件、费用、回执或关系状态，不声明完整/永久跨请求历史。

## 固定源与实际命令

唯一重跑父任务session49545已终态exit0，报告`evals/results/p04-affect-targeted-20261112-v2/report.json`。固定基线N5R3 manifest `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974`，加不可变root-history-01-domain/p04-05接受包、root精确Cargo/lock/0024/0025/diag25、自有三源与私有副本最小domain/adapters注册。未复制变化的整仓源码；不包含root最新自传context/model集成或其他未READY代码。

接受输入manifest：root-history-01-domain `0216662e77b8b9968d5e37e19418113e253d3e32d669df4b3af91266c7b31490`，p04-05 `fdf780dbb722a8f9457a6babadd3216679f476232d9623c573c27533b96704fc`。每个接受文件核对SHA，root五overlay前后与首轮一致，Cargo/lock按协调文件完整SHA强制核验。独占target/p04-affect-v2、自己的实际镜像工程PG，DB串行，总锁持续覆盖构建/执行/清理，无并发或重复父任务。

1. `cargo check --locked --test affect`：exit0。
2. `cargo test --locked --lib domain::affect::tests`：exit0，**5 passed/0 failed/0 ignored**，其他60项filtered out未运行且未计通过。
3. `cargo test --locked --test affect -- --ignored`：exit0，**6 passed/0 failed/0 ignored**，4.60秒；显式执行需要PG的ignored测试，未以默认跳过冒充通过。
4. `cargo clippy --locked --lib --test affect -- -D warnings`：exit0。
5. `rustfmt --check --edition 2024 src/domain/affect.rs src/adapters/affect.rs tests/affect.rs`：exit0。
6. 用现有.venv Python执行固定源`scripts/check_phase0.py`：exit0，M0 PASS（schemas5/closed_objects49/examples10/negative_cases56）；不代替产品行为或14天观察。

实际领域测试覆盖同事件不同真实goal相关性权重、有限边界/NaN/Infinity、事件与真实目标/未知字段、长期前身owner/topic/subject/正版本、无absence/decay/personality/expiry写字段。

六个PG组分别验证：

1. 真实goal/subject评价与重启、同事件不同goal_relevance的0.15/0.6权重、DB精确60分钟、created/expiry边界和旧状态不复活。时间推进明确是模拟，未等待真实60分钟。
2. 独立topic/subject长期态度、短状态更新不改长状态、未来365天快照不改关系或扣减态度、同对象显式修订/保留旧文本版本/错误前身拒绝。
3. NaN/Infinity/越界、空/重复/nil源、实际目标与仅对象UUID区分、fiction/feedback、manifest/foreign和真实FK/DB NaN CHECK、unknown字段；拒绝均无行级副作用。
4. 删除affective对象及发布事件的来源边后，实际两代event、goal JSON origin和relationship_events祖先、旧attitude及identity撤销仍阻断对应读/新写/私有重放；独立topic保留。goal只绑定短状态，不虚构长期态度的goal字段。
5. appraisal与attitude联合更新的最晚state.changed outbox异常，原短状态与旧态度/status/version、全部新行/事件/来源/回执/state全回滚；同key一次成功/随后重放不重复发布；stale/privacy/paused/不同意图拒绝。
6. 103可见+3较新隐藏态度，先过滤再limit/真实has_more/同刻UUID排序/精确字段；实际default_transaction_read_only连接、全public表内容指纹不变/零usage、空结果与非法limit。

## 失败保留与证据

首轮session91892、`p04-affect-targeted-20261112-v1/report.json`原样保留：编译、5领域和6PG都过；Clippy exit101仅collapsible_if，后续fmt/M0未到达。只将该条件改let-chain，不忽略lint或削弱断言；最终v2完整同范围通过。两轮都清理自己的PG且源稳定，不累计重复用例数。

最终report SHA `342b904cf2e6394f81e8ff6a8ec189974811339ba6d8bde80665d8a1c91cfe0c`；source-manifest SHA `0117d5c0d065cbf406dfbd06c4d3a5d248f1bd30d339b2d85cd2e2b057a8689e`。report.passed/sources_stable/previous_accepted_sources_stable/cleanup.passed全true，error=null。实际测试二进制 `target/p04-affect-v2/debug/deps/affect-eff8693012cb5b03` SHA `7d3869b399bd56176f55f271dfe34ae8e8355da6002d588207a943720091dc70`。每项命令退出及原日志均在目录中；本轮不跑P03完整矩阵、不调用模型。

自有三源交付时与最终固定输入SHA再次匹配：

| 文件 | SHA256 |
| --- | --- |
| `src/domain/affect.rs` | `fc8464d5c0c55706e0ec4842f89bf22bd4b598cdba9145ca98021d66e5bbbd4e` |
| `src/adapters/affect.rs` | `14c11663475a70ddb36689df3dfcf0b1cf35e756b7f2de5cf21dd58b9bd8c4a8` |
| `tests/affect.rs` | `039ffda96e06d60bf16c2c40cbf2d675deb92608e1ad280d1e557c06525032a7` |

root只读overlay（未修改）：

| 文件 | SHA256 |
| --- | --- |
| `Cargo.toml` | `82c84ee0d075498e12cd77063e45579c089c13b6363023827fdd6da179f1866a` |
| `Cargo.lock` | `8e756235d926f7043d6f8801ebf49b170e9c2d0e5ef7891a785112ad43dd595a` |
| `migrations/0024_privacy_operations.sql` | `1debd5d0f8d508ffbec4a9e18b3bad133fd6ff8e6df8faa17325f3585c4dccfe` |
| `migrations/0025_affective_layers.sql` | `a7fc966a842dc80fc2d058e4ac69cb9d7e2cbc9f878ff6fbcf575f038bed57a7` |
| `tests/api.rs` | `59464d82f83fe59efd4070408ef3452547362d80d030d6dff3f74070e38718a0` |

## 下一依赖与限制

root核对并保存三源、双module注册后可继续context/model/原子提交接入，P03纳入下一固定工程候选及诊断25检查。当前0024表仅随root正确迁移顺序实际建表，不表示本轮验证隐私journal/物理purge业务；那由P05/P03负责。root后续关系分层/M3契约释放前，本窗口不自行扩大所有权或改已接受自传/UI。

按最新用户路线先推进完整M5工程，再集中真实模型验收，不以单独付费v5作为开发前置。保持全部预算、来源、质量和原验收阈值；本轮0收费/不部署，不将合成持久记录、模拟时间或有界数值认证为主观体验、对方情感、成长或完整工程通过。
