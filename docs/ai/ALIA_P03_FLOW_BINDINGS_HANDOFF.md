# ALIA-P03-12 / 固定后继绑定与 COLLAB 入口

当前修复版本为 r2，owner=P03，reviewers=Root/P01，状态 review_pending；下文 r1 结果为保留历史，不再申请 r1 批准。P01 原 P1 和反例不覆盖、不改判。

## r2 实施前算法裁决记录

Root 已拒绝 r1 的候选完整性缺口；P01 原反例及 r1 冻结证据保留。r2 的 `candidate-content` 接口值为 SHA256(canonical JSON)：对象固定为 `{format:"alia-candidate-content/1",sources:{路径:文件SHA}}`，UTF-8、sort_keys=true、ensure_ascii=true、separators=(",",":")；sources 为冻结清单全部文件，唯一排除下述六个**精确路径**。新增、删除或修改任何其他源都改变摘要，卡片 interfaces 纳入该值，既有 r4 subject、Root registry 接口和非作者审查随之绑定；入口独立重算比对，不使用 baseline 字符串替代绑定。

元数据胶囊固定为 `evals/p03/candidate-metadata/` 下的 `task-card.json`、`registry.json`、`review-root.json`、`review-p01.json`、`validation.json`、`source-binding.json`。六项均须存在并通过用途/schema 检查；卡/registry 参数只允许对应精确路径。卡和 registry 使用封闭顶层字段及既有 r4 验证；两个 review 文件为固定 format/reviewer/text 三字段，validation 为固定 format/text 两字段；source-binding 为固定 format/algorithm/capsule_paths/content_sha256 四字段，路径列表必须精确等于固定六项、algorithm 固定。其用途仅为 review/validation 证据或来源元数据；没有自定义排除参数，禁止通配路径、目录排除或把代码/迁移/配置加入排除集合。胶囊之外即使在同一目录下的 JSON 也纳入摘要。胶囊证据字节仍由 r4 reviews/validation 的 SHA 检查；胶囊不能授予 Root release 或作者自批。

本修复派发 Sol/high，reason=候选完整性和关键 QA 门禁；实际 turn_context 的模型/effort 字段未由工具提供，派发与独立观测分开记录。仅本 QA 切片，旧11协议 not_run 不是依赖；不改 helper、不跑完整矩阵。

## r2 实际结果与审查边界

入口专项初次27项通过；补充非代码源和旧释放反例后最终29 tests / exit0，包含 P01 原反例及卡外新增/删除/修改拒绝、合法 revision2 配套重新释放和重新审查正例、旧审查不可复用、代码/迁移/配置/通配/目录排除拒绝、胶囊同目录额外 JSON 仍计入、用途 schema/固定路径/必需文件检查。另执行固定后继 9 项及 infrastructure/management/workspaces 三组定向回归，实际均 exit0；各组精确数量见 r2 日志与报告。M0 实际 exit0。未重跑 COLLAB 原16项、全离线套件、构建/PG/HTTP/浏览器/Docker/模型或完整矩阵。r2 本轮命令无失败；r1 初次失败与 P01 复现仍保留。

r2 只冻结七项 QA 交付源及卡/日志，slice 的 candidate-content 值仅供审查这个固定切片，不能作为完整运行候选的释放。Root 后继完整候选必须在最终完整源集合上重算 candidate-content，并更新对应接口、subject、注册/独立审查；把这张切片卡直接放入更大的树会被入口拒绝。全部六个元数据文件必须按上述精确路径/schema 建立，review/validation 引用均仍需具体文件 SHA。Root 负责实际 registry，作者只交 defined/released_by=null 提案，不自批。

已接收 M34-PARALLEL-01 的 M34-QA 后继授权，复用本窗口/Goal，不重置旧11 objective。顺序先完成本12定向交审，再依据 Root 释放的 P04 typed/evidence、P02 trial runtime、P05 learned capability bridge 固定接口/接受包完成 QA 接线与实际工程阶段；未知 ignored 域继续 fail closed，未运行/blocked 不计通过。当前没有这些完整已释放组合输入，不在本 r2 中声称接线或运行完成。QA 独立于产品实现，接口冲突交 Root 裁决；不重复旧矩阵、不替跑停止协议、不新增收费试验。

revision=1；owner=P03；reviewers=Root（代码）与 P01（证据）；状态=`review_pending`。作者没有批准自己，没有设置 `released_by=Root`。既有 Goal、窗口、历史和结果保留。本交付是有界 QA 改动，不是完整 FLOW 候选或产品接受。

MODEL-DISPATCH-01 显式派发本 QA 切片为 `gpt-6.1-sol / medium`，范围为固定接受清单绑定、工程 QA 编排和结果汇总；运行时没有提供可独立读取的 backend 型号字段，因此这里只记录派发，不声称额外测得实际模型身份、成本或效率提升。此前 P03-11 不追溯套用本次派发。权限、并发、沙盒语义或疑难生产缺陷须交 Root 升级处理；本轮未扩展这些语义。

## 改动与固定输入

- `management_browser.py` 在既有十层之后按 p02-05 → p02-07 叠加；固定 pin 分别为 `ee553288cfe5c2437c6b4635d1c40f6bdfdf83617f7b8c1953f303c7bf036b58`、`234a4608bab9b263db68454c5624b20419f344a0b11aa5fec82f7bb5181b797f`。只为这两个固定包识别 Root 四字段封闭 envelope，继续核对每个最终路径。SSE 后继必须有删除前驱清单；未知源哈希、缺清单、未知字段、越界证据路径仍拒绝。
- 两份 `evals/p03/accepted-slices/p02-05.json`、`p02-07.json` 是 Root 接受清单原始字节复制，未改格式、重签或采样 live 源。独立审计实际核对原始 408/413 项冻结源及各三项构建产物、15/2 项接受归档、原始报告与清单 SHA。N6-base 固定归档加这两个接受包构成十二层 scratch 预检，不是 Root 释放的运行候选。
- `run_engineering.py` 已实际接入已接受 COLLAB-02 r4 的 `validate(..., phase='ready')`，新候选须把 task-card、registry 和证据放进冻结源的允许路径，并通过 `--task-card`、`--task-registry` 传相对路径。卡、注册表、r4 helper 和 common 依赖须同时绑定冻结源及运行时 SHA。门禁先于候选资源锁、构建、数据库和浏览器；失败会写失败报告、所有必需阶段保持未验证，空执行集不再得到 subset pass。
- 仅七个完整历史 checkpoint manifest 的精确 SHA 可以保留旧来源，不根据旧名称或任意新树豁免；不追认历史 COLLAB review。旧 FLOW 两次失败报告及 P03-11 原 READY 哈希经前后核验未变。
- 新增 `test_accepted_successors.py` 9 项、`test_collaboration_entrypoint.py` 17 项。Root release、独立 review、依赖/版本/证据变化及调用顺序使用明确的合成 fixture 验证；这些 fixture 不是实际 Root/P01 审批。

依赖：COLLAB-02 r4 接受包 `31589edf23202d8824d6b5e0db3815156d38375a961e941c853f0db637de865b`，accepted subject `0df6b6b49fc0d37a08e032ec4f9ae110d7b19c7ba264b617b1be7da24100fdcb`；接口 common=`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`、task-card=`alia-task-card/1`。没有修改 COLLAB 所有的四个 helper/test 文件或协作规范。原始基线为 `individual-workspaces-v5-20261112-n6-base/assembly.json`，新旧来源与最终文件 SHA 见专项 source-manifest 和 task-card。Root 的资源锁实现保持原样。

## 实际结果与保留失败

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s evals/p03 -p 'test_*.py' -v`：实际 exit 0，164 tests / 0 failures。其中自然包含已有 COLLAB 两组测试，未另起重复 COLLAB 专项任务。
- `PYTHONDONTWRITEBYTECODE=1 .venv/bin/python scripts/check_phase0.py`：实际 exit 0；schemas=6、closed_objects=60、examples=14、negative_cases=78、traceability=111、task_edges=136、documents=9、links=39、sources=2。M0 不执行产品接受或十四天观察。
- 最终入口专项 17 tests 实际 exit 0；归档与十二层组合审计实际 exit 0，七个旧 checkpoint 逐一验证兼容。日志及哈希在 `evals/results/p03/p03-12-evidence/`。
- 入口专项初次 16 tests 有一项失败：拒绝候选后仍为空数据库清理取得 admin 锁。已改为仅有实际 state 才清理，最终专项和 164 项全量离线均通过，没有放宽拒绝条件。
- 归档审计初次 exit 1：P03 freezer 的 root 文件白名单不含原始 P02 清单中的 `compose.privacy.yaml`，不能拿两个不同工具的清单直接判等。诊断后按原 P02 清单逐文件 SHA 校验，并分别校验报告绑定的三项 dist 产物；接受包、源和报告均未修改。初次失败摘要保留，不伪造未保存的原始日志。

## 仍未运行与下一依赖

本轮没有构建/Rust/数据库/浏览器/Docker/付费模型/完整矩阵，也未重新执行 p02-05/07 浏览器；十二层门禁通过只证明固定来源绑定。P03-11 的新 skills ignored 域仍须由 Root 后继显式分派，未知域保持拒绝。协议独立复验仍为 `not_run`，平台 systemError 前未运行 Cargo/Docker，本窗口未替跑或改写绕过。

专项 task-card 及 `registry-proposed.json` 只是可审阅提案：本任务保持 defined / released_by=null；真正的 ready 门依赖 Root 注册/释放及 Root、P01 对当前 subject 的独立审阅。没有把本地测试成功当作实际 ready/accepted。Root 还须对完整后继源顺序、schema29/社交等其他接受包统一组装，释放唯一固定候选后才启动必要矩阵。P03 不自行补造生产后继哈希或 registry 放行。P02-08 的工程评审在其 API/浏览器依赖就绪后进行，不能按仅有契约检查宣称完整功能通过。
