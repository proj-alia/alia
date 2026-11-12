# COLLAB-02 v1 独立审查

结论：**暂不批准**。以下问题需由作者修复，相关代码及测试变更后按新 SHA 独立复审。此审查检查元数据工具及其测试，不证明声明的审查、命令或工程事实真实；身份字符串和 Root 标记不是鉴权机制。

## 版本与范围

- 审查者：P01；作者：COLLAB-02 实现者。审查对象由协调者指定，四源审查前后 SHA 一致。
- `scripts/qa/collaboration.py`：`49bfec65343ba64ea832c552eceb2a8e971727168f3e3603a32091630d14d5d7`
- `scripts/qa/development_usage.py`：`91bf56dd211d2c894d65adb06cb35c214f6e4d994cf3686cc73261abe154639e`
- `evals/p03/test_collaboration.py`：`a6e5793b5b4036900b69ec09572224ff4d9bb42485a91438cb11b482e7b2af93`
- `evals/p03/test_development_usage.py`：`26edfda1ae55304423f6dec2bdd2bd73eb41ab1f591a92494dec85f1d0fb96e1`
- 读取的共享依赖 `scripts/qa/common.py`：`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`。接口语义为协调者指定的 COLLAB-02 v1：元数据不构成鉴权或事实证明；通知 claim 至多一次，崩溃后结果未知；usage 按 response 去重，排除继承与累计；未知费用不当作零。

## 发现

### P1：同版本、同状态的新证据无法通知

`collaboration.py:106-122` 的通知输入只允许 task_id、revision、state、subject_sha256，去重身份只取前三项；`subject_digest` 在第 12–21 行不包含 reviews 或 validation。因而同一任务版本在 review 状态新增审查证据或在同状态补充验证证据时，subject 不变，第二次返回 `already_claimed_outcome_unknown`，没有字段表达证据增量，也不能形成新的有界通知。这与指定的“ID/revision/状态/证据增量；无变化不反复转发”不一致。测试第 93–105 行只覆盖相同事件和 revision 改变，未覆盖同状态证据增量。

建议由作者定义可验证的证据增量标识及其版本/去重规则，区分相同增量重放和新证据；保留先 claim 后崩溃结果 unknown、不得自动重发的语义，并补充并发和崩溃边界测试。不要把 subject digest 当作审查事实或鉴权。

### P2：不受约束的元数据可把私人正文带入报告

`development_usage.py:25-26, 54-60` 直接将输入 turn_context 的 model/effort，以及调用者指定的 thread、turn、task 字符串作为报告字段。它不复制普通 response_item 正文，但没有验证这些元数据是否确为允许公开的标识。独立内存夹具把合成标记放入 model 字段，`summarize` 的 JSON 输出包含该标记；这是输入字段污染场景，不代表已发现真实隐私泄漏。现有 `test_development_usage.py:64-71` 只将标记放在被忽略的 response_item，不能覆盖该边界。

建议明确可信输入和脱敏/拒绝策略，对所有输出标识实行有界类型和安全验证，避免输出原始不可信内容，并用合成敏感标记覆盖每个可透传字段；错误及诊断也不得回显正文。若设计只支持可信元数据，必须明确该限制，不能笼统保证报告不会含私人正文。

### P2：跨时区时间排序错误

`development_usage.py:67-73` 在保留原偏移的 ISO 字符串上使用字典序 min/max，不能比较不同偏移的实际时间。独立内存夹具输入 `10:00+02:00`（08:00 UTC）与 `09:00+00:00`（09:00 UTC），实际输出 first 为后者、last 为前者。应比较 timezone-aware datetime 的实际时刻后再序列化，增加不同偏移和同一时刻测试。此问题不影响已测的 token 加总，但会误报记录时间范围；provider latency 仍为 unknown。

## 已核对边界与验证

静态检查显示 owner 出现在 reviewers 时拒绝；ready 要求指定 reviewer、绑定 subject、证据文件哈希和验证记录；源码哈希变化会被拒绝。其真实性依赖可信注册表、调用方和独立复核，文件存在和自报 exit 0 不是事实证明。重复 response 内容相同只计一次、冲突报错；不读取累计/Goal 计数；未知 token 字段分别记录 unknown count，actual_cost 为 null。通知仅代表原子 claim，不能证明投递成功；进程崩溃后结果未知、不自动重放。以上为本次静态审查范围，并非实际工程或行为验收。

协调者报告四源原单测 7+6 项通过；本次没有重跑这些测试，因此不将其作为独立复验。此次实际运行一段纯内存 Python 夹具验证元数据透传和跨时区排序，退出码 0；四源和共享依赖哈希再次核对一致。未运行 Cargo、数据库、Docker、模型或生产行为；未修改实现和测试。

## 交接规则与下一依赖

下一 revision 的交接须列 owner/reviewer、源码/依赖/接口版本、审查结论、精确证据与 delta；作者不可自批，相关代码改变须重新审查。通知仅含 ID、revision、状态、证据增量，无变化不反复读取或转发。等待作者修复并提供新版本 SHA；此报告只针对上述固定版本，不覆盖后续修改。


---

# COLLAB-02 r2 独立复审

结论：**暂不批准 r2**。以下只针对本节固定版本；上面的 r1 保留历史，r1 通知缺口不再作为 r2 未修复项。审查者 P01，非实现作者。代码、测试和说明文档审查前后哈希一致：

- `scripts/qa/collaboration.py`：`8db54bd9d5c04cbaed910c17a96b9a028179dbfefb28ec42b1d60d36740cd3de`
- `scripts/qa/development_usage.py`：`91bf56dd211d2c894d65adb06cb35c214f6e4d994cf3686cc73261abe154639e`
- `evals/p03/test_collaboration.py`：`f90ff38d4ebe686e9ec222c7addf393ab7c6eac21c99167f55dc57316cde290f`
- `evals/p03/test_development_usage.py`：`26edfda1ae55304423f6dec2bdd2bd73eb41ab1f591a92494dec85f1d0fb96e1`
- 依赖 `scripts/qa/common.py`：`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`
- 接口说明 `docs/ai/ALIA_COLLABORATION_02.md`：`63246e14bca89ee316194238a0decac5c58432773b0918488591cb667f7bc4df`

## r2 新发现：P1 依赖缺少版本与摘要时仍可通过

`collaboration.py:69-73` 使用 `.get()` 比较依赖版本和摘要，未要求两者存在或格式正确。当 card 的依赖为 `{"D": {}}`、登记表的 D 仅为 `{"state": "accepted"}` 时，两边缺失字段均为 None，比较通过。独立纯内存夹具以 mock 替代文件哈希部分，其余使用实际 `validate`，实际输出 `empty dependency version accepted: True`，退出码 0；该夹具只证明元数据检查漏洞，不声称执行了真实任务。现有 `test_collaboration.py:70-81` 只测有效 revision/hash 的接受和变更，不覆盖双方字段缺失或非法类型。

建议要求依赖 revision 为非 bool 正整数、subject_sha256 为完整规范 SHA-256，并分别检查卡片和登记项；补缺失、null、布尔、错误格式、双方相同非法值反例。鉴权与登记内容的真实性仍由外部约束，不由此工具证明。

## r2 对 r1 发现的处置

- 通知证据增量：`collaboration.py:107-133` 已按 task/revision/state 固定 subject，并以 evidence_sha256 分别 claim；静态检查及作者测试覆盖重复、证据变更和 subject 冲突。r1 原缺口在当前版本已处理。原子发布来自固定的 common.py；claim 仅说明本地获得处理权，外部投递或崩溃后的结果仍未知，不得自动重派。本次未重跑并发测试。
- 私人正文：用量实现和测试哈希未变；r1 合成元数据透传复现仍适用，且当前接口文档第 35 行无条件声称不含私人正文。尚未证明真实数据泄漏；需要可信边界/约束和相应测试，或明确且可靠地限制可输出标识。
- 跨时区排序：用量实现未变；r1 纯内存反例仍适用。不同偏移的 ISO 字符串字典序不能表示实际时间先后。

## 其他范围与交接

状态名已对齐所提供清单，open_blockers 明确拒绝 bool；subject、审查者与证据哈希检查仍是元数据校验，不是身份鉴权或事实证明。调用者应验证当前登记状态，不能把历史通知自动变成权威状态；人工通知不受工具追溯控制。作者报告 7 项单测重新通过及 4331 请求/143 分组基线、M0 通过；本复审未复跑、未读取私人用量基线，也不独立确认这些数字。除上述内存夹具和 SHA 读取外未运行数据库、Docker、Cargo、模型或行为验收。

后继 revision 保留 owner/reviewer、源码/依赖/接口版本、精确证据及 delta；作者不可自批，相关代码改变后按新 SHA 独立复审。通知仅 ID/revision/状态/证据增量，无变化不反复读取转发。等待作者修复并提供最终版本。


---

# COLLAB-02 r3 固定副本独立复审

结论：**暂不批准 r3**。本节审查的唯一源码是 `evals/results/collab-02/r3/source` 固定副本；未以 live 文件替代。P01 为审查者，不是作者。审查前后各文件哈希一致并与 `source-manifest.json` 对应：

- `scripts/qa/collaboration.py`：`8db54bd9d5c04cbaed910c17a96b9a028179dbfefb28ec42b1d60d36740cd3de`
- `scripts/qa/development_usage.py`：`57c6ad08037436204bc4b5f1288addbf99ba52453136760ad03a9fe43008f55b`
- `evals/p03/test_collaboration.py`：`f90ff38d4ebe686e9ec222c7addf393ab7c6eac21c99167f55dc57316cde290f`
- `evals/p03/test_development_usage.py`：`018ff3d3e954f38f1e933dfb5de62ba76560452d7bb116065ade1b2f5ef8f33a`
- `scripts/qa/common.py`：`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`

## 未关闭：P1 依赖空版本被接受

r3 的 collaboration 源和测试与 r2 相同；第 69–73 行依旧将两个缺失的 revision 和 subject_sha256 比较为 None == None。对 r3 固定副本独立执行内存夹具、仅 mock 文件校验：卡片依赖 `D: {}`、注册项 `D: {state: accepted}`，实际 `validate` 返回 passed=True，命令退出 0。因而 r2 的 P1 仍未关闭；须校验卡片及登记项的正整数 revision（排除 bool）和有效 SHA-256，并测试双方缺失、null、类型/格式错误，不以双方相同非法值为有效版本。作者所述“首审三项”不包含后续 r2 发现，本结论不把它误称为 r3 新引入的问题。

## 首审三项的当前状态

- 通知：r3 与 r2 相同，task/revision/state 绑定 subject，同 evidence 摘要重复 claim 被抑制，新 evidence 可另 claim；此前证据增量缺口已处理。claim 后崩溃的外部结果仍未知，不能自动重派；元数据并非身份鉴权或审查事实证明。
- 用量元数据：thread/turn 限 canonical UUID，model/effort 允许清单之外变 null，task 输出摘要；新增固定副本测试覆盖合成私人标记，先前直接透传正文反例已处理。无盐 task 摘要只是稳定关联标识，不保证低熵标签无法被猜出；报告仍应按敏感关联数据管理。
- 时间：记录先转 UTC 再排序，新增跨偏移反例通过。缺失费用和服务端延迟仍为 null，逐字段 unknown 与已知小计分开；累计与 Goal 用量不相加。

## 独立验证和边界

以 r3/source 为工作目录，实际运行 `/workapps/project-alia/alia/.venv/bin/python -m unittest discover -s evals/p03 -p 'test_collaboration.py' -v`（关闭字节码）：退出 0，7 项通过；同命令的 pattern 为 `test_development_usage.py`：退出 0，8 项通过。随后实际运行上述缺版本内存反例，退出 0、复现缺口；验证后五源哈希仍与清单一致。测试为本地合成夹具，不证明实际通知已送达，也不证明工程、行为或产品验收。未读取真实用量报告内容，未运行数据库、Docker、Cargo 或产品模型，未修改生产代码及固定副本。

下一 revision 继续记录 owner、独立 reviewer、精确源码/依赖/接口版本与证据增量；修复后按新固定 SHA 复审。历史 r1/r2 结论保留。


---

# COLLAB-02 r4 固定副本独立复审

结论：**批准 r4 所列固定源码的本次有界工具审查**。审查者 P01，非实现作者。此结论不代表产品工程、行为、真实用量准确性或完整验收通过；r1、r2、r3 的不批准历史保留。审查对象仅为 `evals/results/collab-02/r4/source`，五源审查前后与 r4 manifest 一致：

- `scripts/qa/collaboration.py`：`eb29b79311b04403132ea95d49100b0784f1ba959939a16c70f1dd673771caf9`
- `scripts/qa/development_usage.py`：`57c6ad08037436204bc4b5f1288addbf99ba52453136760ad03a9fe43008f55b`
- `evals/p03/test_collaboration.py`：`dedd6710af0d775ea65faf53bb4dbfde84c5479c17737e589d4dd977d7c26653`
- `evals/p03/test_development_usage.py`：`018ff3d3e954f38f1e933dfb5de62ba76560452d7bb116065ade1b2f5ef8f33a`
- 依赖 `scripts/qa/common.py`：`2e01643abc1b219933bdbc8e2efc352d4966ebb3ac618daea26649dd8d3850c9`

## 修复与验证

`collaboration.py:29-32,68-81` 分别验证卡片依赖及登记项的非 bool 正整数 revision 和 64 位小写 SHA-256，再比较其版本；当前任务登记也执行相同版本校验。新增测试覆盖双方缺失、null、bool、0、错误摘要和大写摘要，以及当前登记 revision 为 bool。先前 r2/r3 内存反例在 r4 固定副本实际返回 `unaccepted or stale dependency: D`，命令退出 0；该夹具只 mock 文件哈希检查。此前合法依赖、来源变更、审查绑定与通知反例继续通过。

从固定副本工作目录独立运行 `PYTHONDONTWRITEBYTECODE=1 /workapps/project-alia/alia/.venv/bin/python -m unittest discover -s evals/p03 -p 'test_collaboration.py' -v`，退出 0、8 项通过；同命令 pattern 改为 `test_development_usage.py`，退出 0、8 项通过。五源哈希验证前后未改变。r4 用量实现与 r3 相同，未知费用/延迟仍为 null，已知小计与缺失分开；task 摘要是可关联标识而非对低熵内容的保密保证。

## 范围和后继规则

通知 receipt 是至多一次 claim，崩溃后外部投递结果未知；元数据字段、文件摘要和声明的成功不是身份鉴权或真实审查、执行事实的证明，调用方和协调者仍需检查实际证据及当前登记。此次未读取实际私人用量报告，未运行数据库、Docker、Cargo、模型或产品行为验收；未修改固定副本或工具代码。

下一 revision 交接记录 owner/reviewer、源码/依赖/接口版本、结论与精确证据、delta；作者不可自批，相关代码或依赖改变须按新 SHA 独立复审。通知仅 ID、revision、状态、证据增量，无变化不反复读取转发。本结论只绑定上列 r4 版本，是否接受交由协调者决定。
