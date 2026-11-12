# ALIA-P04-08｜稳定兴趣身份与修订

状态：**READY（存储与helper限定切片）**。首轮固定输入5领域、6主PG组与1内部helper PG组全部通过，编译/严格Clippy/局部fmt/快照M0 exit0。源码稳定、独立PG资源清理与旧P04-04接受八源稳定通过，0真实/fixture模型调用，未部署或初始化实际个体。不是完整M3发展、人格试用/独立评价/接受撤回或身份连续性全链验收。

## helper保存信号与所有权

已读AGENTS与三份权威文档、稳定兴趣冻结契约及0027真实SQL。仅新增domain/interests.rs、adapters/interests.rs、tests/interests.rs、本handoff；未改root初始化/context/goal/check/注册/Cargo/schema/迁移/其他状态源。旧07finalREADY源与证据保持，不并行修改其他领域。

**root现在可登记domain和adapter的 `pub mod interests;` 并接入真实调用。** typed接口InterestDraft/InterestStatus/CurrentInterest/ValidatedInterest/InterestError、InterestReceipt及Store::record_interest/interests/interest_history均已保存。

pub(super) helper均已实际测试：

- `seed_interests(tx,individual:Uuid,seed:&Value,birth_event:Uuid)->Result<(),StoreError>`，只用于出生事务，严格验证完整合法seed、stored immutable seed与其真实digest、同owner最早system/direct_observation individual.created及其seed_digest；仅创建缺失legacy项，重放不改已修订/retired行，不是读取初始化器。
- `for_context(tx,individual)->Result<Vec<InterestRef>,StoreError>`，从真实表选择完整可见active兴趣，按created_at/id升序稳定排序，最多33项供root决定32项及has_more；不从mutable persona array派生身份。

root拥有live initialize/context/目标来源与提交复查接入。本轮只在私有固定源中加最小出生调用和旧context兴趣数组替换为helper+32项调用者上限，以编译和实际验证接口；**不将这些私有桥接冒称live已接通或全部目标retired检查已实现**。需root真实保存并整合复验。

## 稳定身份与实际存储

六字段draft为id/expected_version/description/status/event_ids/brief_basis。新建id与expected均null且active，由程序产生interest:UUID逻辑ID及独立opaque object_id；用户或模型不能指定新ID。既有id+正expected_version同时提供，严格加载同owner实际对象并比版本；description≤2000、basis≤1000 Unicode字符，非空/NUL拒绝，事件1..32唯一owned且真实，背景/审查反馈不作实际来源，unknown/noop拒绝。

修订、退休、恢复保留同一logical id和object_id，只升版本，初始origin_kind保持；不从当前数组索引、描述或排序重建身份。旧goal.origin_ids和immutable individual.seed/seed_digest不写；当前persona数组改变不会重置兴趣。retired保留历史与身份，for_context不提供它作为新目标候选；完整实际目标/提交gating继续由root接入。

当前interest_sources累积保留所有实际引用，包括initial_seed的真实出生配置事件；immutable修订记录保存前后description/status/version、实际inference formed/revised event_id及previous_revision_id。seed初始来源只证明配置，不认证亲历；experience形成路径也不自动认证语义或世界事实。读取不会因演员未出现、时间或调用数自动退休/衰减。

写事务复用全局锁/state版本/privacy、active/paused、幂等、实际manifest来源检查。更新时opaque object_id及实际最新revision须在manifest；派生边覆盖manifest→新事件/修订与当前对象，跳过无意义self-edge以保留DB source<>target约束。当前行/历史/事件/来源/interest.changed/state.changed/全局版本/回执原子，任一晚期失败全回滚。相同意图回执重放复验当前隐私，不重复写。

隐私闭包同时走derivation_edges、interest_sources、revision的真实event/previous FK和当前object→最新revision，及已有真实关联；UNION终止循环。即使本资源边缺失，旧实际来源或前身被撤销仍阻断当前/历史/新写/私有重放，独立兴趣保留可见。保留整个争议来源，不裁掉部分依据来展示剩余文本。

list与history为单RR/READ ONLY，limit1..100、先完整可见性再limit+1/has_more。list item精确9字段、history精确12字段，均按created_at/id升序；原始seed无修订时history为空，新experience第一修订before_version0、before_description/status=null。没有读时初始化、费用/回执/时钟写入或永久跨请求快照声明。

## 固定输入与实际命令

唯一父任务session53199终态exit0，报告`evals/results/p04-interests-targeted-20261112-v1/report.json`。固定N5R3 manifest `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974` + 不可变root-history-01-domain/p04-05/p04-06 + 精确root Cargo/lock/0024-27/diag27 + 自有三源/私有双module注册、出生/context最小桥接。没有复制正在变化的live隐私runtime、HISTORY02或后续M3模型/schema代码。

接受输入manifest依次为 `0216662e77b8b9968d5e37e19418113e253d3e32d669df4b3af91266c7b31490`、`fdf780dbb722a8f9457a6babadd3216679f476232d9623c573c27533b96704fc`、`4403f1d35bc529859e769e38759559253d166faa19ea124b21588f59dbcd3fba`，逐文件SHA核对；Cargo/lock按固定完整SHA强制比较。副本注册和两处桥接的旧新SHA及全部源集合在manifest，原输入不动。target独占`target/p04-interests-v1`，实际固定镜像/自己工程PG、RUST_TEST_THREADS=1，总锁从固定输入到清理持续持有，无重复作业。

1. `cargo check --locked --test interests`：exit0。
2. `cargo test --locked --lib domain::interests::tests`：exit0，**5 passed/0 failed/0 ignored**；66 filtered out未运行，未计通过。
3. `cargo test --locked --test interests -- --ignored`：exit0，**6 passed/0 failed/0 ignored**，6.76秒。
4. `cargo test --locked --lib adapters::interests::tests -- --ignored`：exit0，**1 passed/0 failed/0 ignored**，1.13秒；70 filtered out未运行。此为实际PG helper组，不混算为纯领域测试。
5. `cargo clippy --locked --lib --test interests -- -D warnings`：exit0。
6. `rustfmt --check --edition 2024 src/domain/interests.rs src/adapters/interests.rs tests/interests.rs`：exit0。
7. 用现有.venv Python执行固定源`scripts/check_phase0.py`：exit0，M0 PASS（schemas5/closed_objects49/examples10/negative_cases56）；不代替行为或14天观察。

主PG六组实际覆盖：

1. 新DB先实际运行0001..0026，持久化明确synthetic legacy seed/persona重排/旧goal，再实际执行0027；legacy ID与描述取immutable出生seed，birth source和opaque对象实际生成；迁移重放不增行/重置，旧goal origin保持。
2. actual Store.initialize（通过固定私有出生hook）后新建、改描述、退休、恢复、重启及persona数组改序，logical/object身份、初始seed与旧goal引用保持；当前列表/完整前后历史形状正确。
3. owner/unknown ID、全局privacy/对象version、manifest对象/前身缺漏、伪造新ID、unknown/noop、fiction/feedback拒绝，无持久副作用。
4. 去掉当前interest/revisions/发布事件来源边后，两代真实来源、当前对象、旧revision撤销仍拒绝当前/历史/新写/私有重放，独立interest可见。
5. 最晚state.changed outbox注入失败，同时验证新建与修订全部回滚；去除注入后同key仅成功一次、重放不写、旧全局版本拒绝。
6. 103新兴趣与同一稳定兴趣103条历史、额外3条隐藏来源，真实limit/has_more/过滤、实际只读连接/全public表指纹不变/非法limit、paused可管理而archived拒绝、usage=0。

内部helper PG组另验证：新出生初始化真实创建seed项；显式修订并退休后重新调用seed_interests不重置row/version/object/status；34个新active加现存seed时for_context精确最多33并排除retired；错误出生模板和实际非birth事件均拒绝。领域5测试覆盖新ID由程序分配、保留legacy身份、版本/owner/noop、Unicode/closed字段、实际source可用性。

## 最终证据与源摘要

本任务首轮即通过，无失败运行或隐藏重试。report SHA `dfcc1fdf81ec76eae8f5302600b3b9d5ae3d3cc96a63129e99155252d52807d3`；source-manifest SHA `26121d1a9cddbdb8dac8908ee6c50346641f417c8ba309ed8f798d801d735c66`。实际集成测试产物 `target/p04-interests-v1/debug/deps/interests-b7cbb06b96b9d6fb` SHA `1f3a94d34499940e317bd950ef41bf323f00988071a39781054e639ff9f3c215`。report.passed/sources_stable/previous_accepted_sources_stable/cleanup.passed均true，error=null，精确命令及原日志保留。未跑完整矩阵、付费模型或部署。

自有三源已与实际通过输入再次匹配：

| 文件 | SHA256 |
| --- | --- |
| `src/domain/interests.rs` | `403d708ff50b5aa4cf4c38410fc4f321764bee77c7aa63ebe6f9905cc256762f` |
| `src/adapters/interests.rs` | `136ccddf9b0526615e92fa9c55eed6f84c973123727d54db027b0af26119ed9e` |
| `tests/interests.rs` | `f9abf420c55243be400ab97651191611090906caaae8171498e43af8c97c18a0` |

root只读覆盖输入（未修改）：

| 文件 | SHA256 |
| --- | --- |
| `Cargo.toml` | `82c84ee0d075498e12cd77063e45579c089c13b6363023827fdd6da179f1866a` |
| `Cargo.lock` | `8e756235d926f7043d6f8801ebf49b170e9c2d0e5ef7891a785112ad43dd595a` |
| `migrations/0024_privacy_operations.sql` | `1debd5d0f8d508ffbec4a9e18b3bad133fd6ff8e6df8faa17325f3585c4dccfe` |
| `migrations/0025_affective_layers.sql` | `a7fc966a842dc80fc2d058e4ac69cb9d7e2cbc9f878ff6fbcf575f038bed57a7` |
| `migrations/0026_relationship_revisions.sql` | `aa5f8196891a899608ff4a39dac067f7a421734d4216e207dd7f2bef95e4e279` |
| `migrations/0027_stable_interests.sql` | `5888fc6a754a6074c785449203fcf46bbe94ae1e38289408d2ed146911d3c9e7` |
| `tests/api.rs` | `3cd533ec485f4696aef4aa0ec86bf66be10219776f64eaa73985310fa637e39c` |

## 下一依赖与限制

root核对并接受三源、实际双module/initialize/context/goal/check接入后，由P03纳入后继固定工程候选及诊断27；P05将interest/interest_revision/FK source family纳入真实purge。当前只实际执行root0024表建表，不认证journal/purge业务。

若live仍用旧人格数组或未接源复查，只能记稳定兴趣存储/helper完成，不记M3身份连续性全链或新目标拒绝retired整体通过。当前无模型自动写入、人格/价值trial/独立验证/接受/撤回、实际成长或集中真实验收声明。root下一0028发展/人格修订契约另行冻结，本窗口不自行扩权。按最新路线先完整M5工程，再集中真实模型验收；预算、质量、来源与原验收门槛不降低。
