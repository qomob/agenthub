# M1-C.1 Final Contract Alignment — 刷新契约收口报告

- 基准 commit：**`b26091f`**（`fix(gui): make the spending overview cache expire and invalidate (M1-C.1)`）
- 授权来源：负责人 M1-C.1 终审裁定（CONDITIONAL PASS + 小范围授权「Final Contract Alignment」）
- 本阶段性质：**只收口刷新契约不一致，不开新能力**
- 终态：**M1-C = PASS（建议）**；缓存缺陷 **CLOSED**；刷新契约 **CLOSED**

---

## 1. Executive Summary

上一轮我声称缓存有「15 秒 TTL」，因此外部（CLI）写入最多 15 秒后可见，并在 zh/en 空态文案中写下「成本数据每 15 秒自动刷新」。负责人的判断是正确的：**TTL 不是定时器**。TTL 只在 `loadSpending()` 被调用时才参与判断，它自身不触发任何读取。本次用真实 Tauri 应用做了三组隔离实验，逐项证实/证伪，然后按实测结果修复触发时机与文案。

结论：
1. 页面驻留不导航不点击，**40 秒内零刷新**（真实 GUI，逐字节比对截图一致）——「每 15 秒自动刷新」是**虚假承诺**，已删除。
2. 重新进入页面会刷新——但机制是 **Vue Router 重新挂载触发 `onMounted`**，不是 TTL 生效，也不是 `onActivated`（全仓无 `<KeepAlive>`）。
3. 窗口重新获焦在修复前**不触发任何读取**——这是真实缺口，已补 `useRefreshOnActivate`。
4. 修复后真实 GUI 验证：窗口获焦即刷新（7 条 / CNY 248.00）；仍不做任何交互时驻留 40 秒依旧零刷新（**没有虚构定时器**）。

---

## 2. 步骤 1：先验证，不先修改

三组实验都在**真实 Tauri 应用**（非 jsdom、非 mock）中进行。

### 环境

| 项 | 值 |
|---|---|
| 隔离 home | `FIX=/var/folders/42/0_vh3ptx7735d89p2rz0kj980000gn/T//ah-gui-accept.zF4VAb` |
| 数据目录 | `$FIX/home/.agenthub/agenthub.db`（lsof 确认进程只打开此库及 -wal/-shm） |
| 启动方式 | `AGENTHUB_HOME=$FIX/home AGENTHUB_DATA_DIR=$FIX/home/.agenthub /tmp/ahdesk/agenthub` |
| 前端 | `pnpm dev`（Vite 1420，`/tmp/ah_vite.log`），Tauri 走 devUrl |
| 窗口 | `76,38,1360,870`；截图 `screencapture -x -R76,38,1360,870`（2720×1740），数字判定一律读全分辨率裁剪 |
| 真实用户目录 | 全程零触碰（脚本断言 + 前序 M1-C 已取证） |

### 实验一：页面驻留、无任何交互，是否自动刷新？

1. GUI 停在 My AI Stack，基线 4 条订阅 / CNY 68.00。
2. CLI 在**同一 SQLite** 上执行：
   ```
   AGENTHUB_HOME=$FIX/home ./target/debug/agenthub sub add \
     --stack <ChatGPT-id> --product "C-Resident CNY" \
     --billing-type subscription --amount 100 --currency CNY --period monthly
   ```
   库内 `subscription_records` 4 → 5。
3. **不导航、不点击、不切窗口**，T+8s / T+18s / T+28s / T+40s 各截一次成本卡裁剪图。

**结果**：四张裁剪图 `shasum -a 256` 唯一值 = **1**，即 40 秒内像素完全一致。UI 始终显示 4 条 / CNY 68.00，而库内已是 5 条 / CNY 168.00。

> **判定：TTL 不构成定时器。页面驻留时外部写入可以无限期不被显示。**「每 15 秒自动刷新」不成立。

### 实验二：重新进入页面时到底是谁触发的读取？

1. AX 触发 `Dashboard` → 再 `My AI Stack`。
2. 成本卡由 4 条变为 **5 条 / CNY 168.00**（外部写入被看到）。

进一步核对机制：
- `src/App.vue:179` 只有裸 `<RouterView />`，**没有 `<KeepAlive>`**。
- 全仓 `grep -rn "KeepAlive\|onActivated\|onDeactivated" src/`（排除测试与本轮新增 composable）**无命中**。
- 因此路由切换会**卸载并重新挂载**视图组件，`onMounted` 再次执行 → 读取发生。

> **判定：「重新进入会刷新」成立，但机制是组件重新挂载，不是 TTL，也不是 `onActivated`。** 三者不可混为一谈。

### 实验三：窗口重新获焦是否触发刷新？

1. 此时库内 5 条，GUI 显示 5 条（已同步）。
2. CLI 再写第 6 条（`"D-Focus CNY"`，50 CNY/月），`subscription_records` 5 → 6。
3. 把焦点切到 Finder（blur），再切回 agenthub（focus）——**不导航、不点击页面内任何元素**。

**结果**：成本卡仍是 5 条。

> **判定：修复前窗口获焦不触发任何读取，是真实缺口。** 桌面应用里「切回窗口」是最自然的复查时刻，此处应当刷新。

---

## 3. 步骤 2：修复策略与实现

原则：**保留 TTL + 写入即失效，只补真实存在的激活触发；不新增后台轮询、数据库监听或网络能力。**

### 3.1 新增 `src/composables/useRefreshOnActivate.ts`（49 行）

```ts
import { onActivated, onBeforeUnmount, onMounted } from 'vue'

export function useRefreshOnActivate(refresh: () => void, options: { focus?: boolean } = {}): void {
  const useFocus = options.focus ?? true

  function onFocus(): void {
    refresh()
  }

  onMounted(() => {
    refresh()
    if (useFocus && typeof window !== 'undefined') {
      window.addEventListener('focus', onFocus)
    }
  })

  onActivated(() => {
    refresh()
  })

  onBeforeUnmount(() => {
    if (useFocus && typeof window !== 'undefined') {
      window.removeEventListener('focus', onFocus)
    }
  })
}
```

三条激活路径各自对应一个真实事件：

| 路径 | 触发点 | 覆盖的场景 |
|---|---|---|
| `onMounted` | 组件挂载 | 今天的行为：路由切换会重新挂载（无 KeepAlive） |
| `onActivated` | KeepAlive 重新激活 | 未来若启用 `<KeepAlive>` 时的兜底，`onMounted` 不会重跑 |
| `window` `focus` | 窗口重新获焦 | **本次修复的真实缺口**：页面驻留时切回窗口即可看到外部写入 |

**没有 `setInterval`、`setTimeout`、`watch` 数据库、WebSocket、轮询。** 刷新永远由真实激活事件触发；TTL 仍决定该次读取命不命中缓存。

### 3.2 接入两处视图

**`src/pages/StackView.vue`**：删除原 `onMounted(async () => {...})`，改为

```ts
useRefreshOnActivate(() => {
  void refreshFromSource()
})

async function refreshFromSource(): Promise<void> {
  try {
    await stackStore.load()
  } catch {
    // error state is surfaced by the banner
  }
  await stackStore.loadSpending()
}
```

`from 'vue'` 的导入去掉 `onMounted`。

**`src/pages/StackDetailView.vue`**：删除 `onMounted(load)`，保留 `watch(itemId, load)`，改为

```ts
useRefreshOnActivate(() => {
  void load()
})
```

### 3.3 文案修正（删除虚假承诺）

| 位置 | 修正前 | 修正后 |
|---|---|---|
| `src/i18n/zh.ts:1532` | `成本概览尚未生成。登记订阅后即可生成；成本数据每 15 秒自动刷新，本应用内的增删改会立即更新。` | `成本概览尚未生成。登记订阅后即可生成。本应用内的增删改会立即同步；其他程序（例如命令行）的改动会在你重新打开本页或切回窗口时更新。` |
| `src/i18n/en.ts:1544` | `The spending overview has not been generated yet. It appears once you register a subscription, refreshes automatically every 15 seconds, and updates immediately after changes made in this app.` | `The spending overview has not been generated yet. It appears once you register a subscription. Changes made in this app sync immediately; changes made by another program (the CLI, for example) refresh when you reopen this page or switch back to the window.` |

新文案只承诺两件**确实会发生**的事：本应用内写入立即同步；外部程序的改动在重开本页或切回窗口时更新。**不再出现「自动刷新」「每 15 秒」这类没有定时器支撑的措辞。**

### 3.4 同步修正上一份报告的 R1 / R4

`docs/proposals/M1_C1_CACHE_FIX_REPORT.md`：

- **R1** 原文「TTL 最坏情况有 15 秒陈旧窗口 / CLI 写入后最长 15 秒内…」是**错误描述**，已改写为：「外部写入没有固定的陈旧时间上限……15 秒不是可见性上限，只是『下一次读取时说缓存还算不算新鲜』的阈值」，等级由「低」上调为「中」，并标注已由本轮收口。
- **R4** 原文「页面重新激活依赖组件重新挂载……需补 `onActivated` 钩子」已标记**已修复**，说明三种激活路径均已覆盖。
- §8 文案表格中的旧文案加删除线并指向最终文案；四重机制表中 TTL 一行补注「**且发生了一次读取调用**」。

---

## 4. 步骤 3：6 类永久回归测试

新增 `src/__tests__/stack-refresh-contract.spec.ts`，**8 个测试**，使用 `vi.useFakeTimers()` + `vi.advanceTimersByTime(15_000)`。

| # | 测试名 | 对应要求 |
|---|---|---|
| 1 | `a page left untouched never refreshes itself, even long past the TTL` | ①TTL 到期但无读取触发时不虚构自动刷新（advance TTL×4，读取次数不变） |
| 2 | `window focus after the TTL expires picks up an external (CLI) write` | ⑤窗口重新激活后刷新行为符合实现 |
| 3 | `window focus inside the TTL is a cache hit, not an extra read` | ⑤补充：TTL 内的获焦是命中，不产生额外请求 |
| 4 | `re-entering the page after the TTL expires refetches` | ②重新进入页面后过期缓存重取 |
| 5 | `the window focus listener is removed on unmount` | ③生命周期正确性（不泄漏监听） |
| 6 | `the GUI's own write updates immediately, without waiting for the TTL` | ④GUI 自身写入立即更新 |
| 7 | `never promises a periodic automatic refresh that no timer implements` | ⑥中英文案与真实机制一致（断言 zh 不含「自动刷新」「每 15 秒」，en 不含 `automatically every 15 seconds`、`refreshes automatically`） |
| 8 | `states the two things that really do refresh: own writes and re-activation` | ⑥补充：文案必须正面陈述两条真实路径 |

> 关于 ③Router 复用场景：实测本项目**没有 `<KeepAlive>`**，路由切换会重新挂载组件，因此「Router 复用」在今天的代码里等价于 `onMounted` 路径（由测试 4 覆盖）。`onActivated` 分支为将来启用 KeepAlive 预留，属于**明确契约**而非当前生效代码路径——报告在此明确声明，不假装它已被真实路由复用验证。

### 双向 RED 证据（证明测试真的能抓到缺陷）

| 破环方式 | 结果 |
|---|---|
| 把 `StackView.vue` 改回 `onMounted` 版本（无 focus 监听） | `Tests 1 failed \| 7 passed`，失败项为 `window focus after the TTL expires picks up an external (CLI) write` |
| 把 zh/en 文案改回「每 15 秒自动刷新」版本 | `Tests 2 failed \| 6 passed`，失败项为 `never promises a periodic automatic refresh` 与 `states the two things that really do refresh` |
| 恢复修复后代码 | **8/8 passed** |

测试不是靠改预期凑绿——两种回退都会精确打击对应断言。

### 类型检查踩坑（记录）

- 测试内 `mockImplementation(async () => [item()])` 触发 `TS2322`（`StackItemView` 无字符串索引签名），改为 `[fixtures.item()] as never` / `fixtures.spending() as never`。
- `i18n.global.t(key, {}, { locale: 'zh-CN' })` 触发 `TS2769`：本项目 locale 联合类型只有 `'zh' | 'en'`，改用 `'zh'` / `'en'`。
- `npx vue-tsc --noEmit` → **exit 0**。

---

## 5. 步骤 4：修复后的真实 GUI 验证

同一个隔离 fixture、同一个真实 Tauri 应用，重启后：

| 步骤 | 期望 | 实测 |
|---|---|---|
| 重启应用，进入 My AI Stack | 反映库内 6 条 | **6 条 / CNY 218.00** ✅ |
| CLI 写第 7 条（`E-FocusFix CNY` 30 CNY/月），等 17s（超 TTL） | 等待本身不刷新 | 无变化 ✅ |
| 仅 blur 到 Finder 再回焦 agenthub（不导航不点击） | 刷新 | **7 条 / CNY 248.00 / 2,976.00** ✅ 截图 `$FIX/shots3/FC-2-focus-refresh-picked-up.png` |
| CLI 写第 8 条（`F-NoTimer CNY` 10 CNY/月），**不做任何交互等 40s** | 不自刷新（无定时器） | T+0 与 T+40 裁剪图 `shasum` 唯一值 = **1** ✅ 截图 `$FIX/shots3/FC-1-resident-no-autorefresh.png` |

> 截图采集纪律：`screencapture -R76,38,1360,870` 只按屏幕坐标裁切，若 agenthub 不在最前会采到别的应用（过程中曾误采到 ChatGPT / DeepSeek Harness / Finder）。每次必须先 `set frontmost to true`、必要时 `perform action "AXRaise" of window 1`，并确认 `name of first process whose frontmost is true` == `agenthub` 后才截图。数字判定一律用全分辨率裁剪（×2.72），不用 1000 宽缩略图。

---

## 6. 缓存最大陈旧时间的准确描述（关键交付项）

这是本轮最重要的口径修正。分三种情况，必须分开表述：

| 场景 | 最大陈旧时间 | 机制 |
|---|---|---|
| **本应用内写入**（GUI 新增/编辑/删除订阅、合并、撤销合并、编辑/删除工具、刷新检测） | **0（立即）** | 写成功后 `invalidateSpending()` 清缓存，随后刷新；不看 TTL |
| **页面重新激活**（重开本页 / 切回窗口 / KeepAlive 重入） | **≤ 一次往返**（TTL 过期时立即重取；TTL 内命中则显示缓存，最多 15s 内的数据） | `useRefreshOnActivate` 触发读取，TTL 决定命中 |
| **页面驻留且无任何激活事件**，外部程序（CLI）改了库 | **无上限**（可以永久不显示） | 没有定时器、没有数据库监听、没有轮询；只有下一次激活事件才会读取 |

**因此：不存在「外部写入 15 秒内必定可见」的保证。** 15 秒只是「下一次读取时说缓存还算不算新鲜」的阈值。用户可见的正确行为是：切回窗口或重开本页即更新；一直盯着不动的页面不会自己变。

---

## 7. 修改文件清单

```
新增  src/composables/useRefreshOnActivate.ts              49 行
新增  src/__tests__/stack-refresh-contract.spec.ts        264 行（8 tests）
修改  src/pages/StackView.vue                             onMounted → useRefreshOnActivate + refreshFromSource
修改  src/pages/StackDetailView.vue                       onMounted(load) → useRefreshOnActivate
修改  src/i18n/zh.ts:1532                                 spendingEmpty 文案
修改  src/i18n/en.ts:1544                                 spendingEmpty 文案
修改  docs/proposals/M1_C1_CACHE_FIX_REPORT.md            R1/R4/文案表/机制表 口径修正
```

`git diff --stat`（相对 `b26091f`）：4 个已跟踪文件 **+24 / −7**，另 2 个新文件未跟踪。**未改动任何 Rust 代码、DB schema、Tauri 命令或 CLI 契约。**

---

## 8. 全量回归

| 项目 | 命令 | 结果 |
|---|---|---|
| 前端单测 | `pnpm test` | **15 files / 92 tests passed**（原 14/84，新增 8） |
| 类型检查 + 构建 | `pnpm build` | **BUILD_EXIT=0** |
| 类型检查单项 | `npx vue-tsc --noEmit` | **exit 0** |
| Core | `cargo test -p agenthub-core` | **149 passed / 0 failed** |
| CLI | `cargo test -p agenthub-cli` | **31 passed / 0 failed** |
| Tauri 库 | `cargo check -p agenthub --lib` | **exit 0** |
| 端到端脚本 | `scripts/stack_acceptance.sh` | **43 passed / 0 failed**（纯 fixture） |

---

## 9. 遗留风险

| # | 风险 | 等级 | 处理 |
|---|---|---|---|
| C1 | 驻留页面对外部写入无上限陈旧（见 §6 第三种场景） | 中 | **设计接受**，因为授权明确禁止后台轮询/数据库监听；已用文案如实告知用户「切回窗口或重开本页即更新」。若未来产品要求「后台自动跟随」，需引入文件监听（notify）或定时器，属新能力、需单独授权 |
| C2 | `onActivated` 分支当前无真实路由复用可验证（无 KeepAlive） | 低 | 已作为明确契约预留并在报告中声明；若将来启用 KeepAlive，需补一条真实路由复用测试 |
| C3 | 窗口 `focus` 在极少数窗口管理器下可能不触发 | 极低 | macOS/Tauri 实测有效；`onMounted` 与 `onActivated` 仍提供兜底 |
| C4 | CLI JSON 模式下 spending 错误不可观测（沿用 R2） | 中 | 按裁定不并入本次，独立 ADR 议题（`agenthub spending --json` 或 envelope `warnings`） |
| C5 | `restore_backup` 不支持 pre-migration manifest（沿用 R3） | 中 | 发布前解决，不阻塞离线 M2 |

---

## 10. 终态判定

```
前端刷新契约        CLOSED
缓存陈旧缺陷        CLOSED（b26091f 已收口 TTL + 失效；本轮收口触发时机与文案）
M1-C                PASS（建议）— 原 FAIL 项与终审 P1 项均已关闭
M1-C.1              COMPLETE
M1-D                建议做，但不自动启动（见下）
M2 / M3             HOLD
v0.6.0 发布         FROZEN（未打 tag、未发布）
```

### M1-D 建议（只建议，不启动）

按负责人拆分，逐项给出本阶段的实测状态：

| 项 | 是否阻塞 M2 | 本轮状态 |
|---|---|---|
| R1 外部写入可见性契约 | **是（已收口，可解除）** | 已完成：机制 + 文案 + 8 条契约测试 |
| R2 CLI JSON spending 不可观测 | 否（建议先做 ADR） | 未动，仍登记 |
| R3 `restore_backup` pre-migration | 否（阻塞发布，非阻塞离线 M2） | 未动，仍登记 |
| R4 页面重新激活刷新 | **曾是（已收口，可解除）** | 已完成：`useRefreshOnActivate` + 双向 RED |

### 需要负责人裁决的事项

1. **证据归档**：`docs/proposals/` 全目录（含 M0 / M0-R / M1-A / M1-B / M1-C / M1-C.1 / 本报告）**至今从未纳入 Git**，真实 GUI 截图仍在临时 fixture 目录（重启系统即失效）。发布前必须建立可追溯证据归档——请裁定归档位置与是否纳入版本库。
2. 本报告是否作为 M1-C 阶段（含 M1-C.1 两轮）的最终验收依据。

---

## 11. 复现命令

```bash
# 隔离环境
FIX=/var/folders/42/0_vh3ptx7735d89p2rz0kj980000gn/T//ah-gui-accept.zF4VAb
pnpm dev &                                   # Vite 1420
AGENTHUB_HOME=$FIX/home AGENTHUB_DATA_DIR=$FIX/home/.agenthub /tmp/ahdesk/agenthub &

# 实验一/四：驻留不自刷新（CLI 写库后不做任何交互，40s 前后截图应完全一致）
AGENTHUB_HOME=$FIX/home ./target/debug/agenthub sub add \
  --stack <ChatGPT-id> --product "F-NoTimer CNY" \
  --billing-type subscription --amount 10 --currency CNY --period monthly

# 实验三/五：焦点刷新（blur 到 Finder 再回焦，应看到新数据）
osascript -e 'tell application "Finder" to activate'; sleep 2
osascript -e 'tell application "System Events" to tell process "agenthub" to set frontmost to true'

# 契约测试
npx vitest run src/__tests__/stack-refresh-contract.spec.ts
```
