# ALIA-P04-04｜实际信念记录、历史与共享来源检查

状态：**READY**。限定只读切片交付，4项真实HTTP/PostgreSQL测试、10组最终真实浏览器检查、严格Clippy/局部fmt/前端构建/快照M0均已实际通过。源与静态资源稳定，独立服务和PG清理通过，0真实或fixture模型调用。完整工程/行为/M2/M5完成不由本结论代替。

## 基线、所有权和注册交接

已读AGENTS、BLUEPRINT/CONTRACTS/IMPLEMENTATION、P04-04与P02-04冻结契约、真实0022表；Cognition.tsx修改前SHA `32df98233850b72ea72da35a5e109563958a239206d97272cdec3ccb47837f46`，本任务其余源文件均ABSENT。保留实际认知配置/能力/目标面板及链接，只替换原不可用信念面板。新adapter/service/tests、beliefs前端目录、专属脚本和本handoff属P04；未改root写引擎/Schema/迁移/App/shared/P02源/先前接受包。

root注册所需精确入口：adapters.rs与service.rs各声明 `mod beliefs_view;`，router增加 `.route("/beliefs", get(beliefs_view::beliefs))` 与 `.route("/beliefs/{id}", get(beliefs_view::belief))`。本窗口未改开发目录公共注册；主线现已实际保存两模块及两个GET注册，并经本窗口只读核对。限定测试仍是在独立派生副本中执行；不将注册保存冒充开发服务已部署或全工程通过。

## 最终行为

- GET /beliefs支持q/精确scope/current默认、superseded或all/分页；q与scope按Unicode字符长度和空白/控制字符严格校验，保留非空白空格，未知/重复字段422。字面PostgreSQL lower子串，不解释%/_为通配符；scope未提供允许管理侧浏览全部作用域。
- 倒序(created_at,id)游标绑定namespace/order/q/scope/status，anchor必须当前可见且仍匹配筛选。无永久跨页快照或虚构总量。GET /beliefs/{id}拒绝查询字段，返回同一个精确12字段item。
- 每响应单个RR只读事务；身份、认识、全部支持/反证和完整前身链递归可见。即使没有对应派生边，隐藏反证或前身仍阻断整条认识，不删反证后展示其余。前身循环可终止，同事件双角色完整保留。
- UI按需加载详情、显示实际版本/作用域/时间/已保存简要依据、前身引用和分角色来源。held是立场，简要依据不是隐藏推理；没有认识写入/回滚/人格操作。
- 来源只调用P02稳定 `SourceEventInspector({eventId,onClose})`，P04已删除自己的来源body请求/分页/render及CSS。真实/source-events投影、not_exposed、Unicode片段、实际inline资料API与canonical trace均由共同组件呈现；没有stub或第二执行器。filter/session/version/完成同步/offline变化使选中详情与来源失效，旧响应不能恢复私有DOM。

## 固定输入与真实验证

首次工程固定源：不可变N3 + accepted p04-02/p04-03/p05-01 + 本任务源 + 私有副本最小模块/路由/key/version注册。N5及root写引擎在途变更均排除。原不可变输入不动，独占Cargo target `target/p04-beliefs-v1`。后续仅前端/浏览器改动时，复用已核对SHA的同一Rust二进制，重建绑定的新静态资源；API源未变化，因此不重复已通过Rust/PG套件。总锁覆盖建库/服务/浏览器/清理，PG串行，浏览器显式Chrome `/opt/google/chrome/chrome`。

实际命令：

1. `cargo check --locked --test beliefs_view`：exit0。
2. `cargo test --locked --test beliefs_view -- --ignored`：exit0，**4 passed/0 failed/0 ignored**。显式执行独立PG用例，默认跳过不计通过。
3. `cargo clippy --locked --lib --test beliefs_view -- -D warnings`：exit0。
4. `rustfmt --check --edition 2024 src/adapters/beliefs_view.rs src/service/beliefs_view.rs tests/beliefs_view.rs`：exit0。
5. `cargo build --locked --bin alia`：exit0。
6. 用现有.venv Python执行固定源`scripts/check_phase0.py`：exit0，M0 PASS；不代替产品场景或14天观察。
7. 最终固定前端 `npm run build --prefix web`：exit0（TypeScript与Vite）。
8. 最终固定源 `node web/scripts/test-beliefs.mjs`：exit0，**10组**实际HTTP/PostgreSQL/Chrome检查通过。

API覆盖>30项、current/superseded/all与不同scope、完整双角色、版本/时间；Unicode/%/_/空格字面检索、同刻UUID排序、并发追加和绑定游标、anchor撤销；无显式派生边时两代反证/前身隐私、循环和identity屏障；非法/重复参数、鉴权/同源/no-store、未初始化和所有public基表摘要无副作用。

最终浏览器10组覆盖分页/按需读取、status/scope控件、来源片段/转义/双角色、前身与metadata-only/实际资料片段/真实运行跳转、键盘与dark 360/390/1440、读不变更事件/目标/outbox/usage、筛选详情一致性与实际SQL故障重试、真实离线/恢复、实际延迟DB读取后两代来源撤销、退出清空令牌/私有DOM/storage。已查看移动端与深色截图，删除宿主重复来源标题后最终v8再次通过。模型学习和工具执行均不由合成持久化记录证明。

## 证据、失败和边界

- 汇总：`evals/results/p04-beliefs-ready-20261112-v1.json`，SHA `caf50b9f1f879d63722bf27ea540341d448b9012890e1651d332b1db9b2fd992`。这是派生汇总，不改写原失败。
- 工程原报告：`evals/results/p04-beliefs-targeted-20261112-v1/report.json`，SHA `aaeb327390bcee7dd93c8e7977841311a7e6d0ec423b9e4075578af892f9d536`。整体passed=false保留；其中前7命令实际exit0，浏览器因默认headless_shell1254未安装而未启动。汇总只采用实际通过的检查及最终前端/浏览器证据，不把整体失败改为成功。
- 最终共享组件浏览器父报告：`evals/results/p04-beliefs-browser-20261112-v8/report.json`，SHA `5d3b4ebe0ff362b827b9c17d9e4637c045249f094558511652d1e93204bc6a86`；passed=true，sources_stable/static_assets_stable/cleanup/service_cleanup/browser_database_cleanup均通过。
- 最终浏览器子报告SHA `3ba7618ab6b57010141bf6148ce4c5be78c18bdf95f876819ff32f12cdce91c2`，10组通过，浏览器清理true。
- 最终完整来源/依赖/静态资源绑定manifest SHA `58e98c1175e4e9c51e01d89345007d0dbbf11b7f159c89c8022c56b33192aa7a`；Rust binary SHA `9147242a5471dda8e4300331b7c7915b02d56564af79580d2a819e52d604ef33`。
- 浏览器v2：已加载真实跨页列表，但select精确可访问名称定位失败，已补明确aria-label。v3：资料UUID也等于来源事件UUID，泛定位匹配两处，测试改为metadata dl。v4：N3主题按钮旧标签与新测试标签不同，改精确兼容。v5：浏览器前docker exec建库失败，未计产品失败，加入最终TCP就绪门。各轮失败原样保留、资源清理通过。
- v6旧独立来源实现10组通过后，按root新分工删除重复实现；v7共享组件10组通过；最终v8删除宿主重复标题后10组通过。只把最终v8计为本版浏览器证据，不累计重复轮数。

## 最终自有源SHA256

| 文件 | SHA256 |
| --- | --- |
| `src/adapters/beliefs_view.rs` | `0f0ce48e7e32c6f626318a9c32f943928bbf2c5c576950b33698602126ddf463` |
| `src/service/beliefs_view.rs` | `ea0f04b8299b50cb8edef72355f1672fbed10306bc719617fb476c1560956c0a` |
| `tests/beliefs_view.rs` | `5f7f6408e872bfd5133e0af7fec843aff73be0678f5f2beca5e95bd564ce9c86` |
| `web/src/features/Cognition.tsx` | `e2fdbc961aaeeac3f876a31282ffb2a70c72f1c1ecc11dd472dd9c25d15e2ebf` |
| `web/scripts/test-beliefs.mjs` | `59033d90a78c0793c5080f9e7b6a56bf8ff4f69c39d9eef5e6e4d6cc9c38c9d2` |
| `web/src/features/beliefs/BeliefsWorkspace.tsx` | `7ccde3e0e3a259cf079d696a44cee01fb86733c959ceb94e5ccb66ab8550d52b` |
| `web/src/features/beliefs/beliefs.css` | `0d9f69dc41af0f38b58da217364731cfea75623eee128e8653d98ca9eee3c5a8` |
| `web/src/features/beliefs/useBeliefsRead.ts` | `e26fd35eb2b82a95c76871969382ac7d92bdb62c9df8a40bcdc7c5a9062b5b9a` |

## 实际消费的P02共享依赖SHA256（只读、未修改）

| 文件 | SHA256 |
| --- | --- |
| `web/src/features/sources/SourceEventInspector.tsx` | `1acfc702364276008691043ed544535bbc0cb71cde13ef1a1f109bbbe72fbe5c` |
| `web/src/features/sources/sources.css` | `ea4719ef6a8fda38eda7374974546cc08e8b138b230272646f74b32b15e8e6aa` |
| `web/src/features/sources/types.ts` | `010ef2ea2567b835881723a1feb0a796f47839107ff8e0ce625560b15d38100d` |
| `web/src/features/sources/useSourceRead.ts` | `fbe19942fc92b338dec235cc5b52d3093a59ee69522011497fd70e39fd224f5f` |

交付时自有八源与共享四源均和最终固定副本匹配。P02组件当时NOT_READY，本窗口仅证明这些精确源在认识宿主中的使用；记忆草稿/Config guard等共享组件完整宿主回归继续归P02，不冒称已通过。

下一依赖：root核对并接受八源（公共注册已实际保存），确保P02最终接受的共同组件与这里消费的版本一致；若共享源变化，针对变化重新跑认识浏览器。P03将接受版本纳入后继固定工程候选，P01负责真实认知/来源行为；本切片不发布、不初始化实际个体、不扩大收费授权。广义来源图/物理删除/完整导出/永久跨请求快照仍未完成，ROOT-BELIEF-03与完整M2/M5不由本报告覆盖。

主线后续交接已接收：N5R2为core-only固定工程候选，本只读模块不在其输入，不能借用其通过结论。P04-04八源SHA仍与READY汇总匹配；本次仅更新此交接记录，未重跑检查、未修改READY源或追加收费。
