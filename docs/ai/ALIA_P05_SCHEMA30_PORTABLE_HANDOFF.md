# P05-SCHEMA30-PORTABLE-02 / r3

**review_pending**。Owner P05，reviewers P04（数据/恢复）和 Root（共享注册、原子 hook）。本检查点补齐 schema30 个人导出、真实受管备份及隔离恢复的内部适配；没有启用 HTTP/worker/生产能力。原 P05-08 Goal 和全部旧证据保留，M4 bridge 没有修改。

## 可整合源码

固定输入为 P04 `p04-m3-measurements-integrated-20261112-v6/source`，manifest `35dfffa26bc403cf2975b591c0ec06fc9f40442a943642427f1bfd1cc621327d`，加 P05 自有模块/测试；完整最终501源清单在 `evals/results/p05-schema30-portable-v3/source-manifest.json`，SHA `1a150b60403adcdf0c071819866a1d72e363ad345b4fc0c6d37bb607b711ced9`。所有者六源/0030未改，测试只使用固定副本。

| 自有文件 | SHA256 |
| --- | --- |
| src/adapters/privacy_development.rs（同上一消费检查点） | `960bec614916a441f19152b3fd2addab8a017e475348b4379e7f799623c4c388` |
| src/adapters/export.rs | `778d31b3975e2afcd7fc56262fe0ac1850122e94aff213dda0b8b01fbe005d81` |
| src/adapters/managed_backup.rs | `6debfce8277691052d6e4848d6dd911c2863fb7c5cee7801344fa1d93942ef70` |
| tests/development_privacy.rs | `1c622eca9aec51b74eb7e2d3df683afe153cd20d7112a1f806b81fe97e9cdcc4` |
| tests/managed_backup.rs | `3c71d17a10476cdeda26a8391934f1926322fa0925c61e2775cbfe4c548a82d0` |
| tests/export.rs | `f1b293c47f4b9949cc06f0da1fd97986f463de77a12c1b6e1ce5b5ea967a799b` |
| tests/schema30_backup_catalog.rs | `734a6dfeb729390ff8a408f8c76e7ee8b9c211b4659826cbfb076cd548b3ad24` |
| tests/support/p05_backup_catalog.sql | `1d01478b02603834085954724c3381539186724cd86d30c77bf1d1f9cf7799f7` |

精确消费补丁 `work/p05-schema30-privacy/portable-owned-r3.patch`（SHA `cdfa4f9dfac3b240dae1518753c7cfd88540da875266d8eb9eec37eb0ce936ea`）。源码已实际保存于自有 live 文件，未写共享注册/控制器。必要 Root 私有候选补丁 `work/p05-schema30-privacy/root-portable-registration-guard-r3.patch`（SHA `0a335b6ece53de4108e0fc00446c816c3fd67b917d9dbfc63f205f40089f978c`），仅 managed_backup 模块登记与实际 schema30 privacy inventory 常量。Root 最新已决定保留29+30两个精确已知指纹白名单，由 Root 自行采用；本私有单30补丁不是要求删除29兼容，也不是移除未知 schema 拒绝。

## 最终行为

- 个人导出精确支持 schema29/30：旧56表与旧 fingerprint 保持，新61表新增 program/plan/case/attempt/case_sources；清单、关联可见性、JSON递归脱敏和并发行版本校验均包含新表。只接受两种完整已知列/约束指纹；未知列/表/约束仍拒绝。
- 新数据用同一实际来源图筛选：完整合法个人计划/判据/上下文/结果可导出；删除后整族行及关联标识不可见。attempt 的 request_digest、租约 generation 作为操作元数据省略；原费用保留，隐藏 run 关联仍剔除。64MiB/100000行/稳定排序/整份拒绝规则保持，不截断。
- 备份绑定实际 canonical0001..0030 的 SQLx checksum、完整列/约束及强目录（默认值/索引/类型等）；全部新表真实 pg_dump/pg_restore 还原。schema30结构指纹 `eb00e02e410278d5ca86d0efed7ea3364dba839f48a01615f68ad6f5129e5976`，SQLx真实迁移后强目录指纹 `7f22bd56d77da3d9711120a634b031183900b0e754215b122c28457fee6dc52a`。
- 删除前旧archive失效后，只能原日志受控 quarantine 恢复；真实隔离新库强制重放原删除，清除 plan/case/context/result 正文，保留原48已知/60未知合成结算和 opaque reservation，不退款、不新造请求、不激活。精确清理自己创建的恢复库，不改源库。
- 备份本版明确接受30，不自动升级旧29 artifact；旧29 r8内部实现/证据仍保留，30 manager对旧版本/目录明确拒绝。跨版本备份升级策略尚未实现，不能按兼容已验证处理。

## 实际结果及复用边界

[最终汇总](../../evals/results/p05-schema30-portable-r3-summary.json)，SHA `f315dbbb00473fa9ce8da4e608adde9def6c06dae42e77fdd0f7bcdaadd623c0`。v3 [原报告](../../evals/results/p05-schema30-portable-v3/report.json) SHA `66f396a8a16131df633ac76f8b5d7f34313974bfec894b24f89996600ddfda75`。

| 范围 | 实际结果与版本 |
| --- | --- |
| schema30 development_privacy | v2原13条通过，v3新增4条通过；合计17条，不冒称v3重跑全部17 |
| schema30 managed_backup | v2原17条通过，v3新增真实双恢复1条通过；合计18条，不冒称v3重跑全部18 |
| schema30 export | v2完整11PG通过；纯JSON/限制1条通过 |
| 真实 SQLx schema30 catalog | v2校准1PG通过，指纹与v1完全一致 |
| schema29 export兼容 | 独立无0030固定副本完整11PG通过，旧56表/指纹分支实际运行 |
| vault路径/限制普通测试 | v3封存实际二进制1条通过，18PG显式ignored；不计成其重跑 |
| v3严格Clippy（lib+三个相关targets）/自有Rustfmt | 均exit0 |
| M0当前工作区 | exit0；与真实产品/14天验收分别记录 |

v2与v3生产源码、schema、配置逐字相同，仅两个测试夹具的不同场景输入修改。旧13/17条位于夹具前的测试源码逐字一致；复用的41PG、catalog和普通JSON结果绑定v2报告、源码及封存产物，v2原报告 passed=false 不被改写。v3只复验受影响5条并严格检查；P03仍须对Root整合的最终固定候选跑所需整合，不继承自我批准。旧/schema30副本各自独立PG/卷/私有目录全部清理通过；没有模型请求、真实供应商费用、生产个体或部署。

历史失败保持：v1旧基线两条旧测试的56表/初始兴趣数量断言失败，18备份全部通过，Root旧context严格检查失败；v2按实际schema/seed修正旧断言并换P04正式固定源，原41PG/strict通过，但五个新fixture因同prompt不满足P04新不同场景校验而失败；v3仅改合成夹具为来源核验、矛盾处理、反证搜索三种输入，保留生产规则，五条全通过。未把同供应商意见或Fixture当真实成长。

## 主线下一依赖

Root已通知其真实整合的schema unsupported失败是旧guard/export未采用的级联，未认定新泄漏；请采用以上精确export/相关tests，并按Root所有权更新29+30已知guard。Root-controlled managed_backup注册、原删除事务与archive失效hook、运行镜像中的PG17客户端、鉴权/管理入口与08非作者完整审批仍未完成；本轮manager测试的invalidate_in在已真实提交的删除后另开显式TX，不声称原子安装已完成。

P04当前P02 permission原RED仍为全链阻断，P05没有用本次隐私专项替其批准。P01行为、P03全工程、正式成长/模型切换/14真实日仍独立未完成，M5没有判通过。主线可立即继续独立核心代码，无需等待重复跑相同专项。本版Sol/high是Root显式派发要求，当前实际回合model/effort未独立观察，未伪称生效。App Goal/预算未重置。
