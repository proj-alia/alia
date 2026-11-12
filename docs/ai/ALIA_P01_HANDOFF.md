# ALIA-P01 行为验收与失败诊断交接

## 初始检查点（2026-11-12）

- 任务：ALIA-P01；窗口 01a1cf0b-7c86-7361-997f-45e8bce24d27；持续 Goal 已建立，active，无 token_budget。
- 主线唯一协调者：规划 AI伴侣方案 / 01a1cbe8-e270-75e1-9535-d3f2e0ea81c4 / local。仅以本文件持久交接，不向其他聊天发消息。
- 当前 release：等待。首次检查未发现 ALIA_PARALLEL_COORDINATION.md；本次仅新增此专属 handoff，未修改驱动/场景/代码/构建/DB/模型。
- 已读取根 AGENTS、三份权威文档与驱动，继续只读核对历史报告及 Rust 观察投影。遵守 Individual/High/Adaptive/Human-Like；不降低目标、不部署、不初始化生产、不改 contexts、不存原始供应商日志或凭据。

### 读取时 SHA256 基线

| 文件 | SHA256 |
| --- | --- |
| AGENTS.md | 79abcc88b4fcc5cabd74cac54423b1a6478ed93bbb3019edc4590b7412857fe4 |
| docs/BLUEPRINT.md | 30ef0bfb9cc9732b234844e7b81515fd1bede9d93c24d00a3b5176d647164943 |
| docs/CONTRACTS.md | b950bc1ca441701a8f643accd443a7bdd9e58fb7f59d7eb8251e3c2ecbf95b6d |
| docs/IMPLEMENTATION.md | 3d52b085713d2cdd8283959e90bba39c8aab29cdeddb14649fbcac03957c2abe |
| scripts/test_live_lifecycle.py | b4139cfc179d3b47b9a7e8be7ffe3a4dc49483d8bf0f161a0a2cdfab309ba413 |

### 初步发现与边界

- 驱动已有 first-study→restore→observe→unknown、独立进程、失败后 bounded usage recovery 与旧报告拒覆盖；将复用，不创建第二执行器。
- 当前 subprocess 输出逐行 JSON 的列表推导会在任一无效行时丢失本阶段其他完整 JSON；latest_exit 在解析之后才保存。DB create 在 try 外，cleanup 的 check=True 可覆盖原失败且未入报告。以上需要离线故障注入后修复。
- 配置尚无二进制目录/固定构建产物标识；需新增兼容默认目录参数并记录清单/哈希，真实运行只采用主线固定检查点与费用调度。
- 阶段 usage 为累计快照；历史汇总不能逐阶段相加重复记账。unknown 费用维持未知，不记零。
- 最近 typed-origins-v4 的前三阶段与专项 atomic-v4 局部成功不替代最终 unknown 失败；独立语义结论必须逐条对照来源及目标声明，不能只信 engine_pass/关键词/supported。

### 实际命令与结果

- 只读 rg/cat/sed/wc/sha256sum：成功获取文档、驱动及现有观察位置；尝试不存在 examples/support/live_report.rs 和 handoff 的 ls 返回非零，未触发执行/收费。
- Python/M0/Rust/DB/真实模型：本窗口尚未运行；无测试通过声明。

### 下一依赖 / 主线可调度事项

1. 登记并释放 P01 文件所有权（scripts/test_live_lifecycle.py、新增 evals/acceptance、独立 Python 驱动/检查器测试及本 handoff）；确认 evals/results 的新汇总文件名空间。
2. P01 释放后完成离线工程、冻结判据、历史来源明确的基线分析与不收费故障路径测试/M0。
3. 主线提供固定源码/配置/模型/二进制 checkpoint 和一次真实联合验收资源锁、费用调度；未获调度不自主调用。
4. Rust 观察字段若需扩充，将在本文件提出精确请求，由主线修改 examples/support 或 src；本窗口不修改公共入口。

## 协调登记已读取 / 历史证据核对

- 已发现并读取主线 ALIA_PARALLEL_COORDINATION.md：P01 held；重型资源 gate CLOSED draining（主线 uncertainty 试验进行中）。保持只读与专属 handoff 更新，不修改驱动。
- 主线登记实际新增文件命名空间为 evals/p01/；将遵守此集中命名空间（含场景、判据、检查器和测试），不扩展公共入口。
- 已核对 typed-origins-v4 的实际四份 JSON：累计 usage 分别3/5/9/11行、18,672/31,137/64,571/77,209 token。最终11次77,209，仅取最终完整快照，不跨阶段相加。报告 SHA256 6349fc7443c3b5d46f4f7465a5ac40ae475a4906e1ce2eea2b127ac590d445c6。
- unknown 作者候选只引用当前 question 5c52a845-bf60-47a2-b395-4b85fe7a19d4，却概括“我们只聊过紫杉手账项目和保存方式资料”；该当前问题并不证明历史范围。独立源比对认为引用覆盖失败；运行时实际 Stopped/Grounding、replies=[]，没有把草稿当已发布历史。此结论不是采用供应商 uncertain 标签代替评审。
- atomic-v4 SHA256 731b28bae1e6ff16af789911eb9d48cb58740301554ca97eceafcf25e8572fd5：11次32,379 token，是一轮真实作者及九个预写审查探针；预写探针不是作者行为留出集。atomic-v3 SHA256 7903ea55e08a03bb6248598d9a7bc569b8e3d58328a525f177edae19d82c6555：7次21,746账本恢复，审查协议失败保持失败。atomic-v1 SHA256 b6f3359655c623f2c6097fd5653a4c87d21704cf49bcf995804dcbd9f015a51f 无reports/账本，费用未知，不能记零。

### 待主线观察字段需求（无需立即扩充，缺失将标未验证）

1. 当前 live_individual usage 无 reservation id/run_id；新驱动失败恢复可读有ID的最终账本，并在成功路径同样保留 bounded 最终实际账本，避免依赖无ID行做去重。仅驱动SQL元数据，不需改Rust。
2. 当前作者/审查诊断为累计 events，候选 run_id 有但回复缺显式 trigger_event_id/source正文；独立语义评审需要 question/source 的固定用例正文及候选声明引用与已提交输出逐项对应。当前已能从固定 example 源文件+SHA256和 JSON 交叉复核，不要求主线为本任务扩大原始日志。
3. 真实联合 checkpoint 应包含 examples/live_individual.rs、examples/live_autonomy.rs、examples/support/lifecycle.rs、examples/support/observed_provider.rs、构建二进制哈希、config/persona.example.json、config/runtime.example.json 与模型/计数profile标识。观察缺失不推定相同。

## 离线工程 ready / P01-SEG-01 前置（2026-11-12）

- P01 release 已确认；修改前 scripts/test_live_lifecycle.py 与主线登记 b4139cfc…ba413 完全一致。
- 已修改：唯一原驱动 scripts/test_live_lifecycle.py；新增 evals/p01/{cases-v1.json,check_results.py,test_driver.py,baseline-review-v1.json,README.md}；本交接文件。未修改 Rust/src/schema/权威文档/上下文/生产状态。
- 驱动保留四阶段与五种 isolated 模式；新增 --binary-dir（兼容默认）/--checkpoint（支持 id+sources 或主线 format+files 清单），无额外模型重试。完整对象不被相邻坏行抹去；退出码先保存；最终账本统一采集后清理；清理不覆盖主因；未知/截断费用 null；所有新输出拒绝覆盖。
- 冻结八项独立语义判据，要求按目标声明引用与实际结果比较；无证据维持 unverified。检查器不做关键词/供应商自评投票。
- `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py'`：实际退出0，25项测试通过（包括多个subTest故障变体），无ignored；全为注入subprocess fixture，无DB/模型调用。日志 /tmp/alia-p01-python-final.log。兼容主线format/files清单后重跑退出0。
- `.venv/bin/python scripts/check_phase0.py`：实际退出0，M0 PASS：5schema/47闭合对象/9示例/52反例/111追溯/136依赖/9文档/34链接/2来源。Rust/DB/浏览器本窗口未运行。
- `.venv/bin/python evals/p01/check_results.py evals/p01/baseline-review-v1.json --output evals/results/alia-p01-baseline-v1.json`：实际退出1（预期的行为失败，validation_errors=[]）。旧报告未覆盖。独立指出未知历史、人物属性和跨目标引用失败；原子探针的真实提交覆盖及旧最终账本完整性未验证。不能把退出1误写为驱动故障或通过。
- 主线已释放一次 P01-SEG-01 uncertainty-only；收费任务尚未执行（此检查点）。已验证新冻结manifest df3a9ba76d6e80ac1c90085c941b8444860d2b1f8d5b0cad97935f9267c2225c：13 source/config 文件及4只读二进制逐字节SHA256相符；未重建/替换它们。
- 现在仅执行登记的 evals/results/p01-segments-uncertainty-v1.json 一次，完整父驱动持有 /tmp/alia-project-heavy.lock，固定 tokenizer，仍不执行未经释放的完整四阶段或重试收费。
- P01-SEG-01 父驱动已启动（本窗口session 75287），当前等待共享flock，尚未创建输出/测试数据库；这是队列等待，不新增第二次任务。执行命令为上节模板加主线已登记的 segments/bin、manifest.json、--uncertainty-only、固定tokenizer及唯一新输出。不因等待超时或无阶段JSON重复启动。

## P01-SEG-01 已自然终态 / 无重跑

- 唯一一次命令实际退出0，父驱动session 75287终态；共享锁已随父进程退出释放。新报告 evals/results/p01-segments-uncertainty-v1.json，SHA256 2ba2ea3efeebc924ae2e57f37cae447ae832ab244389ba4881855e8da20ab52f。engine_pass=true；五例均Committed。
- 最终账本实际恢复11/11行，54,207 charged token，全部settled且usage_unknown=false；输入均与input_bound相等；无截断、无冻结；cleanup passed/exit0。货币价格未知。unknown_event 的审查首次 assessment_schema 后同候选重试成功（该次额外审查已计费），不可把工程重试隐藏成10次调用。
- 独立语义评审进行中：实际未知回答未捏造昵称/比赛/礼物，补充历史现在声明了真实seed/历史问题引用；但仍需区分范围限定的事实总结和穷尽断言。当前观察只返回最终 memories.content，无 memory ID/source IDs，mixed/known引用 a4496ba4… 的对应关系缺少直接投影；不能仅信reviewer的supported证明其对应。将明确记录缺口，不再收费碰运气。
- 已读取四Goal职责补充和八个观察程序released登记。旧冻结二进制、case/config baseline不改；任何观察增强只进入下一检查点，P03负责全量回归/产物。后续完整四阶段仍等待主线独立排队，不把此五例对照当完整生命周期。
- 已读取 P03 初始接口建议，尚未擅自实现共享接口变更：其 --checkpoint 拟为directory，而当前P01 --checkpoint 是manifest path；主线冻结后P01可向后兼容两者。--test-db-state 私有凭据接口与公共产物artifacts.json先由主线登记，避免P01/P03各自设计不同DB行为。

## P01-SEG-01 独立评审终稿 v1

- 来源绑定独立评审：evals/p01/segments-review-v1.json；新汇总 evals/results/p01-segments-uncertainty-v1.independent.json。检查命令实际退出0、validation_errors=[]；behavior_pass=null、case_behavior_pass=null（不是通过）。五例内容均未捏造未知对象，已知事实回忆正确；seed的真实记忆写入可比较通过，其余保留观察缺口。
- 结论依据不是支持标签：未知昵称/比赛候选的具体偏好/项目信息已由真实seed/既有提问引用覆盖，修复了旧“只引用当前问题却补历史”的缺陷；但“当前可见…只涉及/只有”的穷尽范围没有作者实际上下文清单证明，不能用关键词一律判错，也不能用supported放行，维持未验证。mixed/known输出内容正确，但memories投影无ID/source，无法直接核对所引a4496ba4与正文映射，维持未验证。
- 工程成功、事实内容观察和完整语义通过已分开。无实际语义否决不补造为失败；证据缺失不补造为通过。本报告保留审查assessment_schema故障与随后已计费重试。

### 下一检查点的最小观察增强（已获源文件所有权，尚未修改/编译）

1. examples/live_uncertainty.rs 的memories SQL增加 id、version、source_event_ids；每case增加 receipt.event_id/operation_id，直接把question绑定实际源事件，而不是从origin_catalog推定。
2. examples/support/observed_provider.rs 只对合成开发程序，在已解析作者候选中增加 `evidence_view`：本次state/privacy/config/run、实际可见event_ids/memory_ids与input_manifest、每个目标自身declared IDs对应的实际源text或memory.content+source IDs；明确这是本轮选择的上下文，不是全库完整历史。仅保存领域源内容与结构化候选，不保存完整提示词/原始provider输出/隐藏推理。
3. 观察缺口不触发收费重跑。主线选择是否把增强放入P03下一冻结检查点；完整生命周期另行释放。当前代码驱动/判据已ready，Goal仍active，需固定版本完整四阶段报告才能关闭本交付。

## 下一观察切片已实施 / 等P03快照验证（非新收费）

- 修改前已核对 examples/live_uncertainty.rs=0501046e…42b50、examples/support/observed_provider.rs=b0d54196…25db2，与主线transfer baseline一致；live_individual源为主线释放的60bba4ea…3690。
- 已对这三份拥有文件补上最小投影：uncertainty每case增加实际source_event_id/run_id；最终memories加入id/version/source_event_ids；individual记忆加入id/version；ObservedProvider的已解析候选新增synthetic-target-evidence/1，将实际可见event/memory IDs、input_manifest和每目标自己的声明来源正文/记忆版本分开保存。导入元数据只返回无正文投影，不冒充阅读；找不到源明确null。源范围固定标为selected author context，global_history_complete=false。审查的成功结构意见另保留实际provider_model/provider_mode。
- 仅合成开发例子引用此观察适配器；不保存完整提示词、原始provider wire、隐藏推理或凭据。没有改变原案例文本、目标条件、生产src/规则/schema/预算/权限。
- 新增两个Rust纯观察单测，检查元数据/未可见ID不补正文、目标独立来源及可见范围字段。`rustfmt --edition 2024 examples/live_individual.rs examples/live_uncertainty.rs examples/support/observed_provider.rs` 实际退出0（只格式化拥有文件）。Rust编译/单测 **尚未运行**；不将语法格式化当编译通过。
- 请P03在主线批准的下一稳定源码快照中运行定向 `.venv`/Rust检查；建议定向 `cargo test --locked --example live_uncertainty --example live_individual` +相应example check/clippy。不编译/替换旧冻结产物；本观察增强不追改 P01-SEG-01 历史报告。
- 离线driver后续又覆盖KeyboardInterrupt及严格一次性测试库名（避免单阶段中断被标engine_pass），checker覆盖嵌套case协议/语义分类和遗漏case评审。`.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py' > /tmp/alia-p01-python-v3.log 2>&1` 实际退出0，28项通过，无ignored；全部subprocess为fixture，无收费/DB。该driver补丁发生在SEG-01自然终态后，原报告已保留当时driver SHA256，不伪称新补丁经过真实试跑。
- 当前P01-owned文件可供下一次稳定冻结；没有正在写代码/执行模型。下一依赖：主线/P03冻结产物与DB输入接口正式登记、观察增强实际编译结果、完整四阶段唯一收费job释放。Goal active。

## 主线确认 / 稳定点等待

- 主线已核对SEG-01的11次/54,207 token及独立unverified结论，接受上述三份观察增强进入下一候选；明确不追加同组收费。
- 按主线新指令保持P01当前源码稳定，等待P03正式冻结产物/行为DB输入契约和观察编译检查；不再自行修改接口或启动完整生命周期。现有 --checkpoint manifest文件模式兼容性必须保留。
- 当前没有在途进程、没有占用heavy锁、没有新模型调用；P01 Goal active，当前交付不冒称完成完整四阶段。工程/真实行为/未验证结论及所有实际退出码均已记录。

## READY — 共享显式输入契约v1适配完成（2026-11-12）

- 主线补充释放后已仅在自有Python驱动/测试/README接入目录模式。旧 --checkpoint manifest文件与旧examples目录模式保留；新 --checkpoint目录强制显式 --binary-dir artifacts/bin 与 --test-db-state，输入缺失拒绝，不读开发.env/Compose/不构建当前checkout。
- 新模式从checkpoint清单逐字节验证四个QA adapter后加载，禁止写bytecode；实际编译/产物全覆盖交给P03 `artifacts.verify_artifacts(checkpoint,binary_dir)`。执行driver自身SHA必须与快照一致。所有例子/SQL在 immutable source/ cwd运行。
- build证据已按主线指出的真实字段写入 `provenance.artifact_build=artifacts['build_commands']`、`artifact_build_environment=artifacts['build_environment']`及tool_versions_log引用，非原错误build/commands字段；有对应单测断言。
- 只接受 ready/behavior角色、带container_id/image_id的私有state；先验container和volume所有权，每个subprocess前再验；P01只create/drop自身随机trial数据库，P03父任务仍拥有容器/卷，不删除父资源。重型锁由实际P03 common.heavy_lock全程持有/验证，包括最终账本、清理和最终artifact/source复验。最后复验失败保留费用/清理及失败报告。
- 费用/协议/运行时/驱动/语义边界保持。未新增收费任务、未重跑SEG-01、未改Rust观察功能或冻结判据。

### 实际验证

- `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py' > /tmp/alia-p01-explicit-final.log 2>&1`：退出0，38项通过/0 failed/0 ignored。包含旧模式、超时/坏JSON/多JSON/阶段失败/账本失败/清理失败，以及新模式锁全程、final verify失败保留账本、artifact预检拒绝、错误DB角色、资源更换阻断、无.env/Compose回退、缺输入、symlink、snapshot helper哈希绑定及不写pyc。所有过程调用均fixture；另有真实加载SHA绑定的P03 helper函数签名测试，但不运行Docker/DB/编译/模型。
- `.venv/bin/python scripts/check_phase0.py`：退出0，5schema/47闭合对象/9示例/52负例/111追溯/136依赖/9文档/34链接/2来源。
- P03真实两库隔离及helper实现由P03报告，不冒充本窗口运行；P01显式模式真实集成、Rust观察单测、完整生命周期均尚未运行。下个冻结应包含本次driver源码，否则自校验会拒绝启动收费。

### 实际依赖签名（已按P03当前实现接入）

- common.heavy_lock() -> context manager(fd,queue_seconds)；common.clean_env() -> sanitized env。
- artifacts.verify_artifacts(checkpoint directory,binary_dir artifacts/bin) -> (source_manifest,artifact_manifest)。
- database.load_state(private_path)、assert_owned(state,'container'/'volume')、url(state,database)、public(state)、DOCKER。新模式自检查format/role/ready/id；未调用database.provision/cleanup，不擅自新建或销毁P03资源。

P01 当前 **READY/源码稳定**，请主线/P03据下列SHA命名统一冻结；下一依赖为固定产物真实工程结果与主线唯一完整四阶段job/行为DB资源交接。Goal保持active。
86333d7b55890823fae4a0e7d413dd9ed51c6535937318ab192a165addd5f929  scripts/test_live_lifecycle.py
7b5294c38495af9b81ef1067ea24a6456b70109112ebd0da8bdbddf9f0a5619b  evals/p01/cases-v1.json
c26983f9a224574bd977d3426e05b19f8fa8823544793cafff3e3e02ed10ec5c  evals/p01/check_results.py
28d036c20430ebdb67ed98fe61ea88325edf6981ba988ab866c9554eea3c7caf  evals/p01/test_driver.py
4c6d12c5bac638e0199f7da2ed94f43588a109c3ec217290ad44fed5fc34a11c  evals/p01/README.md
e1d27d6d1bab1a3467361fdead1f48946f4cfef17061cadf1b0354fe22a45305  examples/live_individual.rs
460ebce0a6853a4605a8b0cf5825b1193b72046267c1ef2ea74a53ed4e74c437  examples/live_uncertainty.rs
85d5036b66a0f57caa488a40fbd7e26453965d01dcada1435eea5b39c26f5bc8  examples/support/observed_provider.rs

## N1冻结与观察工程实测已核对

- P03已发布并校验 N1：checkpoint evals/results/p03/checkpoints/segmented-console-20261112-n1，278源文件；manifest SHA256 1ff8cdc45323598f4e8e63755fec53148d628b67a311b8f867f4f4a3e8c6633a。本文此追加发生在冻结成功之后，不改写快照。
- 已只读核对P03实际日志：rust-build.log正常结束，dev构建36.00秒；driver-unit.log为38通过/0失败/0ignored；observation-tests.log中live_individual、live_uncertainty各运行两个新观察测试，合计4次实际执行全部通过，均0ignored。编译与测试由P03执行，本窗口仅核对，不复跑其全套。
- P03唯一父session87653、PID1147713已通过ps确认仍存活，完整工程矩阵继续运行。后续DB/浏览器/feature及最终清理/源码完整性按P03最终报告判，不提前标全通过。无新增收费任务；完整四阶段等待主线单独释放。

## N1 P01只读真实产物预检

- 本窗口用N1冻结的driver和SHA绑定QA adapter，实际调用 `load_frozen_qa`→`verify_artifacts`→`collect_provenance`；退出0。确认全量13产物集合/哈希、完整checkpoint源清单、执行driver自身哈希及选用live_individual/live_autonomy二进制均相符；build_commands和build_environment实际存在。
- 新报告 evals/results/p01-n1-readonly-preflight-v1.json；此为真实文件/adapter集成预检，不是fixture模拟，但没有DB访问、模型/例子进程、编译或收费。未触发默认.env/Compose。
- P03父PID1147713仍活着，完整工程尚未终态；本只读检查不代替P03最终报告、行为库资源验证或完整生命周期。仍只等待主线明确新收费job。

## N1工程已终态（只读核对，非P01重跑）

- 原P03父PID1147713现已不存在，最终 report.json 已出现。full_engineering_pass=false；所有非browser阶段均passed，包括177项真实SQLx/HTTP与P02专属浏览器；旧browser因strict-mode定位器歧义失败，restore未执行，不能算通过。两套父测试资源cleanup均passed，费用模型调用0。
- P01读取的是同一个N1输出，不因观察到失败而创建新矩阵/付费任务。N1完整工程失败不被只读产物预检覆盖；P01四阶段报告仍缺，Goal保持active。

## 六项原始要求完成性审计 v1

- 新增只读证据汇总 evals/results/p01-delivery-audit-v1.json：绑定12份实际源/报告/日志SHA256，逐项审计最初1—6要求，明确verified_engineering/partially_verified/incomplete，objective_achieved=false。
- 最主要的未完成项仍是一个新固定版本的完整first-study→restore→observe→unknown联合报告；SEG-01仅五例对照，不缩小交付范围。当前没有该付费job释放，不能自发调用。
- N1工程最终报告SHA256 134d64062d3506cb5b953c35f2574704c6dfc6f3af5a35a3567d6f5a44e5ec7b；full_engineering_pass=false、browser exit1，其余17阶段exit0、父库cleanup均通过。报告不被P01预检或旧例子通过记录覆盖。主线/P03后续修复归其所有权，不改N1。

## 等待审计 1（续行：无新行为job）

- 上一轮已产出真实产物预检和六项证据审计，为progress；本轮重新读取协调及P03终态：P03 Goal已complete、N1父进程不存在，主线登记仍明确no paid job。没有在途P01行为进程/可继续的收费handle。
- 本轮无源码/测试/模型变更，不把重复状态检查算新工程进展。当前唯一阻碍是主线尚未指名固定候选、行为DB资源及完整四阶段一次性费用job；P01不能越权自行收费。初次无进展续行，Goal仍active，未标blocked/complete。
- 完整交付需求保持：新固定版四阶段报告（运行时失败也如实形成最终报告）与独立评审，不以SEG-01五例替代；现有驱动/判据/观察工程READY不变。

## 等待审计 2（同一依赖未解除）

- 已重新核对主线最新协调与实时聊天状态：P02-02新切片已释放，但P01仍无新完整行为job/候选产物/行为库交接，N1工程与SEG-01均终态，没有P01在途模型handle。
- 无可继续且获准的执行步骤；不修改已READY源码来制造进展，不重复收费或将五例对照替代四阶段。此为连续第二次无进展续行，Goal保持active；待主线外部调度变化。

## 用户授权继续后的诊断交接增强（N2冻结后，零收费）

- 已收到六Goal分工及“在观察/驱动/判据内主动补足证据缺口”的继续指令；按新的恢复审计起点继续，不重建/缩小原Goal。已重读AGENTS和权威当前状态/最新协调，N2独立冻结、主线/P02/P04/P05开发不混入行为证据。
- 新增 evals/p01/diagnostics.py、test_diagnostics.py，README补调用方法。没有修改已冻结cases-v1、原driver、Rust观察源码、生产src或N1/N2。新工具只从已验证独立summary抽取证据，复验原始报告SHA与JSON pointer；生成expected/actual、报告/候选/二进制/源版本、实际观察model IDs、责任owner/module及精确来源。
- 类型分开：observed_failure / evidence_gap / observed_runtime_event。审查协议错误即使恢复成功也保留；正确runtime拒绝/语义否决不自动宣称production bug；missing coverage不伪造成缺陷；历史无完整build/config时版本明确unknown。owner为排查入口，非根因证明或越权修生产的授权。
- `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py' > /tmp/alia-p01-diagnostics-v2.log 2>&1` 实际退出0，44项通过/0失败/0ignored；新6项诊断测试均独立合成文件，原38项保留，无subprocess收费、DB或编译。N2冻结内仍是38项旧driver测试，不冒称N2执行了此后新增6项。
- `.venv/bin/python scripts/check_phase0.py` 实际退出0（5schema/47闭合对象/9示例/52负例/111追溯/136依赖/9文档/34链接/2来源）。
- 两次实际生成均退出0，model_calls=0：p01-baseline-diagnostics-v2.json（8条，包含历史作者失败及结构化runtime事件）；p01-segments-diagnostics-v2.json（6条，5条范围/来源证据缺口+1条实际assessment_schema事件）。原v1包、原报告及旧独立结论全部保留。

### 精确缺口与责任入口

- SEG-01冻结版本manifest df3a9ba…2225c，报告2ba2ea3…ab52f：unknown_event/unknown_promise追加历史的具体事实已引用，但可见范围证据缺失；mixed/known所引memory UUID未直接映射到memory正文/来源。责任P01观察投影；已在N1/N2源补上并编译/单测通过，**尚待新获批真实联合报告验证**，不追改旧报告。
- SEG-01 unknown_event的实际assessment_schema失败和同候选重审记为root reviewer协议入口（src/adapters/model.rs / domain/grounding.rs），非新的行为否决或作者错误；全11次费用54,207已保留。责任路由需要主线核查，不由P01修改生产或重复收费复现。
- 历史typed-origins-v4未知历史/人物v3属性及跨目标引用的独立失败仍为历史基线，不冒充N2新缺陷。packet保留旧源SHA和candidate pointers，代码/配置未完整固定的部分明确unknown。
- 当前仍无新的收费job，不初始化/部署；主线给出固定N2（或后继）产物、行为DB及唯一四阶段job后，继续原完整目标。
ddff9ac64d16967d3c56091486b31036e737b9641c2553857a366595da2b8e9a  evals/p01/diagnostics.py
b56c38a6473c0127e8d8fb8becc4aac87491ca94dfe44d7ef1d502bb75454423  evals/p01/test_diagnostics.py
b89e2dda43e2ddd528931538274b06b05af32798e8f4653311a7b50d86780e50  evals/results/p01-baseline-diagnostics-v2.json
2d7d238e55a01efd1c3bcf8acb70e36ae365c5210a4412a4235d4b5981da9580  evals/results/p01-segments-diagnostics-v2.json

## N2 P01只读真实产物预检（恢复后）

- 本窗口使用N2自身冻结driver和绑定helper实际验证：278源文件/13产物及driver自SHA完全相符，live_individual/live_autonomy已按本候选独立产物取哈希；退出0。
- 新报告 evals/results/p01-n2-readonly-preflight-v1.json；checkpoint manifest SHA256 54e4eb000d95f90d36ae92f3d91762da7be797a44876f326d5c1829e254b96e4。build_commands实际存在、build_environment已保存；未使用当前新增P02/P04/P05源。
- 无DB访问/模型/编译、无收费。N2父PID1208628仍运行，完整工程按其最终report判；此预检不放行行为、不重写N1失败或SEG-01 unverified。

## P01-N2-LIFE-01 已正式释放并启动（唯一一次）

- 主线正式登记已读取：N2工程18/18通过，report SHA dd47593ac74dbb2b84826627636bb6182837a84e335dced73e50ba2661d32b06、artifact SHA 83d06783401d687cc2837f829ca84e39086934dac9f52a261f8c3b5c2136a975均本窗口逐字节验证。上一条N2预检中的“父仍运行”沿用前次观察；随后实际ps退出1，N2工程已终态，现以该最终报告为准。
- 已获准写私有父驱动 work/p01_n2_life_01_parent.py（0600），只引用N2冻结driver/QA；AST语法检查退出0。连续持全总锁，经冻结provision创建专属behavior资源；子继承真实FD/private state，只create/drop trial DB，父finally精确cleanup容器/卷，包括部分startup，最终复验源/产物并保存脱敏resource报告。没有defaultCompose/生产操作或预算修改。
- 唯一真实启动命令：`.venv/bin/python -B -u /workapps/project-alia/alia/work/p01_n2_life_01_parent.py`；当前本窗口工具session **20987**，父PID **1252879**，子PID **1253454**。实际started_at 2026-11-12T10:20:22.605668+00:00，排队2.490107秒后取得总锁，已进入running。后续跟踪同一handle，不重启/重复收费。
- 固定child输出 evals/results/p01-n2-lifecycle-v1.json；父资源输出 evals/results/p01-n2-lifecycle-v1.resources.json。完整first-study→restore→observe→unknown，无uncertainty-only；N2冻结source cwd、固定tokenizer、Go glm-5.3-flash和原预算/三次上限/真实时钟不变。`.tmp.env`只由子驱动私读，父不保存凭据/URL/原始stderr或供应商日志。
- 原阻塞条件现因正式释放而解除；继续原Objective，未创建替代Goal，终态后做独立来源/声明/实际结果比较并保留所有未验证项。

## P01-N2-LIFE-01 唯一任务已终态 / 工程通过

- 原session20987自然终态exit0，父PID1252879、子1253454不再在途，绝不重复启动。四阶段均exit0，child engine_pass=true、parent job_engine_pass=true；原目标625f840c-3f7e-4808-8902-f4bd1f63343c原criterion/completion=null保持，后台真实347秒后completed。
- 原始报告SHA256 008f67d1d3e06a3b9208e24fd721d4c01078c77ee22dc666e86adcd0e52d32fa；resource报告SHA256 8d48285e69160d8402f79b896d52ecd27c8e290fd3daa3e151ad550cd2a74f96。child起止2026-11-12T10:20:27.588930+00:00至10:27:34.474834+00:00；parent至10:27:35.617904+00:00。
- 最终恢复12/12有ID账本：全部settled/unknown=false，84,609 token，后台两个run 3+2次合计38,234；first3次19,885，restore2次13,288，unknown2次13,202。不跨阶段相加累计快照。所有input==input_bound，无截断/冻结；货币价格未知。
- 第一后台run因model_rejected/invalid_schema停下（源事件67e19087-582f-452e-befd-4618add4b2a7），没有原始无效供应商正文，字段级原因保持未知。实际资料笔记/目标审查先保存，真实下一到期cycle2次完成；保留该作者结构拒绝和全部费用，不当成审查协议错误，也不隐藏为全程无故障。
- 子trial DB cleanup exit0；父资源cleanup passed、final_integrity true。随后本窗口用N2 frozen inspect独立确认exact owned container/volume均不存在，private state status=cleaned且password字段已移除。无生产初始化/部署。
- 独立语义评审进行中，阶段engine通过不直接作行为通过。初步已对实际3条memory、各目标declared material、原文/引用笔记逐条比对；终稿另存，不覆盖上述原始证据或旧失败。

## 最终工程交付 / 完成性审计通过

### 固定版本首份完整报告

- 综合报告：evals/results/p01-n2-acceptance-report-v1.json；原始四阶段/父资源/独立评审/清理复验/诊断均独立另存，旧失败及unverified记录未覆盖。
- 原创逐项评审：evals/p01/n2-lifecycle-review-v1.json；机器核对summary：evals/results/p01-n2-lifecycle-v1.independent.json，实际exit0、validation_errors=[]。四阶段的本次合成序列正向语义比较均observed pass with limits；完整八判据behavior_pass=null，人物not_run、跨目标/外部动作负控覆盖unverified，不冒称整体行为通过。
- unknown没有编造绰号，限定于目前可见记录；其可见memory IDs及内容现在完整可核对，项目/偏好与声明记忆一致，保存方式资料作为话题已在声明用户消息中。没有以“只有/没有”关键词或provider supported自动判真/假；不证明全库历史/现实从未发生。
- 仍保留restore“等你想推进时再继续”的助手式表达限制；真实后台无新消息完成原目标，不把措辞直接等同实际审批等待。责任入口root author/model guidance，精确来源/reply及后续goal证据均在qualitative_findings；不修改生产规则来过本次验收。
- 第一后台作者invalid_schema的结构拒绝为真实已计费事件，不属于本轮审查协议失败；字段级无效原文未保存，不能进一步归因。诊断包evals/results/p01-n2-lifecycle-v1.diagnostics.json实际生成exit0，保留2项控制覆盖缺口+1项runtime结构拒绝。

### 验证与范围

- `.venv/bin/python -m unittest discover -s evals/p01 -p 'test_*.py' > /tmp/alia-p01-delivery-final-tests.log 2>&1` 实际exit0：44 passed/0 failed/0 ignored，subprocess/file全部fixture，无新DB/模型。N2冻结中实际是38 driver+4 Rust observer，额外6项诊断测试为冻结后离线代码，明确分开。
- `.venv/bin/python scripts/check_phase0.py` 实际exit0；原5schema/47闭合对象/9例/52负例/111追溯/136依赖/9文档/34链接/2来源。回执另存p01-final-checks-v1.json。
- 独立清理复验：evals/results/p01-n2-lifecycle-v1.cleanup-verification.json；实际再次inspect精确container/volume均不存在，private state cleaned、password移除，冻结source/13 artifact再校验通过。
- 六项原始要求完成审计更新：evals/results/p01-delivery-audit-v2.json，15个具体completion checks全为true。审计覆盖原执行器/固定产物/四阶段/冻结八判据/独立比较/真实账本/历史保留/失败类别与不收费故障测试/一次获批实测/M0与清理；未用阶段工程通过替代完整行为或发布。
- 用户可读公开证据副本：/home/zero/Documents/Codex/2026-11-12/alia-p01-acceptance/outputs/ALIA-P01-N2/（8份JSON+摘要+SHA256SUMS）。只有合成结构化候选/来源与公开元数据；无私有DB state、凭据、原始provider wire/隐藏推理、二进制或模型资源。副本逐字节SHA与权威源相符。

### 所有权与下一依赖

- 交付包含原唯一Python生命周期驱动、evals/p01冻结判据/检查器/诊断器与44离线测试、三份已释放Rust观察源码增强和本handoff。N1/N2原快照/二进制未改；不动contexts、核心src/schema/迁移/权限/预算，不部署，不初始化生产。
- **本ALIA-P01 objective为验收工程与第一固定版本完整可核查报告，现已达到；不是M1/M2—M5或完整行为发布通过。** 原始parent执行一次，12次实际调用84,609 token全部已知，后台38,234；无需再收费完成本工程交付。
- 下一工作由主线安排：如需继续专项人物/跨目标/外部动作行为门槛、措辞改进复验或后继候选，须新固定检查点与独立费用job。本窗口不自行越过M2—M5依赖或重复本次收费。准备将原Goal标记complete，所有旧失败/pending仍作为主线依据。

### 最终交付SHA256

947965b0d52bcb686648713454ce700e7ab7f6c6efed4a669d61f3d70fd43c7f  evals/results/p01-n2-acceptance-report-v1.json
e319efc5fb18f4f467b596dee3eeb306dd5ac537b6cd78f40fb7dd7e5faf5d91  evals/results/p01-n2-lifecycle-v1.independent.json
54b544e8577a2f052db57b3ad72d3378d0b78dae0bfc6f8fd6e6e9ec5372d74a  evals/results/p01-n2-lifecycle-v1.cleanup-verification.json
efdcb8fc65e2e13e238c15d9f7a91aa17072dd76aa87cb010d23275f6d3fac0e  evals/results/p01-delivery-audit-v2.json
a7fe71193f5a1f782929cbf527bf2132dd462afdfa3708e164d6c744e4f11ad2  evals/results/p01-final-checks-v1.json
