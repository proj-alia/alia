# ALIA-P02-07｜实际 SSE 隐私失效、清屏与重连

状态：**READY**。先保存独立测试并实际复现前端缺陷，再按 Root 授权只修改 `web/src/shared/useIndividual.ts`；新增 `web/scripts/test-sse-privacy.mjs` 与本交接。最终精确源码及证据：`evals/results/p02-07-final-v3/ready.json`，SHA256 `4cb9d86346d556ba1aaa8b944c1d81d74d1eccf811a09c084d86c4e847ae7990`。未改 privacy provider、events.ts、导航、guard、协议或后端；P02-05/06 已固定源码与证据逐字节保持，未重跑其历史套件或完整矩阵。

## 改动与实际行为

旧行为：真实 DELETE 已202、目标GET404且五个旧SSE读取器均EOF后，认知页仍显示被删除来源派生的目标正文。旧 useIndividual 只变更连接文案，保留全局 state/目标与已挂载工作区；重连还可能继续旧cursor，而先前在途 state 响应未取消。

修复：失流/离线时递增会话读取代次并取消旧 AbortController，清空私有 state/记忆/目标/消息/输入/回执及旧分页位置，使旧工作区卸载并清理私人草稿与选中项。恢复以新 signal、当前认证重新读取 state 和全部待展示列表，完整成功后原子发布，再由原 SSE effect 订阅新 event_cursor。失败保持隐藏并有界退避，退出/401取消恢复循环。保留 runtime 配置草稿、原文及版本基线和现有唯一 guard，不以清空 guard 通过检查。传输中断/离线保留当前内存令牌用于重新认证读取；实际401或主动退出仍按原规则清令牌，所有路径均无令牌持久化。

| 交付源 | 最终 SHA256 |
| --- | --- |
| web/scripts/test-sse-privacy.mjs | `56f33a3b49fdeaa920192879b7114b7a4162da32e153ce110e41b3ee0f4d0580` |
| web/src/shared/useIndividual.ts | `9d73e4482e8f25ee6e090ae30bd0b02b0644317e772abd6b6b082c08eb4cd2f6` |

useIndividual 原 SHA `117ead78ebe9b139ca4e7eba8236212d5036039620959d41e08cede5e8723d6e`；限定差异 45 added / 12 removed，副本 `evals/results/p02-07-final-v3/useIndividual.diff`。现有 privacy provider 生命周期已能随个体快照失效清除显示，本任务未编辑，SHA `03cf9cf629111c72938cdc8247abeed9fa93dc4b7cc4f25e88dac68bf7792c66`。

## 固定输入、命令与结果

固定底座：05 full-v5 manifest `3b02b37f8d777d26284d06b19bd5f6e110ae58e029b6eb4f2ba1356ec429e481`（包含 root-privacy-http-v2 固定182源，manifest `893b0073a5193c25158aafd8defae4579fec4060e16eb8bd1211e271fef2ba72`）；叠加06五文件 ready `38b94d1919c65fddf8b2517c33b8ce952627d4f250baf461f6b92c01106a9167`、Root两源SSE补丁及本切片，总计 413 个源文件。不会带 live packet7/发展/导出代码。

Root 两源固定 SHA：service/events.rs `a9c077f09bbbd78d476ecdd8eb7749bff97e0c949a336de1acc1aee0eb8a0fc6`；adapters/notifications.rs `576324433ab4cfac48afb80c75fe58403a7b820d8f88ca690f59c11705d2fb15`。取自 P01 固定副本 `work/p01-privacy-review-02/source`，已核对 P01 实际4过报告 `a47d48ad0d97ed4a95897e80f1eff4ee212db0760c3d0a9225cefb003f84c6f6`，不计为本窗口测试次数，也未修改这两源。

最终父命令 `ALIA_SSE_RUN=p02-07-final-v3 ALIA_SSE_SHARED=1 flock -x /tmp/alia-project-heavy.lock python3 /tmp/alia-p02-07-parent.py` **exit0**。可审阅副本 `evals/results/p02-07-final-v3/run-parent.py`。连续持锁覆盖构建、真实服务/PG/browser、重启、全部清理；RUST_TEST_THREADS=1，专用 target/p02-sse-privacy、新私有端口/DB/0700 journal、复制 binary/web，父控制器仅可重启自有服务。stage600s上限及自身进程组清理，不因观察超时重启任务。

| 命令 | 实际结果 |
| --- | --- |
| node --check web/scripts/test-sse-privacy.mjs | exit0 |
| npm run build --prefix web | exit0，TypeScript/Vite |
| npm test --prefix web | exit0，原 SSE parser、组件/13路由/Config 合同 |
| cargo build --locked --bin alia | exit0，固定源构建 |
| node web/scripts/test-sse-privacy.mjs --shared-context | exit0，10组实际HTTP/PG/Chromium，同一上下文五标签 |
| .venv/bin/python3 scripts/check_phase0.py | exit0，6 schemas/53 closed objects/13 examples/68 negative cases等静态规范检查；非产品验收 |

## 实际覆盖与精确证据

10组检查覆盖实际DELETE使隐私代次改变，以及真实独立日志fsync后PG INSERT故障使DELETE503/health503的两种失效；每次五个实际已认证页面打开记忆+来源+私人草稿/dirty预览、认知目标+信念+来源、关系档案+表达、自传正文+来源、Config草稿。真实旧 reader.read.done 后各页无私人正文/预览，旧 state 请求实际 aborted，先前读到的真实 state/source200延迟返回不复活；恢复时真实完整新快照先于新SSE，cursor等于新state.event_cursor，旧选中项/私人草稿不复活；Config保留11013草稿、旧baseline与真实409/取消行为，无静默覆盖。其余检查为真实离线/在线、实际state401、实际SSE401、主动退出时迟到真实snapshot200、退出不自动重认证，0 uncaught errors、usage_reservations/runs均0。

Playwright门只延迟route.fetch得到的真实响应；鉴权反例把无效token发送给真实服务并转交实际401。EOF观察只记录原fetch reader.read的done并原样返回Response/字节/结果；未注入模拟通知、reset、成功JSON或人工后端状态。最终四图（两种清屏、两种恢复后Config409 guard）均已目视核对；不把图中加载占位称作已恢复。

| 证据 | SHA256 |
| --- | --- |
| p02-07-final-v3/report.json | `096197141d7799dd044d5afcb8e603a52fdd7c705bb211ad5412a88db717ab55` |
| p02-07-final-v3/source-manifest.json | `192ffbbcc4045dc457ed44c951e1f66ec7a580588560f67f9be59fc1fad9e7af` |
| p02-07-final-v3/browser/report.json | `e05d6556bcd4ee08d220d1f67cfdb1ea6c0d28a5ca8ef583138c36a656a54557` |
| p02-07-probe-v2/browser/report.json（修复前实际反例） | `d9bb9a797d15b6c5f84368a47ef905799b61adab70cfe2bc595a94cbb9ecbd35` |
| p02-07-final-v2/report.json（独立浏览器上下文10组，同产品源） | `9ea0b38450d4a7f25fa0702ba77078524cf168a00286e4b13b0ab2f7d59d4cea` |

以上相对 `evals/results/`。最终 source_stable=true、browser cleanup=true、自有 service/controller/database cleanup=true，所有启动ready/停止graceful。原05 ready `189ec29788376f7fbd2c9e0aec3ac44a5800ad7380a7b7ed41e6f0e32f2dfe9a` 与06 ready及其全部自有live源和旧固定source库存均未改写。

## 失败记录、限制与下一依赖

probe-v1 的浏览器画面显示旧目标，但 requestfinished 不是reader EOF观察点，该轮检测器超时；其未取消的诊断轮询使Node写报告后未退出，已只对经/proc/cwd/cmdline/PGID核验的本子进程SIGTERM，父任务独立清理true。修正观察器后probe-v2真实202/404/五EOF仍保留认知私有目标，构成获准产品修复依据，失败截图/报告保留。

修复后browser-v1八组全通过。final-v1在清屏后恢复Config快照时15秒等待失败，仍无私有复活；仅推测同上下文长流与连接队列有关，不宣称已证明根因。随后独立上下文final-v2十组通过；最终final-v3回到同上下文五标签，保持EOF后独立清屏断言不变，仅把恢复等待上限设45秒以覆盖原请求15秒截止与重试，全部十组通过。故障报告均未覆盖，不用旧通过抵消失败。

这是一项有界浏览器工程验收，不是全部网络/背压调度、长期运行、完整工程矩阵或整体隐私安全证明。断网现在会销毁私人记忆草稿，符合本任务清理要求；未保存非隐私运行配置仍保留原guard和baseline。未跑05/06旧套件、付费模型、行为/十四日观察、备份/云端撤回；这些不写作通过。下一依赖由Root/P03取ready.json中两个精确前端源，与已固定SSE后端和其余READY切片组装不可变候选后执行受影响整合/唯一完整矩阵。本窗口不改主IMPLEMENTATION/协议、不向其他聊天发消息。
