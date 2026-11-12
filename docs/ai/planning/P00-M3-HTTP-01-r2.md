# P00-M3-HTTP-01｜发展读取与 owner 撤回 HTTP 接入

| 字段 | revision 2 |
| --- | --- |
| task_id / state | P00-M3-HTTP-01 / defined，待 Root 审查；不是实施授权 |
| owner | 建议 Root；若已有同范围任务，由 Root 合并，禁止重复释放 |
| reviewer | P04：发展事务与版本；P05：隐私、来源和回执重放；均审精确实施版本 |
| 后继工程／界面 | P03 固定候选工程验证；P02 沿原 P02-08 接浏览器验收 |
| 相对 r1 | 固定两个已归档依赖；明确逐 development 版本与 adapter 内部全局 CAS 的边界；r1 保留为历史草案 |

## 基线与依赖

2026-11-12 定向读取确认：`src/service.rs` 的列表／撤回仍接 unavailable，没有详情路由。Root 当前社会状态写入、P04-11 兴趣修订、P05-08 备份和 P03 接受链任务各自继续，本卡不重复它们。

- P04-10 接受 manifest：`bfb6875b087435eaa4660e2d65a55bdb556a0b6c2ac581307cea04f1c124fccc`；adapter `cc6df7c97362cd7dd645af6cb06ae005594b230e1b4712ebc5d23d25bf18aca4`，见 [P04-10](../ALIA_P04_10_HANDOFF.md)。
- P05-07 接受 manifest：`d7bf94d2444845e9772decd3fd69cfa7e4d48182a5665ff14a766a96d640a3fd`；隐私投影 `0d2cff4d5e0641c31ca3e79382a045d45e8c2dcf442b1cf7fe875eed5e8d4cfe`，见 [P05-07](../ALIA_P05_07_HANDOFF.md)。Root 须与 canonical28/29 及共享入口组合成实施基线，不能拿旧 P04 报告覆盖 P05 新版本。
- [P02-08](../ALIA_P02_08_HANDOFF.md) 已有 DTO、未知结果同意图重放与隐私屏障；未就绪浏览器通过不等于完整 API 通过。

## 目标、非目标与范围

接通真实 GET `/developments`、GET `/developments/{id}`、POST `/developments/{id}/revert` 及实际 management readiness。复用 Store 的 `developments`、`development`、`owner_revert_development`；保留六态、历史、实际目标恢复与来源屏障。

仅 Root 新服务模块、共享路由／management 最小注册、定向 HTTP/PG 测试及必要协议配套。发现必须改领域／存储时修订范围并交 owner 审查。P00 不编辑实施文件。

不实现模型发展提案、trial overlay、验证器／评审生成、Accept 入口、兴趣、导出或备份。上述机制不作为开始本接入的前置，但完整 M3/UI 生命周期仍依赖它们。

## Root 释放前确认

1. 选定源码／接口版本、owner/reviewer，确认无同范围在途任务。
2. 对齐已有列表 `include_history`／`has_more` 与 CONTRACTS 全局 `{items,next_cursor}`，冻结详情与 `development.read_enabled`／`revert_enabled` 语义；P00 不自行裁决协议。
3. POST 仅 reason、逐 development 强 If-Match、原幂等键及现有七字段回执。适配器签名为 `(key,id,expected_development_version,reason)`，自行读全局 state/epoch 再 CAS；HTTP 不另收客户端全局版本或做先于适配器的旧全局版本检查。
4. 适配器在新意图字段／生命周期检查前查询幂等回执，但重放仍核查来源和共享门。释放时确认 active/paused 新撤回与 archived/terminated 拒绝的现有语义，以及合法历史重放的处理，不在服务层无意改序。
5. 新增协议由 Root 同步 CONTRACTS、schema、示例、验收。浏览器完整试用记录的可信生成方式明确；未实现的生产验证器不能用直接 SQL 状态或假 JSON 充当成功。

## 验收与交接

- 实际 PG/HTTP 覆盖列表、详情、新事件／修订及目标恢复；auth、严格 query/JSON/header、no-store 和不可见对象沿用现有 API。
- 未知结果同键重放只提交一次；之后全局版本变化也不能被 HTTP 前置版本检查误拒。异意图同键、逐 development 旧版本、竞争 head、事务后段故障分别断言，无部分人格／策略更新。
- 祖先撤销、缺通用边、schema29 removed 状态、旧回执与共享门关闭／迟到返回均不能泄漏失效内容；不能解封旧 ID 或重置新 generation。
- P02-08 使用实际接口运行对应检查；缺可信完整 trial／验证记录的分支显式 not_run。可信合成服务记录只能证明接口边界，不能证明成长或评审质量。
- 按影响范围执行 Rust/Clippy、实际 PG/HTTP、M0 与相关浏览器；完整矩阵按 Root/P03 固定候选节奏。交接附本卡版本、源码／接口、非作者审查及处置、实际命令／退出码、失败／未运行／清理和残余依赖；相关源码变化重新确认审查。

本卡未被接受、实现或验收；不改变完整 M5 工程候选、真实行为及连续14日门槛。
