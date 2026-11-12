# M0-R P0 Verification — 逐项复现、修复设计与验证证据（M0-R 冻结交付物）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。
> 验证环境：sqlite3 3.50.2 CLI + python3 Decimal，全部在 `/tmp` 隔离目录执行（未触碰真实 `~/.agenthub` / Agent 目录）。
> 每项 P0 结构：复现 → 修复设计 → 验证证据。

---

## P0-1 金额折算公式与精度

**复现**：负责人裁定指出 DATA_MODEL_PROPOSAL 原 106 行 `monthly_equivalent = amount × 12 / period_months` 实为年度等效公式（USD 20/月 + USD 120/年 会被算成 USD 250/月，正确为 USD 30）。

**修复设计**（DATA_MODEL §4 冻结）：
- monthly_equivalent = `minor / period_months`；annual_equivalent = `minor × 12 / period_months`（从原始 minor 直接计算，**永不由已取整的月度值反推**）。
- 12 可被 {1,3,6,12} 整除 → 年度等效**恒为精确整数、永不取整**；取整（round-half-up）只出现在月度等效展示层。
- 聚合取整顺序：组内先对精确年度值求和，再 `annual_total / 12` **取整一次**（聚合规则 9），禁止逐条取整后求和。
- 精度：`amount_minor` i64 存储 + ISO-4217 exponent 静态表；JSON 输入校验 `minor ≤ 2^53-1`（越界 → `Error::invalid_input`），输出在该范围内 i64 ↔ JS Number 双向无损。

**验证证据（E3，python3 Decimal ROUND_HALF_UP）**：
- USD 2000/月 + 12000/年 → monthly_equiv=3000（USD 30）、annual_equiv=36000（USD 360）✅
- 年付 12000 → 年度等效 = 12000 == 原值（True）——取整不扭曲年度账单 ✅
- 除不尽：5000/6→833、2500/6→417（单条展示取整）✅
- MAX_SAFE = 9,007,199,254,740,991 < i64 max = 9,223,372,036,854,775,807 → 存储 i64、JSON 输入校验 ≤ 2^53-1 ✅
- 聚合反例：两条 CNY 1000 半年付——逐条取整和 167+167=334，精确和取整 2000/6=333 → 必须采用精确法（写进验收矩阵 3.11）✅
- 周期 unknown / usage 的订阅 → monthly_equiv=None（进 incomplete / usage_without_bills，P0-6）✅

---

## P0-2 合并与撤销可逆性

**复现**：原 I5 设计"合并事务内迁移订阅 FK 到主条目 + 审计快照迁回"，负责人裁定：缺少原始归属保存时撤销不可靠，且成功操作写入 operations 会让操作历史语义不可信。

**修复设计**（DATA_MODEL I5 冻结）：**合并 = 一条 UPDATE `SET merged_into=<主条目>`，订阅 FK 永不迁移**；聚合按合并组遍历（组 = 主条目 + merged 子条目）的**原始订阅**；unmerge = 一条 `SET merged_into = NULL`，天然幂等可逆、无需快照回放；组内两条不同订阅不自动判重复扣费（仅 R1 提示"潜在重叠"）；服务层强制"目标必须 merged_into IS NULL 且非自身"→ 结构性禁止多级合并与自环。变更记录走 `audit_events`（D-1），不写 operations。

**验证证据**：
- E1c（sqlite3）：主条目硬删时子条目 `merged_into` 因 ON DELETE SET NULL 变 NULL（child|<null>）——不级联删除，删除语义符合设计 ✅
- E3 对照：合并前后组内订阅行数与成本恒等由"数据未动"结构性保证（无迁移即无漂移），验收矩阵 2.8 断言两次 spending 快照相等 ✅
- 多级/环防护：merge 目标必须 merged_into IS NULL——子条目永远不是合法目标 → 深度恒 ≤1，环不可能形成（3.2 仍保留服务层显式拒绝用例作防回归断言）✅

---

## P0-3 检测稳定身份

**复现**：原 `UNIQUE(stack_item_id, adapter_key)` 只约束"同一条目内不重复"，同一 adapter 仍可挂到不同条目各建一行；刷新挂靠若用删建方式会改变检测行 id、破坏稳定映射。

**修复设计**（DATA_MODEL I2a + §2 冻结）：`stack_detections.adapter_key` **全局 UNIQUE**（每适配器全库最多一条）；refresh 写入 = `INSERT ... ON CONFLICT(adapter_key) DO UPDATE SET stack_item_id=..., ...`（**UPSERT 稳定身份**）；挂靠规则（I2 ①-④）不变——优先挂既有未确认 detected 条目，挂不到才新建条目；已确认/已合并条目永不自动产生重复项。挂靠条目被删 → 检测行随 CASCADE 消失，下次 refresh 重新 INSERT（唯一 id 不稳定场景，明确登记）。

**验证证据**：
- E1b（sqlite3）：`UNIQUE(adapter_key)` 下同适配器第二行被拒（UNIQUE constraint failed）——全局唯一约束可强制 ✅
- 幂等断言进验收矩阵 3.8b：同输入连续 refresh 两次 → 检测行数与 id 不变（M1-A 实现后执行）⏳
- 挂靠稳定性验收 3.8：合并 → 刷新 → 仍 1 条目（M1-A 实现后执行）⏳

---

## P0-4 删除约束统一

**复现**：数据模型写"无 active 订阅即可删除"，但 FK `RESTRICT` 对任何状态订阅都硬拦（E1a）——服务层预检与数据库约束口径不一致，会导致用户撞 Database 原始错误。

**验证证据（E1a，sqlite3）**：对挂有 `status='cancelled'` 订阅的 stack_item 执行 `DELETE` → `FOREIGN KEY constraint failed (19)`——证实**任何状态**（active/paused/cancelled/expired/unknown）的订阅都阻止直删。

**修复设计**（DATA_MODEL I4 + ARCHITECTURE §7 冻结）：服务层预检覆盖全部状态 + merged 子项检查，统一口径"存在任何关联订阅（任何状态）→ 禁止直删"；返回 `Error::ResourceConflict`（message 只含计数与原因），提供解除关联/先删订阅的引导文案；FK RESTRICT 保留为兜底（正常路径不会撞到）。

---

## P0-5 迁移与测试隔离

**复现**：SCHEMA_V4 升级后 user_version=4 且 DDL 已提交——SQLite 无逆向迁移，`git revert` 代码不能降级数据库；且现有 `v31_acceptance.sh` 虽隔离写入，仍会**读取**真实 `~/.agenthub` 数据库副本与本机 Agent 目录（M0_BASELINE_AUDIT §测试基线表 :114 行），不满足"纯 fixture 验收"。

**修复设计**（DATA_MODEL §6 冻结）：
1. 迁移前强制 `backup_before_migration`（VACUUM INTO 快照）；
2. 恢复路径唯一且显式：restore 迁移前备份 → user_version 回 3 → V4 表消失 → 可再次迁移（M1-A 新增断言测试）；
3. 应用回退读到 user_version=4 的库：现有 migrate() 逐版本机制不降级不破坏（M1-A 补断言测试，不新增降级逻辑）；
4. 每步 integrity_check；
5. 新增验收一律**纯 fixture 模式**（隔离临时 AGENTHUB_HOME + 测试构造 fixture 库），不读真实环境。

**验证证据（E2，sqlite3 /tmp 隔离库）**：
- 迁移事务内注入语法错误 → 回滚干净：既有数据行数不变（existing_data=1）、V4 新表不存在（new_v4_table=0）、user_version 保持 3 ✅
- `VACUUM INTO` 备份生成且可读（BACKUP_READABLE）✅
- 纯 fixture 验收脚本列入 M1-C 交付（`scripts/stack_acceptance.sh`）⏳

---

## P0-6 成本语义

**修复设计**（DATA_MODEL §4 聚合规则 + COST_OPTIMIZER R2/R5 冻结）：
- 固定费（subscription/known hybrid base）与按量费（usage）分列：usage 无账单 → `usage_without_bills`，绝不估算；
- hybrid：仅已知固定费参与折算与阈值比较，UI 注明"浮动部分未计"；
- `billing_period ∈ {unknown, usage}` 或缺失 → **不折算**，金额已知也进 incomplete（E3 CASE7/8）；
- 取整顺序与年度精确性见 P0-1；
- 不同币种按 currency 分组，组间永不相加、无汇率功能。

**验证证据**：E3 CASE7（unknown period→None→incomplete）、CASE8（usage→None→usage_without_bills）✅；多币种断言进验收矩阵 2.5（struct 无跨币种合计字段）⏳。

---

## D-1 / D-2 / D-3 冻结契约（契约核验证据）

**D-1（有条件批准→冻结）**：本地 CRUD 直接执行；delete/merge 高影响需显式确认（CLI 非交互必须 `--yes`）；变更记录**只写 audit_events**。
核验证据（只读仓库审计）：
- operations 消费面绑定 plan/approval/rollback 语义：`operation_view`（service.rs:1702-1730）反序列化 row.plan 为 OperationPlan、查 approval_state/rollback_of；dashboard recent_operations（service.rs:2195）；CLI cmd_operations_*（main.rs:646-650）；前端 stores/operations.ts + PlanDialog.vue——无快照无回滚步骤的"成功操作"会污染该语义。
- 前端 `OperationKind` 为严格字符串联合（src/lib/types.ts:29-41），后端新增 kind 会使前端反序列化失败；`OperationKind::parse` 未知字符串回退 `Scan`（model.rs:1085）会误标。
- `audit_events` 载体完备：表 db.rs:305（含 `event_json_redacted` 列）、`insert_audit_event`（db.rs:1782）、`insert_audit`/`latest_audit`/`list_audits`（db.rs:2033/2052/2083）。
- 未来扩展 OperationKind 的前置条件（写进 ARCHITECTURE §5）：核验全部消费者、状态机与恢复逻辑兼容。

**D-2（原方案不批准→修订冻结）**：新增 `Error::InvalidInput` / `Error::ResourceConflict` 变体 + `ErrorCode::ResourceConflict`（exit 4），三态区分；既有映射零改动。
核验证据：`Error::invalid()` 实际映射 `Error::InvalidSkill`（error.rs:45-47）→ `INVALID_SKILL` → exit 3——原方案会把状态冲突伪装成"技能非法"，语义陷阱证实；`Conflict→PlanStale`（contract.rs:105）保留给计划失效；`NotFound→InvalidInput` 既有映射不动（瑕疵登记技术债）。

**D-3（批准方案 A→已执行）**：`docs/CHANGELOG.md` 原「0.5.1 — 未发布」节已改题为「0.5.0（生产安全修复切片）— 随 v0.5.0 tag 一并发布，未单独发版」+ 裁定引注；不移动 tag、不补造 Release；My AI Stack 按 0.6.0 规划。

---

## 状态汇总

| 项 | 状态 | 证据等级 |
|---|---|---|
| P0-1 公式/精度/取整 | **已关闭（设计冻结+算术验证）** | E3 实测 + 设计冻结 |
| P0-2 合并可撤销 | **已关闭（设计冻结）** | E1c 实测 + 结构性论证 |
| P0-3 检测稳定身份 | **已关闭（设计冻结）** | E1b 实测；幂等测试待 M1-A 执行（⏳） |
| P0-4 删除约束 | **已关闭（设计冻结）** | E1a 实测 |
| P0-5 迁移/恢复/隔离 | **已关闭（设计冻结+迁移回滚实测）** | E2 实测；fixture 脚本与恢复路径测试待 M1-A/M1-C（⏳） |
| P0-6 成本语义 | **已关闭（设计冻结）** | E3 CASE7/8 |
| 契约修复 5 项 | **已关闭** | 文档修订 + 仓库审计 |

> 注：本文件是**设计验证**交付物。所有 ⏳ 项均为 M1-A/M1-C 的实现期测试，M0-R 不创建正式迁移、不修改业务代码——设计冻结不等于已实现或已通过产品验收。
