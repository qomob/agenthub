# Architecture Proposal — My AI Stack / Cost Optimizer（M0 冻结草案）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 原则：最小侵入。所有新逻辑进 `agenthub-core`；GUI/CLI 是薄前端；新命令走 `ApiResult<T>` v3.1 信封；加法迁移；完全离线。

## 1. 模块职责图

```
┌─────────────────────────── src-tauri (GUI) ───────────────────────────┐
│ lib.rs: +8~10 个 #[tauri::command]（stack_* / subscriptions_* /       │
│ spending_overview / optimizer_*，全部走 state.run_api 信封）          │
│ src/pages/StackOverviewView.vue / StackDetailView.vue                 │
│ src/stores/stack.ts   src/lib/api.ts +stack 段  src/lib/types.ts 镜像  │
└──────────────────────────────┬───────────────────────────────────────┘
                               │ 同一组方法
┌──────────────────────────────┴───────────────────────────────────────┐
│ crates/agenthub-core/src/service.rs — AgentHub（唯一业务引擎）        │
│  新区段 `// -- stacks / subscriptions --`（建议插在 profiles 区段后）  │
│  读方法：薄委托 db.*，不加锁                                            │
│  写方法：write_lock → runtime_guard → db 事务写 → audit_log 登记       │
│  派生：spending_overview()（纯计算）                                    │
├──────────────────────────────────────────────────────────────────────┤
│ crates/agenthub-core/src/optimizer.rs（新，纯函数，模式照 validation.rs）│
│  derive_recommendations(items, subs, params) -> Vec<Recommendation>    │
│  simulate_scenario(req, subs) -> ScenarioResult（无 IO 无锁）          │
├──────────────────────────────────────────────────────────────────────┤
│ crates/agenthub-core/src/db.rs — SCHEMA_V4（stack_items /              │
│ stack_detections / subscription_records 三表 + 索引，加法迁移 v3→v4）   │
└──────────────────────────────────────────────────────────────────────┘
                               │ 同一组方法
┌──────────────────────────────┴───────────────────────────────────────┐
│ crates/agenthub-cli/src/main.rs — +stack / +cost 子命令（gate_plan    │
│ 三件套仅用于未来写扩展；M1 只读命令照 cmd_skills_list / --json 信封）   │
└──────────────────────────────────────────────────────────────────────┘
```

职责边界硬规则（对应主控约束 A/B）：
- **stack/subscription 是应用自身数据库写入，不是 Agent 目录写入** → 不走 ops.rs Executor，不改 ops.rs。仍持锁、仍登记变更，但**只写 `audit_events`，不写 operations 表**（D-1 裁定冻结，见 §5）。`OperationKind` 枚举**不扩展**：前端 `src/lib/types.ts:29-41` 是严格字符串联合（新增 kind 会让前端反序列化失败）、`OperationKind::parse` 对未知字符串回退 `Scan`（model.rs:1085，误标风险）、operations 行语义绑定 plan/approval/rollback（无快照无回滚步骤的"成功操作"会污染操作历史与 dashboard recent_operations 展示）。
- 检测事实复用现有 `sync_agents`/`list_agents`，不新增扫描逻辑、不改 adapters/。
- 金额算术在 core（`util.rs` 或新 `money.rs` 小模块），i64 精确整数（与 SQLite INTEGER 对齐，checked_mul/checked_add 防溢出）；前端只做展示格式化，不做汇总计算（禁止 Vue 页面里实现业务计算）。

## 2. 数据流图

### 2.1 Inventory 流（检测 → 条目）

```
AgentHub::sync_agents() (现有，service.rs:157)
        │ Detection{status,evidence,...} ×11 适配器（只读 home 目录）
        ▼
stack_detections 表（可再生；每次"刷新检测"全量替换，模板照 replace_scan_results 单事务 db.rs:1874）
        │  服务层推导（冻结的三条规则，全部按规则先后短路）：
        │    A 任意 detection detected_status=installed        → installed
        │    B 存在 detection 且 detected_status=not_installed  → not_installed（M3：sync_agents 对
        │      全部 11 适配器都会产出 Detection，NotInstalled 是合法态，adapters/mod.rs:34）
        │    C 无任何 detection                                  → unknown
        ▼
stack_items（用户资产；detected 条目由"检测刷新"自动创建 confirmed=0，挂靠规则见 DATA_MODEL I2：
             优先挂靠"未合并且 confirmed=0 且 source=detected"的既有条目，挂不到才新建，
             已确认/已合并条目永不自动产生重复项；manual 条目由用户创建；用户确认/合并/改名只写这张表）
```

- `stack_refresh_detections()` 是**修改类写操作**：持 `write_lock + runtime_guard` 双锁并写 `audit_events`（event_key=`stack.refresh`），与 §5 锁策略一致（M3）。
- 卸载 → stack_detections 按 adapter_key UPSERT 后按规则 A/B/C 推导 detection_state；**stack_items 与 subscription_records 永不被检测流程删除**（FR-2.4）。

### 2.2 Subscription 流

```
用户表单（GUI BaseModal / CLI 参数）
  → service.subscription_upsert(req)          [write_lock + runtime_guard]
      ├ 校验（金额/币种配对、周期枚举、日期格式、minor ≤ 2^53-1）→ Error::InvalidInput
      └ 单事务写 subscription_records + audit_events(订阅变更摘要)
  → 读取：service.subscriptions() / spending_overview()（只读，无锁）
```

### 2.3 Optimizer 流（完全无状态）

```
读 stack_items + subscription_records + settings 阈值（只读）
  → optimizer::derive_recommendations(...) 纯函数
  → ApiResult<Vec<Recommendation>> → GUI 面板 / CLI --json

R4：optimizer_simulate(ScenarioRequest)（同样只读，算术 → ScenarioResult，不落库）
```

## 3. 核心服务接口草案（service.rs 新增，全部 ApiResult 信封）

```rust
// -- stacks / subscriptions ------------------------------------
// 只读（不加锁）
pub fn stack_items(&self) -> Result<Vec<StackItemView>>;
pub fn stack_item(&self, id: &str) -> Result<StackItemDetail>;        // +detections+subscriptions
pub fn subscriptions(&self) -> Result<Vec<SubscriptionView>>;
pub fn spending_overview(&self) -> Result<SpendingOverview>;
pub fn optimizer_report(&self) -> Result<OptimizerReport>;            // {generated_at, recommendations}
pub fn optimizer_simulate(&self, req: ScenarioRequest) -> Result<ScenarioResult>;
// 检测事实刷新（修改类写操作：持 write_lock + runtime_guard，audit_events 登记）
pub fn stack_refresh_detections(&self) -> Result<StackRefreshReport>;
// 写（write_lock + runtime_guard；audit_events 登记，不写 operations——D-1）
pub fn stack_upsert_item(&self, req: StackItemUpsert) -> Result<StackItemView>;
pub fn stack_confirm_item(&self, id: &str) -> Result<StackItemView>;
pub fn stack_merge_items(&self, child_id: &str, into_id: &str) -> Result<StackItemView>;
pub fn stack_unmerge_item(&self, child_id: &str) -> Result<StackItemView>;
pub fn stack_delete_item(&self, id: &str) -> Result<()>;             // 存在任何状态订阅或有 merged 子项→Error::ResourceConflict（§7 D-2）
pub fn subscription_upsert(&self, req: SubscriptionUpsert) -> Result<SubscriptionView>;
pub fn subscription_delete(&self, id: &str) -> Result<()>;
```

新 serde 模型（`model.rs`，camelCase）核心：`StackItemView{id,name,vendor,source,detection_state,confirmed,last_confirmed_at,subscription_state,merged_into,detections:Vec<StackDetectionView>,...}`；`SubscriptionView`（字段同 MY_AI_STACK_PRD FR-2 表）；`SpendingOverview{by_currency:Vec<CurrencyBreakdown>, upcoming_renewals, incomplete, usage_without_bills, counts}`；`CurrencyBreakdown{currency, monthly_equivalent_minor, annual_equivalent_minor, subscription_count}`。

## 4. GUI / CLI 契约

- **Tauri 新命令**（src-tauri/src/lib.rs，照 run_api 模板 lib.rs:56）：M1-B 11 个 `stack_list / stack_get / stack_upsert_item / stack_confirm_item / stack_merge_items / stack_unmerge_item / stack_delete_item / stack_refresh_detections / subscription_upsert / subscription_delete / spending_overview`；M2-B 再加 `optimizer_report / optimizer_simulate`（合计 13）。注册进 `generate_handler!`（:557-613）。capabilities 无需改动（自定义命令属 core:default）。
- **CLI 新子命令**（main.rs）：
  - 只读：`agenthub stack list [--json]`、`agenthub stack show <id> [--json]`、`agenthub cost report [--json]`、`agenthub cost simulate <json-file|stdin> [--json]`（照 cmd_skills_list:1096 / cmd_operations_preview:1980 的 ApiContext 信封模式）。
  - 写：`agenthub stack add|edit|merge|confirm|delete`、`agenthub sub add|edit|delete` —— **D-1 裁定冻结**：本地 CRUD **直接执行**（不引入 approval token 流，不挂 persist_plan；stack/subscription 是低风险应用内数据，不写 Agent 目录）；**删除、合并等高影响操作要求显式确认**：GUI 走确认对话框，CLI **非交互执行高影响命令（delete/merge）必须传 `--yes`**，缺省拒绝并提示；交互式 TTY 下逐条确认。变更记录一律走 audit_events（§5）。
- 一致性保证：同一 `AgentHub` 方法；CLI `--json` 与 Tauri 信封同构（unwrap 前字节等价）；ACCEPTANCE_MATRIX §2.13 专门测两侧结果一致。

## 5. 写入与事务策略

- 锁：所有写方法 `let _w = self.write_lock.lock()?; let _r = self.runtime_guard()?;`（照 upsert_profile service.rs:2546-2548；**不要**照 backup 的直调 acquire 形式）。
- 事务：涉及多行写（refresh detections、merge/unmerge）用 `unchecked_transaction`（模板 db.rs:1874-1896）；单行 upsert 单语句。
- **审计（D-1 裁定冻结）**：每笔写只写 `audit_events(actor='user'|'cli'|'desktop', event_key='stack.item.upsert' / 'stack.merge' / 'stack.refresh' / 'subscription.upsert' 等，payload 含变更前后 id 级摘要，敏感值不落审计)`。**不写 operations 表、不扩展 OperationKind**——理由（契约核验 m00263-m00268）：operations 行消费链（operation_view service.rs:1702-1730 反序列化 plan 为 OperationPlan、approval_state、rollback_of）绑定 plan/approval/rollback 语义，无快照无执行步骤的"成功操作"会污染操作历史与 dashboard recent_operations（service.rs:2195）；前端 `types.ts:29-41` OperationKind 为严格字符串联合，后端新增 kind 会反序列化失败；`OperationKind::parse` 未知字符串回退 `Scan`（model.rs:1085）会误标。未来如需扩展 OperationKind，必须先核验上述消费者/状态机/恢复逻辑兼容（负责人 D-1 附加条件）。快照（snapshots 表）不适用（无文件系统写入），留空。merge/unmerge 因"不迁移数据"设计（DATA_MODEL I5）天然可逆，无需快照回放。
- 回滚：应用内数据写不提供 undo（首版）；数据安全靠 ①迁移前备份 ②用户备份（backups_create 已含全库 VACUUM INTO）③删除均需确认。CLI/GUI 的"取消订阅"是状态变更非删除，天然可逆。

## 6. 迁移与回滚策略

1. `SCHEMA_V4`：3 张新表 + 5 个索引（idx_stack_items_merged / idx_stack_items_group / idx_stack_detections_item / idx_subscriptions_item / idx_subscriptions_status，另有 UNIQUE(adapter_key) 约束非索引），加法，事务内执行；`user_version 3→4`。
2. 迁移前强制 `backup_before_migration`（沿用 db.rs:649 规则）；**实施前置项**：`backup_before_migration` 的 manifest `to_version` 硬编码 3（db.rs:705），v4 迁移前先参数化（属 db.rs 内部函数签名调整，影响面小）。
3. 失败回滚：迁移在 unchecked_transaction 内，失败即 rollback，user_version 不动；已产生的备份文件保留供恢复。
4. 功能回滚：新表对旧功能零影响（无反向触发器/视图依赖）；若需回退产品，可保留空表或由负责人决定 `DROP TABLE` 脚本（不在迁移序列中写 DROP）。
5. `schema_migrations` 表登记 version=4 + checksum（照 v3 模式 db.rs:315-316）。

## 7. 错误处理策略（D-2 裁定冻结）

- 裁定要点：**不得把资源状态冲突伪装成 `InvalidInput`**；保留 `Error::Conflict` 真实语义与 PLAN_STALE 映射、CLI 退出码不变；必须区分"参数非法 / 对象不存在 / 对象存在但当前不能操作"三态。
- 契约核验事实（m00263-m00268）：`Error::invalid()` 实际映射 `Error::InvalidSkill`（error.rs:45-47）→ ErrorCode `INVALID_SKILL`——语义陷阱，禁用于本模块；`Error::NotFound → InvalidInput` 是既有映射（contract.rs:105），**保持不动**；`Error::Conflict → PlanStale` 保留给计划失效场景，本模块不借用。
- **最小增量错误契约（加法，向后兼容）**：
  1. `error.rs` 新增变体 `InvalidInput(String)` + 构造 `pub fn invalid_input(message)` → 映射既有 `ErrorCode::InvalidInput`（exit 3，语义归位：参数/枚举/日期/minor 越界非法）。既有 `From<&Error>` 表逐臂保留，仅**追加**新臂，不改动任何现有映射。
  2. `error.rs` 新增变体 `ResourceConflict(String)` + 构造 `pub fn resource_conflict(message)`；`contract.rs` ErrorCode **追加** `ResourceConflict`（SCREAMING_SNAKE）；`exit_code()` 追加分支 `ResourceConflict => 4`（与 PlanStale 同为"状态冲突"族：操作对象存在但因当前状态无法执行；不改动 PlanStale 自身语义）。CLI `map_api_error` 需同步新增该映射（M1-B 范围）。
  3. 不存在的 id → `Error::not_found()`（沿用既有 `→ InvalidInput` 映射；该映射的语义瑕疵登记为技术债观察项，本次不改）。
- 三态映射表：

| 情形 | 错误 | ErrorCode | CLI 退出码 |
|---|---|---|---|
| 参数/枚举/日期/minor 越界非法 | `Error::invalid_input(msg)` | `INVALID_INPUT` | 3 |
| id 不存在 | `Error::not_found(msg)` | `INVALID_INPUT`（既有映射，不动） | 3 |
| 存在但不可操作（有订阅记录的删除、有子项的删除、merge 目标非法/成环、对 merged 子条目 upsert 订阅） | `Error::resource_conflict(msg)` | `RESOURCE_CONFLICT` | 4 |

- 消息要求：`ResourceConflict` 消息只含计数与原因（如 "stack item has 2 subscription record(s); delete or reassign them first"），不得回显路径或金额明细。
- GUI 呈现走 ErrorBanner + 表单内联；CLI 失败走 `map_api_error` 退出码（main.rs:532）+ `--json` stderr 失败信封（main() 内，main.rs:577-589）。

## 8. 安全威胁分析（摘要，完整版见 SECURITY_AND_PRIVACY_REVIEW）

| 威胁 | 缓解 |
|---|---|
| 新表迁移破坏现有数据 | 加法迁移+迁移前备份+integrity 校验+失败不提交（§6） |
| 检测流程误删用户订阅 | 检测只写 stack_detections；stack_items/subscriptions 无任何级联删除路径；FK RESTRICT |
| 多适配器重复计费 | merge 组聚合（DATA_MODEL I5：订阅归属不动，按组遍历）；合并前后组内订阅数与总成本恒等（测试断言）；疑似重复仅 R1 提示"潜在重叠"，不自动去重 |
| 多币种错误相加 | 按币种分组，无汇率不合计；测试断言 |
| 金额浮点误差 | minor units 整数 + i64 折算（checked_mul/checked_add），禁 f64 |
| 并发写（GUI+CLI 同时开） | runtime_guard flock + write_lock 串行；busy 显式报错（runtime.rs:96-106） |
| 隐私扩大 | 不新增权限/网络/遥测；不读 Keychain/prompt；表无敏感列；evidence_json 本地路径属准身份数据，仅本地存储、不进日志与诊断导出（SECURITY §隐私注） |
| 真实 Agent 目录被意外修改 | 本模块零 Agent 目录写入路径；测试断言（ACCEPTANCE §3.17 回归） |

## 9. 对抗式检查结论（七个视角，均已在设计内闭环；开放项标注）

- PM：数据全靠手动 → 空态引导与"成本数据源=用户登记"的期望管理是首要文案任务（两份 PRD §0 已定位）。
- 普通用户：月度等效 vs 当月扣款的混淆 → UI 固定说明位 + 测试断言文案。
- Rust 架构师：不引 async、不新增依赖、不动 ops.rs/Executor；OperationKind 不扩展（D-1 冻结，前端严格联合类型 + parse 回退误标风险）；错误契约仅加法扩展（D-2 冻结）。
- 数据库工程师：backup_before_migration 参数化是实施前置项（db.rs:705）；FK RESTRICT 防孤儿；索引覆盖外键查询。
- 安全工程师：无新权限、无网络、无敏感列；订阅数据会进入 backups_create 的全库备份（本地 0600/0700，可接受，SECURITY 文档登记）。
- QA：Keychain 沙箱失败会中断 cargo test 套件 → 新测试不含 Keychain 触点；运行命令用 `--test` 分片规避（执行计划写进 IMPLEMENTATION_PLAN）。
- 长期维护者：CLI 单文件 main.rs 已 2209 行 → 本模块新增 cmd_* 控制在 ~200 行内并集中在一个区段，避免进一步碎片化（重构 CLI 模块化属 Out of Scope，登记为技术债观察项）。

**开放项全部关闭（M0-R 裁定冻结）**：① CLI 写命令 = D-1 有条件批准（直接执行；delete/merge 高影响需显式确认，CLI 非交互需 `--yes`）；② 错误契约 = D-2 冻结（§7：`Error::InvalidInput` + `Error::ResourceConflict` 新变体，三态区分，既有映射零改动）；③ 版本序列 = D-3 批准方案 A（安全修复归档 0.5.0，不补造 0.5.1 Release；My AI Stack 后续按 0.6.0 规划，发布需单独授权）。
