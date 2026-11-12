# ALIA-P04-02｜只读记忆修订历史 API

状态：READY。主线已注册实际路由；v2 定向编译、新6项与原8项真实HTTP/PostgreSQL测试、严格Clippy、局部格式检查及M0全部exit0。源码摘要前后稳定，独立临时数据库清理通过。原P04-01接受包与首轮失败证据保持不变；本结论不代表完整工程回归或行为验收通过。

## 输入与所有权

- 窗口 `01a1cf39-aa98-7801-93af-12ac3758bb5c`，主线 `01a1cbe8-e270-75e1-9535-d3f2e0ea81c4`；本文件供主线/P03/P02读取，不另发消息。
- 已读根 AGENTS、BLUEPRINT/CONTRACTS/IMPLEMENTATION 及协调文件末尾 P04-02 冻结接口。仅改已有两个 memory_commands 模块、新 `tests/memory_history.rs` 与本 handoff。无迁移、前端、store/P01/P03/规范改动。
- 允许已有文件SHA与接受包一致：adapter `307ba5c872ae59fc1a2ad88e806da86b59476153f77656732b86cfba4361cd4a`；service `d41baf8462d9c16a64e658b2c7f98a305961376ea3681d01e2ece7399aaf7dc8`。新测试ABSENT。

## 实施与注册约定

- `GET /memories/{id}/history?cursor&limit`，默认30/最大100；连接目标的同个体supersedes链，升序(created_at,id)，循环安全，单响应只读可重复读快照。
- 响应严格按主线固定：individual_id/state_version/privacy_epoch/observed_at/target_memory_id/items[{memory,correction|null}]/next_cursor；不返回总量、不保证跨页永久快照。
- 来源/祖先不可见的行、理由和链接不能泄漏。纠正理由只从该记忆实际直接来源中的owner/reported_claim memory.corrected提取，并核对其前身、正文和作用域，不从相似内容推断。
- 新handler `memory_commands::history` 已由主线在service路由注册 `.route("/memories/{id}/history", get(memory_commands::history))`；P04未编辑公共注册入口。
- 新游标绑定namespace/升序/target/精确anchor时间与ID，anchor必须仍在当前可见连接分量。字段错误、跨目标或失效anchor422，目标不存在/撤销404。

## 验证范围

定向真实PG/TCP覆盖跨页链/同刻排序、并发追加、循环/其他个体/无关分支、祖先与个体阻断、实际理由与假关联反例、严格查询/鉴权/无副作用；回归原8项修订测试，再做局部严格Clippy/fmt/M0。0真实模型，不部署，完整矩阵交P03。

adapter/service与测试已保存并定点rustfmt；原GET/POST行为保持并经原8项测试回归。主线注册已到位，没有待合入的helper或依赖。

实现细节：合法supersedes必须同原scope（既有纠正/模型supersession契约），遍历不跨隐藏或非法scope链接。源关联缺失/被阻断的记忆不纳入；公开supersedes仅在前身仍可见时返回，否则null。两级递归均UNION去重确保循环安全；游标anchor也需属于当前可见链且时间一致，跨页不固化旧隐私快照。纠正理由额外核对实际source-linked事件中的前身/content/scope，其他事件一律null，不透传任意payload。

新6项实际测试覆盖：33项链/默认与多页/原始理由null/只读；同刻UUID排序/跨目标及伪anchor拒绝/并发追加；祖先、后继、理由来源、链接和identity阻断；循环与其他个体约束/非法scope隔离；源关联/actor/source_kind/前身/正文/scope/空白理由反例；strictquery/auth/no-store/no-side-effect。新6项与原8项memory_revisions均实际执行通过，无ignored或未运行项混记通过。

## 定向父任务与证据

首轮tool session `75051` 已终态exit1，失败报告 `evals/results/p04-history-targeted-20261112-v1/report.json` 保留。夹具纠正后唯一重跑父任务 `44581` 已终态exit0，报告 `evals/results/p04-history-targeted-20261112-v2/report.json`。两轮各自完整持现有总锁，按实际镜像创建独立工程PG并清理，无并行或收费重跑。

## 首轮真实测试失败保留 / 夹具纠正

v1父任务已终态exit1，定向编译exit0，history 5 passed/1 failed/0 ignored（Cargo遇到该目标失败后原8项未到达）。源码前后稳定、私有资源清理通过。失败是cycle/foreign测试尝试插入false singleton的第二身份，被实际`individuals_singleton_check`正确拒绝；不是接口读取泄漏。旧报告不覆盖。

已修复自有测试为断言第二身份23514、未知个体对象23503与外部记忆ID404，保留循环/其他scope/无关本地记录真实读取检查；不删除或放宽实际Single Individual约束来制造数据，不改生产实现或迁移。v2已重新执行新6项+原8项与严格检查，结果如下。

## v2实际命令结果

1. `cargo check --locked --test memory_history --test memory_revisions`：exit0。
2. `cargo test --locked --test memory_history --test memory_revisions -- --ignored`：exit0，history 6 passed/0 failed/0 ignored；revisions 8 passed/0 failed/0 ignored。这里显式执行需要独立数据库的ignored测试，不能把默认跳过计为通过。
3. `cargo clippy --locked --lib --test memory_history --test memory_revisions -- -D warnings`：exit0。
4. `rustfmt --check --edition 2024 src/adapters/memory_commands.rs src/service/memory_commands.rs tests/memory_history.rs`：exit0。
5. `.venv/bin/python scripts/check_phase0.py`：exit0，M0 PASS；该检查不执行产品行为场景或14天观察。

报告的sources_before/sources_after一致，cleanup.passed=true，无基础设施异常。真实模型调用0；原修订回归中的模型场景使用既有FixtureProvider，不是行为验收。数据库单线程，重型编译并发与旧设置一致，总锁未拆分。

## READY文件摘要与下游依赖

| 文件 | SHA256 |
| --- | --- |
| `src/adapters/memory_commands.rs` | `593b14905113d0be9ef36ad37ebaed3c9e6878c82816e9ddc4e91aec801d9779` |
| `src/service/memory_commands.rs` | `9ade8cde38e3fa2a7635063ad9fab8d33015a9ec24c4fcd7354250dbf27a3bfb` |
| `tests/memory_history.rs` | `4a2764f9bdcbc81df3c3d84f3e64872667efa255369fb32c315b7b2a810762b1` |
| `tests/memory_revisions.rs`（未改） | `51894cb8ee7bdd4c3dd03dcc6e02cd60558896d2dfa75bc6db5e74533f59b8c7` |
| `evals/results/p04-history-targeted-20261112-v2/report.json` | `9c279845739783c781b86e8ae15ff95888c8c9cb8e6746af669f5402dd147571` |

下一项依赖：主线/P03按摘要纳入下一冻结候选版本，P02消费已冻结GET完成历史展示。P03负责完整工程矩阵，P01负责行为验收；本窗口不部署、不初始化实际个体、不修改权威进度。其他个体隔离由真实单个体CHECK/FK与外部ID404验证，未制造合法第二身份或绕过约束。跨页当前视图允许并发追加，不承诺长期固定历史快照。没有扩大本窗口所有权。
