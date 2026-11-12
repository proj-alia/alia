# P01-CAPABILITY-REVIEW-PREP-01｜只读能力边界准备

本任务只读检查在途 `sandbox_skills`/`sandbox` 及P03报告，未修改代码/原测试/collector，不运行Docker、PG、模型、收费或全矩阵。P03尚在完成自己的冻结/READY；以下是待固定版本验证的可检验问题，不是已复现漏洞、安全通过或生产功能已开放。

## 取证边界与已有实际覆盖

本次查看的live源码SHA：`src/adapters/sandbox.rs` **a7c74643391f336acd8568e55bee9263a268571bd348aa3d22b702723c460868**；`src/adapters/sandbox_skills.rs` **f39695f8ab98337c059fce82263e88eb9de33fcaf84716de67227312106d7145**；对应 `tests/sandbox.rs` **e2bbcb3401a6cb67a26010a7f3c7c5939984201e405126c04b3f667a146e11c6**、`tests/sandbox_skills.rs` **59023d3600da2422d2cceab93594d366311668ff5efd0dc3c921e16d08318ba0**。源码在途，这只是采样时身份，不把以后变动认定为回归。

P03 v3报告 `evals/results/p03/skills-targeted-v3/report.json` SHA **ea214dbd2f651dfd9dedd5298175828cb6e038d9c5d59de00898d7e1c8a4a664**，实际build/普通/Clippy/M0、旧SDK9及新skills9阶段通过；v4报告 `evals/results/p03/skills-targeted-v4/report.json` SHA **7114ee5d23d1b0b7f0c048291f2fb4656c304ff4c134c9d4aa0cd15557994d6a**，实际新target构建、普通/严格检查及skills9阶段通过。v4没有重跑旧raw9；可引用v3旧raw结果的前提是实际SDK源与测试SHA一致（上述a7c746…/e2bbcb…在v3和v4均一致），不是跨版本自动通过。v4 source manifest SHA **3a6364e41078cd327634da76970625efeebac37c815ffd30cac2827671ebc65a**。

原实际测试包括固定镜像/前导、无网络和host挂载、资源/time/output限制、foreign cleanup保护、完整24KiB source×8KiB input、固定argv和环境不带私有输入；vault实际验证/注册/调用/新进程恢复、不同源/报告重绑定拒绝、在途修订/撤销/到期、错误JSON/额外日志/错误case、超时与清理不确定。cleanup=false的故障注入是预期负例，不能当验证成功；实际父控制器另清理其自有资源。v1编译缺libc与v2旧锁准入失败也保留，不能改写成实际隔离执行成功。上述结果都限于对应固定源及真实测试方法。

## 控制链与集成责任

`CandidateDraft`接受individual和来源UUID但vault自身不认证数据库归属、来源真实性或删除状态；SkillVersion绑定individual/skill/version/source SHA。受信任控制器的ValidationCases绑定原始input/expected与case digest；实际execution记录source/input SHA、固定protocol/wrapper、镜像、唯一执行ID、隔离/退出/限额/cleanup及输出。只有完整实际通过且恢复重验的ValidatedSkill可以由server签发非Deserialize的RegistrationGrant；grant绑版本、report/case/image/wrapper/scope/expiry；注册先持久化再回执。invoke前后复验当前版本、grant、epoch和到期；更改/撤销使旧在途结果不可用，已发生调用和不确定清理不得丢失或记零。`sandbox_calls_upper_bound`是调用前预留上界，不能代替实际结算。

root集成层仍须在验证、签发、注册、派发及结果提交的恰当事务边界绑定实际individual、来源图/隐私代次、权限、预算、配置/策略与可信case来源，并在在途变化后拒绝旧结果。候选不能自行写grant、伪造受信任controller/注册表或修改服务端文件；这些不作为模型攻击假设。vault本地哈希链不单独证明DB来源可见性，也不是抵抗同UID管理员整体回滚的外部单调锚。删除来源后的源文件、验证报告、审计内容及其受管备份/恢复处置须由删除与存储整合切片规定，不能只凭撤销调用权限宣称物理清理。

## 建议后续1—3组独立负例（尚未运行）

1. **候选可控输出冒充返回值**：候选源中包含合法顶层 `def run(x)`，但模块顶层或run内部用 `os.write(1, b'true\\n'); os._exit(0)` 直接给出与受信任expected相同的唯一严格JSON，绕过正常return路径。当前已测的“缺函数后提前退出”缺少合法def，此组合不同；固定wrapper先写隔离标记，再exec候选，需用实际完整validate/registration路径确认候选是否能使并未正常返回的case取得ValidatedSkill。控制组正常return同值；记录真实执行、退出、stdout、清理及最终phase。仅为可检验风险，尚无实际结果；若协议只承诺stdout而非run返回，需先由root明确验收语义，不把推测当已证漏洞。
2. **外部来源／权限在途失效**：用真实已登记来源及实际可见权限完成注册，在受控阻塞的实际验证/调用期间由正式删除或权限撤销路径改变来源/epoch，然后恢复/重启。检查旧结果不得进入可信注册、输出或后续模型上下文；真实已派发次数、未知费用和cleanup状态完整保留，不自动补偿重试。以独立有效来源/权限为正控。现有vault内revision/revoke/expiry测试不等于数据库来源删除或权限变化；在集成接口实际冻结之前标not_run。
3. **删除与受管产物闭包**：经实际来源创建并注册候选后，按真实删除操作撤销其来源；检查source、validation report、audit中的受保护内容及受管备份/恢复后的可见性、文件清理回执和未完成操作恢复；缺清理回执只能unknown/未完成，不能冒充成功。并列独立未受影响技能正控。当前没有可由本只读审查认证的完整DB/备份接入，不用直接改私有文件/伪造server grant模拟模型攻击。

若第1组得到意外结果，应保留实际独占资源、固定source/wrapper/image/case哈希、逐调用与stdout/exit/cleanup证据，报告“输出协议与声明的run返回语义”范围；不能将一个case误判扩大成容器逃逸或任意执行。后续只由root在P03固定READY及社会写入首轮固定报告后指定正式实测；此准备不改变M1—M5、既有100/60×3/20ABC和14真实日验收，不改变预算/隐私或释放收费trial。
