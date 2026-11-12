# ALIA-P04-10｜0029来源移除兼容

状态：**READY（canonical0029发展来源移除兼容）**。最终v4的19项owned实际PG、12项P05隐私PG、15项领域测试及定向编译/严格Clippy/fmt/固定源M0全部通过，源码/QA辅助摘要稳定、独立DB清理passed，0模型/未部署。09B-owner接受副本和09A序列化不改，GET projection字段不改。新增失败与其修复版本完整保留，最终源和报告见文末。

读/写/恢复/私有重放的来源闭包拒绝任何targets/developments/revisions.provenance_removed，即使对象blocked标记丢失也拒绝。live槽和candidate/revision查询仅非removed，SQL CAS也含此条件；removed不转换六态，不解封旧ID。

新head从immutable seed目标值（strategy null）建立，验证完整PersonaSeed/seed digest、真实最早system/direct_observation出生事件及可见性、实际同owner goal/run scope。基线version=max同逻辑target/scope全部旧head.version及其revision.after_version+1（初次1）；只读取旧opaque版本，不把旧对象/前身作为新provenance。新head的来源仅真实birth和scope，候选仅新显式manifest，避免旧删除日志沿边擦除新历史。插入version显式保存，不依赖默认1。

无revision head恢复同时要求：值等seed/null；版本等其他所有已removed历史head及其revision最大版本+1；不能有其他非removed头。带revision head继续核查实际未removed的revision/value/version/来源。后继及旧ABA撤回依然拒绝。新来源manifest若主动含旧removed对象，明确拒绝而不静默裁剪。

已保留原16实际PG/15域，并完成真实P05删除/清理/旧journal重放后的新提案/接受/重启、状态独立、基线篡改、版本竞争及幂等验证。固定canonical29及精确P05源码，在独立PG/资源锁执行；未改P05、canonical迁移或共享注册。root继续整合HTTP/认知/完整工程；不因本切片通过宣称M3行为或完整M5。

## 已解决的ROOT/P05边界失败（保留实际失败版本）

v1完整通过：19 owned实际PG/15域 + 当时P05十项实际PG + 严格Clippy/fmt/M0，0模型、cleanup/source稳定。随后为证明removed标记本身阻断**已有owner回执缓存**，及旧head缓存低于真实revision时max取修订历史，补强owned测试；v2 owned19/域15均通过，但复制到的新P05第11个用例实际失败：`development_privacy_initial_head_version_and_immutable_birth_seed_fail_closed`在tests/development_privacy.rs:935，真实UPDATE无revision head.version=99后，`Store::export_individual()`仍成功。v2原始`evals/results/p04-development-removal-20261112-v2/04.log`与report保留，10 passed/1 failed；失败后Clippy/fmt/M0未运行，不记通过。

绑定P05实现privacy_development.rs SHA1eadb4abfa98e143c1754cc1d5c8a30f2e82d83ca901574a5135d96a65e39ce7，test SHA8dab17e04d9e7ddb6d28e5841173fc3c77dadb57bfc4cc17dc1705a839a1ab87。位置为P05 `projected()`无revision分支，仅检查h.version>0及seed值，没有核查本模块已实现的generation_baseline（其他全部removed head及revision最大+1）；且P05 projected目前只检查相邻seed/hash，未核查真实出生事件seed_digest/actor/source。该新用例后半段还覆盖相邻seed/hash同时替换与birth锚点不符，但v2因首断言失败**未执行后半段**，不能声称后半段已复现。

root/P05建议：在P05受管导出/恢复投影路径独立加入实际birth绑定和相同逻辑scope的基线版本证明，或经root冻结复用只读纯校验helper；不要改/删除负例或将version>0视作证明。P04不编辑P05源，自己的verify_head/immutable_seed/generation_baseline保持严格。接下来保留该独立P05失败，同时跑完本任务其余独立检查；不把P05全套标通过。

v2另有测试代码`cached`未使用的编译warning（原测试新写请求未换为实际当前devversion），已在live仅修正该调用参数，确保新写拒绝不由旧ETag掩盖。v2固定副本和警告不覆盖；下一轮须严格Clippy实际退出0才交付新摘要。

上述P05问题现已在v4绑定的P05新源码ded74773f1802ab267de335a3d45c1ffc94ca5928c2304a28875917d734e7188修复；12项P05实际PG均通过，包括初始head版本及相邻seed/hash替换不匹配birth的两个断言。P04未编辑P05文件，旧v2/v3失败仍保留。

## 最终固定输入、实际命令与接收摘要

最终父任务v4 session51946终态exit0；报告`evals/results/p04-development-removal-20261112-v4/report.json` SHA256 **f52c12efc0fe45a0717a532f53d5456d9a827752ad415d77e9f50e295446ac4c**，source-manifest SHA256 **d8acbd07d8534fe2fff06287fc0b0d7b2148762e4fa069ece99f76dacb51f416**。evidence.json绑定三个实际测试二进制的独立副本、原报告、31项实际PG/15域计数及每项源。

基线为不变owner-v1源清单77084c7d15e47c617f3aba82ba2b513adf5843b276c5e94d6c072f21b69017ee，逐文件核对后只叠加P04两源和精确固定的P05/Root外部依赖；私有副本登记export/privacy_development模块，无live共享修改。canonical28保持e61da144fa25a887d5a1c686c8dccd554c44981051608e732a1d8d791fc37f12，实际增加canonical29 d56e9f0a09fe4f06069cbd26f883f49c9c1be8855b52fa80f51c7cb193cfdd7c。没有用不同名字的P05草案冒充canonical迁移。

外部固定源精确为：privacy_development.rs ded74773f1802ab267de335a3d45c1ffc94ca5928c2304a28875917d734e7188；privacy_operations.rs 4bd5c0e64a83191b7d278f3bb027b77343c90963fcc432c557122bb005972dfb；root deletion.rs 4b928a86a148bc9f16b258bf5311f19ecd1979f6597d1f1ec20b5288d07b7933；export.rs d29863c5483e0bab99fbed2fc3e5d25881a5ac68464f65a3c2b70e0699c77cd7；domain/export.rs 070c733cf1d943ab8a4f2f5d49a42cca92bc7e0ed0bed1d5a55fcff31015714b；tests/development_privacy.rs b2c1dde3d7a8538ca94006cbfdd772926853f9be7628f6401df76dba22fe4c95。全部复制前后核对摘要；没有修改这些文件，也不将其未来live版本自动算通过。

实际RUST_TEST_THREADS=1：

1. `cargo check --locked --test development_storage --test development_domain --test development_privacy`：exit0，7.18秒。
2. `cargo test --locked --test development_domain`：exit0，**15 passed/0 failed/0 ignored/0 filtered**。
3. `cargo test --locked --test development_storage -- --ignored`：exit0，**19 passed/0 failed/0 ignored/0 filtered**（原16全部保留），实际18.82秒。
4. `cargo test --locked --test development_privacy -- --ignored`：exit0，**12 passed/0 failed/0 ignored/0 filtered**，实际11.38秒。P05最新两个负例均运行，不skip。
5. `cargo clippy --locked --lib --test development_storage --test development_domain --test development_privacy -- -D warnings`：exit0，8.08秒。
6. owned四源`rustfmt --check --edition 2024`：exit0。
7. 用现有.venv Python运行固定副本`scripts/check_phase0.py`：exit0，schemas5/closed49/examples10/negative56。该固定基线M0不替代root当前权威文档/未来模型schema的整合检查；SQL29已由实际PG执行验证。

新三组owned PG分别证明：

- 真实begin+purge使旧accepted保持生命周期但removed/blocked，当前persona先回seed；合法新提案新object baseline3/value=seed，trial/真实可信fixture审查/接受后version4；同意图回执重放、新Store重启和旧journal重放不改全表、不擦掉新历史，导出有新label而无旧label/旧headID，旧对象持续blocked。
- targets/candidates/revisions任一removed标记本身（objects.blocked仍false）使detail/list、新owner请求和**已有owner回执重放**拒绝，全表指纹不变；测试新请求使用实际当前devversion，避免仅由旧ETag掩盖缺失来源检查。临时恢复flag只为隔离各表反例，不是生产解除旧ID。
- 旧removed head缓存version人为低于实际revision历史时仍按历史revision max分配新基线3，不能reset1；相同expected并发新generation仅一项成功/另一StaleState；无revision当前head的错误version/value、seed digest或真实birth actor篡改均在只读恢复拒绝，无副作用。

四轮证据都保留：v1按当时P05十项全过；v2 owned19/域15过，P05新第11项实际失败，严格后续未运行，另有已修正cached参数警告；v3修正owned测试后，owned19/域15/Clippy/fmt/M0全过，旧P05反例仍10过1失败，overall=false如实保留；v4绑定P05实际修复源及新增第12负例，31实际PG与全部七命令通过。没有覆盖失败、移除负例或把未运行补判通过。

资源按已释放协议持自己输出workspace、target/p04-development-storage-v1、独立pgvector/pg17容器；provision和cleanup持database-admin共享锁，cleanup另持本DB锁；子进程继承经校验的FD/map。v4总排队约0.000022秒，cleanup/source_unchanged/owned_live_unchanged/qa_helpers_unchanged全部true，error=null。0真实/fixture模型、0模型费用、未初始化生产个体/部署，不重复完整工程矩阵。

| 最新接收源/产物 | SHA256 |
| --- | --- |
| src/adapters/development.rs | cc6df7c97362cd7dd645af6cb06ae005594b230e1b4712ebc5d23d25bf18aca4 |
| tests/development_storage.rs | dfce1b3c7d60b0331ec232b21da1eec71be803b962718b4b86f85eea52cf41b1 |
| src/domain/development.rs（09A不变） | 93b36aad9e668ef2348be790f0dfbb9b1abaa8237de08256770596500492f1fc |
| tests/development_domain.rs（09A不变） | 04980a4c4564c70c9577bdc4f1f07a9aa0eae4d83f729cb1a80d5114db08ddd9 |
| v4/bin/development_storage | fec5e79718e6895e0ff8c7186b03b88793c09f290afaa68128e7011bb15833a2 |
| v4/bin/development_privacy | c6096e020a575634ef523f5681926926847d5c88d461f6d41c367a690eae50dd |
| v4/bin/development_domain | e815c2f521d82b94035111325b3841626190011601a12bdacc2b954d0025df2e |

下一项依赖：root接收本两源及精确P05修复版本、统一路由/服务和工程候选；P05继续自己的完整导出/恢复回归。本轮没有改09A序列化、GET字段、生产提示词/预算或验收标准。trial认知overlay、真实评估器、HTTP/浏览器、20组成长与M5完整验收仍由后续实现/实际证据证明，不能据本切片宣称完成。
