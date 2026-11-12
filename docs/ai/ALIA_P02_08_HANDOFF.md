# ALIA-P02-08｜完整实际发展浏览器交接 r6

Owner **P02**；非作者 **P03工程/P01来源**；Root唯一接口/整合裁决。流程 **review_pending**，作者完整有记录检查通过，尚无独立批准；不自批accepted或生产trial完成。原Goal/历史不重置，M3 runtime继续独立实施。原r5交接完整保存于 `evals/results/p02-08-review-r6/handoff-r5-preserved.md`；冻结08十源与旧80/5证据从未改写，本版明确是获Root释放后的实际分支后继。

## 本版改动与固定输入

Root persona-sources-v6固定483源（manifest `a965d6b835e029ea987dd2babe60b534b01394dce3f547a53253d811aa482b1c`），叠加已接受07 web、08模块、09 r2 controller、独立P01 producer三源（manifest `09c6b35bd9d6dcba3b6d354026217808ddfded1e8fd84bffe2341ce00fe23e08`），最后510源。未把活跃schema30/typed runtime或Root当前未完成shared代码混入此候选。前端真实模块仍只读与单目标revert，不新增trial/accept写入口。

相对r5：专属完整脚本接真实producer/private request，采集body-free幂等轨迹，等待私有fixture运行自然终态；state503后按09 fail-closed行为由真实state200及显式用户重读恢复。Root要求的Identity过时“撤回未实现”文案已改为实际management.development两bool提示/真实入口，与本页不提供提案、试用执行、评测或接受入口分开。原08 feature八源与Workspaces未改；共享controller只引用09已固定SHA，无新修改。13主导航和唯一dirty guard保留。

| 被实际验证的前端源（含09依赖） | SHA256 |
| --- | --- |
| web/src/features/Identity.tsx | `87ea932d6d71afb2ac965d7a6c7f065e55345d78ac9e7fca0607d54067f32131` |
| web/scripts/test-developments.mjs | `5d347961d074cb2c0acb47d88f7810e1d8fb9d0aa0360317bc9933eb781248e8` |
| web/src/shared/useIndividual.ts | `dfb9d88531fbb6853d8f04bddd5c25a86d7aff0f0681eafe9b894521d954d620` |
| web/src/features/Workspaces.tsx | `ab1ce8bdaf12cd127669321875c833ac5f5076f3bf63d956e02f698922624259` |
| web/src/features/developments/DevelopmentWorkspace.tsx | `2e462b7b94b283cdecf4787de0933d3ea134b6a225dabb088e2ba3e9bf2ab889` |
| web/src/features/developments/RecordView.tsx | `5a4773489bf9bda8b2c8649334aecd1fd48917398224fdd8097a1b09bb49cfaf` |
| web/src/features/developments/client.ts | `490ca731209740b0fe9884b8356b343ef651cf7b5432cdbfe80749e94c4cf587` |
| web/src/features/developments/contracts.ts | `b0293d131d38f3be9268261dc8e4c6ed94e5b63caa81554ccbc93e6f7e4e5a92` |
| web/src/features/developments/developments.css | `d03ca18093cd952fd977bfc26ecae0e24b9f625a13b5a4647b2f8fa0134bd7dc` |
| web/src/features/developments/stateMachine.ts | `104e60ee435ed43b48d6a71cd11e559e71fde06c9f20bac5696be56f04935c2f` |
| web/src/features/developments/types.ts | `cd3072d778d2fe413371de906010ea427d80cfaaf59f947f82934a6900a3fe3c` |
| web/src/features/developments/useDevelopmentRead.ts | `6b629e51e3a257768bb1079933526eea789a59bedf942dd5c9a5e08bc6339bab` |

## 实际命令和17组覆盖

`ALIA_DEVELOPMENT_RUN=p02-08-full-v6 flock -x /tmp/alia-project-heavy.lock .venv/bin/python3 work/p02-08-full/run-parent.py` **exit0**。node --check、npm build、npm test、cargo build --locked --bin alia --example development_ui_fixture、完整browser均 **exit0**。没有重跑原80合成/5未就绪或旧05/06/07套件。

17组覆盖真实空/加载；Identity actual readiness；严格closed DTO/no-store/ETag/query/reason边界且拒绝无写；六态/保存的validation/goal-task/真实deadline；旧head409及UI不可申请；键盘取消0POST；真实409/422说明保留；真实200逐development If-Match/仅reason/新revision/单目标恢复；1000 Unicode字符理由受理丢200后原键/body/version恢复一次；真实PG后段rollback503及同键恢复一次；真实DELETE202来源/epoch清屏、迟到source200不回填、被删记录404；privacy维护真实state200后显式重读；offline/online；实际401；已提交但迟到200与logout；再鉴权后的真实reverted、深浅360/390/1440/键盘/无横向溢出、无本地token/reason存储或浏览器pageerror。六张最终图均已目视查看。

producer先由真实empty/loading检查确认空库，再通过Root许可的synthetic measurement前置和实际Store转换产生12条六态/五target，绝无本窗口写accepted或自造通过payload。其身份、source/bin、数据库与5独立角色由私有request强绑定；服务在空库启动前已有安全0700私有journal。producer留下8 queued任务，本版仅通过已有带owner/nonce的服务重启让原worker自然处理，readonly确认pending=0，不SQL伪造终态、不复用账目。最终persisted runs=13、usage_reservations=0；run数不作模型调用数。0云端模型/无部署/无生产个体。

## 失败与实际限制保留

full-v1：10组后rollback503重放得到409，未显示成功；full-v2：12组后whole-App503清屏导致旧脚本找不到workspace按钮；full-v3：16组完整通过（旧Identity/旧脚本）；full-v4：新增Identity检查通过，后续真实409触发旧route中assert导致无child report，parent service/DB清理true，未独立证明该失败的browser cleanup；full-v5：只读20秒后仍有8 queued任务，父/子清理true，未开始revert。全部旧输出保留，未覆盖为通过。

此前409的具体竞争原因未独立证明。本版在合成前置真实终态后运行隔离撤回用例，未放宽服务CAS/head/权限约束，未在未知状态换key。异常lost200路由现在转发真实非200而不把它伪造成成功，也不以未处理断言跳过失败报告。当前通过证明此固定场景，不泛称无并发竞争。09 state失败后保留token但卸载私人内容是既定安全行为，本脚本正确走显式重读而非要求旧DOM继续存在。

## 精确证据和下一依赖

父report SHA `a4c6174c13ec7aacdf7605f5450723de4116c39e943ef4afe1e4686205c4f00c`；510源manifest SHA `5a8c3bb8f762d575c7b4f99dd7cc007711a07ab3ff6be68f4f720fcdbab4a519`；browser/report SHA `bccc4f7eddb90a878c7a4f44a0402d6a550ac561ed9ec795ac244d9e944a7740`；public producer report SHA `ad394b8c3f9ed56760c116ff9cc6d62cbd61341a9b9c91b546bb5de01c796f1d`。父副本/精确命令/bin/web哈希、私有PG公开身份及实际service/controller/DB清理见 `evals/results/p02-08-full-v6/`，全部source_stable及cleanup=true。credential request和临时journal随父私有目录安全删除，报告不含凭据或原始供应商内容。

本版完整validation与证据/图片摘要：`evals/results/p02-08-review-r6/validation.json`（SHA `d041e30a26ca856008a7f0986f64588c806bbf223963ca113770d74fd77d966b`）。正式task card subject `3eaa9a6c199d90969278f6ebb6da660e6b2d3f9facbf653ceb80c06fd92d2cb6`，reviews仍空；Root登记请求同目录，不自行改Root registry。下一依赖P03/P01按本源码独立审查、Root/上游backend与producer各自接受及组合整合。当前未跑新全工程矩阵、收费模型、生产trial/评测、20ABC或十四日，不把synthetic工程验证当行为成长。M3 runtime及新schema30候选必须另验；原Goal不在此标complete。
