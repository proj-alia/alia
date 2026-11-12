# ALIA Console v4.0 — Operational Console UX

本版**推翻** v1–v3 的「紫色大卡片 / 产品展示型 Dashboard」视觉。以 AstrBot 和 Koishi 的实际控制台为主要 UI/UX 参考（不是照搬它们的 bot 领域模型），做一套更高密度、更少装饰、更偏操作的数字个体管理 WebUI。

## 入口

- `prototype/index.html`：**单文件、无任何外部依赖的可交互 HTML 原型**，双击浏览器打开即可；全部数据明确标记为 Demo。
- `AGENT_START_HERE.md`：开发 Agent 的执行入口。
- `design/REFERENCE_AUDIT.md`：产品间具体的视觉/交互证据，以及取舍。
- `design/UX_SPEC.md`：页面、交互、响应式、状态、无障碍规范。
- `design/tokens.json`：设计变量。
- `engineering/ENGINEERING_HANDOFF.md`：工程落地与验收标准。
- `qa/`：Playwright 测试代码、运行结果、桌面/移动端预览截图。

## 重要边界

- 这是**UI/UX 交互原型**，不是已连接模型、插件注册表、媒介或数据库的真实运行后台。
- 所有变更按钮仅模拟交互；不得在生产实现中借用 demo 内存状态冒充实际服务端响应。
- 不使用角色立绘、动漫插画、头像角色图或没有功能意义的装饰性大型卡片。
- 对 AstrBot/Koishi **借鉴控制台操作模式，而非将 ALIA 的个体身份降格为机器人实例**。

## 推荐评审顺序

1. 扩展管理：已安装 → 选择插件 → 修改未保存提示 → 保存；插件市场 → 安装审查。
2. 配置文件：左侧分区 → 修改配置 → 保存状态 → JSON 预览。
3. 模型服务 / 接入渠道：左侧资源列表、右侧详情与参数。
4. 依赖管理：版本比较表、更新确认。
5. 数据与日志：追踪筛选与运行详情。
6. 移动端：侧栏与实例选择。

## 参考来源

AstrBot 官方 WebUI: https://docs.astrbot.app/use/webui.html

Koishi 官方插件配置与市场指南: https://koishi.chat/zh-CN/manual/usage/market.html

Koishi Console: https://koishi.chat/zh-CN/

上述链接作为**设计参考**，原型所有 UI 代码均重新编写。
