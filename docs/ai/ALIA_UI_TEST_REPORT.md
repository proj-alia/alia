> 历史版本 v3。当前设计、实施和验收见 [Console v4 状态](ALIA_CONSOLE_STATUS.md)。以下保留原有证据。

# ALIA UI/UX v2 验证记录

2026-11-11。使用独立临时 PostgreSQL 数据库和测试令牌，无生产个体数据，无真实模型调用，无 HTTP route mock。SQLx 的 ignored 条目由数据库脚本实际执行后才计数。

| 检查 | 实际结果 |
| --- | --- |
| `cargo test --locked --workspace` | 31 单元 + 9 提案测试通过；数据库项默认 ignored，不计通过 |
| `python3 scripts/test_database.py` | 108 项 PostgreSQL 测试通过，包括7个持久事件订阅用例和6个用户目标管理用例 |
| `cargo clippy --locked --all-targets -- -D warnings` | 通过 |
| `cargo fmt --check` | 通过 |
| `npm --prefix web run build` | TypeScript strict + Vite 生产构建通过 |
| `npm --prefix web test` | 分片 UTF-8、CRLF、多行、游标、缓冲限制、410重同步、401、取消与重复通知检查通过 |
| `python3 scripts/test_web.py` | create / restore 两阶段通过，真实 HTTP/DB/SSE |
| `.venv/bin/python scripts/check_phase0.py` | 通过；初次系统 Python 缺 jsonschema，切换项目已有 venv 后通过 |
| `node web/scripts/verify-handoff.mjs` | 原始 MOCK 原型十页可导航、无 JS 错误；仅设计参考检查，不算生产验收 |

浏览器使用 Node 22.16.0、Playwright 1.65.0、本机 Chromium，脚本支持 CHROMIUM_EXECUTABLE。当前工程没有单独 ESLint 配置，不能把 TypeScript 检查称为 ESLint。

实际浏览器覆盖：

1. 令牌连接、初始化、真实消息保存、模型缺失时显示真实运行失败。
2. 第二 HTTP 客户端提交消息，当前页面在4.5秒内经 SSE 获知并显示，无五秒轮询。
3. 暂停对话框打开时服务端状态不变；Escape 取消无写入；确认后出现实际暂停状态。
4. 资料导入、运行配置完整编辑、变更前后对照、服务端回执。
5. 编辑过程中第二客户端改变版本，旧配置提交收到真实409；原有12000预算保留，没有覆盖为13000。
6. 十路由 × 1440/1024/768/390/360px，共50种布局无整体横向滚动；每页桌面截图与五宽度总览截图。
7. 深浅主题切换前后服务端状态完全相同；断网显示网络已断开，恢复后重新同步。
8. 抽屉 Shift+Tab 焦点循环、Escape、选页关闭、刷新当前路由；最初发现焦点逸出，修复后全流程通过。
9. 目标创建、等待、激活、放弃经真实API完成；每次变更要求理由，放弃后不显示激活入口；重启后目标及终态保留。
10. localStorage/sessionStorage 为空；断开清除个体视图；重启服务后的身份、历史、暂停状态和配置保持。

截图位于 `evals/results/web/`（忽略版本控制，均为隔离测试数据）：overview/identity/memory/cognition/relations/agency/media/governance/events/settings-desktop.png、overview-dark.png、overview-{1440,1024,768,390,360}.png、restored-chat.png。已检查桌面/手机/深色总览，未使用截图中的测试数据填充生产界面。

生产打包检查：Docker镜像构建与启动成功；http://127.0.0.1:3000 的health200、无令牌state401、有令牌state409/not_initialized、静态页和两个bundle资源200；未初始化生产个体。

事件后端额外检查：游标合法性与快照交接、重放不重执行、隐私阻断仅推进游标、未知类型不泄露、七天到期与空 outbox 水位恢复、时钟反向的连续后缀保留、100条批次、16连接上限/释放、空闲不占事务、shutdown 结束流。

尚未验收：真正的外部审批、人格/记忆修订与完整删除、对象关系、完整审计、模型行为、十四天真实观察。对应界面展示不可用状态，未伪称生产能力。

用户目标API新增检查：重复/并发创建只产生一个来源事件与目标、幂等重放跨重建、调度确实排入目标、旧排队/运行中任务隔离、终态禁止重开、新外部证据恢复无进展目标、普通wait/activate保留未满三次的计数、并发目标版本单赢家、来源阻断后读取和幂等重放均拒绝、事件/outbox失败原子回滚。

补充领域用例：owner_wait目标的模型Activate/Abandon提案被拒绝；其他资源等待仍按模型正常状态机处理。

确认流程复核（2026-11-11）：修复通用确认框在服务端拒绝后仍关闭的问题。配置与自主暂停/恢复命令明确返回成功状态；只有成功才关闭，失败在弹窗内显示，保留原编辑内容和版本。提交中阻止重复触发，断开连接后旧命令回执不再回填。浏览器真实409场景验证弹窗仍打开、预算13000草稿保留、服务端仍为12000，随后取消；create / restore两阶段均通过。此次增量未改Rust或数据库；TypeScript/Vite构建、事件解析单测通过。

模型readiness新增ready状态后，TypeScript/Vite、事件解析、真实HTTP/DB浏览器create/restore再次通过。此浏览器脚本仍主动移除所有模型凭证；真实模型跨进程/新会话验证由独立脚本完成，详见[MODEL_BUDGET](../MODEL_BUDGET.md)，不能合并成浏览器已测试真实模型行为的声明。最新Docker及静态资源已部署检查通过。

最终安装与交互验收复核（2026-11-11）：设计源与安装目录24文件逐项SHA256一致。增加无效令牌拒绝、全部页面导航后主标题焦点、移动抽屉选页后焦点、跳到主要内容且不改变哈希路由、离线不刷新成功同步时间，以及十页无图片/画布/视频元素检查。浏览器create/restore均通过，50页面/宽度组合无横向滚动；重新目视检查1440px与390px截图。TypeScript/Vite、事件解析、40项普通Rust测试、108项真实PostgreSQL测试、严格clippy、fmt和M0检查全部通过。实际日志在本机/tmp/alia-ui-{browser,tests,database,clippy,build}.log。

本轮发现工作区有一个尚无实现文件的后端reviews模块声明，移除悬空声明恢复构建；保留既有审查协议开发成果，不把未接入worker/提交存储的开放目标审查标为可用。UI验收没有调用真实模型，也未创建生产个体。

本轮部署完成：Docker镜像重新构建，使用compose.yaml与compose.dev.yaml共同启动，保留数据库loopback映射。http://127.0.0.1:3000健康检查200、无令牌401、带令牌状态409/not_initialized；返回的HTML及CSS/JS与本地构建逐字节摘要一致。全程未向生产个体写入测试历史。


## 当前视图导出与最终复验（2026-11-11）

- 24个设计文件与tmp来源逐项SHA256一致，原始包未改写。
- 设置页新增状态摘要JSON下载，事件页新增当前筛选/已加载交流事件下载。两个入口均先确认并可预览，不导出完整控制器、令牌或未保存配置。
- 浏览器实际验证取消无下载、筛选后仅导出一条真实消息、保留个体ID与complete_backup=false、状态摘要不含消息/令牌、导出前后服务端状态一致。没有通过mock代替文件下载。
- 最新npm build（TypeScript/Vite）、npm test、真实HTTP/PostgreSQL的create/restore全流程通过；50页面/宽度组合、明暗主题、断网恢复、409草稿保留和键盘焦点检查仍通过。桌面1440和手机390总览截图已目视复核，无角色形象。
- cargo fmt、严格clippy和普通Rust测试通过（35单元+9提案=44项）；数据库ignored不计作本轮执行。最新数据库独立运行记录为119项通过，见/tmp/alia-review-database.log。M0检查4schema/31关闭对象/7示例/41负例通过。
- 本轮日志：/tmp/alia-ui-final-{browser,native,rust,clippy,docker}.log。截图仍位于evals/results/web；样本均来自隔离测试个体。
- 开放目标审查后端现已接入并有工程测试，替代上文历史记录中的未接入状态；真实自主试验尚未通过，不属于UI验收结果。

本次最终部署：Docker生产镜像构建通过；compose.yaml + compose.dev.yaml更新应用后，health=200、未认证=401、认证状态=409/not_initialized。HTML及CSS/JS逐字节摘要匹配本地最新构建。保留既有数据库映射，没有初始化生产个体或写入测试历史。

## 真实人物引用检查增量（2026-11-12 UTC）

- /memories新增只读人物关联投影，UI以独立MemoryPeople组件展示主体/提及、推断/陈述、人物ID、真实来源/关联事件、actor和时间。无人物图像或角色资产。
- 新增API/真实数据库用例验证actor/source_kind不改写、no-store、来源ID、内部external_key不泄露、故障改文使旧注记失效、人物阻断后递归隐藏整条记忆。默认50项普通Rust和135项真实PostgreSQL全部通过；feature为51项普通Rust，默认/feature严格all-targets clippy均通过，fmt/M0/冻结集完整性通过。
- TypeScript/Vite生产构建与SSE单测通过。scripts/test_web.py真实HTTP/DB的create/restore通过，保留原十页×五宽度、鉴权、断线恢复、409、主题和焦点验收。
- 重启前由仅允许alia_web_test临时库的显式Rust夹具保存周岚/乌龙茶、来源消息和owner/reported_claim注记；不是模型推断、不是生产历史。重启后浏览器核对源事件和关联事件与API一致、显示陈述注记且不显示模型推断。带真实数据的检查器再测1440/1024/768/390/360px无整体横向滚动；已目视复核memory-person-390.png，另留memory-person-1440.png。
- 人物推断真实模型开发试验属于独立运行证据，见IMPLEMENTATION；不能将它与这个无模型的浏览器夹具合并成一项真实端到端模型/UI行为验收。
- 日志/tmp/alia-memory-ui-{tests,database,clippy,feature-tests,feature-clippy,web-build,web-tests,browser}.log；截图位于evals/results/web。完整业务仍有后端依赖，尚未将管理面板全部业务标为完成。

最新部署复验：Docker镜像构建成功，Compose沿用现有数据库与Go tokenizer挂载更新应用。health200、未认证401、认证state409/not_initialized，HTML及JS/CSS与本地构建摘要相符。没有初始化生产个体或写入测试经历。已目视检查人物引用检查器1440px与390px截图，24个设计源文件仍逐字节一致。

索引状态展示回归：/state.indexing启用标志与profile由实际进程配置给出，记忆页仍明确词项检索。TypeScript/Vite与SSE单测通过，真实HTTP/DB浏览器create/restore再次通过。独立E5服务试验证明启用与缺失资产拒绝，未把维护开关当成语义检索或完整人物理解通过。

实际启用维护分支：index-service-ui-v2/v3通过真实HTTP读取enabled/profile，展开编码版本，五宽度无横向溢出且无角色资产。v3增加长记忆列表四行预览及选中检查器完整正文验证；截图index-enabled-1440/390.png，已目视复核手机编码版本展示。v1共享debug产物被默认构建替换而启动失败，测试器固定构建并复制二进制后通过，未绕过启动校验；报告均保留。此试验仍无云端模型调用，不是语义服务读路径或最终Recall验收。

## WebUI v3.0：17页验收（2026-11-12 UTC）

范围与未完成业务见ALIA_UI_V3。沿用React/Vite与原Rust运行时，不使用HTTP route mock或生产个体数据。

| 实际检查 | 结果 |
|---|---|
| TypeScript strict + Vite生产构建 | 通过，54个模块 |
| npm test：SSE解析/恢复 + React组件语义 | 通过；覆盖错误转义、状态/警告、Tabs焦点约定、等待服务端确认、17唯一页和配置差异 |
| 普通Rust测试 | 50项通过，数据库ignored不计入 |
| local-embeddings普通Rust测试 | 51项通过 |
| 真实PostgreSQL | 147项通过，包含新增4个console/sandbox用例 |
| 默认/feature all-targets严格clippy、fmt、M0 | 通过 |
| scripts/test_web.py create/restore | 真实HTTP/DB/SSE，两进程通过；没有真实模型请求 |

浏览器实际覆盖：

- 17个侧栏链接逐一点击，四组导航；当前路由、标题焦点、刷新恢复、256/76px收起展开；移动抽屉可访问第一组及底部“系统设置”，Escape与焦点循环保持。
- 17页×1440/1024/768/390/360px共85种浅色布局无整体横向溢出；17页1440px深色布局也检查，无图片/画布/视频角色素材。
- 工作台提交真实当前人格成功（未执行），原文重复schema_version被拒，提案结构通过仍提示来源/预算/权限未验证；前后服务端个体状态完全相同。
- 扩展分类的ArrowRight/End/Home键、未接入市场禁用、能力搜索空状态；模型密钥仅显示未配置，推理测试按钮如实禁用；真实运行记录可进入详情，未执行运行允许无步骤。
- 原鉴权、持久SSE、断线恢复、确认取消、409草稿保留、目标状态写入与重启、资料导入及受限视图导出仍通过。配置编辑改为原文JSON发送，重复字段由服务端拒绝且确认框保持、服务端状态不变，不在浏览器解析时静默覆盖。
- 截图保留17页*-desktop.png、overview五宽度/深色、workbench/models/traces-390.png和恢复的人物引用检查器；已目视检查桌面总览/工作台及手机总览，未将合成测试数据写入生产。

服务端新增测试实测：提供商凭证/端点/自定义模型字符串不泄露，管理读取未经授权被拒；沙盒严格解析/未知字段/大小与零写入；实际fixture驱动的本地工具计时和模型/工具交错排序，原始输入/输出不出现在Trace/Audit，错误码白名单、源撤销后运行不可读取；稳定分页、真实诊断和出生种子不被当前配置改变覆盖。fixture不冒充真实云模型质量验收。

失败与修复：初次native检查漏导入serde_json（修复后通过）；导航测试定位器同时命中品牌和总览（收窄为nav链接后通过，未删除点击验收）；17菜单原先挤出首屏（改菜单独立滚动，保留品牌/身份后通过）；容器前端初次缺少公开decision校验模板（Docker定点COPY后复验，不复制私有eval结果）。

本轮日志/tmp/alia-v3-{database-final,browser-release,components-release,clippy-complete,feature-clippy-complete,docker-final,deploy}.log。密钥写入/切换/推理探测/回退、外部连接器与安装、人格回滚和完整记忆修订删除等仍为未实现，不能以17页可访问当作业务全部通过。

最终部署通过：使用现有四份Compose配置保留数据库、Go tokenizer和本地E5挂载，镜像构建/启动成功并应用0019。health=200、未认证=401、认证state=409/not_initialized；HTML及CSS/JS与最新本地构建摘要一致。没有初始化生产个体或写入合成历史。已目视检查修复后的桌面总览/工作台与390px模型页，完整17页截图在evals/results/web。
