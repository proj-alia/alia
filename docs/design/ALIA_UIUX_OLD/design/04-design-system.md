# 设计系统 v2 · Purple Observatory

**权威机器源：** [`tokens.json`](./tokens.json)。所有前端实现优先用变量而非硬编码颜色；原型 v2 的 `prototype/assets/theme.css` 是实际展示参考。

## 视觉变量

| Token | Light | Dark | 用法 |
|---|---|---|---|
| canvas | `#F7F5FC` | `#110E1C` | 页面背景 |
| surface | `#FFFFFF` | `#1B1629` | 卡片、弹窗 |
| sidebar | `#241A40` | `#140F22` | 一级导航 |
| border | `#E8E3F1` | `#383047` | 分隔与输入边界 |
| textPrimary | `#211C32` | `#F6F1FF` | 标题正文 |
| textSecondary | `#6E667F` | `#B9ADC9` | 提示信息 |
| accent | `#8C67DF` | `#C5A8FF` | 选择/强调 |
| accentSoft | `#F0E9FF` | `#2F2447` | 焦点衬底 |
| success | `#22856F` | `#77D6BD` | 成功状态 |
| warning | `#A56B18` | `#EAC380` | 风险待审 |
| danger | `#C34F70` | `#F5A5B9` | 高风险/失败 |

## 版式

- 侧栏 256px；紧凑侧栏 76px；移动端侧栏是 256px 抽屉。
- 顶栏 72px，手机 60px。内容最大宽 1630px，桌面边距 38px，手机 15–17px。
- 四列 KPI；宽度 <=820px 自动两列；明细双栏在小屏改成单栏。
- 卡片圆角 18px、主 banner 22px、输入/按钮 10–11px；不为每个组件使用不同装饰。
- 主标题 30px，卡片标题 14–15px，正文 13–14px，标签 10–11px；数字需要展示单位。
- 留白：`[4,8,12,16,20,24,32,40,48,64]px`；卡片间隔 14–18px。

## 颜色不仅是视觉

- 紫：选中页、主操作、当前聚焦，不用于标识安全结果。
- 绿：明确的连接成功/服务端成功。
- 黄：待确认或可处理的风险。
- 红：失败、拒绝、破坏性操作。
- 灰：未知/离线/无相关数据。

## 控件规范

- **AppShell**：Sidebar + Topbar + Main + Overlay + ModalHost + ToastRegion。
- **Button**：primary / secondary / quiet / danger；图标按钮必须拥有 `aria-label`。
- **Card**：header（title/subtitle/action）+ body；交互卡需可识别点击区域。
- **Badge/Status**：状态的名称必须随颜色一起呈现，不单靠色彩区分。
- **SearchField**：清楚 placeholder；过滤变化立即反馈；可清除。
- **Tabs**：正确 aria role + 选中状态；键盘可操作。
- **List+Inspector**：左侧结果列表、右侧详情，空/选中/加载状态明确。
- **ApprovalRequest**：风险、目标、范围、有效期、详情、批准/拒绝、结果反馈。
- **Dialog**：标题、简短影响摘要、主次按钮、关闭方式、焦点回收。
- **PermissionSelect**：auto / approve / deny；修改前后差异明显。
- **AuditEvent**：时间、actor、resourceId、traceId + 详情展开。

## 响应式

- `>1080`：全侧栏 + 4 列 KPI + 双列卡片。
- `821..1080`：缩窄导航栏 + 内容自动重排。
- `601..820`：2 列 KPI；主内容单列；不强行挤压右侧 Inspector。
- `<=600`：侧栏变模态抽屉；KPI 2 列；工具按钮可换行，最低触控区域目标 40px。

## 无角色形象约束

**不可出现**角色图片、人物剪影、头像照片、动漫插图、虚拟形象动画、角色主题背景。允许 Logo 几何图形、SVG 功能图标、纯文字标记、真实数据图表。不要从此前的角色概念图提取 UI 元素或加入产品代码。

## 无障碍

- 文本与背景以 WCAG AA 作为验收目标，尤其是紫色小字/浅紫卡片。
- 按钮、Tabs 和下拉框提供键盘操作与可见焦点态。
- 弹框需处理 focus trap、Escape、恢复焦点；移动端导航需保证背景不可聚焦。
- 尊重 `prefers-reduced-motion`；不依赖装饰动画传达系统状态。
