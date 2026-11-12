# ROOT-PRIVACY-HTTP：在线删除、启动恢复和后台清理

状态：实际服务端闭环已接通并局部验证。不是完整M2/M5、前端验收、受管备份或十四天观察完成。

## 实际改动

- 注册P05-05真实管线；P05五源接受包独立保存，不改其历史证据。
- `Runtime::configure_deletion_journal`在main监听、模型、索引和普通恢复前执行。日志路径来自`ALIA_DELETION_JOURNAL`，私有目录须已由可信部署准备；已有个体不得创建丢失的日志。
- `DeletionJournal::open_existing`使用不带创建标志的打开方式，避免metadata检查后文件丢失仍被悄悄创建。原`open`首次创建机制和日志链校验保留。
- `DELETE /api/v1/memories/{id}`无正文/查询，唯一幂等键、目标记忆强ETag。独立日志fsync及DB阻断提交后才202；只有这个不含正文的实际回执可跨自身隐私代次。
- `GET /api/v1/privacy-operations/{id}`鉴权查询实际进度；`/management.privacy`依据真实日志恢复与共享门就绪，不是前端权限开关。
- 独立后台每批至多4任务、5秒检查并接受唤醒；未知结果先日志重放，普通自主暂停不撤销已受理删除；停机结束循环。
- 可选`compose.privacy.yaml`显式挂载独立持久目录，禁止自动创建宿主路径。基础部署缺日志且无历史任务时禁用删除；任何历史任务都要求原日志才能启动。

## 固定源码及实际验证

最终输入：`evals/results/root-privacy-http-v2/source`，182文件；单父session18454自然终态exit0，没有并发重复作业。

报告：`evals/results/root-privacy-http-v2/checks/report.json`，SHA `aa945d96be12fb55cfd40c1a9cbad871b2256ff43e2d00dd6a8a6addc89f4cf4`。

| 检查 | 实际结果 |
| --- | --- |
| 服务/测试编译 | cargo check --locked，exit0 |
| 共享门普通测试 | 3 passed；其余filtered未计通过 |
| HTTP/启动/后台，实际PG | 5 passed / 0 ignored |
| P05删除管线，实际PG | 13 passed / 0 ignored |
| 来源闭包，实际PG | 9 passed / 0 ignored |
| 独立文件日志 | 10 passed / 0 ignored |
| 严格Clippy | lib、bin及privacy_http，-D warnings，exit0 |
| 固定源与清理 | source_unchanged=true；owned数据库清理passed |

HTTP覆盖：阻断受理而非完成、正文真实清理/独立分支保留、强ETag与重复header/body/query拒绝、同键重放与不同意图冲突、重启、缺失/损坏日志启动关闭、未授权401和quarantine503、实际fsync后DB触发故障回滚→维护重放恢复→同键重放completed，以及无新消息/无模型下后台实际完成与shutdown。模拟故障仅在自己的隔离PG/私有临时日志，不使用生产路径。

v1同范围通过证据保留，尚未包含非创建式open修订，不冒用其结果。首次可选Compose静态检查尝试因系统Python没有yaml模块未运行；改用实际`docker compose config --format json`、全合成环境及`--env-file /dev/null`成功验证合并与create_host_path=false，无daemon调用或部署。

接受包：`evals/results/accepted-slices/root-privacy-http-02`十一源，manifest SHA `95238a35d5187802be5cf860a3eb6d6c502627c14435a0539b2445244e34f7f7`。包含runtime/main/service、journal安全改动、两组测试和可选Compose。依赖P05-05及已有affect/关系模块，不能只拿注册文件而遗漏依赖。

## 后继与限制

P02已收到最终v2固定输入，06交接后返回05做真实HTTP/PG浏览器验收；不接受Mock冒充生产操作。P03-10独立处理SQLx/Docker用例分派，再将固定接受切片组成下一完整候选。P04-09A开发人格候选/试用验证/撤回领域；P05-06开发纯内存完整个人导出。均不等待付费trial，原预算与门槛保持。

尚未执行该切片的完整矩阵、完整前端删除验收、生产部署、受管备份/恢复、模型行为或长期观察。未来增加表、持久导出、发展数据或技能文件，须同步删除闭包/清理规则；已知schema绑定拒绝未知结构，不能绕过它先宣称删除完整。完整Goal继续active。
