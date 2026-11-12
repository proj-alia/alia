# VISIBLE-ALIA-REPLAY-01 / VISIBLE-DEMO-01

Owner P01，展示与原文/来源核验；Root统一调度。状态：只读回放已交付。醒目标记“旧N2固定版本真实模型对话回放，非当前checkout新演示”。公开原文及绑定在 evals/results/visible-demo-01/transcript-existing.json，完整可读版 transcript-existing.txt；verification.json记录逐字核对结果。

三段Alia回复均直接取既有报告/reports/0、1、3的replies/0/text；原固定源码SQL从message.sent按当前run correlation_id读取，worker均Committed。第一轮合成输入在实际evidence材料中直接保存；restore/unknown输入为该精确SHA绑定源码中实际submit_message采用的固定字面量，解析报告未单列保存当轮输入事件正文，此缺失显式标注，未补写或冒充重新读取已清理DB。后台observe无新增用户消息、不伪装聊天回复；实际保存的笔记另附。

原N2 checkpoint 54e4eb000d95f90d36ae92f3d91762da7be797a44876f326d5c1829e254b96e4，报告SHA 008f67d1d3e06a3b9208e24fd721d4c01078c77ee22dc666e86adcd0e52d32fa。模型观测glm-5.3-flash，provider opencode_go；唯一结算reservation共12条，input79688/output4921/合计84609，含失败和后台调用。真实货币费用未知，不将token当账单；不相加重复阶段快照。

原始engine_pass=true、behavior_pass=null、behavior_review=unverified保留；此前独立四阶段正例评审、背景invalid_schema失败及助手式措辞限制保留。不是当前M3/M4通过证据。实际运行纯本地提取/逐字和账本核对命令退出0，无新DB/服务/浏览器/模型或费用；producer原始review_pending未改。M4停止协议保持not_run。模型派发Sol/high，实际运行指令gpt-6.1-sol，未从turn_context独立测得effort。

本轮只读展示完成；新真实演示本任务未运行，后继由Root独立核对用户范围及安全入口，不自动重试收费。
