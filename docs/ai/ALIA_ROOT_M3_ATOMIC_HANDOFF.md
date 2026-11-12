# ROOT-M3-ATOMIC-01/r2：模型发展提案的原子提交

当前r2的Root-owned范围已获P04和P05限定批准；P04作者上游kernel/catalog/FK仍需Root/P05对应审查，不将整条M3标为accepted。r1已被P05独立P1反例要求修改，不能依据旧109PG判其接受。r2固定schema29仅补实际候选→目标强FK并纳入原独立两反例，60实际PG/严格检查通过；下文r1保留为历史，r2精确记录见文末。

Owner Root；非作者reviewers P04（事务/版本）、P05（来源/删除）。状态review_pending，尚非独立接受。开发模型Sol/high用于真实来源、对象版本、计费和事务边界。

## 实际改变

模型的development由任意JSON改为闭合DevelopmentDraft。实际上下文提供五类目标及真实before_version、可选head和精确task/goal作用域；作者请求schema绑定这些值，不允许猜版本或修改身份/权限。提前校验核对支持与反证、来源类别、截止、目标值和无变化；最终Store仍按真实数据库重新核验。

依据审查增加第九个Development目标，packet为target-citations/11；支持意见必须覆盖完整支持及反证。候选不是当前人格、已执行试用或改善结果。审核后，原commit_decision事务调用P04的propose_in_tx：候选、回复、外层回执、来源与通知一起提交，仅一次全局版本推进。模型不能直接开始试用、写评测或接受变化；当前人格、出生种子和权限不随此提案改变。旧回执检查发展对象及实际来源，不能通过删除通用边绕过。

## 固定版本与实际测试

固定源`evals/results/root-m3-atomic-v4/source`共489文件，沿Root request-key基线，显式叠加P04固定事务内核及只读domain DTO。**未包含**measurement writer/schema30、trial worker、SDK bridge或备份manager。Root修改22路径与上游作者输入分别记录，P04不能自批自己的内核。

- v4共19阶段退出0：M0、compile、16组实际PG/HTTP共109项、严格Clippy。源码与执行文件摘要复验、临时数据库/卷清理均通过；实际执行二进制在target锁内封存。
- v3的124普通测试实际通过，112 ignored没有算通过；随后新PG负例因预期错误类别不准确而失败，3过/1失败，完整报告保留。v4相对v3仅`tests/development_decisions.rs`错误类别断言改变，所有生产字节相同；原124普通结果有明确复用证据，未重复运行也不把v3整体改为通过。
- v1/v2分别保留新fixture调用签名/测试助手缺失及旧reviewer exhaustive match编译失败。v3补齐新枚举分支，没有删改原旧断言。原P04人格来源两反例逐字保持且实际通过。
- 新4项PG检查：proposed与reply单次原子提交/重启读取/同意图恢复；晚期outbox故障全表回滚及并发恢复仅一次；缺通用边的候选来源失效阻止旧回执；伪catalog预留零写入和不支持审查拒绝但保留实际已结算费用。

manifest SHA `08274e77fa9c75596df6affb16a8a84d93972dacd8adcf0bdf45c6919b0ca94b`；report SHA `312c0deb92dc6477beb68741131feb7cd58ce2ab6a6c87518919470de34144ac`；evidence SHA `0d3de1af826d694ce5be8a4d34989489c11970ebc3f2494d0c28a2fd14bf5eae`；task subject `1e79496e21f2153500cd2bdbd44bc29e4ce02aed0e150fec3ce9e853cd13944c`。源清单中继承的overlays文字不是v4差异清单，实际v3→v4差异在evidence中按字节复验，不能按继承标签推断修改。

0云端调用，合成provider总数未独立汇总，保持null。没有完整checkout工程回归、此候选全记录UI、真实模型M3效果、正式60×3/20组成长对照或十四真实日证据，不据工程夹具判断成长。

## 下一项

Root接共享reserve/dispatch/commit及worker的真实evaluation_run隔离；P02负责受控context与driver，P04负责唯一plan/attempt/observation写入，P05负责新表来源/恢复链。日常路径必须依据实际评估run映射拒绝降级，即使调用方移除了trial_binding；评估失败不能修改原目标的等待原因或no_progress计数。完整试用/独立测量/接受仍必须真正接通，不以此proposed-only切片作为M3完成。

## r2：补齐P05独立发现的候选目标引用

P05-M3-ATOMIC-F1实际0过/2失败：首次候选在事务内创建的新target不在原作者manifest中；缺通用边后，仅target.provenance_removed或仅target object blocked仍返回旧decision receipt中的候选ID。Root已读取原两反例及独立结论，保留其P1/changes_requested。此问题是派生ID失效遗漏，不夸大为已观察到明文泄漏。

P04唯一owner在live `development.rs`的LINKS实际增加`UNION SELECT id,target_id FROM developments WHERE individual_id=$1`，Root固定schema29只采用此一行。原removed/object阻断、created-event、review、完整来源校验不变。P05独立反例逐字保留，SHA `29268af66ec7af3772d811b7bf70f07ce0aa9879397b98036c991bd4233ab506`，未改其预期。

最终`evals/results/root-m3-atomic-v6/source`490源，与r1的准确差异只有该强FK及独立反例文件。10阶段全退出0：compile/M0/strict及60实际PG（Root4、独立P052、context10、development privacy13、HTTP7、privacy HTTP5、storage19）。执行文件在锁内封存，完整源和产物摘要复验、owned清理均通过。没有重复124普通测试，也不把它们计为r2实际执行；没有引入当前schema30 writer/driver/UI/SDK。新v5准备阶段的全文件substring断言错误在源码变更/测试/DB开始之前拒绝，改为精确LINKS常量检查后用新v6运行，原失败保留。

manifest `44a9b2d24552f9065eb396fa022a68edd65b4484359fc4ab700079acd5a9c9be`；report `b10a3ce3015730bc45034ba55085b92346ffacafdec78e2c22ae244a3e09b595`；evidence `a325c8518507ea62c4b8444b803fa94807a9c7ac1c464432ff48615b94eee6e4`；subject `4ce0d065424ac8bbb454e5d989d121779e7db472dfb0e57aedcc5aa8debce17d`。r2接口绑定完整固定源码manifest SHA；所有者映射区分Root原实现、P04共享FK修正和P05原反例。P04审Root部分，P05重审来源边界，作者不自批。0云端调用；完整工程、行为及十四日仍未验。

P05已独立运行原4+未改两反例，共6实际PG通过，限定批准此schema29 r2；Root核对490源、7个封存二进制、报告摘要与两份实际日志，ack在`evals/results/root-peer-review-acks-20261112-v1/ack.json`。P1 F1仅在该固定r2关闭。P04后续限定批准Root-owned 21个路径（以实际root_overlays为准；前文“22路径”为旧概括误差），原子外层事务/类型/来源/审查覆盖无新发现；该独立审查未新跑PG，明确排除P04自己写的kernel/catalog/FK。Root已核对其完整路径集合与固定manifest/report，结论SHA`21da3ca70d8de9a8b8d35ab80a2ae74f64b02386e02d2f94974ad2988c05fb2e`，路径`evals/results/p04-root-m3-atomic-review-r2/review-conclusion.json`。上游非作者审查仍保留，不批准schema30/M4/成长或发布。
