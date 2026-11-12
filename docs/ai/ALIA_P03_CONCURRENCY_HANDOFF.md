# ALIA-P03 / 当前总锁下并发隔离验证 READY

任务：在P03-10编码终态之后，顺序完成唯一验证跟进；不新开Goal、不改core/common/锁、不运行模型、部署或生产资源。此READY只表示以下实测范围的隔离证据已交付，不授权自行拆锁或宣布完整M5通过。

## 结论与范围

当前总锁可以支持一个持续持锁父任务内的多个并发验证子任务，无需修改锁。实际完成两份不可覆盖的N5R3快照、两个独立target的真实Alia二进制构建、两套PostgreSQL与实际Alia/Chromium页面的并发探针，以及卷创建后被SIGKILL的部分启动恢复。补充两个真实开放事务并发写入，排除把并发SQL读取当写入证明。两父任务均自然exit0，末尾与独立审计清理均通过。

这仍不是两个独立完整工程runner的并行证明。独立顶层任务依然被总锁串行化：第二探针真实等待94.517秒，原锁文件device38/inode1660460保持。测试中外部独立非阻塞flock竞争确实被拒绝。没有更改`check_isolation.py`、`common.py`、`database.py`、生产配置或锁协议。

## 原有证据审计

| 项目 | 原有覆盖 | 本次补证与仍未覆盖 |
| --- | --- | --- |
| 固定源 | freeze单测验证变动/覆盖/链接拒绝，旧候选精确校验 | 真实冻结两个312源快照，id和manifest不同；实际拒绝再次覆盖；两个执行副本全程与同一原N5R3精确相等。未取changing live |
| 构建目录 | 旧runner每次有独立target，但全矩阵顺序 | 两个真实`cargo build --locked --bin alia`进程与编译产物流同时推进；各631个Cargo产物路径全部在自己的target。未运行两个完整矩阵或并发npm安装 |
| PG资源/数据 | `check_isolation.py`先后provision、顺序SQL与顺序清理；两资源生命周期重叠不等于并发操作 | 两套容器/卷/owner/port实为不同，真实并行启动；并发浏览器初始化、PG读取与各自输出写入。另两套全新PG开放事务实际并发插入/提交，数据不串写，清理A不影响B |
| 服务/浏览器 | `test_web.py`同一资源的create/restore按顺序执行 | 两个新构建实际Alia进程、独立loopback随机端口和私有data dir、各自随机令牌、两个真实Chromium页面；不同持久individual_id，跨令牌401，持续轮询；没有完整UI/布局验收 |
| 结果/产物 | 不覆盖发布和来源绑定单测/旧全矩阵 | 两个实际独立输出树、结果与bin不同inode、同源可重复二进制hash一致、各自输出标记不串写。实际拒绝原路径覆盖，来源/产物末尾全部复核 |
| 中断/部分启动 | SQLx启动失败单测主要mock；旧真实隔离验证错误ID拒删与顺序清理 | 第三临时worker实际创建独占卷后在container run之前被SIGKILL，持久恢复state限定清理真实卷；另SIGKILL服务A并清理其DB时B页面/服务/数据仍成功；不覆盖任意daemon/宿主/内核故障 |

## 实际执行与证据

固定基线：`belief-context-errors-20261112-n5r3`，原manifest SHA `529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974`，原artifact manifest `f38fbb5a05913ab25b14d509f3ebd33274701ed675020cdd8db885f761bcebc3`。验证之前/之后源与完整旧产物均verify，不改变旧工程接受结论。使用该基线的已接受静态web作为只读共享输入，实际服务bin是本次两份新构建。

主探针命令：`PYTHONDONTWRITEBYTECODE=1 python3 -u work/p03-concurrency-v1/probe.py`，唯一PID2097822/tool session99994，exit0。UTC 2026-11-12T15:00:33.712287 → 15:01:49.357894，约75.646秒，锁等待0.000082秒。原始报告`evals/results/p03/concurrency-20261112-v1/report.json` SHA `0deb0f0d86d8f7ef76698456afd825536e47f8b0fb64a7f4ba831cf6127fca47`。

- 构建A UTC15:00:34.274027 → 15:01:40.352979，B 15:00:34.375727 → 15:01:39.952159。真实进程重叠65.576秒，两个Cargo编译产物流各自持续变化的窗口重叠64.674秒，不只依据同时启动时间。`CARGO_BUILD_JOBS=2`各自、debug0、incremental0、独立绝对target；实际命令/环境/PID/cwd/progress均在报告。
- 新快照manifest A `7d09811aff46b2db4fdb35fa7e200dc10f2a5429d0236186e7e5ca422ba3d258`，B `11b8cae32aca3cbd17b6c6e52615168dac61a99d73992e6da673d01d096eee28`；各312源全部只读。两个执行副本独立、构建前后哈希精确；产物均SHA `a65b4a0f741aa1b10d96fa31c06f4f6db49c919ecb3496202ab9972e77e40f49`，实际不同inode。
- 实际PG随机端口32872/32873，实际服务57645/48817。Browser A/B有不同id；已初始化后的共同浏览器/服务窗口UTC15:01:45.149 → 15:01:46.816，1.667秒；A/B分别15/27次真实状态轮询，两个浏览器正常完成并close。A服务受控SIGKILL=-9，B仍通过状态/身份/数据检查。
- 第三worker在真实卷创建完成、尚未run container时被SIGKILL=-9；用原有owner/name/label持久state精确清理，不猜资源名、不删除其他QA资源。原partial state与两套DB state在成功清理、撤除密码后删除私有运行目录；失败分支保留私有恢复目录，不擅自丢弃恢复信息。

补充命令：`PYTHONDONTWRITEBYTECODE=1 python3 -u work/p03-concurrency-v1/write_probe.py`，唯一PID2117149/tool session23513，exit0；UTC15:03:12.365733 → 15:04:55.103918，其中等待原总锁94.516913秒。原始报告`evals/results/p03/concurrency-write-20261112-v1/report.json` SHA `219c26d21598f419af77a67f7855c7ca792857f10ba79807e95f4b00a668ff71`。

另两套全新PG端口32877/32878，Thread barrier后分别执行`BEGIN; INSERT ...('a'/'b'); SELECT pg_sleep(2); COMMIT; SELECT ...`。每个成功命令和SQL均在报告；各自只读到自身值，清理A后B仍保留已提交的b。两个调用窗口重叠2.112446秒，总包络仅2.114131秒。两段真实服务器睡眠各至少2秒且都处于此包络内，因此实际服务器睡眠（从而开放事务）重叠下界为`4 - 2.114131 = 1.885869秒`；不是只凭两个父进程开始时间声称事务重叠。独立审计保存推导。

独立只读审计`evals/results/p03/p03-concurrency-ready-v1/independent-audit.json`实际exit0：4个已创建PG容器、5个已创建卷全部精确查询不存在；未创建的partial容器名字也确切查询不存在（共10个名字，不冒称10个实际创建资源）。6个已知build/service/browser PID不存在、2个服务端口关闭、2份新snapshot及执行源/产物、原N5R3完整源/产物、P03-10 READY五源和六项保留源全SHA不变；原总锁inode一致。所有fixture只有合成名字/UUID，提供方配置/凭据未继承，0模型调用、无生产初始化/部署。

## 失败、限制和下一项依赖

两轮实际探针和最终审计没有测试失败，未重跑旧矩阵。准备脚本时一次patch格式错误未产生文件改动，最终运行的是固定SHA记录的脚本。初始主探针的并发PG段主要读取；已通过独立新事务探针补足，未覆写原报告。

可并行资源隔离在上述条件下已得到实际正证据；仍不能据此保证任意branch/feature/candidate、模型SDK、所有fault point、并发npm或无限资源负载。Cargo全局cache/tool内部锁与旧web只读资产共享，测试未取消这些保护。当前全局锁继续保留，ROOT审阅后才决定锁边界；本窗口不实施拆分。

最小后继QA工具建议（供root决定，不在本验证任务实现）：将这次source/target/output/port/owner清单、双子任务barrier与两个受控中断点接入现有`check_isolation.py`的一个显式并发验证模式，保留当前排他父锁并拒绝覆盖。若将来要使两个独立完整runner并行，还须明确每候选的构建/产物所有权锁、DB/service资源锁与父候选锁的继承及清理协议，再以该真实模式验证；不能仅删除原flock。

并发验证本身只新增`work/p03-concurrency-v1/`验证脚本、对应results与本handoff/总交接，不修改业务或共享QA核心。独立审计时P03-10 v1源全部吻合；随后交付前复核对P03-10自有QA helper补了恢复失败的ID/handle/私有目录保留与两项离线反例，见其独立READY v2，不把该QA-only修复算成新的并发真实证据。v1五源精确归档，报告与原探针固定源不改。

最新root接受信号另已记录：root-history-03 manifest60138982a7a8dd452bfb121f890ef32742281e1ee7ded90093d6f2b27fba571a、root-privacy-stream-01，以及root-autobiography-write-v3的402源15阶段局部95ordinary/79actualPGHTTP。它们未混入本次N5R3基线，不是模型接受。共享target曾在源码未变时发生cargo check成功后ordinary链接新字段/variant不一致，原失败与v2/v3局部证据各自保留；本窗口不将其称为业务修复。后继正式候选必须独占target并绑定可执行SHA，不跨不可变输入共享本包rmeta/rlib；不因信号追加旧全矩阵或模型任务。

完整可审阅输出在`outputs/ALIA-P03-CONCURRENCY-READY`；含报告、只读审计、脚本和脱敏日志，不含bin/build、完整源码、镜像、私有state或凭据。不创建额外Goal。
