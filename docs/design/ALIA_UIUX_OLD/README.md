# 彼我 / ALIA · UI/UX 工程交付包 v2.0

**交付日期：2026-11-11**  · **主题：紫白色专业管理控制台** · **无角色形象版本**

本包是用于 **Codex / Claude Code / DeepSeek Harness / 其他编码 Agent** 的**设计—实施交付源**，不是单张 UI 图。目标是让 Agent 不必猜测布局、状态、交互和接口边界，就可以在真实仓库中实现 WebUI。

## 直接使用（只需要做这三件事）

1. 将本压缩包解压到待开发仓库的 `docs/design/alia/`（或确保 Agent 可以访问解压后的完整目录）。
2. **先打开** `prototype/index.html` 评审界面，导航切换、搜索/详情、审批确认、策略、主题、移动端抽屉都是**可操作的模拟交互**。
3. 把 `PROMPT_FOR_AGENT.txt` 的内容复制给 Agent。Agent 必须先读取 `AGENT_START_HERE.md`，在现有仓库中实施，不要将原型 HTML 当作生产代码直接部署。

## 文件内容

```text
ALIA_UIUX_NoCharacter_v2.0/
├── README.md
├── PROMPT_FOR_AGENT.txt            # 最短的可复制执行提示词
├── AGENT_START_HERE.md            # Agent 总施工指令（权威入口）
├── prototype/
│   ├── index.html                 # 独立打开的交互原型，无安装依赖
│   ├── assets/
│   │   ├── base.css               # 原型基础样式
│   │   ├── theme.css              # v2 紫白界面/深紫侧栏，附深色变体
│   │   └── app.js                 # 10 页面交互、状态与 MOCK 数据
│   └── screens/                  # 实际浏览器渲染截图
├── design/
│   ├── tokens.json                # 机器可读的视觉变量
│   ├── 01-ui-direction.md         # 概念与 UI/UX 原则
│   ├── 02-page-specs.md           # 10 页面功能与状态规格
│   ├── 03-interactions.md         # 交互、反馈、风险流
│   ├── 04-design-system.md        # 组件 / 视觉 / 响应式
│   └── 05-component-inventory.md  # 可复用组件清单
├── engineering/
│   ├── 01-domain-api.md           # 前端 DTO/Repository 契约
│   ├── 02-implementation-plan.md  # M0→M6 实施计划
│   └── 03-acceptance.md           # DoD 与全页面验收条件
└── qa/
    └── smoke-test.json           # 已执行的设计原型交互检查结果
```

## 设计立场

**管理的是同一个数字个体，不是某个聊天窗口。** 模型、媒介、记忆、人格版本、工具调用各有独立生命周期，UI 将它们置于同一可观察的身份锚点之下。

**这版明确没有**：人物立绘、头像肖像、动漫背景、宠物插画、形象素材生成、虚构“意识数值”。左侧图形与 A-01 是**抽象字母标记**，仅用于识别系统中的实体，并非角色形象。

**原型数据全部是 MOCK。** 点击“批准”、“暂停”、调整策略后能看到模拟状态变化，但不会执行真实操作、访问模型或生成服务端回执。生产实现必须以服务器确认为准。原型不是可上线的安全实现。

## 前端视觉规范速览

- 默认：浅色主画布 `#F7F5FC`，深紫侧栏 `#241A40`，白色卡片 `#FFFFFF`，主强调 `#8C67DF`。
- 页面：左侧导航 + 顶部状态栏 + 主内容区；宽屏四指标卡与双栏管理区，小屏转抽屉与单栏。
- 字体：系统无衬线、中文优先。图标：原型内联 SVG（不依赖远程字体/CDN）。
- 行为：除普通查看外，带外部副作用的操作必须经过确认、授权与服务端回执。
- 深色模式：可切换，属于**显示偏好**，不修改 ALIA 的运行时状态。

## 与你的仓库对接

- **已有 React/Vue/Nuxt 等：** 复用已有框架，按页面/组件/数据层迁移设计；不移植原型的单文件 JS 状态机。
- **没有 WebUI：** 建议 React + TypeScript + Vite，路由、组件、仓储、API Client 分层；技术栈应以仓库现实约束为准。
- **后端 API 未完成：** 明确声明 Mock/Disconnected；通过 Repository 接口隔离模拟数据。**生产运行不得静默回退 Mock**。
- **已有账号/权限：** 以服务端鉴权、审计和审批状态为准；前端不能自授权限。

## 原型运行

双击 `prototype/index.html` 即可查看；也可在此目录下运行：

```bash
python -m http.server 8000
# 打开 http://localhost:8000/prototype/
```

不依赖 Node 安装、API Key 或后端。脚本仅用于**静态交互演示**，不能作为真实 Agent Runtime。
