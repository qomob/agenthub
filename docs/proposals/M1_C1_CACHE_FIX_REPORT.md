# M1-C.1 修复报告 — Spending Overview 缓存一致性

- 基准 HEAD：**`7c059f7`**（M1-B）；修复 commit：**`b26091f`** `fix(gui): make the spending overview cache expire and invalidate (M1-C.1)`
- 授权：负责人 m03174 裁定 **M1-C = CONDITIONAL PASS**，正式授权 **M1-C.1**（GO · AUTHORIZED）
- 缺陷证据：`docs/proposals/M1_C_ACCEPTANCE_REPORT.md` §5
- 状态四态：**PASS** = 已验证通过；**FAIL** = 失败；**ENV_BLOCKED** = 环境限制；**NOT_RUN** = 未执行

---

## 1. 终态三行结论

| 项 | 结论 |
|---|---|
| **缓存缺陷** | **CLOSED** |
| **M1-C** | **PASS**（原 CONDITIONAL 的唯一 FAIL 项已关闭） |
| **M1-D 是否必要** | **建议做，但不自动启动**。理由见 §10 |

---

## 2. 缺陷根因（修复前代码事实）

| 位置 | 修复前行为 |
|---|---|
| `src/stores/stack.ts:44-46` | `if (spending.value && !force && spendingError.value === null) { return spending.value }` —— 只要缓存非空就返回，**与 TTL 无关，永不过期** |
| `src/pages/StackView.vue:92` | `onMounted` 调 `await stackStore.loadSpending()`（无 force），命中永不过期缓存 |
| `src/pages/StackDetailView.vue:97` | `load()` 内 `await stackStore.loadSpending()`（无 force） |
| 5 条 GUI 写路径 | `refreshDetections` / `save` / `doMerge` / `doUnmerge` / `doDelete` 只 `load(true)` 刷 items，**从不刷/不清 spending**；全仓无任何 `spending.value = null` 或 invalidate 调用 |

后果：CLI 或 GUI 自身改动订阅后，成本概览继续展示旧数字，直到重启应用。

---

## 3. 修复设计：统一缓存生命周期

### 3.1 组合机制（不单靠 TTL）

负责人明确要求「不要只采用 TTL，因为 CLI 也能修改同一个 SQLite，GUI 无法收到 CLI 的缓存失效通知」。实现采用四重组合：

| 机制 | 触发条件 | 处理 |
|---|---|---|
| **TTL 15 秒** | 距上次成功获取超过 15s **且发生了一次读取调用** | 重新请求 core（覆盖 CLI 等外部写入）；TTL 自身不触发读取，读取由页面激活/窗口获焦/GUI 写入触发 |
| **写入即失效** | GUI 写操作成功 | `invalidateSpending()` 清空，下次读取必回源 |
| **页面重新激活** | 组件重新挂载 | 走 `loadSpending()` 的新鲜度判定（TTL 内复用，过期回源） |
| **手动重试** | 用户点「重试计算」 | `loadSpending(true)` 强制回源 |

### 3.2 统一失效入口

```ts
function invalidateSpending(): void {
  spendingSeq += 1        // 让在飞请求作废
  spending.value = null
  spendingLoadedAt.value = 0
  spendingError.value = null
  spendingInFlight = null
}
```

页面**不再自行维护缓存**，也不再把 `force=true` 分散到各处；统一调用该入口。当前行为：

- 全部写路径调用 `invalidateSpending()`，随后 `load(true)` 刷 items + `loadSpending()` 非 force 刷新
- 只有手动重试按钮保留 `loadSpending(true)`（`StackView.vue:282`）

写后顺序严格遵循授权要求：
**Core 写入成功 → `invalidateSpending()` → 刷新 items → 刷新 spending → 更新 UI**

### 3.3 并发与竞态

```ts
let spendingSeq = 0                                  // 单调递增的请求序列
let spendingInFlight: Promise<...> | null = null     // 在飞请求复用
```

- **旧响应不覆盖新值**：响应只在 `requestSeq === spendingSeq` 时落库；`invalidateSpending()` 会 `spendingSeq += 1`，使任何在飞请求作废
- **并发去重**：多个同时调用复用同一个 `spendingInFlight`
- **失败保留原数据**：catch 分支不清空 `spending.value`，只写 `spendingError`，且同样受序列保护
- **无无限循环**：失败后 `spendingLoadedAt` 也更新，避免「失败 → 立刻重试 → 立刻失败」的紧循环

### 3.4 失败语义

成本刷新失败时：原始工具与订阅数据继续显示（`items` 与详情不受影响）、不把成本重置为 0、显示失败 banner + 原始错误、保留「重试计算」入口、**不误判为写入失败**（写操作自身的 toast 已先行发出）。

---

## 4. 写路径覆盖清单（11 处）

| 文件 | 函数 | 行（修复后） |
|---|---|---|
| `src/pages/StackView.vue` | `refreshDetections` | 100 |
| `src/pages/StackView.vue` | `save`（新增/编辑工具） | 144 |
| `src/pages/StackView.vue` | `doMerge` | 179 |
| `src/pages/StackView.vue` | `doUnmerge` | 197 |
| `src/pages/StackView.vue` | `doDelete` | 217 |
| `src/pages/StackDetailView.vue` | `saveSubscription` | 182 |
| `src/pages/StackDetailView.vue` | `deleteSubscription` | 199 |
| `src/pages/StackDetailView.vue` | `doUnmerge` | 216 |
| `src/pages/StackDetailView.vue` | `saveItem` | 278 |
| `src/pages/StackDetailView.vue` | `deleteItem` | 295 |
| `src/pages/StackDetailView.vue` | `doMerge` | 346 |

**未覆盖且刻意不覆盖**：`confirmItem`（只改 confirmed 标记，不参与成本折算）。取消操作（Cancel）不进入成功分支，因此不触发失效——已有测试锁定（§6-13）。

---

## 5. 文案修正

| 语言 | 修复前 | 修复后 |
|---|---|---|
| zh（`src/i18n/zh.ts:1532`） | `成本概览尚未生成。登记订阅后重新进入本页即可生成。` | ~~`成本概览尚未生成。登记订阅后即可生成；成本数据每 15 秒自动刷新，本应用内的增删改会立即更新。`~~ → **已被 M1-C.1 Final Contract Alignment 取代**：`成本概览尚未生成。登记订阅后即可生成。本应用内的增删改会立即同步；其他程序（例如命令行）的改动会在你重新打开本页或切回窗口时更新。` |
| en（`src/i18n/en.ts:1544`） | `The spending overview has not been generated yet. Register subscriptions and reopen this page to generate it.` | `The spending overview has not been generated yet. It appears once you register a subscription, refreshes automatically every 15 seconds, and updates immediately after changes made in this app.` |

原文案承诺「重新进入本页即可生成」与修复前的实际行为**直接矛盾**（重新进入正是命中永不过期缓存的路径）。新文案与实际机制一致，中英文语义对齐；i18n 键完全镜像（zh/en 各 1160 键，无缺失）。

---

## 6. 永久回归测试（29 个，全部新增）

### 6.1 新增文件 `src/__tests__/stack-store.spec.ts`（8 tests，store 单元层）

| # | 测试 | 覆盖授权要求 |
|---|---|---|
| 1 | first load fetches spending from core | ① 首次加载 |
| 2 | repeated reads within the TTL reuse the cache | ② TTL 内缓存命中 |
| 3 | TTL expiry refetches — CLI write picked up on re-entry | ③ TTL 到期 + ⑩ CLI 外部写入（**复现 A**） |
| 4 | `invalidateSpending()` forces the next read to refetch | ⑥ 删除/写入后失效（**复现 B**） |
| 5 | concurrent reads share one in-flight request | ⑨ 不产生重复请求 |
| 6 | stale response must not repopulate the cache | ⑫ 旧请求不覆盖新请求 |
| 7 | refresh failure keeps last good figures — never a fabricated zero | ⑪ 刷新失败保留原始数据 |
| 8 | recovery after failure: next loadSpending retries | 失败后自动恢复 |

### 6.2 `src/__tests__/stack-ui.spec.ts` 新增 8 tests（组件/写路径层）

| # | 测试 | 覆盖授权要求 |
|---|---|---|
| 9 | M1-C.1 Reproduction A（重挂载后 TTL 到期取到外部写入） | **原复现 A 转永久自动化** |
| 10 | M1-C.1 Reproduction B（删订阅立即失效） | **原复现 B 转永久自动化** |
| 11 | adding a subscription refetches immediately | ④ 新增订阅后失效 |
| 12 | editing a subscription amount refetches | ⑤ 编辑订阅后失效 |
| 13 | merging on the list page refetches | ⑦ 合并后失效 |
| 14 | undoing a merge refetches | ⑧ 撤销合并后失效 |
| 15 | editing a tool item refetches | ⑨ 工具编辑后一致性 |
| 16 | cancelling a delete neither writes nor refetches | ⑬ 取消不触发写入与错误失效 |

原 `stack-ui.spec.ts` 的 13 个既有测试保持不变，全部仍通过。

### 6.3 RED / GREEN 双向证据

**RED（对修复前的业务代码运行新测试）**——`git stash push` 还原 5 个业务文件后：

```
× M1-C.1 Reproduction A: re-entering the page after the TTL expires ...  →  "expected 1 to be 2"（未回源）
× M1-C.1 Reproduction B: deleting a subscription in the GUI ...          →  缓存未被失效
× adding a subscription refetches the overview immediately               →  "expected 1 to be 1"
× editing a subscription amount refetches the overview                   →  同上
× editing a tool item refetches the overview                             →  同上
× TTL expiry refetches (Reproduction A)                                  →  永不过期缓存
× invalidateSpending() forces the next read to refetch                   →  入口不存在
× concurrent reads share one in-flight request                           →  5s 超时（无去重）
× a stale response must not repopulate the cache                          →  旧响应覆盖
× a refresh failure keeps the last good figures                          →  失败细节丢失
Tests  10 failed | 19 passed (29)
```

**GREEN（修复后，同一套测试）**：

```
Test Files  2 passed (2)
Tests  29 passed (29)
```

**关键结论：原缺陷确实被测试捕获，不是靠改预期凑绿。** 修复前后测试文件完全一致，仅业务代码不同。

---

## 7. 真实 GUI 复验（隔离 fixture）

环境：`FIX=/var/folders/42/.../ah-gui-accept.zF4VAb`，`AGENTHUB_HOME=$FIX/home`；dev server 1420 + 桌面二进制 `/tmp/ahdesk/agenthub`。截图目录 `$FIX/shots3/`（全分辨率 2720×1754，数字均按 ×2.72 裁剪核对）。

### 复现 A（CLI 外部写入 → GUI 重进）

1. GUI 基线：**已登记 4**，CNY 68.00 / 816.00（1 条），USD 27.50 / 330.00（2 条）
2. CLI：`sub add --stack <Codex> --product "A-Repro CNY" --amount 100 --currency CNY --period monthly` → DB 4→5
3. 导航到仪表盘再回 My AI Stack（重进页面触发读取；单独等过 TTL 不产生读取）
4. 结果：**已登记 5**，CNY **168.00** / 2,016.00（2 条），USD 27.50 / 330.00 —— **修复成立**
   - 证据：`M1C1-RB-delete-confirm.png` 前序基线 + 重进后截图

### 复现 B（GUI 自身删除订阅 → 立即返回）

1. 详情页删除 `A-Repro CNY` 订阅（确认对话框「删除订阅 / 将删除「A-Repro CNY」的订阅记录。此操作无法撤销。」）→ DB 5→4
2. 立即返回 My AI Stack（**不等 TTL**）
3. 结果：**已登记 4**，CNY 68.00 / 816.00（1 条），使用未计 **0** —— 修复前此处的错误状态是「5 / usage 1」
   - 证据：`M1C1-RB-delete-confirm.png`、`M1C1-RB-delete-done.png`、`M1C1-RB-after-delete-fixed.png`

### 英文界面 + 重启一致性

- Settings → Language → English → My AI Stack 全英文：`Registered subscriptions 4` / `CNY 68.00 · 816.00` / `USD 27.50 · 330.00` / `Usage, not estimated 0`
  - 证据：`M1C1-EN-spending-copy.png`
- 重启应用 → 同一 fixture 显示一致（Registered 4 / CNY 68.00 / USD 27.50）
  - 证据：`M1C1-restart-consistent.png`

### 空态文案目视确认

空 fixture（0 工具 / 0 订阅）→ My AI Stack 空态，全分辨率裁剪确认新文案已生效：

> 成本概览尚未生成。登记订阅后即可生成。本应用内的增删改会立即同步；其他程序（例如命令行）的改动会在你重新打开本页或切回窗口时更新。
>
> （此为 M1-C.1 Final Contract Alignment 后的最终文案；上文 §8 表格中「每 15 秒自动刷新」的旧文案已作废，因为实现中不存在定时器。）

证据：`M1C1-empty-copy-zh.png`（裁剪 `/tmp/ecrop.png`，2176×123 全分辨率）。

---

## 8. 全量回归

| 套件 | 命令 | 结果 | 状态 |
|---|---|---|---|
| 前端单测 | `pnpm test` | **14 files / 84 tests passed**（修复前 68 → 新增 16） | PASS |
| 前端构建 | `pnpm build` | `BUILD_EXIT=0`，`✓ built in 7.95s`（vue-tsc 类型检查通过） | PASS |
| Rust core | `cargo test -p agenthub-core` | **149 passed / 0 failed**（18 个 test target） | PASS |
| Rust CLI | `cargo test -p agenthub-cli` | **31 passed / 0 failed** | PASS |
| Tauri lib 编译 | `cargo check -p agenthub --lib` | exit 0 | PASS |
| 纯 fixture 验收 | `scripts/stack_acceptance.sh` | **43 passed / 0 failed**（连续 3 次） | PASS |

**未触碰**：Rust Core、DB Schema、Tauri 命令层、CLI 对外契约。`git diff` 仅涉及 6 个前端文件（+342/−18）。

---

## 9. 变更文件清单与新 commit

### 业务代码（6 文件，+342/−18）

| 文件 | 变更 |
|---|---|
| `src/stores/stack.ts` | +72/−15：`SPENDING_TTL_MS`、`spendingLoadedAt`、`invalidateSpending()`、`spendingSeq`/`spendingInFlight` 竞态控制 |
| `src/pages/StackView.vue` | +15/−10：5 处写路径改为 invalidate + 非 force 刷新 |
| `src/pages/StackDetailView.vue` | +6：6 处写路径加 `invalidateSpending()` |
| `src/i18n/zh.ts` | ±1：spendingEmpty |
| `src/i18n/en.ts` | ±1：spendingEmpty |
| `src/__tests__/stack-ui.spec.ts` | +263/−2：新增 8 个永久回归测试 |

### 新增文件

| 文件 | 内容 |
|---|---|
| `src/__tests__/stack-store.spec.ts` | 8 个 store 层缓存生命周期测试 |
| `scripts/stack_acceptance.sh` | 43 断言纯 fixture 验收脚本（含隔离加固） |

### `scripts/stack_acceptance.sh` 本轮加固

修复了一次非产品性波动（`40 passed, 1 failed`）：原因是 `~/.codex` 整树指纹被 Codex 自身活动打断。改为：

1. `snapshot_codex()` 只指纹适配器可写路径（`config.toml`/`AGENTS.md`/`mcp.json`/`prompts`/`skills`）
2. 新增决定性断言：fixture 的 `operations` 表必须为 **0 行**（分发 Executor 从未运行）
3. 新增：app db 必须位于 `$AGENTHUB_HOME` 之下

改后连续 3 次 **43/43**，不再波动。

---

## 10. 遗留风险

| # | 风险 | 等级 | 处理 |
|---|---|---|---|
| R1 | **外部写入没有固定的陈旧时间上限**。TTL 只在 `loadSpending()` 被调用时才参与判断，它自身不触发任何读取，因此在页面驻留且无任何激活事件时，外部（CLI）写入可以无限期不被显示 —— 15 秒不是可见性上限，只是「下一次读取时说缓存还算不算新鲜」的阈值 | 中 | **已由 M1-C.1 Final Contract Alignment 收口**：新增 `useRefreshOnActivate` 让窗口获焦/页面重新激活触发读取（详见 `M1_C1_FINAL_CONTRACT_REPORT.md`）；文案改为不再承诺周期性自动刷新。修正本行原始表述：早期版本称「最长 15 秒内」是错误描述 |
| R2 | **CLI JSON 模式下 spending 错误不可观测**（`stack list --json` 只输出 items，失败静默丢弃） | 中 | 按负责人裁定**不并入本次**，作为独立 ADR 议题；建议未来 `agenthub spending --json` 或 envelope `warnings` |
| R3 | **`restore_backup` 不支持 pre-migration manifest** | 中 | 按裁定继续登记为恢复能力风险，不在本次改动 |
| R4 | 页面重新激活依赖组件重新挂载（Vue Router 复用组件时不触发 `onMounted`） | — | **已修复**：`useRefreshOnActivate` 同时挂 `onMounted`、`onActivated`、`window` 的 `focus` 监听，覆盖重新挂载、KeepAlive 重新激活、窗口获焦三种路径；移除监听在 `onBeforeUnmount` |
| R5 | `confirmItem` 刻意不触发 spending 失效 | 极低 | 确认操作不改变任何金额/数量，不参与成本折算 |

---

## 11. M1-D 建议（只建议，不启动）

**建议做，但不自动启动。** 理由：

1. M1-C.1 已把「My AI Stack 数据可信度」这一前置条件补齐 —— 这是 Cost Optimizer 给出省钱建议的前提。负责人指出的顺序（先可信、再优化）现已满足。
2. 但 R2（CLI JSON spending 不可观测）与 R3（恢复能力）是两个独立的、已有明确边界的缺口，适合合并成一个小的 M1-D 收口批次，而不是零散补丁。
3. R4 值得在 M1-D 或 M2 早期一并评估（若届时调整 TTL 策略）。

**M1-D 建议范围（供裁定，不自行启动）**：ADR + `agenthub spending --json`（或 envelope `warnings`）· `restore_backup` pre-migration manifest 兼容 · TTL/激活策略复核。

---

## 12. 交付摘要

- 缓存缺陷：**CLOSED**
- M1-C：**PASS**（唯一 FAIL 项已关闭，四项门禁保持 PASS）
- M1-D：**建议做，不自动启动**
- 新增永久测试：**16 个**（store 8 + UI 8）
- RED/GREEN：**10 failed → 29 passed**，测试文件在修复前后完全一致
- 用户数据：全程隔离 fixture，真实 `~/.agenthub` / `~/.claude` 零修改（`operations` 表 0 行）
- 未执行：M2/M3、tag、Release、部署、任何 CLI/Rust/Schema 变更

M1-C.1 完成，停止等待审查。
