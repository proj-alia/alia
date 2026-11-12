# P01-PRIVACY-REVIEW-01｜v2固定修复复验4/4通过；v1原失败保留

范围：独立只读审查 `accepted-slices/root-privacy-http-02` 与 `root-privacy-http-v2/source` 的当前单体在线删除、来源/结果未知、日志恢复和长连接边界。仅拥有新 `tests/privacy_boundary_regression.rs` 与本交接；不编辑生产源、原测试、阈值、其他worker或在变packet7，不迁移collector、不调用模型。EVAL-03的37文件与正式unverified结论保持。

## 固定输入与已有证据

已实际核对182个完整冻结源、接受包11源及既有报告SHA一致。完整源清单SHA `893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72`；接受清单SHA `95238a35d5187802be5cf860a3eb6d6c502627c14435a0539b2445244e34f7f7`；既有report SHA `aa945d96be12fb55cfd40c1a9cbad871b2256ff43e2d00dd6a8a6addc89f4cf4`。已有5 HTTP/13管线/9闭包共27 PG、10日志文件、3共享门单测及Clippy均为原报告的实际结果，本次不重复、不冒充新增覆盖。

静态核对：main先migrate→configure_deletion_journal→配置恢复→bind→模型/索引/worker。已有个体非创建式open_existing；日志缺失/损坏/错身份及旧v1不能恢复时关闭共享门。begin_memory_deletion先校验key/目标版本、同键回执、schema、真实来源闭包，再读取日志；durable_append之前已经fence_privacy，DB阻断与operation/event/outbox同事务，commit之后才ready/202。实际新测试只覆盖此前不同的异步请求取消，不重跑原fsync后故障组。purge/replay保持未知、blocked/retry_pending与completed分开，当前没有已开放的受管备份/导出入口；不能用未来产品缺失制造线上回归。

## 实际覆盖缺口与新增验证

通知旧测试 `tests/events.rs::blocked_sources_and_unknown_types_only_advance_cursor` 在读取batch前阻断，不能覆盖已排队而尚未发出的SSE。authenticate仅在handler返回Response时再次检查代次；后续流在service/events.rs:115出队后直接yield，notifications.rs:75起读取没有共享privacy_ready/generation检查。独立测试现已实际复现下述两个失败场景；归为同一SSE隐私边界缺口，不将两个场景重复计作两项独立漏洞。

新增三个有界测试：

1. 已建连且队列含memory.changed，实际DELETE返回202/blocked、普通GET404且新读取已隐藏目标之后，旧流不得再披露该memory UUID。允许关闭流或仅发送无资源ID游标，不要求撤回客户端已收到的字节。
2. 空闲已建连流，在真实日志fsync后以PG触发器使accept事务失败；新HTTP503、共享门false时，旧流应终止而不是继续轮询发送资源元数据。仅对隔离合成数据注入DB故障。
3. 用实际PG advisory barrier停在fsync后的operation INSERT，取消请求future；共享clone保持503，无成功回执；释放barrier后由实际replay恢复、同键completed一次、重启保持同operation、费用0。未通过任意改源码、任意同uid读取或伪造时钟构造失败。

执行副本只由固定182源加新测试构成183源，位于 `work/p01-privacy-review-01/source`；不会带入当前packet7。单父进程 `work/p01-privacy-review-01/run_boundary.py` 在全局 `/tmp/alia-project-heavy.lock` 下实际等待98.27秒，持锁覆盖编译、独立私有PG和精确cleanup。已完成下述实际验证，原182源及执行183源全量SHA保持。


## 实际发现：P1，既有SSE未随隐私门/来源代次失效

**触发与结果**：客户端已认证建立GET /events，收到connected后暂未消费下一条。后续删除或未知结果已经改变可见性，先前返回的Response绕过了handler结束时的检查；stream自身只检查shutdown，既不持有/复验privacy_generation，也不在逐条yield前复核来源。

| 场景 | 已实际成立的前置条件 | 实际结果 |
| --- | --- | --- |
| 旧队列晚到 | 合成memory.changed已在流内排队；实际DELETE返回202且status=blocked；随后GET同memory为404，新的notifications读取也已隐藏目标 | 新测试在第171行失败：旧stream下一帧仍含该已阻断memory UUID。不是回收客户端此前已收到的数据。 |
| 空闲流遇到结果未知 | 建连时队列为空；真实journal fsync后DB INSERT触发合成故障，DELETE503/privacy_unavailable、ready=false，普通GET也503 | 新测试在第205行失败：旧空闲流仍输出下一帧而未终止。该断言证明继续出帧；未单独导出其精确payload，不把结果扩张为正文泄漏。 |

固定代码定位：`root-privacy-http-v2/source/src/service/events.rs:100-124` 仅核shutdown、从旧queue直接序列化资源ID/版本；`:126-138` 在空闲后继续轮询。`src/adapters/notifications.rs:77-113` 直接pool.begin读取，没有Store共享privacy_ready/generation两端校验。`src/service.rs:216-255` 的HTTP门发生在Response生成期间，不能覆盖之后的流yield。完整源码SHA见本任务report，避免把定位套用到正在演进的packet7。

修复要求交root协调：逐次yield与实际轮询须遵守同一共享隐私门和代次，隐私失效后关闭旧流；恢复重新ready也不能让旧代次queue重新可见。已经发头不能改成503，关闭后让重连走当前鉴权/快照/来源过滤。若复用notifications作内部读取，也需明确相同边界，不能仅靠建立订阅时鉴权。具体生产实现由root维护，本审查没有修改生产源。

严重性与范围：这是当前已实现单体服务中来源元数据/流生命周期的隐私回归。第一项实际披露的是opaque resource UUID；静态序列化仅含kind/id/version，未发现/证明记忆正文或凭证泄漏。第一项在UUID断言先失败，后续正文断言未执行，不能引用该未执行断言作为正文保护通过。第二项仅确认门关闭后仍出帧。没有未授权访问、任意源码改写或同uid任意文件读取假设。

## 新增通过边界与实际命令

异步取消测试实际通过：PG advisory barrier确认请求停在真实fsync后的operation INSERT，取消request future无成功响应，所有共享HTTP访问503；释放barrier后maintenance真实重放并完成一次、同键回放completed，目标404；重启重新打开原journal仍同operation UUID，usage_reservations为0。此测试验证取消/恢复边界，不证明真实TCP断流一定取消Axum handler，也不代表所有取消时点已经覆盖。

| 命令（执行cwd为独立183源） | 实际终态 |
| --- | --- |
| `cargo test --locked --test privacy_boundary_regression -- --ignored --test-threads=1` | exit101，1 passed / 2 failed / 0 ignored；14.46秒含编译，测试4.00秒。失败均发生在预期隐私断言，不是编译、连接或超时失败。 |
| `cargo clippy --locked --test privacy_boundary_regression -- -D warnings` | exit0。未重复原27 PG/10日志或完整工程矩阵。 |
| `rustfmt --edition 2024 --check tests/privacy_boundary_regression.rs` | exit0。 |
| `PATH="$PWD/.venv/bin:$PATH" python3 scripts/check_phase0.py`（live根） | exit0，M0 PASS：6 schema、53 closed objects、13 examples、68 negative cases；只能代表当时规范检查，生产测试仍绑定固定v2。 |
| 父驱动及精确清理 | exit0表示审查harness完成，不表示回归通过；report明确all_new_regressions_passed=false。独立label/ID容器及volume cleanup=true，后续inspect再次确认两者不存在，私有state已移除password。 |

原始报告 `evals/results/p01-privacy-review01-v1/report.json` 保存真实命令、时长、queue、资源ID和cleanup；`boundary-pg.log`保留完整1过2败，`boundary-clippy.log`保留exit0；不改旧通过报告。执行测试SHA `94c79102c68082d7e6f208e93dfea2f5f81f9796a8d9f5ec498cb4a5f4642f6e`；183源manifest SHA `b33d276dfd04a8d7e67dd5b235db6315f3df7f52bf49503e729cc36cbd484d29`。所有交付SHA见 `review-ready.json` 与交付包SHA256SUMS。

EVAL-03交付37文件再次逐项核对保持，SHA256SUMS仍为 `61173ce74a98e172d1ee066170287d7e0a35fa340afedd4bd643ea4582a89211`；原正式semantic unverified/null结论不变，未迁移任何collector。

## 限制与下一依赖

本次只审查并验证固定在线版本的上述具体边界，未证明隐私安全完整。已有27 PG/10日志是原独立范围；packet7、UI长时间消费、真实TCP背压/断连、硬断电与fsync之后各个调度时点均未由这3例验证。当前未开放的受管导出/备份不列作已启用回归，也未以原PG夹具模拟旧数据库回退冒充真实备份产品验收。无收费调用、无模型、无部署、无自动付费重试。

下一项由root在自己的固定新切片修复SSE隐私门/代次与读取边界，再针对同一新测试运行；同时保留本次失败证据，按实际新源绑定结果。生产owner保持唯一，本审查不自行扩大修改域或重跑历史全套。M1—M5及十四真实日观察的整体状态不由本次局部审查改变。

## v2固定修复复验完成（以上为v1原结论，未改写失败）

root释放固定旧182源，仅叠加以下两源；未带packet7，未改原183源目录，未覆写v1报告或原READY交付。原3测试字节前缀SHA仍 `94c79102c68082d7e6f208e93dfea2f5f81f9796a8d9f5ec498cb4a5f4642f6e`，新增第4例显式跨过故障关闭→真实重放→ready恢复，期间不poll旧queue/idle流；恢复后旧流须结束，新订阅仍须读取独立可见来源。完整新test SHA `886d39cdc00c6191075f9b6126b7329acaa4bf8722ba626def79955add0aee7b`。

| root修复源 | 本次固定使用SHA256 |
| --- | --- |
| `src/service/events.rs` | `a9c077f09bbbd78d476ecdd8eb7749bff97e0c949a336de1acc1aee0eb8a0fc6` |
| `src/adapters/notifications.rs` | `576324433ab4cfac48afb80c75fe58403a7b820d8f88ca690f59c11705d2fb15` |

新183源在 `work/p01-privacy-review-02/source`，manifest SHA `9bbe94c185c5d80510e30ff86fe14a8626812f7c2953ceaa4096ee69aa5104d9`；独立新target `work/p01-privacy-review-02/target-private` 在运行前不存在，未复用共享target，禁用rustc wrapper。全局锁覆盖首次完整编译、独立PG与清理；实际v2终态如下。


| v2实际命令/核对 | 实际结果 |
| --- | --- |
| `cargo test --locked --test privacy_boundary_regression --no-run` | exit0，独立空target首次完整编译60.75秒；在此后才创建独立PG，避免编译期间占用DB。 |
| `cargo test --locked --test privacy_boundary_regression -- --ignored --test-threads=1` | exit0，4 passed / 0 failed / 0 ignored，2.46秒测试；原三个函数/断言的字节前缀完整保留。 |
| `cargo clippy --locked --test privacy_boundary_regression -- -D warnings` | exit0，26.21秒；同一自有target，不混用共享产物。 |
| 新test `rustfmt --edition 2024 --check` | exit0。 |
| live根M0 | exit0，6 schema、53 closed objects、13 examples、68 negative cases；不是用此项覆盖固定生产测试。 |
| 精确清理及清理后独立inspect | cleanup=true；本次独立label/ID的容器、volume均不存在；私有状态password已移除。 |
| 原182源、旧183执行源、新183执行源 | 三份完整manifest均未变。旧v1的1过2败报告、原READY交付及EVAL-03的37文件逐项SHA保留。 |

固定报告 `evals/results/p01-privacy-review01-v2/report.json` 保存完整命令/日志SHA/独立PG身份/清理及两处root实际SHA；`source-manifest.json`/`input-verification.json`给出精确180未变＋2替换＋新test。新编译唯一测试可执行文件SHA `60e0ad2fedd6318ea6a40269e3af37783a5c374ddfac5d6173ce5cc82a1dbe9e`，日志显示从 `work/p01-privacy-review-02/target-private/debug/deps/` 实际执行；不是共享target或旧rmeta/rlib的通过记录。v2测试首次执行即4过，没有改变原断言后重试。

第4例实证：两个旧流（有旧queue、空闲）均在失败关闭门期间完全不poll；真实replay使ready恢复且generation变更，再poll均EOF；新订阅可见独立新来源并不含已删除目标。它验证恢复不能复活旧流，同时不以永久关闭通知来取巧。原第1/2例现在同样通过，取消/重启幂等保持通过。结合固定两处生产修复，在本次定义的有界场景中原发现已修复；不宣称所有并发调度、TCP层断流/背压、全工程整合或隐私安全完整。

后续依赖变为root接受这两个精确修复源及新test，并在后继命名整合候选中验证。无需迁移collector或绑定packet7，未重复原27 PG/10文件全套；0模型/付费/部署，既定完整M5后集中行为验收与十四真实日观察要求保持。
