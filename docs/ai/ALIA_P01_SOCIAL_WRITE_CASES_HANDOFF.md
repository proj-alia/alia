# P01-SOCIAL-WRITE-CASES-01｜离线场景材料就绪

新增8组中文合成场景，覆盖关系新证据、短期失败与长期承诺、缺席、分歧修复、历史与当前、第三方转述、多目标审查、拒绝与费用。分别区分实际事件、短期评价、长期态度和关系理解；每组列前置来源、拟议变化、允许/禁止结果、程序断言和独立语义复核问题。所有verdict及正式验收为null，状态not_run。

材料仅准备未来验收，不验证正在变化的生产写入接口。root计划的appraisal≤1、attitude≤2、relationship≤2及packet9应在未来固定版本上显式绑定；旧状态只供比较，新状态审查必须逐目标覆盖自身实际event_ids。拒绝后原子不写，已结算和未知费用如实记录。没有用关键词给自然语言打分，也没有将合成标签当独立评价。

`absence_observation` 是待真实采集的缺席观察条件，逻辑ID是材料标签，不预设实际事件或以缺席证明拒绝；未来必须由受授权控制器提供实际来源/时间，缺失则未验证。其它逻辑事件ID同样不是数据库UUID或实际经历。场景为作者拟定，尚未独立验证质量。

实际验证（项目根）：` .venv/bin/python evals/p01/social_write_cases_check.py` exit0；`.venv/bin/python -m unittest discover -s evals/p01 -p social_write_cases_test.py -v` exit0，4通过；`PATH="$PWD/.venv/bin:$PATH" python3 scripts/check_phase0.py` exit0。无真实或合成模型调用、无DB、无部署。没有运行真实行为、语义、费用、并发或写入验收，不能由结构检查推断通过。

文件SHA256：

- `evals/p01/social_write_cases_v1.json`：`761e31d5be41e7c1de091016761a88140ce7c3755d8bb9f949d3997e86d58baf`
- `evals/p01/social_write_cases_check.py`：`ae556682f3d35a0d77089d7ace14ea3c7f6b6539faa0e3454fa2d427bf1549ff`
- `evals/p01/social_write_cases_test.py`：`8f7631707122c74161c1ac8fdacba537b37bffe7103aa54a7e15945dea67a967`

下一依赖：root冻结并明确释放实际社会状态写入版本后，另做独立负例审查和实际回执绑定；本任务不从changing live构造PG通过。既有60×3、20组ABC、正式检索集与collector未改，完整M5候选前不新增收费trial。
