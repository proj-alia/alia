# ALIA-P01-BELIEF-03｜READY：免费兼容准备

限定B03 Goal已完成。实际DB/模型调用0，semantic_verdict=unverified、live_model_pass=null。**最新用户门槛：完整M5工程候选以前不新增任何收费模型trial，包括N6/B03后的独立v5试跑。** 本交付不会释放真实job；随后立即转P01-EVAL-03完整离线材料。此前B02/协调旧文中的“下一N6后试跑”仅为历史，已被覆盖。

## 输入、精确差异与不变量

固定参考为`evals/results/p03/assemblies/individual-workspaces-v5-20261112-n6-base/source`，assembly SHA `c5e45965abaaa2aafd80ca1fec4d20656f216afdad32765a4ddb055a72b2cadc`。373源/75overlay全部实际复核，不使用变化中的live core。N6-base只作为兼容基线，不是完整工程或行为通过。

依据最新更正，旧92个N5核心pin实际恰好5项改变、87项相同；早先三项数量已纠正，没有略过memory_commands两文件。所有92个新SHA仍逐项精确比较，不设wildcard、不删除不匹配项。

| 路径 | 旧SHA256 → 新SHA256 |
|---|---|
| `src/adapters.rs` | `4452bad8f8ac9f25e4044bded69038af1bad50c168b22b45fdec4f4b61519c64` → `829779d98bf6262592df424504751df104dd09e1c27c36bc98f28bd28f08091c` |
| `src/adapters/context.rs` | `86e9295ae56397a7b3f18abdf53deb851762a86f6a22507e7f400e539cb74d06` → `39280b2be431fefcad46860bb86fdba0d0ea5175634fe1e22aad45cb510b4cc3` |
| `src/adapters/memory_commands.rs` | `307ba5c872ae59fc1a2ad88e806da86b59476153f77656732b86cfba4361cd4a` → `593b14905113d0be9ef36ad37ebaed3c9e6878c82816e9ddc4e91aec801d9779` |
| `src/service.rs` | `c1a101e52fe39e90d4173595641aa985168701005519a1b2050240561956b965` → `53933355387a21611a42048da5456ff021467dbc8c67693db1e3772ba7c0fbfc` |
| `src/service/memory_commands.rs` | `d41baf8462d9c16a64e658b2c7f98a305961376ea3681d01e2ece7399aaf7dc8` → `9ade8cde38e3fa2a7635063ad9fab8d33015a9ec24c4fcd7354250dbf27a3bfb` |

有意义的变化：context先以同个体读取直接可用触发，再执行不变的递归来源门；直接Invalid/间接StaleState契约保持。关系/目标/近期事件改用递归可见性、当前个体及filter-before-limit，省略不可用可选项，不放宽必需来源/lease/budget。adapters/service根文件只登记接受的只读资源和GET。两个memory_commands增加只读RR历史、真实更正原因与游标校验；旧owner更正/读取函数的完整后缀逐字相同，未改写入流程。

完整5项unified diff、各diff SHA及87个不变SHA在`p01-belief03-compatibility-diff-v1.json`；`source-controls-v1.json`另外证明driver执行/费用/清理等8函数、完整性checker语义与账本等函数AST未变，verify_inputs全部检查除通用错误名外未变。

四原输入、required_observations、禁止语义结论、unverified条件、语义提问、三调用/三tool与200k/50k预算均保持。对原cases除baseline外的整个对象冻结指纹：`d416008ce4fa6263e22f539f6e84ec75be0103fc6669f8d959d894270ad911fa`。新loader即使case hash已更新，也拒绝偷偷缩短任务/改原判据。cases的task_id仍保留原B02用例身份；B03是同一套用例的明确兼容适配。

v5模型/提案/作者与审查schema/atomic writer/DecisionReceipt/预算/策略均为原字节；review policy仍`decision_grounding/1:6a1adc532e3c502d2ef5b2767f74a951a195901119f240015627ab55ee97d1c7`，固定tokenizer仍`19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d`。migration保持0022，未采用进行中的自传0023/domain/diag23。

## 实际修改与控制

只改原B02拥有的cases基线、checker冻结SHA/原判据指纹验证、driver通用基线provenance与静态错误名/最新M5授权文字、测试和本handoff。Rust collector及shared opt-in observed_provider两个源完全未变，但已在新固定源上重新编译/测试。旧B01、旧driver/八观察器、所有已接受B02材料、旧失败与报告不改。

parent/collector行为沿用B02：默认文件预检，显式冻结source/artifacts/QA/tokenizer，无.env/Compose/live-build回退；whole-parent lock直到唯一最终费用账本与精确拥有资源cleanup，per-run≤3、原日预算、真实时钟、不累加累计快照、不隐藏未知用量、不自动重跑。真实belief仍必须由模型候选/审查/事务形成，不允许fixture预写目标。无实际DB或模型调用发生。

新provenance使用baseline_assembly_id/baseline_assembly_sha256，另存previous_baseline_assembly_sha256和preserved_criteria_sha256，避免把N6错误称为N5。核心错配静态code改为frozen_core_binding_mismatch；拒绝策略未放宽。新增5测试覆盖五pin仍逐项强制、旧四输入指纹、v5/迁移22排除自传、通用provenance；原44测试全部保留。

## 实际验证

| 命令/范围 | 实际结果 |
|---|---|
| 当前checkout定向test_belief_writes_integrity.py | exit0，49 tests |
| 稳定N6-base＋六owned code overlays的全部P01 Python | exit0，123 tests |
| 当前checkout的全部P01 Python | exit0，129 tests |
| 同稳定源cargo test --locked：新例、live_individual、live_uncertainty | exit0，7＋4＋4＝15次纯测试执行 |
| 同稳定源cargo clippy --locked 同三例 -- -D warnings | exit0 |
| 同稳定源M0（项目.venv） | exit0，5schema/49闭合/10示例/56反例/111追溯/136依赖/9文档/34链接/2来源 |
| 两个自有Rust源rustfmt --check | exit0 |
| 独立CLI重新生成并检查B03四阶段fixture | exit0，4材料就绪；runtime_execution_verified=false、语义未验证 |
| 10项关键来源/回执/范围/未知用量fixture负控 | 均不就绪，未按keyword或supported放行 |

Rust/Clippy持全局heavy锁；CARGO_TARGET_DIR明确项目target，jobs4、DEV/TEST_DEBUG=0；源码来自N6稳定副本。所有命令/cwd/exit/log SHA、373源/92 pin及代码交付绑定在offline-validation-v1.json。123和129不是同一套件：固定N6-base少test_diagnostics.py的6项，P01没有删除它，当前129仍实际复验通过。不得把123说成129，或用数目差异补判产品失败/通过。

B03没有失败测试运行；曾在global锁队列等待的同一Clippy父任务已自然终态0，未超时重启。B02初次compile/static/UUID排序测试失败留在不可变原日志与原handoff。测试stdout的engineering FAILED来自免费spy负控，不代表曾执行收费trial。

## READY代码 SHA256

| 文件 | SHA256 |
|---|---|
| `examples/live_belief_revisions.rs` | `dbd91c9532b34a619bd13b02277a6fb5da2dca3cf39c619b4ac6c405b8707835` |
| `examples/support/observed_provider.rs` | `4cdb45c49daf532990ba69d104d13ab07c4a5db4248d80b59e2ae7aaf3095f16` |
| `scripts/test_live_beliefs.py` | `f5e3a649de29ffe5f6d28d5169bc19829da74d4d82229e30c2b9da32d397a13c` |
| `evals/p01/belief_writes_cases_v1.json` | `0ad7bcfc4d771d4523993dc01cc54149217aca5a0c77dfe3a174cb6f02701fdc` |
| `evals/p01/belief_writes_integrity.py` | `8d49d767af18a481cb00f6080a5a68ae72dbf9c19793ca72ea2283e9d5f5300f` |
| `evals/p01/test_belief_writes_integrity.py` | `d55441d07e9fb985feb3b4ea3949ac1ae584ae56d4f73fa62937b8f8460c1e7b` |

本handoff SHA由外部ready报告记录，避免自引用。B02 accepted manifest `7ebb986f418d3249e2da5f8359b0099df688a446c51b94b47801881383bb6ce9`及其七个不可变源都已复核；B01四源未变。

新证据均为`evals/results/p01-belief03-*-v1.json`：compatibility-diff、source-controls、fixture-observations、fixture-integrity、negative-integrity、offline-validation、ready。全部新建，未将B02旧报告改名冒充。

## 限制及下一项

尚未运行真实PG/HTTP collector、新正式N6 artifact预检、实际模型写入/独立逐句语义复核；fixture从未当运行或行为通过。没有新checkpoint/artifacts/full-engineering/部署/M5完成声明。现有v5/92 pins只属于这一历史版本适配；root可持续演进自传/情绪/发展schema，不必等待v5收费试跑。最终完整M5候选须有明确新adapter、来源绑定和相应工程验证，未知接口保持未验证，不将旧v5字段永久套到未来版本。

本有界Goal complete后立即P01-EVAL-03：100条中文检索标注、60中文多轮各3次及阻断负控、20组同条件A/B/C成长、证据采集和独立语义复核准备。原Recall@10≥90%、180次≥90%且泄漏/虚构共同经历/越权阻断、C至少16/20且事实/权限不下降、14真实日观察全部保持；在完整M5工程候选前继续0收费，仅离线准备。
