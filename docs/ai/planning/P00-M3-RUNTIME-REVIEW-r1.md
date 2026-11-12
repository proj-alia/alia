# P00-M3-RUNTIME-REVIEW｜Astra 差异复核 r1

输入：[运行接入方案 r1](P00-M3-RUNTIME-PLAN-r1.md)与本轮实际源码。范围仅可信trial评测身份、decision事务内接入、persona来源/删除屏障。复核者P00；技术裁决Root。本稿是原方案作者的架构复核，**不充当非作者代码审查或方案批准**。

执行模型已从本聊天实际 `turn_context` 核对：`gpt-6-astra` / `high`，turn `01a1d0b9-6bbb-7d93-9d07-7a35e74d329c`，记录时间 `2026-11-12T17:06:15.118Z`。没有以模型名称代替证据。

结论：原A→B→C顺序可供Root继续评审，但三个边界需补充下列约束。均为源码与方案检查发现的遗漏／待裁决项，没有运行反例、证明现网漏洞或宣称验收通过。

## 1. 可信评测身份：还需绑定评测权限与完整范围

**依据。** `development.rs::verifier/observation/review_proof`核对真实登记、actor、事件payload、trial及结果；0028注册只存kind与registration_event_id。`Development::accept`检查已记录validation全部通过及案例唯一、context数量，但没有预先固定的必需案例集合／完整结束凭据。现有身份与FK证明“谁记录了结果”，不能单独证明该执行器有资格评价该判据、实际执行了正确版本，或没有省略失败／未运行案例。

**补充差异。** Root须冻结服务端选择的评测计划：候选／trial绑定、执行器和判据版本、允许评价范围、必需case与可证明的context身份、实际执行尝试及完整结束条件。不能让模型通过换context字符串或挑选两个成功case满足跨情境门槛。记录所有已要求的失败、未完成、取消与结果未知，接受前核对覆盖，不能仅检查已提交的成功子集。具体持久化位置由Root决定，本稿不增表或配置。

身份撤销、执行器升级及判据变更也要明确生效规则：旧事实记录保留，旧评测结论能否继续用于接受必须显式判定，不能同UUID静默更换实现。模型语义意见仍不能直接转成measurement成功；模拟场景里的事实不能凭`development.observed`被提升为共同亲历，直接观察只证明测试确实发生及其实际测量。

**最小新增验收。** 冻结计划含两项成功和一项失败／未完成，只提交两项成功时不得接受；同case内容换标签不增加情境覆盖；同UUID切换判据版本、身份失效后，旧完成凭据不能无条件复用。此类工程检查不代替20组成长行为评价。

## 2. decision事务：统一提交还需统一标识、回执和试用副作用

**依据。** `propose_development`自行生成candidate/event、写步骤、advance、outbox及独立幂等回执；`commit_decision`以已结算reservation绑定统一回执。当前decision重放已逐一检查返回对象与生成后审查事件；这些对象不全在作者输入manifest中。原卡“抽in-tx内核”正确，但没有明确发展对象接入这些输出校验的位置。

**补充差异。** 内核只参与调用者事务；候选ID、proposed事件与版本由同一原子提交持久化，统一decision回执能找回原对象。成功后丢响应必须返回原candidate，不产生第二份提案；事务回滚则不能留下独立development回执。提交后重放必须检查本次生成的development/event及实际审查证明的来源，不能只检查原context。内核不得独立commit、推进全局版本、唤醒worker或派发模型；既有费用结算继续独立保留。

Root还须明确trial run是否允许普通decision的memory/goal/action/reply副作用。仅保证“persona未改”仍可能让模拟试用写入真实记忆或对外执行。合成评测与真实情境试用分别标识；需要保留的真实结果保持原权限、来源和审查，模拟内容不得伪装真实经历。无需新建第二个生产个体，但须有明确的提交边界。

**最小新增验收。** proposed已插入后统一提交晚期失败，全局状态/回复/候选/通知一致回滚；成功后新进程同reservation重放返回同candidate；删除新candidate的实际依据后旧统一回执拒绝。模拟trial发出普通记忆／工具意图时，验证Root选定的提交边界，不能只断言persona相等。

## 3. persona来源：manifest中的ID还需强来源图与trial派生闭包

**依据。** `adapters/goals.rs::unavailable`当前只遍历`derivation_edges`并检查objects.blocked；发展adapter的`available/LINKS`另覆盖实际FK及`provenance_removed`，删除模块也有专属关系图。将head ID加到manifest后，仅复用通用unavailable仍不能等同于发展来源完整验证。当前`privacy_development::verify_development_projection_in`验证投影但只返回`()`, 不提供所使用来源集合。

**补充差异。** Root需确定统一的受信任读取结果，将实际persona/当前strategy值、head/revision版本、来源集合及generation绑定到同一快照。读取、预留、派发、提交、缓存重放与公开返回采用同等强度的发展来源检查，覆盖FK仍在而通用边缺失，以及removed=true但blocked=false；不能以普通可用对象计数替代。保留r1“不把identity挂到私人发展来源”的约束。

trial overlay尚未成为accepted persona，其来源还包括候选、trial事件和新证据。实际使用overlay的run、结果及合法持久输出必须带这些依赖；仅带基线persona head会漏掉试用新增的私密来源。拒绝／撤回与隐私删除是不同操作：前者保留有来源历史，后者沿真实依赖清除；新generation不得重新连回已移除的旧head。

**最小新增验收。** 保留development FK、去掉通用边后撤销祖先，预留/派发/旧回执/公开读取仍拒绝；单置removed标记亦拒绝。只撤销trial新增证据时，基线persona可继续使用，但受影响trial结果及派生输出不可读取或提交。删除前已取出的读取快照须通过原generation返回屏障。

## Root待裁决与交付限制

Root先确定：评测计划和身份版本的可信绑定；trial副作用提交规则及统一回执形状；强来源读取／复验接口。再将对应差异并入现有A/B/C实施卡，沿P04/P05非作者审查和P03工程验证，不由P00另派任务。原HTTP接入及在途兴趣／备份任务不在本次复核范围。

本轮只写本复核及P00交接；未修改代码、迁移、权威规范或运行配置，未运行Cargo/PG/Docker、产品模型或已被平台拒绝的协议组合。原方案仍待Root裁决，M5与原Goal均未标完成。
