# Alia

围绕一个持续存在、能够自主活动并通过经历改变的人工个体开展开发。设计基线为 **Individual / High / Adaptive**，以完整人类生命维度作为研究参照。

当前已完成 **Phase 0 与 M1-01—M1-04 工程部分**：工程基础、身份与经历存储、事务性记忆写入、严格配置校验、模型适配与有来源的上下文检索。数据库集成测试已验证连续性与并发约束；M1-05已加入持久租约、崩溃恢复、预算账本、有记账的模型步骤、提案原子提交，以及真实本地笔记/资料读取/记忆查询。受限HTTPS读取已加入，隔离TLS与数据库测试通过；当前宿主DNS把公网域名映射到基准测试网段，公网端到端验收尚未通过。有界后台worker已接通；初版API与中文WebUI现可初始化、保存对话、查看历史/记忆/目标、导入资料与调整运行配置。持久SSE与Console v4的13页管理框架及子工作区已接通；手动目标创建、等待、激活和放弃已接通，真实受管对话与跨进程/新会话恢复已验证，完整自主探索闭环仍待完成。

- [存在与架构蓝图](docs/BLUEPRINT.md)
- [配置、数据与执行契约](docs/CONTRACTS.md)
- [实施任务、验收与当前状态](docs/IMPLEMENTATION.md)
- [UI/UX设计包入口](docs/design/ALIA_UIUX/AGENT_START_HERE.md)
- [Console v4 实施状态](docs/ai/ALIA_CONSOLE_STATUS.md)
- [Console v4 审计与页面/API映射](docs/ai/ALIA_CONSOLE_AUDIT.md)
- [Console v4 测试与截图说明](docs/ai/ALIA_CONSOLE_TEST_REPORT.md)
- [历史 v3 接入与验收记录](docs/ai/ALIA_UI_V3.md)

## 验证设计交付

需要 Python 3.11+：

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements-dev.txt
.venv/bin/python scripts/check_phase0.py
```

该命令检查 schema、示例、负例、用例引用、文档链接和原始资料完整性；不证明产品行为已经实现。当前正在实施 M1-05：持久调度与预算账本已通过测试，提案与本地能力已接入，受限网络单步执行已加入，主动联系额度/时段/去重的存储路径已实现并测试；目标到期触发与无进展等待也已加入；指定资料摘录已有独立完成判定；有界worker已串联调度、动作、完成判定和应用内联系；API/WebUI首个切片已接通，目标管理已接通，指定Go模型已通过预算契约验证，继续实现开放目标评价与真实自主探索。

`config/` 中的 JSON 是初始配置示例，`schemas/` 是配置与提案契约；`evals/` 是待执行的验收场景。`contexts/` 保留原始参考资料，以三份权威文档和最新用户决定为实施依据。

## 构建与本地运行

已验证工具链为 Rust 1.99.1、Node 22.16.0；Node 需至少 22.12。依赖由 Cargo.lock 和 web/package-lock.json 固定。

```sh
cargo test --locked --workspace
cargo build --locked
cd web
npm ci
npm run build
cd ..
python3 scripts/init_local_env.py
docker compose up --build -d
```

初始化脚本仅首次使用，创建权限为 0600 的 `.env`，拒绝覆盖已有文件。它生成本机数据库密码与实例令牌，不输出秘密。默认服务为 `http://127.0.0.1:3000`；`/healthz` 检查数据库与隐私恢复门，不能证明个体已诞生。所有 `/api/v1/*` 要求 Bearer 实例令牌。打开WebUI后输入本机 `.env` 的 `ALIA_INSTANCE_TOKEN`，在页面初始化个体；令牌只保存在页面内存，不写入浏览器持久存储。已有个体不会重新加载种子。未初始化状态返回409，尚未实现的已定义接口返回501，未知接口返回404。

WebUI采用docs/design/ALIA_UIUX中的无角色形象Console v4控制台，提供13个主页面和资源子工作区；文字交流属于接入渠道的子页。配置采用分区表单和严格原文JSON，未保存保护覆盖菜单与浏览器前后退。通过持久SSE同步，支持过期快照恢复、深浅主题、移动抽屉和变更确认。模型缺少凭证或token上界未验证时仍可保存消息，但运行明确失败，不自动生成示例回复。目标支持持久创建和状态管理；记忆修订与来源读取已接通，删除服务端需要独立日志配置，完整删除界面验收和人格发展仍在推进。

在线删除可选设置 `ALIA_DELETION_JOURNAL`：先准备0700的私有持久目录；全新数据库可首次创建0600日志，已有个体必须提供已存在的日志，丢失时拒绝启动，不能创建空文件假装修复历史。日志应独立于数据库备份保存。容器部署可显式叠加 `compose.privacy.yaml`，`ALIA_PRIVACY_DIR`指向为应用UID10001准备的私有目录；bind不自动创建宿主路径。未配置且没有历史删除任务时，删除入口禁用。DELETE的202只表示持久受理和阻断，进度须查询至completed；后台清理与启动重放均不调用模型。当前完整导出和受管备份恢复尚未交付，详见[删除契约](docs/CONTRACTS.md#8-删除备份与恢复)。

开发时仅启动数据库并验证本地已构建的服务：

```sh
docker compose -f compose.yaml -f compose.dev.yaml up -d --wait db
python3 scripts/smoke_foundation.py
python3 scripts/test_database.py
```

开发数据库默认仅映射 `127.0.0.1:55433`；smoke 脚本自动选择临时服务端口，退出时停止自己启动的服务，不删除数据库。生产 Compose 不映射数据库端口。前端开发可运行 `npm run dev`，其 API 代理指向本机 3000。

数据库集成测试默认在普通 `cargo test` 中标记 ignored；必须另运行 `scripts/test_database.py` 才能验证。SQLx 为每个测试创建独立数据库，成功后清理，不在应用数据库中初始化测试人格。服务启动自动执行嵌入迁移；已应用迁移不要修改，后续变化新增迁移文件。

## 浏览器验收

先构建Rust与WebUI并启动开发数据库，再运行：

```sh
cd web
npx playwright install chromium
cd ..
python3 scripts/test_web.py
```

该脚本使用密码免交互的 `sudo docker compose` 创建并清理 `alia_web_test_<随机ID>` 独立数据库，启动两次服务验证重启恢复。测试进程移除模型凭证，不触发真实模型计费；不会初始化或删除生产个体。截图与合成测试身份记录保存在忽略版本控制的 `evals/results/web/`。如已有兼容Chromium，可通过 `CHROMIUM_EXECUTABLE` 指定其可执行文件。浏览器验收覆盖真实HTTP、数据库与UI操作，不能替代真实模型行为或十四天观察。

## 模型接入与独立探测

默认使用 DeepSeek，凭证通过 `DEEPSEEK_API_KEY` 提供。也实现了 OpenCode Go 的兼容适配：`ALIA_MODEL_PROVIDER=opencode_go`、`OPENCODE_API_KEY`，默认模型 `glm-5.3-flash`；可使用 `ALIA_MODEL_ENDPOINT`（API基址，不含/chat/completions）和 `ALIA_MODEL` 显式覆盖。空覆盖值使用提供方默认配置。两家凭证各自读取，不互相回退。

`python3 scripts/probe_model.py` 只发送仓库中的合成场景，输出提供方标识、用量和提案校验结果，不创建生产个体，也不输出凭证、推理或回复原文。Go模式先无凭证检查公开模型列表，再执行唯一一次合成生成，最后对同一响应做提案校验；首个失败停止。错误只输出状态、受限格式请求标识和白名单类别，不进行自动修复或重试；它不是预算已验证的后台运行器。默认读取进程环境与本机 `.env`，不会自动扫描其他秘密文件。

2026-11-11 最新复测已接通：正确标注的Go凭证在Alia中返回HTTP200，真实结构化提案和领域校验通过（glm-5.3-flash，3,851 tokens）；直接运行克隆的DeepSeek Harness原生适配器也返回200并正常结束。用户已确认早期提供的API key有误；改用正确Go凭证后，此前403不再复现。完整模型行为、输入token可靠上界、长期伴侣后台和M1产品验收仍待执行，详见实施记录。

本机笔记可显式传入 `python3 scripts/probe_model.py --credentials <path>`；只读取 `opencode-go` 标题下的唯一 `apikey: <value>`。可同时保留其他供应商分段，但不会按全文件最后一条选取。`python3 scripts/test_probe_credentials.py` 无网络验证凭证分段隔离。


受限网络读取的独立诊断（无模型、无个体写入、只打印状态/大小/类型）：

```sh
cargo run --locked --example read_web -- https://www.iana.org/domains/reserved www.iana.org
```

该入口使用同一HTTPS与地址策略。若DNS返回私网或基准测试网段，报告AddressDenied；不要用允许私网或关闭TLS验证的方式将其改成成功。

指定 OpenCode Go / GLM-5.3-Flash 文本路径已通过真实受管对话与跨进程恢复试验；其他模型仍需建立计数配置。[预算契约与限制](docs/MODEL_BUDGET.md)记录来源、复验命令和实际用量。凭证仅由部署环境提供，不进入人格或模型上下文。


### 可选：固定 tokenizer 的精确文本计数

仅用于已经核对的 OpenCode Go 官方端点与 `glm-5.3-flash`：

```sh
python3 scripts/install_tokenizer.py
# 已有下载缓存时可加 --source /path/to/tokenizer.json；仍强制校验固定SHA256。
docker compose -f compose.yaml -f compose.go-tokenizer.yaml up --build -d
```

本机直接运行时设置 `ALIA_TOKENIZER_PATH=data/models/glm-5.3-flash/tokenizer.json`。开发端口可额外使用 `compose.dev.yaml`。资产保存在忽略版本控制的data目录，容器只读挂载；核心服务不会联网下载模型。未配置该路径时仍采用原保守字节上界；配置文件缺失、哈希不符、端点或模型不匹配会拒绝，不静默更换计数器。该计数针对固定公开模板，托管服务差异仍由实际用量结算与超额冻结处理，详见MODEL_BUDGET。

离线验证需要requirements-budget.txt中的可选研究依赖：

```sh
cargo build --locked --example verify_tokenizer
.venv/bin/python scripts/check_model_budget.py --rust-binary target/debug/examples/verify_tokenizer
```

联合真实验收：构建`live_individual`和`live_autonomy` examples后，用`ALIA_TOKENIZER_PATH=... python3 scripts/test_live_lifecycle.py --credentials <私有凭证文件>`运行。它会产生真实模型费用，使用独立测试数据库与新进程，报告保留实际失败，不操作生产个体。

### M2 本地嵌入（编码基础）

可选`local-embeddings` feature已提供固定资产的本地E5编码器及安装脚本，运行方式与实际限制见[EMBEDDINGS](docs/EMBEDDINGS.md)。持久分块与向量查询已实现，自动索引worker已通过可选feature和ALIA_EMBEDDING_DIR接入服务；配置本地编码后，对话、memory.search与WebUI已接入共享混合读取；未配置时明确采用词项模式，编码故障不静默降级。首次融合试验召回0.87，独立反例验证后改为语义主序与未索引候补，冻结回归召回1.0，但仍有93次原始错源命中，完整验收未通过。契约与证据见[共享检索](docs/MEMORY_RETRIEVAL.md)。

记忆查询可按人物UUID或精确姓名/别名限定，二者互斥；同名及未知明确失败，不回退全库。模型与工具可检查主体/提及及来源注记。真实模型开发试验已执行姓名定向工具，但仍有未给定属性和事实引用不足，不能据此宣称自然语言人物理解或M2已经完成。
