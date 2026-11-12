# ROOT-HISTORY-03：模型形成与修订个人历史解释

状态：实际端到端工程路径已接通并通过定向验证；语义意见来自合成provider，未进行真实模型评测、成长对照、部署或长期观察。

## 可运行结果

模型可提出最多两条不同topic的`autobiography_changes`。字段是topic、title、interpretation、实际event_ids、带完整依据的belief_ids、当前同主题supersedes。旧提案可省略该字段；人格development仍未在此通道开放。

当前信念即使没有任何自传记录，也会以完整前身/支持/反证加入有限历史背景。来源包遵守原96KiB预算、64信念和1024来源上限，超限整包省略；旧立场明确superseded，不冒充当前认识。原始事件不被解释改写。

审查packet为target-citations/7，每个自传目标必须覆盖所有显式引用，信念展开全部历史状态与反证，旧解释携带真实依据。声明解释不证明世界事实。最多18目标、41显式引用，原响应字节/循环调用/token预算不变。

可信内部记录和模型路径共用事务内校验/插入helper。模型显式来源不可是审查反馈/虚构背景，但未引用的审查通知作为因果manifest不会污染整个后续写入。模型所写自传、信念、记忆、目标和回复在同一事务发布；回执增加可省略autobiography_ids。晚期发布故障全回滚，同一准备结果重放不重复历史，拒绝审查已消耗的实际测试用量不被退款。

## 实际验证与失败保留

最终输入`evals/results/root-autobiography-write-v3/source`固定402文件；唯一父session59074自然结束0，15阶段。报告`checks/report.json` SHA `2b9301f6fcbde3e777b5bacb0e004a7b70378741a0e72900e9a02b48e4a7c8b6`。

- 普通Rust：95 passed；15 ignored没有计通过。
- 实际PG/HTTP：79 passed / 0 ignored，覆盖新自传写入5、既有存储6、历史上下文5、信念写入4/上下文4、审查9、在线删除5、隐私3、调度14、持久化17、事件7。
- 新五组验证首次写入/修订/重启/幂等、history-only被拒且作者与独立审查都记账、晚期outbox失败时自传/信念/记忆/回复整体回滚、首次自传引用完整信念前身/反证、来源撤销后不能提交且账本保留。
- M0、严格Clippy、固定源码不变、自有PG/volume清理全部通过。未执行完整候选矩阵。

v1在共享target中cargo check通过，但普通测试随后链接的类型缺失已保存新字段/variant，尚未进入PG。原编译日志保留；不能把源码哈希本身当成可执行正确性的充分证明。v2使用本包dev/test debug=1区分产物后普通95及20PG通过，遇到旧belief_context“前身完全不得进入manifest”断言失败。新增历史背景需要前身出现，但必须明确历史状态；因此v3保留该用例，增加“old不在current、在historical时为superseded、unrelated仍不进入”的断言。不是删除用例或省略反证。v1/v2原报告不覆盖，v3不拼接其局部通过。

接受包`evals/results/accepted-slices/root-history-03`十五源，manifest SHA `60138982a7a8dd452bfb121f890ef32742281e1ee7ded90093d6f2b27fba571a`。共享schema、示例、检查器、fixture枚举匹配与协议版本断言同步保存。后继完整候选使用独占target并记录实际二进制来源，不能沿用不明共享rmeta/rlib。

## 同批独立SSE隐私修复

P01旧源1过2败确认已建连旧队列在删除后仍发目标opaque UUID，或在结果未知门关闭后继续帧；没有证明正文/凭证泄漏。root补充每次出队/等待后的ready/generation检查及notifications RR两端门。P01保留原三测试字节，再加恢复后旧流不复活/新流正常场景，独占首次新target实际4过，Clippy/源码/精确清理通过。

独立接受包`root-privacy-stream-01`三源manifest `f0df87f89957df500033eaa3a6b66c99d6dee1f10fc9a9bc1d78e4cbe0cb8f1f`；报告`evals/results/p01-privacy-review01-v2/report.json` SHA `a47d48ad0d97ed4a95897e80f1eff4ee212db0760c3d0a9225cefb003f84c6f6`。不是用旧七项events测试替代这个新反例证据。P02-07继续真实浏览器清屏/重连验收。

## 下一步与明确未完成

root继续情绪/关系提案、认知上下文及原子写入，接稳定兴趣与M3发展持久化；P04-09B已有独立实际PG/私有0028草案任务，canonical迁移/模型/路由仍归root。P05-06开发完整个人内存导出，新增持久结构前必须同步隐私规则。P03维护测试分派与隔离验证，再组装后继完整候选。

此切片不证明模型确实理解历史、人格已经成长、完整M2/M3/M5通过，也不启动付费trial或十四天观察。原完整Goal保持active，全部原门槛不降低。
