# ALIA-P03-11 / 版本化技能 vault 与真实沙盒验证 READY

COLLAB-02 元数据补记（revision 2，文档差异）：owner=P03；独立 reviewers=Root（代码）和 P01（证据）；状态=`review_pending`。原 READY 与公开 v1 包不改写，四个 Rust 源继续逐字绑定最终 v4；本补记不代表 Root 放行、独立批准或新增测试通过。生产维护回归 Root，P03 后续只承担已授权工程 QA。依赖为原 P03-11 固定输入、Root SDK 扩展授权及 COLLAB-02 r4；协议审查的 `not_run` 继续保留。MODEL-DISPATCH-01 只适用于后继 QA 切片，不追溯标记此前 SDK 实施模型或强度。

**Root新增独立协议审查限制**：P01-CAPABILITY-PROTOCOL-01原计划绑定不可变v4，比较正常return与“合法def run＋候选直接stdout＋提前退出”是否被区分。该独立复验被平台内容审查systemError中断，root已停止；Cargo/Docker均未运行，状态严格为`not_run`，既不记产品pass也不记fail。本窗口未新增、替跑、改写绕过该组合。协议要求以run(input)正常返回并由wrapper序列化为完成；当前实测仅覆盖下述已跑cases，不把正常case通过或无函数拒绝等同于该新增对抗属性已验收。该审查由root后续决定，生产开放仍为false。

P03-11已完成本地QA范围。仅改新`src/adapters/sandbox_skills.rs`、`tests/sandbox_skills.rs`、本交接，以及root追加授权的`src/adapters/sandbox.rs`/`tests/sandbox.rs`。没有修改共享module/Cargo/权限/迁移/生产路由；本模块和SDK仍未在生产adapters注册，0模型/收费/部署/生产初始化。验证不等于运行权限，不宣称M4/M5或通用正确性通过。

## 最终 typed API

```rust
SkillVault::create_in(trusted_private_parent: &Path, launcher: DockerCommand) -> Result<SkillVault, SkillError>;
SkillVault::open(trusted_root: &Path, launcher: DockerCommand) -> Result<SkillVault, SkillError>;
SkillVault::root(&self) -> &Path;
SkillVault::create_candidate(&self, draft: CandidateDraft) -> Result<SkillVersion, SkillError>;
SkillVault::revise(&self, current: &SkillVersion, draft: CandidateDraft) -> Result<SkillVersion, SkillError>;
SkillVault::view(&self, individual: Uuid, skill: Uuid) -> Result<SkillView, SkillError>;
SkillVault::validate(&self, version: &SkillVersion, cases: &ValidationCases) -> ValidationAttempt;
SkillVault::validation_report(&self, operation: Uuid) -> Result<ValidationReport, SkillError>;
ValidationCases::new(Vec<(String, String, String)>) -> Result<ValidationCases, SkillError>;
ValidationCases::sandbox_calls_upper_bound(&self) -> u32; // 1..3，执行前预留原三工具额度
RegistrationGrant::issue(&ValidatedSkill, DateTime<Utc>, SkillScope) -> Result<RegistrationGrant, SkillError>;
SkillVault::register(&self, RegistrationGrant) -> Result<RegistrationReceipt, SkillError>;
SkillVault::revoke(&self, &SkillVersion, reason: &str) -> Result<RevocationReceipt, SkillError>;
SkillVault::invoke(&self, &SkillVersion, input_json: &str, timeout_ms: u64) -> InvocationAttempt;
```

`CandidateDraft`闭合Deserialize字段：individual_id/name/description/source/source_event_ids。名字≤80字符、描述≤2000字符、UTF8源1..24576bytes、来源ID1..32且非nil/不重复；个体UUID非nil，skill UUID程序分配，首版1，revise必须从最新精确版本生成N+1。name不参与路径。旧源/报告/事件不覆盖。`SkillVersion`绑定individual/skill/正version/source_sha256，所有外来引用实际重验。

`ValidationCases`、`ValidatedSkill`、`RegistrationGrant`不实现Deserialize；前者只由受信任controller构造唯一case ID的1..3个(input_json,expected_json)，每份原文≤8KiB且严格单JSON。不是模型自选case或自评分。Grant私有字段，只能从实际验证/恢复产生的`ValidatedSkill`发行，精确绑定个体/id/version/source/report/case-set、官方image、wrapper版本、未来到期时间及唯一`sandbox.python`范围；模型bool、manifest registered/passed字段不能授权。新case验证会废止旧报告的grant，N+1即使源字节相同也须重新验证。

Attempt始终给出`sandbox_calls`（实际调用execute_skill次数，包含失败/不可用）、`attested_executions`（实际通过执行前隔离前导的次数）、真实outcome及结果/持久状态；两者不混计。失败、在途变更或落盘失败也保留已发生调用数。Root必须**调用前**用上界预留原三工具slot，调用后按实际结果结算；不能事后才发现超额。没有自动重试、追加case或自动注册。Root action_id幂等、未知操作不自动重放仍须由共享执行层实现。

## 新旧 SDK 协议

旧`SandboxRequest/execute`保持源码1..32768bytes、原outcome字段及原9测试函数/断言/helpers从首个#[test]到末尾逐字不变；仅在测试头部加严格JSON的domain alias和public测试模块可见性。旧09源/报告保留，root已归档received-slices/p03-09。

root解决24KiB源＋8KiB输入无法再装入旧32KiB脚本的问题后，新增：

```rust
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSandboxRequest { pub source:String, pub input_json:String, pub timeout_ms:u64 }
SandboxExecutor::execute_skill(&self, &SkillSandboxRequest) -> SkillSandboxOutcome;
```

源1..24576bytes、input_json≤8192bytes且严格单JSON、timeout100..10000ms。`SkillSandboxOutcome`包含原outcome及protocol/source_sha256/input_sha256/wrapper_sha256，不改变旧outcome消费者。协议固定`python-skill/v1`，编译期固定`python -I -B -c <wrapper>`，原隔离前导也来自同一编译期常量；可变内容只有stdin JSON `{source,input}`，carrier≤160KiB。源JSON最坏6倍转义仍受该独立上限，input保留已严格校验的原始JSON字节，不靠重编码浮点数改变8KiB边界。新carrier不扩大旧raw限额。

新模式inspect精确argv、protocol及source/input哈希label，用户内容不进入argv/env/mount。原网络none/rootfsRO/capdrop/NPRIV/65534 UIDGID/64pids/128MiB memory+swap/1cpu/私有tmpfs/seccomp/墙钟/64KiB双流/实际owner ID清理均保留，执行用户代码前重验。wrapper在容器内AST确认顶层同步def run，并在exec后确认一个位置参数；只向函数传入已解析JSON，输出JSON禁止非有限值。Vault再严格解析stdout：恰好一个JSON，无额外stdout/stderr日志、重复key/NaN/截断/非0/隔离或清理失败才可验证。完整24KiB源×8KiB输入已实际通过，包括注释控制字节造成的大carrier；不截断/减配，不在宿主执行Python。

## 持久化、状态与在途约束

- 可信owner的0700私有父目录内程序分配vault，源及事件/head/lock均0600不可执行常规文件；拒symlink路径组件、hardlink、危险权限、未知文件/字段。外部API没有source/report路径参数；UUID/版本生成固定文件名，目录fd锚定访问。
- 源和审计事件原子不可覆盖发布，文件及目录fsync，head原子替换；成功创建/注册/撤销回执在持久化之后返回。互斥文件锁只覆盖本地短事务，不跨容器执行，因此可实际撤销或换版本。
- 事件hash链与独立head绑定完整顺序。恢复重放闭合事件状态机，重新核对所有旧源、真实结果、source/input/image/protocol/wrapper/case-set/唯一execution IDs；不读取passed/registered布尔作为真相。pending写入或截断/孤立文件拒绝，未完成执行保持未知及保守调用上界，不自动重放。
- candidate/validated/registered/revoked分开。注册仍不授予Root运行权限；invoke只接受最新未撤销、注册未过期、精确源/产物/版本的对象。实际执行后再检查版本/epoch/撤销/期限/文件完整性；返回SDK成功也可能是被拒或未持久的工具结果，不能提交为成功经历。
- 验证预算30s、单SDK≤10s，读链/源/锁和前后检查使用同一deadline；SDK的1s清理保留不取消。像现有SDK一样，无法声称可强制打断任意宿主文件系统/kernel阻塞；超时/不确定提交不升级成功。持久存储配置与进程/硬件故障分别看待。

## 实际执行与失败保留

所有尝试使用不可变N5R3基础（manifest529c856a196d1c4f0b2ee0e766386dd97f1436b92684bc1f4db5b5aed09af974）加五个自有源。v2以后只读叠加当前root canonical Cargo.toml/lock（相对N5R3仅增加已存在的libc直接依赖），没有改共享Cargo。每次不同输入使用全新独占target；测试可执行文件由Cargo JSON产物逐一记录SHA，不跨输入共享rmeta/rlib。

- v1 parent2292420/session98719 exit1：旧N5R3缺直接libc，build101；未启动Docker。固定源/私有清理通过，失败保留。
- v2 build/25普通/Clippy/fmt/M0实际0；原9在准入处9fail/0pass/0ignored，未开始Docker。原因是Root已将heavy_lock变成model/shared别名，旧9要求global的fd却收到模型锁；不是SDK隔离/业务失败。原全局inode1660460未删除，1873055是模型锁，已按Root新锁接口纠正驱动，不改变原断言。
- v3 parent2348698/session11146 exit0：317固定源，原SDK9实际通过/13记录，新skills9组全实际0；报告SHA `ea214dbd2f651dfd9dedd5298175828cb6e038d9c5d59de00898d7e1c8a4a664`。SDK新模式修改后的旧9回归只在此运行一次有效矩阵，未用旧09证据代替。
- **最终v4** parent2409610/session31941 exit0：317源manifest `3a6364e41078cd327634da76970625efeebac37c815ffd30cac2827671ebc65a`；25普通通过、18ignored不算通过，9组新actual独立二进制逐组实际通过；Clippy/fmt/M0全0。源码/执行SHA/清理真。UTC16:18:52.743465→16:20:37.219138（104.476s，含锁等待15.110s）；报告SHA `7114ee5d23d1b0b7f0c048291f2fb4656c304ff4c134c9d4aa0cd15557994d6a`。v4只加Vault调用上界/重放deadline，SDK与旧9源及实际bin SHA与v3完全相同，因此只读引用其原9真实证据，不重复原9。
- 最终skills测试binary SHA `6e4ee9312de6c3915866d28843824ffb3c350204970390c6f3734fa0ce3d6af3`；raw09 testbinary `20faaa8e4c66056a1e33f083404d459afb838995a99374a78b22d5e8ff76c286`。M0是在固定N5R3范围实际schemas5/examples10/negative56，不冒称最新主线schema29或完整候选已验收。
- **实际磁盘补证** parent2440139/session35147 exit0，使用同一最终binary，TMPDIR设为工作区磁盘的0700私有父目录。真实source文件device66306、mode0600/nlink1，区别于/tmp tmpfs device38；实际验证→注册→invoke→新进程恢复通过，两个版本源保留后精确清理。另磁盘上权限/链接/篡改拒绝实际0。报告SHA `9ba41029f777f1c3cd8ea438be6fb9d64db5eb9aae95218456e5b242d53a18aa`，不是内存HashMap或Mock Outcome，未做断电硬件实验。

最终实际覆盖：正常/完整边界/真实失败case、stdout与stderr日志/重复JSON/NaN/无函数提前exit拒绝、过期grant、别源/版本/image/case/protocol/wrapper与未知字段换绑（重封hash链仍拒绝）、源/权限/链接篡改、新进程恢复、实际在途撤销/新版本/过期/文件篡改、超时/输出/非0和真实清理不确定性。人控删除准确自有容器造成1项expected cleanup=false/infra_failed，未验证/注册；实际控制器已移除，最后确切查无。另篡改在途结果corrupt/durable=false但实际调用1仍保留，恢复明确unknown上界1。

v4＋磁盘补证共有30次Vault execute_skill调用/30次已隔离执行；另直接structured SDK 6次中2次实际隔离、4次非法JSON准入拒绝。11份成功验证产物/7份失败产物（含同一技能重新验证，不当成11个当前可运行技能），失败不升级。独立审计实际exit0：校验全部协议/源/输入/case/结果与可执行SHA，历次已知81个handle成功查询全absent，私有vault/client目录与已知父PID不存在；P03-10 READY和并发隔离报告原SHA保持。

Root批准拆锁已读取：驱动使用Root现有sandbox/shared、candidate/full-matrix、独占target及workspace资源锁；仅为保留旧9断言另持legacy global并传真实fd，不占模型预算锁，不改共享锁实现。完整setup/子执行/清理/报告同父锁生命周期。SDK source/input哈希label与测试唯一源码comment使在途故障只选自己的实例，恢复不能删除外国身份。

## 下一项依赖与明确限制

1. Root负责生产module/能力注册、实际DB individual/source归属、来源递归可见性/删除屏障、权限/租约、原三工具预算和action_id幂等/unknown结算。查询报告/发grant/注册/调用前后均须检查；拒绝结果的Evidence不能绕过Root栅栏成为成功经验。当前仅QA私有artifact，没有生产工具创造开放。
2. Root必须把候选各版本、验证case输入/产物、grant/撤销、调用输入/结果及所有private payload纳入来源派生和显式物理删除。当前append-only链没有删除API，不能只删一个source文件让它继续可用；按skill隔离vault或设计受控重建/清除与外部单调锚由root决定，不自行选DDL29。
3. 可信owner私有文件系统不是对抗同UID管理员整体回滚的外部信任锚；Root恢复/备份必须用实际DB/journal隐私代次、版本/撤销单调锚复核。只证明声明cases和所测Linux/Docker/cgroup-v2环境，不证明通用功能、意识、任意宿主/kernel故障、完整M4/M5或14真实日历日。
4. 新`tests/sandbox_skills.rs`的9个ignored实际用例不是SQLx；Root/P03后继整合须显式加入分派域与独立mandatory阶段，并给旧9保留legacy准入桥或另行授权更新准入。当前Root资源锁版SDK QA helper不能把resource fd冒充旧global fd；未知ignored应继续fail closed，不能跳过或混计PG。P03-11未越权修改共享QA分派器。
5. 下一正式P03编码切片已由root释放：management_browser接受链按p02-05→p02-07增加固定pin及正/反单测，不能从live采样或放宽未知源哈希，不加跑完整矩阵。p02-05 manifest `ee553288cfe5c2437c6b4635d1c40f6bdfdf83617f7b8c1953f303c7bf036b58`，p02-07 `234a4608bab9b263db68454c5624b20419f344a0b11aa5fec82f7bb5181b797f`。后继候选还需Root组织p04-09b-owner、root-social-context-01/format-01及P05-07有序覆盖；packet8不是旧collector已验证模型语义，不收费。本窗口在本任务交接完成后顺序进行接受链工作。

最终READY/哈希/独立审计在`evals/results/p03/p03-11-ready/`；公开`outputs/ALIA-P03-11`只含报告、合成结果、源码改动与脱敏日志，不含bin/build/镜像/完整基线/私有vault/state/凭据或供应商原始日志。最终交接文档在测试后整理，代码四源与最终测试输入逐字相同，文档差异单列在READY，不伪称重新编译文档。
