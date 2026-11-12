# Acceptance Matrix — M1/M2 验收矩阵（M0 冻结草案，执行列待填）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 规则：每条测试明确【输入 / 预期 / 实际 / 状态 / 证据位置】。状态四态：`✅通过 / ❌失败 / ⚠️环境限制 / ⏳未执行`（负责人裁定新增 ❌：真实失败必须有归属，不得并入 ⚠️）。**未运行的测试永远标 ⏳，不得写成通过。**
> 固定样例（主控提示词 §9）：订阅 A=USD 20/月；订阅 B=USD 120/年 → 月度等效 USD 30（B 折算 10 + A 20），但 USD 30 不得显示为当月实际扣款；新增 CNY 订阅时不得与 USD 混合求和。

## 1. 样例库定义（所有测试共用 fixture）

| ID | 工具 | 订阅 | 字段 |
|---|---|---|---|
| F1 | manual · FreeTool | free | 无金额 |
| S-A | manual · ToolA | subscription | USD 2000 minor / monthly / active / renews_on=+3d |
| S-B | manual · ToolB | subscription | USD 12000 minor / yearly / active / 无 renews_on |
| S-C | manual · ToolC | subscription | CNY 4000 minor / monthly / active |
| S-D | manual · ToolD | usage | 无金额/无账单 |
| S-E | manual · ToolE | subscription | amount=NULL（未知金额） |
| S-F | detected(detected item) | 无订阅 | 检测 installed |
| MERGE | detected ×2（trae + trae-cn） | 1 条订阅 | 合并场景 |

## 2. 场景矩阵（主控提示词 §9 的 17 条 → 具体测试）

| # | 场景 | 测试载体 | 输入 | 预期 | 状态 | 证据 |
|---|---|---|---|---|---|---|
| 2.1 | 空 Stack | core 单测 + vitest | 空库 | stack_items=[]，SpendingOverview 各组为空但结构完整，EmptyState 引导文案 | ⏳ | |
| 2.2 | 单个免费工具 | core | F1 | 计入 free 计数，不进任何成本合计 | ⏳ | |
| 2.3 | 单个月付订阅 | core | S-A 单独 | monthly_eq=2000 minor(USD)，annual_eq=24000 | ⏳ | |
| 2.4 | 年付折算 | core | A+B | monthly_eq = 2000+1000=3000 minor = **USD 30**；annual_eq=36000；**UI 断言**：月度等效卡片带"≠当月扣款"说明（vitest 文案断言） | ⏳ | |
| 2.5 | 多币种分别统计 | core | A+B+C | USD 组 3000 minor；CNY 组 4000 minor；**无跨币种合计字段**（struct 断言） | ⏳ | |
| 2.6 | 未知金额不计为零 | core | S-E | incomplete 组含 S-E；USD 合计仍 3000（不含 E）；UI 显示"金额未知" | ⏳ | |
| 2.7 | 按量无账单 | core | S-D | usage_without_bills 组；不估算 | ⏳ | |
| 2.8 | 多处检测不重复计费 | core + service | MERGE 合并后 | 订阅归属原条目不动（I5）；合并组聚合计数 1 次；合并前后组内订阅数与总成本恒等（断言前后两次 spending 快照相等）；子条目显示"已并入"；组内若另有第 2 条订阅则两条均计入、不自动去重（R1 提示"潜在重叠"） | ⏳ | |
| 2.9 | 卸载后订阅保留 | core | F 模拟 agent not_installed → refresh | stack_detections 更新、detection_state 变化；subscription 行数不变（断言） | ⏳ | |
| 2.10 | CRUD 与恢复 | core | 建改取消暂停删除 | 状态流转正确；cancelled 不进合计；硬删需确认（UI ConfirmDialog vitest） | ⏳ | |
| 2.11 | 重复提交与并发修改 | core 集成 | 同 id upsert×2；两线程同时写；外部持 runtime.lock 时写 | upsert 幂等（同结果）；串行化无交错；busy 返回明确错误 | ⏳ | |
| 2.12 | 迁移/备份/失败回滚/恢复 | tests/stack_migration.rs | V1-V3 fixture 库 | user_version=4；既有表数量不变（13+5+4=22 张）+ 行数抽查；integrity ok；备份 manifest 存在；人为注入 DDL 失败→回滚+user_version 仍 3（E2 已证模式）；**P0-5 恢复路径**：对 V4 库 restore 迁移前备份→user_version 回 3、V4 表消失、数据完整、可再次迁移到 V4；旧版本读到 user_version=4 库不降级不破坏（断言测试） | ⏳ | |
| 2.13 | GUI/CLI 服务一致 | CLI 集成 + vitest | 同一库同一请求 | 比较**规范化业务数据**：CLI `--json` 信封 data 与 Tauri `run_api` data 的业务字段等价（`requestId` 信封差异不计入比较——两侧请求天然各自生成；对 data 做 JSON 规范化后逐字段比较） | ⏳ | |
| 2.14 | 建议证据不足降级 | core（M2） | 无 overlap_group / 无阈值 / 缺字段订阅 | R1/R2 静默；建议带 missing_information；R5 出数据缺口清单 | ⏳ | |
| 2.15 | Potential ≠ Realized | M2 vitest + core | 模拟结果展示 | 无"已节省"字样；Realized 位固定显示无账单证据文案（断言） | ⏳ | |
| 2.16 | 无网络/无敏感泄露 | M1-C/M2-C 检查 | grep + 诊断导出检查 | 新模块无 reqwest/ureq；export_diagnostics 不含 stack/subscription 内容；新表无 secret 列 | ⏳ | |
| 2.17 | 现有功能零回归 | 全量测试 | 现有 109 core + CLI + 前端测试 | 全绿（Keychain 环境限制项单独标注 ⚠️）；v31_acceptance.sh 通过；home 目录哈希不变断言 | ⏳ | |

## 3. 追加专项

| # | 场景 | 测试 | 预期 | 状态 |
|---|---|---|---|---|
| 3.1 | 折算除不尽取整 | core（如 CNY 5000/半年 → 月 5000/6 ≈ 833.33 → **833**，四舍五入半进位） | 取整 + UI"折算取整"标注；半进位规则与 DATA_MODEL §4 一致 | ⏳ |
| 3.2 | merge 环/多级防护 | core | A→B 且 B→A（服务层拒绝）；merge 目标为已合并子条目；merge 进自身；对 merged 子条目 upsert 订阅 | 全部被拒 `Error::ResourceConflict`（message 只含原因与计数，无路径/金额明细）；不迁移数据使多级合并结构性不可能（目标必须 merged_into IS NULL） | ⏳ |
| 3.3 | 删除前置检查 | core（4 case） | 删除有 active / cancelled 订阅的工具；删除仍有 merged 子项的工具；删除干净条目 | 前三者被拒 `Error::ResourceConflict`（三态契约，ARCHITECTURE §7）；干净条目成功 | ⏳ |
| 3.4 | 续费窗口边界 | core（renews_on=today / today+N / N+1） | 只含前两者；窗口参数生效 | ⏳ |
| 3.5 | 规则引擎确定性 | core | 同输入两次调用输出 JSON 相等（recommendation_id 为确定性 ID，COST_OPTIMIZER §2） | ⏳ |
| 3.6 | i18n 镜像完整性 | vue-tsc（en=typeof zh 编译期强制） | pnpm build 通过 | ⏳ |
| 3.7 | 金额整数溢出防护 | core | i64 上界用例 + `checked_mul` ×12 溢出返回 `Error::invalid_input` 不 panic（存储按 SQLite INTEGER=i64，M6） | ⏳ |
| 3.8 | 合并后刷新不重建 | core | trae 条目并入 trae-cn → 刷新检测 → 断言 stack_items 仍 1 条目、无新 detected 重复项（C2/I2 挂靠规则） | ⏳ |
| 3.8b | 刷新幂等（P0-3） | core | 同一输入连续 refresh 两次 | stack_detections 行数与 id 不变（adapter_key UPSERT，I2a）；无删除重建 | ⏳ |
| 3.9 | JSON 安全整数边界（P0-1） | core + 契约 | minor = 2^53-1（合法入库）；minor = 2^53（拒绝）；minor = 2^63-1（拒绝） | 上界内 i64↔JSON number 双向无损；越界 `Error::invalid_input`，不静默截断（E3 实测 MAX_SAFE < i64 max） | ⏳ |
| 3.10 | 币种精度表（P0-1） | core | JPY 订阅（exponent=0）金额 1000 minor | 显示 ¥1,000（非 ¥10.00）；exponent 表按 ISO-4217 维护 | ⏳ |
| 3.11 | 聚合取整顺序（P0-1） | core | 两条 CNY 1000 半年付 | 组月度等效 = 精确和取整 = 2000/6 = **333**，而非逐条取整和 167+167=**334**（先精确求和后取整一次，DATA_MODEL 聚合规则 9） | ⏳ |
| 3.12 | 周期未知不折算（P0-6） | core | 金额已知但 billing_period=unknown；hybrid 有金额无周期 | 不进月度/年度等效；进 incomplete 组；UI 说明"周期未知无法折算" | ⏳ |

## 4. 执行方式与证据归档

- Rust：`./scripts/cargo.sh test -p agenthub-core --test stack_subscriptions --test stack_migration`（绕开 `tests/mcp.rs` 的 Keychain 沙箱失败；全量复跑在正常终端进行并记录）。
- 前端：`pnpm test` / `pnpm build`。
- 验收脚本：`AGENTHUB_HOME=/tmp/test/home ./scripts/v31_acceptance.sh` 仅作现有功能回归参考；**P0-5 冻结**：现有脚本会读取真实 `~/.agenthub` 数据库与本机 Agent 目录（M0_BASELINE_AUDIT §测试基线），**本模块新增验收一律用纯 fixture 脚本**（隔离临时 AGENTHUB_HOME + 测试构造的 fixture 库，M1-C 交付），不依赖真实环境；不执行 real-acceptance.sh。
- 证据：测试名 + 输出文件/提交哈希，填入"证据"列；最终在 M1-C/M2-C 把本矩阵转成带结果的正式验收报告（延续 docs/VALIDATION.md 的三态标记惯例）。
