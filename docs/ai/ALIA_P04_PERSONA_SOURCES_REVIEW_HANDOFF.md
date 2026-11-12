# P04非作者审查｜ROOT-PERSONA-SOURCES-01/r1

**decision=changes_requested，open_blockers=1。** Owner=Root，reviewer=P04；P05另作来源/隐私交叉。本次未修改Root生产源码、原七项测试、预算或完成标准。Root已在同一审查链确认同类TaskScope时序缺口并承担r2修复；原r1报告及本次独立RED保留，修复后的版本另审。

## 固定输入与证据核对

固定`evals/results/root-persona-sources-v3/source`，481源逐个摘要核对；manifest SHA `3f8b02c52333d9884791c529af480a5d0a5b13abdfd71a759779e56b6bb7e36a`；作者report SHA `61dbb3939495d91f873697ef2d0db18a6c2a69a678c64bb02dd2072a9a7733cd`。作者15日志及12封存执行文件全部验证。任务卡主体摘要 `2e1ec02fd7aaf76cfd768e3a2a5a29bcce388056e90c5bbe826289025324a569`。

相对HTTP-v5无移除源；development.rs仅LINKS/available/immutable_seed三项pub(super)可见性与其格式改动，P05 privacy_development投影算法未变。Root社会r4已审修复被纳入；interest_context只有已存在可选字段及P04-11固定领域DTO，不接兴趣helper，不据此批准11。作者89普通/93PG及默认36 ignored分开记，不由本次补判为完整行为或发布。

## P04-RPS-r1-F1｜P1：Task策略的动态run来源使自身正常预留误拒绝提交

位置：`src/adapters/development_context.rs`的read闭包及require_context全等比较；`src/adapters/development.rs` LINKS的target→scope_id和run→input_manifest；`src/adapters/scheduling.rs` reserve_model_call追加实际context.input_manifest。

已接受的TaskStrategy scope指向当前run。首次模型快照的development_context.source_ids包含该run当时的祖先。预留会正常把本次完整上下文输入追加到run.input_manifest。随后require_context重新从同一实际head计算完整来源，其source_ids因此扩大；即使persona/head/version/scope没有改变、没有来源删除，重建视图仍与原prepared context不相等，返回StaleState。

实际独立PG `accepted_task_strategy_must_survive_its_own_model_reservation_and_commit`：复用作者显式合成观察夹具建立合法accepted当前Task策略，正常claim/model_step(FixtureProvider)，再fresh commit。结果 **0 passed/1 failed/0 ignored/0 filtered**：来源 **19→22**，提交实际`Err(StaleState)`。该次正常合成调用已经产生一条settled reservation、charged_tokens=**4097**，仍保留；这不是云端费用或真实模型行为。没有测试代码在预留后修改head、人格、scope、权限、来源对象或隐私代次。

原作者Task策略用例只验证model_context及reserve成功，未走完整model_step→commit。本次另外独立执行其原样development_context七项，实际 **7 passed/0 failed/0 ignored/0 filtered**。因此不是用原七项通过抵消本次RED，也不否定其真实局部覆盖。

影响：合法当前任务策略不能完成整条决策提交，真实运行若走到同类路径可能消耗已派发调用后才失败；本次未发真实模型、未测货币费用。修复须区分首次预留的真实完整来源与之后同一已绑定请求的复查，保持实际persona/head/scope、原始依据、真实FK/removed屏障及费用账本；不能靠清空来源、关闭后续验证或删反例通过。具体算法仍由Root统一裁决。r2还需检查真实独立review包的后续预留及合法缓存，不只让Fixture路径通过。

## 其他已核对边界

- 当前run→goal从实际runs读取并精确比对；Task策略优先Goal，未接受候选不进入persona；策略不新增权限、预算或完成条件。head元数据与人格分开，serde可选字段兼容旧输入。
- RR中验证出生摘要/真实最早事件与完整当前投影；模型来源4096和整包96KiB超限明确拒绝，不截断；公开persona采用完整图验证，未错误套用模型来源载荷上限。错误直接persona改值应拒绝，原API故障注入测试更新保留seed/实际故障值检查且另有真实accepted正向验证，不将此变更视为放宽原来源标准。
- reserve/fresh commit/review检查当前视图；lease/dispatch与缓存增加实际FK/removed检查，费用settle仍独立于lease，不因后续拒绝提交抹掉。缓存没有强制等于后来当前人格，合法后继重放的原语义保留。此次Task动态闭包问题须修后再确认这些组合路径。
- packet10在真实作者与review序列化路径携带来源视图；原allowed_evidence_ids未因development IDs扩大。协议/合成审查并不证明成长、事实或主观体验。

## 独立运行与交接

独立RED证据`evals/results/p04-root-persona-sources-review-20261112-v1/`：report SHA `14119c1cd81c9f66dbda5aacde891bc71bae2b0172048c5605d3209434316cd3`；manifest SHA `2b6f3698260800b527afded36107353e3ba839413360741e4f66f629532e6fc0`；review-conclusion.json绑定r1、主体、finding和日志。原RED反例源 SHA `82588452d8c0f0020b27adedb9376e353e51f8382c690ad4752f38d3396c9584`，实际执行二进制在释放专属target锁前封存。

实际命令：cargo test --locked --test persona_sources_boundary_regression -- --ignored --test-threads=1 exit101；cargo test --locked --test development_context -- --ignored --test-threads=1 exit0。独立schema29 PG容器/卷/端口、P04结果/快照及target资源锁，cleanup/source_unchanged/root_input_still_verified/review_test_unchanged均true。未重跑其余86PG、完整矩阵、收费模型、浏览器或平台拒绝操作。

原RED编译有共享fixture未使用成员警告。随后只对本测试target增加明确dead_code lint许可（复用夹具子集；不影响库或生产lint），测试正文/断言逐字不变，未重复PG。当前`tests/persona_sources_boundary_regression.rs` SHA `135baea75a00eeec5269fbe8f9a43e257cbc477d3823e56552e0678661a0639c`。style-v2固定副本实际严格cargo clippy --locked --lib --test persona_sources_boundary_regression -- -D warnings与局部rustfmt均exit0；report SHA `dc88c6a9037974033e92433d375da65d47796fc232440a696e0743f1f10fc756`，manifest `6d9c876094120456a19170204521b81a4e7084f94c13528d1d6ea60eafacce43`。v1原源码、警告、失败报告和执行文件不覆盖。

实际turn_context：turn_id `01a1d0e2-0b70-77e0-9dd5-20384f3e2391`，model `gpt-6.1-sol`，effort `high`；reason=跨调用人格来源与版本屏障独立审查。0网络模型调用；Fixture的实际测量账本与未知货币费用分开。下一依赖Root固定r2及精确差异，再用原反例和受影响屏障定向复审；P05交叉仍独立，不自行代修或发布。

## r2 / 固定v5独立复审：changes_requested

Owner Root，reviewer P04，**open_blockers=1**。r1-F1已解决并独立原样复验通过；新增r2-F2为未派发作者预留的幂等恢复缺口，不能因原提交链修复而批准本版。原r1及本轮RED均保留，生产源码未由P04改动。

固定`root-persona-sources-v5/source`全部481源逐项验证；manifest `a2f73e7108bd4a8771d6b516238b993bfac111698e2acae985bd23351a35d68e`，report `06a3a36b58bfb0a116c6b69951338b0b3a5dfd11fba9bec303603af9b4f81d4c`，evidence `29e644dabdb00d585c068e4cc4853ecfc48c48fcfa5ed157705d2265d8681c18`，卡subject `1aac285f1181262cba6161421e6cc0c172ae19792c7bec384a468004cff39a23`。卡全部交付源、19项v4/v5日志及1个v5封存执行文件均核对。相对r1只有development_context/grounding/scheduling三生产源及契约、单测变化；相对v4只有测试两处unused trait imports移除，实际diff确认，无生产/断言变化。作者v4的89普通/94PG与Clippy失败、v5实际8PG/Clippy通过分别保留，不冒称v5重跑全部94。

### r1-F1已解决的依据

首次作者预留仍完整物化并匹配实际来源。后续阶段验证真实persona/head/scope、原来源canonical/根集合/input_manifest及完整FK/removed，保留原观测列表，不要求其等于run后来增加的祖先全集。审查预留先将decision_review移除后，通过实际settled且已dispatched的作者reservation、individual/run/lease generation及context+原allowance摘要绑定；accounted仅内部可见性变化。fresh commit仍用opaque PreparedDecision和真实作者摘要；缓存继续复查原来源而不强制等于后来当前人格。没有据review标记直接放行篡改来源。

原`persona_sources_boundary_regression.rs`当前源SHA `135baea75a00eeec5269fbe8f9a43e257cbc477d3823e56552e0678661a0639c`一字未改，在独立固定r2副本实际 **1 passed/0 failed**。独立执行作者原样8项development_context实际 **8 passed/0 failed**，包括真实合成作者→独立角色审查→commit/replay，以及保持manifest/roots却改review.source_ids的摘要拒绝。只证明这些工程链路，不证明语义质量。

### P04-RPS-r2-F2｜P1：同key未派发作者预留仍被当首次新请求重验而无法恢复

位置：固定`src/adapters/scheduling.rs:365-381`。无decision_review的请求先调用require_author_context严格物化来源，之后才查询已有`usage_reservations(run_id,request_key)`。合法Task策略的第一次reserve已经扩展run.input_manifest；同一lease、key、原context和allowance再次调用时，便在命中原预留前因来源集合增长返回StaleState。

新独立测试`accepted_task_strategy_must_replay_the_same_undispatched_author_reservation`在相同合法accepted Task前置下，第一次reserve成功，第二次相同调用实际`Err(StaleState)`，**0 passed/1 failed/0 ignored/0 filtered**。第二次前后全表指纹相等，实际账本 **count/reserved/charged=(1,150,0)**。该用例完全未dispatch或调用provider，不涉及额外计费、泄漏或费用丢失。问题是丢失本地返回/重复调用时不能恢复原预留标识，不是重复写入。

现有`scheduling.rs`的`concurrent_claims_and_dispatch_are_single_winner`明确要求相同key/上下文两次预留返回同id，但其前置没有已接受Task策略，不能覆盖本组合。修复应区分“已持久化且摘要绑定的原作者请求恢复”与首次新作者预留；保持原lease、对象/来源/removed和摘要冲突拒绝，不通过新key、清空来源、跳过后续强校验或重复预留规避。具体实现由Root统一负责，不由P04代修。

### r2精确运行与下一依赖

主证据`evals/results/p04-root-persona-sources-review-r2-20261112-v1/`：report SHA `497af534cfa3fbf8b14801f1345996055a5345b56fc061c0fa560f625501e9be`；manifest `bf42157c611fc11431c7b198cbf9495e43cc33a635998968190492a64488aa96`；review-conclusion.json绑定本r2主体、resolved/open处置及所有日志。新测试`tests/persona_reservation_replay_regression.rs` SHA `e825feb074bfd0ba4c61791725c97d7652337ab31b9897d39b5e5ad3e2ba7784`，仅P04自有新测试；原独立测试不改。

首次multi-target cargo命令因新失败binary提前停止，**原反例在该命令未执行**；没有算成通过。随后只补跑原反例一次，另存`p04-root-persona-sources-review-r2-original-20261112-v1/`：report SHA `eed8b382955b39d474b64cf1f64b999016e999ae3ed6e094ba972d496111bb48`，manifest `97472b6046a48d1b6dd1d29565138d7ffcbf004e7f638dd368faa5ee0f40bdf3`，实际1项通过。主报告第二命令8PG通过；两自有反例+库的严格Clippy和局部rustfmt均exit0。两个运行各有独立临时PG/结果/源码，复用P04已终态target并持锁，执行文件锁内封存；cleanup/source/root_input/test稳定全部true。

实际turn_context `01a1d0ef-270a-7201-8ddf-2231de4d67ca`，model `gpt-6.1-sol`、effort `high`；reason=跨阶段来源及真实账目绑定独立复审。0云端模型，旧Fixture及作者合成provider总次数未独立汇总，不记零；新F2用例0provider/0计费有直接路径和账本证明。未运行其余86PG、完整工程、浏览器、收费行为、成长ABC或十四日观察。下一依赖Root修复原请求恢复边界并给新固定revision，再原样跑F2、原提交反例及受影响强来源/审查账目门；P05隐私交叉仍单独完成。

## r3 / 固定v6独立复审：approved

Owner Root，reviewer P04，open_blockers=0；只批准本固定r3来源/事务增量，保留r1/r2失败，不替代P05交叉、完整工程或正式行为，不批准随后P04自写M3生产代码。输入483源全部核对，manifest `a965d6b835e029ea987dd2babe60b534b01394dce3f547a53253d811aa482b1c`，report `37e339bdd87dc6a441b9d18e5d49b12e29da08f2ca0a5bdb7b8b65c70b0f4fbc`，evidence `9feacd62a7317d534e643c16511979fcb613836e31437319eabf747aec289528`；subject `6c31a68f153f6ae2df43be7c8abb721942f130268b788a64b3bebfb74bfa8310`。14日志及10封存执行文件摘要核对。两P04原反例SHA与r2交付完全相同，未修改断言。

相对r2唯一生产改动为scheduling：有效lease/直接来源门后先取真实individual/run/request_key的持久预留，精确认证context+allowance摘要及generation；相同旧作者请求使用已绑定快照复查，首次新key仍完整物化精确比较。review依旧认证实际settled作者context，所有原观察、persona/head/scope、完整实际FK/removed复验保持在返回原reservation之前。不同上下文/allowance不凭同key恢复；过期lease、撤销、removed仍拒绝。未将record存在本身当授权，也未重派发、重预留或篡改费用。

独立执行两原反例**2过0失败**，作者原样development_context **10过0失败**；包含reserved/dispatched/settled三阶段并发同key返回同id、单次dispatch和原账目，以及改context/source/allowance、新key旧快照、错误generation、祖先撤销/removed拒绝。严格Clippy与局部格式均exit0。此为固定相关12PG，不重复作者86PG或全矩阵；无收费模型、部署或生产初始化。原r1提交及r2恢复两个发现均resolved。

证据`evals/results/p04-root-persona-sources-review-r3-20261112-v1/`内report、source-manifest、review-conclusion及4日志与3个实际执行文件均保存，结论绑定Root subject和独立报告摘要。独立PG、专属源码/结果、P04 target持锁；cleanup/source/root_input/test稳定全部true。实际模型/回合见review-conclusion的model_actual，本阶段Sol/high。复审在M3-CORE写代码前完成；随后P04按Root正式冻结接口提取propose_in_tx，交Root+P05非作者审查，不能沿用本r3批准自批新作者切片。
