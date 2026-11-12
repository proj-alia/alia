# ALIA-P04-05｜来源绑定自传持久化

状态：**READY**。首轮独立固定输入的6组真实PostgreSQL测试全部通过，定向编译、严格Clippy、局部fmt及快照M0均exit0；源码前后稳定、独立PG清理通过、P04-04旧接受八源保持不变。0真实/fixture模型调用，未部署或初始化实际个体。仅为受信任的有界存储切片，不代表完整自传/认知作用或M2—M5验收通过。

## 输入、所有权与注册信号

已读取AGENTS与三份权威文档、ROOT-HISTORY-01/P04-05冻结契约、主线AutobiographyDraft/Previous/Validated/Error及0023迁移。开发目录仅新增 `src/adapters/autobiography.rs`、`tests/autobiography.rs` 和本handoff。未改root领域/迁移/Schema/context/HTTP/UI、P04-04接受包或其他任务文件。

**文件及内部API均已保存；root现已实际登记 `pub mod autobiography;`，本窗口只读核对通过。** 没有HTTP/model写入口或公共注册文件改动；定向测试副本内的这一个最小注册已实际执行验证，不冒称已发布开发服务。API：

- `Store::record_autobiography(key:&str,expected:ExpectedState,input_manifest:&[Uuid],draft:&AutobiographyDraft)->Result<AutobiographyReceipt,StoreError>`。
- receipt严格六字段 `individual_id/autobiography_id/previous_autobiography_id/event_id/version/autobiography_version`；version为提交后的全局状态版本，新条目autobiography_version=1。
- `Store::autobiography(include_history:bool,limit:u32)->Result<Value,StoreError>`，limit1..100；响应及11字段item均按冻结契约，完整标识当前/历史解释。

## 存储与来源边界

复用既有advisory写锁、状态FOR UPDATE、expected.version/privacy_epoch、active检查及 `/internal/autobiography` 幂等摘要/回执。无网络或模型等待。同topic必须指向同个体/current/正版本前身，不允许静默覆盖；旧条目只改变status/version/superseded_at，新条目新ID，原文本与原事件保留。

全部manifest对象必须实际owned且递归可用，manifest非空/去重/非nil，draft引用须包含其中。event_ids必须来自实际events行，认识UUID不能冒充事件；fictional_background与decision.reviewed反馈不作经历依据，包括额外塞进manifest的该类事件。生成内容、陈述或导入事件仍只是其原source_kind的实际记录，不自动证明世界事实、共同经历或模型已读。

直接引用及manifest中的认识必须带齐全部支持/反证和完整前身认识及其实际依据；不能仅保留有利材料或仅引用认识ID。前身自传的原有来源继续递归检查可用性，旧解释不被复制为新事实；新条目只保存本次明确选择的event_ids/belief_ids。

隐私闭包联合实际derivation_edges、自传事件/认识关联、双方前身及belief_evidence；UNION终止循环。缺少来源边时，仍不能泄漏关联表指向的隐藏事件、认识、前身或反证。写入、只读与实际回执重放均检查；关联实际事件为空的自传不公开。派生边连接全部manifest与新条目/修订事件，修订事件source_kind=inference、actor=individual，类型autobiography.formed/revised，不伪造用户消息或工具成功。

更新旧条目、插入新条目及关联、事件、来源边、autobiography.changed、state.changed、版本与幂等回执在一个事务内；晚期失败全部撤回持久记录。只读单个RR/READ ONLY快照，先按完整隐私过滤再排序/limit+1/has_more；读取不写事件/费用/回执。has_more不是完整自传，也不提供永久跨调用快照；隐藏条目不等于从未发生。

## 固定输入与实际验证

唯一父任务session29664已终态exit0。固定基线不可变 `belief-context-errors-20261112-n5r3`，manifest SHA `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974`；上覆root四个精确新域/迁移/注册/诊断23文件、自有adapter/test及副本内最小adapter注册。与root随后保存的接受包 `root-history-01-domain` 四源逐项完全匹配，manifest SHA `0216662e77b8b9968d5e37e19418113e253d3e32d669df4b3af91266c7b31490`。未修改N5R3，不采用并行变化的开发checkout进行构建，不混入新observer/UI。目标独占 `target/p04-autobiography-v1`，实际镜像及资源身份在报告中，PG串行；现有总锁从固定输入到资源清理持续持有，仅执行一次，无超时重启或重复作业。

实际命令：

1. `cargo check --locked --test autobiography`：exit0。
2. `cargo test --locked --test autobiography -- --ignored`：exit0，**6 passed/0 failed/0 ignored**，6.32秒；显式执行需要独立数据库的用例，默认ignored不计通过。
3. `cargo clippy --locked --lib --test autobiography -- -D warnings`：exit0。
4. `rustfmt --check --edition 2024 src/adapters/autobiography.rs tests/autobiography.rs`：exit0。
5. 用现有`.venv/bin/python`执行固定源`scripts/check_phase0.py`：exit0，M0 PASS（5 schemas/49 closed objects/10 examples/56 negative cases）；不代替行为或14天观察。

六个实际PG组：

1. 形成、重启读取、同topic重新解释、原始陈述/生成/导入事件完整保留、当前/历史版本与实际inference事件/精确回执。
2. 同topic不可无前身覆盖、跨topic/旧前身/错误版本拒绝、unknown owner与真实FK屏障、manifest缺漏/重复/nil、同key重放/意图冲突，失败无行级副作用。
3. 认识同时保留完整支持、反证及前身实际事件；缺反证/缺旧依据/仅认识无实际事件/把认识当事件均拒绝；背景与审查反馈、异常存储认识反例拒绝。
4. 删除条目/认识/修订事件的派生边后，两代来源、counter、认识、前身和身份撤销仍阻断读/新写/幂等重放；独立topic保持可见。
5. 最晚state.changed发布注入异常，验证旧status/version、新行、事件、state、receipt及来源边均回滚；移除注入后同key一次成功，随后重放不重复发布。
6. 103个可见主题+3个较新隐藏主题，先过滤再limit/真实has_more/同刻UUID稳定排序；空结果和非法limit；使用default_transaction_read_only连接实际读取；全public表内容指纹不变，paused与过期privacy门拒绝新写，usage=0。

主线另报5个纯领域测试及严格lib Clippy通过，其证据在接受root包中；未在本轮重复执行或混算为本6组数据库用例。

## 证据与最终摘要

- 报告 `evals/results/p04-autobiography-targeted-20261112-v1/report.json`，SHA `c0e82332f79f737e265856a041675263e8086c55b3804e060967488b6464f101`。
- 固定完整source/overlay/注册旧新SHA manifest，SHA `7ad606a70386367fc4c1b0737a0e937134d20425f80380b0ad307965ffc98579`。
- 实际执行测试产物 `target/p04-autobiography-v1/debug/deps/autobiography-4a74058152aac3e8`，SHA `d1dd08cda647d9e321848eb8966e96c59955069edfdb61a3e2e8535e3ac4c1b7`。
- report.passed/sources_stable/previous_accepted_sources_stable/cleanup.passed均true，error=null。每项实际命令和原日志保留，未运行完整工程矩阵，未调用模型。本任务没有失败运行或未通过断言。

自有两源交付SHA（再次与固定输入核对一致）：

| 文件 | SHA256 |
| --- | --- |
| `src/adapters/autobiography.rs` | `9ddb563ac0fb657eabdaa4884023530f258e65223b78e44652552d1c61a20412` |
| `tests/autobiography.rs` | `8a5ba5c4ac4eafea70ca48de4d5cc138f0432b2fab60fedffd8d86abb5314988` |

root四源只读输入（未修改）：

| 文件 | SHA256 |
| --- | --- |
| `src/domain/autobiography.rs` | `74ad63fc3d12dcfeedf51b90f080e3857bc61a6b66494e62e1780d751599c80e` |
| `src/domain.rs` | `d91180e2ee965514332b02c8edae05cb3152a968f5152060095bdd33a2318a40` |
| `migrations/0023_autobiographical_interpretations.sql` | `7f508a40db40e35b05996abeff46e9cbf564eab1c9721e99b5dbc3ba4c806b97` |
| `tests/api.rs` | `4199f7fa7432807dca16b7cfa5e0e7fd9fa4681fb47dfc84aaf071bd6bfaee27` |

## 限制与下一依赖

这是结构化来源可用性及原子持久化工程证明，不是叙事语义正确性或实际模型学习证明。fixture明确是合成持久记录，不认证共同经历。当前无模型/HTTP/context自动调用、自传选择预算/逐目标审查/行为影响、情绪长期情感/关系分层/成长等实现声明。auth独立internal方法只供受信任调用，未来外部入口须由root另行设计和校验。读取有界100，不宣称完整导出/物理删除/备份验收。

已读取最新权威覆盖：先推进完整M5工程候选，再集中真实模型验收；旧“先单独v5付费接受才开发自传集成”的前置条件已撤销。root可以按稳定接口和定向工程继续schema/context/model集成，不以付费trial作为开发门；预算/质量/来源/验收标准不降低。

下一依赖：root核对并接受两源（public adapter已实际保存），P03纳入下个固定候选及诊断23整合；根新领域四源及其0023实际SQL已被本轮PG执行。后续情绪、长期情感/关系分层及M3持久化由root另行冻结接口释放任务，本窗口不自行扩大所有权或触碰旧P04-04八源。
