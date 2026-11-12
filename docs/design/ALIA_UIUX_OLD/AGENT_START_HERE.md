# ALIA WebUI · Agent 实施总指令（v2 · 无角色形象）

## 0. 任务与输入

你是一名能够实际操作现有仓库的资深前端工程师 / UI 系统架构师 / 测试工程师。目标是将设计包中**可验证的交互 UI/UX**实现到**现有工程**，而非重新构建数字个体引擎。

产品原则：**Entity（数字个体） != Model（模型） != Session（会话） != Channel（媒介） != Tool（工具）**。

UI 视觉原则：紫白色轻量控制台，左侧深紫导航栏，白色内容卡片、紫色交互强调；抽象文字标记代替头像形象。**严禁引入**任何角色立绘、真人/动漫头像、宠物图、背景角色插画。不要使用此前的带角色形象概念图作为 UI 实现依据。不得改造成聊天机器人聊天主屏。

## 1. 必读材料与优先级

1. **实际仓库**：运行脚本、路由、设计系统、已有鉴权、API 和业务约束（决定集成方案）。
2. `design/01-ui-direction.md` 与 `design/02-page-specs.md`：决定应该实现什么。
3. `design/03-interactions.md`：交互与状态，不得省略。
4. `design/tokens.json` + `design/04-design-system.md`：颜色、间距、组件、移动端。
5. `prototype/index.html` + `prototype/assets/*`：界面可见结果与交互参照；**是演示**，不是生产代码。
6. `engineering/01-domain-api.md`：数据契约建议，需与实际后端适配。
7. `engineering/02-implementation-plan.md`、`engineering/03-acceptance.md`：阶段顺序与质量标准。

如果设计与实际后端不一致，优先保护仓库实际鉴权与稳定接口，建立 Adapter，并在风险报告记录差异。不允许伪造数据来掩盖缺失的后端能力。

## 2. 强制执行序列

### M0 · 仓库摸底与集成图
- 列出技术栈、包管理器、入口、路由、现有组件、图标、状态管理、鉴权、数据层、测试和 CI。
- 建立 `docs/ai/ALIA_UI_RECON.md`：资产复用表、页面路由映射、API 对接差距、实施路径。
- 已有 WebUI 则在其中实施，不另建平行 Demo；没有则采用符合仓库约束的前端基础脚手架。

### M1 · 设计系统与应用壳
- 迁移 `tokens.json` 为项目可用的 CSS 变量 / 主题与 TypeScript 常量；定义 Button、Card、Badge、Table/List、Search、Tabs、Dialog、Empty、Skeleton、Toast、Status。
- 实现深紫 Sidebar、Topbar、主内容滚动区、移动端抽屉、面包屑、当前路由状态、深浅主题。
- 无 CDN 依赖的核心 UI；提供 `prefers-reduced-motion` 和键盘焦点。

### M2 · 十个页面视觉与导航
- **全部**落地：总览、身份与人格、记忆档案、认知画像、关系脉络、意图与行动、感知与媒介、授权与治理、事件回溯、运行与设置。
- 与 `02-page-specs.md` 对齐加载、空、错误、禁止访问、未连接、刷新与成功/失败反馈。
- 页面有真实交互逻辑，不允许只做占位或大图背景。

### M3 · 数据契约与 Adapter
- 以 `entities/id` 为主键，把其他数据作为关联资源。
- 使用 `Repository`/API Client 隔离 `Mock` / `Real`；开发模式可演示，生产模式**不能**静默回退到 Mock。
- 完整映射网络失败、无权限、过期、并发冲突、删除/撤销后的状态。

### M4 · 高风险交互与权限
- 审批卡先查看目标、作用域、payload、风险、有效期，再用户确认，服务端成功后更新状态。
- 权限策略、外部写入、暂停自主行动、删除/修订记忆都不得以单击即宣称成功。
- 显示服务端回执/审计记录 ID；防重放、幂等、并发版本核验由后端提供。

### M5 · 完整 UX 测试
- 检查 1440、1024、768、390、360px；无整体横向滚动；不可访问的内容与操作正确提示。
- 测试键盘导航、输入聚焦、Escape、抽屉焦点管理、对话框、筛选、空状态、错误恢复。
- 运行 lint / typecheck / unit / E2E / build；修复失败项。

### M6 · 实施交接
- 更新 `docs/ai/ALIA_UI_STATUS.md`（完成/部分/未完成）、`ALIA_UI_TEST_REPORT.md`（实际命令与结果）、`ALIA_UI_RISKS.md`（后端依赖与安全限制）。
- 交付可运行代码、对照截图、路由表、Adapter 说明、测试报告。不要把模拟功能称为已接通生产。

## 3. 仓库代码组织（仅建议，按实际情况调整）

```text
src/
  app/                        # router, shell, theme
  design-system/              # tokens, primitives, feedback
  features/
    overview/ identity/ memory/ cognition/ relationships/
    agency/ media/ governance/ audit/ settings/
  shared/
    api/                      # typed clients and validation
    repositories/             # interfaces + mock + real
    types/                    # domain types
```

**不可**：所有页面都塞进一个文件；将 persona 与权限合并成 prompt；把模拟标签隐藏；复制概念图的动漫角色图；在 UI 层生成伪造服务端审计回执。

## 4. 每阶段提交格式

- 变更的代码/文件；
- 对应的原型页面和设计条款；
- 运行命令与实际输出；
- 风险/失败/回滚方式；
- 下一阶段依赖。

当已知文件和工程约束足够时，**直接实施**，不反复询问用户。遇到真正不可逆的架构决策才给出具体选项与推荐。
