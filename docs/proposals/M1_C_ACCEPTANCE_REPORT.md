# M1-C 验收报告 — 真实 Tauri 端到端 + 反向测试 + CLI 语义裁定

- 基准 commit：**`7c059f7`**（feat: My AI Stack GUI + CLI (M1-B)）；真实 commit 链 `fc4088e` → `114794c` → `48a37af` → `7c059f7`（自旧到新）
- 授权：负责人裁定 **M1-C GO ONLY**（M1-B PASS·CLOSED）；完成后停止等待审查
- 状态四态：**PASS** = 已运行并通过；**FAIL** = 已运行、结果不符合预期；**ENV_BLOCKED** = 环境限制未通过；**NOT_RUN** = 授权范围外或未执行
- 本轮工作区变更：`M src/__tests__/stack-ui.spec.ts`（+27/−2，仅测试）、`?? scripts/stack_acceptance.sh`（新增 211 行纯 fixture 脚本）；**未改任何业务代码语义**

---

## 1. 四项专项门禁结论

| 门禁 | 结论 | 关键证据 |
|---|---|---|
| **V1** 真实 Tauri 端到端 | **PASS** | 真实 macOS 桌面应用（Tauri 2 + WKWebView）中完成 13 个 GUI 场景；IPC 参数 snake_case 与 camelCase 双向映射经实际调用验证（§2） |
| **V2** UX-1 反向测试 | **PASS** | 抽出 `assertNoFakeZero()`，新增反向测试证明断言可失败（§3） |
| **V3** CLI JSON spending 语义 | **PASS（记录限制）** | `stack list --json` 只承诺 items；spending 失败在 JSON 模式静默丢弃（§4） |
| **V4** 报告口径修正 | **PASS** | commit 顺序/接口计数/命令计数已按真实代码校正（§1 与 M1-B 报告一致） |

**另有 1 项 FAIL（GUI 缺陷，非验收脚本失败）**：spending 概览陈旧缓存（§5）。M1-C 边界内只记录定性，不修业务代码。

---

## 2. V1 — 真实 Tauri 端到端验收

### 2.1 环境隔离（零触碰真实用户数据）

```
FIX=/var/folders/42/0_vh3ptx7735d89p2rz0kj980000gn/T/ah-gui-accept.zF4VAb
AGENTHUB_HOME=$FIX/home  AGENTHUB_DATA_DIR=$FIX/home/.agenthub
```

- 启动前置条件：**必须**先 `pnpm dev`（Vite 1420），否则窗口白屏（`devUrl` 依赖 dev server）
- `lsof` 证实进程只打开 `$FIX/home/.agenthub/agenthub.db` 及 `-wal`/`-shm`
- 核心隔离机制证据：`crates/agenthub-core/src/service.rs:62` `CoreConfig::resolve()` 优先读 `AGENTHUB_HOME`（`std::env::var_os`），未设置才回落 `dirs::home_dir()`；`service.rs:67` 同法处理 `AGENTHUB_DATA_DIR`

### 2.2 IPC 契约事实（src-tauri/src/lib.rs）

| 项 | 事实 |
|---|---|
| 命令总数 | `generate_handler![...]`（lib.rs:630 起）**66 个**，其中 M1-B 新增 11 个（前 55 为既有命令） |
| 新增命令 | `stack_list`:559、`stack_get`:564、`stack_upsert_item`:569、`stack_confirm_item`:577、`stack_merge_items`:582（`child_id`,`into_id`）、`stack_unmerge_item`:591、`stack_delete_item`:596、`stack_refresh_detections`:601、`subscription_upsert`:606、`subscription_delete`:614、`spending_overview`:619 |
| 参数命名 | 全部 `#[tauri::command(rename_all = "snake_case")]` |
| 信封锁 | `fn run_api<T>(&self, action: impl FnOnce(&AgentHub) -> Result<T>) -> ApiResult<T>`（lib.rs:56）；lock poisoned → `ApiError::new(ErrorCode::Internal, "state lock poisoned")`；未初始化 → Internal + `init_error` |
| 前端映射 | `src/lib/api.ts:293-306` 11 方法全走 `callApi`；实证 `stackMergeItems: (childId, intoId) => callApi('stack_merge_items', { child_id: childId, into_id: intoId })` |
| 信封解析 | `src/lib/envelope.ts` — `ApiEnvelopeError` 携带 `code`/`retryable`/`requestId`/`details` |

**V1 结论**：snake_case 参数、camelCase 输出、`{ok,data|error,requestId}` 信封在真实 WebView 中双向生效；13 个 GUI 场景全部通过 IPC 落到 core，未出现参数名不匹配或反序列化错误（应用日志除一条系统级 `IMKCFRunLoopWakeUpReliable` 外无 IPC 错误）。

### 2.3 GUI 场景矩阵（13 场景，全部 PASS）

截图目录 `$FIX/shots3/`（全分辨率 2720×1754）。

| # | 场景 | 结果 | 证据文件 |
|---|---|---|---|
| G1 | 空状态 + 空态引导 | PASS：五格 0 + 明确文案 + 11 张检测卡 | `S2-baseline-before-cli-add.png` 前序态 |
| G2 | 确认（confirm） | PASS：badge `●未确认`→`●已确认`，新增「最近确认: 11/12/2026, 10:54」 | `G2-confirm-after.png` |
| G3 | 编辑订阅（预填 + 保存） | PASS：9 字段全预填；金额 20→25 → 行显示 `25.00 USD`，确认时间推进 | `G3-edit-sub-prefilled.png` / `G3-edit-sub-saved.png` |
| G4 | 合并（merge） | PASS：radio 列表**过滤自身与已合并项**；成功后卡显示 `Merged into <uuid>` + 「撤销并入」；成本总额不变 | `G4-merge-modal.png` / `G4-merge-done-undo-available.png` |
| G5 | 撤销合并（unmerge） | PASS：确认对话框文案完整；恢复独立；**合并/撤销前后成本恒为 25.00 USD / 300.00 USD**（P0-2） | `G5-unmerge-confirm-dialog.png` / `G5-unmerge-done.png` |
| G6 | 删除门禁（Cancel） | PASS：Cancel 后 CLI 校验 12 items 全在（**取消不产生写入**） | `G6-delete-confirm-gate.png` |
| G6b | 删除空条目（成功路径） | PASS：删后 `stack_items` 回到 12，audit 记 `stack.item.delete` | `G6b-delete-item-confirm.png` / `G6b-delete-item-done.png` |
| G6c | 删除有订阅条目（被拒） | PASS：红色 toast「删除失败 / 该条目仍有关联订阅或被合并引用，请先解除关联，或在目标条目上操作。」→ 对应 `RESOURCE_CONFLICT`（error.rs:33、contract.rs:46） | `G6c-delete-blocked-by-subscription.png` |
| G7 | 刷新幂等 | PASS：连点两次「检测刷新」items 仍 12；手动条目保留 | `G7-refresh-done.png` / `G7-refresh-twice.png` / `G11-refresh-includes-manual.png` |
| G8 | 订阅删除（有确认门禁） | PASS：对话框「删除订阅 / 将删除「{name}」的订阅记录。此操作无法撤销。」→ 删除后 DB 4 行、audit `subscription.delete` | `G8-sub-delete-confirm.png` / `G8-sub-deleted.png` |
| G9/G10 | 中英文切换 | PASS：Settings → Language → English 全站即时切换（`Dashboard`/`My AI Stack`/`Spending overview`/`Monthly equivalent`/`Annual equivalent`…）；toast `Language updated.` | `G9-english.png` / `G10-english-settings.png` / `G10-english-stack-list.png` |
| UX-1 | 聚合溢出（反向） | PASS：红色 banner「成本计算失败」+ 原始错误文本 + 「重试计算」；items 全部可见，**无 0.00** | `UX1-overflow-spending-error.png` |
| UX-2 | 信息不完整 | PASS：`Registered 4 / Upcoming 0 / Renewal unknown 4 / Incomplete 1 / Usage not estimated 1`；说明条逐项点名原因 | `UX2-incomplete-and-usage-chips.png` / `UX2-fresh-load-correct.png` |

**UX-1 banner 全分辨率确认文案**（勿读 1000 宽缩略图，downscale 会伪造数字）：

> 标题「成本计算失败」；正文「成本汇总计算出现异常，但这不是数据丢失：工具与订阅登记仍然完好。可稍后重试。」
> 原始错误：`invalid input: annual_equivalent_total 9200000000000000 exceeds the JSON safe integer bound 9007199254740991; the figure would lose precision in JSON`

**UX-2 说明条原文**：`Codex Pro: Missing amount · Missing currency`、`Perplexity API: Usage-based without bill data; cannot fold`。

> 教训（已写入验收纪律）：验收数字必须读全分辨率 crop（×2.72），1000 宽缩略图曾把 `27.50` 显示成 `2750`，导致误判为缺陷。

---

## 3. V2 — UX-1 反向测试（可失败的断言）

`src/__tests__/stack-ui.spec.ts` 变更（+27/−2）：

```ts
function assertNoFakeZero(pageText: string): void {
  // (?<!\d): match a true "0.00", not the tail of "20.00"/"30.00".
  expect(pageText).not.toMatch(/(?<!\d)0\.00 USD/)
}
```

新增反向测试 `UX-1 reverse: the fake-zero assertion itself fails when the UI regresses to a fabricated 0.00`：

- 输入 `['Codex','成本计算失败','0.00 USD'].join('\n')` → `expect(() => assertNoFakeZero(...)).toThrow()` **必须抛错**
- 输入 `'Codex\n成本计算失败\n30.00 USD'` → **不抛错**

**V2 结论 PASS**：断言不是空真。曾用 `not.toContain('0.00 USD')` 误伤 `30.00 USD` 变红，改用 `(?<!\d)` lookbehind 后通过——反向测试正是用来锁住这个回归。

---

## 4. V3 — CLI JSON spending 语义裁定

### 4.1 实测事实（同一 overflow fixture）

| 模式 | 退出码 | spending 错误 | items |
|---|---|---|---|
| human | **0** | 输出到 stderr：`spending overview failed: INVALID_INPUT invalid input: annual_equivalent_total 9200000000000000 exceeds the JSON safe integer bound 9007199254740991; the figure would lose precision in JSON (items above are still shown; nothing was changed)` | 完整输出 |
| `--json` | **0** | **完全丢弃，无任何字段提示** | `{"ok":true,"data":[...],"requestId":"..."}` |

补充事实：`--amount 4600000000000000 --period yearly` 被 `parse_amount_major` 按 exponent 2 转成 4.6e17 minor 后以 `INVALID_INPUT ... amount_minor 460000000000000000 exceeds the JSON safe integer bound 9007199254740991` 拒绝——这是**正确输入护栏**，不是缺陷。

### 4.2 裁定

`stack list --json` 的契约在 M1-B 已冻结为**只输出 items**（`data` 为数组）。M1-C 裁定：

1. **不为 M1-B 契约新增 spending 字段**（会破坏已冻结契约）
2. **必须显式记录该限制**：JSON 模式不覆盖 spending 语义；spending 计算失败在 JSON 模式不可观测
3. 建议后续（M1-D 或 M2）引入 `agenthub spending --json` 独立命令或 `warnings` 字段；实现前须过 ADR

**V3 结论 PASS（带已知缺口）**：缺口已记录，未把 JSON 模式宣称为覆盖 spending。

---

## 5. FAIL 项 — spending 概览陈旧缓存（GUI 缺陷，M1-C 只记录）

### 5.1 缺陷机制（代码事实）

| 位置 | 事实 |
|---|---|
| `src/stores/stack.ts:41-52` | `loadSpending(force = false)`：`if (spending.value && !force && spendingError.value === null) { return spending.value }` ——**与 TTL 无关的永不过期缓存** |
| `src/pages/StackView.vue:86-92` | `onMounted` 调 `await stackStore.load()` + `await stackStore.loadSpending()`（均**无 force**） |
| `src/pages/StackDetailView.vue:97` | `load()` 内 `const overview = await stackStore.loadSpending()`（无 force） |
| 写路径失效缺失 | `saveSubscription`（:142-186）末尾只 `await load()`；`doMerge`/`doUnmerge`/`saveItem`/`deleteItem` 都只 `await load(); await stackStore.load(true)` ——**全部只刷 items，不刷/不清 spending**。全仓无 `spending.value = null` 或 invalidate 调用 |
| 强制路径正常 | 列表页「检测刷新」等按钮走 `loadSpending(true)`，绕开缓存 |
| 文案矛盾 | `src/i18n/zh.ts:1532` `spendingEmpty: '成本概览尚未生成。登记订阅后重新进入本页即可生成。'` —— 实际「重新进入」**不会**重新生成 |

### 5.2 确定性复现（两次，方向相反）

**复现 A（新增订阅后不刷新）**：GUI 先加载（4 条订阅），CLI 增加 `CNY 68/月` → 导航离开再回 → 卡片仍 **Registered 4 且只有 USD 行，无 CNY 行**；同时刻 CLI 输出 `68.00 CNY / month, 816.00 CNY / year` + `27.50 USD / month, 330.00 USD / year`，DB 真值 `CNY|1|6800`、`USD|2|5500`（共 5 条）。证据：`S2-baseline-before-cli-add.png`、`S3-after-nav-back.png`、`S4-GUI-stale-vs-CLI.txt.png`、`S4-cli-vs-db.txt`。

**复现 B（删除订阅后不刷新，GUI 自身写入路径）**：在 GUI 详情页删除 `Perplexity API` 订阅 → 返回列表页 → 卡片仍 **Registered 5 / Usage not estimated 1**，而 DB 实际 **4 行且不含 usage 订阅**，CLI 输出 4 条事实。证据：`S6-after-sub-delete-stale.png`、`S6-GUI-stale-after-delete.txt`。

**修复证明**：重启应用后同一 fixture 显示 **Registered 5**（复现 A 时）与正确 CNY/USD 双行；复现 B 后重启亦回到 4。证据 `S5-GUI-fresh-correct.png`。

### 5.3 责任归属

- **core 计算正确**、**CLI 正确** → 缺陷位于 `src/stores/stack.ts` 缓存策略 + 各写路径未失效，属 GUI 层
- **M1-C 边界禁止改业务代码**，故本报告只记录与建议，不落修复

**建议修复方向**（超出 M1-C 授权，需单独批准）：
1. 写路径成功后 `await stackStore.loadSpending(true)` 或 `stackStore.spending = null`
2. `onMounted` 若沿用缓存，至少引入 `spendingLoadedAt` + TTL 对齐 items 的 `LIST_TTL_MS`（15s）
3. 修正 `src/i18n/zh.ts:1532` 文案使其与实际行为一致，或让「重新进入」真的重新生成

---

## 6. V4 — 口径修正（已在 M1-B 报告中校正，此处复核）

- commit 链按真实 Git 历史排列（`fc4088e`→`114794c`→`48a37af`→`7c059f7`），非实现顺序
- 接口计数分三层表述：**14 个 Service 方法** / **11 个 Tauri 命令对外暴露 9 个** + 3 个内部间接消费（`subscriptions`、`spending_overview_with_window`、`stack_audit_events`）/ **CLI 显式命令**（`stack` 6 个 + `sub` 3 个 = 9 个；`stack unmerge` 冻结为 CLI 不暴露）
- 零前端业务计算：GUI 全部经 `callApi` 落 core（§2.2）

---

## 7. 测试矩阵（HEAD = `7c059f7`）

| 套件 | 命令 | 结果 | 状态 |
|---|---|---|---|
| Rust core | `cargo test -p agenthub-core` | **149 passed / 0 failed**（18 个 test target，`CORE_EXIT=0`，无 `panicked`） | PASS |
| Rust CLI | `cargo test -p agenthub-cli` | **31 passed / 0 failed** | PASS |
| Tauri lib 编译 | `cargo check -p agenthub --lib` | exit 0，无错误输出 | PASS |
| 前端单测 | `pnpm test` | **13 files / 68 tests passed** | PASS |
| 前端构建 | `pnpm build` | `BUILD_EXIT=0`（vue-tsc + vite） | PASS |
| 纯 fixture 验收 | `AGENTHUB_HOME=$(mktemp -d) ./scripts/stack_acceptance.sh` | **41 passed / 0 failed** | PASS |

测试环境限制（如实记录，未删测试凑绿）：`pnpm test` 输出 `Not implemented: Window's scrollTo() method` 若干条，为 jsdom 未实现 API 的噪声，不影响断言；Keychain 相关测试本轮未触发沙箱失败。

---

## 8. `scripts/stack_acceptance.sh`（新增，纯 fixture，211 行）

完全离线、只用临时 `AGENTHUB_HOME`，覆盖 41 个断言：

- A 建/确认/编辑（省略 flag 不清字段）
- C **USD 20/月 + USD 120/年 → `30.00 USD / month, 360.00 USD / year`**（P0-1 冻结样例）
- D **CNY 68/月 → `816.00 CNY / year`，且 USD 行不受污染**（币种不混加）
- E unknown 金额保持 null（不填 0）；hybrid 固定部分 `35.00/420.00`；incomplete 2；usage_without_bills 1
- F merge 后 `40.00/480.00` 且两种链式合并 exit 4；G 有 active/cancelled 订阅均 exit 4、无 `--yes` exit 1、无订阅可删、`amount` 无 `currency` exit 3、`sub delete --yes` 成功
- audit 六类 event_key；`user_version=4`；`foreign_key_check` 干净
- **真实 `~/.agenthub` / `~/.codex` / `~/.claude` 前后指纹一致**

---

## 9. 零触碰真实用户数据核查

| 路径 | 文件数 | 08:00 后修改 | 结论 |
|---|---|---|---|
| `/Users/jonki/.agenthub` | 4903 | **0** | 未触碰 |
| `/Users/jonki/.claude` | 1225 | **0** | 未触碰 |
| `/Users/jonki/.codex` | 12777 | 194 | **非 agenthub 造成** |

`.codex` 194 项逐项归因：`plugins` 96、`.tmp` 41、`computer-use` 29、`vendor_imports` 12（其余零散），均为 Codex/WorkBuddy 自身活动；`~/.codex/config.toml` mtime 09:16:55 且 birth 09:15:02（应用启动前即被 Codex 重建）。

**决定性证据**：fixture 数据库 `operations` 表 **0 行**（分发 Executor 从未执行），`audit_events` 仅 stack/subscription 六类事件 —— 证明 agenthub 未对任何 Agent 目录做分发写入。

---

## 10. 交付物与结论

**交付物**：
- 本报告 `docs/proposals/M1_C_ACCEPTANCE_REPORT.md`
- `scripts/stack_acceptance.sh`（新增，纯 fixture）
- `src/__tests__/stack-ui.spec.ts`（修改，V2 反向测试）
- GUI 证据 31 张 PNG（`$FIX/shots3/`）+ 3 份文本对照（`S4-cli-vs-db.txt`、`S6-GUI-stale-after-delete.txt`、`S7-final-consistency.txt`）

**结论：M1-C READY（带 1 项已定性的 GUI 缺陷）**

- 四项门禁 V1/V2/V3/V4 全部 PASS
- 全量测试矩阵 PASS（core 149 / cli 31 / 前端 68 / 纯 fixture 41）
- 真实用户数据零触碰
- **唯一 FAIL**：spending 概览陈旧缓存（§5），core/CLI 正确、GUI 层缺陷；M1-C 边界内只记录，修复需单独授权

**未执行（授权范围外）**：`scripts/v31_acceptance.sh`、`scripts/real-acceptance.sh`（含真实读取）、任何 tag/Release/发布动作。

M1-C 至此停止，等待负责人审查。
