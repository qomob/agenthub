# M1 验收证据（Git 内关键子集）

本目录是 M1 阶段验收证据的**第一层**：只保留对裁定有决定性作用的截图与对照文本，随代码长期保留。

- 归档日期：2026-11-12
- 对应阶段：M1-A / M1-B / M1-C / M1-C.1（最终裁定 M1 = PASS · CLOSED）
- 完整证据包（第二层）：`agenthub-m1-acceptance-20261112.tar.zst`，见文末

## 采集环境

| 项 | 值 |
|---|---|
| 应用 | AgentHub 0.5.0 桌面版（Tauri），真实 WebView，非模拟环境 |
| 前端 | `pnpm dev`（Vite 1420），Tauri 经 devUrl 加载 |
| 隔离 home | 临时 fixture 目录，`AGENTHUB_HOME` / `AGENTHUB_DATA_DIR` 均指向该 fixture |
| 数据库 | `$AGENTHUB_HOME/.agenthub/agenthub.db`（lsof 确认进程只打开此库及 -wal/-shm） |
| 真实用户目录 | **全程零触碰**：`~/.agenthub`、`~/.claude` 修改数 0；`~/.codex` 的变动逐项归因于 Codex 自身进程，决定性证据是 fixture 库 `operations` 表 0 行 |
| 采集方式 | `screencapture -x -R<窗口矩形>`；数字判定一律读全分辨率裁剪（×2.72），不读缩略图 |
| 隐私处理 | fixture 内只有合成的测试工具/订阅数据，不含真实凭据或 API Key |

## 文件一览

| 文件 | 证明什么 | 阶段 |
|---|---|---|
| `screenshots/confirm-flow.png` | 确认流：详情页确认后徽标由「未确认」变「已确认」并出现最近确认时间 | M1-C |
| `screenshots/merge-modal.png` | 合并弹窗：目标列表过滤掉自身与已合并项 | M1-C |
| `screenshots/merge-undo-confirm.png` | 撤销合并确认框（写明将恢复哪个条目） | M1-C |
| `screenshots/merge-undone.png` | 撤销后条目恢复独立，**成本总额不变**（278.00 CNY / 330.00 USD）（P0-2） | M1-C |
| `screenshots/delete-blocked.png` | 删除门禁：仍有关联订阅时拒绝，提示「still has linked subscriptions or merge references」 | M1-C |
| `screenshots/spending-overflow.png` | 汇总溢出时显示错误横幅，条目仍全部可见，**不显示 0.00** | M1-C |
| `screenshots/incomplete-cost.png` | 缺金额/币种计为「信息不完整」而非 0；usage 无账单不估算 | M1-C |
| `screenshots/stale-cache-before-redacted.png` | 缓存缺陷现场：CLI 写入后 GUI 仍显示旧条数（**已脱敏，见下**） | M1-C |
| `screenshots/stale-cache-after.png` | 修复后：GUI 内删除订阅立即返回列表即反映新条数（不等 TTL） | M1-C.1 |
| `screenshots/resident-t0.png` | 驻留实验起点（CLI 写库后立即截图） | M1-C.1 Final |
| `screenshots/resident-t40.png` | 驻留实验 40 秒后：**与 t0 逐字节相同** → 页面驻留不自刷新 | M1-C.1 Final |
| `screenshots/focus-refresh.png` | 仅切换窗口焦点（不导航不点击）后条数由 8 变 9、CNY 258.00 变 278.00 | M1-C.1 Final |
| `screenshots/empty-state-copy-zh.png` | 空态文案终版（含「短时间内可能仍显示缓存数据」的 TTL 例外说明） | M1-C.1 Final |
| `cli-vs-db.txt` | 同一 fixture 上 CLI 输出与 SQLite 真值逐项对照 | M1-C |

## 两处必须说明的事实

### 1. `resident-t0.png` 与 `resident-t40.png` 哈希相同

这不是重复文件或误命名，**相同哈希正是该实验的结论**：CLI 写库之后，页面驻留 40 秒、不做任何交互，UI 像素没有任何变化。两张图分别在 T+0 与 T+40 秒真实采集，SHA-256 完全一致即证据本身。

### 2. `stale-cache-before-redacted.png` 是脱敏副本

原始截图右上角混入了本机一个后台应用的系统通知卡片（与 AgentHub 无关的个人环境信息），按「不得把个人环境信息提交到仓库」的要求做了遮盖。

| | 位置 | 说明 |
|---|---|---|
| 原件 | `quarantine/original-stale-cache-before.png` | **不随仓库发布**，仅本地保留；`quarantine/manifest.sha256` 记录其哈希 |
| 脱敏件 | `screenshots/stale-cache-before-redacted.png` | 遮盖通知卡片所在区域，其余像素未改动 |

脱敏件因遮盖，右上角工具栏按钮有一小块被一并覆盖，这是脱敏代价，已在文件名中以 `-redacted` 明示，未冒充原始截图。

该场景**无法重新采集**：它记录的是修复前的错误行为，代码修复后不可复现，因此只能用脱敏方案而非重拍。

## 校验

```bash
cd docs/evidence/m1
shasum -a 256 -c manifest.sha256      # Git 内证据
shasum -a 256 -c quarantine/manifest.sha256   # 本地隔离原件
```

`manifest.sha256` 用于确认 Git 中的证据与采集当时一致，未被裁剪、压缩或替换。

## 采集纪律（本轮踩坑后固化）

macOS 的系统通知会随机落在截图右上角，把与本项目无关的第三方应用暴露进证据。因此：

1. 截图前必须确认 `first process whose frontmost is true` 等于 `agenthub`，否则采到的是别的应用。
2. 截图后必须扫描右上区域是否存在通知图标，命中即弃用重拍。
3. 该检查已固化为脚本（`/tmp/shoot.sh` 同款逻辑），本轮据此重拍了 5 张、弃用 5 张被污染的原件。

## 完整证据包（第二层）

第一层只覆盖决定性截图。完整集合（含全部原始截图与文本对照）打包为：

```
agenthub-m1-acceptance-20261112.tar.zst
```

需要放到**私有、持久化、有版本保护**的存储（对象存储或独立证据仓库），**不放入系统临时目录**（重启即清空）。

> 当前状态：完整证据包**尚未上传**。上传后请在此登记存储位置、归档包 SHA-256、上传日期。在此之前，M1 的完整证据仅存在于采集机临时目录，属已知的可追溯性缺口。

## 与报告的关系

- `docs/proposals/M1_C_ACCEPTANCE_REPORT.md` — M1-C 验收（V1–V4）
- `docs/proposals/M1_C1_CACHE_FIX_REPORT.md` — 缓存失效修复
- `docs/proposals/M1_C1_FINAL_CONTRACT_REPORT.md` — 刷新契约收口（本目录证据的主要来源）

本目录只提供证据，不重复结论；验收判定以三份报告与负责人裁定为准。
