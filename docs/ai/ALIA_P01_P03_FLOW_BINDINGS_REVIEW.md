# P03-12/r1 P01 独立证据与门禁审查

结论：**暂不批准**；一个 P1 候选来源绑定缺口。owner=P03，reviewer=P01；Root 独立代码审查及最终裁决另行进行。本报告不改变释放登记，不构成第二协调入口。

## 固定版本与审查输入

任务卡 subject：`06acaaf9b67d9ce95938545a1d36cfa5309391146a5302a4f0fb7b3e5ee32e37`，revision=1。实际核对 task-card 与 source-manifest 的 7 项源绑定一致，逐文件 SHA 与交付清单相同，subject 重算一致；baseline 摘要和 5 份 validation evidence 摘要也一致。固定审查副本在 `work/p01-p03-12-r1-review/source`，含 7 项交付源和 8 项测试依赖；其精确哈希清单为 `work/p01-p03-12-r1-review/review-inputs.json`，审查前后 15 项字节哈希一致。

核心版本：

- `scripts/qa/run_engineering.py`：`0f28b9a496da046f50380fe43c72681acc47c410975536e9edfb8caac79e1237`
- `scripts/qa/management_browser.py`：`fbba92c0e625199e945bc2a743e03b544f2edeed45c5ec190e124f218c3ccfa2`
- `evals/p03/test_collaboration_entrypoint.py`：`a579735b1e781e5a513365cd72cc10c6197d09526c2f3dbd63b80f5d9d07f3a5`
- `evals/p03/test_accepted_successors.py`：`5a3f7823f4ae4e825b5ca4b0aea3ec5cad69ba9414a07e10375c7a7ca13a5958`
- COLLAB r4 helper：`eb29b79311b04403132ea95d49100b0784f1ba959939a16c70f1dd673771caf9`；common：`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`。task-card 接口为 `alia-task-card/1`。

## P1：已审查任务可用于含未审查新增源码的另一候选

位置：`run_engineering.py:46-83`。入口验证冻结树内部一致、卡/登记/helper 在冻结树内，并调用 r4 ready 门；但未把这次 Root 释放及非作者审查绑定到候选的完整源码集合。r4 只要求卡片 sources 对应文件存在且匹配，其 baseline 字符串不与候选清单核对，也不要求卡片 sources 包含整个候选。因此另一棵新增未审查源码的树，只要仍带同一卡片及其旧绑定文件，便能复用同一 subject、审查记录和释放登记通过本门。来源冻结证明字节稳定，不能证明这些新增字节已获这次候选释放或审查。

精确反例：使用冻结副本的 `test_collaboration_entrypoint.CollaborationEntrypoint` 合成 fixture（其中所有审批均为合成数据），先执行 `case.check()` 并保存 manifest 摘要；然后向 `case.source/src/unreviewed.rs` 写入仅含注释的文本，将 `case.checkpoint` 改为新的目录，再执行 `case.check()`。未改变 card、registry、review 或 evidence。实际两次 passed 均为 true、subject 相同、manifest 摘要不同，新增路径不在 card.sources。反例结果保存在 `work/p01-p03-12-r1-review/candidate-binding-counterexample.json`，命令退出 0；没有编译或执行该注释源码，也没有启动重资源步骤。

本发现不是要求元数据工具承担鉴权，也不证明当前提出的 P03-12 卡片伪造：它说明新入口只绑定任务切片，尚不能兑现“新候选不能跳过该候选的 Root 释放/独立审查”的门禁要求。现有 17 项测试覆盖卡片内部陈旧/缺审查/文件变更，但未覆盖保持旧卡、给候选增删改卡外源码再冻结的情况。

建议 P03 与 Root 明确候选层绑定：对独立的完整候选源摘要或可信基线加完整 delta 生成 Root 释放及非作者审查绑定；卡片、登记及审查元数据本身可通过明确排除规则避免自引用。新的候选完整集合变化应使旧释放/审查失效，而不是只检查切片内路径。请补新增、删除及修改卡外源码的拒绝反例和合法新 revision 正例，由 Root 统一裁决接口；P01 不自行修改 helper 或生产规则。

## 本轮实际验证及未运行范围

在上述固定副本执行 `PYTHONDONTWRITEBYTECODE=1 /workapps/project-alia/alia/.venv/bin/python -m unittest discover -s evals/p03 -p 'test_collaboration_entrypoint.py' -v`：退出 0，17 项通过；pattern 改为 `test_accepted_successors.py`：退出 0，9 项通过。另执行上述独立候选反例，退出 0、复现缺口。没有重复 COLLAB 原 16 测或完整离线套件。

静态及合成测试核对：p02-05/07 固定 pin、封闭 envelope、前驱清单和最终路径按顺序匹配；未知或变更的受接受包约束路径被拒绝。历史豁免以七个精确 manifest 字节 SHA 匹配，并先 verify 冻结内容及 execution inventory；仅复用历史名称不能获得豁免。预检拒绝发生在候选资源锁、Popen 和 DB provision 前，空执行集 subset=false，必需未运行阶段为 unverified，完整 passed 要求所有必需阶段通过。上述判断不覆盖卡外新源码已审查与否，见唯一 P1。

读取了 accepted-inputs/review-delivery 及其摘要绑定；其中完整候选、Rust/build/PG/browser/Docker/model 和真实独立 release 均明确未运行。该交付保持 review_pending，没有把本地测试成功转为实际 ready。原历史 FLOW 失败不由本报告改判。未重跑旧浏览器、完整档案审计、整矩阵、收费 trial 或产品行为；平台中断的能力协议组合继续 not_run，完全不在本任务范围。

## 模型记录与后继依赖

MODEL-DISPATCH-01 本任务显式派发 `gpt-6.1-sol/high`，理由为 QA 来源绑定和误报通过风险；运行模型指令标识为 gpt-6.1-sol。本轮工具没有提供可读取的实际 turn_context/effort 元数据，故实际 effort 尚未独立观测，不能把派发或已保存设置当成切换证据。未声称额外费用或节省。

下一依赖：P03/Root 对完整候选绑定缺口给出修复及新固定版本，由 P01 对 delta 复审；不要求暂停 Root 其他实现。后继交接继续列 owner/reviewer、源码/依赖/接口版本、精确结果和 delta；相关字节变化须重新审查，作者不得自批。本次只新增本审查文档及 work 下审查证据。
