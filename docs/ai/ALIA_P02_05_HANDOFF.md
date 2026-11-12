# ALIA-P02-05｜生命周期记忆删除与鉴权进度

状态：**READY（实际在线删除定向验收通过）**。最终固定源码与证据见 `evals/results/p02-05-full-v5/ready.json`，SHA256 `189ec29788376f7fbd2c9e0aec3ac44a5800ad7380a7b7ed41e6f0e32f2dfe9a`。本节覆盖下方历史记录中的 DEV_READY / HTTP_PENDING 状态。产品源码沿用已固定开发版本；接通实际 API 后仅加强本任务验收脚本。P02-06 已交五文件逐字节未变，未修改其切片。

## 最终改动与范围

新增 `features/privacy/**` 的严格 DTO/client/状态机、实际就绪及鉴权进度、App 级单个 provider；Memory/Inspector 只增加真实删除入口及私有视图屏障。沿用原来的唯一 dirty guard，取消/放弃/先保存后删除均经过原流程；App 只包装 provider，未修改 Config、导航或 shared 模块。

DELETE 无 body，以目标 memory.version 强 If-Match 和唯一幂等 key 固定意图；202 仅受理。unknown 不自动重发，刷新与再认证后只能由用户显式使用同 key/target/etag 重放。localStorage 仅保存绑定个体的 opaque 操作 ID 和无正文待定请求元数据，不保存 token、正文、理由、草稿或缓存完成状态。GET 的实际认证响应才更新四阶段进度。readiness 缺失、失败或任一布尔 false 均禁用删除。

本窗口只编辑下列 15 文件（包括 root 明确授权的两个旧脚本入口边界与 0 DELETE 监测）；后端、schema、共享类型、Config、导航、原 accepted 副本未编辑。

| 文件 | 最终 SHA256 |
| --- | --- |
| `web/src/features/Memory.tsx` | `576caaff7e875e6e1e592d101706da6d03ee2ead803b5cb26e700fe13d90a496` |
| `web/src/features/memory/MemoryInspector.tsx` | `def38f0482968338021366827f2a3e3b387396a698092dbdf16d1ae4bb8418d3` |
| `web/src/app/App.tsx` | `e9d8320a5571b9dd192c628566ca8391baf62b3b83f861093b7cded68ad0d467` |
| `web/scripts/test-memory-deletion.mjs` | `18597ea6fa3f07cd029cc13d32fbf9f1a998bd48f41fe103272e9c3e28458133` |
| `web/scripts/test-memory-revisions.mjs` | `54ad66d61cc44c5a8841cbe63d1b6681b6addabe1a29f26ebd8f20b1ac330bd8` |
| `web/scripts/test-source-inspector.mjs` | `8250d538246dfa50046c674431e34a10f92ed82051a8be9cbde39f7c01e3953c` |
| `web/src/features/privacy/PrivacyContext.tsx` | `fe84a13ff9ed1d6cfe50432b3e184ab718ba6d12c240f23b3f896153f0f05e56` |
| `web/src/features/privacy/PrivacyPanel.tsx` | `48341816e4fcd6c9b11ddec6968edddf8b713c5373785992d64140fe43fc7520` |
| `web/src/features/privacy/PrivacyProvider.tsx` | `03cf9cf629111c72938cdc8247abeed9fa93dc4b7cc4f25e88dac68bf7792c66` |
| `web/src/features/privacy/client.ts` | `8d3fed19db9dc5e22b2093f4f9e01fc16c56cfac74f94014ab04f3c508616509` |
| `web/src/features/privacy/privacy.css` | `6fcfad1bf24eb59e53716a3f79b46e1cd1dc061e746a58a044ecbe6c7d445633` |
| `web/src/features/privacy/recovery.ts` | `c796ef70801c8d62cf126536dbadec57cd6549864df5202af38a4ae4c08230b3` |
| `web/src/features/privacy/stateMachine.ts` | `103ea67f55ee92e1f388ed374139c00c0226d122bf41d0e6226a9704e306f82b` |
| `web/src/features/privacy/types.ts` | `95674f01d1f200d3dcdb9957f2e01427e64a9e7f2dc5cb2614ec90fcb5979eca` |
| `web/src/features/privacy/usePrivacyRead.ts` | `a98842962066dce1ddfda712dee26441bd7c3240860a51a65a9658d91097242a` |

## 实际命令与结果

最终父任务：`ALIA_PRIVACY_FULL_RUN=p02-05-full-v5 ALIA_PRIVACY_HTTP_MANIFEST_SHA256=893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72 flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-05-full.py`，**exit 0**。可审阅父脚本副本在 `evals/results/p02-05-full-v5/run-parent.py`；控制器副本在同目录 `service-control.py`。固定 RootHistory381 + root-privacy-http-v2 的 182 源 overlay + 本切片源码，共 408 个源文件，未借 live 未验后端。所有重型阶段从构建到子进程、控制器、QA 数据库清理连续持锁；RUST_TEST_THREADS=1，独占 Cargo 缓存，私有 binary/web/journal 及全新 QA 数据库，0 模型、无部署或生产个体初始化。

| 检查 | 实际结果 |
| --- | --- |
| node --check web/scripts/test-memory-deletion.mjs | exit 0 |
| node web/scripts/test-memory-deletion.mjs --offline | exit 0；57 项合成合同/渲染检查，不计 HTTP 验收 |
| npm run build --prefix web | exit 0；TypeScript + Vite |
| npm test --prefix web | exit 0；原 SSE、组件、13 路由与 Config 合同 |
| cargo build --locked --bin alia | exit 0；固定输入构建 |
| node web/scripts/test-memory-deletion.mjs | exit 0；15 组实际 HTTP/PG/browser，四状态均真实观测，0 uncaught browser errors |
| node web/scripts/test-memory-revisions.mjs | v3 exit 0；完整原 15 组，0 DELETE |
| node web/scripts/test-source-inspector.mjs | v3 exit 0；完整原 16 组，0 DELETE |
| .venv/bin/python3 scripts/check_phase0.py | 当前工作区 exit 0；6 schemas/68 negative cases 等静态检查，非产品验收 |

旧 15/16 回归位于 `p02-05-full-v3`：与最终 v5 的产品代码及两个旧测试逐字节相同；差别只有专属删除验收脚本。其原全部 case 均保留，实际覆盖 Config 空值/save/discard/cancel/409/back-forward、记忆 201/409/503 rollback/同键未知重放、source 隐私屏障、keyboard 和深浅 360/390/1440。

## 最终实际证据

| 证据 | SHA256 |
| --- | --- |
| v5/report.json | `03b5e6e12b539f0a957923c21668916fb1d32f2e49808b99e7817a1f3d6eaa9a` |
| v5/source-manifest.json | `3b02b37f8d777d26284d06b19bd5f6e110ae58e029b6eb4f2ba1356ec429e481` |
| v5/browser/report.json | `08637f951f988813e56b9827e876348da20b5ee9eaf3f478714afc51c4552a95` |
| v3/legacy-memory/report.json（15组） | `7d7e72267a2b649092679fc213d185d517c3e04ba90f92fedc2b3ca21597562b` |
| v3/legacy-source/report.json（16组） | `2b753e34c7a6da106ba712722c0f4d979ea1b172849678b62292e9b04e232039` |

以上 v5/v3 均位于 `evals/results/p02-05-full-*/`。最终 source_stable=true、browser cleanup=true、所有 service/controller/database cleanup=true；四次服务启动和三次显式重启均由私有父控制器执行，退出均正常。8 条合成 journal 记录只含规定 ID/版本/摘要键，唯一 operation 8 个，不含正文。六张完成态深浅 360/390/1440 截图及实际 purging、retry_pending 两张截图均已目视核对；无水平溢出，完成与未完成含义清楚，无虚构百分比或撤销。

15 组实际检查包含：逐 memory If-Match（global 版本不同）、无 body 202 非完成；刷新和实际服务重启后 GET 恢复且不再 DELETE；真实 202 丢响应后跨页面刷新同意图重放并去重；保存更正后重新读取原目标 version2；实际 422/409/401；真实 PostgreSQL 受理故障使 fsync 日志已落盘但 DB 未提交时 503/unknown，实际重启 verified replay 后显式原键恢复；真实 purge 故障前用 pg_sleep(6) 保持可观测 purging，再得到 retry_pending 静态码，实际重启后 completed；未知操作 404 不当作完成或保存的已验证 ID；另一个同源目标实际 DELETE 清除已加载草稿及 dirty 预览，迟到真实来源 200 不能复活；真实受理 202 在退出后迟到不恢复 private DOM，再认证显式原键恢复同一 operation；全程 usage_reservations=0。

## 失败、限制与下一依赖

保留 v1 exit1（第三次正常重启后的测试 GET fetch failed；关闭测试客户端连接复用后不再复现，原因为推断而非定论）。v2/v3 已完整通过；v4 四阶段组通过，但之后隐私检查失败：等待的是此前遗留“已清除”提示，再固定睡 400ms，未等待当前目标实际失效。v5 改为先等当前 guard hidden 与当前目标/正文/理由实际消失，再释放迟到真实响应，保留同样“不复活”断言并通过。未据此改产品或降低隐私标准，v4 失败报告和截图不改写。系统 python3 的 M0 首次 exit1 因缺 jsonschema，随后使用已有 .venv 的固定 4.23.0，实际 exit0，两个日志均保存。

范围仅本地在线数据。冻结后端尚未启用受管备份/导出包，未宣称这些路径或已下载副本、云端提供方撤回通过；无真实模型行为、全工程矩阵、14日观察或发布验收。Root/P03 下一依赖是按 ready.json 取精确源码，与 root-privacy-http-02 / P05-05 / P02-06 等切片组装不可变候选后执行受影响整合及唯一完整矩阵；本窗口不代写主 IMPLEMENTATION/接口所有权，不向其他聊天发消息。

## 历史开发记录

以下各轮“未接通/未就绪/内存恢复”等描述仅记录当时状态，最终设计和验收以本节及 ready.json 为准；旧证据不被覆盖或转作新版本通过。

## 接口先行源码已保存

privacy/types.ts、client.ts、stateMachine.ts已保存。DELETE无body，Idempotency-Key绑定target/逐memory.version强ETag，非global版本；客户端仅202接受回执，4status只认blocked/purging/retry_pending/completed。GEToperation需实际认证/observed_at/静态last_error，无进度百分比/超时推断完成。缺management.privacy任一实际bool或读取失败均disabled；生产不Mock成功。

unknown保持原body-free key/目标/ETag/个体绑定，不允许换意图；202与进度必须验证实际individual/operation/target。记忆删除从已有单dirty guard进入后重新读取原点击目标的实际ETag，再由用户最终确认；若先保存更正，仍重新读原目标，不能用旧版本直接DELETE。来源/派生范围与无法撤回设备副本/云端数据会明确展示，不伪造闭包数量。

恢复设计：opaque操作ID保留于App级内存，token/正文/草稿不持久化。断线/退出清除回执与私有显示，重新连接同个体后须认证重新查询；浏览器刷新/服务重启后可输入已保存操作ID恢复实际进度。未知请求的body-free意图仅内存保留、重新认证后由用户显式同key重放，无自动DELETE。若root要求自动跨浏览器重启恢复，其唯一允许持久化项需另精确指定，本窗口不保存token/私有草稿或将DELETE重试作为自动化。

## 当前依赖与验证

root/P05真实DELETE、独立journal/epoch任务和GET privacy-operations仍未连接，management当前没有privacy字段；本窗口先写可用UI、纯状态机与实际未就绪浏览器检查，再在服务接通后跑独立固定源/真实PG/HTTP/browser。所有重型父任务连续flock到清理完成，DB线程1；未运行不记通过。主线唯一接口/发布协调者，必要跨owner缺口只记录本交接，不向其他聊天发消息。

初始SHA：
```json
{
  "AGENTS.md": "79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4",
  "docs/BLUEPRINT.md": "30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943",
  "docs/CONTRACTS.md": "5a0aa89decfce78d40a80d79e022a7a076170c0721bdaa9067ac7a8d5a39d8e2",
  "docs/IMPLEMENTATION.md": "4fdbf14a0168dd34db7857cb7d6432c38d05f7ad129f7e9d08bba8f9462792cd",
  "docs/ai/ALIA_PARALLEL_COORDINATION.md": "0938c231d5a11fadd923d717109278e3631a09a67afc5263e230c5339e470525",
  "web/src/features/Memory.tsx": "b0faa7c085e7adf4641ffd50ea9e074f51171db7759c4101bb3c4cfec98fbffd",
  "web/src/features/memory/MemoryInspector.tsx": "47576efd661958ce14e9b323170d7d5d6f1511a3fca0e19554026a83e5f05a4a",
  "web/src/app/App.tsx": "e0313e99551de77db526d274feb690ad74738916ef3ee78630102296cbc504c9",
  "web/src/app/useConsoleNavigation.ts": "fb6addcf552a9ab566e47a4f60e7721e27e016c78c3efd79b50a84f0961c79f5"
}
```

## 第一轮实际准备检查

`p02-05-readiness-v1/report.json`与browser/report.json仅scope=pre-API readiness-only，父exit0、passed/source_stable/浏览器cleanup/service/database清理均true。使用已校验N6-base373源（assembly c5e45965abaaa2aafd80ca1fec4d20656f216afdad32765a4ddb055a72b2cadc）+本窗口前端，未借live核心/新增迁移25或在途API。新node语法/36项合成状态机header校验/npm build/npm test/cargo独占build全部exit0。真实PG/HTTP浏览器4组：实际缺management.privacy时目标删除按钮disabled且0DELETE；进度能力缺失禁用认证查询/无完成百分比；既有记忆草稿cancel/discard和Config空值/cancel/save守卫正常；退出清隐私UI与存储/令牌。全部明确不是删除已接通或完整生命周期通过。

当前前端已保存PrivacyProvider/Panel/context/read/session/types/client/stateMachine/CSS及三处最小接入，App只增加provider包装，不改原navigation/dirty/Config分支。delete最终确认/进度/未知意图重放已编码。代码复核补重新认证后认证进度可恢复私有视图的快照读取；management仅Memory页读，避免无关页面重复调用。增加拒绝0epoch及completed+非空last_error矛盾回执；这些最终变更正在新一轮定向检查，不提前记通过。

真实依赖仍是root DELETE和GET privacy-operations及完整management.privacy，当前路由尚未保存；P05 journal/closure与root0024迁移已看到，未视作API已运行。需要服务实际返回202/四phase与故障/恢复后才运行并计完整浏览器。旧accepted脚本test-memory-revisions.mjs与test-source-inspector.mjs中“任何删除按钮数量0”的历史边界将被本任务有意改变；未修改这些受保护旧文件，后继candidate QA须按root授权处理该精确边界，不能直接用旧断言补判新UI失败或删去整个旧回归。

v2准备检查已实际通过：`p02-05-readiness-v2/report.json`仍是pre-API readiness-only，node语法/38项纯状态机/npm build/npm test/固定N6-base独占Rust构建/4组真实未就绪浏览器与全部清理exit0。该版本已包含重新认证后实际progress+刷新快照恢复视图和202明确“受理非完成”提示；不包含待root接通的真实DELETE验收。

代码复核发现409/422后调用refresh可能被并行SSE刷新取代而返回undefined，因而私有fence保持；正在自有Provider补“拒绝之后确有完成的新快照才解开隐藏”的lastSyncedAt屏障，不用未验证旧缓存恢复内容。完整浏览器将覆盖真实外部revision造成409、503事务/日志故障、401重新认证、202丢响应原key重放、服务重启与job恢复。actual full模式在API尚未接通时会失败而非soft skip，绝不将readiness-only passed算本Goal完成。

## 刷新恢复补强（覆盖上文最初仅内存的选择）

为完整满足“未知请求同意图重放”和“刷新/重启恢复”，当前改为最小非正文浏览器恢复元数据：features/privacy/recovery.ts仅持久化当前individual绑定的pending请求五字段{individualId,targetId,memoryVersion,etag,key}及已取得202或已认证GET200的operation ID。各请求/操作独立key，不以一个数组覆盖并发标签页的数据；保存pending成功后才发DELETE。始终不写token/正文/草稿/原因/进度status，已完成也必须重新认证GET，不能由本地缓存推断。退出/离线仍清当前视图和回执，auth后按individual隔离恢复；原请求只由用户显式重放。多个待定标识都保留，UI先完成当前未知意图再显式恢复下一条。

上文“仅内存、需手动输入ID”只保留为初期设计记录，现已被此恢复实现覆盖。原accepted测试不发DELETE/不查询本新面板，因而仍不产生存储写入；新生命周期测试严格只允许alia.privacy.pending.v1/operation.v1元数据命名空间，并检验无凭据/正文/完成状态。纯状态机/只存标识/跨owner隔离/多个独立pending/拒绝同key改目标/拒绝正文与未知字段的新增检查已编写，尚不代替root真实API与durable journal验收。

v3未就绪父任务已终态exit0，仍固定N6-base+本前端，38+projection渲染共47项当时纯检查及原4组真实未就绪/guard/browser/清理通过。此后的恢复元数据版本需重新定向验证；不把旧报告转用于新源码。完整API分支当前未运行，状态保持NOT_READY，Goal active。

最终开发固定v1未通过：新增真实management断链后重试定位器把Panel的panel-head误当整个Panel，waitFor超时；截图显示实际错误状态与disabled删除入口正确。父exit1、两项前置浏览器检查通过，service/database清理true，旧两个browser尚未到达，不能记通过。已只修正专属定位器至实际section.panel，并把网络失败提示固定为中文静态消息，避免直接显示浏览器TypeError。按root最新分派，最终fence/readiness/旧15+16回归通过后保存DEV_READY源码；05完整HTTP验收仍排队，Goal不complete/pause，06接为唯一coding项。


## 最终开发就绪交接｜切换06为唯一coding项

按root最新分派区分开发与验收：本05实现与完整专属脚本已保存，当前不再占coding；真实DELETE HTTP gate留队列，root/P05 API稳定后返回执行。旧检查未删除，Goal不假complete/pause，06接为唯一ACTIVE CODING。

最终父任务 `ALIA_PRIVACY_RUN=p02-05-development-final-v2 ALIA_PRIVACY_LEGACY=1 flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-05-final-dev.py` exit0。输入为已逐字节验证N6-base373源加本15个前端/测试overlay，独占P02 Cargo缓存/私有复制binary+web、三个全新QA库/逐个独立服务、总锁一直保持到全部子进程与资源清理。未接入live未完成privacy runtime或迁移26，也未运行全矩阵。

实际命令均exit0：node --check；`node web/scripts/test-memory-deletion.mjs --offline`（**57项合成纯状态/头/回执/恢复元数据/投影渲染**，0 API/model）；npm build（TypeScript+Vite）；npm test（原SSE/组件/导航Config合同）；cargo build --locked --bin alia；`node web/scripts/test-memory-deletion.mjs --readiness-only`（**5组真实HTTP/PG/browser**）；原test-memory-revisions.mjs（**15组**）；原test-source-inspector.mjs（**16组**）。全部脚本都使用固定实际HTTP/数据库，不伪造成功payload。三个浏览器passed/cleanup均true；service逐个正常退出、own PG resource cleanup均true，source_stable=true。此scope只证明开发就绪与原回归，绝不证明DELETE生命周期已通过。

最终5组真实未就绪检查包括：实际management无privacy使真实目标入口disabled且0DELETE；实际status能力控制恢复，未得完成/百分比；management真实网络断链禁用与显式重试实际读取；原memory草稿cancel/discard与Config空值/cancel/save；退出清privateUI/无令牌或私有存储写入。旧15/16完整原case保留，仍包括201/409/503rollback/同key未知重放/source privacy/Config409/前后退/dark/3603901440/keyboard，且新增0DELETE监测。没有用“已有按钮”误判有意的新禁用入口。

### 精确证据

- 父报告 `evals/results/p02-05-development-final-v2/report.json` SHA256 `7dc0c1e709a1b9b125f221b443975a4933371888b9a3a64c643cfb56021a19e8`。
- 完整固定输入 `evals/results/p02-05-development-final-v2/source-manifest.json` SHA256 `13a55a975aa0676bd75aa480448f78be84c8405ece2563ec545812d9118b0689`。
- 开发就绪清单 `evals/results/p02-05-development-final-v2/development-ready.json` SHA256 `d854a1300a38d2ba0f9a45331c657071bd79577f2ea5bbbc7d29c581891ae787`。
- 未就绪浏览器 `evals/results/p02-05-development-final-v2/browser/report.json` SHA256 `00e968610d3453011b2ed13c5ecd741255909ebb64c9f1322318185b34260e7f`。
- 原记忆15组 `evals/results/p02-05-development-final-v2/legacy-memory/report.json` SHA256 `1d2aa4e44fcda89ae9b4548a8481705410e72d66d3dd727b3261d330c2adb7c7`。
- 原来源16组 `evals/results/p02-05-development-final-v2/legacy-source/report.json` SHA256 `9445f1cac2475906b318bfa130c6a8e8629e5dd4583d170b7c43d7797b515901`。

### 15个固定开发源码

完整冻结副本都在上述`source/`同路径内，后续06不覆盖这些文件；主线可按表接入。表内包含root额外授权的两个旧测试最小边界修正，未修改旧accepted副本。

| 文件 | SHA256 |
| --- | --- |
| `web/src/features/Memory.tsx` | `576caaff7e875e6e1e592d101706da6d03ee2ead803b5cb26e700fe13d90a496` |
| `web/src/features/memory/MemoryInspector.tsx` | `def38f0482968338021366827f2a3e3b387396a698092dbdf16d1ae4bb8418d3` |
| `web/src/app/App.tsx` | `e9d8320a5571b9dd192c628566ca8391baf62b3b83f861093b7cded68ad0d467` |
| `web/scripts/test-memory-deletion.mjs` | `0f273bf25907d2422fc29aae9c0679570c7d71592f15f32c73570045ef74828e` |
| `web/scripts/test-memory-revisions.mjs` | `54ad66d61cc44c5a8841cbe63d1b6681b6addabe1a29f26ebd8f20b1ac330bd8` |
| `web/scripts/test-source-inspector.mjs` | `8250d538246dfa50046c674431e34a10f92ed82051a8be9cbde39f7c01e3953c` |
| `web/src/features/privacy/PrivacyContext.tsx` | `fe84a13ff9ed1d6cfe50432b3e184ab718ba6d12c240f23b3f896153f0f05e56` |
| `web/src/features/privacy/PrivacyPanel.tsx` | `48341816e4fcd6c9b11ddec6968edddf8b713c5373785992d64140fe43fc7520` |
| `web/src/features/privacy/PrivacyProvider.tsx` | `03cf9cf629111c72938cdc8247abeed9fa93dc4b7cc4f25e88dac68bf7792c66` |
| `web/src/features/privacy/client.ts` | `8d3fed19db9dc5e22b2093f4f9e01fc16c56cfac74f94014ab04f3c508616509` |
| `web/src/features/privacy/privacy.css` | `6fcfad1bf24eb59e53716a3f79b46e1cd1dc061e746a58a044ecbe6c7d445633` |
| `web/src/features/privacy/recovery.ts` | `c796ef70801c8d62cf126536dbadec57cd6549864df5202af38a4ae4c08230b3` |
| `web/src/features/privacy/stateMachine.ts` | `103ea67f55ee92e1f388ed374139c00c0226d122bf41d0e6226a9704e306f82b` |
| `web/src/features/privacy/types.ts` | `95674f01d1f200d3dcdb9957f2e01427e64a9e7f2dc5cb2614ec90fcb5979eca` |
| `web/src/features/privacy/usePrivacyRead.ts` | `a98842962066dce1ddfda712dee26441bd7c3240860a51a65a9658d91097242a` |

旧测试授权变化（仅入口边界与DELETE监测；原全部case数量保持）：
- `web/scripts/test-memory-revisions.mjs`：`2b1db2c8cdc1cbed80c04ffe86071e0a9690807c07e509959cc29dfcfaac16bb` → `54ad66d61cc44c5a8841cbe63d1b6681b6addabe1a29f26ebd8f20b1ac330bd8`。无实际readiness时若入口存在必须disabled（旧无入口候选仍兼容），所有原编辑/来源工作流一律0DELETE；实际能力全true时不把新业务入口存在视作旧功能失败。
- `web/scripts/test-source-inspector.mjs`：`e42b5a3a042286c9578582b6e9464fad365a08e024194886ddeb8be6e655c717` → `8250d538246dfa50046c674431e34a10f92ed82051a8be9cbde39f7c01e3953c`。无实际readiness时若入口存在必须disabled（旧无入口候选仍兼容），所有原编辑/来源工作流一律0DELETE；实际能力全true时不把新业务入口存在视作旧功能失败。

### 真实完整DELETE验收尚未运行（不可补判）

- 实际202/逐memory If-Match/无body，以及blocked→purging→retry_pending→completed的真实job状态与清理后来源不可读。
- dirty guard保存/放弃/取消后的实际删除，保存更正后重新读取原目标版本2。
- 实际409、401重新认证、422、503/fsync后DB失败的gate/replay，受理丢响应跨页面刷新后原key重放及操作去重。
- 真实purge故障retry_pending静态码、root恢复后查询完成而不新DELETE；未知operation404不是完成。
- 实际父进程服务重启/日志重放/恢复元数据；实际epoch使另一个加载草稿和来源迟到200清屏；logout时迟到202不能还原privateDOM、重新认证原意图恢复。
- 真实完成/错误进度的深浅360/390/1440截图与键盘；完整API测试中的metadata命名空间只有IDs/请求标识/版本，无凭据/正文/缓存完成状态。

完整脚本default模式必须取得真实management三bool全true，还要求显式ALIA_P02_RESTART_COMMAND由资源父任务控制真实服务重启；缺接口/命令就失败，不soft skip。503真实journal/DB不确定性按root最新共享fence契约：去除测试故障后必须通过父服务实际重启/verified replay恢复ready，再由用户同key重放，不从前端绕过Store fence。Purge故障触发器只作用指定合成对象，真实状态只能从鉴权GET读取。

0真实模型/无部署/无实际个体初始化。早期源码变更与失败报告保留；生产没有mock成功、没有自动DELETE重试、没有新脏路由守卫。后继06只修改timeline/**/专属脚本/Workspaces最小identity/history，不再独立写本05已固定文件；05接口真实连接后由同窗口回到验收队列。

## 返回05真实验收｜控制器实证与真实API接通

自动续行复查上一轮为实际进展（06已交11组真实browser/源码交接）；不把API意图或目录当live job。先完成独立验收父工具：`p02-05-restart-control-v1/report.json` passed=true，真实固定RootHistory服务在私有QA库/目录完成错误owner/action拒绝、graceful stop→new PID→认证同identity/persona/runtime/version读回及清理。此scope仅父重启控制器，不算DELETE/job/journal replay通过；其源副本保存在同目录service-control.py，旧证明不覆盖。

随后root实际保存并释放固定`root-privacy-http-v1/source`181源，manifest SHA2a774ef52140a0dc5b1bd06a001915d80e312d54571f10d97f9028d1a57a76a3；root说明5HTTP/startup/worker+13pipeline+9closurePG、9journal/3primitive/Clippy及清理已通过。本窗口不将其次数算自身测试。已看到真实DELETE/GET privacy-operations、management.privacy及Runtime/main ALIA_DELETION_JOURNAL启动重放；按root授权立即返回05真实Gate，无新paid/fullmatrix/后端编辑。

唯一实际父任务已启动：`ALIA_PRIVACY_HTTP_MANIFEST_SHA256=<上述固定SHA> flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-05-full.py`，session28660，输出p02-05-full-v1。固定RootHistory381+root181后端overlay+05已固定前端；私有0700journal dir在全新QA数据库初始化前配置，镜像/服务/bin/web/控制Unix socket均父任务拥有，安全控制器只执行自己服务的restart且重新认证核对identity。仍在运行，完整DELETE/故障/恢复未提前记通过。06与05已交source不并行写；本轮先运行正式Gate，必要时只修自有前端/脚本，跨owner问题记录交接不改后端。

v1已实际终态exit1（非继续运行）：node语法/57离线/npm build/npm test/真实服务build均0；完整browser前9组实际通过，包括202→实际completed、刷新+真实服务重启、丢202同key、先保存后目标ver2、422、503独立journal/DB边界恢复、409与401再认证。第三次正常重启后Node测试GET报`fetch failed`，controller记录四个不同服务PID都ready且graceful退出、ownDB清理true；没有据此宣称purge/全部Gate通过，也未重复同一进程。推断可能是Node测试客户端复用旧TCP连接，已只对测试端请求设置Connection:close（无DELETE自动重试、业务断言不变），并补静态error.cause.code记录供后续判断。实际前端未改变。

root同时要求最终使用root-privacy-http-v2/source182源，manifest893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72，accepted root-privacy-http-02 manifest95238a35d5187802be5cf860a3eb6d6c502627c14435a0539b2445244e34f7f7（已有身份必须open_existing无create标志）。下一v2固定此源，不再验证v1。父任务hard deadline600s能终止自己整个子进程组并单独记失败/清理，不让异常Promise无限占资源；不因普通观察超时重启任务。
