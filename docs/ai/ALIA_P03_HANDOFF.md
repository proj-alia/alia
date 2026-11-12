# ALIA-P03｜工程回归与验收流水线

- 窗口：01a1cf17-37ab-7d13-bd07-beb8600a1c02；主线唯一协调者：01a1cbe8-e270-75e1-9535-d3f2e0ea81c4 / local。
- Goal active（无 token_budget）：建立并验证冻结源码、独立构建产物、工程/行为两套测试 PostgreSQL 与工程回归流水线，交付固定候选首份完整报告。产品失败保留，不改生产代码消灭失败。
- 已完整读取根 AGENTS、三份权威文档、协调登记、P01/P02 handoff；P03 released 已登记，两个现有入口哈希与 transfer baseline 一致。旧总锁持续权威。不部署、不初始化生产、不改 contexts/生产/依赖/权威文档，不调用模型。
- 当前仅文档/只读检查；尚无本窗口测试通过声明。

## 统一驱动输入契约（供主线登记、P01适配）

- `--checkpoint <directory>` 指向 source/ + manifest.json；清单格式 `alia-p03-checkpoint/1`，id、sources（相对路径→SHA256）、created_at UTC。验证源码集合/哈希、无越界symlink；输出拒覆盖。
- `--binary-dir <absolute directory>` 包含 alia 和 examples/（或P01独立例子目录）；产物清单 `artifacts.json` 绑定 checkpoint manifest SHA256、实际命令、编译器版本与每文件哈希。任何测试先验证。
- `--web-dir <absolute directory>` 固定前端 dist，不能回退开发目录。
- `--test-db-state <private absolute JSON>` 工程/行为二选一，包含本任务创建的容器/卷身份、随机凭据及动态回环端口；报告只写 owner/id/用途/端口/镜像，无凭据。DATABASE_URL 只经环境注入。
- `--output <new directory>` 运行输出与source、产物及旧报告分开；命令/退出码/UTC/排队和运行秒数/日志/cleanup/通过失败ignored未验证分开记录。保留旧默认用法；显式 checkpoint 模式缺输入立即拒绝，不读 .env、不启动默认Compose、不编译开发checkout。
- P01保留自己的驱动/观察程序及行为判据；P03不修改它们。P02专属浏览器脚本复用时由环境 ALIA_TEST_URL/TOKEN/OUTPUT 注入。

## 稳定冻结请求

基础工具和离线故障检查完成后，请主线在协调文件登记短暂冻结源码稳定点（含P02完成保存），P03执行 allowlist 哈希→复制→全输入复验→发布 manifest，即恢复开发。首份报告只使用这一真实源码快照，不把现有13文件二进制清单当全源码快照。需要主线选定候选；本窗口先完成公共辅助和离线验证。

## 初始输入 SHA256

```json
{
  "AGENTS.md": "79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4",
  "docs/BLUEPRINT.md": "30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943",
  "docs/CONTRACTS.md": "837d5ff8dc2831d4ac5267014845f5962a5ad5331fee6b29c75dce9de9e16cbf",
  "docs/IMPLEMENTATION.md": "40ac2d40bef2a540a96cd9ce8e6f2a78472a45e39703151fe2ef9c1b5e3e7bf8",
  "docs/ai/ALIA_PARALLEL_COORDINATION.md": "53b2bf59aec9450f31906b3240334dba8f19ad46e8c9939715181912ae3484c0",
  "docs/ai/ALIA_P01_HANDOFF.md": "5cb74f1e4a7e222a5d57c57385b9c5dcad1502739f51a91e005690463f3ccec5",
  "docs/ai/ALIA_P02_HANDOFF.md": "7e5b55f582c1ae2ad9d6d98c2ceb39ac8cce5b3e3e5b7b7f2d93985d9708f9c1",
  "scripts/test_database.py": "717effdb337f4e54883e0702acccc130f3c523d92db44d9e499f26d26bce6ee3",
  "scripts/test_web.py": "5621bd15787c561c556cf1122814ff840ece4b2261a405b76aab843c2ebcc54a"
}
```

## 基础设施实现 / 离线验证（进行中）

- 已接收主线选定当前 segmented-author/1 候选（消息授权）。生产 model/reply_wire/cognition/grounding/schema/config 哈希仍与旧13文件manifest相符；P01的3个观察文件已经在其所有权下更新，旧二进制不再代表当前观察源码，首份新快照将记录真实新版本，不伪称同版。
- 已实现 scripts/qa/{common,freeze,database,artifacts,run_engineering}.py，并在获准两个入口增加显式 frozen 模式。没有修改生产/P01/P02文件。
- `.venv/bin/python -m unittest discover -s evals/p03 -p 'test_*.py' -v` 最终退出0：12项通过，含开发后改动隔离、复制中变动拒绝、损坏/越界manifest、额外文件/symlink、检查点/报告/产物拒覆盖、资源owner检查、清理失败保留私有恢复状态、未知用量不记零、ignored不计通过。首轮一项测试tearDown错误（对symlink chmod）已改为跳过链接并改用任务自建外部fixture，失败结果不写作通过。
- PostgreSQL真实启动/互不干扰与独立构建尚未执行。下一步补齐P02外部脚本适配、等待其READY稳定点后冻结；只通过旧总flock排队。
- 统一接口补充：源清单在checkpoint/manifest.json；产物另在results/artifacts/artifacts.json绑定manifest SHA，bin/alia与bin/examples/*、web/。为P01提供artifacts/p01-manifest.json兼容其当前id+sources+binaries，`--binary-dir artifacts/bin/examples --checkpoint artifacts/p01-manifest.json`。测试DB state存于0600 `/tmp/alia-p03-db-*/{engineering,behavior}.json`，parent0700，绝不在仓库/manifest保存凭据。
- runner有一个execution/source工作副本（与只读source逐项哈希相同），npm依赖在该副本web/node_modules以保留默认解析；Rust target与最终web/dist、二进制、日志均在独立输出目录。原始快照及开发checkout不被构建改写。

## READY：基础设施离线故障检查 / 等主线命名冻结候选

- 主线审阅的五项边界均已补齐：必需二进制+静态入口/完整产物集合覆盖及路径每层symlink拒绝；run→异常→清理→报告同一总锁生命周期；ProvisionError携带精确private state供父驱动finally清理；最终快照校验异常写入失败报告；P02外部浏览器父驱动已纳入必要阶段，任何必需未验证不可能full_engineering_pass。
- 产物负例包含主线复现的“unrelated唯一manifest项+未列bin/alia+空web”；另覆盖多余web资产、修改二进制、缺静态入口与父目录symlink，均拒绝。发布使用同目录fsync+原子link且O_EXCL，不覆盖旧证据。
- 最终 `.venv/bin/python -m unittest discover -s evals/p03 -p 'test_*.py'` 退出0，19项通过（0 ignored）；日志 `/tmp/alia-p03-offline-final-v1.log`。模拟部分startup失败后精确owned卷清理、清理期间持续持锁及末尾校验损坏仍写失败报告均通过。未把fixture当真实DB结果。
- 当前P03 READY，等待主线命名稳定候选后才冻结全源码/执行完整回归；P02最终Unicode修复后的READY尚待其交接。下一步可先在旧总锁下单独运行真实两库隔离验证，不会触发模型、生产或完整编译。
- 数据库随机密码由Docker `--env-file /dev/stdin`私有管道注入，绕过sudo环境过滤；没有密码参数/公开日志/仓库文件。

### READY脚本 SHA256

```json
{
  "scripts/test_database.py": "30ec41cc2456452d0f3a5d3a1c045fca4c95dc8f77acc97da488b1b717e2503a",
  "scripts/test_web.py": "484fbee3f6f6fd5ce0997cf7395bfa8ea78482c43f6a12a68cac17851c46b082",
  "scripts/qa/artifacts.py": "fa3a249b6844a59edee286ab4bca6a7918cddfa5619acc62c20199123d24f645",
  "scripts/qa/check_isolation.py": "3d1e506ef262e147350ccc2f95d40348cf071ec3df0690556a8150b86a16c16c",
  "scripts/qa/common.py": "7713aa2e005f363284c4be8e16ce8736ee9fe16194690c40102e52c699fb1bcc",
  "scripts/qa/database.py": "0e06f0581ed5e70b8fc150d145ea8f13f9a8c1f231014daa515a1e1102a5bfd5",
  "scripts/qa/freeze.py": "a7c47b3f33f160876be0c6a924c824d28bf8d606e24ac1b414fc587e0d4a7489",
  "scripts/qa/run_engineering.py": "7e22f701d1b6ce9585880569a0efff18fd556232f4894365cac5c4169386b54c",
  "scripts/qa/test_p02_browser.py": "866bc464289c7419b3297b9b16367a2e5f1dd78ed21a81162b832cde51f64954",
  "evals/p03/test_infrastructure.py": "c1f5ba979b6a1fa5e1ff3ac62db6d9432d3eff49d3591a7c9c90cec9a1727d2c"
}
```

## 真实两库隔离验证终稿 / READY v2

- v1实际退出1：两库数据隔离、错误container ID拒绝、行为清理不影响工程均通过；重复清理时Docker volume的`no such volume`小写错误未被识别。两套容器/卷均已实际清理。旧失败报告 `evals/results/p03/isolation-20261112-v1/report.json` 保留。
- 已修正为只匹配大小写无关`no such container|volume`；daemon连接错误仍失败。新增专门负例；离线20项通过，退出0，日志 `/tmp/alia-p03-offline-final-v2.log`。
- 真实v2：passed=True，四组隔离/身份拒绝/清理隔离/重复清理，独立两套固定pgvector与随机回环端口，cleanup逐资源记录，model_calls=0；报告 `evals/results/p03/isolation-20261112-v2/report.json`，SHA256 `d9237b1e04df5ec645a598aa5301f03af47268bfd9af4f45048ccde42532fe48`。开始2026-11-12T09:44:38.825637+00:00，结束2026-11-12T09:44:45.102807+00:00，运行6.277秒，排队0.000030秒。
- 当前基础设施READY v2。P02已写最终READY且不再改动；P03等待主线命名候选后执行冻结/完整回归，总锁未拆。
```json
{
  "scripts/qa/database.py": "74cd4aefbed8ec8578a77bf15c00aa9096bbd4678dd94975892f5ed2884c0aa9",
  "scripts/qa/artifacts.py": "fa3a249b6844a59edee286ab4bca6a7918cddfa5619acc62c20199123d24f645",
  "scripts/qa/run_engineering.py": "7e22f701d1b6ce9585880569a0efff18fd556232f4894365cac5c4169386b54c",
  "scripts/qa/test_p02_browser.py": "866bc464289c7419b3297b9b16367a2e5f1dd78ed21a81162b832cde51f64954",
  "evals/p03/test_infrastructure.py": "fed917de326220dbefceb44c8fbf04ec9ed541e1972f962abc049493ba7079fe"
}
```

### READY v3 / 发布后校验正向集成

- 主线指出的兼容清单顺序已在READY v2前的实现修正：先生成p01-manifest.json，再计算整个产物集合SHA并发布artifacts.json（无循环引用）。补了真实调用 `Run.publish_artifacts`→`verify_artifacts` 正向round-trip：包含完整必需二进制、web/index.html和兼容清单；随后篡改兼容清单必须拒绝。
- 最新离线21项通过/0 failed/0 ignored，命令退出0；日志 `/tmp/alia-p03-offline-final-v3.log`。真实两库v2已终态成功且两套容器/卷清理成功，私有状态密码已撤除。P03 READY，等待P01新接口READY和主线命名冻结。
fa3a249b6844a59edee286ab4bca6a7918cddfa5619acc62c20199123d24f645  scripts/qa/artifacts.py
7e22f701d1b6ce9585880569a0efff18fd556232f4894365cac5c4169386b54c  scripts/qa/run_engineering.py
2ec989ef04b2304873d15a37413f3b1a9fe6987a03ef296410a31e7e898e4412  evals/p03/test_infrastructure.py

### READY v4 / 默认调用与未运行矩阵

- 最终22项离线故障测试通过，日志 `/tmp/alia-p03-offline-final-v4.log`，退出0。补了外层既有flock CLI传入真实fd时的嵌套锁兼容，避免旧调用外层持锁、内层重新打开锁而自锁；仍只认文件inode与有效fd，不信布尔标记或锁文件存在。
- 必需阶段列表显式固定，基础设施中止后未达到的阶段逐一记unverified并给下一责任依赖；full_engineering_pass需要全部必需阶段实际通过及最终源码校验、cleanup通过。
- allowlist补齐requirements-budget.txt；docs设计包历史results.json也排除，运行报告仍不进入快照。未启动完整冻结/编译；READY状态保持，等主线命名。
444a8f11cc79e7ed6eafe29dd792415a1e195760f452ab8d630b67bf130daa8b  scripts/qa/common.py
10660d3db0f88e0874a2fc4a8cb4161058c2ac86cac5739105b7ac48af58deb2  scripts/qa/freeze.py
fb441920ad0070a93defed957041e56b36138cf63254dc684cc88aff9720ac4a  scripts/qa/run_engineering.py
a5327393b7bd32743ebd54fab6ddcdd1f58cdabdb76af5eae0ba40767d7711f0  evals/p03/test_infrastructure.py

### READY v5 / 私有DB连接绑定

只读集成复核发现：私有state若意外改错port，单独检查container标签不能保证host URL仍指向这个容器。现统一在url生成前核对容器实际loopback绑定、端口、ID及已记录image ID；state format和固定测试owner用户名也严格校验，防止测试连接与owned资源脱钩。新增错误port/缺format反例，最终23项离线通过（退出0），日志 `/tmp/alia-p03-offline-final-v5.log`。不新增运行或变更任何生产资源；P03继续READY等待正式freeze。真实隔离v2报告仍为上一工具版本的明确实测，不伪称已跑新全套。
d8f317cef88fca3e1650e88f3d4830fdb490890626e3a5a084b50e1d196dcd4e  scripts/qa/database.py
0b637f3bd33b3ff953ce50c59b1af3dc7eceff63d9b9657453dd7d44d1a9f6bb  evals/p03/test_infrastructure.py

## N1条件释放已收到

已读取主线正式命名：checkpoint `evals/results/p03/checkpoints/segmented-console-20261112-n1`；results `evals/results/p03/engineering-segmented-console-20261112-n1`。P03将在P01明确接口READY并核对保存后，直接短暂冻结→校验→一次完整工程矩阵，无需第二次确认。主线allowlist停写；本handoff先记录接收，冻结期间P03也不再写allowlist。复制失败保留证据不自动另起。P02 READY已接收；不收费、不部署、不拆锁。

N1冻结前最终路径检查：runner已保留`.venv/bin/python`调用路径（不resolve该解释器symlink，否则会落到缺jsonschema的系统Python）。新增虚拟环境路径正例；离线24项通过，退出0，日志 `/tmp/alia-p03-offline-final-v6.log`。P01最终READY与P02最终READY已读，接下来核对SHA并冻结；本条之后复制期间不再改allowlist。

## N1冻结成功 / 完整工程父任务已启动

- P01/P02最终16项源码/证据SHA与其READY逐项一致。冻结278个允许清单文件，复制前→复制后全开发源→快照内集合与SHA全部相符，退出0；原始contexts01/02逐字不改，运行/私有数据不入快照。
- checkpoint：`evals/results/p03/checkpoints/segmented-console-20261112-n1`；manifest SHA256 `1ff8cdc45323598f4e8e63755fec53148d628b67a311b8f867f4f4a3e8c6633a`。
- 只读source与manifest已发布并全量verify成功；短暂冻结完成，主线现在可恢复协调文件登记的唯一下一核心切片。开发后续变化不影响N1。
- 唯一完整父任务工具session：`87653`（仍在运行，不能另起重复任务）；用冻结source中的runner，`.venv/bin/python -B -u <checkpoint>/source/scripts/qa/run_engineering.py --snapshot <checkpoint> --output /workapps/project-alia/alia/evals/results/p03/engineering-segmented-console-20261112-n1 --python /workapps/project-alia/alia/.venv/bin/python --node-bin /tmp/alia-go-diagnostic/node-v24.11.1-linux-x64/bin --chromium /home/zero/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome`。
- 输出：`evals/results/p03/engineering-segmented-console-20261112-n1`；产物将是`artifacts/{artifacts.json,bin/,web/}`；最终`report.json/report.md`，日志`logs/`。现在M0/语料/P01离线/P03离线/fmt均实际退出0，Rust独立构建进行中；未完成项不宣称通过。父进程总锁持续持有，无模型调用。
- 最终P03离线版本为24项通过（不是早期23），新增虚拟环境解释器symlink保留测试；已包含于N1源码。

N1实际父进程PID `1147713`；当前Rust构建子进程PID `1147785`（以session87653继续收取，PID后续阶段会自然变化）。只监测同一父任务，不重启、不另起完整矩阵。

N1中间证据（同一session87653，非另跑）：默认Rust构建/普通测试/观察单测/严格Clippy、npm ci/前端build/test均退出0；普通63 passed、177 ignored（DB未混计），观察4 passed（两个example各执行两个纯观察单测）。实际发布后 `verify_artifacts` 正向校验通过，13个产物文件覆盖全部必需bin/examples/web及P01兼容清单，未覆盖旧产物。feature阶段进行中；DB/浏览器尚未完成，不能据此宣称完整报告通过。

N1隔离实证补充：主线开发目录`tests/persistence.rs`及handoff已变化，但N1只读snapshot再次全量verify通过（退出0）；工程运行source保持N1哈希，不读取后续开发代码。feature普通测试与严格Clippy均退出0，真实独立SQLx/HTTP正执行，当前仍session87653。

N1可复核限制：DB入口内部clean_env未继承runner的CARGO_PROFILE_* / CARGO_BUILD_JOBS，故DB测试在相同隔离target中使用Cargo默认debug配置，会产生一次不必要的重新编译；没有切换源码或覆盖固定公开bin。已通过只白名单读取自身cargo子进程环境记录 `database-effective-build-env.json`，无DB URL/凭据。该实际构建配置与前面公开服务产物的debug=0不同，最终报告应明确；N1不可中途改源码/重启，下一轮辅助脚本可修复转发。RUST_TEST_THREADS=1与隔离CARGO_TARGET_DIR/CARGO_INCREMENTAL=0已实际确认。

## N1当前失败责任 / 不改冻结代码

- SQLx/真实HTTP完整阶段已实际退出0：177 passed、0 failed、0 ignored（普通cargo此前177 ignored已在此实际执行，不混算）。
- `browser`实际退出1：create在冻结`web/scripts/smoke.mjs:135`的 `page.getByText('网络已断开',{exact:true}).waitFor()` 产生strict mode violation；banner与main-content各有同文案，2个匹配。失败属于旧浏览器断言定位器歧义，不能据此断言实际离线产品功能失败；restore未到达，保持未验证。create服务SIGTERM退出0，自建DB删除成功；截图/driver-report/log均保留，未重跑。
- 文件不在P03所有权。建议主线精确整合补丁（下一候选，不改N1）：把上述等待限定 `page.getByRole('banner').getByText('网络已断开',{exact:true}).waitFor()`；保留断网后时间不变/恢复及其他断言，不能改成忽略错误。复现入口为N1原完整report中的browser命令，需新的独占DB与全新输出；本窗口不自行启动第二轮。
- 同一session87653仍在P02专属浏览器阶段，最终report尚未写出；P02结果和最终cleanup/源校验待终态。

## N1首份完整工程报告已交付（固定失败候选）

- 唯一父任务session87653/PID1147713自然终态退出1，未启动第二个完整矩阵。开始2026-11-12T09:51:52.169772Z，结束09:58:08.705207Z；各命令实际运行合计369.410秒，墙钟376.535秒，排队0.000097秒。
- 原报告 `evals/results/p03/engineering-segmented-console-20261112-n1/report.json` SHA256 `134d64062d3506cb5b953c35f2574704c6dfc6f3af5a35a3567d6f5a44e5ec7b`；清单 `artifacts/artifacts.json` SHA256 `375497a2c51b43c5a765ce8d74f2c0d12334bb9f2476f780823c4bb9c89e4514`。manifest仍`1ff8cdc4…8c6633a`，278源文件。
- 全矩阵结果：M0/语料/fmt/默认和feature严格Clippy/前端构建测试通过；默认63 passed/177 ignored，feature64 passed/177 ignored；177个独立真实PostgreSQL/HTTP全部passed/0 ignored；P01观察单测4 passed、离线38 passed，P03基础设施故障24 passed；P02专属浏览器12组全部通过、20实际请求、每列表/详情最大并发1。
- `full_engineering_pass=false`：旧browser create精确定位歧义失败；restore未验证。不是生产代码失败结论；服务正常退出、自建DB已删。原源码、报告、失败截图均保留。
- 工程与行为两套随机独立测试PG均cleanup passed，末尾分别inspect确认本任务container/volume实际不存在；原始source、execution副本、公开产物再校验全通过。0收费模型调用。所有日志/UTC/退出码/复现命令/责任模块在原report和 `audit-supplement.json`；中文摘要`summary.zh.md`。
- P01行为只引用其旧五例固定报告（11次54,207token）与独立评审的unverified，不自行代判、不称N1完整生命周期通过。双周期机器/排队/重复试跑/返工及主线分类耗时unknown记录`delivery-cycle-metrics.json`；范围不同，不给效率倍数。
- 已接收主线扩展下一测试切片：精确banner断线定位（待协调基线登记）与Cargo profile转发。只改下一开发版，不改N1或另启完整重跑；主线内部privacy单测/修复不重复。旧总锁继续权威。P03 Goal保持active直至这项新授权补丁/离线验证交接。

## 下一测试切片实施中（保持N1不变）

- 已核对scripts/test_database.py的N1基线`30ec41cc…2503a`，仅转发固定白名单CARGO_BUILD_JOBS/CARGO_PROFILE_DEV_DEBUG/CARGO_PROFILE_TEST_DEBUG，复用同版已编译测试；仍移除供应商凭据并显式绑定隔离target/DB、RUST_TEST_THREADS=1。
- 新增入口级subprocess fixture检查实际传出环境：上述profile保留、测试DB/target正确、供应商secret不继承。最新25项离线基础设施测试通过，退出0，日志 `/tmp/alia-p03-next-profile-v1.log`。属于下一开发版，不冒称N1已运行该修复。
- smoke定位修正等待协调文件扩展的baseline登记（主线消息已授权，按文件登记边界执行），只需banner范围，无.first()、无放宽断线/恢复断言。未启动新编译/DB/浏览器全套。

## READY — N1交付终稿与下一测试补丁

- 已读取协调文件正式扩展，smoke修改前基线8cd69aa…6c8a完全匹配。只将断线“网络已断开”与恢复“服务已连接”的等待限定到banner；未使用.first()、未删断线前后最后同步时间比较、未降低恢复要求，未改生产或其他断言。
- `node --check web/scripts/smoke.mjs`（已知Node24绝对路径）实际退出0；profile入口改动的25项离线测试已实际通过。新定位器尚未在下一固定候选真实浏览器重跑，明确待N2；没有擅自冻结N2、重编译/重复全套或收费。
- N1父任务已终态退出1，原报告及artifact/source全部保留只读；最终统计与已清理资源见上文和audit-supplement。下一依赖为主线选择N2、冻结和安排工程/行为；总锁尚未拆，P01收费未释放。
- P03本次objective交付已达成：可重复冻结/独立产物/两套PG/完整可核查首报告及必要故障用例，明确旧浏览器失败与未验证restore、无生产修正。附加授权的测试工具修正也已保存并局部验证。建议本首Goal complete，后续回归由主线重新分配。

### 下一补丁最终SHA256
```json
{
  "web/scripts/smoke.mjs": "1117dd3df8342f3c3102c3ab23be36c0c80dc0f62ff6d022d3f2decf79b35f98",
  "scripts/test_database.py": "0925f6fef5393dea0b86f35c0c5415483db78416d7414af198a478bab53d0027",
  "evals/p03/test_infrastructure.py": "224025919306b089d94b3454c00d1808856e1d94ab7130588bfd28eeed9b9cdf"
}
```


N1视觉复核：已目视查看本次冻结产物的P02 traces-1440/390实际截图；桌面主从区与手机分区可见，手机表格保留组件内横向滚动，没有把长页当页面横向溢出。未额外宣称像素级或OS后台标签验证。用户可读公开证据副本输出到 `/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N1/`，仅报告/红acted日志/哈希清单，无二进制、原始source、私有DB状态或凭据；原项目证据仍为权威且不改。

已接收启动窗口关于P02-02新概览切片的转达：新开发不替换N1，不立即触发完整重跑，不由P03登记领域接口。当前工程工具READY，仍由主线命名下一候选/调度，P01付费尚未释放；四Goal分工与旧总锁保持。

## 首Goal终态

本窗口首Goal已标记complete：冻结/独立产物/双测试PG/必要故障路径及固定N1首份完整可核查报告均已实际完成并交主线；complete指工程交付objective，不指N1 full_engineering_pass或M1—M5/行为通过。N1保留旧浏览器失败、restore未验证，未更改生产以消灭失败。附加两个测试修复已READY/局部验证；新候选/后续回归由主线继续分配，不擅自冻结N2、不拆锁、不部署或收费。窗口可继续接收主线后续任务。

## P03-02 / N2 active：assembly独立核验与冻结成功

- 新Goal已建立active，无budget；保持原目标/来源权限隐私预算事务约束。已读取根AGENTS/BLUEPRINT，并以已完整读取的N1内容逐行差异核对CONTRACTS/IMPLEMENTATION和最新协调登记，未用旧聊天猜新接口。
- 独立验证assembly SHA256 `8947595541f0e627a35d7a8f6a2d2d13bb3b87a5553a260904e3c3a1446c0450`，N1 manifest绑定、精确278源文件集合、11条overlay旧/新SHA、其余267文件逐项等于N1；无symlink/越界/额外文件，未READY overview新模块/交接不在候选。预检`evals/results/p03/n2-assembly-verification.json`。
- 从assembly/source冻结（没有从live checkout冻结）；N2 checkpoint `/workapps/project-alia/alia/evals/results/p03/checkpoints/segmented-console-privacy-20261112-n2`，manifest SHA256 `54e4eb000d95f90d36ae92f3d91762da7be797a44876f326d5c1829e254b96e4`。复制前后assembly源码/快照一致，freeze与verify均退出0。N1源、产物、失败报告未改。
- 接下来唯一完整矩阵至`evals/results/p03/engineering-segmented-console-privacy-20261112-n2`，显式.venv/Node24/Chromium、全程总锁、独立target/测试资源，无模型/部署；Goal保持active直至实际报告和具体失败交接。

N2唯一完整父任务已启动：tool session `3671`，PID `1208628`，用N2冻结source内runner。M0/语料/P01离线/P03离线/fmt已实际退出0，独立Rust构建开始；其他阶段未验证。继续收取同一session，不重启；已有总锁全程持有。

已收到启动窗口扩展为6Goal（P04记忆纠正/P05资料目录/4开发+P01行为+P03工程），仍主线唯一接口裁决；新源码不混入N2，不因窗口增加拆锁/盲开DB或模型。产品缺陷给责任开发线精确证据，工具问题在本所有权直接修；不降低断言。

P03-02/N2中间实际证据（同一session3671）：默认63 passed/181 ignored，feature64 passed/181 ignored，观察4 passed；默认与feature严格Clippy、前端构建/测试均退出0。P01离线38、P03离线25通过。181 ignored尚不计DB通过，当前正在真实SQLx/HTTP阶段。

已只白名单读取本任务Cargo子进程（PID1229650）环境，实际CARGO_BUILD_JOBS=4、CARGO_PROFILE_DEV_DEBUG=0、CARGO_PROFILE_TEST_DEBUG=0、CARGO_INCREMENTAL=0、RUST_TEST_THREADS=1，target明确N2/build/default；记录`database-effective-build-env.json`无URL/密码/令牌。证明profile转发修复实际生效；耗时结论等最终日志，不推算效率倍数。

N2实际SQLx/HTTP阶段退出0：181 passed/0 failed/0 ignored（含新增4项内部来源屏障）；数据库日志cargo `Finished test profile [unoptimized] in 0.14s`，无Compiling行，已重用同版既有测试二进制。当前通用浏览器create/restore执行中，同一session3671，尚不宣称完整通过。

## P03-02 / N2实际完整报告通过（终稿）

- 唯一父任务session3671/PID1208628自然终态exit0；18个必需阶段全部实际退出0，没有未运行必需阶段，没有重复完整矩阵。运行UTC 2026-11-12T10:09:28.227399+00:00 → 2026-11-12T10:15:20.678576+00:00，墙钟352.451秒，命令运行合计345.604秒，排队0.000104秒。
- 原report `evals/results/p03/engineering-segmented-console-privacy-20261112-n2/report.json`，SHA256 `dd47593ac74dbb2b84826627636bb6182837a84e335dced73e50ba2661d32b06`，full_engineering_pass=true；artifact manifest SHA256 `83d06783401d687cc2837f829ca84e39086934dac9f52a261f8c3b5c2136a975`；source manifest `54e4eb000d95f90d36ae92f3d91762da7be797a44876f326d5c1829e254b96e4`。
- 默认普通63 passed/181 ignored，feature普通64 passed/181 ignored；观察单测4次执行通过；P01离线38、P03离线25通过；M0/语料/fmt/默认与feature严格Clippy/前端构建和测试均通过。
- 181实际SQLx/HTTP/PostgreSQL passed/0 failed/0 ignored（普通63项filtered），包含主线新增四项来源屏障回归。DB阶段实际profile/jobs保持0/4，cargo完成0.14秒且无Compiling行，不再无谓重编。
- 通用浏览器create与restore均exit0，两个服务正常SIGTERM/exit0，实际DB已删；P02专属12组passed、20实际请求、列表/详情各最大并发1，browser与parent cleanup均通过。N1精确定位器缺陷已在N2完整验证修复，不改N1失败结论。
- 最终再次verify source、execution与13项完整产物通过；assembly278文件/11overlay和原assembly哈希仍相符；N1原report SHA仍134d6406…5ec7b。两套测试container/volume逐一inspect确认已不存在，密码已从其私有状态撤除；无生产或付费模型操作。
- 工程失败open=[]，没有待本窗口修复的具体N2工程失败。真实模型行为仍未验证，明确引用P01旧独立unverified报告而不代判；本结果不包含未READY概览/记忆纠正/资料新模块、不宣称M1—M5/完整删除/十四天观察通过。
- 原完整命令/UTC/日志在report；assembly-binding、audit-supplement、delivery-cycle-metrics、中文summary均在同一results目录。两周期分类开发/测试杂务/返工耗时unknown保持，不推效率倍数。
- P01可用固定目录（仅主线另行释放行为job后）：`/workapps/project-alia/alia/evals/results/p03/engineering-segmented-console-privacy-20261112-n2/artifacts/bin`，checkpoint `/workapps/project-alia/alia/evals/results/p03/checkpoints/segmented-console-privacy-20261112-n2`，静态资源`/workapps/project-alia/alia/evals/results/p03/engineering-segmented-console-privacy-20261112-n2/artifacts/web`。行为DB需按同一N2 helper新建fresh owned role=behavior，当前验证用实例已清理。总锁不拆；后续READY变更须主线命名新assembly，不覆盖本候选。

N2用户可读公开证据副本已保存`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N2`（报告/已脱敏日志/清单与哈希；不含二进制、私有DB state/凭据或原始source）。源项目固定证据仍为权威。前端静态树逐文件SHA与N1完全一致，沿用已目视的1440/390布局证据；N2通用浏览器另实际通过13路由/六宽度/深色与create/restore断线恢复，不冒称新OS后台标签试验。

P03-02 Goal已标记complete：实际N2完整报告/源产物独立核验/双库清理及N1两项具体工程工具修复的全矩阵验证均已完成，工程失败open=[]，没有仅凭计划或局部测试关闭。此终态仅针对N2工程objective；N1仍failed、N2真实模型行为未验证、M1—M5总体Goal与新开发线不由此完成。后续工程回归接主线下一明确命名候选，仍不擅自收费/部署/拆锁。

## P03-03 / N3 active：独立核验与冻结成功

- 新Goal active（无budget），N2后AGENTS/BLUEPRINT未变；已读取权威文档与协调差异，主线belief领域/存储slice已READY，运行时信念应用及语义审查仍未实现。
- assembly SHA256 `261c7921fe297c798bb2f4a0bb5b604cdee6f8e38b957393f3d79fb75956f487`；逐项核验282精确文件集、10overlay旧新哈希、4个new base_sha=null且在N2实际不存在，其余文件与N2逐项一致。最小adapters/domain注册严格等于base单行追加，API唯一改21→22且0022迁移同入；无未READY管理模块/路由污染。
- 从assembly/source唯一freeze，checkpoint `/workapps/project-alia/alia/evals/results/p03/checkpoints/belief-storage-20261112-n3`，manifest SHA256 `22003de0618727be042ec4d157936fb099b67d438e7e6ebc59525535010d2b3e`；复制前后assembly和snapshot全哈希一致，verify退出0，预检`evals/results/p03/n3-assembly-verification.json`。N1/N2未改。
- 接下来一次完整矩阵到`evals/results/p03/engineering-belief-storage-20261112-n3`，使用冻结runner和显式工具链，总锁全程、独立target/双PG，0模型/不部署；不等待其他新功能READY。

N3唯一完整父任务已启动：tool session `74893`，PID `1291966`，运行冻结source内runner，M0/语料/P01离线/P03离线/fmt实际exit0，独立Rust构建进行。源模型/行为driver仍N2，belief仅内部受信任接口；主线后续live权威文档不追改assembly。跟踪本session，禁止重复父任务；旧总锁保持。

N3同一session74893进度：默认普通Rust68 passed/185 ignored（新增5项belief领域用例已实际执行）、观察4次执行passed，默认严格Clippy和前端build/test均exit0。185个DB ignored仍未计通过，feature阶段进行中；独立库与浏览器随后执行。没有更改N3源码或接入live新功能。

N3同一父任务阶段证据：feature69 passed/185 ignored、严格feature Clippy exit0；随后185项实际PostgreSQL（含HTTP）全部passed、0 failed、0 ignored（68普通项filtered），包含4项belief新存储回归。DB日志cargo finish0.23秒，无重新编译；进程白名单环境已保存`database-effective-build-env.json`，profile=0/jobs4/串行与N3 target确认。当前通用浏览器执行中，整体报告尚未终态，不提前判全通过。

## P03-03 / N3完整工程通过（终稿）

- session74893/PID1291966自然终态exit0，18必需阶段全部实际exit0，无重复完整矩阵；UTC 2026-11-12T10:36:06.023752+00:00 → 2026-11-12T10:42:15.425657+00:00，墙钟369.402秒，命令合计362.475秒，排队0.000098秒。
- report `evals/results/p03/engineering-belief-storage-20261112-n3/report.json` SHA256 `ca875e3e66afeec93b8c535c6a73286b17e57b981907cf10f08f9264c0b1b5a7`，full_engineering_pass=true；artifact manifest SHA256 `3413c5da271c9c396ce3446bf17df4262d1747c553ceddbfc5a143a39d750991`；source manifest `22003de0618727be042ec4d157936fb099b67d438e7e6ebc59525535010d2b3e`；assembly `261c7921…956f487`。
- 普通68 passed/185 ignored，feature69 passed/185 ignored；包含5个新belief领域单测。185实际PostgreSQL（含HTTP）passed/0 failed/0 ignored（普通68 filtered），其中4项新belief存储回归。观察4次执行passed；P01/P03离线38/25 passed；M0/语料/fmt/默认和feature严格Clippy/前端build/test全部passed。
- 通用浏览器create/restore均exit0、服务graceful/exit0，P02专属12组passed、child与parent cleanup均通过。DB复用编译产物finish0.23秒、无Compiling行，profile0/jobs4/串行与隔离target均实际确认。
- 末尾source、execution、13项完整产物verify通过；assembly仍为原282源/10overlay/4新absent绑定；N1/N2原report SHA再次确认不变。工程/行为两个测试container/volume逐项inspect均不存在，私有state密码撤除。0付费调用，无生产改动/部署。
- 工程失败open=[]，无需生产或工具修复来消灭本轮结果；不另跑。belief只证明领域/存储约束，未证明模型/HTTP写入、实际认知或语义正确，非完整M2/删除/备份/十四天验收。
- N2 P01真实生命周期engine_pass=true，12 settled/已知用量84,609 token；独立review的behavior_pass及case_behavior_pass原值均null（validation_errors=[]），精确引用其文件/SHA见audit-supplement，不由P03转判N3行为通过。
- 报告目录另含assembly-binding/verification、audit-supplement、delivery-cycle-metrics及中文summary。N2/N3两个交付周期记录实际机器/排队/试跑次数和返工状态；主线分类开发/测试杂务/返工耗时unknown，不声称效率倍数。
- 固定artifact供后续主线决定：`/workapps/project-alia/alia/evals/results/p03/engineering-belief-storage-20261112-n3/artifacts/bin`；checkpoint `/workapps/project-alia/alia/evals/results/p03/checkpoints/belief-storage-20261112-n3`；web `/workapps/project-alia/alia/evals/results/p03/engineering-belief-storage-20261112-n3/artifacts/web`。真实行为若需运行，必须另行释放named job及fresh behavior资源；不重用已清理状态。新核心或管理功能需新的命名assembly，当前总锁不拆。

N3用户公开报告副本已保存`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N3`，仅报告、脱敏日志、清单/哈希，不含二进制、私有资源state/凭据或原始source。静态Web资源SHA与N2一致；沿用已目视1440/390布局证据，N3自身真实浏览器create/restore和P02均实际重新执行通过。固定项目证据仍为权威。

P03-03 Goal已标记complete：N3独立assembly核验、唯一冻结/完整矩阵、末尾源产物校验、精确资源清理和固定报告交接均实际完成；engineering_failures_open=[]，本轮无需要后续修复的具体工程失败。完成的是N3工程objective，不是belief认知/语义、N2行为独立结论或M1—M5总体目标。N1失败和N2已通过报告均原样保留；后续接主线下一明确命名候选，不擅自重跑、收费、部署或拆锁。

## P03-04 active / READY：已接受管理切片接入工程矩阵

- 新Goal active（无budget）；读取N3后权威文档/协调变化，AGENTS/BLUEPRINT/CONTRACTS未变。原N1/N2/N3 source/artifact/report不改，不使用live Settings/Memory/后端版本替代已接受来源。
- 独立逐项验证accepted-slices/p02-02（9文件，manifest7199cedc…ff16fc）、p04-01（3文件，b98460dc…6acf8d）、p05-01（9文件，b05c2694…15695）来源集合/源SHA与原清单完全一致，21源文件无symlink。原始manifest字节镜像在evals/p03/accepted-slices，固定主线公布SHA；N4候选每个源文件都要精确相符，不能只信包名或live交接。
- 完整matrix从18变21必需阶段：新增management-inputs（编译前固定来源检查）、overview-browser、documents-browser。缺输入/阶段缺失/unverified/重复stage/child或cleanup失败都不能full_engineering_pass；早停后未到达阶段逐项unverified。
- 新共用QA父驱动scripts/qa/management_browser.py仅启动既有冻结服务/静态树及已接受browser子脚本。每case随机fresh独占trial DB+进程+私有data/output，工程role及container/volume身份验证；按P02/P05各自真实环境契约注入，密码不在SQL argv，移除provider/部署凭据，不读.env、不回退Compose/target/debug、不编译live。
- 全父锁一直覆盖输入/资源/child退出/服务/DB/临时目录清理/最终report；child负责浏览器。CREATE回执丢失仍清理精确尝试名；服务清理失败不跳过DB，独立记录各清理结果。失败/超时/最终source或artifact不符保留失败报告，旧输出拒覆盖。
- child原报告/输出先在自建0700私有临时目录，日志和JSON脱敏后公开；若出现凭据，屏蔽截图导出。必须验证实际binary/source hash、对应task、至少9概览或8资料不重复检查组、child passed及cleanup=true；exit0单独不算通过。没有改任何P02/P04/P05脚本/断言。
- P04-01 tests/memory_revisions.rs及两backend文件为输入门禁必需项；现有全库命令cargo test --locked --workspace --lib --tests -- --ignored自动收集新HTTP/PG测试，补离线断言防止变回硬编码--test名单遗漏。

### 实际离线验证

- `.venv/bin/python -m unittest discover -s evals/p03 -p 'test_*.py' -v`：46 tests通过（原25+新增21），0 failed/0 ignored；日志`/tmp/alia-p03-management-offline-v2.log`。所有Popen/SQL/DB/服务和浏览器均fixture；覆盖固定来源/后续live替换/缺P04用例/损坏manifest/显式前缀/敏感环境剔除/child协议与部分输出/超时/启动关闭和CREATE回执故障/cleanup失败/最终校验/拒覆盖及三新必需阶段不能跳过。
- 两个已接受JS分别用已有Node24绝对路径执行`node --check`，均退出0；未启动真实浏览器、服务、PG或模型，没有运行新完整矩阵。
- 新门禁对原N3只读source的实际预检退出1（预期）：N3无新accepted-manifest输入，明确拒绝以旧候选冒充21阶段验收；N3原18阶段已通过结论不改。
- 一次只读git status因仓库dubious ownership退出128，未改global config、未用Git猜全量基线；本次以N3/accepted manifest SHA核对。

### READY / 下一依赖

P03 QA源码READY，以下8个文件已保存。本Goal保持active，等待主线从N3+三个accepted包+本READY QA变更发布命名N4 assembly及精确绑定，再唯一freeze/完整矩阵。ROOT-BELIEF-02与新的管理开发不自动进入N4。不要求用户选择，不在此另跑全套、收费或部署，原总锁仍权威。

```json
{
  "scripts/qa/run_engineering.py": "721f7e68cc389e8ae06bd94b7bd9f20fdd96066cff25700f9809039aefe5451c",
  "scripts/qa/management_browser.py": "eae7bf5eeae9664720f645463e6a553c1c8ed3978d133c92e84ac822b64ab51e",
  "evals/p03/test_management.py": "7963dcf95f5d8dc53e256c29ed30472f1afa162b531d4df1b1ca4f16812c5189",
  "evals/p03/test_infrastructure.py": "ee3c98f3fa4dc42a695a7a56f9899883f6f12aaf7183529f4d9bf2fbc372e322",
  "evals/p03/README.md": "bff22133666f126da9f789ea5992b162a0139c126f978c9fb23795c0a91a3869",
  "evals/p03/accepted-slices/p02-02.json": "7199cedcaa00f2b7e12354a21963233cb816aeb2adf00c73f516ef427bff16fc",
  "evals/p03/accepted-slices/p04-01.json": "b98460dc7d90490ef2eb76e81adb0d09b5c96d7c1c655a3c5920f45b1b6acf8d",
  "evals/p03/accepted-slices/p05-01.json": "b05c26944db0c9a6a95481770b74a2a330090250213299320ac4e2fc57115695"
}
```

P03-04续轮核验：8项READY源码SHA均未变；离线最终核验session18797已实际自然终态exit0，46 tests/OK，日志`/tmp/alia-p03-management-offline-final-exit.log`；overview/documents两条node --check独立实际exit0。审计`evals/results/p03/p03-04-ready-audit-v1.json`。主线当前任务经wait_threads确认仍active，但协调文件未发布N4 assembly精确binding且N4目录不存在，完整矩阵明确尚未执行。下一依赖仍仅主线发布受控N4，不能取live源码代替，也不重跑旧候选。本Goal保持active，不把READY或离线测试当作整个Objective完成。

## P03-04阻塞审计 / 等主线N4发布

已连续三Goal轮确认同一外部依赖：协调文件仅有planned N4，assembly目录及精确binding仍缺失；明确禁止据live源码自行freeze/full run。8项READY源码和N1/N2/N3原报告SHA均本轮再次核验不变，46项离线实际exit0及两脚本syntax exit0已完成，未发现其他可独立修复的QA缺口。当前没有在途P03测试（session18797已终态0），主线其他core仍active不等于N4发布。审计`evals/results/p03/p03-04-blocked-audit.json`。按三轮阻塞阈值将Goal标记blocked（不是complete/paused）；objective与21阶段标准不缩减。主线发布明确N4 assembly/source/hash和唯一完整matrix释放后可恢复本Goal，仍不收费/部署/修改旧候选，不需要用户选择或转达。

## P03-04恢复 / N4发布后独立核验与冻结成功

- 主线已正式发布并释放N4，先前唯一外部阻塞已解除，恢复本原Objective工作，不创建替代Goal、不改变21阶段标准。Goal工具仍显示blocked且无resume接口（用户/系统管理）；不擅自改状态或缩小目标，实际按明确恢复指令继续至完整报告。
- assembly SHA256 `caf5379eaedadbd1404f085ee0c3139a3a6ab9e7f54228dfe697a63a986e0a24`；N3 base绑定、308精确源集合/全部哈希、36overlay、26个new base=null且base实际absent均独立通过。三个接受包21源及3handoff、8项P03 READY哈希与主线绑定一致，其余N3源不变。
- 三个注册/helper文件严格按N3纯字符串派生核对一致，只加入overview/documents/memory路由及key/version sibling可见性；没有从live替换，71项后来改动明确排除。
- 从assembly/source唯一freeze至`/workapps/project-alia/alia/evals/results/p03/checkpoints/management-console-20261112-n4`，manifest SHA256 `3b49fcaf5d261e6123e1c7e4e85da5ca3fbf495894f540dafc291a237a6c3202`。复制前后/冻结源verify均exit0；实际新management-inputs门禁也正向通过，来源计数9/3/9。预检`evals/results/p03/n4-assembly-verification.json`。
- 现在可启动主线释放的唯一21-stage矩阵至`evals/results/p03/engineering-management-console-20261112-n4`，只用冻结runner与显式工具链、总锁/独立资源；0模型、不部署、不初始化生产、不改旧候选。

N4唯一21阶段完整父任务已启动：tool session `94768`，PID `1451320`，从N4冻结source内runner执行；management-inputs/M0/语料/P01与P03离线/fmt实际exit0，独立Rust构建进行。目标results `evals/results/p03/engineering-management-console-20261112-n4`。总锁全父生命周期；跟踪同一session到terminal，不能因观察超时重启或取live源码。

N4同一session94768中间证据：固定management-inputs实际9/3/9源门禁通过；默认构建/普通Rust/观察/严格Clippy/前端build/test均exit0，冻结P01离线38与P03离线46实际passed。feature进行中，SQLx及4组浏览器（通用/P02数据/概览/资料）尚未到达，不提前宣称完整通过。

N4同一session94768阶段进度：默认普通68 passed/202 ignored，feature69 passed/202 ignored；观察4次passed；默认/feature严格Clippy和前端build/test均exit0，当前实际202项SQLx/HTTP执行。只白名单读取本任务Cargo进程PID1471905确认profile0/jobs4/CARGO_INCREMENTAL0/RUST_TEST_THREADS1与N4独立target，记录`database-effective-build-env.json`无URL/凭据。202 ignored在数据库阶段终态前不算通过；21必需阶段中4组浏览器仍待执行。

## P03-04 / N4唯一完整21阶段通过（终稿）

- session94768/PID1451320自然终态exit0，21个必需阶段全部实际exit0，无缺失/skip/unverified阶段，无重复完整矩阵。UTC 2026-11-12T11:32:15.086599+00:00 → 2026-11-12T11:38:57.403484+00:00，墙钟402.317秒，命令合计395.106秒，排队0.000101秒。
- report `evals/results/p03/engineering-management-console-20261112-n4/report.json` SHA256 `89cdf971566d43b538ceec6bcdb35425d260179da4bdacaa012dfd7d5b0704a1`，full_engineering_pass=true；artifact manifest SHA256 `8df7a10eef84a083f39d8dbf01868a2fe998706f838dfeb3a9f79934345a0ce9`；source manifest `3b49fcaf5d261e6123e1c7e4e85da5ca3fbf495894f540dafc291a237a6c3202`；assembly `caf5379e…86e0a24`。
- management-inputs实际9/3/9接受来源门禁通过。默认普通68 passed/202 ignored，feature69 passed/202 ignored；观察4次passed，P01/P03离线38/46 passed。M0/语料/fmt/默认和feature严格Clippy/前端build/test全passed。
- 202真实PG（含HTTP）全部passed/0 failed/0 ignored（普通68 filtered）。另外逐名核对accepted测试：overview5、memory_revisions8、documents_view4的所有case在真实stdout均ok，各目标实际Running，证据`accepted-test-execution.json`；不是只看到文件或ordinary ignored便称通过。
- 通用create/restore各exit0且服务graceful；P02数据12组passed；新增overview9组、documents8组passed，子协议/binary/source绑定/cleanup/final_integrity均true。两case的独占随机trial DB及服务/0700私有运行目录分别清理成功，child输出脱敏后公开，未用live或default Compose。
- 末尾source、execution和完整13项产物verify、新management门禁再次通过；assembly308源/36overlay仍原哈希，71后续live路径不入。N1/N2/N3源码/产物完整verify及原report SHA再次核验保持。两套测试container/volume逐项inspect不存在，private state=cleaned/password已撤除；无生产动作或付费调用。
- 已目视本轮overview1440/390及documents1440/390（含正文片段）截图：全量统计/UTC范围、来源元数据、Unicode/原样HTML文本与内滚动可见；自动360/390/1440无page横溢/键盘检查亦实际passed。截图为合成fixture，不是私人资料。
- engineering_failures_open=[]，无本轮需修复的实际工程缺陷；N1失败、等待N4的blocked审计历史均保留。先前唯一外部阻塞已由正式发布解除，原完整Objective实际达到，不缩小至READY或离线子集。
- 同目录有assembly-binding/verification、audit-supplement、delivery-cycle-metrics、accepted-test-execution及summary。N3/N4两周期范围18→21阶段、185→202 DB与25→46基础设施用例不同，记录真实机器/排队/试跑次数，分类开发/测试杂务/返工时间unknown，不推效率倍数。
- P01 N2真实行为原报告及独立判断只按精确文件SHA引用，N4没有真实模型结论。ROOT-BELIEF-02/03及后续管理功能排除，不宣称总体M1—M5、完整删除/备份/十四天通过。
- 固定输入供主线后续独立排队：checkpoint `/workapps/project-alia/alia/evals/results/p03/checkpoints/management-console-20261112-n4`，bin `/workapps/project-alia/alia/evals/results/p03/engineering-management-console-20261112-n4/artifacts/bin`，web `/workapps/project-alia/alia/evals/results/p03/engineering-management-console-20261112-n4/artifacts/web`。任何收费/部署/新候选由主线明确安排；当前总锁仍不拆。

N4用户公开报告副本已保存`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N4`，50项逐文件SHA核验及bundle-hashes.json，仅报告/脱敏日志/固定清单/已目视合成截图；不含二进制、build/execution原始source、私有DB状态/凭据或供应商原始日志。项目固定证据仍为权威，旧输出未覆盖。

P03-04 Goal已实际标记complete：固定accepted来源核验、QA READY/46离线故障测试、主线N4精确绑定、唯一完整21-stage矩阵、末尾源产物校验、独占资源清理和50项公开证据交接均已完成；engineering_failures_open=[]。Goal工具累计记录1457秒（约24分17秒；包含先前等待阻塞前记录，不将其当作完整墙钟），本次矩阵实际402.317秒。此次完成仅限N4工程Objective；不宣称N4真实模型行为、后续live模块或M1—M5总体通过。后续依赖主线明确命名新候选或单独授权行为任务，不自行收费、部署、重跑或拆锁。

## P03-05 active / N5独立核验与唯一冻结通过

- 主线已正式释放belief-decisions-20261112-n5。独立核验assembly SHA 7104055475ecaa386020052e01c8883f111115f295455f6e62a92eae63d320fc；N4 base清单和全部源通过；311精确源/19overlay/3个new base=null且实际absent，root-belief-02及root-belief-03-final原manifest/全部源/handoff逐项通过。按两层顺序重建精确源map，再叠加3份固定权威文档，与N5全表完全一致。
- 全部非overlay N4源、管理21源及注册/helper原样；62后续live变化不入。验证证据evals/results/p03/n5-assembly-verification.json。
- 唯一checkpoint /workapps/project-alia/alia/evals/results/p03/checkpoints/belief-decisions-20261112-n5；manifest SHA256 dad2bc205776904f0bb8899729fc728fc807ae8e918b8120faffbaa6990a2824，freeze/verify实际exit0。将从该冻结runner启动唯一21-stage完整矩阵，固定output engineering-belief-decisions-20261112-n5；总锁/独立资源、0模型、不部署、旧候选不改。

N5唯一完整父任务启动：PID 1532513，将exec同一进程进入冻结runner；source manifest dad2bc205776904f0bb8899729fc728fc807ae8e918b8120faffbaa6990a2824，固定results /workapps/project-alia/alia/evals/results/p03/engineering-belief-decisions-20261112-n5。记录对应tool session后只跟踪同一handle至terminal，不因观察超时重启。

N5唯一父任务tool session10316/PID1532513已实际启动，冻结输入门禁、M0、语料、P01/P03离线均exit0；只跟踪本session，完整结论待21阶段与清理终态。固定assembly-binding及独立verification已附results。

N5同一session10316进度：management-inputs/M0（5schema/49closed/10example/56negative）及P01/P03离线38/46、fmt、独立Rust build exit0；普通Rust75 passed/210 ignored，观察4次passed。默认ignored数据库未算通过；严格Clippy/feature/真实DB/浏览器继续同父任务，当前不判完整通过。

N5同一session10316：feature普通76 passed/210 ignored、默认/feature严格Clippy、前端build/test均实际exit0。当前真实210项SQLx/HTTP执行；仅白名单读取本任务Cargo PID1553525确认N5独立target、profile dev/test0、jobs4、incremental0、串行，记录database-effective-build-env.json，不包含URL/凭据。真实DB与浏览器仍未终态，不把ignored算通过。

### P03-05 / N5真实DB失败即时交接（父任务仍继续浏览器/清理）

同一session10316的database-tests实际exit101。tests/contact.rs::new_input_and_deleted_ancestors_cancel_stale_drafts在固定源第50行prepare_decision().unwrap()收到Storage(StaleState)。Cargo截至contact累计{'passed': 59, 'failed': 1, 'ignored': 0, 'measured': 0, 'filtered_out': 60}，其后targets未运行，绝不能记210实际DB通过。新belief_context4、belief_decisions4及beliefs4的实际DB均已ok，但不抵消contact失败。证据logs/database-tests.log、database-failure-audit.json。完整父任务仍在浏览器，结论full pass必false；不重跑N5、不修改冻结或他人源，主线须定位来源屏障与旧contact fixture的语义后另命名候选。

N5失败位置文字校正：第50行是draft helper对`model_step(...).await.unwrap()`，不是prepare_decision。仅凭现有无backtrace日志不能判定是test中第一次或第二次draft调用，也不能提前认定应修改fixture还是生产来源屏障；精确失败类型Storage(StaleState)及冻结SHA不变，由root定位，不弱化隐私复验来过测。

N5末尾P03工具审计：最初freeze子命令未设置PYTHONDONTWRITEBYTECODE，在主线assembly/source旁生成唯一common.cpython-313.pyc缓存（不在源allowlist）。311份源和assembly清单无一改变，checkpoint/execution/产物校验均通过；独立exact-file审计发现后，仅删除本次生成的精确缓存及空目录并恢复311精确集合，证据assembly-cache-cleanup-audit.json。没有重冻/重跑/修改权威文件；之后所有冻结helper以禁写bytecode调用。该可修正工具痕迹不掩盖真实contact失败。

## P03-05 / N5唯一完整矩阵失败（终稿）

- session10316/PID1532513自然终态exit1；21必需阶段20 passed，database-tests failed/exit101，full_engineering_pass=false，无重复父任务。UTC 2026-11-12T11:54:36.313709+00:00 → 2026-11-12T11:59:33.780261+00:00，时间戳间隔297.467秒，runner记录290.039秒，两个计时口径分别保留。
- report SHA256 8e1e3b91bd5bae049239918e2ac9dfff3b6a362d1554d9db59b9be03347846b0；artifacts SHA256 b186e236f5c7b52c0839349e112670ca41a8982c69bf59888c9ed23458082c7d；source manifest dad2bc205776904f0bb8899729fc728fc807ae8e918b8120faffbaa6990a2824；assembly 7104055475ecaa386020052e01c8883f111115f295455f6e62a92eae63d320fc。
- 真实DB59 passed/1 failed/0 ignored，剩余150未执行。失败tests/contact.rs::new_input_and_deleted_ancestors_cancel_stale_drafts，第50行model_step await unwrap Storage(StaleState)，非QA启动或资源故障。根因未定、不先判fixture/生产修复；新belief_context4/belief_decisions4/beliefs4均实际ok，后续overview/memory/documents/grounding/scheduling DB未到达。逐名证据accepted-test-execution.json，不用N4补判。
- 普通75/feature76 passed，各210 ignored；观察4，P01/P03离线38/46，M0/fmt/默认和feature严格Clippy/前端build/test均exit0。通用create/restore、P02数据12组、概览9组及资料8组真实浏览器passed；parent/child/provenance/cleanup/final_integrity均true。已目视本轮1440/390合成概览/资料正文/转义HTML，未据UI判断模型学习。
- 最终source/execution/全部13产物、新management输入和assembly311精确集合复核通过；N1—N4源/产物及原报告SHA重新核验保持。工程/行为容器及卷逐项inspect不存在，私有state cleaned/password撤除，trial服务/DB/0700目录已清理，0模型/不部署。自生成bytecode精确清理审计保留，不改变失败或原源。
- engineering_failures_open=[contact stale-state failure]。主线下一依赖：定位并修复实际contact故障，保留来源/隐私保障，发布新固定命名候选后才有下一完整工程。不能在N5重复全矩阵、复用已清理state或提前记210通过。固定artifact仍可供主线诊断，但不是accepted engineering candidate；N4仍是最近工程通过版本。N4/N5范围及提前停止不可用于效率倍数或token质量优化结论。

N5失败工程交付的用户公开证据在`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N5`，55项原样copy SHA逐项通过，加bundle-hashes.json；仅报告/脱敏日志/清单/已目视合成截图，不含二进制、原source、私有状态/凭据或供应商原始输出。失败59/1/150未执行及next-dependency明确保留。P03-05执行与证据交付Objective已达到，不等于N5验收通过或contact缺陷关闭；下一P03-06仅新观察器兼容QA代码和免费离线测试，不掩盖N5失败、不授权另一完整矩阵或付费任务。

## P03-06 / 第九观察器条件发布 READY

仅改自有artifacts校验、runner必要发布名单与新增20项offline文件。已验证checkpoint精确源码key决定固定第九观察器；旧八必需，未知bin拒绝，digest/精确文件集/path/symlink与兼容清单hash覆盖保持。build --locked --bins --examples/所有其他Run methods AST原样，未改阶段/配置/锁。
- 全P03免费offline66 tests实际exit0，M0实际exit0；new20 + old46。首轮1项fixture注入错误exit1日志保留，解除自身临时fixture只读后注入损坏，不改保护来过测。
- 新校验器实际只读验证N1—N5源/全部13产物均兼容，所有原report SHA未变；N1/N5原失败仍失败，N5 contact工程缺陷仍open。未编译真实新观察器、未起新full matrix/DB/model/deploy；需主线接收P01observer/driver和本READY+fixed核心后另命名/释放。
- 专项交接`docs/ai/ALIA_P03_OBSERVER_PACKAGING_HANDOFF.md`，READY证据`evals/results/p03/p03-06-ready/ready.json` SHA 8f6a6322967d5abb5dc921bb87d8c5d1fdb70de7bca88344ae7f721b34005cac；公开副本`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-06`。
```json
{
  "scripts/qa/artifacts.py": "8b79f3dff9b60d54567a5ba59496659383aaa16488c5a2fea8d80f837ff754ad",
  "scripts/qa/run_engineering.py": "93802afc2aa1fb3bfa578d1616df655b8460a7986b92580bc1b9a518772752cf",
  "evals/p03/test_observer_artifacts.py": "cc54c42082626ac44f819c6235cd8b71b46c02c05bcd65c4a19d231682cbed61",
  "docs/ai/ALIA_P03_OBSERVER_PACKAGING_HANDOFF.md": "09321069e98d18789321c02c8ba34a211171662e98d3974705b625ddf2d0d73a"
}
```

P03-06 Goal已实际complete：限定QA源码/20新增离线反例/全66用例及M0实际exit0、N1—N5新validator只读兼容、READY4源SHA及公开证据交接均完成，Goal累计364秒（约6分4秒）。前P03-05执行交付Goal累计715秒（约11分55秒），对应N5工程仍failed/具体contact缺陷open；不能把任务交付完成等同验收通过。后续主线接收固定observer/driver和修复核心并另行命名/释放前，不起新全矩阵或收费trial。

## P03-07 active / N5r2独立核验和唯一冻结通过

- 正式release后新Goal active。authority差异已读，blueprint不变。assembly SHA 3eaf96f715de90afc625afe471508d0bb7973716a184bf94a1296aa5c0f2bf5e；N5 base完整verify，312精确源/5overlay/新增tests/context_privacy.rs base=null且absent，两份accepted源/原manifest/handoff逐项通过。其余N5源全部原样，contact SHA c3dc66e513c17b49d904eda69918278ee2aa0b87a9202a14a2da8168d1b04972；reserve/lease/model/schema/front/register等均无变。
- N5原QA helper固定哈希保留，无P03-06新校验器/测试和第九观察器，79后续live变更排除。证据n5r2-assembly-verification.json。
- 唯一checkpoint /workapps/project-alia/alia/evals/results/p03/checkpoints/belief-context-privacy-20261112-n5r2，manifest SHA256 2da04226c46e1fd47443e3b066d3e289b8cc9781eb37b0c22654c5a1e060d43c，冻结和verify实际exit0；freeze/helper均禁写bytecode，冻结前后stage精确312文件保持，无缓存新增。接下来唯一21-stage全矩阵，全部DB新执行、不复用N5 partial59；总锁/独立资源/0模型/不部署。

N5r2唯一完整父任务启动：PID 1635621，exec同一进程至冻结runner，manifest2da04226c46e1fd47443e3b066d3e289b8cc9781eb37b0c22654c5a1e060d43c。固定results /workapps/project-alia/alia/evals/results/p03/engineering-belief-context-privacy-20261112-n5r2，只跟踪对应tool session至terminal，不因观察超时重启。

N5r2唯一父任务tool session39037/PID1635621已实际启动。输入门禁/M0/语料/P01/P03离线/fmt均exit0，独立Rust build进行；冻结仍为原46项P03基础设施，不混入P03-06新20项。旧N5失败不改。

N5r2同一session39037：普通75 passed/213 ignored，观察4次passed，默认严格Clippy/前端build/test均exit0；feature执行中。独立源diff确认仅3项可选投影查询采用递归VISIBLE/owner过滤后limit，所有required source/lease等文件原样；213 ignored暂不算真实DB通过，原contact8/新privacy3及后续目标待实际全执行。

N5r2同一session39037：feature76 passed/213 ignored、feature严格Clippy exit0，当前213项真实SQLx/HTTP实际执行。Cargo PID1656424仅白名单确认profile0/jobs4/incremental0/串行与N5r2独立target，database-effective-build-env.json无URL/凭据。所有DB和后续浏览器仍待完整终态。

### P03-07 / N5r2新数据库失败即时交接（父任务继续）

- 同一session39037 database-tests实际exit101；tests/persistence.rs::context_drops_blocked_evidence_and_rejects_blocked_trigger固定第856行期望model_context返回Err(StoreError::Invalid(_))的matches断言false。断言没有打印实际返回值，不能先判返回成功还是其他error variant。需要root诊断，P03不改断言/来源规则。
- 真实数据库截至停止146 passed/1 failed/0 ignored，66未执行。原contact8与新增privacy3已逐名ok，新belief4+4+4及overview5/memory_revisions8/documents_view4/grounding9等均已实际运行。后续目标未到达不补判。证据logs/database-tests.log/database-failure-audit.json；N5原失败仍保留，N5r2不能full pass。
- 父任务继续通用/P02数据/概览/资料浏览器及精确清理；不能因本失败擅自再跑完整矩阵或混入P03-06/helper/live。

## P03-07 / N5r2唯一21阶段执行失败终稿

- 唯一session39037/PID1635621自然终态exit1，21必需20passed，database-tests failed/101，full_engineering_pass=false，不重启/不重跑。UTC 2026-11-12T12:26:16.318591+00:00 → 2026-11-12T12:32:12.956253+00:00，时间戳间隔356.638秒（约5分57秒）；runner计时349.601秒。
- report SHA256 b5597877c44d8283d7cfc7c55d50846af77173976825cc27adfe149be5a5105f；artifact manifest 2cfbe0b69af3906cb551275307e78e638cef49e2a210b2942b4aca2dfa1bd9e1；source manifest 2da04226c46e1fd47443e3b066d3e289b8cc9781eb37b0c22654c5a1e060d43c；assembly 3eaf96f715de90afc625afe471508d0bb7973716a184bf94a1296aa5c0f2bf5e。
- 实际DB146 passed/1 failed/0ignored，66未执行。原contact8、新privacy3、belief4+4+4、overview5/memory_revisions8/documents_view4/grounding9均逐名ok；persistence16passed/1failed，scheduling等后续未运行。不是以N5 partial59补判。失败persistence.rs第856行matches期待Invalid为false，未打印实际返回，根因与实际error/success尚需root诊断，不先指认隐私绕过或测试过时。
- 默认75/feature76普通通过，各213ignored；观察4，P01/P03离线38/46，M0/fmt/两严格Clippy/前端build/test通过。通用create/restore服务graceful、数据12组、overview9/docs8实际浏览器通过；child/parent来源绑定/cleanup/final_integrity真。当前390实际目视，Web树SHA与N5已目视1440完全一致，本轮自动360/390/1440检查passed。
- 最终源/执行副本/13产物/新输入门禁和assembly精确312复核全部通过；N1—N5原源/完整产物/原report SHA再次核验不变。两套DB container/volume不存在、private state cleaned/password撤除，case服务/DB/private目录全部cleaned。P03-06 helper/第九观察器及79live变更排除，0模型/不部署/不初始化生产，总锁不拆。
- engineering_failures_open=[persistence blocked-trigger Invalid-variant assertion]。下一依赖root获取实际返回值并处理必需触发拒绝语义，保持来源和隐私保障后另发固定候选；P03不改core/旧断言、不在N5r2重复矩阵，不复用清理state。N5和N5r2失败各自保留；N4仍最近此前工程通过。
- 公开交付`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N5R2`，51项文件SHA一致，加bundle-hashes.json；只含报告/脱敏日志/清单/已目视合成截图，不含bin/build/source/private state/凭据/provider原始输出。任务执行与交接完成不等于候选验收通过。

P03-07执行与精确报告交付Goal已complete，工具累计607秒（约10分7秒）；矩阵实际UTC间隔356.638秒。此次complete只表示独立绑定/唯一freeze/完整21阶段尝试/失败和未运行计数/源产物与精确清理/51项公开证据交接全部完成。N5r2仍failed，persistence具体缺陷仍open待root诊断；N5原failed与P03-06 READY4项源哈希再次确认未改。未释放新候选前不自行再跑或收费。

## P03-08 active / N5R3独立核验与唯一冻结通过

- authority差异与新QA准备合同已读，blueprint不变。N5R3 assembly SHA 1fa141f7aad810a8017f602ac3be11392dfd98715649266be1a4206ef88aa748；failed N5R2 base完整verify；312精确源/4overlay，无新文件，root-context-privacy-04一份源/原manifest/handoff独立验证通过。3份权威快照之外所有非context源原样，原persistence/contact/context_privacy完整hash未变。
- 无第九observer/P03-06 helper和新UI，保留N5原21stage源码，79后续live变化排除。stage冻结前后exact312无缓存污染，helper禁写bytecode。
- 唯一checkpoint /workapps/project-alia/alia/evals/results/p03/checkpoints/belief-context-errors-20261112-n5r3，manifest SHA256 529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974，freeze/verify实际exit0；预检n5r3-assembly-verification.json。将启动一次全工程，213真实DB全部新执行，总锁/独立资源/0模型/不部署，N5/N5R2旧失败不改。终态交付后再做正式释放的管理QA免费代码准备，不混入本候选。

N5R3唯一完整父任务启动：PID 1697507，同进程exec冻结runner，固定output /workapps/project-alia/alia/evals/results/p03/engineering-belief-context-errors-20261112-n5r3；只跟踪对应tool session至terminal，观察超时不重启。

N5R3唯一父任务tool session86777/PID1697507已实际启动；management输入/M0/语料/P01离线38/P03离线46/fmt exit0，独立Rust build进行。源manifest529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974，保留原helper；只跟踪此session不重复启动。

N5R3同一session86777：普通75/feature76 passed，各213 ignored，观察4，默认及feature严格Clippy和前端build/test均exit0。当前213真实DB执行，Cargo PID1719123白名单确认N5R3 target/profile0/jobs4/incremental0/串行；database-effective-build-env.json无凭据。独立diff确认direct-trigger同owner读取前置、原递归来源gate仍在；原测试全未改，不以局部28代替本次全量。

N5R3同一session86777：database-tests真实213 passed/0 failed/0 ignored（普通75filtered），实际exit0。原persistence17/contact8/新增privacy3及belief/概览/纠正/资料/grounding/scheduling逐名stdout ok、Running目标逐项确认，accepted-test-execution.json。没有跳过原N5/N5R2失败断言。父任务进入通用浏览器，整体21stage及清理仍未终态。

## P03-08 / N5R3唯一完整21阶段通过终稿

- 唯一session86777/PID1697507自然终态exit0，21必需阶段全部实际passed/exit0，full_engineering_pass=true，未重复。UTC 2026-11-12T12:48:18.541078+00:00 → 2026-11-12T12:55:09.071577+00:00，时间戳间隔410.530秒（约6分51秒），runner记录403.538秒。
- report SHA256 a697b45510e205959e75ac3fa20cd3d89dc929094c789d7f784d9e901d1986c2；artifacts f38fbb5a05913ab25b14d509f3ebd33274701ed675020cdd8db885f761bcebc3；source manifest 529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974；assembly 1fa141f7aad810a8017f602ac3be11392dfd98715649266be1a4206ef88aa748。
- 213真实DB/HTTP passed/0failed/0ignored（普通75filtered）；persistence17/contact8/privacy3逐名实际ok，belief4+4+4/overview5/memory8/docs4/grounding9/scheduling14亦逐名ok，Running目标确认。原断言全未改；N5与N5R2旧失败保留，不追改。普通75/feature76，各213ignored，观察4，P01/P03离线38/46、M0/fmt/两Clippy/前端全passed。
- 通用create/restore、数据12组、overview9/docs8实际浏览器passed，parent/child/provenance/cleanup/final_integrity全真。最终source/execution/13产物/management gate/assembly312精确集合复核通过，N1—N5R2完整源/产物及原report SHA保持；两套container/volume不存在，state cleaned/password撤除，case服务/DB/private目录清理成功。0模型/不部署/无生产初始化，原总锁不拆。
- engineering_failures_open=[]（仅本N5R3）。第九observer/新管理UI/P03-06 helper明确不入，不宣称行为/M1—M5/删除备份/十四天通过。后继N6-base仅为准备参考，不是final工程/行为release。
- 用户公开证据`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-N5R3`，50项文件SHA一致，加bundle-hashes；只有报告/脱敏日志/清单/已目视合成截图，无bin/source/private/provider原始输出。本轮终态精确交付已完成，现按正式授权继续管理QA免费代码准备及条件观察器实际unit执行接入，不启动另一完整矩阵或模型trial。

## P03-08 / 个体工作区QA免费准备 READY（N5R3终态交付之后）

- 限定3QA源+专门handoff，五个固定source-declared browser case：memory-revisions15/source-inspector16/beliefs10/relationships9/notes8；旧21保持，全部声明时26必需，缺失/失败/未验证/重复不完整通过。按base3+7可选包固定顺序复合最终路径，MemoryInspector/memory_commands/Settings有意覆盖正确绑定，所有最终源/HTTP-PG测试必需。字符串与对象check协议分别适配，完整实际reported source集合、零调用类型/cleanup/SQL-fault/timeout/integrity fail closed；原独占trial资源和父锁不变。
- 保留P03-06 artifactchecker和20用例/发布AST原样；已有observation-tests条件加入固定第九example的真实cargo test，按Running目标顺序逐目标核对unit summary，新目标至少15/旧目标各2、零failed/ignored/filtered，不能只编译或跨目标补数。本准备没有实际执行新目标或新browser。
- 最终90 tests=原46+observer20+workspace24全部actualexit0，M0 actualexit0，五接受JS node --check实际exit0。首轮旧66因正常总锁等待40秒后exit0，无拆锁/重复父任务；无本次QA代码测试失败。两个独立临时审计假设已在metadata如实校正（same-byte overlay、旧三包无handoff缓存），没有变更candidate或放宽source门禁。
- N6-base c5e45965…b2cadc exact373源/75overlay（74bytechange+原tests/memory_revisions同字节accepted overlay），十层接受源完整核验、management10 manifests及最终路径通过，得26 stages/7children/conditionalobserver；只读，没有finalN6 checkpoint/freeze/build/DB/browser/model/fullmatrix。旧N4/N5/N5R2/N5R3新validator输入/13产物/真实旧观察日志兼容，原结论不改。
- 最新用户路线覆盖已采用：完整M5工程后集中真实验收，N6后无v5付费trial；下一有界sandbox由root先定义稳定接口，当前不越权开工。详见`docs/ai/ALIA_P03_INDIVIDUAL_UI_HANDOFF.md`；READY`evals/results/p03/p03-08-individual-ui-ready/ready.json` SHA 4ff737c8bb4d051ddd1f9aed433c52ec2056741ce65736baee24f802d8cdfbdb，公开`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-08-READY`。
```json
{
  "scripts/qa/management_browser.py": "490b35759c0b0644593b975c5a7c0b014efa388585cab063705b81fa13975f4b",
  "scripts/qa/run_engineering.py": "d68a64274e24dc57cc71beee4bbd66291b4ab308826a92ea6cc5bf259b738c7a",
  "evals/p03/test_individual_workspaces.py": "143c0dc88e3716fce2f56025719ac7f3b60f96d756da8a86470d65963c4ea40d",
  "docs/ai/ALIA_P03_INDIVIDUAL_UI_HANDOFF.md": "7783fb2a29ec8a56ff58a568d7b7e6d92a097187d062f92a0ba52f02f431c8e1"
}
```

P03-08 Goal已实际complete：N5R3独立绑定/唯一freeze与21阶段213真实DB全通过/末尾完整性清理/50项公开交付，加终态后限定五工作区QA/条件观察器实际unit执行门禁/90免费offline/M0/只读N6-base预检/READY4SHA交接均完成。工具累计1843秒（约30分43秒）；core矩阵UTC间隔410.530秒（约6分51秒）。本Goal无未完成自有事项；最终N6/26阶段/真实新观察器/实际模型/M5整体均不被此complete代判。下一依赖root批次命名和下一sandbox稳定契约；遵守M5工程完成前不新增收费模型任务，不自行启动额外全矩阵或并行占有多任务。

## P03-09 active / 已发布sandbox契约与只读可行性

正式root契约已到并优先采用：仅新sandbox adapter/test/handoff，不做第二协议或生产注册，继承最新用户M5前0收费路线。只读实际env-clean sudo -n /usr/bin/docker info：Linux/x86_64，Docker29.9.3，systemd cgroupv2，MemoryLimit/SwapLimit/PidsLimit=true，builtin seccomp+AppArmor default；CPU实际配额仍需容器内证实，不能按info字段空值猜通过。/usr/bin/docker、/usr/bin/sudo实际可用；python仓库本地镜像清单为空。未改宿主/生产配置或现有容器，未启动模型/矩阵。后续实际镜像准备/隔离测试按已发布root范围+总锁，不把支持标志当隔离验收。

## P03-09 / 真实隔离Python执行器 READY

仅new sandbox.rs/tests/sandbox.rs/专项handoff，未改注册/Cargo/schema/权限/旧QA。Finalv6 parent1936664/session97470自然exit0：7普通guard通过/9实际ignored未计通过；独立actual9 passed/0failed/0ignored（7filtered），13记录=10attested payload+1never-startedforeigncontrol+2Unavailable。严格Clippy/fmt/M0均actual0，固定N5R3+2owned测试源稳定；全测试/清理连续总锁，最后queue47.745秒。
- 官方python digest python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72，image setup明确root授权且持锁，runtime pullnever。实际内核Seccomp2/NPRIV1/CapEff0/UIDGID65534/memory128MiB/swap0/pids64/cpu.max100000100000/tmpfs16MiB1777noexec/rootro/networklo，通过fork拒绝/OOM137/双流与UTF8输出界限/实际超时停止/hostTCP与DNS正对照/私有路径Socket环境隔离/foreignowner拒删/native权限与缺image不可用验证。
- 每hostCLI非阻塞有deadline，source只有stdin，private空Dockerconfig/envclear/固定本机socket；owner实际ID/label/policy inspect先于start，kernelbootstrap先于用户source。有效timeout100..10000总预算含setup清理，保守留1000ms teardown，≤1000不创建即timed_out；cleanup缺证/失败保留精确name handle并infra_failed，绝不success或fallback。
- 外部只读audit六轮69已知handles全部absent、私有client dirs/launcher processes无残留，最终2SHA与测试copy一致，P03-08 READY4源未改。v1真实7fail（Command保留stdio阻止EOF）、v2actual8pass但Clippy1fail（测试写法）完整保留，修复未放宽policy；后续通过报告各自保留。不把普通ignored当真实通过，不把容器当VM/完整M4。
- 专项`docs/ai/ALIA_P03_SANDBOX_HANDOFF.md`；READY`evals/results/p03/p03-09-ready/ready.json` SHA 851ebea224bc2416ed5360854c6232639bad297c0efe29b823a6d0b44c64e267；公开`/home/zero/Documents/Codex/2026-11-12/alia-p03-engineering-qa/outputs/ALIA-P03-09`，不含image/build/source/privatecfg/provider日志。下一依赖root受控产物/注册/资源准入稳定契约，仍不收费/不部署/不额外fullmatrix。
```json
{
  "src/adapters/sandbox.rs": "a41c2b4f9bf139ecff9f7ede181de52eef30bda1eb42aa049995016e170a8ee7",
  "tests/sandbox.rs": "94d06b1ab803321a7998076eb12edf0bd3b7ec96a530a7f05da8a25e9bf1946e",
  "docs/ai/ALIA_P03_SANDBOX_HANDOFF.md": "21f3af87971bef9134cbd19f8f64d32c30ba65f0fa962f8930a10dfbdfb1e28f"
}
```

P03-09 Goal已实际complete，工具累计2867秒（约47分47秒）。限定SDK/测试/交接3源、官方pin准备、最终7offline/9actual/Clippy/fmt/M0真实exit0、69已知handle与privateclient/launcher独立残留审计、3READY SHA及31项公开证据均已完成。原失败报告保留，最终engineering_failures_open=[]仅指本有界sandbox任务；仍未生产注册、未做artifact export/受控skill registry/完整M4或M5接受，未改旧QA/Cargo/共享权限。后续按root下一稳定契约继续，不自行发明新接口或启动模型收费/全矩阵。

## P03-10 / SQLx与真实Docker独立分派 READY

- 仅数据库入口/实际scripts/qa runner/新sandbox QA helper/新28单测/专门handoff五源。原--ignored确会混入9 Docker，现自动SQLx目标仅排除固定9项，未知域/误过滤拒绝，PG实际逐名对照；SDK声明时另加mandatory sandbox-tests，旧21/26保留，全管理+SDK为27。原UI/观察器/产物/SDK/旧断言SHA保持。
- Docker固定cached pin、真实继承fd准入、独立target/结果、无DB/provider环境、私有空Dockerconfig；9实际函数+13结果及父子来源/完整性/精确清理多层门禁。只能清理空namespace持锁准入后精确自有ID；foreign/身份改变拒删，恢复需求即使清理成功仍失败；不放松总锁。
- 最终118offline（旧90+新28）actualexit0，M0 actualexit0；七旧checkpoint/报告/产物只读verify，新PG日志校验保留N5/N5R2失败，旧观察实际日志兼容，N6-base26/含SDK27。P03-09实际9日志/13结果再次只读通过新validator，2SDK原SHA不变。新入口没有实际Docker重跑/新PG/新完整候选或收费，不以mock代真。
- 最初只读验证把Unavailable静态stderr误拒exit1，QA-only规则已纠正，不改SDK/断言。READY发布命令最后一个console print缺import json而exit1发生在全部文件成功写完之后；独立publication-audit实际exit0逐SHA确认，原READY不覆写。
- 专项docs/ai/ALIA_P03_SANDBOX_QA_HANDOFF.md；READY evals/results/p03/p03-10-sandbox-qa-ready/ready.json SHA ec1e7ba753f6e6f4b58c3cfda70f9201dcfae2911fb1b71b8b76ad649ff8016a；公开ALIA-P03-10-READY。下一依赖root接受五源并统一freeze/完整工程；本编码任务已终态交付，随后按已授权仅做并发隔离验证审计，不新开Goal。
{
  "scripts/test_database.py": "e89e4d88a42936ec8101ac57c572dfc8b614d08ea9cf2c3b0c2c4cf607baab65",
  "scripts/qa/run_engineering.py": "63df803143bf2a3979703ca29735ce81517b814b197536842fec0ad5538d8593",
  "scripts/qa/sandbox_tests.py": "e1eb37ded42ed9596812946934ee26b87c34b33115d48e2634d1a90bbf139372",
  "evals/p03/test_sandbox_dispatch.py": "cbd2ce22acb2452725a63984046da787af44390939f9c001095feaf27e04a9ef",
  "docs/ai/ALIA_P03_SANDBOX_QA_HANDOFF.md": "abf3b80c16949ae3502248ee99e4f05eb72e3bd1887e41008efef8a0906d4895"
}

## P03-10 v2 / 恢复失败证据保留补正（当前接受源）

- 交付前复核发现原QA恢复函数在remove/recheck异常时可能只留错误类型；仅自有helper/test/handoff补正，保留精确ID/name、部分结果handle与失败时私有恢复目录。v1 READY/报告/公开输出不改，原五源按SHA精确归档p03-10-sandbox-qa-archive-v1/source。当前v2优先，最终120（原90+新30）offline与M0均actualexit0；新入口真实Docker仍未重跑，P03-09实际9/13只读validator再次passed。
- 当前READY evals/results/p03/p03-10-sandbox-qa-ready-v2/ready.json SHA 2275330d28dd2a3941d42653eb45a4eb3a3f788cad11357fb12d31c1d4c21105；公开ALIA-P03-10-READY-V2，SDK/原断言/产物与工作区QA/共享锁未改。无需新Goal，原P03-10 complete记录1483秒为此前v1交付时刻，v2复核为本次顺序跟进，不把旧工具耗时追改。
{
  "scripts/test_database.py": "e89e4d88a42936ec8101ac57c572dfc8b614d08ea9cf2c3b0c2c4cf607baab65",
  "scripts/qa/run_engineering.py": "63df803143bf2a3979703ca29735ce81517b814b197536842fec0ad5538d8593",
  "scripts/qa/sandbox_tests.py": "cca52fb7714f19f7d22f2d3f5f7e41c62b79226d3ef232a26fbc629b69ca4245",
  "evals/p03/test_sandbox_dispatch.py": "4c0dbd8936fd3ec787dbab58486110ed43e7685a447a40f684ba395b92985ac2",
  "docs/ai/ALIA_P03_SANDBOX_QA_HANDOFF.md": "775569ad7c41e44cb7bf2f54fea7706d736c4b7f45887ec967598b8b98d8b486"
}

## ALIA-P03-CONCURRENCY-VERIFY / 当前总锁下真实隔离 READY

- P03-10终态之后唯一验证跟进，没有新Goal/agent或并行活跃编码任务。原check_isolation仅证明双PG共存/顺序SQL与清理，本次持原总锁的单父任务内真实两个固定312源、两target Cargo bin build、两PG/Alia/Chromium并发。主PID2097822/session99994 exit0，UTC15:00:33.712287→15:01:49.357894，75.646秒；真实构建重叠65.576秒，两个编译产物流重叠64.674秒，真实已初始化browser/service重叠1.667秒，A/B 15/27轮询。
- 原源manifest529c856a…af974及完整旧artifacts f38fbb5a…cebc3原样；两份新snapshot id/hash/target/output/产物inode不同，bin同字节SHA a65b4a0f…40f49；PG32872/32873，服务57645/48817，不同身份，跨token401。SIGKILL A服务=-9并精确清理A DB时B仍可用；第三worker真实创建卷后run前SIGKILL=-9，原持久owner/state精确恢复清理，非mock。
- 主探针PG并发段以读取为主，故另新双PG开放事务实际并发写：write PID2117149/session23513 exit0，UTC15:03:12.365733→15:04:55.103918，真实总锁等待94.517秒；真实事务调用重叠2.112446秒，两pg_sleep(2)的真实服务器重叠下界1.885869秒，各提交仅有自身a/b，删除A保留B。无重跑旧全矩阵/模型。
- 只读独立audit actualexit0：4个已创建container+5个已创建volume全absent，另1未启动container名也absent，共10个精确名字；6个knownPID不存在、2服务端口closed、各312源/执行副本/产物与旧固定源完整artifact稳定，总锁device38/inode1660460不变，独立竞争仍被挡。私有state/password撤除及运行目录删除，0模型/部署/生产初始化。
- 主report SHA 0deb0f0d86d8f7ef76698456afd825536e47f8b0fb64a7f4ba831cf6127fca47；事务report SHA 219c26d21598f419af77a67f7855c7ca792857f10ba79807e95f4b00a668ff71；READY evals/results/p03/p03-concurrency-ready-v1/ready.json SHA 61221f3f252eb20b4b6ab8345a27f19fc19177e7e713577f707084b2c30ff2d7。专项docs/ai/ALIA_P03_CONCURRENCY_HANDOFF.md；公开ALIA-P03-CONCURRENCY-READY（报告/精确脚本/编译与脱敏服务日志/清单，无bin/build/完整源/private state）。
- 限制：只证实同一排他父锁下当前固定N5R3的资源隔离，不保证两个独立完整runner、所有fault点/branch/SDK/并发npm或任意host/daemon崩溃。当前全局锁未拆；下一依赖root审阅范围并决定资源锁策略，P03不自行改common/core/锁或追加完整矩阵/收费。新root-history-03/autobiography-write-v3/privacy-stream-01信号记录在专项handoff，不混入本基线；后继候选始终独占target与执行SHA，不跨不可变输入共享本包rmeta/rlib，不把共享target链接不一致称业务修复。

## P03-11 / 私有版本化技能vault工程交付，独立协议组合not_run

- 五源范围=SDK/原SDK测试（root追加授权）+新sandbox_skills.rs/test+专项handoff，无共享module/Cargo/权限/迁移/生产注册。新SkillSandboxRequest保留24KiB源与8KiB严格JSON，固定python-skill/v1 -I -B -c wrapper，variable stdin carrier≤160KiB，argv/env/mount无用户正文；原raw32KiB及原9全部函数/断言/helpers字节不变。
- 真正private0700/0600非执行源，固定UUID/version路径、文件锁、原子不可覆盖源/事件+fsync/head；严格恢复重放hash链及源/实际协议产物，不信passed/registeredbool。候选/验证/注册/撤销分离，非Deserialize typedGrant匹配source/report/cases/image/wrapper/expiry/sandbox.python；注册不等于Runtime权限。invoke前后重验，实际调用计数含失败/unknown；Root调用前必须用cases.sandbox_calls_upper_bound预留原三工具slot，不额外放大。
- 最终v4 parent2409610/session31941 exit0：固定317源manifest3a6364e41078cd327634da76970625efeebac37c815ffd30cac2827671ebc65a，25ordinary/18ignored另分，9新actual组及Clippy/fmt/M0全部0，测试bin6e4ee9312de6c3915866d28843824ffb3c350204970390c6f3734fa0ce3d6af3，UTC16:18:52.743465→16:20:37.219138含queue15.110s。SDK/source与bin逐SHA同v3，所以只读引用v3已actual通过的旧9/13记录，不重复；原SDK新模式改动后实际旧9已跑，不拿旧09数据代判。
- v1 build101（旧Cargo无canonical libc）、v2 raw9 guard9fail（Root拆锁heavy_lock已成model锁但旧测试要求global）原失败保留，均未启动用户Docker代码。Root批准新resource locks已采用，另持legacyGlobal1660460仅作原9兼容，无改shared locks/无model预算锁。后继full QA仍需独立skill domain及mandatory阶段/legacy桥，不得将新9忽略或混入SQLx。
- 真实磁盘补证parent2440139/session35147 exit0，同最终bin，source device66306/600/nlink1，区别/tmp tmpfs38；真实验证/注册/调用/新进程恢复及权限链接篡改反例全部过，私有数据已清理。最终Vault30次真实execute_skill均有前导；另SDK6次中2actual/4准入拒绝。11成功/7失败验证产物（不是11当前能力）。Expected cleanup=false与corrupt/durable=false都未升级，实际计数和unknown上界保留。独立audit exit0，历次81已知handle全absent，private vault/client dirs和父PID无残留；P03-10/隔离原READY报告保持。
- P01-CAPABILITY-PROTOCOL-01新增合法def+直接stdout/提前exit组合被平台内容审查systemError中断，Root已停止，Cargo/Docker均未运行。严格not_run，不记产品pass/fail；本窗口未替跑/改写该组合，已正常return/无函数拒绝用例不能代证该属性。Root后续协议审查未关闭，生产开放false，不能把本工程交付称全M4/M5接受。
- 专项docs/ai/ALIA_P03_SKILLS_HANDOFF.md；评审包READY evals/results/p03/p03-11-ready/ready.json SHA c8524fbb4a70c8b4044c12bf2c46c24e20e6f42a3127ae85f7f00ed618c6c7ea；公开ALIA-P03-11含代码/合成报告/日志/结果/完整哈希，无bin/build/完整基线/私有vault/凭据。四代码源与实际最终固定输入一致，测试后整理的文档差异明确单列，不伪称重编译。
- 下一正式自有任务：management_browser接受链p02-05→p02-07新增固定pin与正/反离线检查；root给出的manifest分别ee553288…6b58/234a4608…97f，不采样live、不改未知源门禁、不加跑完整矩阵。Root Social/format、09B-owner、canonical29/P05-07后继由Root有序组装，packet8不视作旧模型collector已验收，0收费。
{
  "src/adapters/sandbox.rs": "a7c74643391f336acd8568e55bee9263a268571bd348aa3d22b702723c460868",
  "tests/sandbox.rs": "e2bbcb3401a6cb67a26010a7f3c7c5939984201e405126c04b3f667a146e11c6",
  "src/adapters/sandbox_skills.rs": "f39695f8ab98337c059fce82263e88eb9de33fcaf84716de67227312106d7145",
  "tests/sandbox_skills.rs": "59023d3600da2422d2cceab93594d366311668ff5efd0dc3c921e16d08318ba0",
  "docs/ai/ALIA_P03_SKILLS_HANDOFF.md": "bb47223f0433073a7e4bb59d5f15e24fa6d5657f1c36c747dbfe515529cc2c1a"
}


## P03-12 r1 / 固定 FLOW 后继绑定与 COLLAB 入口 review_pending

- owner=P03；reviewers=Root/P01；固定 p02-05→p02-07 接受链与已接受 COLLAB-02 r4 ready 门已实际接入；四 QA 源、两份原字节接受清单和专项 handoff 为本切片审阅范围。Root registry 没有修改，作者没有自批；registry-proposed 对 P03-11r2/P03-12r1 都保持 defined/released_by=null，实际 ready 门明确拒绝。
- 实际 164 项离线、入口最终17项及 M0 全 exit0；固定 P02 原始408/413项源、15/2项接受源、各三项 dist、十二层 scratch 组合和七个历史 checkpoint 预检通过。两个初次失败及修复摘要保留；旧 FLOW 两次失败和 P03-11 READY 原 SHA 未改。本轮无 Rust/构建/PG/浏览器/Docker/模型/完整矩阵。
- MODEL-DISPATCH-01 本 QA 派发 gpt-6.1-sol/medium，后端身份未单独可读，不回标旧任务。仅 QA 编排、固定绑定和证据汇总；生产/权限/并发/沙盒语义归 Root。既有 Goal/历史保留。
- P03-11 增补 revision2 协作元数据，四 Rust SHA 与实际 v4 不变，原公开包保持；独立协议门仍 not_run，生产 false。P03-12 下一依赖 Root/P01 当前 subject 审阅及 Root 释放唯一后继候选，再运行必要矩阵。P02-08 待 API/浏览器依赖，不能提前认全功能。
- 专项 docs/ai/ALIA_P03_FLOW_BINDINGS_HANDOFF.md；证据 evals/results/p03/p03-12-evidence/review-delivery.json；公开 outputs/ALIA-P03-12。task subject SHA 06acaaf9b67d9ce95938545a1d36cfa5309391146a5302a4f0fb7b3e5ee32e37。本地检查通过不等于正式 accepted。
