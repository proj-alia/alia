# FLOW-01 / QA锁拆分与首次候选交接

## 任务与改动

- 任务ID：`FLOW-LOCKS-01`（Root协调；不新建Goal）
- 工作流：按用户批准方案保留P01—P05所有权、M1—M5范围与验收门槛。
- 锁实现：`scripts/qa/common.py` 增加稳定资源锁与子进程FD身份校验；`run_engineering.py`、数据库/SQLx、浏览器、P02浏览器、隔离检查和sandbox入口按候选、target、数据库管理、数据库实例、浏览器输出、workspace和sandbox资源申请锁。P01仍可用兼容model lock。
- 验收材料：P03新增资源锁重叠/互斥/FD拒绝测试；补齐七份既有accepted-slice清单的规范副本，逐份SHA与既定pin一致。
- 流程文档：进度唯一入口为`docs/IMPLEMENTATION.md`；协调页为当前窗口与状态索引。

## 实际命令和结果

- `python3 -m compileall -q scripts/qa scripts/test_web.py scripts/test_database.py`：exit 0。
- `PATH="$PWD/.venv/bin:$PATH" python3 -m unittest discover -s evals/p03 -p 'test_*.py'`：122项，exit 0。包含资源锁独立类别可重叠、同资源互斥、继承FD身份拒绝；失败路径报告由测试主动注入。
- `PATH="$PWD/.venv/bin:$PATH" python3 -m unittest discover -s evals/p01 -p 'test_*.py'`：168项，exit 0。
- `PATH="$PWD/.venv/bin:$PATH" python3 scripts/check_phase0.py`：exit 0，M0 PASS（schemas=6、closed_objects=53、examples=13、negative_cases=68、traceability=111、task_edges=136、documents=9、links=37、sources=2）。
- P03真实资源隔离证据来自[并发交接](ALIA_P03_CONCURRENCY_HANDOFF.md)：两个Cargo target、PG写事务、服务/browser、产物、SIGKILL清理均有实际重叠；全局父锁当时保持未变。原始READY和两份报告SHA见该交接。

## FLOW-01候选结果

- 两个READY切片：`ALIA-P03-10`与`ALIA-P03-CONCURRENCY-VERIFY`。最后READY=`2026-11-12T15:13:05.438390Z`。
- 首次快照`evals/results/p03/flow-01-snapshot`含476源，SHA `7563db745c5248645844e438ec4f994d067fb9a41591e85fb0de88d3bea56441`。首跑因运行器PATH无Node/npm并发现7份既有清单未复制到规范源目录而失败；证据保留在`evals/results/p03/flow-01-candidate/report.json`。
- 以原始accepted manifest字节补齐七份清单，逐SHA匹配原pin；使用Codex runtime Node/Corepack npm后重冻`flow-01-snapshot-v2`（484源，SHA `afa573b22e8f21e849fbefc067994985f0466d37a607e53ccefeb418f4629462`）。
- 重试命令：`PATH="/usr/lib/chatgpt/resources/cua_node/bin:/usr/lib/chatgpt/resources/cua_node/lib/node_modules/corepack/shims:$PATH" python3 scripts/qa/run_engineering.py --snapshot evals/results/p03/flow-01-snapshot-v2 --output evals/results/p03/flow-01-candidate-v2 --python "$PWD/.venv/bin/python" --node-bin /usr/lib/chatgpt/resources/cua_node/bin`：exit 1。tool-versions exit 0；management-inputs exit 1，精确错误为`candidate missing or changed accepted source: web/src/features/Memory.tsx`。完整工程矩阵其余阶段均为unverified/not run；未运行真实PG、Docker或浏览器候选。
- 当前实际哈希无已READY版本：`Memory.tsx`、`App.tsx`、`test-memory-revisions.mjs`、`MemoryInspector.tsx`、`test-source-inspector.mjs`（P02-03/04层共6条哈希不匹配记录，涉及5个不同源码路径，`MemoryInspector.tsx`在两层重复）。这些当前哈希在正式accepted-slice manifests中没有匹配项。P04-01/P05-01的旧哈希则已有后续P04-02/P05-02接受覆盖。
- FLOW-01等待时长1419.625483秒；本次候选锁等待0.000014537秒；接受清单补归档为一次QA修复。该项是失败候选记录，不计为完整工程矩阵通过或完成整合周期。

## 限制与下一依赖

- 不篡改P02接受manifest、未回滚P02文件、未跳过来源绑定门。下一依赖是P02窗口对六个变化文件提交新的有界交接和实际验收证据，必要时更新accepted manifest；随后Root重新冻结并运行完整候选。具体门槛与矩阵保持原值。
- 未运行两次完整整合周期，因此尚无提速结论。当前锁实现已通过离线回归，但完整集成候选未验证；不得据此声称M1—M5或FLOW-01工程候选通过。
- 无模型调用、部署或生产资源操作。失败报告、两个快照和候选输出均保留，不覆盖。
