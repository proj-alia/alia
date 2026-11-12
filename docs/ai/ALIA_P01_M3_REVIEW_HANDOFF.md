# P01-M3-REVIEW-01｜READY：初始09B有限审查与2项新增PG通过

范围：只读审查p04-09a及初始p04-09b的可信试用/独立评审token、来源/反证/验证者、重启重建及接受/撤回边界。仅拥有新 `tests/development_boundary_regression.rs` 和本交接，不改生产、原测试、阈值、共享schema或其他worker。正式root任务已明确释放新增有界验证；其他只读准备通知的“等待正式后继”条件已由该释放满足。

## 固定输入

已全量核对 `p04-development-storage-20261112-v4/source` 的360源，09A接受两源、09B接受三源及原报告。09A manifest SHA `15215fb354828df7e1ea17eb433addf2008fd6aba13abc94d3a2aeae9f0c83d1`；09B manifest SHA `dff02568d5e7b4ada1cd99311a4e6029494e3b9e3f81e6ad9b437956b4565370`；原report SHA `969b29c124378a3cf133dd1d5d1c0a3ae844893b811441730824f4ac191f5c98`；完整source manifest SHA `cc90ad49928d2b537feadf297159e9df281e2107edad36a8711d9d745efe1d60`。

| 初始09B实际固定源 | SHA256 |
| --- | --- |
| src/domain/development.rs（09A相同） | 93b36aad9e668ef2348be790f0dfbb9b1abaa8237de08256770596500492f1fc |
| src/adapters/development.rs | b8c4779ffad2f8de431236c43eb9be19ee0afdaad8488e7842e4b7e9680e4fda |
| tests/development_storage.rs | 33241c98c9b741736fee19a48bb7420a7549a803f5421f6e991182c20aac0b21 |
| canonical0028 | e61da144fa25a887d5a1c686c8dccd554c44981051608e732a1d8d791fc37f12 |

未带入live后续owner-revert、Social或隐私重建helper。schema28清理/人格来源重建尚未接通是已明确的P05-07/整合依赖，不列为新线上漏洞。本切片尚无模型/HTTP发展命令入口或真实验证器写入器，也不因其未开放而制造线上回归。

## 已有覆盖与静态审查

读取原15domain/12PG源码和实际02/03日志，确认原退出结果15/12通过、0ignored；本次不重复全套。原域覆盖token绑定精确candidate/trial、独立性/反证完整、失败/未验证保留、重复context/result、过期、target/ABA撤回和所有白名单路径。原PG覆盖正常接受/新连接撤回、accept幂等/晚期回滚、提案并发和中途fence、失败洗白、伪评审字段/注册、来源FK撤销、scope、分页和严格重建。

adapter按实际events/action/登记记录重载SourceClass；RecordValidation必须匹配development_review_records的candidate、trial、reviewer及assessment原值，注册来源/实际观察/来源FK全部核验，再由TrustedValidator构造不可Deserialize的token。load从候选和连续steps重放，每步重新加载来源/可信记录并核查实际event摘要、transition/validation及revision；不将保存的accepted bool直接当授权。accept/revert只替换白名单目标，核对实际version/head/value并在同事务写persona、target、revision、事件、步骤、回执；普通读取/幂等重放也完整重建。

静态检查尚未确认新绕过，只补两个未被原组单独覆盖的组合：

1. 两个候选均在实际trial，用A真实存在且正确绑定的合成server评审引用请求B的RecordValidation，须按candidate/trial绑定拒绝且全public表指纹不变；A同记录正常接收。新连接重建后，同key回放不新增验证；新key重复同review_id也必须拒绝。B不能借A评审Accept，A仍可正常Accept。
2. 两个候选各自具备正确合成独立评审，持同expected并发Accept，恰好一项成功另一项StaleState；失败者刷新全局expected后仍不能覆盖已改变的target head。新连接核对accepted/trial状态；仅对实际Revert revision INSERT注入QA失败，persona/head/全部public表回滚；去故障后原key/原expected完成一次并精确重放，恢复原persona但保留接受/撤回两条revision。

可信注册/观察/评审SQL仅用于正确的合成前置条件，沿用已接受fixture形状，不作为模型攻击能力；不修改这些server记录来制造漏洞。被测不可信输入仅是DevelopmentDraft、实际评审引用与DevelopmentCommand；不伪造ServerSource、私有token、accepted状态或服务器身份。0模型调用，合成可信评审不证明行为质量或独立验证器已实现。

## 执行状态

使用固定360源加新test的独立361源，manifest SHA `9d7a61eed89029c90ae91117239f9c2a50f5671d5a3dd5886ddd7976d851536b`；test SHA `1eb23c51a1ea9bd4bd2370d66f6d15222b858e62105c70e576a102b028133412`。运行前不存在的 `work/p01-m3-review-01/target-private` 单独编译，不共享rmeta/rlib，wrapper禁用。全局锁实际覆盖首次编译、自己label/ID私有PG与精确清理；下表为实际终态。固定360源与新361执行源的完整SHA均保持。


## 实际结果

在本次固定初始09B与合成前置条件范围内，未复现评审引用跨候选使用、重启后同review重复计数、竞争接受覆盖当前head或撤回故障留下部分写入的新绕过。此结论是有限工程审查，不是安全完整或成长行为通过。

| 实际命令 | 实际结果 |
| --- | --- |
| `cargo test --locked --test development_boundary_regression --no-run` | exit0，新独占空target首次编译64.21秒，随后才创建PG。 |
| `cargo test --locked --test development_boundary_regression -- --ignored --test-threads=1` | exit0，2 passed / 0 failed / 0 ignored，实际测试2.73秒；只运行两个新组，没有重跑原15域/12PG。 |
| `cargo clippy --locked --test development_boundary_regression -- -D warnings` | exit0，26.66秒。 |
| `rustfmt --edition 2024 --check tests/development_boundary_regression.rs` | exit0。 |
| `/workapps/project-alia/alia/.venv/bin/python scripts/check_phase0.py`（固定361源） | exit0，5 schema、49 closed objects、10 examples、56 negative cases；这是初始09B的旧固定基线规范检查，不代表当前在变主线文档/源码。 |
| 单父进程与清理 | exit0、cleanup=true；清理后按原label/ID独立inspect确认容器及volume均不存在，私有state password已移除。 |

第一组实际结果：B引用A正确server评审在 `missing trusted development review` 处拒绝，全public表指纹不变；A正常RecordValidation进入version3。新连接同key回放回执完全相等且无写入，新key重复review_id返回development policy拒绝，只有1项validation；B无自身评审不能Accept，A可实际Accept，只有允许traits路径改变。

第二组实际结果：两个已验证候选使用同expected并发Accept，恰好1成功/1 StaleState，实际accept revision只有1。落败者刷新全局expected仍因目标版本/head冲突拒绝且指纹不变。新连接看到winner accepted/loser trial；Revert revision INSERT的隔离QA故障实际返回DB错误，全public表指纹、persona和head保持。解除故障后用同key/expected撤回一次，revision ID与accept不同；精确重放不再写入，完整persona恢复基线，最终保留accept/revert两条revision。没有篡改私有token、可信评审行或accepted状态制造失败。

两组均assert实际usage_reservations=0；真实模型、合成模型调用、收费、部署均0。首次新target执行即2过；没有新增编译/断言失败或调整断言后的重试。旧09A/09B失败与通过报告、原独立SSE RED/v2通过、HISTORY-REVIEW、EVAL-03正式unverified结论保持。

`evals/results/p01-m3-review01-v1/report.json`保存真实命令、时长、queue、来源/runner/test/log SHA、PG公开身份和cleanup。实际新bin SHA `fc46426a696169fc40c319deb51b3fa82bf99b15545f0412ca0175d0a0676aa5`；日志确认从 `work/p01-m3-review-01/target-private/debug/deps/` 执行，运行后再次核对同SHA，未复用共享产物。完整新源清单、输入核对、格式/M0日志与原始执行日志随READY保存。

## 限制与下一依赖

本次没有对缺失的可信生产验证器注册/观察/审查写入器作安全认证，也没有实际试用overlay影响模型行为或20组ABC成长评价。正确的合成server记录仅验证adapter会核对绑定，不能证明现实验证者真正独立、反证搜索充分、试用质量有效或价值变化合理。未穷举所有来源撤销/时间/并发/崩溃组合；实际新连接证明重建，不等于完整进程或受管备份恢复验收。

本次只绑定上表三个初始09B SHA及canonical28，后续live owner-revert/Social/helper自动不在验证范围。schema28隐私移除/人格重建是已知P05-07依赖，不能因未在此scope测试而列为新线上漏洞；本次正常撤回不替代隐私删除重建。P04-09B内部存储、M3整体工程、实际模型成长与M5集中验收分别记账。

下一依赖由root接受这两个新owned文件，并把后继真实试用/独立评审写入器、上下文来源、隐私重建和运行接入绑定各自命名固定切片。无需迁移EVAL-03/collector；原100检索/60x3/20ABC与十四真实日观察继续待完整M5候选，预算、收费/重试、权限和隐私规则保持。本次没有需要root实施的生产修复建议，不宣布安全完成。
