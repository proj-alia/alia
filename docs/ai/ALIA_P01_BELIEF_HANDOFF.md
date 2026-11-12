# ALIA-P01-BELIEF-01｜READY 离线交付

状态：本任务限定的用例、完整性检查和免费正反例测试已完成，READY。真实信念行为语义仍 unverified，live_model_pass=null；没有DB/模型调用。旧ALIA-P01/N2报告保持，本任务不补判完整M1/M2或旧八判据。

## 授权与基线

主线协调文件“P01-BELIEF-01 released — explicit new-file ownership clarification”已明确释放以下四文件；初始记录中“尚未登记”是当时误读，现已纠正，不是等待条件：

- `evals/p01/belief_cases_v1.json`
- `evals/p01/belief_integrity.py`
- `evals/p01/test_belief_integrity.py`
- `docs/ai/ALIA_P01_BELIEF_HANDOFF.md`

原型曾使用其他新文件名，已将本任务自己的草稿移至`work/p01-belief-provisional/`，不属于READY输入。未修改旧八观察器、收费driver、核心schema/领域/生产实现、contexts、其他任务的文件；未创建分支、提交、部署或生产个体。

固定依据为只读接受副本`evals/results/accepted-slices/root-belief-02/source/`，manifest SHA256：`61810ddccbb9748a3111335df385fc12c5a0c104338dbe3f26e1132a14586a6b`。重新验证manifest与八个源文件哈希全部匹配，完整列表在cases及offline-validation报告。此适配器只支持`target-citations/4`。v4没有模型belief写接口，但内部Store.record_belief已存在；不能说信念永远不可写。ROOT-BELIEF-03/v5是主线后续工作，未知版本、baseline或capability返回incomplete/unverified，在早期门控停止，不借v4缺接口解释新版本。

## 用例与判定边界

7个冻结用例均列出required_observations、synthetic_source_setup、forbidden_semantic_conclusions、unverified_when和independent_semantic_question：

| ID | 核查内容 |
|---|---|
| PB-01 | 当前held立场不等于世界事实；信念ID仅证明个体立场 |
| PB-02 | 支持与反证完整保留，含同event双角色 |
| PB-03 | general与task局部例外并存，不跨scope替代 |
| PB-04 | 旧认识superseded，当前认识绑定同scope前身和真实历史覆盖 |
| PB-05 | 撤销祖先的派生闭包、privacy_epoch、context/manifest/packet排除 |
| PB-06 | v4缺模型写接口时不能声称已保存；memory回执或其他run回执不等价 |
| PB-07 | 最多6项且has_more明确，有限选择不证明全部历史不存在 |

检查器只做归一化观察的内部证据完整性：snapshot归属/版本、实际event全文hash、当前问题/manifest绑定、current belief全字段、支持/反证、scope、历史链、来源可见性、撤销闭包、packet逐目标声明集合及角色、候选hash和独立claim标注绑定、回执类型/个体/run/对象绑定。嵌套basis来源不能扩大target的allowed IDs。异常JSON（重复key/非有限数）、过大输入、缺字段或冻结cases哈希变化均拒绝或保留未验证。

`machine_evidence_status=complete`和`machine_ready_for_semantic_review=true`只表示材料齐备。所有输出始终`semantic_verdict=unverified`、`live_model_pass=null`、`provider_self_rating_used=false`。不读关键词、不使用模型supported自评分；有event也不等于事实正确。标注与实际候选含义是否一致、来源是否支持主张、局部例外是否被语言泛化、是否虚构保存，仍须独立语义复核。测试明确保留该能力边界，不能将错误标注包装成语义通过。

## 观察格式与后续采集依赖

输入是`alia-p01-belief-observation/1`，不是生产schema或新增权限。模块make_fixture给出完整合成形状：

- `provenance`：candidate_id、reference_capture_id、baseline_id与manifest SHA。输入文件SHA绑定在CLI报告；这不自动证明collector可信。
- `reference`：权威读取快照的individual/run/state_version/privacy_epoch/current_scope，case_focus_complete覆盖声明，focus belief，belief含status/available/supersedes的验收投影，event_visibility及实际event canonical SHA，必要时history_complete_for_focus。
- `context`：实际作者所见CurrentBelief/SelectedBeliefs、完整events、current_event_id、input_manifest；packet须为实际v4逐目标证据投影，而非事后按候选补造。reference须独立于待核context采集，不能从被测选择反向构造。
- `proposal`与`review_packet`：实际canonical候选与decision_review/proposal一致；belief材料必须标记individual_stance_not_world_fact，保留全部basis_events及support/counter roles。
- `independent_claim_annotations`：候选canonical hash、all_reply_claims覆盖、独立来源与各claim类别/声明来源；fixture_specification只许fixture。程序不自动将语言映射为claim类别，也不认证人工注记覆盖真实全文。
- `execution_observations`：该固定版本模型belief写能力和实际执行回执；包装中的kind/run/committed归属需由采集器与真实内部BeliefReceipt映射证明，不能将此fixture包装当生产receipt实现。
- `revocation`：一致epoch、case覆盖、blocked对象/祖先及推导边。该表只是采集证明要求，当前未新增数据库查询或真实运行collector。

`data_mode=fixture`只证明合成材料检查。`observed_synthetic`仅接受声明为实际合成任务观察的外部材料，仍标记declared_observation_not_live_execution_proof；现无实际collector/DB/模型集成，不能据CLI退出0宣告运行或来源真实性。

## 实际命令与结果

均在`/workapps/project-alia/alia`执行：

| 命令 | 实际结果 |
|---|---|
| `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_belief_integrity.py' -v` | exit0，36 tests OK |
| `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py' -v` | exit0，80 tests OK（旧44+新36） |
| `PATH="$PWD/.venv/bin:$PATH" python3 scripts/check_phase0.py` | exit0，M0 PASS：5 schema/49闭合对象/10示例/56反例/111追溯/136依赖/9文档/34链接/2来源 |
| `.venv/bin/python evals/p01/belief_integrity.py --fixtures --output evals/results/p01-belief-fixture-integrity-v2.json` | exit0，7 fixture材料齐备，语义全部未验证 |
| 独立8组负例报告生成与断言 | exit0，缺反证、错scope、无回执、memory回执、错run回执、撤销来源重入、新packet、新capability均不就绪 |

全P01测试输出中`Lifecycle engineering FAILED; inspect structured report`来自旧driver的mock负例，测试套件实际exit0/OK，不是本轮执行真实lifecycle。未运行Rust/前端/DB或模型；本离线切片不需要全局heavy任务，也没有持锁启动重任务。

正式证据：

- `evals/results/p01-belief-fixture-integrity-v2.json`
- `evals/results/p01-belief-negative-integrity-v1.json`
- `evals/results/p01-belief-offline-validation-v1.json`（命令、实际exit、日志SHA、基线八源复核）
- `evals/results/p01-belief-ready-v1.json`（最终四文件与报告SHA）

早期`p01-belief-fixture-integrity-v1.json`是草稿时的fixture检查，保留但不作为本次READY输入。cases最终冻结后嵌入checker的SHA，未在冻结前启动收费或真实模型试跑。输出采用新文件并拒绝覆盖旧报告。

## READY 源码 SHA256

| 文件 | SHA256 |
|---|---|
| belief_cases_v1.json | 30cd9676ecc9662ffc93f591c5cc36d82b4b749127e7dd2fc106f3667954fdac |
| belief_integrity.py | 3befae951fd5c999ef9c1f8a171c2fe5a09f15afc307a4e8484df93ae635003d |
| test_belief_integrity.py | c7edff87f19647e5e04c013543fb542b46d71e8026f37b377e0e9cab14f01c43 |

本handoff自身SHA在外部ready报告记录，避免自引用。尚无真实行为通过。下一依赖由主线选择固定候选：若选v5须新增明确适配及对应免费反例，再排队实际collector、来源独立比较和单次真实模型任务；不能将当前v4报告换名作为v5验收。本限定离线Goal在以上交付核实后complete，主线M1—M5与真实行为目标状态由主线维护。
