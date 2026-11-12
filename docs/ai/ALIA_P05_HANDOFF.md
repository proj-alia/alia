# ALIA-P05-01｜资料目录与阅读检查器

## 初始检查点

- 窗口：`01a1cf3a-7d6a-76e3-a7b2-fe3d78ae964b`；独立 Goal 已建立，active，无 token_budget。
- 主线：`01a1cbe8-e270-75e1-9535-d3f2e0ea81c4`，唯一接口/协调/规范/发布者。只通过本文件交接，不向其他聊天发消息。
- 已读根 AGENTS、BLUEPRINT、CONTRACTS、IMPLEMENTATION、并行协调；当前尚未发现 P05 released。先只读核对代码和写本专属 handoff，不修改其他文件或运行重型任务。
- 已有 documents/events 和受限 documents.read 可复用；Settings 目前只有文本导入，无服务端目录/片段查看。新 owner GET 不写阅读回执、生活事件、运行或费用，页面查看不代表模型已读。
- 不改模型/工具权限、core store、迁移、冻结 N1/N2、P01/P02/P03/P04 文件或 contexts；不部署、初始化生产或调用模型。

## 请求主线冻结的接口及注册

- `GET /api/v1/documents?q&cursor&limit`：q 最多200 Unicode字符，title/source_label 的 PostgreSQL literal lower/strpos 搜索（%/_非通配符），默认30最大100，导入 event.sequence/id 倒序；新 documents 游标绑定筛选，列表不返回正文。
- `GET /api/v1/documents/{id}?offset&max_chars`：offset 非负且不大于字符总数，max_chars 1..4096，默认4096；Unicode半开片段与 total_chars/next_offset，末尾 next_offset=null。未知/重复/非法参数422，未知/递归不可见404，未初始化409。
- 响应同一 REPEATABLE READ 只读快照，含 individual_id/state_version/privacy_epoch/observed_at；目录每条 document_id/title/source_label/source_kind/total_chars/occurred_at，片段附 text/offset/next_offset。不返回原始payload/URL凭据或生成阅读收据。
- 新 adapter：`src/adapters/documents_view.rs`；新 service：`src/service/documents_view.rs`，导出 `documents` 与 `document` handler。主线需整合 `src/adapters.rs` 的 `mod documents_view;`、`src/service.rs` 的 `mod documents_view;`，在既有 POST /documents 旁增加 GET 并保留2MiB导入层，新增 `/documents/{id}` GET。
- 前端只改 Settings 的资料区域并新增 Documents.tsx/专属 types/CSS；新增 tests/documents_view.rs 与 web/scripts/test-documents.mjs，不碰共享 context/style/API 客户端。
- 局部 HTTP/PG 测试覆盖完整分页、Unicode片段、游标筛选、严格参数、两代来源阻断、GET无状态/事件/收据/费用副作用；真实浏览器覆盖导入兼容、目录/详情/断线/退出/注入文本/布局。重型运行遵守总flock；full工程交P03、行为交P01。

## 当前结果与依赖

- 只读定位/文档读取完成；尚未运行编译、数据库、浏览器或模型，无测试通过声明。
- 下一依赖：主线登记 P05-01 允许范围、Settings基线及上述接口，released后持续代码实施。已有任务不必等待本功能，候选通过前不算里程碑完成。

## 已释放 / 冻结接口落地

- 主线已登记 P05 released；按其最终wire实现：目录items使用id/title/source_label/source_kind/created_at，不额外加入正文或字符总数；detail max_chars默认2000，返回规定individual/document/event metadata与Unicode片段。前述初始候选字段不是最终契约。
- **发现一个精确接口说明缺口**：现有migrations/0004与0005及后续迁移都没有`documents.created_at`。不引入新迁移；实现将使用已有源事件`events.recorded_at AS created_at`，稳定按`(events.recorded_at,documents.event_id)`倒序，wire字段与用途保持。请主线修正协调说明中的列名；若要求改用occurred_at请在主线登记。event_id和源可见性在同tx复验。
- Settings实测完整SHA以本文件上段`81acbcdf...`为准，登记中的`944f1935`为示意且不能匹配当前保存源码；当前仍未改，准备在此基线上仅增加资料组件。
- 主线注册需求保持`mod documents_view;`及`documents_view::documents`、`documents_view::document`两个handlers；将在模块保存后提供可编译签名。

## Backend模块已保存 / 可立即注册

- 两个新增模块已保存。`src/adapters.rs`增加`mod documents_view;`；`src/service.rs`增加`mod documents_view;`，已有`.route("/documents", post(api::document).layer(...))`改为`get(documents_view::documents).post(api::document).layer(...)`保留原2MiB层；另加`.route("/documents/{id}", get(documents_view::document))`。
- handlers均`pub async fn`，参数State<Runtime>、严格Query，以及detail的Path<Uuid>；不需要其他helper可见性修改、不增加依赖。尚未编译，接下来新增局部测试/前端，待主线注册后统一定向验证。

## 源码实施中

- backend两个独立模块已保存，新增4项真实TCP/HTTP+PostgreSQL测试（均显式ignored，需定向实际执行），Documents.tsx和专属types/CSS/session-keyed hook已保存，Settings仅加入资料组件。
- frontend目录元数据自动查询，正文仅选择后按2000字符读取，不自动拼接全资料。新查询/翻页/断线/刷新/会话变化立即清空旧数据；请求活动标识拒绝晚到回灌。导入原表单保留。
- 尚未构建/执行DB/浏览器，无新增通过声明；主线注册完成后将锁内运行本切片定向检查，不运行完整矩阵。

## 首轮局部检查

- `rustfmt --edition 2024`仅本任务三个Rust文件，退出0。
- 总flock内 `npm --prefix web run build`退出0，TypeScript+Vite通过；未运行全工程矩阵。
- 专属外部浏览器子驱动已保存`web/scripts/test-documents.mjs`，仅显式新鲜隔离服务/数据库/二进制/输出；需要`ALIA_TEST_URL/TOKEN/OUTPUT/BINARY`、`ALIA_P05_DATABASE/PSQL_COMMAND/LOCK_HELD=1`，不读取.env、不自行构建或启动默认Compose，不调用模型。父驱动持锁并清理，子驱动清理browser与自己故障进程。
- 模块注册仍等待主线保存（本任务不改公共注册文件）；后端代码和handler已可注册，目标测试编译/实际DB将按注册后结果记录。

## 注册完成 / 局部检查排队

- 主线已实际注册两个模块和GET routes，并保持POST及2MiB层；不再等待注册。
- `npm test`父session85244、`cargo check --locked --test documents_view`父session26574已按同一全局flock排队；观察无输出不启动替代任务。
- 唯一定向DB/HTTP+Clippy+固定本机二进制/静态树+专属浏览器父session94242，驱动`/tmp/alia-p05-local-checks.py`，新证据`evals/results/p05-local-v1`；仍等待全总锁，不取消P01/P03在途任务。驱动用P03拥有资源校验/清理库建立独立测试PG，不读取生产.env；持锁直到子进程退出及清理。
- GET副作用断言除行数外已增加identity/events/documents完整记录摘要对照；合成排序fixture修改在独立比较区间，不把fixture变更算GET副作用。
- 浏览器子驱动node语法检查退出0；所有新DB/浏览器结果尚未运行，不宣称通过。

## 已核对的保存基线与实现细节

- Settings.tsx SHA256：`81acbcdfeda467118b5d660f969783ae4821b1919a9fb1f2f2d9d60501f3aeae`；本任务其他源码/测试文件目前ABSENT。
- 只读参考 capabilities.rs SHA256：`3162b5b9e5800a1fb45549922968726772aa5a1b2b2452931b1f8ff760ef09ce`。当前工具按Rust Unicode字符读片段，owner投影将用等价PostgreSQL char_length/substring，避免把UTF-8字节当位置。
- 目录筛选采用PostgreSQL `strpos(lower(title),lower(q))` 与label同类比较，不用Rust预lower，避免已知Unicode点I排序差异；游标绑定trim后的原查询摘要，不把游标当鉴权。
- 可见性复用adapters::api::VISIBLE/existing，event_id就是document_id；正文不进入目录。成功GET不创建outbox、events、runs、actions或usage。
- 独立前端读取hook将以token/individual/state.version/lastSyncedAt和connection失效，不改共享hook；断线立即隐藏私人资料，旧响应的generation校验在回填前执行。仅选择一个文档的一个4096字符页，不自动拼接全正文。

## v1实际结果 / 浏览器驱动格式修复

- npm test退出0；cargo check --locked --test documents_view退出0。
- `p05-local-v1`父驱动退出1：4项真实HTTP/PG测试实际4 passed/0 failed/0 ignored（2.79秒），定向严格Clippy退出0，服务与前端构建退出0；browser退出1。
- 浏览器实际通过7组：完整目录/字面搜索/键盘/安全HTML、片段与三宽度、GET无变更、真实慢响应+两代撤销SSE、原导入、真实SQL错误+重试、实际offline恢复。最后zero-usage断言读取到psql表格字符串而不是标量0，失败属于本测试驱动，原报告不改判。
- 已修复SQL调用显式`-A -t`，也确保真实table-lock等待检查读到数字。新v2仅重建固定服务/静态树并重跑专属浏览器，不重复未修改Rust的DB/Clippy，仍同总锁和独立新测试资源；父session见下一实际记录。v1服务及container/volume清理均通过，model_calls=0。
- 现行参数以主线最终契约为准：非空q保留空格、默认片段2000；上段初始候选trim/4096描述不是现行实现。

## v2浏览器通过 / 截图复核

- v2父驱动退出0；8组真实浏览器检查全部通过，含最终logout/storage及tool.result/usage精确0断言，source unchanged=true，服务及独立PG容器/卷全部清理成功。没有重复Rust DB/Clippy；它们对应未修改后端v1证据。
- 目视复核390/1440全页截图发现：当前滚动位置下Chromium fullPage对sticky外壳和内滚动pre的离屏绘制不完整，不能把该截图当正文视觉证据。未据此改产品CSS；新v3截图改为真实滚动到正文并清零pre.scrollTop的viewport，再单独记录页面顶部，避免离屏拼接伪影。业务断言与实际HTTP不变，旧v1/v2报告保留。

## 最终 READY / P05-01 局部交付完成

- 原P05完整目标已在限定范围完成，源码现在稳定，可由主线/P03选入下一候选。上文候选字段/等待注册仅历史；最终按主线冻结wire：目录五项元数据和cursor，片段默认2000 Unicode字符、上限4096；created_at=源events.recorded_at，非空q空格保留。
- 实际验证：4项真实TCP HTTP/PostgreSQL测试全部通过（0 failed/0 ignored），包含37资料跨页、同刻UUID排序、%/_和Unicode字面查询、UTF-8大于128KiB旧导入、emoji/combining字符边界/EOF、两代来源阻断、严格参数/认证/跨源/no-store与完整identity/events/documents摘要无变更。
- `cargo check --locked --test documents_view`、定向`cargo clippy --locked --lib --test documents_view -- -D warnings`均退出0；仅本任务Rust文件rustfmt --check退出0；npm build/test退出0。
- `.venv/bin/python scripts/check_phase0.py`实际退出0：5 schema/47闭合对象/9示例/52反例/111追溯/136依赖/9文档/34链接/2原始资料。M0不替代生命周期或十四天验收。
- 浏览器v3：8组通过，真实HTTP/PG/浏览器、无响应mock。360/390/1440布局及键盘、目录和正文、原导入、SQL失败重试、实际offline/logout、真实慢读+SSE两代撤销均通过；390/1440正文viewport截图已目视检查可读且恶意HTML未执行。
- v1失败保留：最后零费用断言误读psql表格输出，修为-A/-t后v2/v3成功；未更改产品判定或删除断言。v1/v2 fullPage截图存在离屏滚动拼接伪影，v3改正确viewport证据；未修改产品CSS掩盖问题。
- v1/v2/v3每轮服务和P03-owned独立PG容器/卷cleanup通过；v2/v3源码末验unchanged=true。模型调用0、usage/tool.result精确0；无部署/生产初始化/运行规则修改。
- 完整Rust/feature/全数据库/旧浏览器矩阵本窗口未重复，交P03；本READY不宣称M1/M2/行为或完整发布已通过。

### 证据与复现

- 后端/Clippy证据：`evals/results/p05-local-v1/{database-http.log,clippy.log}`（父报告因浏览器驱动失败整体false，不能改判）。
- 最终浏览器与局部父报告：`evals/results/p05-local-v3/{report.json,browser/report.json}`；最终父SHA256=`4fdc93ece9bbe97b2eb4c1a343eedd22e4df1ce4c93de69bd5832cfd2d6505ad`，browser SHA256=`b5bc0289d9de20bf8a10b0fc0a4afbf078c0d2333c70a9d8bb547ba48861020e`。
- v3仅浏览器重跑（重建固定bin/web），未重复Rust DB/Clippy；后端三文件SHA与v1实际通过版一致。父临时驱动`/tmp/alia-p05-local-checks.py`可运行`--output <fresh path>`，全父过程持锁，使用独立资源与固定复制产物，日志不写凭据；完整产品复现入口为下述专属测试。
- P03接入：在候选DB矩阵明确加入`cargo test --locked --test documents_view -- --ignored`（四项，不把普通cargo的ignored计通过）。
- 专属browser是child-only：`node web/scripts/test-documents.mjs`；需独占且初始未初始化的测试服务与DB，环境`ALIA_TEST_URL/TOKEN/OUTPUT/BINARY`、`ALIA_P05_DATABASE`、`ALIA_P05_PSQL_COMMAND`(JSON argv，psql由脚本追加-A/-t)、`ALIA_P05_LOCK_HELD=1`及既有Chromium路径。父driver必须持真实flock，固定bin/web并负责服务/DB生命周期；子driver只清理自己browser/故障SQL进程。不得指向生产或默认Compose；无.env读取或构建。
- 下一依赖：主线确认最终接口/文档，P03把READY源码放进新冻结候选后运行完整工程；行为判断继续P01。无本范围待修生产缺陷，不新增未授权下一域功能。

### 最终已保存文件SHA256

```json
{
  "src/adapters/documents_view.rs": "5016d9d72eef903949385f84a65a03b85e26879d1306ca961fb554de2768a0b6",
  "src/service/documents_view.rs": "1dfb780676573d91280c8b334bb531a581a30233d92142ce60d900743be4eece",
  "tests/documents_view.rs": "7668e23484904f5522e3fbe97d3d98bef12f4d3760052e77857a6b72d3ebb1eb",
  "web/src/features/Documents.tsx": "a6fa451b8c076ca72aaab9d518a555731443829c2371d889bebf3b0c2b218020",
  "web/src/features/Settings.tsx": "bcb095490abd42b54fc4fa1be2b17a24f888e1f0a04e7d4755416c74a6b64942",
  "web/src/features/documents/types.ts": "50712ba4b545d1408a72e0ba77674542e0f451909d1c3dc60e5822e98646c39f",
  "web/src/features/documents/documents.css": "0f417efc48fa0c4e0b0ef4e43b0ed615057afe0319bb51de2fafa58179237971",
  "web/src/features/documents/useDocumentsRead.ts": "a255d2b8b64429c25515290c74e88649b7b152821a623edcddd1dd7396330b10",
  "web/scripts/test-documents.mjs": "c0c844f516d23b12160c84a91a80a5bbcd2baf4feec2f46f0b44a6a7c17e7c43"
}
```
