# M2 本地文本编码基础

状态：编码器、分块、持久向量集合、精确查询及共享读取已接通；持久索引worker已接入可选服务配置。配置ALIA_EMBEDDING_DIR后，对话/工具/WebUI使用同一个编码器与混合读取，未配置时保留明确词项模式。已加入姓名/别名精确解析及人物角色投影；自然语言意图、最终回答来源质量和完整M2验收仍未通过。100题历史测量和失败记录保留，当前读取与真实模型开发证据见[MEMORY_RETRIEVAL](MEMORY_RETRIEVAL.md)。

采用官方`intfloat/multilingual-e5-small`的ONNX FP32文件，修订`614241f622f53c4eeff9890bdc4f31cfecc418b3`。固定五个文件的大小和SHA256见[资产清单](../config/embedding-assets.json)。[官方模型说明](https://huggingface.co/intfloat/multilingual-e5-small)要求查询与段落分别加`query: `和`passage: `前缀，使用注意力掩码均值池化与L2归一化，输出384维；输入长度上限512 token。

Rust可选feature `local-embeddings`固定fastembed 7.7.0和ort 2.0.0-rc.15；通过用户提供模型文件的构造入口加载，不启用HF Hub下载feature。底层tokenizer 0.23.2与原生成模型计数器的0.22.2是独立依赖，不能互相替代。[fastembed包](https://crates.io/crates/fastembed/7.7.0)与Cargo.lock保留版本。CPU策略明确为PreferCPU、两条内部线程，batch为8。ONNX执行库的下载发生在可选feature构建时，应用编码过程不访问网络。

可复现执行：

```sh
python3 scripts/install_embeddings.py
cargo build --locked --features local-embeddings --example verify_embeddings
target/debug/examples/verify_embeddings data/models/multilingual-e5-small
cargo test --locked --workspace --features local-embeddings
cargo clippy --locked --all-targets --features local-embeddings -- -D warnings
```

安装器分块下载，逐文件核对固定大小和哈希，fsync并原子替换；重复安装复用已验证文件。中途失败不能激活不完整资产集。Rust加载器再次逐项核对所有文件，错误只返回固定类别，不输出路径、私有文本或底层执行正文。下载文件在忽略版本控制的data目录。

编码入口区分Query/Passage；1..32条、单条最多16384 UTF-8字节、总量最多256KiB。以同一tokenizer关闭截断后预检，超过512 token拒绝，不以截断向量冒充整条记忆。空白、NUL、异常维数、非有限值或非单位范数拒绝。单条encode拒绝超长输入；encode_memory现提供下节的完整分块方法。服务接入时须在事务外使用有界阻塞任务执行，完成后复查来源、版本和privacy_epoch。

profile包含模型修订、FP32、mean/L2、前缀协议及fastembed/ort/tokenizer版本。更换资产、编码规则或影响向量的依赖必须建立新profile并重建索引，不能混合不同向量空间。

开发smoke使用三条独立合成文本，不使用冻结留出集：相关段落余弦0.905241，无关段落0.784064，重复编码最大差0；384维/单位范数检查与空白、超长、超大batch拒绝通过。这只是该主机上的编码证据，不证明所有语义理解正确，也不是Recall@10或性能基准。

下节记录后续实现的持久分块、精确检索和冻结基线；本开发smoke本身不替代这些交付。


## 持久集合与精确检索

0015使用pgvector 384维列，先不建立近似索引。[pgvector官方说明](https://github.com/pgvector/pgvector)定义`<=>`为余弦距离，默认搜索为精确搜索。每条记忆按Unicode最多256字符、64字符重叠切分，实际token超过512时缩短窗口；纯空白窗口略过，其他字符必须被完整覆盖。编码在事务外，提交快照绑定个体、记忆版本、内容SHA256及privacy_epoch。事务一次替换该profile的集合和全部分块，失败回滚；旧版本和不同profile不会混检。

查询先过滤有效时间、一般/局部scope、关系参与者和递归阻断来源，再按每条记忆最小块距离排序。历史模式允许当时有效、现已被替代的记录；它是valid-time查询，不等同证明个体在当时已经知道这些内容。现有subject_id过滤的是来源交互的关系参与者，并非正文中提到的人物，尚不能作为人物实体解析。旧摘要即使被故障注入直接改文而未增版本，也因内容摘要不符而不再命中；缺块或隐私代次变化同样拒绝。

开发贯通检查（使用真实E5及PostgreSQL，合成资料、无云端调用）：

```sh
cargo build --locked --features local-embeddings --example verify_semantic_store
python3 scripts/test_semantic_live.py
python3 scripts/test_database.py
```

实际长记忆分成3块，无关记忆1块；重建连接后相关记忆排名第一。默认数据库测试新增数值向量不变量用例，显式标注fixture，不把单位向量当作语义模型证明。

## 首次冻结100题基线

```sh
cargo build --locked --features local-embeddings --example evaluate_retrieval
python3 scripts/test_semantic_live.py --evaluate
```

脚本只在自己创建的alia_semantic_test随机数据库运行，结束清理。使用生产索引写入/查询方法和真实本地编码；仅评测的有效时间/状态元数据由受控合成fixture设置，不伪称真实生活历史。人物标注未提供给排序器，所有事件来源交互对象为owner；不会拿正确答案做实体过滤。报告保留原始排名和错误命中。

首次结果：100题宏平均Recall@10=1.0，各类20题均为1.0；禁止原始命中93次，全部属于近名他人记忆（current20、paraphrase19、historical19、local_exception15、compare_scope20）。这证明基线能召回所需条目，同时暴露人物区分不足；并非100%记忆准确率或最终答案使用正确率。M2不据此通过。报告在忽略版本控制的evals/results/retrieval-baseline-v1.json，SHA256 `dd2255d752fd01a2c02cd12ea1b6050f464a938de6f3966e09d28030cb19455a`，冻结语料哈希保持不变。未根据该结果调整编码、排序参数或标注。

下一项是正文人物实体及引用的有依据关联、后台索引调度与服务读路径整合；开发使用另一个数据集。完成后再做明确版本的回归，保留本次缺陷基线，不能用总召回分数掩盖错误来源。

## 人物引用与主体过滤（内部接口）

0016/0017增加独立person实体、带来源别名和记忆主体注记。实体表示对一个人的引用，仍归属于同一个体；它不等于对话关系、AI身份或种群。`external_key`区分同名人的身份，不能因为名字相同就合并。

内部可信调用可注册人物、补充别名或注记记忆主体。注册及别名要求名字确实出现在有效实际消息/资料/已成功读取的文本区间；记忆注记还要求该事件属于记忆来源，记忆正文相应Unicode位置也有同一名字，并检查记忆版本。当前role仅subject/mentioned；同一记忆/人物保留一个当前role，更正追加事件，历史不擦除。内容摘要也随注记保存，防止文本被故障修改后仍套用旧主体。

`resolve_person`仅精确匹配已登记别名：无匹配返回Unknown、多个人物匹配返回Ambiguous，不模糊猜测。`SemanticQuery.person_id`是与subject_id关系参与者不同的主体过滤，只有有效subject注记才匹配。主体/来源/注记事件被阻断后不可借用；实体阻断传播到包含该主体或提及的记忆，是保守的隐私策略。旧词项查询也已使用递归祖先阻断，避免在回退路径泄漏。

这些是字面证据及状态边界，不能证明语法角色理解正确。owner内部方法标记actor=owner、source_kind=reported_claim；模型已另经收费entity_changes提案提交，固定alia/inference，不调用owner接口。HTTP注册仍未开放。没有调用方应把Unknown/Ambiguous静默降为无过滤搜索。冻结评测的subject标注没有被拿来生成这些注记，本轮不重跑或宣称消除93次错源。

独立开发用例使用赵芸/赵云及同名、别名和emoji位置，验证subject与mentioned过滤、更正历史、不同来源/过时版本/改文拒绝、歧义与删除阻断。它们不测自动NER或角色分类；下一步需要更完整的独立语义开发集、索引和服务组合，再回归冻结集。

### 收费人物推断开发试验

entity_changes使用实际可见来源区间、当前/新记忆引用及new/existing/prior人物引用；person_catalog提供有来源且有界的已知别名。记忆中的名字位置可由程序唯一字面匹配，不让模型把来源偏移套到另一份正文；歧义拒绝。全部变更与原decision原子提交，不增加免费分类调用。

独立沈遥/沈瑶开发输入通过真实worker和Go/GLM调用，合成资料不当亲历。v1、v2均因不准确关联而失败，修复后模型省略entity_changes却在回复宣称已经关联；原始结果保留。v2固定诊断定位到entity_memory_quote。v3取消冗余的强制记忆偏移后，一次调用创建两个近名人物及两条subject注记，均alia/inference；输入4466、输出716、合计5182 token，未冻结预算。人工检查名字与红茶/咖啡归属正确，但每条摘要还括注另一人的事实，且无mentioned注记，不能据此宣称完整人物消歧或召回纯净度已达标。报告live-entities-v1/v2/v3.json保存在evals/results，失败不覆盖。

冻结语料和首次93次错源基线不变，未使用留出集人物标签填表，也未重跑冻结检索。仅内部引用和单个开发场景进步，不是M2通过。

## 自动索引维护与服务配置

0018增加memory_index_jobs，按individual/memory/INDEX_PROFILE保存版本、正文摘要、隐私代次、状态、尝试次数、租约和单调代次。每次确定性扫描最多发现32条有效/历史记忆，逐条执行；只存派生缓存任务，不添加生活经历、目标或收费模型调用。完整有效集合已存在时跳过，缺块或内容/隐私版本变化后重建。重复启动不重新计算已完整索引。

单次租约120秒，编码等待60秒；来源、记忆版本、内容摘要、隐私代次、生命周期与暂停状态在写入前复验，完整向量与ready状态同一事务提交。每份内容/代次最多三次尝试，错误等待60秒；失联租约有限重算，旧代次不能提交。确定性无效输出直接失败；未变内容的失败不会靠重启重置次数。重建仅修改派生索引，不推进个体状态版本。

当前前台任务优先，暂停自主活动或个体非active时不领新任务。计算不持有事务或写锁。空闲10秒检查、存储故障30秒退避；无待处理记忆不运行编码。程序能中止异步等待，不能强制打断已经进入原生ONNX的CPU调用；同一编码器的信号量直到原生调用返回才释放，防止超时不断堆叠后台线程。中止后没有持久化权限，任务通过租约恢复；硬进程级CPU隔离仍待能力执行阶段。

原生服务启用方式：

```sh
python3 scripts/install_embeddings.py
cargo build --locked --features local-embeddings --bin alia
# DATABASE_URL、ALIA_INSTANCE_TOKEN等仍按现有部署配置提供。
ALIA_EMBEDDING_DIR=./data/models/multilingual-e5-small target/debug/alia
```

不设置ALIA_EMBEDDING_DIR则关闭worker；显式空值、缺失/损坏资产、未编译feature却请求启用均报错。不会静默宣称编码器可用。/state.indexing提供enabled、profile和实际读取策略；当前配置同时连接索引维护与共享混合读取，未配置时保留lexical。每次查询独立报告实际模式和索引覆盖；见[共享检索契约与实验](MEMORY_RETRIEVAL.md)。

可选Compose覆盖（保留现有Go tokenizer与开发数据库映射）：

```sh
sudo docker compose -f compose.yaml -f compose.dev.yaml -f compose.go-tokenizer.yaml -f compose.embeddings.yaml build app
sudo docker compose -f compose.yaml -f compose.dev.yaml -f compose.go-tokenizer.yaml -f compose.embeddings.yaml up -d --wait app
```

Docker通过ALIA_CARGO_FEATURES构建参数启用feature，E5目录以只读挂载，缺失目录不自动创建；运行镜像明确提供libstdc++6。实测Bookworm构建因ONNX预编译库依赖的__isoc23_strto*及新版C++符号链接失败，因此此覆盖通过ALIA_RUST_IMAGE/ALIA_BASE_IMAGE单独选择Trixie构建和运行；默认无嵌入镜像继续使用Bookworm。覆盖不改变模型凭证、人格或当前实例数据。

真实本地服务验证：

```sh
cargo build --locked --features local-embeddings --bin alia --example verify_index_worker
python3 scripts/test_index_service.py
```

以下为索引维护阶段的历史证据，当前读取升级另见MEMORY_RETRIEVAL。

实际index-service-v1.json（SHA256 5c92ccecf76238dd3f3a7ef2970c1265bc5ab42812548abcf206d2bce5082942）使用显式合成来源，未插入向量或任务fixture，两个真实服务进程自动建立3+1块；两个任务均一次成功，重启后attempts/generation/updated_at完全相同；独立真实E5查询正确排列相关段落，无云端调用。缺失资产启动被拒，进程正常关闭。此证据覆盖服务和队列，不是冻结100题回归或完整M2验收；首次93次错源报告保持不变。

容器实际验证已完成：Trixie可选feature镜像构建及启动通过，容器getconf报告glibc 2.41；健康200、无认证401、认证state409/not_initialized，HTML/JS/CSS与构建一致。没有初始化生产个体或写入合成经历。原生服务与容器分别验证，不以某一项代替另一项。

界面分支另行通过真实启用服务检查：--web模式在1440/1024/768/390/360px读取真实enabled/profile、仍为词项的读路径，展开编码版本不产生横向溢出。长记忆列表四行预览，选中后检查器保留全文和来源，避免长记录撑满列表。结果index-service-ui-v3.json哈希55bc05b2a94cd4542040c0219bb140f0a5de043102c4d73ed60dd7da9e0257fc，截图index-enabled-1440/390.png。

保留index-service-ui-v1.json失败记录：其他默认构建替换了共享target/debug/alia，无feature路径拒绝启动。测试器现显式构建feature并把两个可执行文件复制到独立临时目录，使重启使用同一二进制；v2/v3通过。现有报告拒绝覆盖，复验须使用新的--output路径；不得静默改写历史证据。可用`python3 scripts/test_index_service.py --web --output evals/results/index-service-next.json`执行新的隔离试验。
