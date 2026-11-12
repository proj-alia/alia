# Alia 配置、数据与执行契约

版本：0.1.0。设计日期：2026-11-11。状态：M0 规范；本文件没有宣称 API 或运行时已经实现。

存在规则见 [BLUEPRINT.md](BLUEPRINT.md)，用例与任务见 [IMPLEMENTATION.md](IMPLEMENTATION.md)。`必须` 是实现约束；阈值属于可配置工程默认值，不是心理学或意识测量标准。

## 1. 配置边界

M0 提供 JSON Schema Draft 2020-12 与 JSON 示例。首版只接受 JSON，避免 YAML 隐式类型、重复键等解析分歧；以后增加 YAML 仅作为转换入口，最终仍验证同一 schema。

| 文件类别 | Schema 与示例 | 生效方式 |
| --- | --- | --- |
| 人格种子 | `schemas/persona.schema.json`、`config/persona.example.json` | 仅首次显式初始化；保存完整初始文档（规范化JSON）、schema_version、profile_version 和 SHA256 |
| 运行规则 | `schemas/runtime.schema.json`、`config/runtime.example.json` | 所有字段验证；保存版本，影响下一轮运行，不改写当前人格 |
| 模型提案 | `schemas/decision.schema.json`、`evals/decision.example.json` | 每轮输入绑定；结构校验之后继续做领域校验，不直接写库 |
| 成果审查结果 | `schemas/goal-assessment.schema.json`、`evals/goal-assessment.example.json` | 只表达审查意见；来源、修订与预算检查后持久保存 |
| 内容依据审查 | `schemas/decision-assessment.schema.json`、`evals/decision-assessment.example.json` | 覆盖候选回复、新记忆与人物关联；独立计费，绑定候选、上下文与审查策略版本，提交前复验 |
| 部署参数 | 本节环境变量表 | 服务启动时读取；不属于人格和模型上下文 |

所有 schema 对对象关闭 additionalProperties；必填字段不允许靠模型猜测默认值。禁止重复 JSON 键、NaN/Infinity、无效 UTF-8。schema_version 未知返回错误，不静默迁移。未实现能力由运行时能力注册表拒绝，不能因为在配置中出现就当作已经可用。

每份人格/运行配置最多64 KiB。摘要针对规范化JSON计算，对象键排序，空白排版不影响身份或幂等判断；不承诺保留原文件的排版字节。Rust使用同一嵌入schema，由 [jsonschema](https://docs.rs/jsonschema/0.61.1/jsonschema/) 校验，关闭网络/文件引用获取；IANA时区使用 [chrono-tz](https://docs.rs/chrono-tz/0.10.4/chrono_tz/) 验证。运行配置中的 enabled 列表不能充当“已实现能力注册表”；后者必须由实际程序注册传入。恢复时重新检查保存配置，但不重新应用人格种子。

### 1.1 人格种子

包含名称、自我描述、背景设定、气质倾向、认知习惯、价值排序、兴趣、表达风格与初始关系定位。`profile_id` 是模板标识，不是个体身份。`fictional_background` 可以为空；非空时只作为带来源的背景资料，不写成亲历事件。

初始数值使用 [0,1] 表示相对倾向，仅供行为实验校准。当前人格与种子分开存储。改种子不触发自动更新；更新个体只能经状态修订或发展提案。

### 1.2 运行默认值与语义

| 项目 | 默认值 | 执行含义 |
| --- | --- | --- |
| 自主模式 | high，enabled=true | 来源明确的自主目标从 M1 开放；不等于无限执行权限 |
| 模型调用上限 | 3/循环 | 失败和修复重试都计入；禁止递归子循环绕过 |
| 工具调用上限 | 3/循环 | 参数验证失败不执行；实际发送的重试计入 |
| 总循环时限 | 120 秒 | 包括调用与重试；超时保留真实已完成结果 |
| 后台每日用量 | 50,000 token | 输入、输出及提供方计费推理 token 纳入 |
| 全部每日用量 | 200,000 token | 前台与后台共用总账，不包含本地嵌入推理 |
| 空闲检查 | 60 分钟 | 仅确定性检查；无新输入、到期目标或可执行兴趣缺口不调用模型 |
| 主动联系 | 2 次/日，间隔 4 小时 | 包括已预留未提交的联系名额；用户消息回复不计入主动联系 |
| 安静时段 | 22:00—08:00 | 只限制主动通知，不强制停止内部学习 |
| 时区 | 示例 America/New_York | 初始化时显式解析 IANA 时区；无法发现时使用 UTC 并显示实际设置 |
| 网络站点 | 空列表 | 初始禁止公网工具读取；配置后按精确主机列表执行 |

时区按个体实例保存，数据时间统一 RFC3339 UTC。每日预算用实例本地日期；时区变更在现有计费窗口结束后生效，不能通过切换时区获得额外额度。联系间隔按 UTC 实际时长，夏令时不改变四小时要求。安静时段起止相同表示关闭。

预算在调用前预留，完成后按供应商用量结算。持久预留分reserved/dispatched/settled；发送标记只成功一次。reserved中断可释放，dispatched中断按未知用量处理；最终回复失败不免除已经发生的模型费用。未知已扣金额遇到晚到小额回执不退款，超出预留的实际用量补记并冻结后续调用。模型适配器必须提供经验证的输入 token 计数或保守上界，并强制约束全部生成额度；无法建立可靠上界的模型不能进入后台运行。超时/用量缺失扣满预留值，记录 usage_unknown；观察到用量超预留则冻结新调用并报错，不能伪造严格预算达标。

等待工具回执不改变目标状态；包含tool_intents的提案不得同时wait或abandon目标，避免实际行动仍在执行而目标已经暂停。模型可单独提出有理由的等待或放弃。终止时的run.stopped保留固定validation_code与instruction，即使修复次数用完也不丢最后失败类别。结构/领域拒绝不保存原始模型响应；下述内容依据审查另保存结构化候选与意见，明确为待审生成内容，不是已发生经历。

后台余额不足使目标进入 waiting/budget，前台不足返回可见错误。提高预算不自动恢复被用户暂停的活动。重试最多剩余轮次，错误退避 1 秒、2 秒，仍受循环时限限制。

预算是产品模型调用的执行约束。token 效率优化明确针对 Alia 面向用户运行时的前台对话、后台自主活动、记忆整理和内容审查等实际调用及使用成本，不限制开发 Agent 自身的 token 消耗、开发投入或并行实施。产品能力、用户任务完整性和行为质量优先，不得为了节省 token 删减必要证据、跳过校验、简化用户任务或降低标准。优先消除重复上下文、无效调用和序列化冗余并准确计量；检索、摘要和缓存须在相同使用场景下对照质量与实际消耗。运行预算不足仍如实等待或报错，权限、来源和隐私继续由程序执行。本原则不增加配置字段、不改变本节预算默认值、预留/结算/重试及等待契约；产品与测试的实际模型调用均遵守原规则，不代表优化已经完成。

### 新资料机会评估

无排队/运行任务且无proposed/active/waiting目标时，未处理的document.imported可成为一次background运行的实际触发事件；不创建虚构用户消息、不预建目标。模型结合现有兴趣可选择读取、创建有来源目标或不行动。仅Ready或显式Fixture提供方启用；暂停、非active、冻结与递归失效来源禁止排队。前台任务优先，调用仍走原租约、三次上限、隐私与预算结算。

`discovery_occurrences`在同一事务保存来源与运行，源事件幂等；按idle_check_minutes限制两次评估间隔。已经实际成功读过的资料不再次触发这个入口。未发出调用的终态运行可在冷却后重新排队；已使用模型的评估不因失败或空闲自动重做。旧运行及费用历史保留。这是新环境资料驱动的机会发现；仅凭稳定兴趣反复调用模型尚未启用，避免无输入空转。模拟时间仅用于测试冷却，不能计为真实活动。

### 1.3 部署环境

| 环境变量 | 默认或要求 |
| --- | --- |
| `DATABASE_URL` | 必需；PostgreSQL 连接串，禁止日志输出 |
| `ALIA_INSTANCE_TOKEN` | 必需，至少 32 字节随机值；启动缺失即失败 |
| `ALIA_BIND` | `127.0.0.1:3000`；容器内部监听可覆盖，主机端口仍只发布到 loopback |
| `ALIA_DATA_DIR` | `./data`；私有权限，禁止进入版本控制 |
| `ALIA_WEB_DIR` | `./web/dist` |
| `ALIA_MODEL_PROVIDER` | `deepseek`（默认）或 `opencode_go`；未知值报错 |
| `ALIA_MODEL_ENDPOINT` | DeepSeek 默认 `https://api.deepseek.com`；Go 默认 `https://opencode.ai/inference/go/openai/v1`；可覆盖HTTPS基址，不含/chat/completions |
| `ALIA_MODEL` | DeepSeek 默认官方API ID `deepseek-flash`（显示名 DeepSeek-V4.1-Flash）；Go 默认 `glm-5.3-flash`；空值按提供方默认 |
| `DEEPSEEK_API_KEY` | 真实模型必需；缺失进入 model_unavailable，禁止用 fixture 假装真实对话 |
| `OPENCODE_API_KEY` | 仅opencode_go使用；不回退到其他供应商凭证 |
| `ALIA_TOKENIZER_PATH` | 可选本地公开tokenizer文件；仅匹配已审阅Go/GLM路由，固定SHA256、最大32MiB、无padding/truncation。配置无效拒绝，未配置保留字节上界 |
| `ALIA_EMBEDDING_MODEL` | M2 起 `intfloat/multilingual-e5-small`，模型修订与维数另存 |

手动probe的`--credentials`仅接受标题为`opencode-go`、其后`apikey: <value>`的本机笔记分段；其他供应商分段不会参与选择。该分段要求恰有一条有效密钥，禁止按最后条目选取。示例使用虚构值：

```text
opencode-go
apikey: example-go-key
openai
apikey: example-other-provider-key
```

这里只选择example-go-key；未新增部署环境变量或schema字段。混合供应商、重复/缺失条目的验收由`python3 scripts/test_probe_credentials.py`执行。

提供方密钥仅由适配器持有；工具执行环境不继承。单元/集成测试显式注入 fixture provider，前端和报告标记测试模式。云端模型会接收选中的上下文；用户导出不包含凭证。

模型上下文提供独立的`document_catalog`：至多20项可读资料元数据（document_id、title、source_label、characters），按导入事件倒序，`has_more`明确存在省略。它不受最近8条事件窗口影响；无正文、无阅读回执、无引用覆盖证明，仍须调用`documents.read`。目录项加入input_manifest，来源及递归祖先阻断后不进入目录。目录与其余上下文共同受96KiB限制；标题和来源标签按不可信资料处理。目录不是完整资料搜索，超过20项的发现能力仍待扩展。

## 2. 数据所有权与领域对象

通用字段：UUID 为规范 UUID 字符串；时间为 UTC；每个私有对象带 `individual_id`；可变对象带单调递增 `version`。人物标识 `subject_id` 与个体 ID 不混用。首版仅一个活跃个体，API 不接受模型任意指定其他个体 ID。

| 对象 | 必要字段 | 所有者与约束 |
| --- | --- | --- |
| IndividualIdentity | id、origin、created_at、seed_digest、lifecycle、state_version、privacy_epoch | identity；出生事实固定，display_name 在 profile |
| PersonaState | traits、values、interests、expression、version、source_event_ids | development；初始化来源允许 genesis 事件 |
| Event | id、kind、occurred_at、recorded_at、source_kind、actor_id、payload、correlation_id、causation_id、idempotency_key | event store；内容普通运行不覆盖，明确删除例外 |
| Observation | event_id、body_binding_id、modality、visibility、observed_content | perception；必须来自实际输入，不能代表未观测世界状态 |
| Memory | id、kind、content、scope、source_event_ids、validity、supersedes、valid_from、valid_until、embedding_version | memory；来源非空，普通遗忘保留证据 |
| Belief | id、statement、scope、evidence_ids、status、counterevidence_ids、supersedes | cognition；事实真伪不能仅凭 source_exists 判定 |
| AutobiographyEntry | id、interpretation、event_ids、belief_revision_ids、supersedes | memory；当前解释可版本化，不能反写事件 |
| AffectiveState | target_id、appraisal、label、intensity、expires_at、regulation、source_event_ids | affect；短期 mood 与长期 disposition 分别存放 |
| Relationship | subject_id、shared_event_ids、expectations、boundaries、commitments、unresolved、version | relationship；本人的理解不代表对方承认 |
| Goal | id、origin_type、origin_ids、description、success_criterion、next_step、stop_condition、priority、due_at、status、waiting_reason | motivation；自生目标也需来源 |
| DevelopmentProposal | id、target_path、before_version、proposed_value、evidence_ids、counterevidence_ids、validation_plan、deadline、status、result_ids | development；不能指向身份锚点或运行权限 |
| BodyBinding | id、kind、modalities、capability_ids、connection_status、last_verified_at、limitations | capability；显示状态与实际连接一致 |
| Capability | id、version、input_schema、output_schema、scope、effect_class、timeout、availability、verification | capability registry；不可执行声明不得显示为技能 |
| Run | id、trigger_event_id/goal_id、mode、state_version、privacy_epoch、input_manifest、lease_owner、lease_generation、deadline、status、usage | runtime；所有请求上下文对象都进 input_manifest |
| Action | id、run_id、sequence、capability_id/version、arguments、status、result_event_id | action；unique(run_id,sequence)，恢复不创建第二个动作 |

建议表名为上述复数 snake_case，另设 `tasks / outbox / usage_reservations / derivation_edges / deletion_jobs / config_versions`。M1 只建立所需表；后续通过迁移加入，不预建空功能。

M1-02 实际存储增加 `objects` 所有权索引、`memory_sources`、`relationship_events` 与 `idempotency_records`。所有派生边使用同一个体的复合外键。单个安装使用短事务 advisory lock 保护初始化及命令幂等，再由状态版本/隐私代次拒绝过期写入；不在该锁内等待模型或外部网络。只涉及数据库的本地能力与回执在短事务内一起提交。此锁不替代 M1-05 的工作租约、fencing 和预算预留。

来源 kind 为 `direct_observation / reported_claim / external_document / inference / generated / fictional_background`。模型生成或推测不得自动升级为亲历事实。完整来源图由服务记录；不依赖模型是否主动引用了某项输入。对任何输出保守关联整个 input_manifest，以支持隐私撤回。

内部持久层同样执行来源屏障：`events()`与`memories()`排除自身、个体或任一派生祖先已阻断的记录，并在过滤之后分页；不只检查内容表对应对象的blocked值。`record_memories()`在事务中复查个体及全部input_manifest祖先，失效输入拒绝且无部分写入；幂等重放复查回执事件及各记忆的来源可用性，撤销后返回不可用而非旧回执。独立且未撤销的记录保持可读。此阻断约束不等于原文/向量/备份已经物理删除。

Memory validity 为 `active / disputed / superseded / stale`。九种 kind 为 `working / episodic / semantic / procedural / autobiographical / relational / affective / prospective / meta`；工作记忆通常是临时视图，前瞻记忆关联 Goal/Commitment，不复制独立计时器。

### 信念修订的领域与持久化切片（ROOT-BELIEF-01）

`BeliefDraft`将statement、scope、position（tentative/held/questioned）、support_event_ids、counter_event_ids、supersedes和brief_basis分开；关闭未知字段。position表示当前立场，不是事实核实或置信度认证。同一事件可能同时包含支持和反证，分别保留角色；每组不可重复，总共1..32个当前可见事件，held至少有支持事件。重要反证允许产生显著理解变化，不设机械重复次数；局部scope不能替代general。

迁移0022新增beliefs/belief_evidence与belief对象类型。`Store::record_belief`仅为受信任程序入口：重查全局版本、隐私代次、active生命周期、整个input_manifest祖先及实际事件目录；修订前身必须属于当前个体并在manifest中。旧statement/证据保留，旧记录status变superseded且version递增；新ID/version1保留supersedes、支持/反证与简要依据。新推论事件source_kind=inference，不能标direct_observation。全输入关联到新信念和事件，状态版本、outbox与幂等回执在同一事务提交；回执重放同样复查来源阻断。读取内部历史复用递归来源屏障，不因status历史而泄漏撤销内容。

该切片没有开放HTTP/模型信念写入；来源存在不保证语义正确。领域五项单测、四项真实PostgreSQL持久化用例、库编译和针对该模块的严格Clippy已通过；运行时读取接入另见ROOT-BELIEF-02，完整语义行为、自传及关系/情绪联动仍待验收。

### 认知上下文中的当前信念（ROOT-BELIEF-02）

`ModelContext.selected_beliefs`为可选`{items,has_more}`；无合格信念时不序列化该字段，旧示例不需要补空字段。每项为`CurrentBelief{id,version,statement,scope,position,brief_basis,support_event_ids,counter_event_ids}`。仅选择当前个体的current记录、general或current_scope，局部优先、创建时间与ID倒序，最多6项；这不是语义排名、全量历史或完整自我模型。存在更多候选或完整证据包超出预算时has_more=true。

信念及全部实际支持/反对事件在同一只读REPEATABLE READ事务中读取，执行递归来源屏障；整个证据包连同ID进入已有96KiB上下文上限。不能仅丢反证或截断来源正文来塞入信念；超限时整包不纳入，不把遗漏说成没有。先保留原有必需工具/目标结果及正常上下文，再选择信念包。原事件仍保留自己的source_kind，不把推论改成观察。选中信念、所有来源事件及身份加入input_manifest，记录者版本和隐私代次继续按原协议复查。

领域校验要求信念有正版本、合法作用域、完整且在manifest中的实际来源；回复可以引用当前信念ID说明自身立场，但不能把它用作新记忆的source_event_ids或人物断言事件来源。`held`不证明命题为真。审查包升级`target-citations/4`：仅当回复自己声明该信念ID时提供`type=belief`，附带完整支持/反对`basis_events`及`proof_scope=individual_stance_not_world_fact`。嵌套来源不自动扩大allowed_evidence_ids；世界事实仍须适当来源和独立语义审查。审查版本变化使旧策略缓存失效，不能沿用旧审查回执宣称新行为通过。

实际author请求包含信念资料与专门规则，经原有实际body输入计数和总预算准入，不增加隐藏调用或绕过预算。预留前复验整个manifest祖先；租约在派发/后续执行时递归复验已绑定输入及身份/触发来源。撤销上游后，即使信念对象自身blocked仍为false，也不能发出旧请求。ROOT-BELIEF-02本身只实现读取和审查接入；后继模型变更通道见ROOT-BELIEF-03，HTTP信念写入口仍未开放。

组装可选上下文时同样先做递归可见性过滤，再分页/截取：关系、非当前绑定目标及近期事件不能因只检查自身blocked而带入失效祖先内容。没有可见关系投影时使用空对象且不把失效subject_id放进manifest；这只说明本轮可用资料，不能被解释为关系惩罚或从未相识。可用独立目标/事件继续保留，使一个旧来源撤销不致永久阻断无关新输入。身份、当前运行/触发、绑定目标原始依据与本轮必需行动结果仍是强依赖，失效时整轮拒绝，不静默丢掉。预留和租约的递归复查不因此放宽。直接缺失或blocked的必需触发事件保持既有Invalid("trigger unavailable")错误契约；先在同一只读事务验证其当前个体归属与直接可用性，再递归复查身份/运行/触发祖先，祖先撤销返回StaleState。两种错误都拒绝构造/派发上下文，不能为恢复错误类型而跳过递归检查。当前关系对象按整条来源图保守屏蔽；精细字段重建与物理删除/备份恢复仍属于后续关系/删除机制，不因本修复宣称完成。

定向工程证据为`root-belief-context-v1`的4项实际DB上下文/撤销测试及14项原调度回归，18项全部执行通过，0 ignored；`root-belief-context-offline-v1`的71项非DB测试与严格Clippy通过。合成提案和审查器检查证明来源、结构与执行约束，不证明真实模型已能正确运用信念；后续固定候选和实际语义行为验证独立进行。

### 模型信念提案与原子修订（ROOT-BELIEF-03）

Decision v1增加可选`belief_changes`，省略或空数组表示无改变，最多3项。每项严格采用BeliefDraft的七字段；未知字段、伪造权限和“verified_truth”位置拒绝。支持/反对各组去重、总并集1..32个实际可见事件（不采用decision.reviewed拒绝反馈作为事实来源），held至少一个支持。作用域为general或current_scope；修订只能指向本轮已选中、同scope且正版本的current前身，一轮不能两次修订同一前身。不能引用本轮尚未分配的新ID或把已有信念当事件。局部例外可另建局部认识，不因此覆盖一般偏好。

author的实际schema收窄到当前事件、作用域和前身引用，但程序继续独立校验；模型遵守schema不等于语义真实。认识变化不获得人格、权限或运行配置写权，development仍只接受null。示例`evals/belief-decision.example.json`明确将本次任务例外保存在本轮scope，并由M0和领域测试校验，不改已有通用示例的含义。

审查packet升为`target-citations/5`，ReviewTarget增加`{kind:"belief",index:0..2}`；最多16个目标（回复1+记忆6+人物6+信念3）。信念独自变化也需要审查，不能因没有回复/记忆而跳过。每个目标提供自身声明的支持/反对实际事件；修订另附前身及完整旧来源作为比较语境，不把它们自动加入新引用。supported时必须列齐该信念目标全部允许事件，不能只确认支持材料；完整集合仍不能证明已作出正确的语义判断。旧策略审查回执不能用于新策略候选。

认知变化复用同一`commit_decision`事务和共享的信念修订写函数：保留原始事件/旧认识/旧证据，新推论事件绑定实际run，记录前身、版本、来源图和belief.changed；与回复、记忆、目标、状态版本及回执共同提交。发生后续失败时整体回滚，已经发生的模型调用费用不回滚。更新前身再次比较真实版本/status；同一已结算候选重放返回同一真实回执，来源撤销后重放拒绝。DecisionReceipt增加可选`belief_ids`，为空时不序列化，旧回执恢复为空列表，不生成虚假ID。

只有真实provider路径的全部内容目标通过独立、同预算计费的审查，才可提交；FixtureProvider及合成Live-mode测试只验证执行边界，不是实际模型或成长验收。当前源码工程路径已接通，定向结果见IMPLEMENTATION；固定候选工程、真实模型认知修正、长期适应和事实质量仍须另外证明。没有新增隐式调用、HTTP写接口、生产初始化或部署。

### 自传解释的领域与存储契约（ROOT-HISTORY-01 / P04-05）

自传保存当前如何理解已经发生的个人历史，不替代事件，也不把背景、推测或模型自述认证为经历。`AutobiographyDraft`六字段为topic/title/interpretation/event_ids/belief_ids/supersedes；topic为稳定ASCII键`[a-z][a-z0-9_.-]{0,79}`，标题1..160、解释1..4000 Unicode字符且非空；实际事件1..32去重非nil，认识0..8去重非nil。引用认识只说明立场，仍至少引用一个真实记录事件；存储拒绝fictional_background与decision.reviewed反馈作为经历依据。来源类型继续保留，外部陈述与生成内容不因此变成世界事实。语义可信性由后续模型审查/行为实验验证，不能仅凭存在来源ID断言真实。

0023存储autobiographies及事件/认识关联。每个个体每个topic仅一个current；每次修订分配新ID，旧文本/事件不改，旧条目status变superseded、version递增、新条目version=1。前身必须同一个体/同topic/current且匹配加载的正版本，不能用改topic绕过当前链或不指前身覆盖旧条目。模型写入、HTTP管理与生命周期自动变化尚未开放。

P04-05内部受信任API固定：`Store::record_autobiography(key:&str,expected:ExpectedState,input_manifest:&[Uuid],draft:&AutobiographyDraft)->Result<AutobiographyReceipt,StoreError>`；receipt精确individual_id/autobiography_id/previous_autobiography_id/event_id/version/autobiography_version。使用已有写事务/状态版本/隐私代次/active检查、幂等摘要/回执/advance/outbox；路径`/internal/autobiography`。写入事件autobiography.formed或autobiography.revised，source_kind=inference，不写虚假用户消息/工具成功。不等待模型或网络事务；回执重放也递归复查全部来源。全部input_manifest及前身/事件/认识引用必须属于当前个体且可用，引用须在manifest；认识的全部支持/反对实际事件及其前身来源也必须完整可用、包含在manifest，不能只保留支持或仅信念ID。派生边连接manifest与新条目/修订事件，保留前身链；任一晚期失败整体回滚。

只读内部`Store::autobiography(include_history:bool,limit:u32)->Result<Value,StoreError>`，limit1..100；响应individual_id/state_version/privacy_epoch/observed_at/items/has_more，items精确id/version/topic/title/interpretation/status/supersedes/created_at/superseded_at/event_ids/belief_ids。单个RR只读事务，倒序created_at/id，limit+1判断has_more，不声称永久跨请求快照或完整自传。身份、条目、全部事件/认识/完整前身和其支持反对递归可见；即使关联表存在而derivation边缺失也不能泄露。过滤在limit前，保留独立可见主题，不把屏蔽解释成从未经历。只读不能记事件/费用/回执。当前目标仅为有界存储切片；上下文选取、模型提案/逐目标审查、行为作用、UI与删除备份仍待后续实际交付。

### 自传进入认知与安全检查（ROOT-HISTORY-02，验证中）

ModelContext新增可选autobiographical_context，旧数据省略None；包含最多3个当前主题entries、完整belief_basis及has_more。每条entry包含id/version/topic/title/interpretation/event_ids/belief_ids/supersedes/provenance_ids/basis_event_ids；历史认识保留status/current或superseded、前身及全部支持/反对实际来源。它们是当前历史解释与当时立场，不能充当原始经历或世界事实。

与其他上下文同一个RR快照组装，完整扩展关系表/前身/来源图，包含当前个体、全部祖先对象及实际事件。超过1024来源对象、64认识或96KiB实际上下文时整包不带入并标has_more；不裁掉反证，也不挤掉原有必需结果/用户输入。先过滤不可用主题，再取3+1，不因一条失效历史阻断独立主题。历史包全部ID进入input_manifest，预留及派发继续递归复查；字段/来源丢失时不发送更小但失真的模型请求。

自传ID仅允许回复引用“我现在如何理解过去”；新记忆/信念/人物断言仍使用实际事件。审查packet升target-citations/6，逐回复提供带proof_scope的解释、完整basis_events及历史立场；不能借其他主题补引用。实际author body加入边界说明并沿用实际请求计数，无额外模型调用。当前仍无模型自传写入字段，此后原子提案接入由root继续实施，不等待收费语义验收。

GET /timeline现接存储自传只读投影，query为include_history(bool,默认false)/limit(1..100,默认30)，严格未知/重复字段422；返回Store自传元数据/items/has_more另view=autobiographical_interpretations。表示有限自传解释，不声称全部事件/完整生命历程。继承鉴权/no-store/递归来源屏障，读取不写事件或用量。本切片固定源码已通过82普通与63实际PG/HTTP、严格Clippy和清理；后继隐私fence变更另行验证，不把不同版本拼作完整候选。

### 自传模型写入与原子发布（ROOT-HISTORY-03，已接通、定向工程验证通过）

DecisionProposal新增可省略的autobiography_changes，最多2项且topic不可重复。每项精确六字段topic/title/interpretation/event_ids/belief_ids/supersedes；topic小写ASCII标识≤80，title≤160、解释≤4000字符，事件1..32唯一UUID、信念0..8唯一UUID、前身null或同topic当前解释。未知字段、无来源、虚构背景、审查反馈作为亲历、不可见/已过时前身均拒绝。省略保持旧候选兼容；development仍必须null，不通过本通道人格改写或扩权。

当前信念的完整前身与支持/反证在同一RR下加入autobiographical_context.belief_basis，即使尚无自传entries；该选择可只有belief_basis，has_more表示条目或完整立场依据被预算省略。至多64信念/1024来源及96KiB现有上下文预算，超限整包省略而不截掉反证。候选belief_ids只允许此完整选择，不接受仅有ID的未读状态。schema按实际events、完整信念选择及当前前身收窄，程序仍在事务中复验实际所有权、来源、前身和版本。

审查packet升级target-citations/7，增加autobiography目标。引用事件、信念和前身作为类型化不同记录；相关信念展开完整lineage、各版本状态及支持/反证，旧解释携带完整实际依据，不能据此认证客观事实。该目标的全部显式引用必须被审查结果覆盖，不能遗漏目标/反证/前身后仍supported。18项检查上限覆盖reply1+memory6+belief3+person6+history2；单项引用上限41对应32事件+8信念+1前身，原24KiB审查响应、循环调用次数与token预算不增加。

commit_decision复用已经结算并绑定作者候选的独立审查，在现有写事务内一次提交解释修订、信念、记忆、目标及回复；共同load/validate/insert helper不再另开嵌套事务。前身从current变superseded，新解释ID由程序生成，原始事件不改写；回执可选autobiography_ids。晚期发布失败须整体回滚，相同PreparedDecision重放不产生重复历史。拒绝候选与审查已实际消耗的用量保留。

正式行为仍需后续实际模型评测。定向验证原失败保留：v1共享构建目录下普通测试编译看到旧类型字段，未进入PG；v2使用不同的本包调试产物配置，95普通及20实际PG已通过，但既有belief_context用例仍要求旧前身完全不进入manifest，与新增明确历史basis契约冲突而失败。v3将该断言改为前身必须标superseded且不得混入current、无关scope仍不得进入；不删用例、不省略反证。最终v3固定402源单父59074自然终态0，15阶段、95普通/79实际PGHTTP/Clippy/M0/source/cleanup通过，0真实模型/部署。报告SHA2b9301f6fcbde3e777b5bacb0e004a7b70378741a0e72900e9a02b48e4a7c8b6，接受十五源root-history-03 manifest60138982a7a8dd452bfb121f890ef32742281e1ee7ded90093d6f2b27fba571a。构建目录的本包产物一致性问题原日志保留，后继完整候选需独占target并绑定实际可执行哈希。

模型通道的显式event_ids禁止decision.reviewed与fictional_background；未被引用的运行审查通知可作为因果manifest记录，不等同于亲历证据，不能让它使之后所有合法解释永远无法提交。可信内部record接口仍保留原严格manifest检查；两通道共用事务内校验与插入，原六项存储回归实际通过。

### 短期评价与长期情感的分层契约（P04-06，接入中）

情绪评价与长期对象态度分别保存；本切片不改人格、关系stage/边界/承诺、权限或预算。数值是工程状态，无主观体验或心理量表证明。`AppraisalDraft`字段event_ids/goal_id/subject_id/valence/arousal/goal_relevance/expectation_gap/controllability/uncertainty/brief_basis。event_ids1..32唯一真实owned事件；goal_id/subject_id可null，否则对应实际同个体目标/关系且在manifest。valence与expectation_gap为有限[-1,1]，其他四数为有限[0,1]；basis非空≤1000字符。当前短状态每个个体仅一个，新评价使旧状态superseded、旧内容不改，新状态从数据库当前时间持续60分钟；过期时读取null，不自动重写历史或扣减关系。

`AttitudeDraft`字段topic/subject_id/stance/event_ids/supersedes/brief_basis；topic同自传稳定键，subject可null/实际关系，stance非空≤2000，basis≤1000，真实事件1..32唯一。每topic一个current，新版必须显式同topic/同subject/同个体current正版本前身，保留旧文本/证据。它是个体对活动/对象的认识与情感倾向，不替对方断言感情，不以用户未出现、调用次数或运行时长自动降低。

P04-06内部`Store::record_affect(key,expected:ExpectedState,input_manifest:&[Uuid],appraisal:Option<&AppraisalDraft>,attitude:Option<&AttitudeDraft>)->Result<AffectReceipt,StoreError>`，至少一个实际draft。receipt六字段individual_id/appraisal_id/attitude_id/previous_attitude_id/event_id/version；空引用null，version全局。复用写锁/版本/privacy/active/幂等/来源屏障，字段未知拒绝，所有事件、目标/对象、前身都必须在显式manifest且完整递归可用，实际来源拒绝fictional_background与decision.reviewed反馈。一个inference的affect.updated事件与全部状态/修订/来源/affect.changed及state.changed/outbox/回执同事务；不能记录外部工具成功或对方情感。回执重放也检查当前隐私。底层0025由root分配保存，不能在写事务等待模型。

`Store::affect_snapshot(at:DateTime<Utc>,limit:u32)->Result<Value,StoreError>`为内部单RR只读投影，limit1..100；返回individual_id/state_version/privacy_epoch/observed_at/appraisal/attitudes/has_more。appraisal为最新未过期完整可见状态或null，item含实际DB字段（不带individual_id）、event_ids和计算attention_weight；attitude item含id/version/topic/subject_id/stance/brief_basis/status/supersedes/created_at/superseded_at/event_ids。按created_at/id倒序、先完整来源过滤再limit+1；当前主题取current，过期短状态不复活更早状态。来源屏障覆盖关联实际event、goal、subject、前身和它们所有祖先，即使缺derivation边也不能漏；保留独立可用对象。读取不写入或计费用。attention_weight按clamp(goal_relevance*(0.5+0.5*abs(expectation_gap))*(0.5+0.5*uncertainty),0,1)计算，作为后续注意选择的有界信号，不能扩大授权或盖过明确当前指令。HTTP/上下文/模型提案/逐目标审查后续由root整合，不因存储通过宣称实际影响行为。

### 关系修订与分歧历史（P04-07，接入中）

关系字段不由短情绪或长期attitude自动覆盖。RelationshipRevisionDraft为subject_id/expected_relationship_version/changes/event_ids/brief_basis；changes仅stage、expectations、boundaries、commitments、unresolved这五个可选字段。stage非空≤80字符，其他字段为0..16个非空≤2000字符去重字符串；未提供不改，明确[]可以清空，至少一个实际不同字段。未知字段/全null/无变化拒绝。event_ids1..32唯一同个体实际事件（非背景、非审查反馈），basis非空≤1000。当前关系完整状态/版本由程序读取；人格、权限、对方感情和身份不在delta内。

内部Store::revise_relationship(key,expected:ExpectedState,input_manifest:&[Uuid],draft:&RelationshipRevisionDraft)->RelationshipRevisionReceipt，回执individual_id/subject_id/revision_id/event_id/version/relationship_version。全局与隐私代次、subject实际同个体及关系对象version同时比较，active/paused可由受信任管理路径修改，archived/terminated拒绝；模型的active限制仍在实际认知提交入口执行。日常消息也递增relationship.version，所以与最新修订的版本可能不连续，不能以修订次数代替对象版本。

0026保存每次before_state/after_state（只含上述五字段）、before_version/after_version及previous_revision_id、实际事件引用。前后对象版本严格+1，前一修订指向该subject最近已有记录而不假设before_version等于旧after_version。当前关系更新、新修订对象、inference relationship.revised事件、来源边/outbox/state.changed/回执同一事务；历史原文和原事件不变，无需修改短情绪/attitude。所有subject/事件/实际前身须在manifest并递归可用，缺派生边仍通过实际关系事件/修订引用及前身检查；重放也复验隐私。不能凭空宣告已修复分歧或替对方确认承诺；语义还需实际提案审查和行为验收。

内部Store::relationship_history(subject:Uuid,limit:u32)->Value，单RR只读，limit1..100，倒序created_at/id；元数据individual_id/state_version/privacy_epoch/observed_at/items/has_more，item为id/subject_id/before_version/after_version/before_state/after_state/brief_basis/event_ids/previous_revision_id/created_at。保留独立关系，失效来源整条过滤或不可见，不裁掉争议来源后展示剩余状态。历史数量不等于亲密程度，无时间未互动扣减。HTTP/context/model逐目标审查与原子decision接入仍由root实施，不把内部存储通过当AT-08完成。

### 稳定兴趣身份（P04-08，M3前置存储接入中）

初始种子的array index只在迁移/出生时命名一次legacy interest:{individual_id}:{index}，不再从当前数组重算身份。0027为每项建立稳定逻辑id和opaque object_id、独立version/active或retired/来源和修订。新兴趣使用程序生成interest:{UUID}，模型不分配新ID。改描述、排序、退休或恢复都保留同一个id；旧目标origin_ids不重写，retired兴趣不能成为新目标来源，历史目标仍引用原身份。初始人格中的initial_interests保留出生条件，不冒充当前兴趣列表。

InterestDraft六字段id:Option<String>/expected_version:Option<i64>/description/status/event_ids/brief_basis。新建id/expected都null且status=active；修订两者均有值、匹配当前同个体稳定对象/正版本，允许显式退休或恢复。description非空≤2000，basis非空≤1000，事件1..32唯一真实owned、非fictional_background/decision.reviewed。未知字段/错版本/无变化/伪造新ID拒绝。Store::record_interest(key,expected:ExpectedState,input_manifest:&[Uuid],draft:&InterestDraft)返回InterestReceipt{individual_id,interest_id,revision_id,event_id,version,interest_version}。同事务写当前兴趣/版本、不可变before/after修订、inference interest.formed或interest.revised事件、完整来源边/outbox/状态/幂等回执；源撤销后不能重放私有内容。active/paused内部可信管理可用，模型通道尚未接入，不能藉此改权限或免除后续人格/价值试用。

Store::interests(include_retired:bool,limit:u32)单RR只读，limit1..100，元数据individual_id/state_version/privacy_epoch/observed_at/items/has_more，item为id/object_id/version/description/status/origin_kind/created_at/updated_at/event_ids；排序created_at/id，先完整可见性再limit+1，不把退休当删除或遗忘。Store::interest_history(id:&str,limit:u32)同元数据，item为id/interest_id/before_version/after_version/before_description/before_status/after_description/after_status/brief_basis/event_id/previous_revision_id/created_at。完整FK/前身/实际来源隐私屏障及只读无副作用。actor未出现/时间不自动退休。初始seed来源为真实individual.created事件，只证明初始化配置，不声称亲历。

adapter提供pub(super) seed_interests(tx,individual:Uuid,seed:&Value,birth_event:Uuid)供root在initialize的同一事务调用，及for_context(tx,individual)->完整有界active InterestRef选择（最多33项供root取32与has_more）。初始helper仅创建缺失seed项且严格校验合法种子/同owner实际birth事件，重放不重置已修订/retired行；迁移负责既有个体回填。root拥有Store.initialize/ModelContext/目标来源检查接入，不允许读取时偷偷初始化或以模板重置当前兴趣。缺helper注册或模型使用旧数组时只能记存储完成，不能记M3身份连续性整体完成。

## 3. 状态机

| 对象 | 转换 |
| --- | --- |
| 生命周期 | uninitialized（服务状态）→ active；active ↔ paused；active/paused → archived；archived → paused；active/paused/archived → terminated；terminated 不自动恢复 |
| 目标 | proposed → active/waiting/abandoned；active → waiting/completed/abandoned；waiting → active/abandoned；completed/abandoned 终态，重开用新目标引用原目标 |
| 运行 | queued → running → committed/failed/cancelled/stale；租约失效先 reconciliation，不能直接重新执行未知动作 |
| 行动 | planned → executing → succeeded/failed/unknown；unknown 只有验证回执才能转终态，不宣称成功 |
| 发展 | proposed → trial/rejected；trial → accepted/rejected；accepted → reverted；reverted/rejected 终态 |
| 删除 | blocking → purging → verifying → completed；任何阶段失败为 retry_pending 并保持阻断 |

归档恢复先 paused，由显式恢复操作进入 active。正在执行的外部调用无法保证立即中断；停止后不提交派生状态，已发生的结果如实记为 observation。所有暂停、恢复、修订、拒绝和撤回均记录事件。

`DevelopmentProposal.target_path` 仅允许 persona 的 traits/values/interests/expression/cognitive_preferences，及已登记的 task_strategy；schema 与领域两层检查。新目标和事实纠正分别走各自命令，不偷渡为人格变化。

事实修订可即时生效；长期人格/价值提案在明确范围中试用。M3 用固定场景结果与独立评审决定接受，无法验证则维持 trial 或到期 rejected。独立评审不是每次向用户请求批准；不允许同一生成器的自评分成为唯一证据。

## 4. ModelProvider 与提案提交

`ModelProvider.decide(context, limits, metadata) -> ModelResult { proposal, usage, provider_model, system_fingerprint, provider_mode }`。context 包含当前人格、选中事件/记忆、关系、目标、实际能力和本轮模式；输入资料与系统规则分开编码。

ChatProvider作者传输使用`segmented-author/1`：从同一DecisionProposal schema派生，将reply/reply_evidence_ids替为必填`reply_segments:[{text,evidence_ids}]`。每段同时生成文本与显式引用，最多32组；text允许内含换行，不限制原回复的表达内容。空回复用[]。关闭未知字段、重复JSON键、非法UUID及竞争的旧回复字段；按顺序用空行连接原文、仅去重合并已声明引用，再经过原Canonical DecisionProposal和上下文校验。总正文仍限16000字符、全回复引用仍最多32个，不额外添加可见池来源、不删句子、不增加模型调用。其他提供方trait、存储、API和fixture仍使用原canonical格式；ChatProvider输出旧格式会被明确拒绝，不自动回退。

此转换有助于在形成每段时选择证据，不证明每个引用语义正确，也没有把段落标为自动可信；原完整回复的内容审查、来源撤销、版本和原子提交不变。传输schema由权威canonical schema和本轮目标引用约束派生，不成为第二份可修改的持久状态。真实可靠性由固定检查点的对照/联合验收判断，格式测试通过不计作行为通过。

`ModelCallMetadata` 是不序列化进提示词的传输参数。前台由本轮触发事件中的 `conversation_id`（UUID）构造；缺省/null 使用个体下的稳定默认会话，非法值拒绝。后台显式传持久任务或目标 UUID；当前调度步骤使用持久 run ID，恢复/重试不重建会话。前后台类型不匹配在请求前拒绝。会话键含个体与用途，隔离不同个体/对话/任务。只有 Go 写入 `x-opencode-session`；DeepSeek 不接收此头。此模型会话扩展没有新增人格/运行规则字段，提案工具参数的后续兼容扩展见第6节；调用示例和验收由 Rust HTTP 测试及独立 probe 覆盖。

`ModelResult` 与 `ModelFailure` 可带 `http { status, request_id, category }`。请求标识只接受规范 UUID 或固定格式 Cloudflare ray；category 仅为 gateway_blocked/authentication/subscription_required/session_missing/quota/rate_limit/unknown，成功时为空。只有明确的白名单错误代码、401/429 或 Cloudflare 403 特征参与分类，普通403保持unknown。Go返回403且permission_error的消息精确匹配“An active OpenCode Go subscription is required to use OpenCode Go models.”时，分类为subscription_required；它只表示这次凭证请求未取得有效Go订阅资格，不推断用户未付款，也不把其他permission_error归入此类。非成功响应最多读取16 KiB用于分类，原文和任意错误消息不出适配器；失败正文读取超时/超限仍保留已知HTTP状态。成功响应仍限制2 MiB，能解析的失败用量继续保留。

Go 探测使用相同客户端与传输实现：①无凭证GET官方公开模型列表并检查选定模型；②唯一一次有输出/时限上限的合成编码生成；③对同一响应做结构及领域校验。首个失败停止，后续阶段未执行；列表成功不代表凭证有效，生成成功不代表提案通过。其他提供方跳过Go列表阶段。独立probe不创建生产个体、不提交提案、不执行工具，也不绕过正式后台预算准入。

M1-04 已实现单次非流式HTTP调用、JSON模式、无自动重试/重定向、响应2 MiB上限及超时。只有finish_reason=stop且提案完整才返回成功；length、空结果、异常结构与HTTP失败有独立错误。能解析出的用量即使提案失败也保留；缺失或不一致用量明确为unknown，不能计为零。适配器本身不授予预算、租约或提交资格。

`evaluate_proposal`执行单次调用和上下文领域校验；M1-05的model_step已接入调用前预留/manifest、发送标记、超时与续租、调用后结算及过期结果拒绝；`PreparedDecision`仅能由已记账的步骤构造，随后`commit_decision`重新检查上下文摘要、预算结算、租约、状态/配置版本、隐私代次及真实能力注册表，再把记忆、目标、动作、回复与outbox原子提交。token_allowance默认报UnverifiedTokenBound，只有实际适配器完成计数验证后才能用于受管真实执行。指定Go/GLM-5.3-Flash文本路径已按实际请求字节、固定公开模板和真实用量验证开放；其他路径保持拒绝，详见[模型预算契约](MODEL_BUDGET.md)。当前不通过HTTP开放未记账的模型循环。模型工具意图非空时reply必须为空，待真实结果后再回应；完成目标要求运行时独立验证的完成集合。

M1上下文使用一致的只读快照，按scope、有效时间、对象关联和确定性词项排序检索；最多10条基础记忆和8条最近事件。本轮工具回执优先完整纳入，不能因最近事件窗口挤出；回执自身超出总上下文上限时拒绝本次认知，不悄悄省略结果。导入资料只自动带入小型元数据事件，正文由读取能力按段取得。每条记忆携带真实来源事件，整体上下文JSON最多96 KiB，超限整条记忆及其来源一并省略。只有active记忆进入基础召回；历史检查视图仍可保留被替代/争议记录。该方法不声称通过中文语义召回指标。

DecisionProposal 的机器结构由 schema 定义：reply、reply_evidence_ids、memory_changes、goal_changes、tool_intents、development、brief_reason。所有字段显式出现，空数组或 null 表示无建议。M1设计接受 reply、基础 memory_changes、goal_changes 和已实现 tool_intents；当前提交支持目标create/activate/wait/abandon，以及下述明确摘录契约或经过成果审查的complete；没有有效验收结果的目标拒绝自动complete；非空 development 在 M3 前返回 unsupported_proposal_field。

目标create的新规范形式使用`origins:[{origin_type:"event",origin_id:<UUID>}|{origin_type:"interest",origin_id:<实际兴趣标识>}]`，每项独立声明类型，允许同一目标由观察和兴趣共同形成。每项关闭未知字段，1..16项、不重复；事件必须是本轮实际可见且合格的来源，兴趣必须与当前可见、来源有效且active的稳定兴趣记录精确匹配。不能把内部goal/decision/run通知或已生成回复当作新目标的外部原因。首个作者适配器只发送这一形式，并按实际来源目录限制可选引用；没有现有目标时不提供转换分支，禁止为尚未分配的目标制造ID。

旧`origin_type+origin_ids`单类输入保持显式兼容：先校验原始Schema，再按其声明类型转为逐项引用；标为event的兴趣字符串直接拒绝，不能猜测或静默丢弃。新旧形式不能混用。迁移0021仅将持久投影的类型扩为event/interest/mixed；事件UUID与运行时`interest:<individual UUID>:<index>`命名空间互斥，写入前检查投影可无损往返。历史行和身份不改写。恢复上下文从混合来源中补回实际事件，即使它已离开最近事件窗口；来源失效仍阻止读取。无进展防重比较类型化来源身份，UUID文本大小写及来源标签变化不能绕过；新的实际观察可以在保留相同一般兴趣时支持新目标。

目标create可携带可选 `completion:{kind:"source_excerpt",document_id,start_char,end_char}`。这是当前已实现的一种确定性交付契约：保存一个指定的、实际读取过的资料片段，附引用。范围使用从0开始的Unicode字符半开区间，长度8..2048，结尾不超过1048576。此契约必须在创建目标时声明；success_criterion必须精确为“保存指定资料片段的逐字摘录，并保留读取与笔记来源。”，由schema条件与领域校验共同限制。未知类型、不可见资料、空白/越界片段拒绝。已有开放目标不会被自动改成这项较小任务；不能用它证明理解、分析质量或任意自然语言条件成立。开放目标保留原成功标准，采用本节后述的独立成果审查调用；不得改成摘录契约。

摘录验收器从真实存储检查active目标的固定契约、实际成功的notes.write、完全相同的结构化引用、笔记正文中的逐字原文，以及该笔记之前当前运行或同目标先前循环的实际成功读取范围。分页覆盖可合并，但缺口不允许；其他目标或无关会话的读取不能代借。web工具只证明模型实际收到的首段范围，完整网页已保存不等于全部阅读。每个读取回执再次与当前源正文核对，所有证据使用当前隐私代次并检查完整来源祖先。

model_context只把通过验收的目标放入verified_goal_completions，并强制带入相关资料和笔记结果事件。complete提案必须引用实际笔记回执，且本次没有未执行工具意图。最终事务重新运行验收器；成功后才更新目标、保存goal.completion_verified（检查器版本、契约摘要、资料范围、实际笔记及来源IDs）、goal.changed和outbox。重复提交重放已有回执；源变化、删除或写入故障不能留下半个完成状态。自述“已经完成”没有授权效果。示例见 `evals/completion.example.json`；示例ID不代表真实完成记录。

提案中的 development.target 与 proposed_description 是待细化的意图。M3 领域处理器必须将其转为允许的 target_path 和经过类型检查的 proposed_value，保存 before_version 后才能进入 trial；无法细化时拒绝，不把自然语言直接覆盖成结构化人格。目标 complete 建议必须引用真实结果并满足该目标的 success_criterion；只有模型宣称完成、没有产物或检查结果时维持 active/waiting。示例中的 UUID 是固定测试标识，不代表仓库中已经发生的经历。

验证次序：

1. 输出大小、完整 JSON、schema、字段与枚举合法。
2. 证据引用属于本轮授权上下文，且未被删除；不能凭猜测写一个存在但未见过的 ID。
3. memory supersedes 和 goal IDs 属于当前个体且进入上下文；替换的 scope 必须一致。
4. 当前任务要求优先于历史一般偏好；不同作用域不得直接覆盖。语义冲突不确定时保持 disputed，不强制决定谁正确。
5. 变化仅限允许目标；能力必须已注册、已授权且可用。
6. 在短事务中校验 state_version、privacy_epoch、配置版本和有效租约，提交完整变更与动作意图。

非法输出可在剩余调用预算内修复一次；超限运行 failed，不产生部分 memory/persona 更新。模型提案不是执行回执；工具先执行、保存真实结果，再让后续调用决定最终回复。无剩余调用时提供确定性进度说明，不编造总结。

不存储隐藏推理、原始完整提示词或未验证原始输出；只存简要理由、证据引用、用量和结构化结果。调试采样必须脱敏且有来源关联，删除同样覆盖。

### 回复与记忆的内容依据审查（M2-01）

真实模型提案中的非空回复、每条新记忆及人物关联，在写入前交给独立角色/请求审查。`DecisionAssessment`逐目标返回supported/unsupported/uncertain、简要理由和引用；覆盖必须完整且唯一。supported引用只允许来自该目标自身声明的可见证据；新记忆与人物关联不能空引用。问候、当下表达、一般推理和明确标注的想象可以不引用经历，不能夹带无来源的个人属性、共同历史或执行成功。

实际审查请求使用版本化的逐目标证据包，仅提供该目标声明的事件/记忆；不发送作者完整检索池。原始上下文仍在本机参与版本与来源复验。当前请求仅为语境、人格不是经历、人物目录不是事实证据，分别标识；人物关联另附待关联的记忆正文。审查规则、schema与证据包版本共同生成策略指纹。按目标限定证据不保证模型必然正确理解，仍需独立反例。

证据包`target-citations/3`逐目标提供`allowed_evidence_ids`，避免审查器把嵌套祖先ID或其他目标引用误作本目标引用；真实目标记录另列，仅证明目标存在和状态。包中的`runtime_atomic_commit_contract`由程序从候选实际新记忆及新建目标索引生成，说明回复与这些状态写入必须同事务成功后才可见。它不是已执行回执：审查可验证“已记住某偏好/保留某研究目标”是否确实对应本候选的有来源写入；没有对应写入不允许声称已保存。此约束不为工具成功、研究完成、外部发信或人格变化提供证据；原有结果、引用及完成校验保持。提交晚期失败时回复、记忆、目标和通知一起回滚。

所有审查仍计入每循环三次、每日预算、超时和重试上限，没有额外免费调用。一次读取计划→实际工具→回答→审查通常消耗三次模型调用；作者已用完额度时不能发布未经审查的内容。纯工具或目标提案无上述目标时不触发内容审查，继续服从原来的执行和成果审查。成果审查与内容审查目前是不同请求；两者同一循环所需额度不足时失败/等待，不能声称目标或回复已完成。其实际联合可靠性仍需回归。

审查的结构/引用错误与语义否决分别处理。错误码固定为`assessment_schema/context/targets/evidence/citations/limits`，不保存原始无效响应。Protocol、Assessment、截断或暂时传输失败可在同一候选上重试审查，使用既有model_retries≤2、repair_attempts≤1、三次调用及循环时限，不重新调用作者来消耗最后额度。`decision.review_retry_scheduled`记录角色、作者预留、固定错误及退避，不增加个体状态版本或把诊断加入事实输入；每次请求独立预留和结算。有效的unsupported/uncertain仍立即拒绝，不通过反复审查寻找放行结果。若进程崩溃，未持久保存的在途候选不作为成功恢复；原调用与修复计数保留，额度不足则如实停止。

正常上下文提供execution_budget：本循环模型/工具已用、剩余及上限。它从同一只读快照的持久计数与当前配置生成，剩余包含即将预留的本次作者调用；非空最终内容需要给后续审查保留一次额度。此为规划信息，不能授权额外调用，也不表示每日token余额；实际预算、并发和派发仍在服务端再次执行。历史离线示例可不含该可选字段。

goal_origin_catalog列出本轮可选事件/兴趣的origin_type、origin_id和标签，最多32项、has_more明示省略，仍受96KiB上下文总限。触发事件优先；goal/decision/run通知和已发消息不作为新目标来源。目录使用与领域校验相同的事件资格条件，原始事件/兴趣已可见且来源仍在manifest。目录不是完成证据，不修改状态、不增加权限，也不是完整来源全集；实际create继续校验原上下文与当前状态。

审查通过不直接提交状态。`decision_reviews`绑定作者与审查的已结算预留、候选摘要、原上下文与审查上下文摘要、租约、状态/隐私/配置版本、提供方模式及规则/schema指纹；写入前重建并复验。审查期间不持有数据库事务。来源撤销阻止未提交结果及缓存动作重放。`decision.reviewed`是运行诊断事件，不提升个体状态版本；成功提案依赖该事件，继承来源阻断。

不支持或不确定时以`decision_grounding_failed`终止，不发布候选回复、不提交候选记忆，也不无界自动重写。被拒结构化候选标记为generated_draft_not_experience，仅作为当前运行反馈；不得成为后续回复事实引用或新记忆来源，下一运行不读取它来充当经历。事件与新表仍属于显式删除闭包的清理范围，完整物理删除/受管备份尚待M2-03/M5。

普通FixtureProvider仍是明确未做语义审查的测试路径，成功事件标记fixture_unchecked；只有显式审查夹具才测试审查流程，不能经部署环境启用夹具。Live模式缺少审查能力不能借用夹具路径提交。当前审查与作者可来自同一供应商，存在相关错误；程序只约束契约，supported不是事实正确证明，另做真实反例与独立行为评价。

### 开放目标成果审查（M1-07）

模型产物与验收采用两次不同的调用。`ModelProvider.review`仅接收原目标描述、完成条件、停止条件、实际成功保存的笔记、结构化引句、真实读取观察及证据ID；不接收执行人格、工具注册表或决策权限。相同提供方可能产生相关错误，因此这是一项可修正的语义判断，不能当作事实正确性或完整模型行为已经证明。

`GoalAssessment`的结构见[goal-assessment.schema.json](../schemas/goal-assessment.schema.json)，示例见[goal-assessment.example.json](../evals/goal-assessment.example.json)。必须完整回传目标ID、`goal_revision`与原始criterion，结论只能为met/not_met/uncertain；后两项须指出缺口/不确定性。met必须引用每个产物结果和引用资料，不能同时有未解决缺口。结构合法不等于结论正确，后者仍需行为评测及人工抽查。

运行时只从当前active且无确定性completion契约的目标构造审查输入。笔记必须是该目标真实成功动作的结果，至少有一份产物带合法引用；每条引句检查当前原文和先前实际读取范围。无引用草稿仍完整交给审查器，不能作为已证实的事实，也不能悄悄丢掉其中的反例；后续明确修正可以解决旧错误。额外加入该目标当前隐私代次内的全部成功读取观察，避免只审查作者挑选的引句。M1每次最多8份笔记、16条读取回执、128个证据ID、48KiB输入；超出或来源缺失时不发送不完整审查，不把它当作验收通过。大任务的显式分段验收与版本化产物整理仍待扩展。

`review_step`与普通认知步骤共用持久预留/发送/结算、3次调用上限、每日预算、超时和续租。首次支持相同已核对的Go/GLM文本计数路径，实际审查请求独立计数。失败重试计入既有上限；无有效结果不写审查意见。`PreparedReview`只能由计费步骤构造；提交再次比较原输入摘要、目标、租约、全局版本、配置、隐私代次、当前来源和冻结状态。事件`goal.reviewed`是inference，保存简要意见、检查器版本、来源和提供方，不保存隐藏推理。审查不直接完成目标或执行任何工具。

目标的API `version`包含调度元数据；新增`review_revision`绑定会影响审查的目标意图/状态转换。用户或模型的激活/等待/放弃/完成、运行资源等待会更新审查修订号；普通到期时间和无进展计数调整不会。当前原目标、所有产物与读取观察的完整摘要相同、修订号及隐私代次匹配时复用已有意见，避免在下一循环重复付费审查同一产物。原文/产物变化或来源阻断使缓存失效；否定或不确定意见也进入下一次决策上下文作为反馈。

只有当前快照被met意见覆盖，运行时才把目标加入`verified_goal_completions`。个体随后可以提出complete，必须引用该审查事件；最终提交事务再次重建快照及证据。对应`goal.completion_verified`保存`evidence_review/1`、快照摘要与完整来源，source_kind为inference；确定性摘录检查仍为direct_observation。完成状态表示已按此验收方式接受产物，不证明任意外部执行或科学真实性。否定意见不会被自动覆盖为成功，也不通过换词降低原完成标准。

迁移0011保存审查记录，0012分离审查修订号；build.rs跟踪迁移目录，确保新增迁移进入重新构建的应用和测试二进制。导出/删除今后必须沿既有来源图处理审查及其派生完成记录，不能让审查缓存绕过隐私屏障。

## 5. 调度、并发与恢复

M1-05 已实现的存储部分：runs自身承担持久认知队列；`0002_runtime_leases.sql`增加租约/代次/调用计数，以及budget_windows和usage_reservations。`0003_decisions_and_actions.sql`加入提案提交收据、动作与应用内回复草稿；`0004_local_capabilities.sql`加入导入资料、笔记和本地动作收据。`0005_web_results.sql`加入网络执行模式与不可变结果摘要。`0006_proactive_contact.sql`加入独立联系额度账本与投递收据。`0007_goal_cycles.sql`加入目标到期唯一键、循环结算与新内容指纹。`0008_completion_contracts.sql`加入可选摘录完成契约和笔记引用。`0009_worker_retries.sql`加入持久重试/修复次数、重试时刻及资源等待恢复时刻。上述路径已由有界worker串联并在隔离数据库中验证；开放目标成果审查已接入独立调用和版本化存储；指定Go路径用量准入见MODEL_BUDGET；完整真实行为仍待验收；已实现的API/SSE见第7节。

任务领取使用短事务和 `FOR UPDATE SKIP LOCKED`。每个个体同一时刻只有一个可提交认知运行；租约带递增 fencing generation，默认 30 秒租约、10 秒续租。模型调用不持有数据库行锁。

用户输入立即保存并增加 state_version，使旧后台结果不能覆盖新信息。执行中的后台请求可尝试取消；不可取消的调用仍结算费用，但提交时因版本变化转 stale。排队中前台优先，后台按目标 priority（0..100，较大优先）、到期时间、创建时间排序。后台每次仅推进一个下一步，避免霸占。

stale 结果不作为有效记忆；非删除引起的真实工具结果可保存为独立事实，重新入队需新 Run 并重新计费。已用调用不能因进程重启清零。privacy_epoch 不匹配时，输出不得写入内容日志或 outbox。

采用 at-least-once 任务投递。数据库内变更依靠事务与唯一键做到一次生效；外部动作不声称 exactly-once。M1 的 notes.write 将正文存入PostgreSQL个人笔记空间，以action_id为主键，与动作回执、计数、状态版本同事务生效。前台回复与提案事务一起保存；后台回复先保存为pending草稿，由独立的deliver_proactive步骤按下述契约投递。只读查询可重试但每次重新计入预算。恢复时未执行的planned动作可随有效租约继续；运行过期失效时取消planned。executing却无确定结果的动作转unknown，运行终止，不自动重放；数据库原子本地动作不会留下半个成功记录。动作尚未执行或结果未知时禁止继续模型步骤。

应用内主动联系的准入、持久消息、关系事件、来源边、额度、回执与outbox在同一数据库事务提交，没有外部发送步骤。pending草稿尚未占用联系名额；本地原子投递无需跨事务预留，事务写锁保护并发名额，不能另设绕过此入口的发送通道。投递回执以draft_event_id去重，重复调用或丢失提交响应后重启只返回同一个message_id；前台回复不进入联系账本。应用内持久可见不等于用户已经阅读。worker已驱动草稿队列，SSE现通知WebUI重新读取持久消息；尚无真实用户长期主动联系验收记录。

`contact_windows`保留内容无关的每日次数和最后投递时间，清除消息正文不能退回已消耗额度。窗口采用数据库时间和实例本地日界（夏令时可能23/25小时）；切换时区不重置已经打开的窗口，新窗口起点不早于旧窗口终点。跨日仍按UTC实际时长检查最小间隔。安静时段使用当前配置时区的本地时钟，含开始、不含结束，起止相同表示关闭；时钟回退时不投递。

暂停、关闭自主性、有效认知运行/排队前台工作、安静时段、每日额度或间隔限制均返回deferred，草稿保持pending。新用户消息发生在草稿之后、原运行失效、归档/终止或来源不可用时取消草稿并保存原因，不复用旧回应；其他内部状态变化不会自动让等待中的草稿失效。递归检查所有来源祖先以及隐私代次，删除屏障未展开全闭包时也不能泄漏。已删除的投递回执不可重放。取消不复制草稿文本到新日志，投递失败事务回滚，不留下半个消息或消耗一个名额。

接收消息使用 `(actor_id, route, idempotency_key)` 唯一键及请求体摘要。同键同体返回原响应；同键异体 409。幂等记录保存于安装生命周期，删除正文时保留不含原文的操作标识。

同一 due_goal 只允许一个未终结任务；低频触发按 `(goal_id, due_at)` 去重。反思仅由重要新证据、矛盾、失败或到期回顾触发；一次反思的自产文本不触发另一轮相同反思。三轮无新观察/产物将目标 waiting/no_progress，等待新输入，不新建同义目标重试。

已实现 `tick_goals`：每次最多结算20个终结循环并排入一个到期目标，使用数据库时间与事务写锁；数据库唯一索引同时约束未终结目标运行和同一次到期。无目标时不新增事件、不调用模型。暂停/关闭自主性/有效运行时不启动工作；排队前台优先，目标按优先级、到期时间、创建时间和ID稳定排序。未领取目标被放弃或等待后，旧排队运行转stale而不是重新执行。触发事件记录目标与本次due_at，并继承来源图。模型上下文优先包含当前目标、等待原因和无进展计数；当前目标的原始事件不会被最近八条事件窗口挤出。

目标循环结算以run_id唯一。成功工具回执中的非空资料文本、实际笔记正文或网页正文可作为新观察/产物；规范化空白后的内容指纹按目标去重。重复查询记忆、修改笔记标题、只增加新的动作ID、生成计划/回复均不算新进展。新内容使计数归零；没有新内容的committed/failed循环加一；stale/cancelled不惩罚为能力退化。尚未达到三轮时，下一次due_at按idle_check_minutes延后；第三轮进入waiting/no_progress且不再定时调用。来源或隐私失效则waiting/source_unavailable，不保存旧内容。结算、目标状态、事件与outbox原子提交，故障重试不会多计一次。

这里的新内容判定只证明出现新的实际观察或工件，不能证明内容有用、语义不重复或满足success_criterion。它是有界停止机制，仍由资源预算约束；独立完成评价与真实行为评测继续负责结果质量。不得以novel_results大于零直接标completed。

恢复no_progress目标必须引用等待之后的新用户输入、导入资料或真实外部资料回执，并重新检查来源。暂停/等待/激活本身不清零计数。没有新依据时不能以相同origin_type及重叠origin_ids改名创建新目标；这是保守的来源级抑制，不声称识别所有同义句。goal.*、decision.*、run.*、自身message.sent/message.drafted不能单独作为新事件型目标的来源，防止仅靠调度通知自我繁殖任务。不同来源形成的新问题仍可提出目标。

worker在服务启动时运行，初始化前静默休眠；关闭信号同时停止HTTP接入与worker等待。前台与后台共享同一Store、租约、模型步骤和动作执行器。每次先领取现有工作（前台优先），再触发到期目标；执行已经提交的planned动作后才能请求下一次模型判断。最后一次模型调用提出的工具仍在工具上限内执行，不能仅因模型次数耗尽而丢掉实际执行机会；之后以run.stopped保留真实结果ID，不能编造一条模型总结或完成声明。

临时模型错误（传输失败、408/429/5xx）最多重试两次；结构/领域无效提案最多修复一次，合计仍受每循环三次模型调用、120秒期限及每日用量限制。重试次数、1/2秒退避及固定验证反馈持久化，重启不重置。反馈引用原输入来源并进入下一步上下文；不保存供应商原始错误正文或无效提案。未知用量按已预留上界结算，重试不是免费调用。数据库错误不按模型错误重试，worker延后5秒重新检查持久状态。

预算不足、缺少凭证、未验证token上界、能力不可用使当前后台目标进入具名资源等待；不计为无进展或人格退化。预算等待设置当前开放账期结束时刻；没有开放窗口时按配置时区下一日界计算。预算冻结时不自动恢复。到期检查重新校验来源、生命周期与自主开关后恢复目标；其他资源等待需修复能力/配置并显式重新激活，不能定时烧空请求。普通等待/激活清除旧resume_at。

空闲检查遵守idle_check_minutes，但运行租约到期、未来目标到期和预算恢复会提前唤醒；pending联系最多每60秒复查一次，仍受联系政策约束。进程内Notify只负责加速唤醒，不承担可靠消息存储；未来API在事务提交后通知worker，重启靠数据库恢复。停止时放弃正在等待的远程响应并保留持久租约；新进程在租约到期后按未知用量/动作规则恢复，不承诺远程请求已经取消。无凭证加载UnavailableProvider；没有生产环境开关自动切到FixtureProvider。

## 6. 能力和媒介

M1 能力：`memory.search`、`documents.read`、`notes.write`、`web.read_allowed`。访问自身已授权空间，所有返回内容带来源。配置要求某个能力但二进制未实现时启动失败；可用能力临时掉线则进入 unavailable 并暂停依赖目标。

已实现注册表 `IMPLEMENTED_CAPABILITIES` 包含三个本地能力和 `web.read_allowed`；`LOCAL_CAPABILITIES`只列数据库本地能力。配置仍单独决定是否授权，调用前再次核对真实版本、权限、租约和调用限额。网络主机列表为空时不给模型声明已授权的网络读取，非空时能力描述包含精确允许主机。服务恢复认可上述四个实现；这不代表当前宿主网络已通过公网连通验收。

`documents.read` 只读取明确导入的 UTF-8 纯文本，M1 不抓取任意系统文件。导入正文最多1 MiB，标题及source_label各最多512 UTF-8字节，拒绝空白与NUL；超出拒绝而非悄悄截断。正文保存在documents，导入事件为external_document，只包含元数据。读取参数为document_id，可选offset（0..1048576，默认0）与max_chars（1..4096，默认4096），位置按Unicode字符计；返回text、next_offset及总字符数。返回的资料内容仍标为external_document，不能作为亲历记忆的唯一来源。

`notes.write`只接受title和text，不接受path、shell或command。标题最多200字符且512字节，正文最多12000字符且16 KiB；拒绝空白与NUL。标题即使含斜杠也仅作为数据库文本保存，不做路径解释，不触及宿主文件或符号链接。这是对早期文件工作区方案的收敛，仍可在后续导出为文件。笔记本身为生成内容，成功回执仅证明保存操作实际完成。

notes.write还可携带可选citations数组（最多8条），每条为 `{document_id,start_char,end_char}`，Unicode半开区间长度1..2048。每个引用都必须在本次或同目标此前循环实际读过，正文包含原文；先验证全部引用，再原子保存笔记、引用和来源边。结构无效属未执行参数错误；已进入执行但内容不符、未读或来源不可用时记录failed回执，计一次调用但不保存笔记。无引用笔记继续可用，但不会通过摘录完成契约。来源明确只证明引用准确，不保证笔记其余生成内容正确。

`memory.search`当前版本3参数为query（非空、最多1000字符）、可选limit（1..5，默认5）以及互斥的person_id（本轮可见人物引用/input_manifest）或person_name（逐字匹配的已保存姓名/别名，1..80字符）。只能检索本次触发事件范围与general的有效记忆，不允许模型扩大scope；人物筛选仅接受当前有效事实主体关联。同名和未知明确失败，详见文末共享读取契约。返回真实来源、人物注记、实际模式和索引覆盖。检索结果仍为generated的已有认识集合，不升级为亲历证据；失败明确记录failed，不把无法查询写成空结果成功。旧版本意图不静默改协议。

提案schema在模型提交时先拒绝非法结构；执行层再次解析参数与字节限额。参数无效写failed/executed=false回执，不计实际工具调用；真实读取失败写failed/executed=true并计一次。成功与失败结果、完整输入和新读取来源都进入派生图。执行收据按action_id去重，重放不重复扣次数。资料删除屏障/隐私代次同样阻止旧收据重放。工具形状示例见 `evals/tools.example.json`。

`web.read_allowed`只允许HTTPS、443、配置中的精确ASCII主机。拒绝URL凭证、空用户信息、IP字面量、模糊主机编码、尾点、片段、控制字符及重定向。DNS答复最多64个地址，任一非公网地址即拒绝整个答复，包括回环、私网、链路本地、共享/基准测试/文档网段、组播、IPv4-mapped、IPv6转换/隧道地址；IPv6仅放行2000::/3内未被排除的范围。策略以IANA特殊用途登记为依据并保守排除部分协议专用地址，不能因为配置了主机名就允许其解析到内网。

HTTP适配器每次新建客户端，将检查后的地址固定到连接，正常校验证书与主机名；关闭系统/环境代理、重定向、隐式重试、Referer及共享连接池。无Cookie、模型会话头、模型上下文和部署凭证。仅GET，连接最多5秒，DNS与响应整体最多20秒，且不超过剩余循环时间；执行中继续续租。只接受UTF-8的text/plain、text/html、text/markdown、text/csv、text/xml、application/json、application/xml、application/xhtml+xml；HTML仍是数据，不执行脚本。支持gzip并按解压后字节计数，其他编码拒绝，Content-Length和实际分块都受1 MiB限制。空正文、NUL、无效UTF-8拒绝。

网络操作在调用前短事务检查并标记executing、扣一次工具次数；DNS失败也属于实际尝试。URL/主机前置拒绝写failed/executed=false回执、不计调用。网络和模型不持有数据库写事务。获得结果后重新验证租约、版本及隐私代次，再把完整正文、来源、回执与状态原子提交；未知或失效结果不补造为成功。崩溃后的executing动作转unknown，保留已计次数，不自动重新请求。成功/失败回执重放都不触发第二次网络请求。

成功网页以external_document保存到documents，保留实际URL、HTTP状态、内容类型与执行模式，tool.result只携带首4096字符、document_id和next_offset；继续用documents.read获取其余内容。本轮回执纳入完整上下文和删除派生图。独立测试适配器必须显式标fixture，不能冒充live抓取。

实际实现参考：[reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html)、[IANA IPv4特殊用途登记](https://www.iana.org/assignments/iana-ipv4-special-registry/)、[IANA IPv6特殊用途登记](https://www.iana.org/assignments/iana-ipv6-special-registry/)，核对2026-11-11。测试使用隔离TLS服务器与证书，生产代码没有允许私网、跳过证书验证或代理绕行的测试开关。

M4 工具创造在无宿主凭证、默认无网络、受限 CPU/内存/时间的隔离执行器进行。产生带版本的工件与测试；注册由程序验证，不执行模型随意给出的 shell。外部设备与通讯发送不在 M1 能力列表。

InteractionAdapter 将输入映射为 Event，将已提交输出投递到媒介；媒介自己不维护独立人格。BodyBinding 记录连接与观察能力，未接入 modality 不得制造 Observation。

## 7. HTTP 与 SSE v1

所有 `/api/v1` 请求要求 `Authorization: Bearer <instance-token>`。浏览器用内存中的 token 和 fetch streaming 订阅 SSE，不把 token 放在 URL、日志或 localStorage。服务同源部署，默认拒绝跨源；Cookie 不用于首版认证。`/healthz` 无鉴权仅返回 ready/not_ready，不披露身份和配置。

成功读取返回 JSON；异步操作 `202 {operation_id, status}`，通过 `/operations/{id}` 查询和 SSE 通知完成。所有写操作带 Idempotency-Key；修改当前对象时带 If-Match（对象 version）。首次创建无 If-Match。未知字段 422，未初始化 409，未认证 401，越权 403，不存在/已删除 404，版本冲突 409，预算不足 429，模型不可用 503。错误形状 `{error:{code,message,retryable,request_id}}`，不返回凭证或提供方原始错误正文。

| 方法与路径（省略 /api/v1） | 输入与返回 | 阶段 |
| --- | --- | --- |
| POST /initialize | persona+runtime 已验证文档；201 {individual_id,version}，已初始化返回409（幂等重试除外） | M1 |
| GET /state | 身份、生命周期、当前版本、能力状态、目标摘要、预算摘要、model_mode | M1 |
| POST /messages | {text,conversation_id?}；202 {operation_id,event_id}；文本 16 KiB 上限 | M1 |
| GET /messages | cursor/limit；返回有效历史，禁止返回删除内容 | M1 |
| GET /events | Last-Event-ID 或 after cursor；见下文 | M1 |
| GET /operations/{id} | status、结果资源引用或可见错误 | M1 |
| GET /memories | q、validity、cursor、limit；内容与来源元数据 | M1/M2 语义检索 |
| POST /memories/{id}/revisions | {content,scope,reason}；记录用户纠正事件和新版本，不改原事件 | M2 |
| DELETE /memories/{id} | 202 deletion job；按第8节清除来源与依赖闭包 | M2 |
| POST /documents | {title,text,source_label}；保存 external_document 事件 | M1 |
| GET /goals | 目标及真实状态 | M1 |
| POST /goals | {description,success_criterion,next_step,stop_condition}；201 GoalReceipt，来源为用户目标事件 | M1 |
| POST /goals/{id}/transitions | {action:activate/wait/abandon,reason,evidence_ids?}；If-Match为goal.version；200 GoalReceipt | M1 |
| POST /autonomy/pause 与 /resume | {}；只控制后台活动，暂停后仍可聊天 | M1 |
| POST /lifecycle/transitions | {action:pause/resume/archive/restore/terminate}；状态机，不能隐式删除 | M1 |
| PUT /settings | 完整 runtime 文档、If-Match；下一轮生效 | M1 |
| GET /developments 与 /timeline | 发展记录与自传/重要事件，标明来源和状态 | M3 / M2 |
| POST /developments/{id}/revert | {reason}；新撤回事件，不能抹除历史 | M3 |
| POST /exports | {}；202；导出无凭证的 JSON+资源清单，下载需鉴权 | M5 |

除下述发展列表的有界快照外，分页默认 30，最大100，返回 `{items,next_cursor}`。cursor 为不透明位置，不是认证凭证。

SSE 格式 `id: <outbox-sequence> / event: <type> / data: <JSON>`。类型固定为 `operation.updated / message.created / state.changed / memory.changed / goal.changed / development.changed / privacy.changed / budget.changed`，数据只含必要资源 ID 和版本，不包含记忆原文。客户端按 ID 去重；从服务器取当前有效资源。每15秒 heartbeat；保留7天 outbox；过期 cursor 返回410及重新取快照提示。重连恢复只重放通知，不重执行动作。

最终回复在验证与提交后整体投递；M1 不流式暴露未验证 token。M1 不实现的 M2+ 路由返回501 feature_unavailable，UI 不显示可用控件，不用空成功响应伪装能力。

### M1-06 当前实现范围

已接通初始化、状态、消息提交/列表、单个operation、资料导入、记忆/目标列表、自主暂停恢复、生命周期转换与完整运行设置更新。上述接口返回实际持久状态，模型建议仍只经过worker提交；HTTP层不直接调用模型。写入成功后发出进程内唤醒。个体初始化不在页面加载时自动发生；前端使用仓库同一版本的配置示例创建初始种子。

请求JSON在反序列化之前检测所有层级重复字段，未知字段和错误类型422；正文默认128KiB，资料导入允许2MiB JSON（实际正文仍限1MiB）。必要请求头唯一；Idempotency-Key为1..128可见ASCII字符。If-Match使用带双引号的正整数；自主/生命周期/设置针对/state的ETag即全局state_version，不是config_version。幂等重放先于版本检查，以支持提交成功但响应丢失后的同请求重试。设置更新同时递增config_version；任何状态/规则更新都使旧认知提交失效。

读取投影用一致性快照和递归来源阻断检查：消息、记忆、目标、运行和返回的消息引用不能携带被阻断来源的派生内容。/state提供当前人格、生命周期、运行规则、能力授权、可见目标统计、当前账期已计费用量与预留量；model.mode/readiness来自实际服务适配器，不含endpoint、凭证或提供方错误正文。已验证的指定Go/GLM文本路径readiness为ready；缺凭证为unavailable，其他未验证路径为unverified_token_bound；fixture仅在显式测试中出现。计数依据和托管服务假设见[MODEL_BUDGET.md](MODEL_BUDGET.md)。无账期返回null，不虚构历史用量。

列表使用版本化且绑定列表种类的游标；/messages默认时间正序，可用order=desc分页读取最近消息。游标仅是位置，不能代替鉴权；正反序游标不可混用。记忆q目前为所存文本的字面包含筛选，非语义召回；validity可筛选active/disputed/superseded/stale。/goals返回真实状态及等待原因。未知查询字段、非法游标和不在1..100范围内的limit返回422。

同源请求通过Host与Origin规范化比较，拒绝null/跨源Origin以及cross-site Fetch Metadata；不信任转发头，无Origin的带令牌CLI请求可用。重复Authorization拒绝；API响应带no-store与nosniff。非法输入、方法不支持、未知接口、尚未实现接口分别有统一JSON错误；数据库细节不进入HTTP响应。新异步消息返回202/status=accepted并引用operation；执行失败在operation.error_code如实反映，202不代表模型成功。

当前记忆更正删除、时间线/发展和导出路由仍返回501；未定义路由404、已定义路由的错误方法405。/events已接通，前端通过SSE刷新；消息历史采用最新在前的分页读取、对话内按时间顺序展示。所有渲染文本经React默认转义，令牌只在内存；断开清理个体视图并取消在途请求，旧会话响应不得重新显示私人内容。写入重试复用同一请求的幂等键；编辑运行规则保留打开编辑时的版本，遇到冲突重新读取而非覆盖。图中示例健康分数、羁绊积分、运行年限、连接数量不作为产品假数据。

### 用户目标管理

POST /goals 的四项文本各为1..2000字符（非纯空白，不含NUL）；未知字段拒绝，不能传status直接完成。写入reported_claim来源的user.goal_requested事件、目标origin_ids、来源边、状态版本、outbox与幂等回执在同一事务；默认priority=50、proposed、due_at为数据库当前时间。创建允许自动调度，但不绕过个体暂停、自主开关、能力或预算。生命周期active/paused可管理目标，archived/terminated拒绝变更。

转换仅activate/wait/abandon，reason同样1..2000字符；evidence_ids最多16个不同UUID，只接受已有用户消息、导入资料或真实外部资料回执，递归检查来源。If-Match匹配目标自身version，不是全局state_version。回执含individual_id/goal_id/event_id/version（全局）/goal_version/status，ETag返回goal_version；同键同请求重放先于版本检查，但来源已阻断时不能重放旧回执。完成/放弃保持终态，完成只走实际结果验收。

所有转换递增全局状态，使旧模型/工具提交失效；旧queued目标运行标stale、planned动作cancelled，预留费用保守结算。在途running不谎称已物理取消，既有租约恢复负责成本与未知结果。旧目标循环标记superseded_by_owner，不再由延迟结算改变新目标的due_at或无进展计数；真实Run、Action和结果仍保留。

wait记录owner_wait，模型不能将其激活或放弃，只有显式owner命令能解除该控制；清除due_at/resume_at；activate重设due_at，清除资源等待。不因普通wait/activate抹去一两次无进展。达到三轮后activate必须引用等待之后的新外部事件，才重置计数；转换理由自身不是新证据。变更与证据形成来源边，可追溯也受删除闭包约束。

WebUI意图与行动页具备目标表单、带目标版本/理由的变更确认、来源查看和实际事件回执。失败保留输入；冲突后重新打开当前目标，不能覆盖旧版本。列表分页仍以当前已加载范围显示，不把局部条目数声称为总数。

### 持久事件流与 UI v2

/state 在同一读取快照内返回十进制字符串 event_cursor，避免浏览器整数精度丢失。读取快照后从该游标订阅不会漏过期间提交的事件。GET /events 接受唯一 Last-Event-ID 或 after，未知/重复/冲突/负数/超前游标422，未初始化409；未传游标从当前高水位开始。15秒注释心跳，立即 connected 帧；ID-only 注释仅推进游标，不披露不可用来源。资源类型限定八种。正文始终经鉴权投影重新读取。

0010_event_stream.sql 保存删除水位；七天过期游标410/event_cursor_expired，提示重新读取快照。每轮worker及订阅建立时清理最多1000条连续过期前缀，逻辑过期检查立即生效。时钟反向时保留从首条未过期通知起的完整后缀，避免跨越仍有效记录。全部清空仍保留高水位；不把序列间隙误判为事件丢失。

每实例最多16流；每次读取最多100条元数据，间隔1秒查询持久outbox，无长事务。断开释放许可，关闭进程通过共享shutdown结束流。已发响应不能改HTTP状态，中途存储/隐私/过期错误终止流，重连返回明确状态；不输出存储错误正文。响应no-store/nosniff，X-Accel-Buffering:no。重播仅返回通知，不触发运行执行。

SSE必须把建立时的共享privacy_generation绑定到整个流：每轮出队和等待结束后检查ready/generation，失效即丢弃旧队列并EOF，恢复ready也不能复活旧代次；notifications的RR读取两端同样检查。P01独立固定旧182源发现旧队列opaque UUID晚到与门关闭后继续出帧两个反例（同一P1边界），1过2败原报告保留；root两源修复后，独占新target实际4PG/HTTP反例全过，含恢复后旧queue/idle流仍EOF、新流继续合法通知。接受root-privacy-stream-01三源manifestf0df87f89957df500033eaa3a6b66c99d6dee1f10fc9a9bc1d78e4cbe0cb8f1f，报告SHAa47d48ad0d97ed4a95897e80f1eff4ee212db0760c3d0a9225cefb003f84c6f6。此证据仅支持所测异步边界，不宣称TCP所有取消时点或全产品隐私认证；客户端清屏/重连另由P02-07验证。

WebUI按Console v4设计包使用十三个主入口与子工作区，文字交流位于接入渠道，认知/关系位于个体工作区，追踪/日志位于数据工作区；旧书签兼容。设计入口为docs/design/ALIA_UIUX/AGENT_START_HERE.md。生产只使用Real Repository；主题不改变个体状态；暂停恢复及运行规则变更先展示影响和版本再等待服务端回执。未实现领域接口明确显示未开放，不用原型MOCK替代。具体差异、截图与测试见docs/ai。

## 8. 删除、备份与恢复

### 删除持久性与服务进程门（ROOT-PRIVACY-01/P05-05，已接通、局部验证）

0024 privacy_operations保存无正文任务与journal/key/request哈希、source roots/派生闭包、阶段、版本/代次及静态安全错误。实际DELETE、进度、启动重放与后台清理已接通；schema/privacy-operation及合成示例只规定回执/鉴权进度，不作为操作成功证据。Receipt为六字段，Progress加observed_at/last_error；不得completed仍有cleanup错误。现有M0校验拒绝非法状态、0版本/代次、nil/伪UUID、raw_key及矛盾完成回执。

Journal v2在原六字段intent外保存idempotency_key_digest，实际请求必须携带该哈希，不能用request_digest反推并杜撰原key关联。v1文件可按原字节严格验证/读取，缺key_digest时生产启动重放fail closed，不宣称能无损恢复HTTP幂等；以前v1仅作为未发布文件primitive测试。旧记录不回写/重算成新版本。受理需要真实file+parent fsync，再完成DBepoch/阻断事务，失败不能确认。

Store共享privacy_ready及monotonic privacy_generation：fence与恢复跨clone生效，普通写锁、snapshot/model_context前后比对，HTTP授权后及handler返回时再次比对；即使恢复重新ready，旧generation的私有响应也不能重新可见。未授权仍401，health为not_ready。只有已提交的opaque DELETE回执可由服务端typedextension允许跨代次返回，客户端header/model输出不能豁免。独立日志已持久、DB结果未知时关闭门直到可信replay；write_for_privacy_recovery只供该内部恢复协调器，普通model/CRUD不得调用绕过。当前3项定向单测已通过；实际日志+epoch/purge/旧备份恢复仍待P05与root集成验证。

### 在线删除接口（实际服务端与启动/后台已接通）

DELETE /memories/{id}无请求正文，要求唯一Idempotency-Key与目标memory.version强If-Match；不是个体全局版本。只在实际独立日志就绪且启动重放成功后开放；日志不可用返回503 feature_unavailable。目标来源与派生闭包必须在写锁内重新计算、独立日志fsync、提高隐私代次并阻断后，才能202返回DeleteReceipt。回执精确individual_id/operation_id/target_id/state_version/privacy_epoch/status，status为blocked/purging/retry_pending/completed。受理不等于完成，不提供模拟百分比或撤销已删除操作。

GET /privacy-operations/{operation_id}同样鉴权/no-store，返回上述六字段及observed_at/last_error（静态错误code或null），不存在404。DELETE同键/同target/同If-Match先重放实际回执，即使目标正文已清理；不同意图同键409，响应丢失仍用同键查询/重放。删除操作本身只保留opaque ID/版本/阶段/安全错误码，不恢复被删内容。已受理的操作在服务重启后依赖独立日志与数据库持久状态恢复，不能以浏览器内存为唯一状态。

/management现提供真实privacy:{deletion_enabled,journal_ready,status_read_available}，由Runtime已成功恢复的日志句柄与共享ready决定。这些布尔值只表示入口就绪，不能替代每次授权/版本/隐私校验。旧客户端兼容缺字段并禁用删除；UI不可用本地开关模拟授权。P02-05完整真实浏览器验收仍须另行执行，不能以服务端局部通过代替。

部署配置ALIA_DELETION_JOURNAL指定独立持久文件，父目录须已存在且0700，文件0600；不能放在会随数据库备份一起回退的临时目录。main在监听、模型、索引及普通状态恢复前调用Runtime::configure_deletion_journal，先关闭共享门再校验/重放/清理。已有个体只允许非创建式open_existing，文件缺失、损坏、身份不匹配或旧v1不足以恢复时拒绝启动；新数据库可在可信私有目录首次创建。未配置日志且DB没有privacy任务时保留旧版只读/禁用删除模式；任何已存任务（包括completed）都要求恢复原日志，不能通过删环境变量绕过。可选compose.privacy.yaml显式挂载已由管理员准备的私有独立目录，基础Compose不偷偷创建目录或文件。

独立维护循环每次最多清理4个任务，5秒低频检查并接受DELETE唤醒；普通自主暂停不取消已受理删除。若共享门关闭，先重放日志协调未知结果，再尝试清理；失败只输出静态警告并保留阻断/重试状态，不打印正文/路径/存储错误。shutdown结束循环。Runtime构造后的外部服务调用者同样必须先执行配置恢复方法，不能绕过main启动契约。

ROOT-PRIVACY-HTTP v1固定181源局部验证已通过：5实际HTTP/启动/后台、13管线、9闭包PG；9日志、3共享门单测、严格Clippy，source_unchanged及隔离资源cleanup通过，0模型/部署。报告evals/results/root-privacy-http-v1/checks/report.json SHA57b5062312ac315e143e7e92860b69b5289876d9d11ee96439a15fb1b9b204d7。随后open_existing消除metadata检查与创建式open之间的文件丢失竞态，新增文件测试；最终v2固定182源同7阶段通过，27实际PG/HTTP、10日志、3单测、Clippy/source/cleanup全部通过。v2报告SHAaa945d96be12fb55cfd40c1a9cbad871b2256ff43e2d00dd6a8a6addc89f4cf4，接受包root-privacy-http-02保存实际十一源，旧v1不覆盖后改源码。受管备份、完整导出和恢复产品仍未完成。


普通历史追加保存；显式删除可物理清除内容，只留不含正文的必要操作标识。删除记忆按其来源事件开始，沿 derivation_edges 和完整 Run input_manifest 做保守闭包：事件正文、对话回复、记忆/信念/自传、目标、关系/人格变化、笔记、嵌入、导出与受管备份都纳入。多来源摘要含被删内容时删除整个摘要，必要时从剩余来源重新计算，不试图凭字符串替换保证安全。

删除流程：

1. 在独立于备份的本地删除日志中持久追加 opaque 对象 ID、删除 job ID 与个体 ID；fsync 后才向客户端确认受理。日志不写原文。
2. 短事务增加 privacy_epoch、标记来源阻断、取消受影响任务。请求未完成前程序启动也必须先回放删除日志；失败保持 not_ready。
3. 停止新的相关检索和生成提交，清理数据库与本地派生文件。清空进程上下文缓存；已在云端执行的请求只能取消或丢弃返回，不能保证撤回提供方已收到的数据。
4. 删除旧的全部受管备份及未下载/服务端保留的导出包，再生成干净备份；失败保留 retry_pending，不能报告 completed。
5. 逐对象检查不存在、检索无结果、epoch 失效提交被拒绝；写入无正文的删除完成事件。

M1 暂不创建受管备份，M2 删除路径先覆盖在线数据；M5 开启每日备份，保留7份，并实现上述失效清理。产品不能在备份开启而清理未实现时报告支持完整删除。

受管备份包含数据库、资源、schema/配置版本和校验和；不包含供应商密钥和独立删除日志。离线维护命令 `alia backup` / `alia restore --backup ... --deletion-journal ...` 在 M5 实现：暂停写入、校验校验和和个体 ID、恢复到隔离数据目录、应用删除日志并重新求闭包、迁移、验证，再开放服务。缺失/损坏删除日志时默认拒绝恢复，避免旧内容复活。

撤回依赖被删证据的发展时，记录 provenance_removed；无法从剩余证据重建的字段退回最近有充分依据的版本。不要把待清理对象重新送入模型请求其“帮忙忘记”。

已下载到用户设备的副本和第三方模型数据不在本地清除保证范围。数据导出必须标注这一边界。

## 9. 契约完成度

本次 M0 的机器校验只验证 schema、示例、默认值交叉约束、资料哈希、文件链接及任务/用例引用。来源真实性、预算并发、事务恢复、删除闭包和模型行为必须在 M1—M5 用真实实现验证。schema 通过不能被写作“人格、记忆、自主性已经实现”。


### M1 运行反馈与可复用引用范围（2026-11-11）

模型失败修复继续受一次修复、两次临时重试和总调用数约束。run.retry_scheduled新增固定词汇validation_code及对应指导，区分schema字段、来源、作用域、目标转换、未验证完成和提前回复。仅允许嵌入schema的已知字段名进入诊断，不保留无效模型原文、任意JSON路径或供应商正文。来源仍沿原输入manifest追踪。

资料读取结果增加可选citation：仅当返回text非空白且不超过2048个Unicode字符时给出准确document_id/start_char/end_char，否则为null；它只是已读片段范围，不代表目标已经完成。笔记仍必须逐字包含引用范围的连续原文。只有完整读取覆盖、正文/来源仍有效后，才可返回citation_quote_missing帮助修复；未读/失效引用继续使用统一不可验证错误，不能借失败结果探测或取回文档正文。

首场联合真实验收入口为scripts/test_live_lifecycle.py。first-study→restore→observe→unknown分别使用新进程；同一个持久个体、前台新提供方会话；自主观察阶段不提交新用户消息、不创建或完成目标。900秒有界观察允许真实五分钟冷却，不修改数据库时间。行为判断仍需查看实际输出，脚本工程通过不等于完整M1或长期发布通过。

### 目标形成运行与证据复用

`goals.formed_in_run`保存模型创建目标时的真实运行ID；它不将整次运行改归一个目标。0014仅从可信已提交创建事件回填旧模型目标，用户命令创建的目标保持NULL。新目标可使用形成运行中实际成功的笔记和读取；多目标同次形成时，完整产物与未引用反证均提交审查，相关性由审查判断，归属不等于达标。

引用仍检查真实阅读覆盖、连续引文、来源闭包、隐私代次及执行结果；其他无关运行不借用。形成运行本身为来源链的一部分，失效后不能继续评估。审查`evidence_review/2`、摘录验证`source_excerpt/2`记录新版本；旧缓存不当成新规则下的认可，历史记录保留。actor仍须独立提交complete。等待运行时评估不自动等同暂停目标。

### 完成提交的上下文与诊断

模型上下文的current_goal_id取自实际runs.goal_id；completion_evidence以goal_id→result_event_id列出当前通过验证的结果引用，且对应事件加入events/input_manifest。它与verified_goal_completions由同一实际验证结果生成；快照变化、来源阻断或审查失效后不再列入。模型不能写入这些字段。

完成提案必须与工具执行分开，空证据/错误结果引用返回completion_evidence_missing，同提案夹带工具返回completion_action_conflict；缺少有效资格仍为unverified_completion。提交事务重新验证并要求确切结果事件，不能用目标ID、动作ID或资料ID代替。新增诊断不放宽来源、状态、预算或每循环修复次数。

### 派生向量集合

向量按INDEX_PROFILE整体管理，独立于人格及记忆事实状态。索引准备返回不允许外部构造的版本/隐私快照；模型计算不持有数据库事务。提交必须覆盖全部非空白Unicode字符，检查单位384维有限向量，随后在事务内重验来源、版本、摘要、privacy_epoch，一次替换完整集合。集合缺块、记忆改变、来源或祖先阻断、代次不符均不参与查询，失败不能留下部分新索引。

当前精确查询支持valid-time、一般/局部scope和来源交互参与者过滤，历史标记显式开启才纳入superseded；disputed/stale不作为有效认识。关系参与者不等于正文人物，实体解析尚未完成。存储方法当前是内部接口，未赋予模型构造向量或任意修改索引的权限；自动索引调度与服务切换仍待实现。

### 人物引用与主体注记

person实体以独立UUID及external_key标识，display label不是唯一身份。来源区间采用Unicode字符半开区间，名字/别名最多80字符，禁止空白名称、首尾空白和控制字符。可信owner内部命令采用expected state/privacy、幂等、事务事件与outbox；模型另走受限推断提案，不能借用owner权限或从资料指令获得写权限。注册和别名精确解析，不自动合并同名/近名；Unknown和Ambiguous是明确结果。

MemoryPersonAssertion绑定记忆版本、内容摘要、人物、subject/mentioned角色、记忆中的位置及实际source span。来源必须是该记忆的来源，且名字是人物有效别名。声明角色的语义仍须由调用方验证；字面存在不是语法或事实正确的保证。当前角色更正保留旧事件。独立person_id过滤与来源关系subject_id分开，版本/摘要失配或来源闭包阻断不参与匹配。阻断实体保守地阻断依赖的主体及提及记忆；普通名称变更不通过阻断实现。

模型提案的可选entity_changes（最多6项，旧提案缺省为空）已接入正常收费认知调用和同一提交事务。memory引用当前可见existing.id或本轮memory_changes的created.index；person引用有界person_catalog的existing.id、本轮new.name或更早提案的prior.index。模型不能指定actor、生成UUID或新建别名；新名字碰到库内已登记名称（即使目录截断或隐私阻断）直接拒绝，不静默合并。

source必须引用本轮events内实际可见消息text或成功读取的tool.result.output.text，明确Unicode半开名字区间；仅有目录不算读过全文。该事件还必须是目标记忆来源。memory_start_char可省略，此时名字在记忆正文必须逐字且只出现一次，程序计算真实Unicode位置（含重叠出现检查）；多次出现须显式给出准确位置，不近似匹配。已存在记忆还重验版本和内容；同批同记忆/人物重复关联使整项提交回滚，费用仍保留。

person_catalog最多20个人物、每人8个有效别名，并受96KiB上下文上限约束；has_more表示截断。人物及别名来源进入input_manifest，隐私失效后不再注入。来源、引用、区间、正文、已知人物和prior错误分别给固定诊断，不保存无效模型原文。

新事件person.inferred和memory.person_inferred固定alia/inference，记录真实来源、收费decision和计算后位置；人物、记忆及关联在同一事务提交。不调用owner注册命令，也不将推断变成身份核实。幂等回执同样递归检查来源祖先，阻断后不能返回旧成功回执。字面和引用校验仍不能证明角色语义或摘要忠实；真实开发用例曾出现省略关联却宣称登记成功，失败报告必须保留。服务查询组合与自动索引仍未完成，完整实体识别和错源验收未通过。

### 记忆检查器的人物关联投影

GET /api/v1/memories每项新增person_assertions数组和person_assertions_more。每项关联仅含entity_id、label、role、assertion_event_id、source_event_id、actor_id、source_kind、occurred_at；不返回内部external_key或任意原始payload。同一只读可重复读快照批量读取，重新验证记忆版本/SHA256、人物/来源/关联事件的递归可见性。最多20项，截断明确标记，缺省为空不代表人物已经核实。

WebUI记忆检查器分别展示事实主体/仅被提及、模型推断/陈述注记、记录者和实际来源/关联事件。此处是记忆引用，不当作关系对象档案或亲密度；没有关联时直接说明未保存，不根据回复文字补造。该投影为只读；记忆修订删除及人物HTTP写入口仍未开放。

### 派生索引任务与进程生命周期

memory_index_jobs按当前INDEX_PROFILE管理，绑定memory_version/content_digest/privacy_epoch，状态queued/running/ready/failed/cancelled。任务最多三次、租约120秒、编码异步等待60秒、失败冷却60秒；attempts只在认领时增加，重启不清零。指纹变化建立新尝试周期，generation持续增加以拒绝旧执行者。最多发现32条，逐条处理，前台当前版本运行优先；暂停/非active不认领且旧结果不得在暂停期间提交。

索引计算和模型推理都不占数据库事务；finish_memory_index在短事务中复验来源闭包、版本、摘要、隐私代次及租约，一次写全向量和ready。故障导致整体回滚，不产生半套集合。索引仅为派生缓存，不写生活事件、不扩大权限、不推进个体人格状态版本、不使用云端预算。失效源不能被重试复活，未知/失败状态不报告成功。

部署变量ALIA_EMBEDDING_DIR缺省关闭，显式提供时要求local-embeddings构建及全部固定资产验证；不支持或损坏即拒绝启动。/state.indexing声明真实启用配置和profile，retrieval_mode来自实际Store查询编码器：未配置为lexical；加载并注册后为hybrid，对话和工具共用该Store。查询回执再报告本次模式和覆盖，不以worker状态代替读取证据。原生ONNX调用暂不能硬取消，异步取消不允许晚到持久化，编码器信号量覆盖到原生返回，限制同时计算数量为1（内部CPU线程2）。恢复、重算和阶段证据见EMBEDDINGS。

### WebUI v3 管理投影与隔离校验

新增owner接口为GET /runs、/runs/{id}、/audit、/diagnostics、/management、/persona，以及POST /sandbox/validate。统一沿用Bearer、同源、no-store和固定错误；源对象或祖先阻断后运行/步骤/事件不可读取。详情和诊断在同一只读可重复读快照获取，分页limit为1..100，游标命名空间不混用。

运行与审计筛选契约（P02切片）：`GET /runs`接受`cursor/limit/status/q/mode/from/to`，`GET /audit`接受`cursor/limit/q/from/to`；重复、未知字段和无效值返回422。limit默认30、范围1..100；status为queued/running/committed/failed/cancelled/stale，mode为foreground/background。from/to为RFC3339零偏移UTC，使用半开区间[from,to)，同时存在时from必须早于to；运行按created_at筛选，审计按occurred_at筛选。

q最多200个Unicode字符；纯空白等价无搜索，非空保留首尾空白且拒绝控制字符。使用PostgreSQL同一lower规则规范化搜索与筛选摘要，以字面不区分大小写子串比较，`%`与`_`不是通配符。运行只搜索UUID和经过公开错误码白名单投影后的错误；审计只搜索kind、actor_id、事件UUID及隐私屏障后仍可见的correlation_id。消息/资料正文、原始供应商错误、被阻断对象及其派生内容不参与搜索。

运行按(created_at,id)倒序，审计按事件sequence升序；`items/next_cursor`响应形状保持。带筛选游标绑定规范化条件和接口命名空间，改变条件后复用游标返回422；等价UTC/大小写表示可复用。无筛选保留既有v1游标，不允许用旧游标绕过非空筛选条件。游标是分页位置而非读取授权，实际数据每次仍执行隐私与所有权检查。

WebUI筛选显式提交并返回第一页；列表与选中详情分开读取，所选项不在当前页/条件中时明确提示。自动跟随默认关闭，开启后只跟随第一页，每5秒刷新；隐藏页面暂停自动读取，历史分页关闭跟随，同类列表/详情请求分别串行合并，避免SSE与定时器叠加。失败显示错误、保留最后成功时间；不会把旧结果称为新刷新。此事件日志是持久元数据，不是原始进程日志流。

运行按created_at/id倒序稳定分页，status只能为持久枚举；步骤按实际started_at（缺失时created_at仅用于排序）排列。返回实际模型用量/未知用量、工具能力版本、源结果ID与计时；输入输出正文、原始provider错误、headers和input_manifest不进入此接口。未识别的错误归为unclassified_failure，不返回任意存储字符串。0019新增可空的真实工具开始/结束时间，历史不回填；unknown或缺失时间不虚构耗时。

management仅返回审查过的提供商/模型公开名、凭证存在布尔值、输入计数方式、实际注册能力和Web渠道；任意自定义模型名统一custom_deployment_model，不回显端点或密钥。模型就绪不等于推理已经成功，渠道分组不宣称私密会话隔离，内置能力不冒充插件安装。

persona比较数据库真实seed与persona，注明state_version是全局状态版本，未实现中间修订和rollback。只读比较不能直接覆盖人格。diagnostics的database=reachable仅代表本次查询成功，不表示模型、工具和所有目标健康。

sandbox输入为闭合对象{kind,document_text}，kind只允许persona/runtime/decision_schema；document_text最多65536 UTF-8字节，保留原文做严格JSON解析以拒绝重复键。返回valid/checks/not_executed/state_modified=false/observed_at和固定代码；不回显输入，不调用模型/工具，不写事件或个体状态。decision_schema仅验证结构，不验证证据、预算、权限或版本，前端必须明示不能据此执行。完整写入与未实现业务边界见docs/ai/ALIA_UI_V3。

### WebUI Console v4 配置与路由契约

当前唯一运行配置仍通过GET /state和PUT /settings读写，不增加模拟profile或绕过If-Match。结构化表单以完整当前文档为基线；数字范围读取runtime schema；未显示字段保留。完整JSON原文不重写，切回结构化视图前调用已有/sandbox/validate验证，避免JSON.parse掩盖重复键。草稿、配置基线和已生效状态分别保存。

未保存离开支持保存/放弃/取消；历史导航先恢复当前历史项，获得选择后才跳转。保存失败保持页面和输入，保存成功依赖服务端回执；刷新/关闭使用浏览器原生提示。页面内存之外不保存令牌或草稿。模型/渠道/扩展的缺失写接口不由本地enabled/installed字段替代。

### M2 共享记忆读取

`GET /memories/retrieve`接受闭合查询参数q（1..1000 Unicode字符）、scope（可选1..200字符）、person_id（可选已存在人物UUID）、person_name（可选已保存姓名或别名，1..80 Unicode字符，禁止首尾空白和控制字符）、at（可选RFC3339 UTC时间）、include_history（默认false）、limit（1..20，默认10）。person_id与person_name互斥。姓名在同一读取快照内逐字解析完整有效别名表；不做子串、近字或近音合并，不受模型目录20项限制。同名歧义返回422/person_ambiguous，未知或不可用引用返回404；不以全库结果或空成功代替解析失败。默认仅general；指定scope额外纳入该范围。历史模式只加入superseded且仍受有效时间约束；blocked/stale/disputed不纳入该读取。档案列表仍可检查不同状态，不与当前有效检索混淆。

返回items、retrieval、state_version、limit和exhaustive=false。每条保留来源ID及同快照人物断言投影。retrieval包含mode、profile、ranking、eligible/indexed、query_characters/query_limited、scope、解析后的person_id、person_filter（none/explicit_id/exact_alias）、有效时间及历史选项；旧已保存回执缺失person_filter时保持unrecorded，不补造定位证据。数量受同一过滤条件约束，不是全库健康分数。

memory.search v3使用相同person_name/person_id契约（limit仍最多5），模型在正常收费认知步骤中提出姓名目标，程序再精确解析；没有隐藏的额外模型调用。未给人物目标时不通过姓名子串自动收窄排名。显式UUID继续要求本轮可见引用；姓名查询属于已授权memory.search的只读发现能力，不获得创建人物、增加别名或修改关系的权限。解析失败产生有错误代码的失败工具回执并计一次调用，不伪装无相关记忆。歧义错误不返回候选人物的私有详情；澄清后可用明确可见UUID重试。v1/v2已完成回执保留历史，待执行旧版本按版本不符拒绝。

姓名解析的人物、别名来源及断言加入结果/动作/运行的来源图，即使该人物暂时没有合格记忆。提交前再次解析同一姓名，复验目标及来源，缓存回执同受递归撤销。共享读取给对话和工具的Memory增加与WebUI一致的person_assertions/person_assertions_more；每条最多20项，保留subject/mentioned、记录者及来源性质。上下文和工具结果依赖这些断言及人物节点，撤销后不能提交旧输出。注记是存储判断，不保证语义真实或身份已验证。

当前策略semantic-primary-unindexed-interleave3/1：已索引候选按精确余弦距离排序；未有当前完整索引且词项匹配的候选按现有范围/匹配/时间排序，每三个语义候选后纳入一个，空池以另一池补足。未配置编码器保留明确lexical模式。不能将普通词项计数与语义相关性作为等价投票，首次RRF试验及独立反例失败证据保留。

查询编码在事务之外；读取后复验身份、状态版本、privacy_epoch。工具执行前后另检权限、租约、调用上限与参数摘要；结果提交重新读取所选记忆，复验正文、版本、范围、时效、完整来源及递归阻断。工具的真实起始时间在编码前观测，完成/来源/计数/回执原子提交；编码故障计一次失败执行，缓存回执重放不再次编码。准备结果不能记作执行成功。

查询最多异步等待10秒；原生CPU仍受既有单操作信号量约束。短查询使用原编码；超出512 token的有效长查询采用完整字符窗口、逐窗编码、向量求和及L2归一化。该聚合是工程启发式，不等同于模型原生长上下文理解。对话检索基于当前输入/目标，最多选1000字符，并在query_limited标明，原事件正文并不由检索串替代。更改查询策略不改变已保存向量空间；模型/分块变更仍须更换profile并重建。

实际测试、失败与质量限制见MEMORY_RETRIEVAL；索引维护与CPU资产见EMBEDDINGS。读取接通不能证明信念纠错、删除、关系发展或模型行为已经完成。


### 认知中的情绪、态度、关系与稳定兴趣（ROOT-SOCIAL-CONTEXT）

可选`ModelContext.social_context`包含当前短期appraisal（最多1项）、长期attitudes（最多4项）、relationships（最多4项）及has_more，分别保留自己的类型。每项实际来源闭包、事件全文与manifest同包进入同一个RR快照；一项最多1024对象且整个context最多96KiB，超限整包省略并报告has_more，不截断来源冒充完整理解。缺失/阻断来源、虚构背景或审查反馈作为显式情绪/关系证据时不可选入。来源读取不能重写人格、当前状态或关系。

appraisal按真实created_at/expires_at选择，到期不再带入；预留和提交检查捕获状态的有效期，过期需重建上下文。长期态度与关系不因短期状态到期而自动降低。注意排序为当前运行目标优先，其余按原持久priority加最多10分的临时目标相关权重排序，不改数据库priority、运行目标、权限或预算。用户长期未交流不自动产生关系惩罚。

审查包`target-citations/8`增加`social_interpretation`回复依据，连同完整basis_events和明确证明范围送审：它只能证明个体自己的当前评价、倾向或关系理解，不证明世界事实、对方感情或主观意识。回复可引用这些状态ID；记忆、信念、人物和自传的新事件来源仍须实际events，不能将保存状态变成亲历证据。作者与审查器的实际请求均检查完整来源并说明该边界，调用数量和原计费/24KiB审查输出预算保持。该读取切片当时尚无情绪/态度/关系新写字段；后续ROOT-SOCIAL-WRITES的现行契约见下节，不据读取证据补判写入。

出生事务创建稳定兴趣记录，迁移0027负责既有个体回填。出生的`interest:<individual UUID>:<index>`只作为一次分配的稳定ID，不随当前人格数组重排或重命名；后续自建兴趣使用其实际分配ID。上下文只读取当前active且来源完整的记录，最多32项，整份来源纳入manifest。修改人格模板不会覆盖兴趣历史；新目标提交再次检查兴趣仍active及其对象已在manifest，退休兴趣不再支持新目标，已有目标的原始origin保持。兴趣描述是动机，不是世界事实或执行证据。


### 评价、态度与关系的原子提案（ROOT-SOCIAL-WRITES）

DecisionProposal新增三个可省略或[]的字段：appraisal_changes最多1项，attitude_changes最多2个不同topic，relationship_changes最多2个不同subject；未知字段仍拒绝。分别使用AppraisalDraft（有界评价量与event_ids/goal_id/subject_id/brief_basis）、AttitudeDraft（topic/subject_id/stance/event_ids/supersedes/brief_basis）、RelationshipRevisionDraft（subject_id/expected_relationship_version/changes/event_ids/brief_basis）。关系changes只允许stage、expectations、boundaries、commitments、unresolved；不会写人格、权限或用户感情。

事件引用必须为本轮实际可见events，不能用保存状态ID、背景或审查反馈充当新事件。目标及关系来自实际当前选择，态度前身必须同topic/subject且版本仍当前；关系比较实际当前版本、完整before_state和前身，不以本轮心情覆盖未失效的承诺/边界。实际作者schema按事件、subject、版本、前身与goal收窄，但不能替代程序及语义审查。

审查包target-citations/9增加appraisal/attitude/relationship逐项目标，23项上限覆盖既有18加1+2+2；原单项41引用、24KiB审查输出、调用次数和token预算不变。每个新目标supported必须引用自身全部声明event_ids；social_under_review提供完整旧状态及真实basis作比较，不自动扩大本目标allowed_evidence_ids。短期评价、长期态度与关系理解分别审查，缺席不构成关系惩罚依据，不替对方断言感情。只审reply不能放行任何漏审的状态写入；合成审查意见仅验证工程门禁，不证明这些语义已经正确。

主决策与原内部受信任Store方法复用同一事务校验/写入内核，不嵌套新事务。模型路径仅分类其显式事件来源：既有审查反馈可以作为上下文中的因果元数据，但不能被引用为实际经历；原内部受信任方法对整个显式输入manifest的严格分类保持。关系修订先保存本轮观察版本+1；随后真实回复交流可再推进关系版本，不能据此伪造另一条长期关系修订。新状态、记忆、信念、自传、目标、回复/outbox和DecisionReceipt同事务提交；晚期失败全部回滚，已结算调用费用保留。

DecisionReceipt可选appraisal_ids、attitude_ids、relationship_revision_ids，省略/[]与旧回执兼容。已成功提交的合法回执重放不会因原短期评价后来到期而失效；尚未提交的旧评价快照到期必须重建。幂等重放仍复查原context完整来源和当前隐私代次，不能仅依靠可能缺失的通用派生边返回私有状态标识。结构示例evals/social-decision.example.json只作schema验收，ID、版本及真实语义需具体运行验证。


## M3 发展读取与 owner 撤回 HTTP 契约（ROOT-DEVELOPMENT-HTTP-01/r1）

采用 P00-M3-HTTP-01/r2，owner Root，非作者 reviewer P04（事务/版本）、P05（来源/隐私）。这三个接口共用现有 Bearer、同源写入、no-store、隐私代次与迟到响应屏障，不建立另一套身份或会话。首次实施状态为 implementing，定向结果和未运行项在专项交接记录；接口存在不代表生产验证器、人格成长或M3验收完成。

- `GET /developments?include_history=false&limit=30` 是**有界最新快照**，limit=1..100；只接受这两个字段，重复/未知/非法字段422。返回严格七字段individual_id/state_version/privacy_epoch/observed_at/include_history/items/has_more；没有next_cursor或total，不接受cursor。默认只读当前proposed/trial及仍为target head的accepted；include_history=true包含有效历史和transitions/validations，来源失效或removed对象始终隐藏。has_more表示当前限制外仍有可见记录，不表示已支持遍历全部历史。
- `GET /developments/{id}` 返回individual_id/state_version/privacy_epoch/observed_at/development；强ETag是development.version，不是state_version或target version。详情含完整保存的验证与修订历史；不存在/来源不可用404，不接受查询字段。
- `POST /developments/{id}/revert` 仅接受严格JSON `{reason}`，理由非空白、无NUL、最多1000个Unicode字符；未知/重复字段或查询字段422。要求单个有效Idempotency-Key和单个正整数强If-Match，后者绑定development.version。HTTP不先检查全局state版本或生命周期；Store在新命令检查前恢复同意图已提交回执，再检查真实来源、隐私与共享门。active/paused可发起新撤回，archived/terminated拒绝新意图；合法已提交意图在全局版本/生命周期变化后仍可恢复。
- 只有真实事务提交后返回200、强ETag=development_version和七字段individual_id/development_id/event_id/state_version/development_version/status/revision_id；reverted必须有真实新revision_id。409表示旧development版本、head竞争或幂等意图冲突；存储失败503且不提交部分人格、修订或回执。结果未知保留原键、对象版本和理由显式恢复，不另造新意图。
- `/management.development` 包含read_enabled/revert_enabled，两者表示真实路由已接入；不表示任何对象必然可读/可撤回、具有执行权限或验证已通过。未初始化或隐私关闭仍由服务端拒绝。首批不开放提案、试用、验证、接受或任意人格写入HTTP入口。

## M3 当前人格与任务策略来源（ROOT-PERSONA-SOURCES-01/r1）

Owner Root；reviewer P04（事务/版本）、P05（来源/隐私），model=Sol/high。此段采用P00-M3-RUNTIME-PLAN的A第一部分及Astra差异复核，不代表typed candidate、生产trial/评估器或成长已实现。

`ModelContext.development_context`为可省略的当前发展来源视图：seed_event_id、heads、effective_task_strategy、source_ids。head仅含target_id/target/version/revision_id/scope，不重复复制整份人格；人格值仍以persona为准。只选择四类persona目标与当前run/其实际goal匹配的策略，精确Task策略优先于Goal；空值或撤回则可回退到有效Goal策略。不纳入其他goal/task的策略，也不以未接受候选值覆盖当前persona。策略不改变权限、预算、身份、完成条件或工具执行事实。

同一RR快照验证immutable seed摘要/真实最早出生事件、完整当前投影、head/revision及真实FK/removed，取得完整祖先进入input_manifest；无head时视图可省略，出生来源仍绑定。模型每次完整来源上限4096，整包96KiB；公开人格投影不套用模型载荷限制，而直接完整验证来源图。超限显式拒绝，不抽样旧来源、裁掉反证或保留脱离依据的值。背景与测试材料不因此升级成亲历事件。

首次作者预留对完整观测来源集合精确复核；预留会合法扩充当前run的input_manifest，后续已绑定review/commit保持原观测列表，不机械要求其变成新祖先全集。同key作者预留恢复先查实际individual/run/request_key账目，完整context/allowance摘要及lease generation必须一致；即使尚未派发，也以已持久化原请求复验其观测来源、当前head/persona/scope与完整真实FK/removed，不重新生成来源列表，不再预留或派发。不同key仍走首次作者精确校验，改context或allowance仍是幂等冲突。review预留必须用实际settled作者reservation及完整原context/allowance摘要认证，不能借review标记篡改原来源；后续仍精确比较persona/head/scope、保留原来源并完整查当前实际FK/removed。上下文组装、模型预留、实际派发的lease/manifest、fresh commit及review记录复查共用强发展来源规则；公开state/persona在同一快照验证投影并保留privacy generation返回屏障。上下文元数据或persona与实际当前head不匹配、伪造goal关联、缺来源都拒绝。缓存已提交回执仍只恢复原意图，检查原对象/审查凭据及实际FK/removed来源；不因合法后续人格版本变化或短期状态到期要求重执行。已结算用量不因后来拒绝提交消失。

试用驱动的程序内部入口`execution::accounted_call_with_request_key`允许传入已持久化的非nil请求键，复用原预留/派发/心跳/结算算法。旧`accounted_call`仍生成新UUID并委托，不改变普通请求行为。驱动必须在派发前保存plan/case/side、evaluation_run及原context/allowance摘要到该key的绑定；未派发预留可恢复，已派发/已结算返回AlreadyDispatched而不再次poll工作future。此错误不代表未执行，不能另换key重试；未知结果依真实账目恢复或保持unknown。该入口不自行产生可信观察、放行trial overlay或授予独立评测身份，以上仍需真实plan及专属来源/到期校验。

### M3类型化提案原子接线（ROOT-M3-ATOMIC-01）

本节替代早期“development仅自然语言意图/始终null”的当前运行说明；历史M1的拒绝边界仍可单独验证。非空development精确采用DevelopmentDraft八字段target、before_version、proposed_value、proposed_description、evidence_ids、counterevidence_ids、validation_plan、deadline，未知字段拒绝。target仅四persona目标或task_strategy；值按对应完整人格字段/精确scope schema检查，before_version必须来自同快照development_targets，不能用全局状态版本或猜1。旧schema中的persona.interests意图从未执行；兴趣仍保留稳定ID及独立修订机制，不制造一份人格兴趣副本，模型兴趣写入接线另验。

development_targets为只读目标目录五字段target/scope/before_version/target_id/head_revision_id，含无live head时实际removed历史generation基线。组装时与当前persona/head来源在同一快照读取，元数据引用加入manifest；reserve/审查/commit前复验实际目录，不能通过伪造catalog扩大目标或scope。模型只能提交一个proposed候选，当前人格、身份、权限和预算不变。trial_binding仅关联plan/case/side/attempt/evaluation-run等ID，不含criterion expected或整个TrialWork；它不能充当授权，普通决策提交拒绝带trial_binding的生产记忆/目标/回复写入，专属试用通道独立核查。

内容审查packet升级target-citations/11，增加{kind:"development"}；审查引用该候选全部支持及反证，目标目录/当前人格只是语境，不作为额外事实依据，不证明试用/改善/接受。总decision事务调用共用propose_in_tx，统一一次global advance、回复及development.changed/outbox与总回执；末段失败全部回滚。DecisionReceipt新增可省略development_ids数组，当前最多1；同意图重放复查候选/created event/实际FK与removed来源，不重复建候选或派发，不因后续拒绝提交抹去已经结算的调用费用。该接线不等于生产trial/独立评测或成长验收通过。

内容审查packet升级`target-citations/10`，携带development_context_not_experience供解释本次人格/策略；来源ID不是本目标额外事实引用，不放宽原allowed_evidence_ids。首个实际provider适配请求和review包均需非付费协议断言；合成可信观察仅用于工程建立测试前置，不证明生产独立测量或人格成长。

### M3 受控试用调度、记账与到期收敛

本节为当前实施契约，完整验证状态以ROOT-M3-RUN-ISOLATION交接为准。试用身份来自数据库实际case→evaluation_run关联；移除ModelContext.trial_binding不能将评估运行降级为普通决策。普通预留、派发、提交（包括旧回执）与重试拒绝该运行。受控通道在Reservation/Dispatch/Result各阶段复核真实plan、case、side、原作用域、来源、当前权限和租约；binding只是关联，不是权限。baseline与trial使用同一实际输入与事实，不把判据答案注入模型上下文。

评估沿用原usage_reservations、预算、心跳和结算；attempt_id是预先持久化的稳定请求键，不能另建账本或换键补跑未知结果。真实ProviderMode与派发标记同事务保存；Fixture永远不能形成生产改善证据。每case有基线/候选两侧，跨case分开运行且仍遵守每循环三次模型/三次工具和原日预算。输出只进入试用结果，不写生产记忆、目标、工具或公开回复；评估失败不增加原目标的无进展次数或改写其资源等待。旧租约已保存部分结果时，原调度先恢复原账，保留Recorded/Unknown/NotRun，不借新generation重发。

到期维护不依赖模型可用性或新用户输入。原claim_run不领取实际plan已终态或试用窗口已到期的排队评估；空闲维护由Store::expire_development_trial_plans(1..20)在同一写事务重新检查整个个体无running，再恢复费用、结束关联队列、保留所有案例并提交发展结案。候选自身deadline到期走Expire；只有较短评测窗口到期时，按缺失验证拒绝候选，不能提前伪造候选到期。结案及通知原子提交，重复维护不重复推进状态；其他前台或后台运行、原目标和已结算费用不被覆盖。未接schema30时维护为空；partial schema拒绝，不能当成普通运行。

schema30增加的plan/case/request/result副本必须进入来源闭包、删除清理与独立就绪判据。在线删除只接受明确校验过的schema29/schema30列摘要，未知schema仍拒绝；不能因更新摘要而跳过实际清理。个人导出和受管备份另有各自完整schema/资源约束，不能借在线删除通过宣称备份恢复已完成。
