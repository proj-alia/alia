# ALIA Console v4 测试报告

本报告仅覆盖实际代码与执行结果。原型复测另见[审计](ALIA_CONSOLE_AUDIT.md)，不能替代产品验收。

最新补充见文末“类型化来源与审查事务语义切片”。以下首轮v4记录保留其原有测试范围与日期。

## 已运行

- 前端 `npm --prefix web run build`：TypeScript 严格检查和 Vite 构建通过。
- `npm --prefix web test`：SSE 解析/同步与组件语义、13 个唯一主路由、旧链接映射、资源选择语义检查通过。
- `cargo test --locked --workspace`：50 项普通 Rust 测试通过；147 项数据库用例在该命令中为 ignored，另由 `python3 scripts/test_database.py` 实际执行并全部通过。
- 首轮真实 HTTP/隔离 PostgreSQL 浏览器验收：create/restore 两阶段通过，覆盖配置保存、冲突保留、重复字段拒绝、未保存菜单/直接 hash/浏览器前后退、SSE、目标操作、静态沙盒、导出、主题和重启连续性。
- 六种宽度（360/390/768/1080/1280/1440）×13 主页面无整页溢出，配置/资源输入控件横向边界检查通过；另覆盖13页深色桌面布局。

最终补充回归 create/restore 均通过：11个旧入口映射、移动资源选择、40×40px按钮、手机配置验证/取消、数值输入清空、空 JSON 草稿与导航保护。默认及 local-embeddings 两种模式的严格 clippy、fmt、M0与文档链接检查通过。Docker 镜像构建并更新本机服务；`python3 scripts/check_deployment.py` 只读检查通过：health=200、未授权401、已授权409/not_initialized，HTML/JS/CSS与本机构建一致。没有初始化生产个体。

## 证据与边界

浏览器产物：`evals/results/web-v4/`（忽略目录），含桌面13页、移动端配置/扩展/模型/沙盒/数据、主题、记忆来源、重启交流截图及分阶段报告。已检查概览、扩展与配置桌面及扩展/配置/模型手机截图。

第一轮失败属于测试等待条件：新标题在初始化前已存在，导致过早读取 `/state` 得到409。改为等待实际个体名称后重新运行，两阶段通过。失败日志保留，没有修改服务端初始化结果。

补充失败与修正：`browser-final.log` 检出 Skills/指令标签宽度不足40px，调整最小宽度后通过；`browser-final2.log` 在新增操作后遭遇真实409并保留草稿，追溯为之前冲突消息的前台运行尚未终止，正向保存测试改为等候该运行实际终态和SSE快照，未放松版本检查。严格clippy指出进行中检索代码的嵌套条件，按等价条件折叠修正。

日志：`/tmp/alia-v4-browser1.log`（初始等待失败）、`/tmp/alia-v4-browser2.log`（首轮通过）、`/tmp/alia-v4-browser-final.log`（触摸失败）、`/tmp/alia-v4-browser-release.log`（修正后通过）、`/tmp/alia-v4-browser-final2.log`（并发冲突）、`/tmp/alia-v4-browser-final3.log`（最新最终通过）、`/tmp/alia-v4-web-final2.log`、`/tmp/alia-v4-components-final.log`、`/tmp/alia-v4-rust-tests.log`、`/tmp/alia-v4-database.log`、`/tmp/alia-v4-clippy-final.log`、`/tmp/alia-v4-feature-clippy-final.log`、`/tmp/alia-v4-docker-final2.log`、`/tmp/alia-v4-deploy-final.log`、`/tmp/alia-v4-deployment-check.log`。测试不使用网络 route mock，不注入生产历史，不发送真实模型请求。截图中的目标、消息及 owner 人物断言属于明确的隔离验收资料。

未运行的全站无障碍评估、外部插件/模型管理业务和长期行为验收不在通过范围。当前发布状态：v4 页面框架与既有真实功能已上线本机服务；完整v4资源业务仍按缺口清单推进。

最新浏览器报告UTC：`2026-11-12T04:19:25.920Z`。产物来自随机隔离实例；测试报告的页面数量不表示插件安装、持久审批、模型执行沙盒或生命周期愿景已经完成。

## 姓名定向读取（2026-11-12 UTC）

- WebUI新增精确姓名或别名输入，与已加载人物UUID互斥，显示服务端实际定位方式；同名/未知不显示旧成功条目或伪造零结果成功。
- `evals/results/person-query-web-v2/` create/restore两阶段通过，实际HTTP与隔离PostgreSQL，无模型请求。既有13主入口、11别名、六宽度/深色、配置/409/未保存/鉴权/SSE/恢复流程保留；新增姓名→UUID实际结果、未知较长姓名404及空旧结果、零状态变化检查。已查看`memory-name-1440.png`及`memory-name-390.png`。
- 首轮`person-query-web-v1`在配置保存后只等编辑按钮出现，读到尚未刷新的React值而失败；改为等待确认流程真正结束并核对服务端值。保存刷新期间编辑按钮禁用，没有忽略写入失败或版本冲突。失败日志与截图保留；当前通过日志`/tmp/alia-person-query-browser-v2.log`。
- 前端build/TypeScript与组件/SSE通过；后端51普通Rust、160项完整DB及1项目录边界用例通过，默认/feature严格clippy和M0通过。普通cargo test中161个ignored未被计作数据库执行。
- 独立真实E5/收费模型的两轮六场景工程通过，但人工行为复核仍失败（未给定属性与事实引用覆盖），见[检索报告](../MEMORY_RETRIEVAL.md)。本浏览器报告不将这些模型缺陷、记忆纠错删除或长期愿景计作完成。
- 本机开发镜像已更新，`/tmp/alia-person-query-deployment-check.log`确认health200、未授权401、授权409/not_initialized与HTML/JS/CSS摘要一致，没有写入或初始化生产个体。完整产品及模型行为仍未发布验收。

## 内容依据审查切片（2026-11-12 UTC）

- 增加decision_grounding_failed中文反馈；v4布局、路由和实际服务适配保留。前端构建及组件/SSE检查通过。
- `evals/results/grounding-web-v2/` create/restore通过，实际HTTP、随机隔离PostgreSQL和SSE，无收费模型请求；13主入口、旧别名、六宽度/深色、配置保护、鉴权与姓名读取继续通过。日志`/tmp/alia-grounding-browser-final.log`。
- 当前后端58项普通Rust、169项独立执行的DB通过。真实模型的小组审查用例通过，但联合生命周期v1—v5均在first-study失败，见[内容审查记录](../CONTENT_GROUNDING.md)。浏览器成功不覆盖该失败。
- 本切片新运行时没有部署；此前本机v4部署与初始化边界保持。未完成的目标来源表达与真实生命周期仍是发布前工作，不将小组通过或页面错误反馈视为完整验收。

## 类型化来源与审查事务语义切片（2026-11-12 UTC）

- 后端新目标允许逐项类型化的事件/兴趣共同来源，迁移0021保持历史；审查协议修复复用作者候选，原子提交约束区分“记住/建目标”与外部执行成功。UI路由、资源架构及真实适配未替换。
- 61项普通Rust、173项独立真实PostgreSQL、默认及local-embeddings严格clippy、M0（5 schema/47闭合对象/9示例/52反例）、前端构建及组件/SSE检查通过。事务故障测试证明最终通知失败时记忆/目标/回复不部分可见；该工程性质不证明模型语义。
- `evals/results/typed-origins-web-v1/create-report.json`（09:09:39Z）与`restore-report.json`（09:09:41Z）均通过，真实HTTP/随机隔离PostgreSQL，无模型请求；13主入口、11别名、六宽度、13页深色、配置/409/严格JSON/草稿离开保护、鉴权/断线、SSE及跨进程恢复均覆盖。已查看`config-desktop.png`、`memory-person-390.png`。日志`/tmp/alia-typed-origins-browser.log`。
- 真实生命周期typed-origins-v4取得首次阅读、重启例外、自主笔记及原目标完成证据，但unknown回答因未正确引用的历史概括被审查阻止，整轮失败。另正常作者加九项预写审查探针在atomic-v4通过，不替代联合回归、180次行为或M2—M5；具体失败、用量及来源复核见[内容依据报告](../CONTENT_GROUNDING.md)。
- 本切片没有部署新运行时，没有初始化生产个体。此前本机v4部署维持，十四天真实观察尚未开始；完整控制台业务和长期个体目标仍未完成。

## P02：运行/审计查询与可见页跟随（2026-11-12 UTC）

- 专项交付[ALIA_P02_HANDOFF](ALIA_P02_HANDOFF.md)已READY。主线逐字节核对其中六项源码/测试及两个报告SHA256，均一致；另查看`traces-1440.png`、`traces-390.png`。这属于集成前证据复核，不是主线重新执行完整套件。
- `/tmp/alia-p02-local-final-v2.log`：4项真实TCP HTTP/PostgreSQL测试全部通过，无ignored；专属严格Clippy通过。覆盖组合条件、UTC半开端点、稳定分页/游标绑定与旧版兼容、字面通配字符、Unicode大写点I、非法/重复参数、鉴权/同源/no-store、递归来源阻断及隐私搜索边界。Unicode首轮真实失败保留，修正数据库与Rust大小写规则分歧后通过。
- `evals/results/p02-data-workspace-v6/report.json`（09:40:40Z—09:41:11Z）及`browser/report.json`：12组检查通过，cleanup=true；72条隔离合成终态运行，真实HTTP/SQL/浏览器，无模型预留。组合搜索与详情脱离筛选提示、历史分页关闭跟随、断线/重试、退出清理及390/1440布局均覆盖。慢SQL下列表与详情各自最大并发1。document.hidden使用受控覆盖来触发真实浏览器定时器/HTTP逻辑，不冒称操作系统真实后台切换。
- v1—v3环境/定位/重复请求失败及v5后Unicode失败保留，不计为通过。v6包含P03可复用的显式外部测试模式；完整冻结候选工程矩阵仍由P03执行。未部署该切片，未初始化生产个体，未改变个体M1—M5或十四天观察状态。

## N1固定整合候选：完整工程未通过（2026-11-12 UTC）

P03在278文件不可变快照`segmented-console-20261112-n1`运行完整工程矩阵，最终报告`evals/results/p03/engineering-segmented-console-20261112-n1/report.json`保留。默认/feature构建、普通测试、严格Clippy、前端build/test、177项实际DB/HTTP和P02专属浏览器通过；源码和执行副本最终哈希校验、两库清理通过，没有模型调用。

通用浏览器create在`smoke.mjs:135`失败：断线状态文字在banner和数据详情各出现一次，旧全页文本定位触发strict mode violation。该失败不证明产品断线逻辑错误，也不能算通过；create后半段及restore没有取得完整证据。完整工程结果为false，原有UI截图和P02专项通过不抵消此项。下一候选将限定实际banner状态定位，并保留断线同步时间不变和恢复断言，不对N1改源码或覆盖重跑。当前没有发布本候选。
