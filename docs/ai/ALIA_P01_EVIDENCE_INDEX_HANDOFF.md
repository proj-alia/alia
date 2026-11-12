# P01-EVIDENCE-INDEX-01 交接

## 改动

建立 `evals/p01/social_delivery_index.json`，索引社会状态审查和社会写入场景交付的 12 项已有证据；新增 `evals/p01/test_social_delivery_index.py`，只离线检查文件哈希、必需字段和记录的验收门槛。未修改采集器或判定器。

索引 SHA-256：`2703516cbac08fd64abe39adba0ca0c3483ca61da47af65e2663efaafdb33655`。测试 SHA-256：`a86dbe6660cd5b1d91863ed49ef3cf9d878c2f837528a794fac41c746ad461a0`。

## 已有证据与边界

固定 v2 输入上的历史报告记录 PostgreSQL 测试 3 通过、0 失败；该固定输入的严格 Clippy 退出码为 101，问题为 `clippy::question_mark`。这些是既有结果，不代表当前代码重新验收通过。社会写入材料包含 8 个场景，其结构测试先前记录 4 项通过；这不代表模型、数据库或生命周期行为已运行或通过。社会语义、社会写入行为及生命周期均未验收；14 天真实运行证据仍缺失。

原有验收门槛保留：检索 100 条、Recall ≥ 0.9；行为 60 条 × 3、比例 ≥ 0.9；成长 20 条，C ≥ 16。正式验收尚无结果。

## 本次验证

`.venv/bin/python -m unittest discover -s evals/p01 -p 'test_social_delivery_index.py' -v`：退出码 0，1 项测试通过。只读取本地文件并计算哈希；本任务模型、数据库、Docker 调用均为 0。Git 状态查询因仓库所有权保护失败，未更改全局配置。索引中的两项交付证据使用本机 outputs 绝对路径，移动文件后检查会失败，需要明确更新路径和哈希。

## 中断和下一项依赖

P01-CAPABILITY-PROTOCOL-01 遭平台内容审查 systemError 中断，测试未运行，既不是产品通过也不是产品失败；按指示不重试。准备文件从 `tests/skill_protocol_regression.rs` 移至 `work/p01-capability-protocol-01/interrupted-not-run/source.rs.disabled`，以避免 Cargo 自动发现；原路径和新路径的 SHA-256 均为 `b1c73bf3a1a50d3b76404bb76f7c250c83004fb0f0e3cc1f1b86e809934a7c43`。另保留 `work/p01-capability-protocol-01/source`。本次未执行该测试。

后续需由协调者指定下一项工作；正式验收仍依赖缺失的语义、行为、生命周期及真实运行证据，并保持原有门槛。


## MODEL-DISPATCH-01 策略接收（2026-11-12）

本轮 owner：P01；派发和后继任务裁决：Root。当前证据索引已完成，COLLAB-02 r4 固定副本的有界独立审查已批准，结论及源码/依赖 SHA 见 `ALIA_COLLABORATION_02_REVIEW.md`。本轮仅记录派发策略，不新增评测或审查结论，不重复 r4 验证。

本轮派发显式指定实际开发模型 `gpt-6.1-sol`、effort `high`；运行时模型指令同样标识 `gpt-6.1-sol`。effort 依据本轮 MODEL-DISPATCH-01 派发记录，不由用量倒推；本条不声明额外的 token/费用统计。

后继纯材料和证据索引任务须显式派发 `gpt-6.1-terra` / `medium`；复杂来源、安全、语义独立审查须显式派发 `gpt-6.1-sol` / `high`；疑难架构由 Root 决定并派发 `gpt-6-astra` / `high`。本轮没有切换在途模型，也没有创建新 Goal。既有 Goal、证据、验收标准及作者不得自批规则继续有效；需要独立审查的后继实现仍须非作者 reviewer 按精确版本复核。

本次 delta 仅为本节策略及模型/effort 来源记录。实际操作为读取并追加专属交接文件；未启动新任务评测、收费 trial、数据库或 Docker，未重试平台中断协议。下一依赖为 Root 的具体有界任务派发；当前轮结束。
