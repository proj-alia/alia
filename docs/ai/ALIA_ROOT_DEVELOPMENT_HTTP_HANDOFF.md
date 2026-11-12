# ROOT-DEVELOPMENT-HTTP-01/r1 — 真实发展读取与撤回

owner Root；非作者reviewer P04（发展版本/事务）、P05（来源/隐私）；当前review_pending，作者定向工程通过，不是独立批准或M3完成。model=gpt-6.1-sol/effort=high，reason=真实状态撤回、同意图恢复与删除屏障。采用P00-M3-HTTP-01/r2；当前接口精确契约见CONTRACTS的同名章节。

## 行为与范围

新增service/developments模块，注册GET /developments、GET /developments/{id}和POST /developments/{id}/revert；management增加development两bool，表示实际handler支持，不保证当前对象/权限/生命周期通过。GET列表为include_history/limit/has_more有界快照，无cursor/total；默认30、最大100，重复/未知query422。detail强ETag绑定development.version。

POST严格reason-only，拒绝空白/NUL/>1000 Unicode字符、未知/重复JSON与非空query；复用单Idempotency-Key、强If-Match解析，If-Match为development版本。HTTP不先查全局版本/生命周期，也不自组事务或可信manifest，直接使用已交付owner_revert_development；同意图回执由Store先恢复并再查来源，active/paused允许新撤回，archived/terminated拒绝新意图。提交成功200/七字段回执及逐development ETag，真实新revision_id；401/403/404/409/422/503沿现有错误、安全头、no-store及隐私代次中间件。

未开放model development、trial、观察/独立评审生成、Accept、任意人格编辑、export或backup HTTP。新增schema、示例及8个M0负例，1000 Unicode边界另正验；新增实际TCP测试。原unimplemented测试改用仍未接入的exports POST，原断言保留；诊断schema断言随canonical28/29改29，不删除检查。

## 固定输入及实际结果

最终`evals/results/root-development-http-v5/source`由Root固定social27及已归档P04-10/P05-07精确源叠加canonical28/29、development/export领域与最小共享登记、Root HTTP/protocol/test构成。未引入P04-11兴趣新helper/未完成SDK或managed backup；不声称live全部文件均通过。source-manifest.json记录477个实际输入，并包含文档引用的原始失败报告，不改旧记录。

9阶段全部exit0：M0（7schemas/61closed/16examples/86negative/111trace/136edges/9docs/39links/2sources）；定向cargo check；81普通测试（默认9 ignored不计通过）；实际development_http **7**、development_storage **19**、development_privacy **13**、api **20**、privacy_http **5**，合计**64**实际隔离PG/HTTP；严格Clippy。源码/QA helpers稳定、精确owned容器与卷cleanup通过，0云端模型调用/未部署/未创建生产个体；既有API回归含FixtureProvider，未单独采集其总调用数，report.model_calls占位0明确由metadata-correction.json排除为测量值。evidence.json绑定实际日志及执行二进制摘要，run-parent.py保存本轮运行器。

新增7项实际TCP场景：默认空列表/两记录限制/history/detail/ETag及读无副作用；单目标恢复/其他人格及seed/rules/identity不变、全局变化与archived后的原意图重放；auth/origin/闭合JSON/重复header/强版本/query/Unicode边界；旧head竞争/terminated拒绝/paused允许；后段outbox触发异常全表回滚503、移除故障后同键并发只一次提交；缺通用边原来源blocked与removed正交标志都阻断detail/list/缓存回执，实际共享privacy fence返回503；未初始化不创建身份。可信注册/观察记录是显式synthetic工程fixture，状态转换和撤回通过真实Store执行，不是生产验证器或成长语义证据。

## 保留的失败及处置

- v1未执行：预审发现测试Values读取不存在root字段，改为有效Expression变化，不作失败/通过计数。
- v2：M0发现快照遗漏IMPLEMENTATION引用的原FLOW失败报告，编译/PG未启动。v3只补原文件，不放宽链接检查。
- v3：新增测试使用不存在IndividualState.version，编译101，未启动PG；v4改实际state_version。
- v4：编译/81普通通过，额外TCP探测未用现有Docker helper的sudo上下文而超时，PG用例均未执行；owned cleanup通过。
- v5：仅修运行器为既有DOCKER前缀、明确postgres DB；产品源码与v4一致，真实64PG/HTTP通过。未盲目重复外部写操作或改验收断言。

## 下一依赖与限制

P04/P05按固定源码独立审查后才能接受。P02-08可基于此接口接实际浏览器，但有记录的全部六态/五种target/goal-task scope/撤回/丢响应/删除/深浅与手机完整分支仍not_run；完整synthetic记录producer尚需有界QA驱动，不可说生产trial已验证。仅readonly/未就绪浏览器证据不补判新API下页面通过。完整模型认知发展、独立生产评估器、20组ABC、模型切换及十四真实日均未验，目标继续active。

最终manifest SHA `08e0faf8072cf387873c3f36923a8690ac48b0fb2ee282953a575e5b15b4abf9`；report SHA `45be6aede20562841846cbefdf7cda1a89547517d0ddff19d069dc8e87bd9b4c`。

P04已在固定v5/r1复审批准（subject37fdb234f8b4db37db8a52ef1f0c1c8960d32941fc821b25c439fd2339eaa563），独立重跑原七TCP用例，不称七个新反例。Root核对其结论、独立report/manifest和三份证据，root-p04-review-ack.json记录；P05、浏览器及后续persona新源另验，未整体接受。
