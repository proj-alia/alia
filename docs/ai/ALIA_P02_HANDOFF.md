# ALIA-P02｜数据工作区与执行追踪

状态：READY_TO_FREEZE / P02 工程交付完成；源码已稳定保存，等待主线/P03选定冻结及完整验收。

## 初始检查点（2026-11-12）

- 任务窗口：01a1cf0b-e8a3-7a72-a261-d7034fb0ce0d；主线：01a1cbe8-e270-75e1-9535-d3f2e0ea81c4（local）。
- Goal 已建立，objective 为 Console v4 公开元数据查询、筛选、稳定分页、执行追踪跟随及真实 HTTP/PostgreSQL/浏览器证据；不设 token_budget。
- 已读取 AGENTS.md、BLUEPRINT.md、CONTRACTS.md、IMPLEMENTATION.md。协调登记 ALIA_PARALLEL_COORDINATION.md 尚未出现；等待文件所有权、SHA256 基线和 P02 released。
- 权威基线 SHA256：AGENTS 79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4；BLUEPRINT 30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943；CONTRACTS b950bc1ca441701a8f643accd443a7bdd9e58fb7f59d7eb8251e3c2ecbf95b6d；IMPLEMENTATION 3d52b085713d2cdd8283959e90bba39c8aab29cdeddb14649fbcac03957c2abe。
- 只读发现：Traces 已有 status/cursor 查询；AuditLog 的 search 仅对当前页客户端过滤；consoleTypes 实际路径为 web/src/shared/consoleTypes.ts，公共读取 hook 为 web/src/shared/useResource.ts。将使用专属跟随 hook，公共注册与全局状态改动交由主线。
- 冻结接口沿用户授权：runs 增 q/mode/from/to，audit 增 q/from/to；q 仅公开元数据、字面大小写无关；时间 UTC 半开区间；未知/重复/非法输入 422；游标绑定规范化筛选且保留旧无筛选兼容；同快照递归阻断。
- 不修改 contexts、runtime、领域/模型/提交、schema/迁移、三份权威文档；不部署、不初始化生产个体、不进行收费调用、不创建 worktree/分支/全仓提交或全仓格式化。

## 命令与结果

- `pwd`、`rg --files`、文档 `cat/sed/wc`、`sha256sum`、代码定位 `rg`：均为只读，退出 0；一次 rg 指定不存在的 web/src/consoleTypes.ts 报错，后定位到 shared/consoleTypes.ts。
- Rust/npm/M0/DB/浏览器：未运行；无 ignored 项冒充通过；模型调用 0。

## 下一依赖

主线登记并释放 P02 文件权限和资源队列；读取登记后持续实施。需新增公共注册或共享 helper 时仅在此交接提出具体整合要求。

## Release 检查点

已读取协调登记：P02 released；六个相关源码/参考文件及两个参考测试的 SHA256 均与登记一致。仅在获准文件中新增专属 helpers，避免新公共模块注册。后续测试需 gate OPEN 及 `/tmp/alia-project-heavy.lock` 全父进程锁。

## 实施中检查点（P02 queries v2）

- 已修改：src/service/api.rs（专属 RunQuery/AuditQuery/filter/cursor helpers）、src/adapters/api.rs（新增 ConsoleFilter/ConsolePosition 与只读 console_list）、Traces.tsx/AuditLog.tsx（服务端表单与专属读取/跟随 hook；hook 暂放 Traces 已获准文件并由 AuditLog 使用，无公共注册）。公共 public_list 和其他列表调用签名未改。
- gate OPEN 后通过同一 flock：cargo check --locked --tests 退出0；专属 rustfmt 退出0；npm build/test 退出0；4项 tests/console_queries.rs 实际 TCP HTTP+隔离 PostgreSQL 全通过（4 passed / 0 failed / 0 ignored），无模型/worker。
- 联合检查v1因默认 PATH 无 npm 在启动前失败（Python FileNotFoundError），v2使用已有 /tmp/alia-go-diagnostic/node-v24.11.1-linux-x64/bin 后通过；日志 /tmp/alia-p02-checks-v1.log、v2.log 保留。
- 检查发现主线文件 src/adapters/model.rs:3 存在 unused import DecisionProposal，普通测试仅 warning；P02 无权改此文件，请主线清理后严格 clippy。不是 P02 测试失败。
- 新测试覆盖组合条件/时间端点/跨页与同刻 UUID 排序、规范化同筛选游标、跨筛选422、旧无筛选游标兼容、字面%/_、重复/未知/枚举/UTC/长度等422、真实鉴权/同源/no-store、递归祖先阻断/隐藏关联ID及原始错误不可搜索与不可回显。
- 浏览器专属驱动仍在编写；完整 cargo/clippy/M0 尚未运行；不宣称整个 Goal 已完成。

## 四 Goal 分工接收与浏览器中间结果

- 已接收 P03（01a1cf17-37ab-7d13-bd07-beb8600a1c02）分工：P02 只继续专属功能与局部测试；完整 Rust/DB/HTTP/浏览器矩阵、冻结与汇总交 P03，不自行重复全套。当前候选 NOT_READY，浏览器 v3 在总锁自然排队；稳定后写 READY 和精确文件/SHA。
- 浏览器 v1：Playwright 默认 headless shell 缺失，未启动浏览器；已创建的合成服务/DB清理通过，报告保留。v2：使用既有 Chromium1234，旧入口与默认关闭跟随/详情链接通过后，下拉框精确label定位失败；已加 aria-label，v3重新验证。
- 字面 q 再核对：只将纯空白规范化为空，非空 q 保留首尾空格，并不将其悄悄裁剪；大小写和 UTC等价表示仍规范化摘要。测试已增加非空首尾空白不命中检查；v2 DB通过后的这项小修订将在下轮局部DB再次验证。
- 专属 web/scripts/test-data-workspace.mjs 内含整个隔离服务/DB父驱动、被冻结临时二进制、72条终态合成数据、真实HTTP；整个调用持 flock。服务环境移除所有供应商密钥/模型配置；不使用生产DB，不调用模型。报告不覆盖旧目录并记录cleanup。

## P03 冻结复用接口补充（自有脚本，无公共注册）

P02 浏览器脚本已增加显式外部冻结模式。设置 `ALIA_TEST_URL` 后，必须同时给 `ALIA_TEST_TOKEN`、`ALIA_TEST_OUTPUT`（全新目录）、`ALIA_TEST_BINARY`（实际冻结可执行文件）、`ALIA_P02_DATABASE`（专用 alia_p02_/alia_p03_/alia_web_test_ 数据库名）、`ALIA_P02_PSQL_COMMAND`（JSON argv，无密码；例 `["sudo","-n","docker","exec","-i","<test-container>","psql","-U","<test-user>","-d","<test-db>"]`）和 `ALIA_P02_LOCK_HELD=1`（父驱动实际持总锁）。CHROMIUM_EXECUTABLE 可显式给已安装 Chromium。

外部模式不读 .env、不编译、不创建/停止服务或数据库，不回退开发产物；先检查loopback和实际current_database、要求服务未初始化，然后初始化/准备72条合成终态数据；服务和DB必须为此用例独占，P03父驱动负责清理。它仍实际HTTP/SQL/浏览器操作，含8秒表锁检验SSE/定时去重。脚本只清理自身浏览器。默认局部模式继续提供完整隔离服务/DB生命周期，并以子进程调用同一个外部路径，因此下轮v5同时验证两种调用边界。清理按浏览器/服务/DB/临时目录独立执行，任一失败不会跳过其余清理。

局部最终Rust检查 `/tmp/alia-p02-local-final-v1.log`：专属rustfmt退出0，4项真实TCP HTTP/PostgreSQL退出0（4 passed/0 failed/0 ignored），严格 `cargo clippy --locked --lib --test console_queries -- -D warnings` 退出0，npm test退出0。新增远祖→中间→事件→run递归阻断、关联ID搜索前后对照和更多重复/非法游标检查均在本轮实际执行。

浏览器v4全部12组检查通过，列表/所选详情各自最大并发1，cleanup=true，模型预留0。已查看四张390/1440截图；随后仅调整追踪表格固定列宽以改善桌面双栏挤压，并新增上述脚本外部模式，v5正在验证最终候选。v1/v2/v3失败报告均保留：缺浏览器、label定位、历史→跟随旧页重复请求；v3问题已通过跨查询串行化并避免先刷新历史页修复。


## 最终边界反例与修复（v5之后）

- v5实际退出0、外部子驱动与父驱动cleanup均true，截图已查看。交接前Unicode反例揭示 Rust `to_lowercase` 与 PostgreSQL `lower` 对大写点I产生不同序列；`İSTANBUL`自匹配返回0，真实专项失败日志 `/tmp/alia-p02-unicode-v1.log`（退出101，1 failed，3 filtered）保留。
- 已在获准的 adapter 新只读helper中统一通过 PostgreSQL `lower` 规范化 q，再用于摘要和同样规则的子串比较；不会扩大搜索字段。此规范化仅处理用户筛选字符串，不读取私有对象；列表来源检查仍在自己的REPEATABLE READ快照内。保留非空q首尾空白。
- `/tmp/alia-p02-local-final-v2.log` 正在总锁下重跑4项真实HTTP/PostgreSQL及专属严格clippy。其后重跑专属浏览器v6，以实际新服务二进制覆盖最终API修复。主线/P03请等待本文件最新READY，不冻结v5之后的半程状态。


## READY — 最终稳定检查点（2026-11-12，UTC见报告）

**READY。上述生产源码和专属测试均已最后保存，不再修改；没有在途编译、服务、测试或模型任务。主线/P03现在可纳入短稳定源码冻结。P02只完成本工程交付；完整候选M0、默认/feature/全DB/旧浏览器矩阵与发布判断由P03和主线负责，未按本交接宣称完成。**

### 最小接口与行为变更

- `/runs`：q/status/mode/from/to/cursor/limit；`/audit`：q/from/to/cursor/limit。闭合serde查询继续拒绝未知和重复参数；时间UTC、from包含/to不包含，默认30/最大100。非法条件422。
- q最多200 Unicode字符，纯空白等于无筛选，其余保留字面空格；由PostgreSQL统一大小写规范化，`strpos`匹配不会把%/_解释为通配符。仅公开ID、允许错误码、事件kind/actor和同快照可见correlation参与搜索，不搜索正文/端点/原始错误。
- v2游标绑定规范化筛选SHA256；limit不绑定。运行按created_at/id倒序，审计按sequence正序；无筛选仍输出/接受v1游标，旧v1不允许跨入有筛选域。新运行游标携带精确微秒位置，避免依赖锚点可见性；旧路径保持兼容。
- 统一REPEATABLE READ只读查询与递归来源阻断；隐藏关联ID先投影为null再搜索，原始错误先投影允许错误码再搜索。
- Traces/AuditLog实际向服务端提交筛选并重置游标；追踪跟随默认OFF，5秒/既有SSE信号合并，隐藏暂停，跨查询也串行化每个列表/详情读取。历史翻页关闭跟随并提示，开启从第一页读取；筛选和所选详情保留，详情不在当前页/筛选时明确说明。列表/详情分别显示实际成功刷新时间及错误；失败不伪更新成功时间。
- `useDataResource`和`DataFilters`仅放在已获准Traces文件内导出供AuditLog复用，没有新增全局状态、公共helper注册、依赖或模块。consoleTypes/Workspaces/style.css未修改；列宽只用Traces局部样式。

### 实际验证与结果

所有重型父驱动实际使用 `flock -x /tmp/alia-project-heavy.lock`；DB `RUST_TEST_THREADS=1`。

- `cargo check --locked --tests` 退出0（早期检查）。
- `rustfmt --edition 2024 src/service/api.rs src/adapters/api.rs tests/console_queries.rs` 退出0；没有fmt --all。
- 最终 `cargo test --locked --test console_queries -- --ignored` 退出0：**4 passed、0 failed、0 ignored、0 filtered**，实际TCP HTTP及每例独立SQLx PostgreSQL。四例覆盖跨页/同刻UUID排序、组合/时间边界、字面与Unicode、规范化摘要与跨筛选422、旧游标、重复/未知/非法参数、认证/同源/no-store、远祖递归阻断、隐藏字段不可搜索/回显。日志 `/tmp/alia-p02-local-final-v2.log`。
- `cargo clippy --locked --lib --test console_queries -- -D warnings` 最终退出0（同日志）；不冒称all-targets/feature完整检查。
- `npm run build --prefix web` 退出0；`npm test --prefix web` 退出0（SSE及组件），日志 `/tmp/alia-p02-checks-v2.log`、`/tmp/alia-p02-local-final-v1.log`；最终UI列宽版再次build退出0。
- 最终浏览器 **v6退出0**：默认父驱动→显式外部子模式，两层报告passed=true/cleanup=true，12组检查、20个运行/详情HTTP请求，各自最大并发1，72条终态合成fixture，模型预留0。覆盖搜索/组合条件、分页/详情保留、默认跟随关闭、隐藏暂停恢复、真实DB慢读与SSE/定时去重、实际离线失败与重试/重连、退出清屏、无浏览器存储、旧traces链接，以及390/1440布局与截图。v6使用最终Unicode修复二进制；UI源码与已目视查看的v5两张追踪截图一致，v4审计390/1440截图亦已查看。无横向页面溢出；移动运行表格保留组件内水平滚动。
- **限制：隐藏状态由浏览器内受控document.hidden覆盖及visibilitychange驱动**，计时器/网络/SQL均真实；未把它称为操作系统后台标签生命周期的独立证明。断线使用浏览器真实offline；没有HTTP响应Mock。
- 完整M0/全Rust/feature/全DB/跨系统旧浏览器矩阵 **本窗口未运行，交P03**。真实模型调用 **0**；不部署，不初始化生产个体，不改变领域运行规则。一般cargo默认会ignored这4项，P03需显式把 `--test console_queries -- --ignored` 纳入实际DB矩阵。

### 保留的失败与已修复问题

- npm默认PATH缺失、Playwright默认安装路径缺失；使用项目已有Node24.11.1和Chromium1234后真实执行，未下载新依赖。
- 浏览器v2下拉框label定位失败，已补明确aria-label；v3慢读检出旧历史页→第一页的重叠请求，已串行化并修复跟随切换。
- Unicode专项v1退出101：大写点I经Rust预小写导致0命中，已统一PostgreSQL规范化，最终4项DB及v6浏览器均通过。
- v1/v2/v3失败报告、v4/v5中间成功及Unicode失败日志全部保留，未覆盖或改判。早期model.rs unused import由主线修复，最终专属严格clippy已无warning。

### 最终文件 SHA256

仅以下六个获准文件有源码/测试变更，其他任务所有权文件未修改：

```json
{
  "src/service/api.rs": "d118017ca504e54507b6415c70f1f63cb33e2efab97286ed653e0738c08987b4",
  "src/adapters/api.rs": "565616351fe2dd4c6c896fb04f40061dc8e9794be5b2664ba8a068a2ce73ded7",
  "web/src/features/Traces.tsx": "08c8b687643a621c7c787c1d73d0d2270f02746d1f249dd07baff9d18b7316bf",
  "web/src/features/AuditLog.tsx": "af11fd50844784a11c1c6198df0a30345b7c64225ad925c5a3274961f373cfd0",
  "tests/console_queries.rs": "1e70bfc5fc628a7c8c3d1232b2d9e8bd06d6e5b1e9149649af0181abd31fa72b",
  "web/scripts/test-data-workspace.mjs": "2be38948f3742de06a943fcb1a9cb4f342201cf345125a144f5b60ded49a8916"
}
```

参考未改文件仍匹配登记基线：shared/consoleTypes.ts=`f1a49fe030ecb2c1fa68b2e631347edb04e4f67dbd9d00b925153b8224b31847`；tests/api.rs=`d5c4ea951e55d47ecae89a4c45d46d0856281b304cf5e71cda48e3d454b74abe`；web/scripts/smoke.mjs=`8cd69aa1068b5df61eb49a1e9f06c47f811236973aa4e4f7a02bc4eb1d9e6c8a`。

### 证据哈希与下一依赖

```json
{
  "evals/results/p02-data-workspace-v6/report.json": "55c66c09e0fd72de74767235a3e90c3013cb460afce1db3e01c37efdb815a826",
  "evals/results/p02-data-workspace-v6/browser/report.json": "1ab4c5cde8bd86c7c3d3303e5b2cde098beb1beea29dbfef113bd4439ad5193f"
}
```

v6报告记录实际服务二进制SHA256及四个生产文件SHA256；截图在同目录。下一依赖：主线选择候选，P03冻结并验证工程矩阵，按上节外部模式输入接入专属浏览器（新鲜独占测试个体/DB、明确SQL argv、固定二进制与父锁），主线独占权威文档更新、接口协调和发布判断。P02不自行进入后续管理业务或M2—M5新切片；后续由主线安排。

P02 持续 Goal 已按本窗口工程 objective 标记 complete；READY 稳定文件不再改动。全项目持续 Goal、候选完整验收及M1—M5状态不由此改变；下一任务等待主线安排。
