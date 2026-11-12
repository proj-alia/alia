# ALIA-P02-09｜状态投影失败后的私人内容清屏 r2

Owner **P02**；非作者 **P03工程/P01来源**；Root统一接口与登记。流程 **review_pending**，作者定向工程检查通过，独立结论均pending/null，不自批accepted。实际turn_context model=`gpt-6.1-sol`、effort=`high`、turn ID=`01a1d0de-088d-7233-bd3c-6cbf6d32a151`，reason=异步状态与来源撤销UI清屏。08原Goal/历史不重置、不complete；08十源已固定在 `evals/results/p02-08-frozen-r5/` 且本轮最终哈希仍全部一致，原80/5及完整有记录not_run保持原样。

## 先复现，再修改

固定Root persona-sources-v3后端manifest `3f8b02c52333d9884791c529af480a5d0a5b13abdfd71a759779e56b6bb7e36a`，叠加已接受07 web、冻结08十源、09独占controller/测试，最终506源。私有空库真实initialize，autonomy=false；身份页合法人格及认知页持久目标先实际显示。测试直接破坏private DB的persona投影一致性后，真实/state与/persona均422/invalid_request且不返回persona/current，浏览器身份与认知仍保留旧正文；原RED **exit1** 和目视截图在 `evals/results/p02-09-refresh-red-v1/`，未覆盖。此为不合法持久投影工程故障注入，不是伪造HTTP成功或生产人格变化。

## 最小修改与约束

唯一产品改动 `web/src/shared/useIndividual.ts`：已发布快照用ref标记；当前刷新任一投影失败时退休旧读代次、取消旧请求并撤下state/消息/目标/记忆/operation/receipt/私人草稿和成功同步时间。App现有state门自然卸载私人子视图，无需改App或其他guard。身份鉴权失败仍完整退出。已为空的EOF恢复代次不重复退休，避免取消既有重连循环。只有此次失败读取保留原未知写意图与运行配置草稿/基线，禁止自动重发或替换key；原EOF/offline/退出的处理保持其既有分支。迟到请求仍受代次与serial屏障约束。

| 当前源 | SHA256 |
| --- | --- |
| web/src/shared/useIndividual.ts | `dfb9d88531fbb6853d8f04bddd5c25a86d7aff0f0681eafe9b894521d954d620` |
| web/scripts/test-persona-refresh-privacy.mjs | `e2d81c8f60274de9b58adec0744c509d688d5c7b03f2814d4669218c7c06dfd7` |

基线controller为07 SHA `9d73e4482e8f25ee6e090ae30bd0b02b0644317e772abd6b6b082c08eb4cd2f6`。精确patch见 `evals/results/p02-09-review-r2/controller.diff`。08模块/Workspaces、其他共享文件、Root backend/API、来源校验均未改。没有为新源证明复用旧测试结果。

## 实际最终验收与失败保留

实际父命令 `ALIA_DEVELOPMENT_RUN=p02-09-refresh-final-v5 flock -x /tmp/alia-project-heavy.lock .venv/bin/python3 work/p02-09/run-parent.py` **exit0**。父副本见run目录 `run-parent.py`。node --check、npm run build --prefix web、npm test --prefix web、cargo build --locked --bin alia、专属实际browser均 **exit0**。新库、随机端口、私有bin/web/journal与controller、独占target/p02-persona-refresh，持续总锁直到浏览器/service/controller/DB容器卷精确清理，全部cleanup=true、source_stable=true。

9组实际覆盖：三tab原私人视图/配置草稿；真实settings200提交后丢响应且不自动重试；真实来源校验422拒绝；旧私人身份/目标与成功同步标识清除；显式200后验证内容及原配置草稿/唯一dirty取消；相同key/body/If-Match显式恢复原settings回执，config_version只增一次；实际服务restart产生EOF，恢复GET真实422后原重连循环继续，修复DB后无新用户操作恢复；真实401退出；退出后迟到真实200不回填。SQLusage_reservations=0、runs=0，无云端模型调用/部署/生产个体。浏览器无pageerror，最后恢复图已目视核对。

保留：green-v1四组基础复验通过；green-v2扩展测试时序失败（SSE先失效后手动点击等待超时，未处理promise导致无子report），父资源清理通过；green-v3六组通过后重启桥拒绝健康前置不满足，浏览器/父清理通过。v4修正并通过9组；其测试顺序为：重启桥在健康状态验证，随后才放行浏览器对损坏投影的真实422读取；不改桥保护或产品断言。v5仅把EOF检查改为有截止的轮询，避免检查超时后留下异步轮询；最终全部命令和9组真实浏览器重新运行并通过。r1候选卡/报告在p02-09-review-r1完整保留，r2替代其登记请求。未将中间失败计为通过。

## 精确证据、审查及下一依赖

最终父report SHA `1f1722d78c277aa4e902348775decd4ea8662f861e206a6dd48ea3836747239c`；browser/report SHA `b202dc7e9c68bd3fb1107a21b33ce80569e268b6ae3c26c4547de8f1c1176e39`；506源manifest SHA `5c10e0ce41479c2b05acb2e5cff982f5feffe0a92001928123dfa5482c549264`。完整证据摘要、命令结果、失败/限制在 `evals/results/p02-09-review-r2/validation.json`（SHA `c486d398b0e4ce4767f3f34044b7a75dd0e1c70524cea5e184c893a5682c7484`）。正式任务卡同目录 `task-card-proposed.json`，subject `7d3601da24a12584f0e3f5534ed277fb335bbc0682ca57830321d4ee3d7ece14`；`root-registration-request.json`供Root登记，作者未改登记表或重试未就绪元数据门。

P03/P01按本固定两源独立审查后由Root接受；当前没有批准证据。未运行实际accepted-development来源删除浏览器（仍等08 producer）、全部05/06/07套件、全工程矩阵、13页/手机/主题矩阵、生产模型/20ABC/十四日观察。未知写仅覆盖settings，不外推message/goal。恢复成功时旧错误提示可能仍保留，沿用原提示策略；未另改UI文案/交互。若审查要求其他共享文件，将先提出精确最小patch需求。08全记录harness仍review_pending/not_run，不因09局部修补而补判通过。
