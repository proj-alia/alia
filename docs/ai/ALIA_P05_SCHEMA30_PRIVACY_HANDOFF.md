# P05-SCHEMA30-PRIVACY-01

后继检查点：[PORTABLE-02/r3](ALIA_P05_SCHEMA30_PORTABLE_HANDOFF.md) 已补 schema30个人导出和真实受管备份/隔离恢复专项，strict通过；本页保留的是此前r1消费接线范围及当时未运行项，不能混同为最新缺口。

Revision r1，**review_pending**；owner P05，reviewer P04（清理合同、事务）及 Root（共享 guard、整合）。本轮已接入真实清理 helper，尚未获得非作者批准。原 M4 bridge r1、08备份证据和 App Goal 进度未重置。

## 行为与边界

Root 当前委派要求在现有 purge 同 TX 消费 P04 唯一 helper，并补新表清理与恢复证据。`erase_development_in` 现在直接调用 `purge_development_trials_in`，没有第二套写 SQL、额外提交或 lint 虚引用。后续发展步骤故障时，计划、判据、上下文及结果的清理一同回滚。

`development_privacy_clean_in` 独立核对受影响计划 JSON/status、案例 criterion/context/status、attempt context/result/request metadata/outcome/state。未清理的私密副本阻止 completed；原 usage、budget 和 reservation 的 opaque 关联继续保留。program 是经作者程序核验的固定目录，case_sources 是 UUID 来源；两者不存案例正文。旧 schema29 缺少整组新表时保持原调用；部分新表仍由作者 helper 拒绝，未知 schema guard 保持。

新增三条真实 PG 用例覆盖：通用派生边缺失时经真实 FK/来源删除 plan 及所有私密副本、原计费账本不变；后续 development_steps 失败回滚；触发器吞掉 attempt scrub 时 clean 门拒绝，修复后独立日志重放，并在 completed ledger 旁恢复旧正文再重放清理、再次幂等。计划经实际 Store API 产生，费用通过原预留/派发/结算 API 注入明确合成 usage，没有调用 provider。已知48、未知按原预留60结算，未知 actual_total 继续 NULL，不能计成零或真实供应商账单。

本轮只修改 P05 `src/adapters/privacy_development.rs`、`tests/development_privacy.rs`、自有 runner/证据/本交接。未修改 P04 helper、0030、公共注册、Root controller 或权威文档。

## 固定来源

Root 基线 `evals/results/root-m3-run-isolation-v3/source`，加输入清单明确记录的 P04 当前作者源及 P05 两源。不是追逐 live 目录执行；P04最终接受状态不由本报告推断。

| 文件/产物 | SHA256 |
| --- | --- |
| P05 privacy_development.rs | `960bec614916a441f19152b3fd2addab8a017e475348b4379e7f799623c4c388` |
| P05 development_privacy.rs 测试 | `38f67cceb0643d615f765eb5e1d0daef0e9fbfd5d077e861b6c5c2beec9e0f5c` |
| P04 development_measurements.rs 固定消费版 | `52a3fe9db1bc13f7022c0ef6dd686b776f11bb3d582a42d54ccdc62c6646d253` |
| P04 canonical0030 | `214822c170b5fcd374b9cb1521d25ceb9d69147379eac526a108bc1fd346d255` |
| v4 完整源码清单 | `cc806cfcf1463ddf8055b328d616c344090d1b4e55ca0b9798f234fea84bc333` |
| v4 原报告（strict失败保留） | `054fbe0c1477dd6a25a21b62a462e929eb1dbfc377e06f7389cf379150afc510` |
| 派生交付汇总 | `bb3b682b6c860f12f1ac1fc758e95089fa0cdaef12a389c5d29de99d7425674f` |

完整证据：[r1汇总](../../evals/results/p05-schema30-privacy-r1-summary.json)、[schema30 v4原报告](../../evals/results/p05-schema30-privacy-v4/report.json)、[反例控制](../../evals/results/p05-schema30-privacy-red-v1/report.json)、[schema29兼容](../../evals/results/p05-schema30-privacy-schema29-v1/report.json)。v4与兼容版实际执行二进制另封存并绑定摘要。所有六次独立数据库/卷及私有临时目录清理成功。

## 实际验证

| 命令/范围 | 实际结果 |
| --- | --- |
| cargo test --locked --test development_privacy --no-run | exit0 |
| cargo test --locked --test development_privacy schema30_ -- --ignored --test-threads=1 | exit0，3 passed，0 failed/ignored |
| 私有反例：仅移除新增 clean 检查，保留作者 purge | 实际exit101，0 passed/1 failed；被吞掉的 scrub 导致 completed 数1，预期0；证明反例能区分缺口，不是绿色自证 |
| schema29独立固定副本，原13条 development_privacy，用 --skip schema30_ | exit0，13 passed，0 failed/ignored；新3条显式 filtered |
| cargo clippy --locked --lib --test development_privacy -- -D warnings | exit101：冻结Root基线 prepare_case_context/bound_case_context 两处 dead_code，另 development_trial_context.rs 一处 collapsible_if；未加 allow、未改其他所有者代码 |
| rustfmt --edition 2024 --check 两份P05文件 | exit0 |
| .venv/bin/python scripts/check_phase0.py，当前工作区 | exit0，schemas7/closed68/examples16/negative86/trace111/edges136/documents9/links43/sources2；不是冻结版本产品验收 |

v4 原报告保持 passed=false，因为严格检查实际失败；派生汇总分别记录3PG通过和strict失败，不改写原报告。P04 purge helper 已真实被消费，不再用未使用函数掩盖接线缺失。

## Root 精确依赖

原 Root controller 仍只接受 schema29 inventory；因此本次 schema30 用例在私有副本中仅更新该常量，没有直接改 live Root 文件。需 Root 审查并采用 [精确 guard 补丁](../../evals/results/p05-schema30-privacy-v4/root-schema30-guard.patch)：新真实列指纹 `b1d0736eb64657672a9eab87ca641611984993591e2e2bbbdc30102262aaefab`。计算依据为同一 pinned PG17上逐事务应用实际0001..0030后的 [schema proof](../../evals/results/p05-schema30-privacy-v4/schema-proof.json)，没有放宽未知/未来 schema。0030 包含新增 request_sources，不能借旧Root49快照的不同0030指纹。

仅P05消费改动的精确补丁 `work/p05-schema30-privacy/owned-consumer-r1.patch`，SHA `fb0e0c5d6a237cf004a7f427d4fe38ad0cff3ccc9d45deca4a5cdb51086598ce`。Root/P04采用或改变相关源后须复核对应版本，不能继承本版全通过结论。Root基线严格检查缺口由其所有者处理；不要求等待完整工程矩阵后才继续独立开发。

下一依赖：P04核对固定helper/迁移及清理判据；Root采用schema30 guard、冻结对应作者源和自己的上下文接线，P03按固定候选工程回归。schema30个人导出及真实 pg_dump/pg_restore 备份仍 **not_run**、尚未适配；本测试“恢复旧正文”仅验证原删除日志重放，不代替真实备份恢复。完整M5、行为质量和14天观察均未判完成。模型按Root Sol/high派发，实际本回合 model/effort未独立观察，不报告已生效。无收费模型、生产初始化或部署。

## 保留失败

v1校准 wrapper 报 docker exec 错误，未保存 SQL stderr，未进入构建/产品用例。v2逐迁移诊断定位0027的临时表在逐句提交后消失；校准器改为每迁移独立事务，匹配SQLx。v3实际3条PG中1通过/2失败，两失败均为故障注入断言后的清理代码将两条DROP交给单条prepared statement；修正为raw_sql多语句清理。v4三条通过、strict外部缺口仍失败。全部原日志/退出码保留，不覆盖失败。
