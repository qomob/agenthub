# M1-A Final Verification — F1/F2 补证

基准：`fc4088e feat(core) M1-A`。负责人裁定（CONDITIONAL PASS / M1-B HOLD）要求的两项补证，本文记录原因、证据与结果。全程仅 Rust Core 测试代码与本文档，未动 GUI/CLI/迁移产物，未操作真实用户数据库。

**最终 commit：`114794c test(core): add F1 frozen-case regressions and F2 mid-migration rollback proof`**
**测试结果：149 passed / 0 failed（较 M1-A 报告 +3：F1 两个独立回归 + F2 一个中途失败回滚）**

---

## 1. F1 · 金额测试矛盾 — 判定：报告笔误，代码无缺陷

### 1.1 原始事实（F1 要求项 1）

M1-A 报告中"两条 CNY 1000 半年付 → 月度 667"对应的真实测试是
`crates/agenthub-core/tests/spending_costs.rs:419` `rounding_order_keeps_half_up_semantics()`：

- 两条订阅：CNY、**2000 minor**（20.00 元）各一、`billing_period = "semiannual"`、`status = "active"`、金额/币种/周期齐全；
- 走服务层 `spending_overview()` → `money::aggregate_currency()`；
- 断言 `by_currency["CNY"].monthly_equivalent_minor == 667`（`spending_costs.rs:439`）。

即 fixture 用的是 **2000+2000**（年度精确 8000 → 月度 4000/6=666.67 → 667），而报告把它写成了 1000。

### 1.2 实际调用函数与断言（F1 要求项 2）

`aggregate_currency(lines)`（`crates/agenthub-core/src/money.rs:202`）：
对每条 `annual = amount×12/period.months()`（i128），`checked_add` 累加精确年度；
输出经 `checked_output` 双重护栏；月度 = `div_round_half_up(总年度, 12)` **一次取整**。
函数 doc 注释自 M1-A 起就引用了 1000+1000 半年付 → 333（167+167=334 是错误的逐条取整）。
`money.rs` 原有单测 `aggregate_avoids_per_line_rounding_distortion`（money.rs 测试段）断言的正是 333。

### 1.3 两个独立回归（F1 要求项 3/4，新增于 money.rs 测试段）

| 新测试 | 输入 | 断言 |
|---|---|---|
| `frozen_m0r_case_two_cny_1000_semiannual_monthly_333` | 两条 CNY 1000 minor 半年付 | `annual == 4000`（每条 2000/年）；`monthly == 333`（4000/12=333.33 → 333，单次取整；非 334、非 667） |
| `m1a_fixture_case_two_cny_2000_semiannual_monthly_667` | 两条 CNY 2000 minor 半年付 | `annual == 8000`（每条 4000/年）；`monthly == 667`（8000/12=666.67 → 667）；且 `monthly == (annual+11)/12`（月度由精确年度推导） |

### 1.4 年度等效断言（F1 要求项 5）

已含于上表：1000+1000 → 年度 4000；2000+2000 → 年度 8000。均精确整数、无取整损失。

### 1.5 判定（F1 要求项 6）

**报告笔误**：M1-A 实现报告把 spending fixture 的金额 2000 误写为 1000，但保留了 667 的结果——两者组合即自相矛盾。业务代码（`aggregate_currency`）、原 fixture（2000+2000 → 667）、原 money 单测（1000+1000 → 333）三者互相一致，且与 M0-R 冻结算术一致。**未修改任何预期值或业务代码；两个冻结案例现已各自拥有命名独立回归。**

---

## 2. F2 · 迁移中途失败回滚 — PASS

### 2.1 测试

`crates/agenthub-core/tests/stack_migration.rs` 新增
`migration_failure_mid_way_rolls_back_partial_ddl()`（纯 fixture，隔离临时 home，不读真实 `~/.agenthub`）。

### 2.2 故障注入设计（使 SCHEMA_V4 部分 DDL 成功后失败）

1. 用既有 fresh fixture 产生的 **pre-migration-v3 备份**（VACUUM INTO 快照，真实 V1–V3 schema）作为第二隔离 home 的数据库 → 这是真正的 V3 数据库，非手造空库；
2. 注入可核验既有业务数据：`INSERT INTO settings (key,value,updated_at) VALUES ('f2-marker','before-v4','2026-11-11T00:00:00Z')`（`settings` 为 V1 既有业务表）；
3. 注入不兼容阻塞表：`CREATE TABLE stack_detections (bogus INTEGER NOT NULL)`。

于是 SCHEMA_V4 的执行顺序（`db.rs` `SCHEMA_V4`）为：

```
CREATE TABLE stack_items              ← 成功（事务内）
CREATE INDEX idx_stack_items_merged   ← 成功（事务内）
CREATE INDEX idx_stack_items_group    ← 成功（事务内）
CREATE TABLE IF NOT EXISTS stack_detections ← 因同名表已存在而跳过
CREATE INDEX idx_stack_detections_item ON stack_detections(stack_item_id) ← 失败：no such column: stack_item_id
```

即**前面 3 条 DDL 已成功执行后**才失败——比"第一条 DDL 就失败"更具证明力。

### 2.3 验证结果（全部满足）

| 负责人要求 | 断言 | 结果 |
|---|---|---|
| 本次迁移新建的表与索引全部回滚 | `stack_items`、`subscription_records`、`idx_stack_items_merged`、`idx_stack_items_group`、`idx_subscriptions_item` 在 `sqlite_master` 中计数为 0 | ✅ |
| （附加）存留的 `stack_detections` 是 fixture 注入表而非迁移产物 | `pragma_table_info` 仍含 `bogus` 列 | ✅ |
| user_version 保持 3 | `PRAGMA user_version == 3` | ✅ |
| V1–V3 既有数据不变 | `settings` 中 `f2-marker = 'before-v4'` 原样 | ✅ |
| integrity_check 通过 | 恢复后 `integrity_check()` Ok | ✅（迁移失败侧以回滚未提交保证；恢复侧显式断言） |
| pre-migration 备份存在且可读 | 失败迁移前的 `pre-migration-v3-*` 目录存在，manifest 就位，用 SQLite 打开该快照 `user_version == 3` | ✅ |
| 故障解除后可重新成功迁移 | `DROP TABLE stack_detections` → 重开 → `user_version == 4`、`stack_items` 存在、`f2-marker` 数据仍在 | ✅ |

> 注：回滚未提交的事务不存在 `schema_migrations` 记录，故无该断言（与既有 `migration_failure_rolls_back_and_keeps_version_3` 同理）。

---

## 3. RED/GREEN

不适用：**无代码缺陷修复**。F1 判定为报告笔误，F2 为测试补强。新增的 3 个测试直接 GREEN（F2 首跑曾 FAIL，原因是我的断言清单误把 fixture 注入的阻塞表 `stack_detections` 列入"必须回滚"——修正断言语义后 GREEN；该修正只改测试，未改业务代码）。

## 4. 变更文件列表

| 文件 | 变更 |
|---|---|
| `crates/agenthub-core/src/money.rs` | +2 个测试（§1.3），含 333/667 双案例冻结注释 |
| `crates/agenthub-core/tests/stack_migration.rs` | +1 个测试（§2），约 110 行 |
| `docs/proposals/M1_A_FINAL_VERIFICATION.md` | 本文档（草稿目录，未提交） |

`tests/common/mod.rs` 曾被 rustfmt 顺带重排两次（rustfmt 默认跟随 `mod` 声明格式化子模块——上一轮"意外改动"的根因），均已 `git checkout` 还原为零 diff。

## 5. 最终 commit 与测试结果

- 新 commit：`114794c`（money.rs + stack_migration.rs，约 +230 行测试，类型 `test(core)`，无业务代码改动）。
- `cargo test -p agenthub-core`：**149 passed / 0 failed**（money 9、my_ai_stack 15、spending_costs 11、stack_migration 5 + 既有全部通过）。
- `cargo check -p agenthub-cli`：通过。
- 格式：改动文件经 rustfmt 处理；全仓既有格式债（含 `examples/dev_smoke.rs`、`tests/common/mod.rs`）维持不动（此前裁定不要求全仓格式化）。
- Keychain 测试：完全权限环境 PASS（同 M1-A 报告，无变化）。

## 6. 未解决风险（维持登记，无新增）

1. `restore_backup()` API 不消费 pre-migration manifest（发布前风险登记项，负责人已裁定不要求 M1-B 擅自扩展恢复 API）。
2. 合法输入聚合超限返回 `InvalidInput` 的语义（M1-B 需展示准确异常说明，不得清空或按零成本显示已有订阅——负责人本轮特别保留项）。
3. exit 4 双语义、`NotFound→INVALID_INPUT` 技术债（M0-R 登记）。

## 7. 结论

**F1：已关闭（报告笔误，代码正确，双冻结案例独立回归落地）。**
**F2：已关闭（中途失败回滚证据完整，7 项断言全过）。**
**M1-B：READY（待负责人最终裁定放行）。**
