# Data Model Proposal — SCHEMA_V4（M0 冻结草案）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 设计基准：现有 `SCHEMA_V3`（db.rs:263-317，user_version=3）。本方案为**加法迁移 v3→v4**，不修改任何既有表。
> 命名沿用主控提示词候选（stack_items / subscription_records）；price_catalog_entries 延后到 M3；optimization_scenarios 首版不持久化。

## 1. 实体关系

```
agents (现有表)                      ← 系统检测事实（安装位置）
   │ 1        (ON DELETE SET NULL)
   ▼ n
stack_detections                    ← 可再生的逐适配器检测事实
   │ n        (ON DELETE CASCADE)
   ▼ 1
stack_items                         ← 用户可见/确认的工具条目（用户资产）
   │ 1        (ON DELETE RESTRICT)
   ▼ n
subscription_records                ← 用户登记的订阅/费用事实（用户资产）
   │ 0..1 (自引用 superseded_by)    ← M2 变更链（Confirmed Change）
   ▼
```

不变量：
- I1 安装事实（agents/stack_detections，系统产生、可重建）与订阅事实（stack_items 用户确认部分、subscription_records，用户产生）**物理分表**。
- I2 检测流程只写 stack_detections（**按 adapter_key UPSERT，见 I2a**）与自动创建 `confirmed=0` 的 stack_items；**永不 UPDATE/DELETE 用户确认字段、永不触碰 subscription_records**。
  - **刷新挂靠规则（冻结）**：refresh 在**单个事务**内执行——①先建/替 stack_detections 前先解析挂靠目标：对每个 adapter_key，查找"未合并（merged_into IS NULL）且 confirmed=0 且 source='detected'"的既有条目；②挂靠成功则该 adapter 的检测事实写入该条目名下；③找不到挂靠目标才新建 `confirmed=0` 条目；④已 confirmed 或已作为合并子项的条目**永不**自动新建重复项（该 adapter 的检测事实忽略——其上一次检测记录保留至 refresh 替换，主条目 detection_state 由规则 A/B/C 推导覆盖）。测试断言"合并 → 刷新 → 仍 1 条目"（验收矩阵 3.8）。
  - **I2a 检测稳定身份（冻结，P0-3）**：`stack_detections.adapter_key` 全局 UNIQUE（见 §2）——同一适配器在整库最多一条检测记录。refresh 写入方式为**按 adapter_key UPSERT**：`INSERT ... ON CONFLICT(adapter_key) DO UPDATE SET stack_item_id=<新挂靠目标>, detected_status=..., agent_id=..., evidence_json=..., detected_at=...`——检测行的 id 在挂靠目标变化时保持稳定（除非原挂靠条目被删除触发 CASCADE，此时该适配器记录随条目消失、下次 refresh 重新 INSERT）。连续刷新零删除零重建，幂等可断言（验收矩阵 3.8b：同一输入连续 refresh 两次，stack_detections 行数、id、updated 内容不变）。
- I3 订阅必须挂在 stack_item 上（stack_item_id NOT NULL）；工具可以没有订阅；订阅可以挂在"未安装"的工具上（detection_state=not_installed/unknown 的 manual 条目合法）。
- I4 删除 stack_item 前必须**不存在任何状态的订阅记录**（active/paused/cancelled/expired/unknown 均阻止——E1a 实验证实 FK RESTRICT 对 cancelled 同样硬拦，故服务层预检必须覆盖全部状态）；同时预检"存在 merged_into 指向本条目的子条目 → 要求先解除合并"；FK RESTRICT 兜底（避免撞 FK 抛 Database 原始错误）。错误契约三态（D-2 冻结，详见 ARCHITECTURE §7）：参数非法→`Error::InvalidInput`；id 不存在→`Error::NotFound`（沿用既有映射，不改）；存在但当前不可删→**`Error::ResourceConflict`（新变体）**，消息含阻止原因与订阅计数，绝不伪装成 InvalidInput。
- I5 合并 = **不迁移任何数据**（冻结，P0-2）：merge 操作仅执行一条 `UPDATE stack_items SET merged_into = <主条目id> WHERE id = <子条目id>`；订阅记录的 stack_item_id **永不因合并而改变**，始终归属其原始条目。
  - **聚合按合并组**：组 = 主条目 + 全部 `merged_into` 指向它的子条目。SpendingOverview / 详情聚合遍历组内**全部条目的原始订阅**——合并前后订阅数量与总成本天然一致（数据未动）。
  - **不自动判定重复扣费**：合并组内出现两条不同订阅记录时照常分别计入，不互相抵消或去重（用户数据权威；疑似重复只由 R1 以"潜在重叠"提示复核）。
  - **unmerge = 一条 UPDATE**：`SET merged_into = NULL`——原始归属从未离开子条目，撤销天然幂等可逆，无需快照或回放。合并后主条目新增的订阅与子条目无关（从未迁移），unmerge 不产生任何歧义。
  - **结构性约束（服务层强制）**：merge 目标必须是主条目（`merged_into IS NULL`）且不得为自身——该规则天然禁止多级合并与自环；"把主条目 merge 进自己名下的子条目"会被目标检查拒绝（子条目 merged_into 非空，不是合法目标）。深度恒 ≤1。
  - **变更记录**：merge/unmerge/delete/upsert 全部写 `audit_events`（event_key 如 `stack.merge`，payload 含主/子条目 id），**不写 operations 表**（D-1 冻结，见 ARCHITECTURE §5）。

## 2. 表定义（SQL 草案，进 SCHEMA_V4 常量）

```sql
CREATE TABLE stack_items (
  id                 TEXT PRIMARY KEY,                -- uuid v4（与现有表 id 风格一致）
  name               TEXT NOT NULL,                   -- 用户可见名称（detected 条目初始化为 adapter name，用户可改）
  vendor             TEXT,
  source             TEXT NOT NULL CHECK (source IN ('detected','manual')),
  detection_state    TEXT NOT NULL CHECK (detection_state IN ('installed','not_installed','unknown'))
                     NOT NULL DEFAULT 'unknown',
  confirmed          INTEGER NOT NULL DEFAULT 0,      -- 用户已确认
  merged_into        TEXT REFERENCES stack_items(id) ON DELETE SET NULL, -- 合并去重（NULL=主条目）；主条目被硬删时子条目解除合并而非级联删除（M5）
  overlap_group      TEXT,                            -- R1 用：用户显式声明的用途组（M2 消费）
  notes              TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  last_confirmed_at  TEXT                             -- 与 created/updated 分离：确认语义独立
);
CREATE INDEX idx_stack_items_merged ON stack_items(merged_into);
CREATE INDEX idx_stack_items_group  ON stack_items(overlap_group);

CREATE TABLE stack_detections (
  id              TEXT PRIMARY KEY,
  stack_item_id   TEXT NOT NULL REFERENCES stack_items(id) ON DELETE CASCADE,
  adapter_key     TEXT NOT NULL,                      -- 'codex'/'trae'/'trae-cn'/...（builtin.rs key）
  agent_id        TEXT REFERENCES agents(id) ON DELETE SET NULL,
  detected_status TEXT NOT NULL,                      -- AgentStatus 字符串原样
  evidence_json   TEXT NOT NULL DEFAULT '[]',         -- Detection.evidence 数组；本地路径属准身份数据：仅本地库存储，不进日志/诊断导出（隐私注）
  detected_at     TEXT NOT NULL,
  UNIQUE (adapter_key)                                -- P0-3 稳定身份：每适配器全库最多一条检测记录，refresh UPSERT 幂等（I2a）
);
CREATE INDEX idx_stack_detections_item ON stack_detections(stack_item_id);

CREATE TABLE subscription_records (
  id               TEXT PRIMARY KEY,
  stack_item_id    TEXT NOT NULL REFERENCES stack_items(id) ON DELETE RESTRICT,
  product_name     TEXT NOT NULL,
  vendor           TEXT,
  plan_name        TEXT,
  billing_type     TEXT NOT NULL CHECK (billing_type IN ('free','subscription','usage','hybrid','unknown')), -- 'unknown'=用户未知计费类型，绝不逼用户编造（m13）
  amount_minor     INTEGER,                           -- NULL=未知；绝不默认 0；无浮点
  currency         TEXT,                              -- ISO-4217 三字码；amount 非空时必非空（应用层校验）
  billing_period   TEXT CHECK (billing_period IN ('monthly','quarterly','semiannual','yearly','usage','unknown')),
  started_on       TEXT,                              -- ISO-8601 date（YYYY-MM-DD），用户输入
  renews_on        TEXT,                              -- ISO-8601 date；NULL=未知，不推断
  status           TEXT NOT NULL CHECK (status IN ('active','paused','cancelled','expired','unknown')),
  notes            TEXT,
  info_source      TEXT,                              -- 用户登记的信息来源
  last_confirmed_at TEXT,                             -- 用户最后确认时间
  superseded_by    TEXT REFERENCES subscription_records(id) ON DELETE SET NULL, -- M2 变更链；后继记录被硬删时解除链接（硬删顺序：先子链后主记录，见 §4 聚合注）
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL
);
CREATE INDEX idx_subscriptions_item   ON subscription_records(stack_item_id);
CREATE INDEX idx_subscriptions_status ON subscription_records(status);
```

**设计上不存在的列**（隐私红线，写进表注释与 SECURITY 文档）：支付卡/token/密码/API Key/账单凭证/汇率缓存/用量数据。

## 3. 字段语义权威表（主控提示词 §6.9）

| 字段 | 权威来源 | 系统可改写时机 |
|---|---|---|
| stack_items.name/vendor/notes/confirmed/overlap_group | 用户输入 | 仅用户操作（upsert/confirm/merge） |
| stack_items.source | 系统判定 | 条目创建时一次定型（detected 条目永远显示"检测来源"，即使被改名） |
| stack_items.detection_state | **系统推导**（非用户输入） | 每次 stack_refresh_detections 重算 |
| stack_detections.* | 系统检测 | 每次 refresh 按 adapter_key UPSERT（单事务，稳定身份幂等，见 I2a；不删除不重建） |
| stack_detections.agent_id / agents.* | 现有 sync_agents | 现有机制，本模块只读引用 |
| subscription_records.*（业务列） | 用户输入 | 仅用户操作 |
| subscription_records.created_at/updated_at | 系统 | 写时打点 |
| SpendingOverview / Recommendation | **计算结果**（永不落库） | 每次请求现算 |

## 4. 金额与日期语义

- **Money 表示**：`amount_minor INTEGER` + `currency TEXT`。Rust 侧 `Money { currency: String, minor: i64 }`（**i64 与 SQLite INTEGER 对齐**；个人订阅场景足够；折算时用 `checked_mul`/`checked_add` 防溢出，溢出返回 `Error::InvalidInput`，不 panic）；**禁止 f64 进入任何货币计算路径**（含 UI：前端仅用整数做千分位格式化，汇总全部来自 core）。
- **JSON 传输边界（P0-1 冻结）**：minor 的合法输入上界 = JavaScript `Number.MAX_SAFE_INTEGER`（2^53−1 = 9,007,199,254,740,991，远高于 i64 上界 9.22×10^18——E3 实测）。服务层输入校验：`amount_minor > 2^53-1` 或负数或 0 → `Error::InvalidInput`（不静默截断、不失真入库）；此范围内 i64 ↔ JSON number 双向无损，无需字符串编码。
- **币种精度（P0-1 冻结）**：minor 单位的 10^exp 严格按 ISO-4217 该币种 exponent 定义（USD/EUR/CNY=2 位，JPY=0 位，KWD=3 位……维护静态 exponent 表于 core）；不允许任意小数位猜测。
- **折算**（仅同币种内部；`period_months`：monthly=1, quarterly=3, semiannual=6, yearly=12）：
  - 单订阅 monthly_equivalent = `minor / period_months`，四舍五入到最小货币单位（定死**四舍五入半进位** round-half-up，不用 banker's rounding）；除不尽时 UI 标注"折算取整"（测试覆盖：CNY 5000 半年付 → 月 833；2500/6 → 417，E3）。
  - **annual_equivalent = `minor × 12 / period_months` 直接从原始 minor 计算（先乘后除，`checked_mul`），永远不由已取整的 monthly_equivalent 反推**。因 12 可被 {1,3,6,12} 整除，年度等效**恒为精确整数、永不取整**——年付 12000 → 年度等效恰为 12000（E3 实测 True），取整绝不扭曲年度账单对账。
  - 例（验收固定样例）：月付 2000 → 月 2000；年付 12000 → 月 12000/12 = **1000**。两式与 ACCEPTANCE §2.4 一致。
- **聚合规则**（SpendingOverview）：
  1. 只计 `status='active'` 且 amount 与 currency 均非空且 `billing_period ∈ {monthly,quarterly,semiannual,yearly}` 的 subscription（'hybrid' 有 amount 部分照计，UI 注明"仅含已知固定费、浮动部分未计"；billing_type='unknown' 的订阅归入 `incomplete` 组，金额未知绝不按零计）。
  2. **按 currency 分组，组间永不相加**；无任何汇率换算功能。
  3. amount 为 NULL 或 currency 为空的订阅进入 `incomplete`，不计入任何合计。
  4. billing_type='usage' 且无 amount → `usage_without_bills` 单列。
  5. status ∈ {paused, cancelled, expired, **unknown**} 不计入成本合计，但在计数区按各状态分别展示（unknown 态不可静默丢弃，m15）。
  6. `renews_on` 为 NULL → `renewals_unknown` 组；**永不推断续费日期**。
  7. `renews_on < today` 且 status='active' → `stale_renewals` 组（"续期日期已过未更新"——数据陈旧必须可见，不得静默排除，m14）；UI 提示用户更新该订阅状态或日期。
  8. **合并组聚合（I5 冻结）**：merged 子条目与主条目按合并组聚合，订阅记录归属原条目不动；组内全部订阅**照常分别计入**，不自动去重或抵消（疑似重复仅由 R1 提示"潜在重叠"）——合并前后组内订阅数量与总成本恒等。
  9. **取整顺序（P0-1 冻结）**：聚合求和用**未取整的精确有理数**（各订阅年度等效均为精确整数，同币种组内先求年度和，再 `annual_total / 12` 半进位取整一次得组月度等效），**绝不对逐条已取整的月度值求和**——避免取整误差累积；仅展示层取整一次。（例：两条 CNY 1000 半年付——逐条取整再求和 = 167+167 = **334**，精确求和后取整 = 2000/6 = **333**，前者多收 1；精确法保证聚合值不劣于逐条法。）
- **日期**：一律 `YYYY-MM-DD` 字符串（ISO-8601，无时区语义）；"即将续费"窗口 = `renews_on ∈ [today, today+N]`（N 来自 Settings，默认 7；边界用例：today 当天含、today+N 含、today+N+1 不含——验收矩阵 3.4）；today 取本机本地日期（chrono `Local::now().date_naive()`，与现有 util::now 的时间戳风格分离并在 UI 显示用户本地时区语境）。

## 5. 索引与查询计划

- 聚合查询模式（spending_overview）：一次 `SELECT ... WHERE status='active'` 全量载入内存计算（数据量为个人级，几十到几百行，无需 SQL 聚合复杂化；与 dashboard 的内存聚合模式一致 service.rs:2185-2284）。
- `idx_stack_items_merged` 支撑 merge/unmerge 查询；`idx_subscriptions_item` 支撑 detail 页 join。
- 全部新查询走 `prepare + query_map`（照 list_plugins db.rs:1327 模板）。

## 6. 迁移与数据安全

- SCHEMA_V4 常量追加在 db.rs:317 之后；`migrate()` 增加 `if version < 4` 分支（v3→v4），事务内 DDL + `pragma_update(user_version,4)` + commit；**执行前 `backup_before_migration`**（需先把 manifest `to_version` 参数化，db.rs:705——实施前置项 P-1）。
- `schema_migrations` 登记 (4, checksum='stack-subscriptions-v1', applied_at)（照 v3 的 db.rs:315-316）。
- 隔离验证：所有迁移测试用临时 `AGENTHUB_HOME` + 构造含 V1-V3 数据的 fixture 库，断言：数据保留（现有表数量不变：V1 13 张 + V2 5 张 + V3 4 张 = 22 张，含行数抽查）、integrity_check 通过、备份文件生成、user_version=4。复用 `tests/validator.rs` 的 user_version/table_count helper（:131-138）。
- 回滚：迁移失败→事务回滚+备份保留（现有机制；E2 实验证实事务内任意失败回滚干净：行数不变、新表不存在、user_version 不前进）；产品级回滚→新表不影响旧功能，DROP 脚本不属于迁移序列。
- **P0-5 冻结：SCHEMA_V4 升级后不存在 `git revert` 式数据库降级**——DDL 已提交的 SQLite 库没有逆向迁移路径，这是设计事实而非缺陷。降级/恢复手段唯一且显式：
  1. 迁移前 `backup_before_migration` 生成 VACUUM INTO 快照（E2 证实备份可读）；
  2. `restore` 路径（现有 service.rs 备份恢复）回放到 V3 库——M1-A 必须新增测试：对 V4 库执行 restore 到迁移前备份 → user_version 回到 3、V4 表不存在、原有数据完整、随后可再次迁移到 V4；
  3. 应用回退到旧版本时读到 user_version=4 的库：现有 `migrate()` 逐版本机制不会降级也不会破坏（旧代码只读到 user_version≥自己版本时不迁移——M1-A 需补一条断言测试证实旧行为，不新增降级逻辑）；
  4. integrity_check 在上述每步后必须通过。
- **P0-5 测试隔离冻结**：新增验收采用**纯 fixture 模式**——全部测试在隔离临时 `AGENTHUB_HOME` 运行，fixture 库由测试代码构造（V1→V3 逐版本建表+样例行），**不读取真实 `~/.agenthub`、不触碰本机真实 Agent 目录**。现有 `v31_acceptance.sh` 的真实环境读取问题记录于 M0_BASELINE_AUDIT §测试基线；M1-C 为本模块验收提供 `scripts/` 内独立隔离脚本，不修改现有脚本语义。

## 7. 与现有能力的关系（主控提示词 §6.3）

- **不复用 capabilities 表**：`capabilities.kind CHECK ('skill','mcp','plugin')`（db.rs:37）排除了扩展路径；且订阅是财务事实不是能力事实，模型不同构。
- **不重复存储检测数据**：agents 表与 stack_detections 各司其职（前者=适配器级 agent 注册，后者=工具级检测事实）；stack_detections.agent_id 是引用不是复制。
- **capability/version/installation 事实模型零改动**：M1/M2 不写这三个表。

## 8. 可删除性验证（主控提示词 §6.10）

测试断言（进 M1-A）：`DROP TABLE subscription_records; DROP TABLE stack_detections; DROP TABLE stack_items;`（或清空）后——agents/capabilities/installations/profiles/MCP 全部读写正常、现有 109 个 core 测试不回归（新表 DROP 用独立 fixture 验证，不在真实库执行）。

## 9. 后续版本预留（不实现，只留接口语义）

- M3 `price_catalog_entries`：{catalog_id, product_key, plan, currency, amount_minor, valid_from, valid_until, source_url, verified_at, stale} —— 必须带来源+版本+时间戳+失效状态；未经核验的价格**不得**作为确定性推荐依据（只允许出现在"参考信息"展示位）。
- M2 `optimization_scenarios`：如需持久化，加独立表存 JSON 快照，不进 subscription 主表。
- Confirmed Change 链：superseded_by 自引用 + audit_events，M2-B 落地。
