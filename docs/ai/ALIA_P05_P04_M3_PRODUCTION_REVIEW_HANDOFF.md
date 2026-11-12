# P05 independent review — P04-M3-PRODUCTION-01

**scoped_approved**，仅 schema30 新增来源/FK及隐私部分；author P04，reviewer P05。Root总事务/生产整合仍待其审查，P02权限原RED继续阻断全链，不是M3或成长通过。P05自有 purge consumer、导出和备份适配不在自我批准范围。

固定 `p04-m3-measurements-integrated-20261112-v6/source`，497源 manifest `35dfffa26bc403cf2975b591c0ec06fc9f40442a943642427f1bfd1cc621327d`、report `fc738ca9297613cf3f3b225e349b4ae87c502cc78dae6458b5f87a47d598a3ed`。P05核对全源码及作者封存binary摘要；实际六个P04-owned文件的精确哈希列于 [独立结论](../../evals/results/p05-p04-m3-measurements-review-v2/review-conclusion.json)，SHA `4fb6a34d530fab692b29dfcf4de19e41e556f5a5b140d151c0b4f26b6d494523`。不借不同版本/旧schema29结果批准新writer。

检查了实际 TRIAL_REFERENCES 每分支 owner `$1`、0/5/partial新表判断、request_sources 在预算预留前绑定及完整来源可用性、实际程序/判据与plan/case摘要、原账本/请求/lease恢复、清理私密plan/case/context/result而不删reservation，以及 cached freeze/finalize 回执的当前完整实际来源屏障。development::available 动态合并 trial 实际FK，避免只靠通用派生或专用关联表。

新增两个独立实际 PG 反例：在冻结计划后给实际 attempt.request_sources 保存额外当前属主事件；该事件不在原 plan manifest，通用/专用派生边已删。先证明同一冻结计划、final Reject回执可幂等恢复；只阻断该实际新来源后，原同意图缓存必须拒绝且全表指纹不变。两条实际 exit0，2 passed/0failed/0ignored。此处是明确的持久化故障注入，未冒称真实prebind或供应商调用，验证的是强来源在便捷边缺失时仍生效。

作者原八条 measurement PG也由P05在新独立数据库实际复验，exit0，8 passed/0failed/0ignored；含owner UUID碰撞、不完整schema和原finalize/review反例。严格 Clippy（lib+新独立test target）及格式检查 exit0。自己固定副本及作者源不变，两个独立PG/卷和私有临时目录精确清理成功。新2与原8分两个不可覆盖报告，不冒称首轮全部通过。

证据：[v1原报告](../../evals/results/p05-p04-m3-measurements-review-v1/report.json)、[v2原报告](../../evals/results/p05-p04-m3-measurements-review-v2/report.json)。v1独立新2已过，原作者封存binary没有执行位，实际PermissionError，原8和strict当轮未运行；v2只在自有bin复制原bytes、核摘要后chmod0500，实际执行原8并严格检查，不改作者artifact、不重复新2。原编排器继承的reviewers元数据保留；明确本评审者只有P05，不能据该字段推断P04/Root批准。

P05 PORTABLE-02/r3另外实际消费同P04作者源验证删除/导出/真实隔离恢复，但其作者适配仍待P04/Root，不在本独立结论自批。旧P04-11兴趣revision来源F1未由本schema30 trial审查重新裁决/关闭；P02原权限/context RED仍单列，全部 M1—M5行为/十四日门槛不变。Root共享controller采用、完整工程矩阵、模型质量和正式成长都未在此运行。0生产源码修改、0云模型、0部署。
