# Cost Optimizer — 产品需求文档（PRD v0.1 草案，M0 冻结）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 状态：M0 草案，待负责人批准。上游：`MY_AI_STACK_PRD.md`（数据来源）、`M0_BASELINE_AUDIT.md` §8（用量数据不在产品边界内）。
> 定位：**可解释、可测试、完全离线的规则引擎**。第一版不依赖外部 LLM、不依赖实时价格 API、不读取任何账单。

## 0. 产品边界

- 只消费用户在 My AI Stack 登记的数据 + 系统检测事实，不虚构任何数字。
- 无使用量证据 → 永不说"闲置/浪费"；无价格目录 → 永不做"更便宜套餐"的事实断言。
- 建议只是"提示复核"，不代替用户决策；采纳建议不产生任何真实世界副作用（不取消订阅、不发请求）。
- 证据不足时必须降级为"数据缺口提示"（R5），而不是给出低质量建议。

## 1. 规则引擎（M2-A）

实现为 `crates/agenthub-core/src/optimizer.rs`：**无状态纯函数**，模式照 `validation.rs::derive_health`（validation.rs:19）——输入 `(stack_items, subscriptions, settings)`，输出 `Vec<Recommendation>`；不触库、不加锁、可穷举单测。

### R1 潜在重复订阅（P0）

- 触发：多个 active 订阅的工具被用户标记为相同用途组（`stack_items.overlap_group`，用户显式维护；**引擎不做自动语义推断**）。
- 输出：提示"以下工具可能存在用途重叠，建议复核"，列明各工具的已知月度等效成本与证据（=用户标记的 overlap_group + 登记时间）。
- 措辞硬约束：一律"潜在重叠"，禁止"可替代""重复付费"等断言。**同一合并组内的两条不同订阅记录不自动判定为重复扣费**——合并组是去重展示结构，不是财务事实判定（P0-2 冻结，DATA_MODEL I5）；若用户怀疑组内重复，只能作为证据"合并组内存在 N 条订阅记录"进入 R1 提示。
- M1 兜底：若用户未维护 overlap_group，R1 保持沉默（不算缺陷）。

### R2 高成本订阅复核（P0）

- 触发：用户在 Settings 登记复核阈值（按币种的 minor units，如 "USD 50/月等效"）；引擎找出月度等效 ≥ 阈值的 active 订阅。
- 输出：仅提示"该订阅成本高于你设定的复核阈值"，附金额与折算说明。
- 硬约束：无用量证据 → 不称闲置/浪费；未设阈值 → 规则静默关闭；hybrid 订阅仅以已知固定费部分参与阈值比较，UI 注明"浮动部分未计"。

### R3 续费提醒（P0）

- 触发：`renews_on` 在未来 N 天内（默认 7，Settings 可调 1-30）的 active 订阅。
- 输出：列表 + 距今天数；无 `renews_on` 的订阅进入"续期未知"组（R5 类展示），不推断；`renews_on` 已过而 status 仍 active 的进入"续期已过未更新"组（数据陈旧可见，m14）。
- 只读展示，不产生通知推送（首版无推送通道，也不该有）。

### R4 成本情景模拟（P0）

- 用户可模拟四类操作：取消某订阅 / 降低某已知订阅金额 / 替换为用户提供的其他套餐（用户输入新金额）/ 调整工具组合（停用某工具的所有订阅）。
- 输出：按币种分组的调整前后月度/年度等效差额 + 受影响订阅清单。
- 纯算术，无写入；模拟结果不落库（M2 首版不持久化 `optimization_scenarios`），可导出 JSON 留档（P1）。
- 硬约束：UI 明示"模拟不等于已取消订阅，节省金额尚未实现"。

### R5 数据缺口提示（P0）

- 系统性枚举：缺失金额 / 缺币种 / 缺周期 / 缺续费日期 / 工具有订阅但从未确认 / `renews_on < today` 且 status 仍 active（数据陈旧，提示更新——与 R3 联动，不静默排除）/ **billing_period 为 unknown 或 usage 但已填金额**（金额已知但无法折算，计入 incomplete，DATA_MODEL 聚合规则 1）。
- 输出：按缺口类型聚合的清单 + "去补全"入口。
- 这是证据不足时的**降级出口**：R1/R2/R4 涉及的任何订阅若字段缺失，建议必须带上 missing_information 并按缺口降级。

## 2. 建议结果契约（Recommendation）

serde struct（core 侧 `optimizer.rs`，camelCase 序列化，进 ApiResult 信封）：

```
Recommendation {
  recommendation_id: String            // **确定性 ID（D-1 契约修复冻结）**：`"rec_" + sha256(规则类型 + related_stack_items 排序拼接 + related_subscriptions 排序拼接 + 规则参数快照)[:12hex]`——同输入同 ID，支撑"同输入输出字节级相等"的可重复性测试；随机 uuid 已否决（随机 uuid 使该测试永远不可能成立）
  recommendation_type: String          // "R1_OVERLAP"|"R2_HIGH_COST"|"R3_RENEWAL"|"R4_SCENARIO"|"R5_DATA_GAP"
  title: String
  explanation: String                  // 含规则名与触发条件的人话解释
  related_stack_items: Vec<String>     // stack_item id
  related_subscriptions: Vec<String>   // subscription id（PRD 字段扩展，主控提示词要求的 related_stack_items 之外补充）
  evidence: Vec<Evidence>              // { kind, description, source_ref }  kind: user_input|detection|rule_param
  missing_information: Vec<String>     // 降级时非空：缺失字段清单
  assumptions: Vec<String>             // 如 "年付按12个月折算为月度等效"
  estimated_impact: Option<MoneyDelta> // { currency, minor_before, minor_after, delta_minor, period }；同币种才有值
  currency: Option<String>
  confidence_or_evidence_level: String // "input_only" | "user_configured" | "derived_arithmetic"
  user_action_required: String         // "review" | "complete_data" | "none"
}
```

- `confidence_or_evidence_level` 三级定义：
  - `input_only`：全部基于用户登记字段（R1/R2/R3）。
  - `user_configured`：依赖用户设置的规则参数（阈值/窗口/分组）。
  - `derived_arithmetic`：仅由用户给的数字做四则运算（R4），不含任何外部事实。
- **不存在 "verified" 级**：本产品没有可验证的外部价格/账单事实源（M3 TokenPlan 落地前永远如此）。
- 错误处理：规则引擎自身不产生 Error；输入校验在 service 层走 D-2 冻结契约——参数非法 `Error::invalid_input` → `INVALID_INPUT`（exit 3，ARCHITECTURE §7）。

## 3. 节省金额的严格语义（三态）

| 概念 | 定义 | 载体 |
|---|---|---|
| **Potential Savings** | 基于假设（R4 模拟）的差额，仅在模拟结果中展示 | `ScenarioResult`（内存，不落库） |
| **Confirmed Change** | 用户确认已完成某项调整（状态流转记录） | M2：`subscription_records.superseded_by` + 变更记录写入 audit_events |
| **Realized Savings** | 有实际费用证据支持的已实现节省 | **M2 首版不实现**（无账单数据源即无证据）；UI 只能显示 Confirmed Change 的"登记值"，并标注"待账单佐证" |

硬规则（写进 UI 文案与测试断言）：
1. Potential Savings 永不计入 Realized Savings，两个词不可混用。
2. 用户点击"采纳建议"只等价于创建一条模拟记录或跳转编辑表单，**不改变任何 subscription 状态、不显示任何"已节省 X 元"**。
3. Realized Savings 展示位在 M2 首版固定显示"暂无账单证据，无法确认实际节省"。

## 4. P0 / P1 / Out of Scope

- **P0（M2-A/B）**：optimizer.rs 纯函数 + R1-R5 + Recommendation 契约 + 只读 service 方法 `optimizer_report()` / `optimizer_simulate()` + GUI 建议面板 + CLI `agenthub cost report` / `agenthub cost simulate`（只读）。
- **P1**：情景导出 JSON、Confirmed Change 登记流（superseded 链）、Settings 中的阈值/窗口管理 UI 完善。
- **Out of Scope**：LLM 建议、网络价格抓取、自动执行取消、用量统计、汇率换算建议、optimization_scenarios 持久化。

## 5. 验收要点

对应 ACCEPTANCE_MATRIX §3（M2 部分）：
- 无证据场景正确降级（R1 无 overlap_group 静默；R2 无阈值静默；R5 兜底）。
- Potential ≠ Realized 的文案与状态断言。
- R4 算术正确性（含多币种不合并、缺失金额不参与）。
- 规则引擎对相同输入的输出字节级稳定（可重复性测试）。
- 无网络断言（测试环境无任何新增出站调用）。
