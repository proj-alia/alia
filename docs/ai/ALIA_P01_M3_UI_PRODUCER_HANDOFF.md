# P01-M3-UI-PRODUCER-01 / r1 review_pending

Owner P01；非作者reviewer Root+P04；Root唯一接口裁决。用户M34原文01a1d0fb-fd64-7cc1-ab59-159746deba45已直接核对，复用Goal。本片交付可执行代码及真实PG/HTTP工程证据，未自批为accepted。

## 代码与精确入口

新增 `examples/development_ui_fixture.rs`（SHA `4a0a4dd6af6fa190604a5742f71ef3357165d54e22dacf453ce23ef249c75a0c`）、`scripts/qa/development_ui_fixture.py`（SHA `5b8a20cf69b84f82dfd9d5b962a11a91e626cad0e334ae05948869b272099c07`）、`evals/p01/test_development_ui_fixture.py`（SHA `66375a6dd2bc81179ebf172c47b8fb3c04e7dc6a4b1ba8a3a8173377fc76fe45`）。不改生产代码、共享schema、UI或正式判据。旧泛化observer草稿保留在work/p01-m34-independent-01，不纳入本片。

精确编译入口为 `cargo build --locked --offline --example development_ui_fixture --bin alia`。固定源/二进制/清单/实际结果均在 `evals/results/p01-m3-ui-producer-01-r1/`；后端为Root指定v6的483源（原manifest `a965d6b835e029ea987dd2babe60b534b01394dce3f547a53253d811aa482b1c`），加3新源共486，逐项SHA一致。v6独立接受pending不伪造成accepted依赖。服务bin SHA `3d5d14b535e8b3645ae03297bf6b26e7e20e2de3cbb4fe484afc2bbc30d0cc3f`，producer bin SHA `5d4b580f1eeeec0202953553ee98e7c26fc77e9c096c1034fdb80506142f3767`。

Python hook 使用 `python -B /FIXED_SOURCE/scripts/qa/development_ui_fixture.py --request /PARENT_PRIVATE/request.json`；原hook仍为JSON argv数组。request仅私有0600文件，严格FIELDS见wrapper：database_state、database（alia_p02_*）、individual_id、service_url（http://127.0.0.1:PORT/api/v1）、service_token、source_root、source_manifest及其SHA、base_manifest、producer_binary/service_binary及其SHA、父目录中新output路径。凭据不放argv、报告或日志。父state必须0700目录/0600文件，数据库container label/ID/loopback匹配；固定后端不可变化，允许P02叠加已接受UI。每DB持久claim至多一次，失败/崩溃保留unknown，不能自动重seed。结果输出allowlist，stderr只可含代码行号，不回显panic/DB正文。

## 真实Store与目录

仅复用Root已明确允许的 `tests/support/development_context_fixture.rs` 合成注册/measurement/反证前置；未直接改accepted或approved标志。proposal/trial/validation/accept/reject/expire/revert均走实际Store；goal经真实鉴权HTTP创建，task来自真实submit_message。producer不initialize父个体、不改其autonomy（要求false），核对同service身份、真实current_database及空开发/验证表和零reservation，持整次DB advisory lock。

实际目录12项，六status/五target、真实goal/task作用域、实际等待deadline到期、保存Passed/Failed/Unverified和counter-search。五个当前accepted不同target/head，另有被后继替代的旧accepted。public report按真实prepared_at/id倒序分配 roles：0 commit、1 lost200、2 rollback、3 source_removed、4 survivor；role包含development/target/head/evidence IDs，0的development.version与global state_version不同。角色依据每次真实目录取得，不用事先随机UUID模拟结果。

## 实际命令和证据

- 固定source-v3构建（例子+alia）：exit0；原v1 E0603因GoalDraft为crate-private，改用公开鉴权goal API；未开放生产私有类型。失败日志保留。构建协调曾因旧QA common无resource_locks导入失败，后使用当前协调helper持资源锁，不改固定后端。
- 固定source-v3严格Clippy例子：exit0；归档source rustfmt --check：exit0。新独占target-private，未共享其他窗口target；model/cloud/fee调用0。
- 固定source-v3 Python wrapper 6项guard：exit0；覆盖绑定、生产名称/非法身份/外部URL/未知字段、source/bin变化、输出不覆盖、私有文件权限、错误不回显敏感marker。
- 固定source-v3 M0：exit0，7schema/61closed/16example/86negative/111trace/136edges/9docs/39links/2sources；只对应本固定基线，不代替live后继验收。
- integration-v1：exit0；真实未初始化/身份不符均非零拒绝，前后development/event/verifier/observation/review计数不变；成功目录与5角色真实source-events可读；同DB非空直接bin被拒绝且计数不变，reservations=0。
- integration-v2：exit1；生产器成功但父进程没配置删除journal，删除补验HTTP失败。没有把该轮计为完整通过。
- integration-v3：exit1；父日志位于祖先可组写的仓库路径，服务按真实safe_parent拒绝启动。本轮未跑目录，不伪装产品失败或生产器通过。
- integration-v4：exit0；父进程在安全0700独立临时目录配置真实journal，重做新隔离DB。删除role3 evidence的真实DELETE202后development404；role4保持accepted/200且source-events可读，target/evidence独立。temporary journal仅本次合成运行，已清理。

四轮各自精确label/ID私有PG清理均passed，随后独立inspect确认4组container和volume均不存在；父request文件含服务凭据，清理后已移除。报告保留公开身份和命令结果，不保留凭据。每个失败均终止当前attempt并清理；未在部分DB上重seed。integration-v4只在诊断并修正父环境后创建新DB，不收费。详细命令/退出/二进制/源码摘要及清理证据见review-delivery.json与各integration报告。

## 审查与下一依赖

已向P02短确认hook和roles/request接口，并说明父服务必须在空库初始化前配置 `ALIA_DELETION_JOURNAL`，其祖先路径不可组写（sticky tmp例外），日志父目录0700；不能因DELETE503放宽隐私门。等P02正式确认本片输出接线及完整浏览器候选。P02产品UI与浏览器用例保持其独占，本片没有重复80项或跑其完整浏览器。

下一项为Root+P04对归档固定源非作者review，以及P02组合候选的有记录真实浏览器。未运行生产trial验证器/20ABC/正式成长/真实模型/14日观察；工程synthetic观察不证明语义质量、独立生产measurement或人格成长。M4平台停止的协议组合始终not_run，无换名重派；本片没有SDK或M4执行。后续M34其他独立反例待Root接口冻结释放。本片状态为review_pending，整体M34仍未完成。

本轮派发Sol/high；运行指令标识gpt-6.1-sol，effort未由实际turn_context独立读取，派发不当作实测费用或模型切换证明。后继revision继续owner/reviewer、源码/依赖/接口版本、精确证据和delta；变更后重审，作者不得自批。
