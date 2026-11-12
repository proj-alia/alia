# ALIA-P03-10 / SQLx 与真实 Docker 独立分派 READY

当前交付为v2，见`evals/results/p03/p03-10-sandbox-qa-ready-v2/ready.json`。v1报告/公开输出不改，原五源已按v1 READY的SHA精确归档在`evals/results/p03/p03-10-sandbox-qa-archive-v1/source`。交付前复核补正QA恢复失败的诊断保留：删除/身份重查失败仍记录已知实际ID和name，部分结果中的execution handles写入父报告；清理不能证实时保留私有恢复目录，不能只留下错误类型后删除恢复证据。无SDK/原断言或共享锁改动。

任务ID：P03-10。仅QA分派、对应离线验证与交接；0模型、不部署、不初始化生产、不改变总锁。本次不是N6全工程接受，也未重跑P03-09实际Docker矩阵。新runner真实路径是`scripts/qa/run_engineering.py`，原任务消息中的`evals/qa/run_engineering.py`不存在，因此没有建立第二套目录。

## 问题与改动

原`scripts/test_database.py`的`cargo test --locked --workspace --lib --tests -- --ignored`会同时启动`tests/sandbox.rs`的9个真实Docker测试；它们不依赖SQLx，不能计入PG通过，也不能因缺Docker准入而破坏数据库分派。

- 新`scripts/qa/sandbox_tests.py`从已经绑定的Rust源码建立ignored用例清单。SQLx保留workspace/lib/tests自动发现，只对9个固定Docker函数添加`--skip`。未知ignored域、SDK/测试声明不完整、缺原9项、错误镜像pin、过滤器与SQLx函数子串相撞，全部拒绝；不维护会漏掉新SQLx文件的目标白名单。字符串/raw string/嵌套注释被屏蔽后再识别属性。尚不支持的ignored条件声明拒绝，不能静默不测。
- 工程父报告逐名、多重集核对数据库日志中的实际ok函数，任何缺失/额外/ignored/failed/Docker混入均失败。普通ignored依然不是通过。固定候选的SDK或其测试任一声明，就增加独立必需`sandbox-tests`阶段；完整声明全部新管理UI时为27项，未声明SDK时旧21/26规则保持。原五UI、观察器、产物门禁源码与检查均保留。
- Docker子阶段只运行固定`cargo test --locked --test sandbox -- --ignored --nocapture`，9项必须逐名实际ok，目标必须是`tests/sandbox.rs`，单独summary须9 passed/0failed/0ignored/0measured。父runner还验证来源绑定的子报告、原始Cargo摘要与13份结果记录；只得到exit0不能完整通过。Docker报告明确`actual-Docker-not-SQLx`，其计数不加入数据库阶段。

## 固定准入、隔离与清理

镜像唯一pin：`python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72`。同时检查固定测试的IMAGE常量、本机缓存RepoDigest及每次结果的pin。缺镜像即失败，QA不pull、不改镜像、不自动改启动权限或降级；原native-unavailable用例要求当前非特权QA账户确实不能直接访问daemon，环境改变会失败而非跳过。

父runner持有既有`/tmp/alia-project-heavy.lock`，子阶段使用继承的真实fd嵌套加入；不拆锁、不只传布尔标志。子阶段持续持锁，覆盖源码/镜像准入、实际测试、精确清理、完整性复核与报告。Cargo显式继承fd、串行测试、独立target和报告目录，保留既有编译profile以复用本候选先前构建。移除DATABASE_URL及提供方凭据，只设置测试要求的合成环境污染正对照；Docker控制采用私有0700空配置目录、0600配置、本机固定socket和绝对`sudo -n docker`路径。

开始前必须成功查询到SDK role=oneshot命名空间为空；已有容器即失败且不触碰。正常完成时，对13个结果handle分别成功查询确切名字不存在。故障恢复只对本次空命名空间准入之后、精确UUID名字、自有execution/role标签、固定镜像和全长实际ID全部匹配的资源，在立即再次inspect一致后按实际ID删除。标签/身份改变、foreign control、查询失败均保留并报告恢复handle，不能猜测删除。发现任何恢复需求，即使清理成功，也不把本次执行改成通过。

子进程超时/中断/部分结果均失败，停止并有界回收Cargo组，再做身份限定恢复和进程回收。子阶段220秒控制预算，Cargo最长180秒，Docker控制每次最多15秒且受总deadline约束；父阶段260秒兜底。SIGTERM与KeyboardInterrupt进入失败/清理路径。原SDK另建的root Docker CLI进程组不由本QA盲目kill；容器清理使attach退出，不能证实进程回收时失败。SIGKILL、宿主/daemon故障仍不能保证立即清理，报告未成功或精确残留需协调者恢复；不宣称任意中断无残留。所有公开报告只含受限来源/结果/身份元数据，不导出Docker完整inspect、宿主挂载或凭据。

空命名空间归属推断的范围明确限于所有Alia重型QA遵守同一排他总锁、且P03 SDK尚无生产注册的当前环境。它不是脱离总锁、多独立SDK调用者同时运行时的归属协议；若未来放开并行，需先加入job级明确归属，不能直接复用此恢复推断。

## 实际验证与限制

1. `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s evals/p03 -p 'test_*.py' -v`：v1实际exit0，共118 tests（旧90＋新28）；v2加入删除/身份查询失败的精确ID保留及失败时私有目录/13handle保留两项，最终120（旧90＋新30）实际exit0。新项覆盖SQLx/Docker分域、全9原函数、镜像pin、遗漏/额外/未知ignored、子串误过滤、Rust字面量、实际日志计数、子报告来源/cleanup/type/结果清单、foreign owner/身份改变拒删、父runner mandatory/exit0不能代证、mocked完整成功/部分启动与中断失败。模拟Docker只验证分派和协议，绝不是新的内核隔离证据。
2. `PYTHONDONTWRITEBYTECODE=1 .venv/bin/python scripts/check_phase0.py`：实际exit0，M0 PASS（schemas6/examples12/negative63/traceability111/task_edges136）。不代表产品场景或十四天通过。
3. `evals/results/p03/p03-10-sandbox-qa-ready/compatibility-audit.json`：只读独立verify七份N1至N5R3原始checkpoint、对应报告及完整产物。新SQLx日志校验器保留旧数据库结果：N1/N2/N3/N4/N5R3真，N5/N5R2假，原失败不改。N4/N5/N5R2/N5R3的21项与N6-base的26项保持；N1—N3本身原18项报告未被升级为21项完整通过。旧观察器实际日志均兼容。未编译或重跑这些候选。
4. 同一只读审计校验P03-09 finalv6的真实9函数日志与13份实际结果，并确认SDK/原测试两SHA与该固定源一致。最初审计把Unavailable静态诊断stderr误判成用户执行输出，exit1；已按原SDK确切两条诊断纠正QA校验，不改原SDK、原断言或历史记录。最终9/13均通过新协议。这是重用已有真实证据，不是新分派入口实际运行。
5. 未改变P03-06 artifactchecker/observer20tests、P03-08 management helper/workspace24tests、业务SDK/原测试/Cargo/schema/权限/核心隐私接口；最终hash在READY给出。未运行新子阶段的真实Docker CLI、全N6的27阶段、任何新PG用例、新UI或新增观察器；这些仍须未来正式固定候选执行。没有把本次live源码只读计数当实际PG通过。

最终v2日志、原兼容性只读审计引用及5项自有源SHA位于`evals/results/p03/p03-10-sandbox-qa-ready-v2/`。v1 READY/旧候选/旧失败保持。

## 运行入口及下一依赖

未来正式候选必须同时纳入这5项自有源：`scripts/test_database.py`、`scripts/qa/run_engineering.py`、`scripts/qa/sandbox_tests.py`、`evals/p03/test_sandbox_dispatch.py`、本handoff；再由协调者统一冻结/接受。完整工程runner条件分派，无需新的第二命令体系。

如只独立验证已声明SDK的固定候选，仍应由一个持锁父任务记录参数和终态，使用：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 /fixed/source/scripts/qa/sandbox_tests.py \
  --checkpoint /fixed/checkpoint --source-root /fixed/execution/source \
  --target-dir /unique/report/build/default --output /unique/report/sandbox-tests
```

输出不可覆盖、build/output不能位于固定输入树。此入口会实际启动Docker测试，不应把上述示意路径直接执行。下一依赖：root接受QA源并组装全M5工程候选，完整工程后再集中模型/原行为门槛/14真实日历天。当前不新增收费试验。后继并发隔离审计属于单独验证任务，在本编码交付结束后顺序进行，不另开Goal。
