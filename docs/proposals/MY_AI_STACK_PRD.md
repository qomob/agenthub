# My AI Stack — 产品需求文档（PRD v0.1 草案，M0 冻结）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 状态：M0 草案，待负责人批准后进入 M1。上游事实基线见 `docs/proposals/M0_BASELINE_AUDIT.md`。
> 定位对齐 AgentHub 长期方向「Your AI Stack Control Center」：My AI Stack 回答"我在用哪些 AI 工具、为哪些付费、周期是什么、每月等效成本多少、哪些即将续费、哪些信息还未知"。

## 0. 产品边界（先说清不做什么）

- **不做账单同步 / 邮件解析 / 支付账户接入**：首版所有金额、续期日期、套餐信息均来自用户手动登记。
- **不做用量统计**：引擎不执行技能、不联网、无运行时代理（M0 审计 §8；PRD §5 已把调用统计列为 MVP 边界外）。产品文案不得宣称具备 Token 统计或实时价格监控。
- **不把"检测到安装"等同"正在使用"或"已经付费"**：安装事实（系统检测）与订阅事实（用户登记）在数据模型上是两张表、在 UI 上必须可视化区分。
- **不因卸载自动删除订阅**：订阅记录是用户资产事实，卸载只影响"检测状态"。
- 完全离线，无任何新增网络出站（M0 审计 §4 证实现有应用离线）。

## 1. 目标用户与核心场景

主用户：同时使用多个 AI 编码/办公 Agent（Codex、Claude Code、Cursor、TRAE、DSH 等）的个人开发者或小团队，订阅分散在不同厂商、币种、计费周期，无法回答"我到底每月花多少钱、下个月哪几笔要续费"。

核心场景：
- S1 首次打开 My AI Stack：看到系统检测到的 AI 工具清单 + 空的订阅登记引导。
- S2 登记/维护订阅：手动补全工具的付费信息，允许留空未知字段。
- S3 查看 Spending Overview：按币种分组看已知成本、月度/年度等效成本、即将续费、信息不完整项。
- S4 处理检测去重：同一产品被多个适配器检测到时合并为一个工具条目。
- S5 生命周期变化：工具卸载/重装/换套餐/取消订阅后，历史记录仍可追溯。

## 2. 功能需求

### FR-1 AI Stack Inventory（P0）

- FR-1.1 从现有检测能力（`sync_agents`/`list_agents`，service.rs:157/352，复用 11 适配器 `detect`）生成"本地检测到"的工具条目；检测结果写入可再生表 `stack_detections`（**按 adapter_key UPSERT 稳定身份，幂等，DATA_MODEL I2a/P0-3**），用户可见的 `stack_items` 条目不因重新检测而丢失用户确认状态。**刷新挂靠规则（与 DATA_MODEL I2 一致）**：每次 refresh 单事务内，各 adapter 的检测优先挂靠到"未合并且未确认（confirmed=0）且 source='detected'"的既有条目；挂不到才新建 `confirmed=0` 条目；已确认或已合并的条目永不自动产生重复项。
- FR-1.2 每个工具条目必须呈现以下互斥标注（来源与状态分开）：
  - 来源：`detected`（系统检测）/ `manual`（用户手动添加）；
  - 检测状态：`installed / not_installed / unknown`；
  - 用户确认：`confirmed / unconfirmed`；
  - 订阅登记：`已登记 / 未登记`（从订阅表推导，不冗余存储）。
- FR-1.3 用户可"确认"一个条目（确认其名称/归属正确）；确认时间 `last_confirmed_at` 单独记录，不与检测时间混淆。
- FR-1.4 去重合并：同一产品被多个适配器检测（如 TRAE 与 TRAE CN 分属两个 adapter key）时，用户可把多个条目合并为一个（`merged_into` 指向主条目）；被并入条目在列表中折叠显示"已并入"。**合并不迁移任何数据（D-1/P0-2 冻结，DATA_MODEL I5）**：订阅记录始终归属其原始条目；展示与成本统计按**合并组聚合**——合并前后订阅数量与总成本天然一致；组内两条不同订阅**不自动判定重复扣费**（疑似重复仅由 Cost Optimizer R1 以"潜在重叠"提示复核）；撤销合并 = 清除 `merged_into`，天然可逆幂等；禁止合并成环与多级合并（服务层校验）。合并后**刷新检测不会为子条目的 adapter 重新创建条目**（挂靠规则见 FR-1.1；验收矩阵 3.8）。
- FR-1.5 手动添加工具：允许登记未安装/非检测范畴的工具（如网页版 ChatGPT、API 平台），来源标记 `manual`。
- FR-1.6 删除确认：删除条目需 ConfirmDialog 二次确认；存在**任何状态**的订阅记录、或存在并入本条目的子条目（merged 子项）时阻止直删并提示先处理（数据层 FK RESTRICT + 服务层预检返回 `Error::ResourceConflict`，message 只含阻止原因与订阅计数——三态错误契约见 ARCHITECTURE §7/D-2 冻结）。对被合并子条目登记订阅 → `ResourceConflict`（提示改挂主条目）。

### FR-2 Subscription Manager（P0）

字段契约（全部为用户输入；**未知字段留空，绝不自动填默认值**）：

| 字段 | 类型 | 约束 |
|---|---|---|
| 工具关联 | stack_item_id | 必填；也可只建订阅不关联工具（M2 扩展，M1 允许 NULL 工具的"孤立订阅"？**否——M1 强制关联**，保持模型简单；未安装工具可先手动建 stack_item 再挂订阅） |
| 产品名 | text | 必填 |
| 供应商 / 套餐名 | text | 可空 |
| 计费类型 | `free / subscription / usage / hybrid / unknown` | 必填（用户选择，含"未知"选项）；无隐式默认 |
| 金额 | **integer minor units（分/cent 级整数）** | 可空；NULL=未知，绝不视作 0 |
| 币种 | ISO-4217 三字码 | 金额填写时必填 |
| 计费周期 | `monthly / quarterly / semiannual / yearly / usage / unknown` | 可空 |
| 开始日期 / 下次续费日期 | ISO-8601 日期 | 可空；NULL=未知，不推断 |
| 状态 | `active / paused / cancelled / expired / unknown` | 必填 |
| 备注 / 信息来源 | text | 可空 |
| 最后确认时间 | 系统记录每次用户保存时间 | 系统字段 |

- FR-2.1 CRUD 全生命周期：新增/编辑/取消（状态改 cancelled，不删行）/暂停/删除（ConfirmDialog 确认）。**变更记录走 `audit_events`（D-1 冻结），不写 operations 表**。
- FR-2.2 **禁止保存**：支付卡信息、账户密码、API Key、账单访问凭证、登录 Cookie——表结构上就不存在这些列，表单上不提供输入位。
- FR-2.3 编辑产生审计轨迹：修改前后值摘要写入 `audit_events`（复用现有表，event_json 不含敏感值——本模块无敏感值）。
- FR-2.4 Agent 卸载（`sync_agents` 检测到 installed→not_installed）**不触碰** subscription_records；仅更新 `stack_detections` 与条目的 detection_state。

### FR-3 Spending Overview（P0）

展示项（计算规则见 DATA_MODEL_PROPOSAL §5，此处为产品语义）：

- 已登记订阅数量（按状态分组：active/paused/cancelled…）。
- 已知订阅费用：按**币种分组**的月度/年度等效成本表。不同币种**永不相加**，无可信汇率就不给换算总额。
- 月度等效成本（Monthly Equivalent）：月付不变、季付 ÷3、半年付 ÷6、年付 ÷12 折算（整数四舍五入半进位，规则见 DATA_MODEL §4），UI 明示"月度等效成本 ≠ 当月实际扣款"。
- 年度等效成本（Annual Equivalent）：月付 ×12、季付 ×4、半年付 ×2、年付不变。
- 即将续费：只依据用户明确登记的 `renews_on` 日期（默认未来 7 天窗口，可设置）；无续费日期的订阅归入"续期未知"组，**不得推断**；`renews_on` 已过但状态仍 active 的订阅归入"续期日期已过未更新"组提示更新（DATA_MODEL §4 规则 7），不静默排除。
- 信息不完整：金额、币种、周期、续费日期任一缺失的订阅单列展示并给出补全入口；**缺失金额不计入任何合计**。
- 按量计费（usage）且无用户登记的实际支出：单列"按量 · 无账单数据"，**不凭空估算**。
- 每个数字都附数据来源标注（"你登记的 N 条订阅 · 最后确认 <时间>"），与"自动检测"标注严格分离。

### FR-4 UI / UX（P0）

- 导航：`navItems` 新增「My AI Stack」入口（group 复用 'workspace'）；⌘1-9 按 navItems **数组下标**映射（App.vue:101-105 取 navItems[Number(key)-1]，与 shortcut 字段无关），故新入口**必须追加在数组末尾（下标 ≥9）**，不得插在中部，否则既有快捷键全部错位。
- 四个视图：Stack Overview（列表+汇总卡）/ Tool Detail（`/stack/:id`，含检测事实、订阅列表、操作历史）/ Add-Edit Subscription（BaseModal 表单，照 ProfilesView 模板）/ Spending Overview（Overview 页内 tab 或独立路由，M1-B 定稿）。
- 设计规则：
  - 沿用现有组件（DataTable/StatCard/BaseModal/ConfirmDialog/EmptyState/StatusBadge）与 `--ah-*` token，不重做导航。
  - 自动检测 vs 手动登记的信息以徽章区分（StatusBadge + 来源角标）。
  - 空状态：EmptyState 给出"手动添加第一个工具 / 去扫描检测"两条引导。
  - 所有删除操作 ConfirmDialog；键盘可达、焦点环、错误用 ErrorBanner + 表单内联错误。
  - 全量 zh/en i18n 镜像（en 是 `typeof zh`，新增 key 双语必须同时写）。
  - 展示数据来源与最后更新时间（检测时间 vs 用户确认时间分开显示）。

## 3. 非功能需求

- N1 所有业务逻辑在 `agenthub-core`，GUI/CLI 复用同一 service 方法；CLI 提供 `agenthub stack ...` 子命令（与 GUI 行为一致）。
- N2 写操作持 `write_lock + runtime_guard` 双锁并写 `audit_events`（不写 operations——D-1 冻结，ARCHITECTURE_PROPOSAL §5）。
- N3 新表迁移走 SCHEMA_V4 加法迁移 + 迁移前备份，失败不提交（复用 db.rs 现有机制）。
- N4 数据可整组删除：删除两张新表+索引不影响现有 Agent 管理（外键只指向 agents 表的 id，ON DELETE SET NULL）。
- N5 无网络、无遥测、不读取 prompt/API Key/Keychain 值。

## 4. P0 / P1 / Out of Scope

- **P0（M1）**：FR-1（除 1.4 合并的撤销动画等润色）、FR-2、FR-3、FR-4 基础版、CLI 只读命令（stack list / spending）。
- **P1（M1-C 可选收尾 / M2 早期）**：stack 合并的 CLI 命令、Spending 导出（CSV，本地生成）、订阅修改历史视图（superseded_by 链）。
- **Out of Scope**：账单同步、邮件/收据解析、汇率服务、云备份、多用户、Windows/Linux 适配器扩展、Token 用量统计、自动取消订阅。

## 5. 验收要点（与 ACCEPTANCE_MATRIX 对应）

至少覆盖：空 Stack；单免费工具；单月付；年付折算（A=USD20/月 + B=USD120/年 → 月度等效 USD 30，但 UI 不得把 30 标为当月扣款）；CNY 加入后不得与 USD 混合求和；未知金额≠0；usage 无账单不估算；周期 unknown/金额已知 → 不折算进月度等效；合并前后订阅数与总成本一致、撤销合并恢复原状；连续两次 refresh 幂等（检测行数与 id 稳定）；卸载后订阅保留；CRUD 与并发双写一致；CLI/GUI 同结果（比较规范化业务数据，非信封字节）。
