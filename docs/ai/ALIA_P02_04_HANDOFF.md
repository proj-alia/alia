# ALIA-P02-04｜共享安全来源检查器与记忆接入

状态：READY（本限定切片局部验收完成；完整候选工程仍交P03）。

已读取AGENTS、BLUEPRINT；相对已读N4权威资料逐行核对当前CONTRACTS/IMPLEMENTATION差异及协调P02-04/P04-03冻结契约。新有界Goal已创建，无token budget。只修改新sources目录/专属脚本、MemoryInspector最小来源接入与本交接，不改App/nav/backend/旧accepted包。

## 稳定组件已保存，供P04消费

`web/src/features/sources/SourceEventInspector.tsx` 已保存命名导出 `SourceEventInspector({eventId,onClose})`，`eventId:string|null`，`onClose?:()=>void`。内联安全读，不新增modal/router/dirty/beforeunload流程；仅依赖现有IndividualContext。配套types.ts/useSourceRead.ts/sources.css属P02维护。P04可直接import，共享接口稳定，初次保存时尚未验收；现已完成P02最终真实HTTP/PG/浏览器16组验收，见下文。

实际GET /source-events/{id}?offset&max_chars=2000，正文分段/空串/末尾、not_exposed真实元数据、真实document_id内联资料API与run_id原trace链接；不dump JSON、不猜correlation。切换/session/version/完成同步/离线有渲染时key屏障、逐请求abort/旧响应抑制。来源选择不改记忆草稿/意图，不触发新guard；trace链接继续走App既有全局dirty guard。

MemoryInspector修改前SHA与接受包一致：c3263ec4919e62ee566878a58dc8f4a7253e3b909dea558ed6a09e0ebb7026cd；memory.css基线9df9e91d9814beaca79e5a7e7ffeedb61459533c6e750978ef564499c513b537，当前不需要修改。P02-03已接受副本与其失败/成功证据保留。

## 验证计划与限制

使用不可变N4源+accepted p02-03/p04-02/p04-03+本切片与私有副本最小history/source注册，独占Cargo target，实际隔离HTTP/PG/浏览器；不以并发live源码/共享target产物冒充稳定基线。重型父进程持同一flock至清理结束，0模型/不部署/不初始化实际个体；完整工程P03。后续记录真实结果、失败、精确SHA与下游依赖。

输入文档SHA：
```json
{
  "AGENTS.md": "79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4",
  "docs/BLUEPRINT.md": "30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943",
  "docs/CONTRACTS.md": "f6d3c2cc428ea3ea33ef78fe4a989bd872e044d3bfde3b9d55f8a42c82331509",
  "docs/IMPLEMENTATION.md": "c69f80d3e8837b919f3144774c842e96afa7d51b87912b2544bea88bbfe2086a",
  "docs/ai/ALIA_PARALLEL_COORDINATION.md": "6396e17b5c021a950ec59524348764bf35e3c9f72e045b37e77d6f6573ae5d51"
}
```

## v1局部父任务记录

`p02-04-source-v1`：node --check、npm build、npm test均exit0。独立Rust build exit101，原因是本任务临时测试父脚本把新增mod放在adapter内层文档注释之前（E0753）；未到达PG/浏览器，不报告其通过。已修正私有副本派生注册的位置，保持原文档注释和N4/accepted源原样。v1源/失败报告保留；未创建DB、私有临时目录清理完成。v2沿用独占target，仅重跑实际浏览器所需构建与隔离资源。

v2：node语法/npm build/cargo build exit0，随后工程PG已创建但第一次建库docker exec失败，未进入浏览器；父报告passed=false、独立数据库资源cleanup=true。QA provision的Unix socket就绪可能遇到initdb临时服务，因此本任务临时父脚本补最终TCP listener就绪只读检查，再进行一次建库；不重试未知建库写操作，不改P03公共资源工具。v2原报告保留。v3继续相同固定源与独占产物。

v3：隔离TCP就绪与服务构建成功，真实浏览器前13组通过，最后撤销测试点击来源刷新超时，父exit1；service/database cleanup均true。定位到最小记忆接入把普通completed-sync与连接标签切换也当成选择清除，造成仍合法的来源面板在后续检查时消失；不是伪造成功或来源泄漏。修正为目标/session/实际state.version/连接可用性变化才清来源选择，普通同步仍由已接受MemoryRead和SourceRead的key屏障隐藏/重新读私有内容。强化每张截图必须包含实际来源正文，补在途真实关联资料200切换丢弃，保留v3失败与原13组证据。Config定位器改用原文精确label，不改变其行为断言。


## 最终v4 READY｜实际结果与命令

唯一父命令 `ALIA_SOURCE_RUN=p02-04-source-v4 ALIA_SOURCE_BROWSER_ONLY=1 flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-04-local.py` exit0。整个固定源码准备/构建/最终TCP就绪/真实服务与浏览器/子进程退出/私有资源清理连续持总锁。独占Cargo target=`target/p02-source-inspector`，jobs=2、dev debug=0，RUST_TEST_THREADS=1；不消费共享target/debug产物，不运行完整套件。

在`evals/results/p02-04-source-v4/source`固定源码内实际执行：

1. `node --check web/scripts/test-source-inspector.mjs` exit0。
2. `npm run build --prefix web` exit0（tsc --noEmit与Vite，76模块）。
3. `cargo build --locked --bin alia` exit0；独占产物复制为私有二进制，web/dist同步复制后启动真实loopback服务。
4. `node web/scripts/test-source-inspector.mjs` exit0，**16组实际检查全部通过**。没有把未执行的Rust套件/P04后端报告算成本窗口测试。
5. 源码最终稳定后，在同一固定source下额外`flock -x /tmp/alia-project-heavy.lock env PATH=/tmp/alia-go-diagnostic/node-v24.11.1-linux-x64/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin npm test --prefix web` exit0；SSE/组件/原路由与配置合同通过，日志`../final-web-tests.log`。v1中npm test也exit0，未重复不可达PG项冒充通过。

覆盖实际用户/合成持久生成/真实owner更正的source_kind与原正文，内部审查/非字符串not_exposed、空串与EOF、Unicode 2000字符区间、真实document/run链接及禁止correlation猜测；实际关联资料片段/安全转义、资料行移除后的404与链接null重验；来源和关联资料两种在途真实200切换丢弃；离线清屏与显式在线再选；实际SQL列暂改故障造成source503、无SQL泄漏/旧正文/草稿丢失，恢复后实际retry；来源切换/关闭不改记忆草稿，trace链接仍用原单guard取消；Config空值/save/discard/cancel/back/forward；真实已提交201断链后的同key/body/If-Match重放（检查来源未改变稳定意图）；运行撤销后链接null但独立事件仍可读；两代来源撤销清草稿/正文并拒绝迟到200；退出令牌/私有正文/旧响应及local/sessionStorage清空。

用于generated/内部/run的记录是明确的**合成持久化夹具**，不是模型生成或实际工具执行证明。没有模型调用，usage_reservations为0、所有run.model_calls合计0；检查源与资料的UI读取前后individuals/events/memories/goals/runs/actions/outbox/usage_reservations指纹一致。初始化、一次真实更正、Config保存及隐私/失效夹具变更分别明确，不混入“读取零写入”比较。网络故障只延迟/丢弃实际接口返回，不生成成功payload；503来自独立PG真实查询错误，finally恢复测试列。

六张light/dark × 360/390/1440最终截图都要求包含实际来源正文，已逐张目视复核；无横向溢出、按钮/长UUID/元数据与预览可用。键盘Enter能开关内联来源，不新增modal；无浏览器uncaught error。source/read与doc/read清屏靠渲染时key、每请求AbortController、active与session/snapshot绑定，不把晚响应交给新资源。

parent/browser passed=true，source_stable=true，service正常退出与database_resource清理均true。v1/v2/v3失败及原已通过分组原样保留，不改历史报告。最终六个自有源码同时匹配live、固定source、父报告与浏览器报告。p02-02/p02-03/p04-02/p04-03 manifest及全部接受副本再核对不变；原p02-03除本次获准的MemoryInspector外另外11个live源码（含App/nav/历史/旧专属测试）和memory.css保持原SHA。

## 精确稳定输入与报告

基线为N4 checkpoint manifest `3b49fcaf5d261e6123e1c7e4e85da5ca3fbf495894f540dafc291a237a6c3202` 的全部已校验源，再按顺序覆盖三个已接受包：
- `p02-03` manifest `83c35098c3841d90403ee534d0426dcce3bb184467dafd8ba942ee23db9830f5`。
- `p04-02` manifest `9a8b770f71846f7041e540ac27788cefaf8d8766b626a4212508ecc1f16fa184`。
- `p04-03` manifest `14c1526d566640054a3f8f5466fe76347d07364b0da370ff1d9f859b09ab0093`。

私有副本仅另派生adapters/service的source_events模块与history/source GET注册，保留N4已接受的memory/documents注册及key/version可见性，旧新SHA均记在父报告；没有修改开发目录后端注册。父资源QA common/database与N4不可变副本SHA一致。node_modules仅依赖位置链接，package-lock与固定N4字节一致；生产源不从live补齐，N5/在途认知/关系UI不进入输入。

- 父报告 `evals/results/p02-04-source-v4/report.json`：SHA256 `bb3e1f63f657ffba2054ae73799e14965d328522fb17a54864a6517bd3bd6832`。
- 浏览器报告 `evals/results/p02-04-source-v4/browser/report.json`：SHA256 `5586bf0c0de16419b432d77b8077e09a14a33836fd805c4999af04b2a3219309`。
- 完整输入清单 `evals/results/p02-04-source-v4/source-manifest.json`：SHA256 `798520a8ce0f3fcc3231aa22de290d3fc064818149681025f50548211c26903a`。
- 最终补充核对 `evals/results/p02-04-source-v4/final-verification.json`：SHA256 `3bec92006c93c33d69bedf10f5ccbf64579504b95292cc61dded200e020ddc36`。
- 实际私有服务二进制 SHA256 `723dd5a71fb825606247abcbacf4dc3f213632130d5d02c0d4fd63cea09ecdd0`；这不是可部署完整候选。

## READY六文件SHA256

| 文件 | SHA256 |
| --- | --- |
| `web/src/features/memory/MemoryInspector.tsx` | `47576efd661958ce14e9b323170d7d5d6f1511a3fca0e19554026a83e5f05a4a` |
| `web/scripts/test-source-inspector.mjs` | `e42b5a3a042286c9578582b6e9464fad365a08e024194886ddeb8be6e655c717` |
| `web/src/features/sources/SourceEventInspector.tsx` | `1acfc702364276008691043ed544535bbc0cb71cde13ef1a1f109bbbe72fbe5c` |
| `web/src/features/sources/sources.css` | `ea4719ef6a8fda38eda7374974546cc08e8b138b230272646f74b32b15e8e6aa` |
| `web/src/features/sources/types.ts` | `010ef2ea2567b835881723a1feb0a796f47839107ff8e0ce625560b15d38100d` |
| `web/src/features/sources/useSourceRead.ts` | `fbe19942fc92b338dec235cc5b52d3093a59ee69522011497fd70e39fd224f5f` |

MemoryInspector最小差异：导入共享组件、单独sourceEvent选择、按target/session/state.version/实际连接可用性清选择、来源ID旁按钮和inline组件。普通完成同步仍先由原MemoryRead隐藏详情，来源重新请求；不因“服务已连接/实时同步中”标签切换无故丢合法选择。不更改编辑/保存/未知意图/guard/history业务，不修改memory.css。其余新增文件仅sources四件与专属脚本，本交接不放自指SHA。

## 下游依赖与限制

P04可消费上述稳定命名导出及两个props；本组件需既有IndividualContext，onClose可省略，eventId=null显示选择提示、不发来源读取。主线按六文件摘要保存新接受包后，将本切片及实际P04-03来源/P04-02历史/已有documents服务注册交P03后继工程；不把N4/N5报告冒用于本新UI，也不由本窗口修改P04认知页面。主线仍唯一接口/发布协调者，本窗口未向其他聊天发送消息。

专属脚本要求显式新库、固定URL/TOKEN/OUTPUT/BINARY、PSQL argv和ALIA_P02_LOCK_HELD=1；不读.env、不构建live目录、不启停既有容器、不初始化真实个体。父任务负责固定输入/构建、一个PG服务及所有清理。保存的原文仅为本地工程合成夹具，无私人运行数据/凭据/provider原始日志。单响应RR不保证跨片段永久快照；not_exposed不是不存在，source_kind不是世界事实认证，页面读取不是模型阅读。广义来源图、导出、物理删除、M2语义与长期观察仍未完成；0模型/不部署。
