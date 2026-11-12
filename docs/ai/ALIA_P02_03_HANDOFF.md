# ALIA-P02-03｜owner记忆纠正与历史WebUI

状态：READY（局部工程验收完成，完整候选回归交P03）。前两P02 Goal及accepted包完整保留。

- 已读取AGENTS、BLUEPRINT全文；以本窗口已完整读取过的N2权威文档为基线，逐行核对当前CONTRACTS/IMPLEMENTATION所有差异及最新协调冻结接口，未用旧聊天猜接口。
- get_goal返回null（前项complete历史仍在），已create新P02-03持续Goal，不设token_budget。
- 修改前Memory/App/nav SHA均与主线登记逐字节一致。只修改这三处指定前端区域、新features/memory/**、专属web/scripts/test-memory-revisions.mjs及本交接；不改后端/共享repository/state或旧accepted证据，不开放删除。
- P04已有GET详情强ETag与POST201/新ID回执；history使用主线冻结P04-02接口（实现尚在进行）。将先写实际前端，不等完整工程；正式历史通过仍须真实后端。
- 主线仍唯一接口/发布者，完整工程P03。所有局部重型/浏览器父任务继续总flock，0模型、不部署/生产初始化。

## 输入SHA256

```json
{
  "AGENTS.md": "79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4",
  "docs/BLUEPRINT.md": "30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943",
  "docs/CONTRACTS.md": "1f85053c625e0c5fb9d594741d57bb485e7c087386b0581c34042c750407526a",
  "docs/IMPLEMENTATION.md": "936f6827e65e3d3a90cc00ae300ea435f457d6a5ec2d6ef05bf5f130b620f7af",
  "docs/ai/ALIA_PARALLEL_COORDINATION.md": "6db439fb6ed0d0f429931a40c4bd0c6a7ea7adff4a993681dbeaeb6a043c2fd6",
  "web/src/features/Memory.tsx": "e30663930c62bf0b45afcd6dd470d8e75ef69af8925493efca475583a9dd300c",
  "web/src/app/App.tsx": "fba0f4869684f0b1809eb6197bd1e5b50c4b87ce1a15bd8e44924c4484b9a409",
  "web/src/app/useConsoleNavigation.ts": "12c4d6e3a69326f256b0b91fca653020fc118799d718c295e75b044b3dfe9430"
}
```

## 实施约束与准备

- 详情必须读取实际ETag，不能拿global state.version当If-Match；POST必须收到HTTP201和验证后的完整receipt才成功。单次意图content/scope/reason/目标版本与稳定key绑定，未知结果只重放同意图，不编辑成新的隐式请求。
- 409保留草稿/原基线，读取最新对照；任何采用新版本/当前链节点须用户明确重新预览，不自动覆盖。
- 资源选择/工作区切换与路由共用现有useConsoleNavigation一个pending流程，扩为可保护本地动作；App只扩展现有未保存弹窗与Config原流程，不增加第二组hash/beforeunload监听。
- session/state/SSE失效时隐藏未重新验证的详情/历史和弹窗私有预览；404或退出清除草稿/意图；失效响应不得回填。所有凭据和草稿只在内存。

当前命令仅只读及本交接写入，尚无新构建/浏览器通过声明。后续持续实施与真实局部验证；必要跨owner请求只写本交接。

## 实施与局部验证进度

已落地真实单记忆ETag读取、正文/理由与不可变scope/kind预览、201完整回执、新ID跟随、409保留基线与显式carry、未知结果锁定原意图重放、分页修订历史。Memory选择/页签/检索与App路由共用原useConsoleNavigation一个pending；Config路径保留。session/state/lastSyncedAt失效隐藏旧详情/历史与弹窗预览；404与退出清空私有草稿/意图/回执。

首轮`evals/results/p02-03-memory-v1/report.json`：node语法、npm build、npm test、cargo build均exit0；浏览器fixture SQL在进入UI前失败（UNION常量被推为text而非uuid），未把浏览器计为通过。父任务exit1，浏览器与服务/私有数据库资源清理均成功。已在自有测试补明确uuid转换并保留原报告，v2仅重跑实际浏览器所需构建与真实隔离资源，不重复无改动旧测试。P04历史路由已由主线注册，P04-02自身14个HTTP/PG用例READY不替代本UI验收。

v2：真实201首组通过，但分页测试发现历史面板随详情重新验证卸载，当前cursor丢失；修复自有MemoryInspector为保留历史组件，其独立useMemoryRead仍按session/version/sync隐藏私有内容。v2父任务exit1与清理通过，保留证据。

v3：`evals/results/p02-03-memory-v3/report.json` passed=true，浏览器13组全通过、父任务exit0，service/database_resource cleanup均true。包括真实201/409/503事务回滚、已提交响应丢失后同key重放、统一guard/native beforeunload/前后退、Config保存/取消/放弃/409回归、两代来源撤销、light/dark360/390/1440与键盘、退出时迟到201清屏，0模型usage/runs。已检查360 light与1440 dark截图，未见溢出。最终证据继续v4，补历史断链重试、离线私有预览与在途旧history200隐私竞态；不把待运行项提前写通过。


## 最终READY｜v4实际结果

- 父任务：`ALIA_MEMORY_RUN=p02-03-memory-v4 ALIA_MEMORY_BROWSER_ONLY=1 flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-03-local.py`，exit0。所有初始化/构建/真实服务/浏览器/子进程退出/资源清理均在同一总锁内；无已有Compose/.env/生产数据复用。数据库单线程约束保留，本任务无新Rust DB suite。
- `node --check web/scripts/test-memory-revisions.mjs` exit0；`npm run build --prefix web` exit0（TypeScript与Vite）；`cargo build --locked --bin alia` exit0。冻结私有二进制与web/dist启动真实HTTP服务，不把并发主线源码称为不可变完整候选。
- `node web/scripts/test-memory-revisions.mjs` exit0，**15组全部通过**，实际PG17/pgvector隔离实例，parent/browser passed=true。真实GET/POST/history/ETag，不伪造成功响应；网络故障只中断或延迟真实HTTP响应，503来自真实PG事务触发器回滚。已检查DB原事件/旧正文保留、幂等链只增加一次、失败无状态/事件变更、usage_reservations和runs均0。
- 新增历史网络失败清空旧页/原页重试；离线隐藏编辑器与统一guard私有预览，禁用保存，在线重新验证后保留内存草稿。真实旧history200在两代来源撤销后迟到，未恢复记录/理由/私有弹窗。
- Config空数字草稿、save/discard/cancel、back/forward、真实409全部保留。记忆与Config只用原navigation一个hashchange/click/beforeunload及一个pending Dialog，没有第二组路由守卫。
- light/dark × 360/390/1440六张最终截图均已逐张目视复核，自动无横向溢出；Modal焦点回环/ESC通过。无删除按钮；无uncaught browser error。
- 最终源码完成后额外`flock -x /tmp/alia-project-heavy.lock env PATH=/tmp/alia-go-diagnostic/node-v24.11.1-linux-x64/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin npm test --prefix web` exit0；SSE/组件/原导航合同通过。日志保存在v4/final-web-tests.log。
- 独立服务正常退出、私有DB容器/volume清理均true；验证后逐字节核对全部12个自有源码与父/浏览器报告一致。p02-02 accepted manifest及九份源副本完整不变。失败v1/v2与成功v3/v4证据均保留，不覆盖。

v4证据：
- `evals/results/p02-03-memory-v4/report.json`，SHA256 `ce46275feee83d38425f9f979196d435ce94dc2f4f04cea44579d813bc2668e1`。
- `evals/results/p02-03-memory-v4/browser/report.json`，SHA256 `db5926ab4e318786cc28602d1a8ea026bbae003b3e93a9e5ad0916d0d4c0d1cb`。
- `evals/results/p02-03-memory-v4/final-verification.json`，SHA256 `a5f27d47ccf3c3ee2ec7d4770678cb04bdc28fe6d6d23475f2a58bb87436b28c`。
- 最终实际二进制 SHA256 `4ec72ba8241458efe4a94d04f12cd35e40a13be24967cea9fd6645f44ce445f2`；仅本地浏览器验证，不作为可部署候选。

## READY文件SHA256与范围

| 文件 | SHA256 |
| --- | --- |
| `web/src/features/Memory.tsx` | `b0faa7c085e7adf4641ffd50ea9e074f51171db7759c4101bb3c4cfec98fbffd` |
| `web/src/app/App.tsx` | `e0313e99551de77db526d274feb690ad74738916ef3ee78630102296cbc504c9` |
| `web/src/app/useConsoleNavigation.ts` | `fb6addcf552a9ab566e47a4f60e7721e27e016c78c3efd79b50a84f0961c79f5` |
| `web/scripts/test-memory-revisions.mjs` | `2b1db2c8cdc1cbed80c04ffe86071e0a9690807c07e509959cc29dfcfaac16bb` |
| `web/src/features/memory/EditorGuard.tsx` | `87de1147931b72ed5ca9c199550c59f3af13fabbcaf95713c5f4a1be6a43701e` |
| `web/src/features/memory/MemoryHistory.tsx` | `3883b6d5bdfb339ce586f2ed3a7b5abbcd5dc66457aafeb30d4174e3ad5b6b1e` |
| `web/src/features/memory/MemoryInspector.tsx` | `c3263ec4919e62ee566878a58dc8f4a7253e3b909dea558ed6a09e0ebb7026cd` |
| `web/src/features/memory/RevisionPreview.tsx` | `9de5353dd2ef881d537722c321d95dcbbd90b34c9dbc76995df9777926ace83b` |
| `web/src/features/memory/client.ts` | `7337534833adff6c580fff15447efbe982f73820cb04b00021f1ccff4d496e48` |
| `web/src/features/memory/memory.css` | `9df9e91d9814beaca79e5a7e7ffeedb61459533c6e750978ef564499c513b537` |
| `web/src/features/memory/types.ts` | `2d53477a6ffe5e1ae3ae4f13a2ba66c5610e5ac7c3add20604c71e9a47cd84ba` |
| `web/src/features/memory/useMemoryRead.ts` | `7fda433a73b8136290976b123f86e78d9dba76f4325ae92067f754c3127c2bec` |

App.tsx仅注册memory guard并扩原离开弹窗；useConsoleNavigation.ts仅在原Pending增加本地action/request，原history恢复流程保留。其余编辑均在指定Memory及新memory目录/专属脚本；后端、共享repository/types/state、Settings/notes/documents/overview、schema与迁移均未由本窗口修改。交接文档本身不纳入其自指SHA。

## 下游依赖与明确限制

主线作为唯一接口/集成协调者，按上述SHA冻结本12个前端/测试文件，并连同P04-01/P04-02实际后端+history注册交P03完整工程。P02只报告本15组真实浏览器局部验收，不把P04的14项测试或任何未跑完整矩阵算成本窗口通过。不改写M1—M5权威进度、不宣称M2/行为质量或14天观察完成。0模型调用、不部署、不初始化真实个体；私人草稿/令牌仅内存，无local/sessionStorage。

专属脚本由资源父任务提供显式`ALIA_TEST_URL/TOKEN/OUTPUT/BINARY`、`ALIA_P02_DATABASE/PSQL_COMMAND`和`ALIA_P02_LOCK_HELD=1`，数据库须全新且生成名匹配允许前缀。脚本不自行构建或启停容器、不读.env、不接管其他服务；父任务负责锁、实际二进制/web固定、认证注入与独立清理。历史每页是各自服务端快照，不承诺跨页固定全链；删除/待复核写操作仍未开放。

普通`git status`因共享目录属主不同返回dubious ownership，未更改全局Git配置；只读核对使用`git -c safe.directory=/workapps/project-alia/alia status --short`，仓库现状整体untracked，不以Git diff伪造独占改动证明。
