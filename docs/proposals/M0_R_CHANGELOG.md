# M0-R Changelog — 设计修订记录（逐条前后差异）

> 裁定来源：负责人 M0 审核（CONDITIONAL GO，批准 M0-R：设计修订与前置验证，暂缓 M1-A）。
> 基准：master@b5ef694（tag v0.5.0）。所有修订限于 docs/proposals/ 与 docs/CHANGELOG.md（D-3 授权的文档修正），未创建迁移、未改业务代码/GUI/CLI、未读写真实用户数据、未打 tag。

## 1. D-1：CLI 写入机制与变更记录（原方案 → 冻结契约）

| 维度 | 修订前 | 修订后 |
|---|---|---|
| 写执行 | "直接执行 + `--yes` 门禁 + operations 登记"（待批准） | **冻结**：本地 CRUD 直接执行；delete/merge 高影响操作显式确认，CLI 非交互高影响必须 `--yes` |
| 变更记录 | 同时写 operations（kind=stack_update 等 4 个新 kind）+ audit_events | **只写 audit_events**（event_key 如 `stack.merge`/`subscription.upsert`），**不写 operations、不扩展 OperationKind** |
| 依据 | — | 契约核验：前端 types.ts:29-41 严格联合类型反序列化失败；`OperationKind::parse` 未知回退 Scan（model.rs:1085）误标；operation_view（service.rs:1702-1730）绑定 plan/approval/rollback 语义；audit_events 列含 `event_json_redacted`（db.rs:305/1782） |

波及文件：ARCHITECTURE §1/§2.1/§2.2/§3 注释/§5；DATA_MODEL I5 审计句；MY_AI_STACK_PRD FR-2.1/N2；M0_BASELINE_AUDIT §3。

## 2. D-2：错误契约（原方案 → 冻结契约）

| 维度 | 修订前 | 修订后 |
|---|---|---|
| 参数非法 | `Error::invalid()`（实际映射 **InvalidSkill**，error.rs:45-47——语义陷阱） | 新变体 `Error::InvalidInput(String)` + `invalid_input()` → 既有 `ErrorCode::InvalidInput`（exit 3） |
| id 不存在 | `Error::not_found()` → InvalidInput | 不变（既有映射保留，瑕疵登记技术债） |
| 存在但不能操作（删除有订阅/有子项、merge 非法目标、merged 子条目 upsert 订阅） | `Error::invalid("...")`（被负责人否决：不得伪装成 InvalidInput） | 新变体 `Error::ResourceConflict(String)` + `resource_conflict()` → 新增 `ErrorCode::ResourceConflict`（exit 4，与 PlanStale 同族） |
| 既有映射 | — | **零改动**（加法扩展，编译器强制检查 From 表穷尽） |

波及文件：ARCHITECTURE §3 注释/§7（重写）；DATA_MODEL I4；MY_AI_STACK_PRD FR-1.6；COST_OPTIMIZER §2；ACCEPTANCE 3.2/3.3/3.7；IMPLEMENTATION_PLAN M1-A/M1-B。

## 3. D-3：版本序列（已执行）

- 修订前：`docs/CHANGELOG.md:5`「## 0.5.1 — 2026-10-29（生产安全修复切片，未发布）」。
- 修订后：「## 0.5.0（生产安全修复切片） — 2026-10-29（随 v0.5.0 tag 一并发布，未单独发版）」+ D-3 裁定引注。内容逐字保留。
- 结论：不补造 0.5.1 Release、不移动 tag；My AI Stack 后续按 0.6.0 规划，发布需单独授权。
- 波及文件：docs/CHANGELOG.md；M0_BASELINE_AUDIT §1.1 + 不一致清单 D8 行（改为"已裁决"）。

## 4. P0-1 金额折算

- 修订前：公式已在前轮修正（monthly=minor/period_months），但①年度等效括注 "= monthly_equivalent_raw × 12" 有误导；②未定义聚合取整顺序；③未定义 JSON 传输上界与币种精度。
- 修订后：年度等效**直接从原始 minor 计算、恒为精确整数**（12 可被 {1,3,6,12} 整除）；聚合**先精确求和后取整一次**（含反例：两条 CNY 1000 半年付，逐条取整和 334 ≠ 精确 333）；JSON 输入校验 minor ≤ 2^53-1（越界 → invalid_input）；ISO-4217 exponent 静态表。
- 波及文件：DATA_MODEL §4；ACCEPTANCE 3.9/3.10/3.11（新增）；IMPLEMENTATION_PLAN M1-A 测试清单。

## 5. P0-2 合并设计（重写）

- 修订前：合并事务内**迁移订阅 FK 到主条目**，撤销按 operations payload 快照迁回；聚合"按主条目、排除子项"；未防多级合并。
- 修订后：**不迁移任何数据**——merge = 一条 `UPDATE merged_into`；聚合按合并组遍历原始订阅；合并前后订阅数与成本恒等；组内不自动判重复扣费；unmerge = 一条 UPDATE 天然幂等；目标必须 `merged_into IS NULL` 且非自身 → 结构性禁止多级与自环；变更记录走 audit_events。
- 波及文件：DATA_MODEL I5（重写）+ 聚合规则 8；ARCHITECTURE §5/§8 威胁表；MY_AI_STACK_PRD FR-1.4；COST_OPTIMIZER R1（合并组内不自动判重复）；ACCEPTANCE 2.8/3.2；IMPLEMENTATION_PLAN M1-A 测试。

## 6. P0-3 检测稳定身份（重写）

- 修订前：`UNIQUE(stack_item_id, adapter_key)`；refresh"全量替换"。
- 修订后：`UNIQUE(adapter_key)`（全局唯一）+ **按 adapter_key UPSERT**（I2a 冻结）；挂靠规则不变；挂靠条目被删 → CASCADE 后重建（唯一 id 不稳定场景，明确登记）；验收 3.8b 幂等断言。
- 波及文件：DATA_MODEL I2a（新增）+ §2 + §3；MY_AI_STACK_PRD FR-1.1；ACCEPTANCE 3.8b（新增）；IMPLEMENTATION_PLAN M1-A 测试。

## 7. P0-4 删除约束（统一口径）

- 修订前：数据模型写"无 active 订阅即可删除"，与 FK RESTRICT（任何状态都拦，E1a 实证）矛盾。
- 修订后：**任何状态的订阅都阻止直删**（服务层预检覆盖全部状态 + merged 子项检查 → `ResourceConflict`），提供解除关联/先删订阅引导；FK RESTRICT 兜底。
- 波及文件：DATA_MODEL I4；MY_AI_STACK_PRD FR-1.6；ACCEPTANCE 3.3。

## 8. P0-5 迁移与测试隔离

- 修订前：回滚仅写"迁移失败→事务回滚+备份保留"；未声明不可 git-revert 降级；未提供纯 fixture 验收。
- 修订后：**冻结声明**——SCHEMA_V4 升级后无 git revert 式数据库降级；恢复路径唯一（restore 迁移前备份，4 步断言进测试）；新增验收**纯 fixture 模式**（M1-C 新增 `scripts/stack_acceptance.sh`，不读真实 `~/.agenthub`/Agent 目录）；v31_acceptance.sh 降级为既有回归参考。
- 波及文件：DATA_MODEL §6；M0_BASELINE_AUDIT §测试基线表；ACCEPTANCE 2.12（恢复路径）/§4；IMPLEMENTATION_PLAN M1-A 测试/M1-C 范围/执行命令备忘。

## 9. P0-6 成本语义

- 修订前：聚合规则 1 只排除 amount/currency 缺失；hybrid 说明含糊；billing_period unknown 未显式处理。
- 修订后：`billing_period ∈ {unknown, usage}` 或缺失 → 不折算进月/年等效（金额已知也进 incomplete，E3 CASE7/8）；hybrid 仅已知固定费参与，UI 注明浮动未计（R2 联动）；R5 新增"金额已知但周期未知"缺口类型。
- 波及文件：DATA_MODEL 聚合规则 1；COST_OPTIMIZER R2/R5；ACCEPTANCE 3.12（新增）。

## 10. 契约修复 5 项

1. 验收状态三态 → **四态**（新增 `❌失败`）：ACCEPTANCE 头注 + IMPLEMENTATION_PLAN M1-C。
2. GUI/CLI 一致性：信封字节等价 → **规范化业务 data 字段比较，requestId 不计入**：ACCEPTANCE 2.13 + IMPLEMENTATION_PLAN M1-B 测试。
3. Recommendation ID：随机 uuid → **确定性 ID**（`rec_` + sha256(规则类型+related ids+参数快照)[:12hex]）：COST_OPTIMIZER §2（支撑 3.5 确定性测试成立）。
4. 文档口径统一：索引数 4 → **5**（逐个列名）；CLI 写命令数 7 → **8**（stack 5 + sub 3）；M1-A error.rs/contract.rs 进文件范围。
5. evidence_json 隐私：删去"路径字符串，非敏感"定性 → **准身份数据**（仅本地存储、不进日志/审计/诊断导出）：DATA_MODEL §2 注、SECURITY 表 2 行、ARCHITECTURE §8 威胁表。

## 11. 前轮（裁定前）已并入本版的 15 项评审修复

公式改正、刷新挂靠规则、i64 对齐、M1-B 范围收缩（11 Tauri 命令）、unchecked_transaction 计数 5 处（含 service.rs:2579）、⌘1-9 末尾追加、OperationKind 蛇形命名、billing_type=unknown、stale_renewals 组、status=unknown 聚合可见、superseded_by ON DELETE SET NULL、基准 commit 戳等——细节见各文档内标注。
