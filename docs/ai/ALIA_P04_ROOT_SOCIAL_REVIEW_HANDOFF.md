# P04非作者审查｜ROOT-SOCIAL-WRITES-01/r3

**review_status=changes_requested，当前r3不通过非作者接受门。** Author=Root，reviewer=P04，P05另做隐私交叉。本审查没有修改Root生产文件/原测试/阈值，没有收费调用或重跑整套矩阵。P04-11/r2已自然交付后才开始本审查，11仍等待P05+Root跨审。

## 固定版本与证据验证

被审固定版本：evals/results/root-social-replay-fixed-v1/source，manifest SHA ebc03304cb7fa7b857fc958866b2b9feaeb1905ed3c964f646930708ed217378；作者report SHA 3a0c710b781134cb8ef10c73a92c2b6592313d0ca066f98da4abd9c23d8966fa。基线root-social-context-v3 manifest e8848cc0e08a13a93075589f2a1edcc472155e6b11f4d6b9049ea3c5ff13f92c。独立逐文件验证411/413源集合，作者evidence.json的18项日志/二进制全部存在且SHA一致。作者105普通/27受影响实际PG及Clippy/M0/cleanup/source稳定报告可核对，但不替代本次新边界反例，也不将23默认ignored计通过。

审查delta为19个路径（具体集合可由两manifest复算）：三项proposal/schema、三项target/packet9、两trusted内核重用、commit/receipt、作者body约束与相关tests/M0资料。范围严格schema27；未纳入live interest_context或schema29注册，不把不同版本合并为完整产品通过。

## Findings / 处置

### P04-RSW-r3-F1｜P1，缓存回执仍返回直接blocked的新产物ID

固定src/adapters/decisions.rs:63-68只检查original context.input_manifest及individual/decision event，没有检查缓存receipt自身新生成的appraisal_ids、attitude_ids、relationship_revision_ids、message_id等产物。它们不可能出现在原author input_manifest。

实际独立PG `cached_receipt_must_not_return_newly_created_objects_that_are_directly_blocked`：正常model_step+真实commit成功，移除通用derivation_edges，依次仅将新产物object设blocked（原输入和epoch不变），每次再重放原PreparedDecision。实际四类 **appraisal、attitude、relationship_revision、message** 全部仍返回原完整DecisionReceipt；每次全表指纹不变。0过/该测试1失败，断言行tests/social_write_boundary_regression.rs:251，日志02.log。

影响限于返回被屏蔽产物的opaque IDs/旧状态标识；没有证明正文、凭证泄漏或额外执行。仅依赖“正常删除一般推进epoch”不足以支持当前缺边/实际object阻断的防线要求，作者已对同类original-source场景采用相同独立反例标准。

处置建议：在重放前从实际stored receipt验证所有输出object真实归属/直接可见性，并沿实际FK/来源图复查其祖先，不仅检查original author sources或通用边。实际receipt/schema默认空字段兼容保持；不能清空ID列表或降低断言冒充通过。

### P04-RSW-r3-F2｜P1，作者后生成的grounding凭据缺边时不被回执复查

同一缓存分支在fresh路径的require_decision_review之前返回。原author context不会包含其后生成的实际grounding review event；只经general review→decision边关联时，丢边便漏掉这项真实依据。

实际独立PG `cached_receipt_must_recheck_post_author_grounding_proof_when_edges_are_missing`：正常真实存储的supported审查+commit后，移除通用边，直接block ready.grounding_review_id()对应object，原source/epoch不变。重放仍实际Ok(原DecisionReceipt)；期望StaleState/NotFound，实际该测试1失败，断言行279。全表指纹不变，无重新调用或提交。

处置建议：用实际decision_reviews/decision_commits/作者及审查reservation关联复验原grounding proof、owner/run/隐私及依赖；不允许仅由返回字段或模型bool补认证。已成功回执在合法appraisal后来到期时仍可重放的行为应保持；此问题是blocked真实凭据，不是时间到期。

### P04-RSW-r3-F3｜P2，审查system规则的输出target清单与packet9不一致

固定src/domain/grounding.rs:580/585要求审三类social及autobiography，591却仍把target列为reply/memory/belief/person_assertion四类，漏autobiography/appraisal/attitude/relationship；schema与expected_targets确实支持全部类型。这是实际代码文本的协议冲突，未调用真实模型验证是否导致具体失败，不夸大为已观测语义否决。

处置建议：将输出target说明按ReviewTarget/schema保持完整，最好由同一实际枚举/expected_targets描述，增加非付费请求体一致性断言。无需新增收费trial；涉及生产prompt变更后须绑定新源码再审。

## 其他已核对边界与限制

- 来源/前身：作者schema限实际events/subject/version/predecessor；domain明确<=1 appraisal、<=2 attitude/relationship且topic/subject去重；本目标supported要求齐全声明事件，social旧状态只作比较，不额外授予引用。actual DB内核重查scope、owner、版本及前身；trusted仍扫描整个manifest分类，model仅显式event_ids分类，未以反馈作事实放行。
- 并发/原子：共享write事务与state/lease/global CAS在fresh路径先检查；关系FOR UPDATE和对象version CAS、态度current前身CAS保留；与回复/其他状态/outbox/回执同TX。late失败会回滚状态，但model_step已结算的合成调用账本在前一事务，不在该TX回滚。对这些点静态核查并核对作者现有证据，未重复原84PG/完整矩阵。
- 预算/错误：token allowance/settled request_digest/lease generation及review reservation仍在fresh路径检查，Social错误有明确invalid_social_change反馈；未引入自动付费重试。合法cache本就可跨lease/appraisal过期只读返回，不能用强制再次收费/重执行修复上述隐私问题。
- 兼容：新proposal数组serde default/空时omit，新receipt数组default/空时omit，旧字段/旧回执路径保留；不是production schema29整合评估。不把schema通过或合成审查意见当行为真实。
- opaque PreparedDecision字段pub(crate)、外部仅不可变getter，不把“任意外部caller能替换原context”当已成立攻击；本次两项反例使用同一真实准备对象，无修改opaque内部字段。

## 独立反例运行与精确交接

新增仅tests/social_write_boundary_regression.rs，SHA **ddedf949afa25a14a5e74c1d37de4ac769b9ea4cc5931c6ce6c0cb390b9883da**。fixture辅助从固定Root social_decisions提取，仅复用SyntheticReviewer/真实Store/accounting准备；两个断言与后置阻断由P04独立编写，没有复制作者通过断言来证明自己。

报告：evals/results/p04-root-social-review-r3-20261112-v1/report.json，SHA **f0b2a61f2935a116a405aff3667d203f42a89d7fb75a666219859ea509fca625**；source-manifest SHA **04aba8bb54c1d19a54d65d74e1958a0daf3376d5c273b6aa4cb7a861b3f87ced**。review-conclusion.json绑定author/reviewer/r3/change_requested、三条finding、原Root manifest、反例源及测试二进制副本。原Root 413源一字未改且运行后重新核对。

实际命令：cargo check --locked --test social_write_boundary_regression exit0；cargo test --locked --test social_write_boundary_regression -- --ignored exit101，**0 passed/2 failed/0 ignored/0 filtered**，实际1.60秒；cargo clippy --locked --lib --test social_write_boundary_regression -- -D warnings exit0；rustfmt --check --edition2024该测试 exit0。0网络/真实供应商调用；仅进程内SyntheticReviewer经真实计费存储路径，不能当语义通过。runner预测counter元数据有误且未独立导出总账调用计数，metadata-correction.json明确将其排除为测量值；原报告不覆盖，provider header也按该澄清理解，不能称0合成trait调用。

独占target/p04-social-review-r3-v1与独立schema27 PG，workspace/target/DB资源锁，provision/cleanup数据库管理锁。cleanup/source_unchanged/root_input_still_verified/review_test_unchanged均true，error=null。未运行任何沙盒协议用例或平台拒绝操作，没有重派被拒任务。

下一依赖：Root在自己文件范围修F1/F2和输出协议说明，提交固定新revision/差异与精确证据；P04只针对改动及这两原始负例重审，P05继续隐私交叉。**相关生产源码改变须重新确认，不沿用r3意见**。保留Root已有RED及本次两RED，不改反例标准、不在此代修Root代码、不自行接受或发布。

Root已在同一审查链确认收到两个实际失败反例并承担修复；本审查交付为change_requested，后续新固定版本再复验。Root独立发展的HTTP/r1任务不属于本次schema27/r3审查，不据此通过；P04-11/r2也仍独立等待非作者审查。

## r4 非作者定向复审：approved（2026-11-12）

本节仅批准Root固定schema27/r4的修复增量，保留上方r3不批准及原始RED记录。Owner=Root，reviewer=P04，open_blockers=0；P05隐私交叉、live schema29整合、完整工程与正式行为仍是独立门槛。未改作者源码或反例。实际turn_context：turn_id=01a1d0bf-efc9-72a3-8a2a-b60597e24bd4，model=gpt-6.1-sol，effort=high，reason=已提交回执来源及opaque审查凭据的独立审查。

固定输入root-social-output-replay-fixed-v3：manifest SHA `9f8a97f481cca035c28f821f753acf375954271a1324e4b2e2ec3241f3174096`，作者report SHA `26c96c86c7bd3894dfeee3e1641bc3f67b5b52c116afd117518e05668d826509`。独立核对414个源码摘要、全部日志与封存8个实际执行文件；相对原r3仅decisions.rs、domain/grounding.rs变更，新增反例文件SHA仍为上方原始值，未删除来源文件。作者106普通/17实际PG与前次105/29PG证据分开，不累加为本次独立执行量；v2编译失败继续保留。metadata-correction明确排除model_calls占位，未据其声称零合成调用。

- F1已解决：从实际保存的DecisionReceipt取得所有对象字段，包括旧memory/belief/autobiography及新appraisal/attitude/relationship、goal/action/message；decision_event与提交记录一致，再执行真实objects归属、阻断和祖先可用性校验。原四种新产物直接blocked、缺通用边反例均被拒。
- F2已解决：从实际decision_reviews按author_reservation_id、individual_id、run_id取得审查事件，与原不可变PreparedDecision的grounding_review_id精确比较；该审查对象加入来源复查。原作者后生成审查事件被blocked、缺通用边反例被拒。没有重新派发、清空回执或把缓存重放改成新提交；已提交评价之后到期的合法重放语义未改。fresh路径原预算、lease、计量与事务校验未修改。
- F3已解决：实际RULES输出说明含全部八种target，新增逐schema分支、真实serde反序列化/序列化和RULES交叉测试；本次独立执行1项通过。该测试证明协议一致性，不证明模型自然语言审查质量。

独立证据目录 `evals/results/p04-root-social-review-r4-20261112-v1/`：report SHA `4069265cc3443fb809e60b24146640d1020e28a479e366e7ee5f2939c9267060`；manifest SHA `d683f473c2a37c5fbb61fe4258ce41bd1188f5812cc75a20ab7a721f962839e3`；review-conclusion.json绑定版本/处置/日志。原样两反例实际独立PG **2 passed/0 failed/0 ignored/0 filtered**，cargo test命令exit0；仅新协议单测 **1 passed/83 filtered**，命令exit0。未重复作者完整106普通/17PG或其他矩阵；未运行收费行为、浏览器或平台拒绝的沙盒协议。

使用P04独占target锁复用已终态编译缓存，独立输出、源码与PG容器/卷/端口；实际二进制在释放target锁前另行封存。cleanup/source_unchanged/root_input_still_verified/review_test_unchanged全部true，无网络模型/部署。合成provider总调用数未独立测量，保持unknown。独立执行约30秒，非作者复审在本阶段完成，下一项为已排队的固定发展HTTP/r1审查，不同时开展第二个实现切片。相关源码改变须重新确认本批准范围。
