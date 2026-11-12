# ALIA-P02-02｜运行概览真实聚合与异常定位

状态：READY_TO_FREEZE / P02-02工程交付完成；最终九个源码/测试文件已保存稳定，无在途任务。

- 窗口：01a1cf0b-e8a3-7a72-a261-d7034fb0ce0d；主线：01a1cbe8-e270-75e1-9535-d3f2e0ea81c4 / local。
- 已重读AGENTS、三份权威文档及当前并行登记。保持能力与质量优先、Individual/High/Adaptive/Human-Like、来源/隐私/预算/权限边界；原P02六文件及N1证据不修改。
- `get_goal`当前返回null（本聊天历史有P02 complete回执，未清除历史）；没有未完成Goal，已按新指令create_goal建立P02-02 active，不设token_budget。
- 目标：全量可见计数、UTC今天及前6日趋势、最多5条近期失败入口；真实HTTP/隔离PG/浏览器局部验证，无模型调用/部署/生产初始化。完整矩阵由P03负责。
- 当前尚未见 **P02-02 released** 或对应精确文件/SHA/响应字段冻结，因此只读准备，不将旧P02 released当新范围授权。

## 基线（读取时）

```json
{
  "AGENTS.md": "79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4",
  "docs/BLUEPRINT.md": "30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943",
  "docs/CONTRACTS.md": "621e28cd088169c3194d4e28933ea660b66950ad77d04c86a8a5f0d07e3b4005",
  "docs/IMPLEMENTATION.md": "fcd43b8a9265c4b0ac7fd6f618d35d5f0e2397bd940218d971df3f3c2163cba6",
  "docs/ai/ALIA_PARALLEL_COORDINATION.md": "9020a4855713e8bb9a9bfe54f01ad00622935bfe77534a22aa561b7918121b18",
  "web/src/features/Overview.tsx": "b1b1d1806bfce304b5d4769a108944356042f20d33a7cd88fb35820c1e7ce860",
  "web/src/features/OverviewHealth.tsx": "424d7bd1d2ed91cbe69169392c9f1e65fcfab15a879f1a8635d85d2d2762ed18"
}
```

## 只读发现与主线最小整合请求

- Overview.tsx用memories.items.length展示“已加载记忆”；OverviewHealth.tsx只读diagnostics，显示可见failed数，无趋势或失败入口。
- 候选响应建议：`{individual_id,state_version,privacy_epoch,observed_at,counts:{memories:{active,disputed,superseded,stale},goals:{proposed,active,waiting,completed,abandoned},runs:{queued,running,committed,failed,cancelled,stale}},trend:{timezone:"UTC",from,to,buckets:[{date,from,to,partial,created,failed,foreground,background}]},recent_failures:[{id,created_at,mode,error_code}]}`。各枚举零项显式0；counts计全可见历史；failed桶表示该日创建且当前status=failed，不声称按失败发生时间或可用率。today.to用观测时间，partial=true；历史日全天，7桶升序。实际接口请主线冻结，P02不自行争定第二套。
- 必需公共整合仅三处（由主线执行）：`src/adapters.rs`新增`mod overview;`；`src/service.rs`新增`mod overview;`及 `.route("/overview", get(overview::overview))`。新handler名称候选overview；会提供最终确切签名，不直接改注册。
- 请求精确登记：src/adapters/overview.rs、src/service/overview.rs、tests/overview.rs、web/scripts/test-overview.mjs、Overview.tsx/OverviewHealth.tsx；为避免共享类型/CSS改动，建议额外专属新文件 `web/src/features/overviewTypes.ts`、`web/src/features/overview.css`。hook可放OverviewHealth.tsx中，不争写原useResource/shared context或原P02文件。
- 只读查询复用adapters::api::{VISIBLE,existing}与console::safe_failure；同一只读可重复读tx里完成全部计数/桶/最近失败，明确内存/目标/运行各自与公开列表相同的来源屏障。不改core store隐私实现。
- 前端专属读取以SSE后的c.lastSyncedAt与state版本作为失效信号；loading/失败/断线清除聚合，旧会话/旧版本响应不回灌，不将缺失当0。现有/state能力及预算语义原样引用，近期目标/交流明确是已加载摘要。

## 验证状态和下一依赖

只读cat/sed/sha256sum退出0；未运行任何编译、DB、浏览器或模型。等待主线在协调文件登记P02-02 release/基线/响应及三处注册；之后持续实施并按全局flock跑局部验收。不向其他聊天发消息。

## Release 与可整合最小注册（已保存）

已读取协调文件末尾 **P02-02 released**，以主线冻结的totals/trend.days/recent_failed_runs实际JSON为准（此前候选形状作废）；专属类型/CSS改用登记的 `web/src/features/overview/` 子目录。Overview两旧文件哈希仍与只读基线一致，四个新Rust/脚本路径此前ABSENT。

**主线现在可整合以下三行**：`src/adapters.rs`加入 `mod overview;`；`src/service.rs`加入 `mod overview;`，API Router链加入 `.route("/overview", get(overview::overview))`。新 `src/adapters/overview.rs` 和 `src/service/overview.rs` 已保存，方法为 `Store::public_overview()`，handler为 `overview::overview`。P02不修改注册文件，待主线注册后再实际编译/HTTP验收。

后端当前代码在单只读REPEATABLE READ内读取版本/epoch/观测时刻，复用既有VISIBLE/existing/safe_failure及m.id/g.id/run-trigger公开过滤；全枚举计数、UTC恰好七桶、创建日当前failed、倒序最多5条失败含finished_at。尚未编译/运行，不标通过。前端/专属测试持续实施。


## 首轮局部验证（实际）

- 主线已整合三处module/GET注册。`/tmp/alia-p02-02-build-v1.log`：cargo check --locked --lib退出0；npm build退出0。
- 新增前端Overview/OverviewHealth和专属overview/{types.ts,useOverview.ts,overview.css}，一次聚合读取；SSE快照/版本失效即隐藏数据，跨失效读取串行，旧会话/旧版本响应不能回填。保留近期目标/交流分页摘要和原路由入口；无新依赖、共享修改、预算推导。
- 专属 tests/overview.rs 五例实际TCP HTTP/SQLx，`p02-02-overview-v1`：4 passed/1 failed/0 ignored。跨页全量65、七日UTC/当前失败状态、两代祖先阻断、并发变化单快照四例通过。空库/错误用例最后断言把数据库不可用误期望500，实际现有服务契约返回503；已把测试改为503，没有修改生产错误处理。失败报告和资源身份/清理证据保留。
- 局部父驱动 `/tmp/alia-p02-02-local.py` 全程实际flock，复用P03已验证的database.provision/cleanup函数创建专属独立PG容器/卷，SQLx/浏览器不访问生产或默认Compose；测试密码只在私有state/env，不输出。新 web/scripts/test-overview.mjs 是显式外部子模式，沿用原P02 URL/TOKEN/OUTPUT/BINARY/DATABASE/PSQL argv环境契约；不自行编译/读.env/回退开发静态目录。父驱动冻结临时服务二进制与web树并负责服务/DB清理。
- 修正后待v2局部重跑，尚未宣称Clippy或新浏览器通过；完整矩阵仍由P03负责。

## v2真实结果与浏览器反例时序修正

- `evals/results/p02-02-overview-v2/overview-http-postgres.log`实际 **5 passed / 0 failed / 0 ignored / 0 filtered**；专属严格Clippy、npm build/test、服务构建全部退出0。独立数据库、服务cleanup均true。
- v2浏览器通过空库/65条全量与七日/SSE/键盘异常跳转后，在旧响应断言失败。实际原因：撤销SQL的子查询读取memories，而慢请求夹具正锁该表；撤销被延后至旧响应结束，旧65在撤销前合法显示，不能据此判断UI已失效后回灌。
- 已仅修正专属浏览器夹具：锁表前取UUID；撤销直接更新该对象并递增state_version/privacy_epoch、发送既有privacy.changed；明确等待浏览器实际读到version2，再释放慢查询并检验旧65不得回填。没有删断言、fake HTTP、降低标准或修改生产hook。
- v3仅重跑浏览器及必要服务/静态构建（不重复五项DB/Clippy/组件），当前在全局锁队列自然等待；没有因无输出启动第二个任务。父驱动支持scope=browser-only且保留前版5项DB证据。

v3排队期间完成只属于本切片的最后文字/可访问性核对：观测/失败详情明确浏览器本地时区，七日表仍UTC；趋势滚动区有region名称、键盘焦点，窄屏会断言ArrowRight实际滚动。慢读夹具先查询pg_locks确认锁已获得，进程退出promise在启动即注册，异常清理不会等丢失的exit事件。生产陈旧响应保护逻辑未放宽。所有这些保存均在v3父任务尚未取得总锁/编译前完成。

## v3真实浏览器通过；最终可访问性保存需按新SHA再验

v3浏览器9组实际检查passed=true/cleanup=true，父驱动scope=browser-only且服务/独立PG容器卷cleanup=true。三张360/390/1440截图已查看。采集结束时对照SHA发现：最后region/键盘检查保存实际发生在v3完成之后（工具尚未收取终态），此前“v3前均保存”时间判断不准确；不把v3报告绑定到这两个新SHA。现停写源码后以v4复验最终版本。

截图核对另外修正专属状态dl行的默认dt margin，保持数字与名称居中对齐；截图前重置键盘滚动及焦点到页面顶端，避免全页截取固定导航位置受测试滚动影响。断网恢复后原共享App全局错误提示需手动关闭，专属统计已实际恢复且旧错误不会作为当前统计；测试明确点击其现有“关闭提示”后再拍正常态，未修改共享会话/错误处理。完整原故障和恢复断言保留。

v4浏览器保留失败：空库初次0显示之后，控制器正常完成首个SSE/分页同步，专属hook按要求清除并重读统计；测试立即数行时落在loading窗口，0行≠预期7。已明确等待现有全局成功同步时间和新概览7行，再进行断言；每次布局截图前也等待当前64统计。没有保留loading期间旧统计来迁就测试，原5项数据库与单快照规则不变。v5将只重验该真实异步浏览器流程。


## READY — P02-02最终稳定交接

**READY：九个获准源码/测试文件均与v5父报告的SHA256一致，已停止写入；没有本窗口在途编译、服务、DB或模型任务。主线可选择接受overview overlay，P03在其下一命名候选做完整矩阵。原P02、N1/N2及其失败/成功报告未由本任务修改。**

### 实际交付

- `GET /api/v1/overview` 已通过主线三处注册整合。无query字段，空query后缀允许，任何实际未知/重复参数422；沿用Bearer/同源/no-store及固定错误响应，未初始化409、身份来源阻断404、存储故障503，不把故障当0成功。
- 一个REPEATABLE READ只读事务中取得identity/version/privacy_epoch/observed_at、三个资源的全量可见总数及全部枚举状态计数、7个UTC创建日桶（仅今天partial）与倒序最多5条当前失败运行。memories/goals/runs使用原公开读过滤及递归VISIBLE/existing；recent error由现有safe_failure处理，只有id/created_at/finished_at/mode/error_code五字段，无正文/payload/raw error。失败桶表示创建于该日且当前failed，不是失败发生日或在线率。
- Overview/OverviewHealth展示真实全量指标、按状态分项、UTC七日表及旧#/traces/{id}异常入口；保留任务/交流入口与明确的“已加载分页摘要”。本地显示时间和UTC分桶范围均明示。零记录不声称健康或整日在线，没有新探测、预算费用口径、依赖或领域写操作。
- 专属hook以token、identity、state.version、成功SSE同步时间、连接状态和手动重试为失效键；跨失效请求串行，loading/错误/断线不保留旧统计，失效/旧版本/退出后的响应不能回填。纯组件及types/CSS/hook均在登记路径，没有改共享context、API adapter、CSS或原P02六文件。

### 最小真实命令与证据（完整矩阵不在本窗口重复）

父驱动均实际由 `flock -x /tmp/alia-project-heavy.lock ...` 连续持锁，覆盖准备、子进程、证据和清理；SQLx `RUST_TEST_THREADS=1`。使用P03已验证的专属engineering资源provision/cleanup函数，随机私有凭据、独立容器/卷及回环端口，不读生产.env/默认Compose。临时服务二进制与完整web树各自复制后使用。局部父驱动保存在 `/tmp/alia-p02-02-local.py`，P03可直接复用以下永久浏览器外部接口，不依赖此scratch父驱动。

- `cargo check --locked --lib` 退出0（初轮）。
- `rustfmt --edition 2024 src/adapters/overview.rs src/service/overview.rs tests/overview.rs` 退出0；仅本任务三文件，未fmt --all。
- **v2 `cargo test --locked --test overview -- --ignored` 退出0：5 passed / 0 failed / 0 ignored / 0 filtered，实际TCP HTTP与隔离SQLx PostgreSQL。** 覆盖65记录对默认30页的全量计数、全部状态显式0、7日UTC微秒边界/今日partial/当前失败口径、近期5条与同刻UUID稳定顺序、未知错误/正文/令牌不回显、两代祖先阻断影响三计数与趋势/失败ID、401/同源/no-store/严格query、未初始化与空库、真实查询故障503、纯读state/events/usage不变。并发测试先建立快照再阻塞memories读，其间改变goal/run状态，返回同一旧快照，下一读取才见新状态。
- `cargo clippy --locked --lib --test overview -- -D warnings` v2退出0；没有声称all-targets/feature完整检查。
- `npm run build --prefix web` v2及最终v5均退出0；`npm test --prefix web` v2退出0，既有SSE及组件检查。最终后续只有文字/局部对齐/region及测试同步修订，均在v5构建/真实浏览器重验。
- `cargo build --locked --bin alia` 最终v5退出0，实际二进制SHA见v5报告。
- **v5 `node web/scripts/test-overview.mjs` 退出0，9组真实浏览器检查通过**：空数据；SSE更新65条全量（/memories仍30条）、7日/范围/5失败；键盘Enter进入既有真实trace；真实锁表慢HTTP读在state/privacy撤销后旧65不得回填而显示64；实际表查询故障时清空且重试恢复；真实offline后清空与online恢复；360/390/1440整页无横溢出、趋势区可聚焦且ArrowRight实际水平滚动；多次读取state/events/usage不变且model reservations=0；退出无统计/列表/浏览器存储。无HTTP响应Mock或可见性覆盖。
- v5父报告 `scope=browser-only`、passed=true；子报告passed=true/cleanup=true；父服务及专属PG容器卷cleanup均true。没有把v5标成五项DB/Clippy又执行一遍；这些取自源码未再改的v2证据。父报告保存并核验九个本任务文件开始/结束SHA。
- **已目视查看最终v5三张360/390/1440截图**；状态行对齐、全量和分页区分、趋势/失败/入口清晰，窄屏趋势为组件内横滚。全页截图中的固定状态栏为既有v4组件。原全局网络错误提示保留显式关闭行为，测试在真实恢复后用既有“关闭提示”确认，不改共享处理。
- **模型调用0、无生产初始化、无部署、无迁移/运行政策/领域修改。** 全项目M0/全Rust/feature/全DB/旧浏览器完整回归在本窗口未重复，交P03。合成数据不作为真实行为或M2/发布通过。

### 外部浏览器接口（供P03纳入未来完整候选）

必须提供：`ALIA_TEST_URL`（loopback）、`ALIA_TEST_TOKEN`、`ALIA_TEST_OUTPUT`（新目录，不覆盖）、`ALIA_TEST_BINARY`（实际固定产物，用于哈希）、`ALIA_P02_DATABASE`（独占且初始未初始化的测试库；alia_p02_/alia_p03_/alia_web_test_前缀）、`ALIA_P02_PSQL_COMMAND`（JSON argv，无密码，如 `["sudo","-n","docker","exec","-i","<owned-container>","psql","-X","-U","<test-user>","-d","<test-db>","-A","-t"]`）、`ALIA_P02_LOCK_HELD=1`（父进程实际持总锁）。支持CHROMIUM_EXECUTABLE，实际使用既有Node24.11.1/Chromium1234。

子脚本不读.env、不构建、不创建/停止数据库/服务、不回退开发静态文件，只创建/关闭浏览器；父进程负责固定完整web树、服务与DB生命周期/身份校验。它校验实际current_database并要求fresh /state409，然后显式初始化合成fixture。SQL与state/privacy改动只为隔离故障/来源夹具，无生产写接口。P03需把5个overview DB用例和这个浏览器阶段实际纳入下一接受该overlay的完整矩阵，不能因N2未含它而称该新切片已全套验收。

### 失败记录与限制

- v1 DB 4通过/1失败：测试错误期待500，现有数据库失败契约为503。只修断言。
- v2浏览器旧响应夹具的撤销自身等锁，不能构成已撤销后回灌证据；修为提前取UUID、等实际表锁和前端version2，再检查无旧值，原断言保留。
- v3浏览器9通过，但工具收取终态前发生最后可访问性保存，旧SHA不当新SHA证据；保留v3。
- v4浏览器读到初始SSE同步触发的正常loading窗口而断言0行，修为等待真实同步/7行，未让UI保留旧私有统计。
- v5最终9通过，各项故障报告保留。独立资源清理均有记录，没有以失败重跑为免费模型调用（本任务从未有模型调用）。
- 这是公开管理元数据切片；全量仅指当前来源可见记录，不能证明完整隐私删除/备份、运行稳定率、模型健康或后续生命行为门槛。Core/来源规则由主线维护。

### 最后保存的九文件SHA256

```json
{
  "src/adapters/overview.rs": "a0fd49b0106c11655387b643b9ce41afa0c1c38d31e80458d8280546a479fc6f",
  "src/service/overview.rs": "9b6f6e289a579f06b5ff4b58ea30eb1d40cae3fd614bb9a09a0d29b604152704",
  "tests/overview.rs": "761ff33aaae6265f32b97111e8eaa3cef236a95c4b6ecab54aa282dc57ad2ba8",
  "web/scripts/test-overview.mjs": "95072f7b741b85f50bc538e3dc24321229653ec703c859c1c4582f28bf048862",
  "web/src/features/Overview.tsx": "d173f80a2cf0a9769e68334ebce02b4abd8a66a1312e352d700b6aee9a73004f",
  "web/src/features/OverviewHealth.tsx": "b16a6c3d8419eb10196dfd1076062fb6724af08df724cb8384cb79b8e054bb6c",
  "web/src/features/overview/types.ts": "234ad23dab6df9c8ce73b442b19e1964288f16785192883470604b427f2746cc",
  "web/src/features/overview/useOverview.ts": "8334cdbdf009055bc974eed91550e67e56e6467832726910fd398ce39be2668d",
  "web/src/features/overview/overview.css": "a36f1acd997d27efe3f4ad5968ba47749992efdea69f8649093bcde11b90c87c"
}
```

### 核心证据SHA256

```json
{
  "evals/results/p02-02-overview-v2/overview-http-postgres.log": "56c6d951b2b99c93294393f4f57fa589e277441edc59557e7d8018345e5da680",
  "evals/results/p02-02-overview-v2/overview-clippy.log": "5dfbb49df74625da23d58859cc7acabb46c503ca9c359a708a25f93ab28aba1f",
  "evals/results/p02-02-overview-v5/report.json": "c3a185bd1af50d8cb7f815dae7df0ef50664dee51cef99fdf558949b9cc370ef",
  "evals/results/p02-02-overview-v5/browser/report.json": "6689e79f6983f0b6f449930aad127c8a72a62fc2fb7c05dafbc571cc9e9beee0"
}
```

下一依赖：主线复核并选定下一accepted overlay；P03基于命名固定候选执行完整回归并独立汇总。主线独占权威文档与发布判断；P02-02完成后继续接主线安排的独立稳定接口任务，不自行跨入他人领域、改变N1/N2或重开旧P02证据。

P02-02本窗口工程Goal已标记complete；上述READY源码保持稳定。全项目Goal、P03完整候选验收与M1—M5门槛不由此改变，下一独立切片由主线接续分配。
