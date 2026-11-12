# ALIA-P04-03｜真实来源事件安全检查

状态：READY。v3独立固定源码与Cargo target的6项真实HTTP/PostgreSQL测试全部通过，定向编译/严格Clippy/局部fmt/快照M0均exit0；源码摘要稳定、自己的PG清理通过，0真实或fixture模型调用。限定Goal交付；不宣称当前共享开发目录、全工程或行为验收通过。已读AGENTS与三份权威文档、协调文件P04-03冻结契约；开发目录只创建本任务四个新文件，不改接受的P04-01/P04-02模块或测试。

## 实现与已完成注册

- 新adapter `src/adapters/source_events.rs`，新service `src/service/source_events.rs`。
- 主线已实际注册adapter/service的 `source_events` 模块及 `.route("/source-events/{id}", get(source_events::source_event))`。本窗口不改开发目录公共注册入口。
- 冻结GET参数offset默认0，max_chars默认2000/范围1..4096；严格未知/重复/无效参数422。实际string payload.text仅三个明确事件kind允许；其余真实元数据与not_exposed，不公开raw payload/correlation/provider/审查/凭据。
- 一个RR只读快照，目标与个体递归可见；文档链接需真实同个体documents行且document.imported，运行链接只认真实trigger或action结果关联及递归可见run/action。隐藏链接null，不能从correlation或payload猜测。正文按PostgreSQL Unicode字符截取，EOF合法、超过EOF422，无正文的非零offset422。
- 只读，不调模型、不部署。6项真实HTTP/PG定向测试已实际通过。覆盖实际用户/纠正与合成持久生成事件、16KiB以上Unicode正文/EOF/空串、内部payload与非字符串拒绝、真实文档/触发/action结果链接与伪correlation、两代祖先/循环/个体撤销、严格参数/鉴权/所有public表摘要不变。完整工程P03，语义行为P01；广义图/导出/物理删除不在此任务。

## 输入与基线

主线已接受p04-02并保存独立副本，manifest `9a8b770f71846f7041e540ac27788cefaf8d8766b626a4212508ecc1f16fa184`。本任务四文件基线均ABSENT。现有总锁、PG串行与0付费调用规则保留。实际验证与摘要后续写入本文件。

## 唯一定向父任务

主线实际adapter/service模块与GET注册均已保存。已启动唯一父任务，报告 `evals/results/p04-source-events-targeted-20261112-v1/report.json`，先持总锁，再创建自己的工程PG，按顺序编译/新6项实际HTTP-PG/严格Clippy/局部fmt/M0/清理并记录前后源码摘要。不重复已接受P04-02的14项测试，不跑完整矩阵或真实模型。当前进度不能记为通过。

v1父任务session54100已终态exit1：定向编译exit0，新测试5 passed/1 failed/0 ignored，Clippy/fmt/M0尚未到达。极大offset边界实际503，未符合422契约；已将SQL切片起点在整数转换前限定于实际文本长度，Rust仍按真实长度拒绝越界，不用SQL分支规避可能提前求值的常量cast。失败报告保留，cleanup通过。另检测到并行主线8个源码/注册文件变化，sources_stable=false，此轮不作为冻结版本通过证据。下一轮固定实际摘要重新验证。

v2父任务session20677已终态exit1：定向编译exit0，6项测试均失败，实际HTTP返回缺少路由的404；当前源与前后摘要确有注册且稳定，不能据此判定新handler六项语义失败，说明被执行产物不符合已注册输入。保留该轮完整失败与cleanup通过，不追加或放宽断言。按主线最新隔离要求，后续使用验证过的N3不可变基线加p04-02接受源、本任务源与最小注册派生的独立源码副本，独立Cargo target构建；不会再使用共享target产物。主线ROOT-BELIEF-03在途代码不纳入。

v3唯一父任务session67958已终态exit0，报告 `evals/results/p04-source-events-targeted-20261112-v3/report.json`。独立源码以N3验证manifest+接受p04-02四文件+本任务三源构成，原基线不动；只在该私有测试副本中派生主线已授权的模块/实际GET与原修订路由、key/version兄弟可见性，完整前后SHA记录。目标目录独占 `target/p04-source-events-v3`，不复用共享项目产物，不纳入在途ROOT-BELIEF-03，不覆盖旧接受包或P03候选。只跑本任务定向检查；仍沿用总锁和自己的PG清理。v2产物错配的根因尚未独立定位；v3独立目录排除该干扰并取得实际6项通过，不将之前404归因为handler的六项行为缺陷。

## v3实际命令与结果

下列命令均在 `evals/results/p04-source-events-targeted-20261112-v3/source` 内执行，独占target；不是针对并行变化的开发目录运行。

1. `cargo check --locked --test source_events`：exit0。
2. `cargo test --locked --test source_events -- --ignored`：exit0，6 passed/0 failed/0 ignored，3.51秒；显式运行需要独立PG的ignored用例，未将默认跳过计为通过。
3. `cargo clippy --locked --lib --test source_events -- -D warnings`：exit0。
4. `rustfmt --check --edition 2024 src/adapters/source_events.rs src/service/source_events.rs tests/source_events.rs`：exit0。
5. 使用开发环境`.venv/bin/python`执行快照内`scripts/check_phase0.py`：exit0，M0 PASS；未代替产品验收或14天观察。

report.passed=true，sources_stable=true，cleanup.passed=true，无基础设施异常。环境凭据仅运行时注入自己的工程PG，报告无URL/密码；未使用生产个体、模型或供应商日志。测试中的生成/内部/action记录明确由合成持久化夹具构造，不伪称模型生成或真实工具执行验证。

## READY摘要

| 文件 | SHA256 |
| --- | --- |
| `src/adapters/source_events.rs` | `7a15d35ee42cbc033765176cd549e590adf9eeb57f1709a1f351d12b8d0d516c` |
| `src/service/source_events.rs` | `c101b9f19ca1c785690c9350a20c4854bc37ca6deb7ef57ef2705b6c0ad0dfe1` |
| `tests/source_events.rs` | `c999132c076e40136048309bfc75b6f459f98f8bbcabe47c33292f7b00ceacc3` |
| `evals/results/p04-source-events-targeted-20261112-v3/report.json` | `86b1c06fc22bce8813144852961a53c9111e5b3f5cfc3959721df00a496ce0fc` |
| `evals/results/p04-source-events-targeted-20261112-v3/source-manifest.json` | `4cc2dcb622514c47981042975c974833ea11c5ba8f3d3f33c4bf6131ec7946be` |

交付时再次核对开发目录这三源与v3冻结输入全部SHA匹配。N3基线manifest SHA `22003de0618727be042ec4d157936fb099b67d438e7e6ebc59525535010d2b3e`；p04-02接受manifest SHA见上。全部精确输入、最小派生注册旧新SHA、独占target及命令日志均可由报告复查。

实际`02.log`执行的是独占产物 `target/p04-source-events-v3/debug/deps/source_events-11eef7f15f3330cc`，SHA256 `8166d196cf67362c2a9cf536fe7a13d304e8ad7fec5cb4d7127a8148ff56ca6c`，并非共享target中的同名文件。

## 限制与下一依赖

v1/v2失败原样保留，不合并为通过。独立基线只证明本限定切片，主线在途ROOT-BELIEF-03及P05关系功能不在v3输入，不能借此宣称它们已验证。资源读取的RR一致性是单响应快照，不是跨多次片段请求的永久快照；来源事件仅为实际记录，不保证陈述世界事实正确。多个真实可见运行关联时按UUID稳定选择一个，不公开总数或隐藏ID。广义来源图、完整导出及物理删除仍为后续要求。

下一依赖：主线按三源摘要接受并保留独立副本、P03将已注册接口纳入后继冻结工程候选；若前端需要来源检查，按此冻结投影消费，不扩展为原始payload日志入口。P03执行完整工程，P01执行行为判据；本窗口不更新权威里程碑、不部署或追加付费验收。
