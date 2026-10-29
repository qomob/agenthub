# <img src="assets/logo.svg" alt="AgentHub logo" width="28" align="top"> AgentHub · Capability Control Center

> **当前版本：0.5.0**（2026-10-28，变更明细见 [docs/CHANGELOG.md](docs/CHANGELOG.md)）。License：MIT（见 `LICENSE` 与 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)；源码仓库：https://github.com/qomob/agenthub）。

> **本机 Agent 能力控制中心**：集中登记散落在 Codex、TRAE、ZCode、WorkBuddy、DeepSeek Harness、Qwen Work、Doubao Work 等 Agent 目录里的 **Skills / MCP / 插件清单**，通过适配器事务化分发，并用 **静态审计 + 依赖诊断 + 更新影响 + 人工验证证据 + Agent 版本健康** 回答「装没装、能不能用、安不安全、更新后会不会坏」。
>
> **v3.1 Phase 1 安全切片（2026-10-20 18:45）**：新增 **plan_hash → 一次性 approval token（15 分钟过期）→ apply → CAS → journal → recover** 安全链路、`operation_steps` 持久日志、`adapter_evidence` 证据分级、迁移前自动备份，以及 `operations preview/approve/apply/recover`、`agents inspect` IPC/CLI 与两步审批 UI。测试 **113 Rust + 55 前端**全部通过；真实库副本 v2→v3 迁移 111 个能力完整保留；隔离双宿主分发/回滚/外部修改保护验收通过。真实 Agent 写入仍未授权、未执行。详见 [docs/handoffs/LATEST.md](docs/handoffs/LATEST.md)、[docs/evidence/phase1/](docs/evidence/phase1/)。
>
> 状态快照：**2026-10-20 17:37（Asia/Shanghai）**（当前代码版本 **0.5.0**，见 [docs/CHANGELOG.md](docs/CHANGELOG.md)；下述为 0.4.0 节点的历史验收记录）。**0.4.0 技能库可见性修复已落地**：Skills 页新增「中央库 / 扫描发现」，支持跨 Agent 去重、批量导入中央库与 `skills adopt`；真实机器重新扫描 396 候选 → 跳过只读 273、重复 11、导入 **112** 个（55 MB、失败 0），中央库能力数从 2 → **111**。此前适配器扩展（Qwen Work / Doubao Work，总数 11）与 Validation Platform 仍在。测试：core **82**（44 unit + 38 integration，含 scan_adopt 2）+ CLI **16** = **98 个 Rust 测试**，前端 **8 files / 38 tests**，全部通过。**0.4.0 `.app` 与 `.dmg` 已构建并覆盖安装 `/Applications`**（17:36–17:37）；签名/公证未做（截至 0.5.0 仍未做，分发给他人前需完成）。详见 [docs/VALIDATION.md](docs/VALIDATION.md)、[docs/ACCEPTANCE-REPORT.md](docs/ACCEPTANCE-REPORT.md)、[已知限制](#已知限制)。

AgentHub 是 **本地优先** 的 macOS 桌面应用（Tauri 2 + Vue 3 + Rust + SQLite），无账号、无云端、无遥测。GUI 与 CLI 共用同一个 `agenthub-core` 引擎。

---

## 目录

- [状态标记](#状态标记)
- [真实验收状态](#真实验收状态)
- [核心原则](#核心原则)
- [Capability Validation Platform](#capability-validation-platform)
- [功能状态](#功能状态)
- [环境要求](#环境要求)
- [开发运行](#开发运行)
- [CLI 用法](#cli-用法)
- [数据位置](#数据位置)
- [安全说明](#安全说明)
- [已知限制](#已知限制)
- [排错](#排错)
- [仓库结构](#仓库结构)
- [文档索引](#文档索引)
- [测试与验证](#测试与验证)

---

## 状态标记

- **已验证**：真实 Mac 安装/分发/更新/回滚验收；GUI 隔离 home 运行验收；**真实 Mac validator 验收**；**真实适配器验收**（11 Agent；Qwen Work installed/1.3.1/31 技能，Doubao Work not_installed）；core 82 + CLI 16 = **98 个 Rust 测试**、前端 38 个测试、`pnpm build`、完整 `pnpm tauri build` 全部通过。
- **未验证**：签名/公证（DMG 已产出）、GUI 自动化回归、真实 Keychain 正向写入、真实 MCP 配置写入、并发/中断/磁盘满、非 macOS、部分 adapter 路径（Codex 项目级、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders`）。
- **已知限制**：见下节；headers 明文、secret argv、快照/备份含 token、备份恢复未实现；静态审计是规则引擎不是完备安全证明，人工证据是用户输入；详见 [docs/SECURITY.md](docs/SECURITY.md)、[docs/VALIDATION.md](docs/VALIDATION.md)。

## 真实验收状态

**首版验收场景已在真实 Mac 通过（2026-10-20，用户批准）**：唯一 slug `agenthub-accept-155547` 导入真实 `~/.agenthub`，copy 分发到真实 `~/.codex/skills` 与 `~/.zcode/skills`（新建、无覆盖），扫描 `inSync=true`；导入 v2 后 `skills sync --dry-run` 检测到 `updates=1/plans=1`（仅有更新时**退出码 0**），`skills sync --yes` 更新，`skills rollback last` 精确恢复 v1 字节，`skills uninstall <ids> --yes` 删除部署且中央库保留 2 个版本，backup/doctor 通过；目标目录最终已删除。

**GUI 运行验收**：release `AgentHub.app` 在隔离 home（`/tmp/agenthub-gui/home`）下打开，Dashboard 显示真实数据（Agent 6/9、技能 1、安装 2、MCP 1、插件 1、待更新 1、扫描 7 技能/重复 1/冲突 0/无效 0），onboarding 5 步第 1 步正常，中文界面正常。截图见 [docs/evidence/gui-dashboard.png](docs/evidence/gui-dashboard.png)、[docs/evidence/gui-onboarding.png](docs/evidence/gui-onboarding.png)、[docs/evidence/gui-health.png](docs/evidence/gui-health.png)。

**Adapter 真实验收（2026-10-20 16:44，用户批准）**：`agents list` 显示 11 个适配器；`qwen-work` = installed（`~/.qwenworkcn/skills`），`doubao-work` = not_installed；`skills scan` 真实发现 **396** 个技能（invalid 5、conflicts 7），其中 **Qwen Work 31 个**（create-skill 1.2.1、dingtalk-*、pptx 2.3.3、xlsx 1.0.2、pdf、media-*、qwenwork-guidance 等）；`health --agents --record` 新增 Qwen Work **1.3.1**。证据 `/tmp/agenthub-adapters-real/`。

**Validator 真实验收（2026-10-20 16:19，用户批准）**：真实 `~/.agenthub` 自动从 v1 迁移到 **DB v2**；`health` total=2/not_deployed=2；`health --agents --record` 检测到 ChatGPT 26.1013.40934、Codex 26.623.70822、DSH 0.2.1-rc.2、TRAE CN 3.4.4、WorkBuddy 5.8.3、ZCode 3.16.2；`skills audit memory-maker` = warn/medium、8 findings；`skills deps memory-maker` = healthy（node 24.19.0、MCP unknown）；`skills impact memory-maker` = medium/skill_md_changed/requiresRevalidation=true；`skills validate memory-maker --agent codex --status passed` 写入成功。证据 `/tmp/agenthub-validator-real/`，方法论见 [docs/VALIDATION.md](docs/VALIDATION.md)。

**打开已安装的 AgentHub**

```bash
open /Applications/AgentHub.app
```

用隔离 home 启动（不读写真实 Agent 目录）：

```bash
AGENTHUB_HOME=/tmp/agenthub-gui/home \
  /Applications/AgentHub.app/Contents/MacOS/agenthub
```

> 构建：完整 `pnpm tauri build` 已于 2026-10-20 17:36–17:37 成功产出 **0.4.0** `target/release/bundle/macos/AgentHub.app`（7.52 MiB）与 `target/release/bundle/dmg/AgentHub_0.4.0_aarch64.dmg`（3.97 MiB）；`/Applications/AgentHub.app` 已覆盖安装 0.4.0。首次 DMG 尝试因沙箱 hdiutil 权限失败，随后在用户授权的完整权限下成功；签名/公证尚未配置。完整报告：[docs/ACCEPTANCE-REPORT.md](docs/ACCEPTANCE-REPORT.md)。

## 核心原则

| # | 原则 | 含义 |
|---|---|---|
| 1 | **Local First** | 数据只在本机：SQLite + 本地文件 + macOS Keychain；无账号/云同步/遥测 |
| 2 | **Single Source of Truth** | 库内一份不可变版本，Agent 目录是部署副本；「装没装」查 `installations` 表 |
| 3 | **Adapter First** | 每个 Agent 的路径/格式/能力差异集中在 `AgentAdapter`，执行器不写特例 |
| 4 | **Non-destructive** | 不覆盖非 AgentHub 拥有的目标；每次写入前快照；冲突需显式 force |
| 5 | **Capability-based** | 适配器显式声明 symlink/插件/传输能力；未验证项标 `Unverified` / `VerificationRequired` |
| 6 | **Transactional** | 计划 → 预检 → 快照 → 写入 → 校验；失败逆序补偿；支持操作级回滚 |
| 7 | **No Fake Integration** | 没有真实验证的能力不展示、不写入；ChatGPT 只读、DSH MCP 不声明 |
| 8 | **Verified, not assumed** | 安装 ≠ 可用 ≠ 已验证；只有审计/环境/人工证据支持的当前版本才显示 `verified` |

完整定义见 [docs/PRD.md](docs/PRD.md)。

---

## 支持的 Agent（11 个内置适配器）

| key | 名称 | 本机状态 | Skill / MCP / Plugin | 说明 |
|---|---|---|---|---|
| `codex` | Codex | installed | skills + MCP TOML + plugin 只读 | 项目级根未声明 |
| `chatgpt` | ChatGPT | installed | 不支持本地 skill/MCP | 只读检测 |
| `trae` | TRAE (International) | installed | 技能根 + 项目 MCP | 全局 MCP 未建模 |
| `trae-cn` | TRAE CN | installed | 技能根 + 全局/项目 MCP + 插件发现 | 热加载 VR |
| `zcode` | ZCode | installed | 技能根 + MCP `mcp.servers` | 远程传输 VR |
| `workbuddy` | WorkBuddy | installed | 技能根 + MCP + 插件只读 | `staticHeaders` 未建模 |
| `dsh` | DeepSeek Harness | installed | 技能根；MCP 不写 | symlink Supported |
| `claude-code` | Claude Code | verification_required | 技能/MCP 只读+安装未验证 | 无版本来源 |
| `cursor` | Cursor | not_installed | 技能/MCP 预留 | 无本机证据 |
| `qwen-work` | **Qwen Work (千问工作)** | **installed（VERIFIED）** | `~/.qwenworkcn/skills`（真实 31 技能）+ `~/.qwenworkcn/mcp.json`（stdio/HTTP/streamable-http）+ 插件只读 | 版本 1.3.1；symlink/热加载/MCP 生效方式 VR |
| `doubao-work` | **Doubao Work (豆包工作)** | **not_installed** | 无全局根；workspace `.user_skills`/`.doubao/skills`；MCP/插件 manual-only | 本机仅豆包浏览器；存在时 VerificationRequired；需注册 custom root 才可分发 |

> 详细路径/证据见 [docs/ADAPTER-SPEC.md](docs/ADAPTER-SPEC.md) 与 [docs/AGENT-COMPATIBILITY.md](docs/AGENT-COMPATIBILITY.md)。

## 界面截图

| 仪表盘 | 隐私与文件访问（Onboarding 第 1 步） |
|---|---|
| ![仪表盘：Agent 登记数、技能、安装数、MCP、最近一次扫描与最近操作](docs/evidence/gui-dashboard-app-only.png) | ![Onboarding：本地优先数据边界说明与只读扫描确认](docs/evidence/gui-onboarding-app-only.png) |

| 健康验证 |
|---|
| ![健康验证：已验证需要审计与验证证据，仅安装成功不会自动变成已验证](docs/evidence/gui-health-app-only.png) |

## 技能库：扫描发现 → 批量入库（0.4.0）

此前 Skills 页只显示「已导入中央库」的能力，本机各 Agent 目录里已有的技能看不到。0.4.0 将扫描发现做成一等公民：

1. **扫描**：`skills scan` / 扫描按钮读取所有 Agent root，写入 `scan_results`，并在每条发现上记录 `rootSource`（adapter/project/custom/builtin/marketplace/workspace/connector/shared）。
2. **发现 Tab**：Skills 页「中央库 (N) / 扫描发现 (M)」，深链 `/skills?tab=discovered`；按 `(slug, contentHash)` 跨 Agent 分组去重，显示 已在库/未入库、只读来源、涉及 Agent、路径、体积；支持搜索/状态筛选（未入库/已在库/只读）、每页 100 行加载更多。
3. **选择与确认**：选择与分页独立；确认对话框显示数量/体积/目标 Agent/将导入路径，明确「只复制到中央库，不改 Agent 目录」。
4. **批量导入**：Tauri `skills_import_scan` / CLI `skills adopt`；默认跳过只读来源（builtin/marketplace/workspace/connector），按 `(slug, contentHash)` 去重、跳过已在库；失败逐条返回；`--dry-run` 可预览。
5. **入口**：Dashboard 在存在未入库非只读技能时显示真实入口；Agents 页有「查看扫描发现」；Onboarding 扫描步骤提示可导入数量。
6. **真实结果**：用户机器重新扫描 **396** 候选 → 跳过只读 **273** / 重复 **11** → 导入 **112**（约 **55 MB**，失败 0）→ 中央库能力数 **2 → 111**（112 次导入按版本共存形成 111 个 capability）。证据 `/tmp/agenthub-adopt-real/`。

## 功能状态

状态：✅ 已实现（代码可证）/ 🟡 部分或未验证 / ⛔ 未实现 / ⚠️ 已知缺陷。

### Phase 1 — 本机能力管理（✅ 已验收）

| 能力 | 状态 | 说明 |
|---|---|---|
| 11 个 Agent 检测与 roots 盘点（含 Qwen Work / Doubao Work） | ✅ | `crates/agenthub-core/src/adapters/builtin.rs`；Qwen 本机 VERIFIED，Doubao 本机 NOT_INSTALLED/VR |
| 扫描发现 → 批量入库中央库（0.4.0） | ✅ | Skills「扫描发现」Tab + `skills adopt`；跨 Agent 按 `(slug, contentHash)` 去重、默认跳过只读来源；真实 396→112（111 capability）/55MB |
| 只读扫描（全局/项目/自定义） | ✅ | 同内容跨 Agent 计 duplicate，同 slug 不同内容计 conflict |
| 导入：folder / zip / SKILL.md / Git / Agent 目录 | ✅ | zip 有路径穿越/symlink/大小/条目数防护 |
| 中央库版本化（内容寻址） | ✅ | `library/skills/<slug>/<hash12>/` |
| 事务分发（copy / symlink） | ✅ | 预检 + 快照 + 校验 + 失败补偿 |
| 更新检测 / 漂移 / 缺失检测 | ✅ | Dashboard 与 `check_changes` |
| 操作级回滚 | ✅ | 每个操作最多一次；拒绝覆盖外部修改 |
| CLI | ✅ | 计划门禁、`--json`、退出码 0/1/2 |
| 测试 | ✅ | core 82、CLI 16（Rust 合计 98）、前端 38 通过（2026-10-20 17:32 实测） |
| GUI | ✅ | 11 条导航路由 + 12 视图 + 20 组件；`pnpm build` 通过；隔离 home 人工运行验收 |
| 真实 Mac 验收 | ✅ | 导入/分发/扫描/更新/回滚/卸载/备份/doctor；见 ACCEPTANCE-REPORT |

### Phase 2 — MCP（✅ 核心完成；headers 待加固）

| 能力 | 状态 | 说明 |
|---|---|---|
| MCP 定义 CRUD（stdio/http/sse/streamable_http） | ✅ | 名称唯一；command/url 互斥校验 |
| 从 Agent 配置只读发现 | ✅ | `json:mcpServers`、`json:mcp.servers`、`toml:mcp_servers` |
| Keychain 存 env 值 | ✅ | service `com.agenthub.mcp`；缺值 → blocked |
| 多格式写回 + 保真 | ✅ | TOML 保留注释/其他表；JSON 保留未知键（格式会重排） |
| 分发 / 移除 / 回滚 | ✅ | 正常路径可用；条目级 preflight，无需 `--force`；MCP 回滚由 harness 验证 |
| 计划不含 env 明文 | ✅ | plan payload 剥离 env，执行时从 Keychain 重建 |
| headers 凭据化 | ⛔ | 仍明文入库/入配置（UI 有警示） |
| DSH Cordis YAML / TRAE 全局 / ZCode 原生项目配置 | ⛔ | 未声明 codec |

### Phase 3 — 工作流与可视化（✅ 人工验收）

| 能力 | 状态 | 说明 |
|---|---|---|
| 能力矩阵（skill × agent） | ✅ | 页面与 `matrixCellState` 已实现 |
| Profiles（4 内置 + 自定义 + 应用） | ✅ | 应用复用安装计划 |
| 插件只读清单 | ✅ | Codex/TRAE CN/WorkBuddy；不安装/不启停 |
| 活动与诊断（4 Tab） | ✅ | 操作/快照/备份/诊断与变更 |
| Onboarding 5 步 | ✅ | 隐私 → 库 → Agent → 扫描 → 完成 |
| 启动自动扫描 | 🟡 | UI 会按设置调用 scan；核心 `open()` 本身不扫描 |
| 结构化日志 | ✅ | `logs/operations.log` JSONL（import/start/finish/failed） |
| 备份恢复 | ⛔ | 备份只能创建/列出；无恢复路径 |
| GUI 运行验收 | ✅ | Dashboard/健康摘要/onboarding/中文界面/活动历史；截图见 docs/evidence |

### Phase 4 — Capability Validation Platform（✅ 本轮落地，持续演进）

| 能力 | 状态 | 说明 |
|---|---|---|
| Capability Validator（静态审计） | ✅ | `audit.rs`：确定性规则、finding/severity/category/path/suggestion；不执行脚本、不联网、无 AI 评分 |
| 依赖与环境诊断 | ✅ | `deps.rs`：shebang runtime 真实探测（2s 超时）、requirements/pyproject/package.json 解析；MCP/凭据 unknown |
| 更新影响分析 | ✅ | `impact.rs`：版本 diff（500 条上限+真实总数）、classes、风险、`requiresRevalidation`；写入 `update_impacts` 并进入分发计划 warnings |
| 人工验证证据 | ✅ | `validation_runs`：agent/case/status/notes/evidence；明确「封闭 Agent 无法自动 E2E，这是人工证据不是自动通过」 |
| 健康状态机 | ✅ | NotDeployed / Failed / NeedsAttention / Verified / Unverified；安装≠验证；安全优先；只有当前版本全部安装 Agent manual passed 才 Verified |
| Agent Health Monitor | ✅ | `agent_health.rs`：`.app` CFBundleShortVersionString（plutil→XML→mdls）+ CLI `--version`；首次 baseline，变化提示重扫/重验；同秒 rowid 决胜 |
| DB v2 | ✅ | 新增 capability_audits / dependency_reports / update_impacts / validation_runs / agent_versions；v1 自动升级、幂等 |
| CLI / UI | ✅ | `health`、`skills audit|deps|impact|validate`；`/health` 页、SkillDetail 四区块、Agents 版本健康、Dashboard/Skills 健康列、分发预览影响摘要 |

**推荐工作流**：导入 → **静态审计** → **依赖/环境检查** → 分发（预览含更新影响）→ **人工/环境验证并记录证据** → 更新（查看影响、重新验证）→ 回滚。安装/同步仍复用原有事务引擎，验证平台不重复实现。

> 详细页面规格见 [docs/UI-SPEC.md](docs/UI-SPEC.md)，验证方法论见 [docs/VALIDATION.md](docs/VALIDATION.md)，阶段路线见 [docs/ROADMAP.md](docs/ROADMAP.md)。

---

## 环境要求

| 项 | 要求 | 说明 |
|---|---|---|
| 系统 | macOS 15.x，**Apple Silicon（arm64）** | MVP 只承诺该组合；未在 Intel/Windows/Linux 验证 |
| Node.js | ≥ 20（本机 v24.19.0） | 前端构建 |
| pnpm | 10.x（本机 10.28.2） | 仓库使用 `pnpm-lock.yaml`；`.npmrc` 指定本地 store |
| Rust | **rustup stable（本机 cargo 1.99.1）** | ⚠️ 系统 `rustc` 是 Homebrew **1.87**，对 Tauri 2.12 过旧，**不能直接用 `cargo`** |
| Cargo home | `CARGO_HOME=/tmp/agenthub-cargo` | 由 `scripts/cargo-env.sh` 固定，避免写入只读/权限受限的目录 |
| Xcode CLT | 需要（链接器/框架） | `xcode-select --install` |
| git | Git 导入需要 | 无 git 时其余功能可用，`doctor` 会标 warning |
| Keychain | macOS 自带 `/usr/bin/security` | 仅 MCP env 值使用 |

工具链版本可用以下命令确认：

```bash
./scripts/cargo.sh --version   # cargo 1.99.1（rustup stable）
rustc --version                # 系统 PATH 上可能是 1.87（已被 wrapper 绕过）
node --version && pnpm --version
```

`scripts/cargo-env.sh` 做了三件事：

```sh
export PATH="$HOME/.cargo/bin:$PATH"          # 用 rustup 的 stable，而不是 /usr/local/bin 的 1.87
export RUSTUP_TOOLCHAIN=stable
export CARGO_HOME=/tmp/agenthub-cargo         # 可写、隔离的 cargo home
export CARGO_TARGET_DIR=$PWD/target
```

---

## 开发运行

### 1. Rust 构建与测试（必须用 wrapper）

```bash
# 在仓库根目录
./scripts/cargo.sh build -p agenthub-core
./scripts/cargo.sh build -p agenthub-cli          # 产物：target/debug/agenthub
./scripts/cargo.sh test -p agenthub-core
./scripts/cargo.sh test -p agenthub-cli
./scripts/cargo.sh check --workspace
```

> 直接运行 `cargo` 会使用系统 rustc 1.87，Tauri 2.12 会报工具链过旧或 edition 相关错误。`./scripts/cargo.sh <args>` 等价于「先 source `scripts/cargo-env.sh` 再执行 cargo」。

### 2. 安装前端依赖

```bash
pnpm install
```

### 3. 桌面开发（Tauri dev）

```bash
source scripts/cargo-env.sh   # 让 Tauri CLI 调用到 rustup stable + /tmp cargo home
pnpm tauri dev
```

- 前端 dev server 端口固定 **1420**（`vite.config.ts` 与 `src-tauri/tauri.conf.json` 的 `devUrl` 一致）。
- `pnpm tauri dev` 会执行 `pnpm dev`（Vite）并启动 Tauri 窗口。

### 4. 生产构建

```bash
source scripts/cargo-env.sh
pnpm tauri build
```

> **当前快照的构建状态**：`pnpm build` 已通过（`vue-tsc --noEmit` + Vite，dist 生成）；完整 `pnpm tauri build` 已通过并产出 0.4.0 `AgentHub.app`（7.52 MiB）与 `AgentHub_0.4.0_aarch64.dmg`（3.97 MiB，2026-10-20 17:36–17:37），`/Applications/AgentHub.app` 已覆盖安装 0.4.0；首次 DMG 尝试因沙箱 hdiutil 权限失败，完整权限重跑成功。签名/公证未做。

### 5. 只构建 CLI

```bash
./scripts/cargo.sh build -p agenthub-cli
./target/debug/agenthub --help
```

---

## CLI 用法

CLI 是同一引擎的薄前端，**默认只打印计划，不会写 Agent 目录**。全局参数：`--home <DIR>`（等价 `AGENTHUB_HOME`，用于隔离测试）、`--json`。

退出码：`0` 成功；`1` 失败；`2` 部分成功 / 计划包含 blocked/conflict / 缺少 `--yes`。

```bash
BIN=./target/debug/agenthub

# 诊断与 Agent 盘点
$BIN doctor
$BIN agents list

# 扫描（只读）
$BIN skills scan

# 导入（文件夹 / .zip / SKILL.md / Git）
$BIN skills import --path /path/to/my-skill
$BIN skills import --path /path/to/skill.zip
$BIN skills import --path /path/to/SKILL.md
$BIN skills import --git https://github.com/org/repo --ref main --skill-path skills/foo

# 列出库内技能
$BIN skills list

# 分发：先预览（退出码 2），确认后加 --yes
$BIN skills install <skill-id-or-slug> --agents codex,workbuddy --dry-run
$BIN skills install <skill-id-or-slug> --agents codex,workbuddy --yes

# 扫描发现 → 批量导入中央库（0.4.0）
# 先用 `skills scan` 生成最近一次扫描结果
$BIN skills adopt --dry-run                 # 预览将导入多少条（默认跳过只读来源）
$BIN skills adopt --agents codex,workbuddy --max 100 --yes
$BIN skills adopt --include-read-only --dry-run  # 显式包含 builtin/marketplace/workspace/connector

# 更新/漂移检查：生成计划（不写）
# 注意：仅有更新时 --dry-run 退出码 0；drift/missing/blocked/conflict 才 2
$BIN skills sync --dry-run
$BIN skills sync --yes

# 卸载：按 installation id（来自 skills list --json 的 installations[].id）
$BIN skills uninstall <installation-id> [<installation-id> ...] --dry-run
$BIN skills uninstall <installation-id> [<installation-id> ...] --yes

# 回滚最近一次可回滚操作（或指定 operation id）
$BIN skills rollback last
$BIN skills rollback <operation-id>

# 操作历史 / Profiles / 备份
$BIN operations list --limit 20
$BIN profiles list
$BIN backup create
$BIN backups list

# 健康与验证（第二轮新增）
$BIN health                         # 能力健康概览（verified/unverified/needs_attention/failed/not_deployed）
$BIN health --agents                # Agent 版本健康（只读）
$BIN health --agents --record       # 记录一次版本观测（首次为 baseline）
$BIN skills audit <skill>           # 静态安全审计；status=fail → 退出码 2
$BIN skills deps <skill>            # 依赖/环境诊断；needs_attention → 退出码 2
$BIN skills impact <skill> --to <version-id>   # 更新影响（默认 to=当前版本）
$BIN skills validate <skill> --agent codex --case "generate invoice PDF" --status passed \
  --notes "Codex 26.x" --evidence "log excerpt or link"

# MCP：列出、发现、分发（CLI 没有「创建 MCP 定义」命令；env 值不会进入 plan）
$BIN mcp list
$BIN mcp discover --agent codex
$BIN mcp distribute <server-id-or-name> --agents codex --dry-run
$BIN mcp distribute <server-id-or-name> --agents codex --yes
```

隔离测试（推荐先这样跑）：

```bash
export AGENTHUB_HOME=/tmp/agenthub-demo/home
mkdir -p "$AGENTHUB_HOME/.codex"
$BIN --home "$AGENTHUB_HOME" doctor
```

> 注意：`--home` 只隔离 Agent 目录与数据目录，**不会隐藏 `/Applications` 下真实安装的 app**；扫描可能列出本机真实 Agent。执行分发时显式指定 agent key，避免误写。

---

## 数据位置

默认数据目录 `~/.agenthub`（可用 `AGENTHUB_DATA_DIR` 覆盖）；Agent 检测基准 home 可用 `AGENTHUB_HOME` 覆盖。

```text
~/.agenthub/
├── agenthub.db                  # SQLite（user_version=2）：v1 全部表 +
│                                #   capability_audits/dependency_reports/update_impacts/
│                                #   validation_runs/agent_versions
├── library/
│   └── skills/<slug>/<hash12>/  # 不可变技能版本（SKILL.md + 资源）
├── snapshots/<operation-id>/    # 每次写入前的目标备份（目录/文件）
├── backups/backup-YYYYMMDD-HHMMSS/
│   ├── agenthub.db              # VACUUM INTO 的一致性数据库副本
│   └── manifest.json
├── staging/                     # 导入/解压临时目录（自动清理）
└── logs/
    └── operations.log           # JSONL 审计：import / operation.start/finish/failed
```

| 设置/变量 | 作用 |
|---|---|
| Settings → 中央库 | 可把 `library_root` 改到其他空目录；**数据库仍在 `~/.agenthub`**，旧库不会自动删除 |
| `AGENTHUB_HOME` | 覆盖「用户 home」，所有 `~/.<agent>` 路径随之改变（测试隔离） |
| `AGENTHUB_DATA_DIR` | 覆盖 `~/.agenthub` |
| `AGENTHUB_EXTRA_APP_DIRS` | 追加 `.app` 探测目录（冒号分隔） |
| `AGENTHUB_CARGO_HOME` | 构建时覆盖 `/tmp/agenthub-cargo` |
| Keychain service | `com.agenthub.mcp`，account 为 `<server_id>/<envKey>` |

数据库表结构、关系与迁移策略见 [docs/DATA-MODEL.md](docs/DATA-MODEL.md)。

---

## 安全说明

### 数据边界（收集 / 不收集清单）

- 📊 **本机读取**：Skills / MCP / 插件清单、各 Agent 配置目录、Agent 版本信息（仅本地处理）
- 🚫 **绝不读取或收集**：对话内容、Prompt 文本、API Key / token 原值；无遥测上报
- 🔐 **凭据**：MCP env 值只存 macOS Keychain；日志与分发计划只记 key 名 / 路径 / hash
- 🎛️ **您的控制**：删除 `~/.agenthub/` 即清除全部元数据；各 Agent 目录内容仅经显式批准的事务修改

### 机制说明

- **纯本地**：无账号、无遥测、无云同步；唯一出站是用户显式触发的 `git clone`。
- **跨进程互斥**：GUI 与 CLI 共用一套数据，修改类操作（apply / rollback / backup / restore）全程持有 `~/.agenthub/runtime.lock`（flock，进程退出自动释放）；另一进程同时发起修改会得到明确的「runtime busy + 持有者」错误，绝不交错写入。
- **写入前预览**：GUI 弹计划、CLI 默认只打印计划；目标、动作、哈希、警告、重启提示全部可见。
- **快照与回滚**：每次写入前快照；失败自动逆序恢复；操作级回滚需显式确认。
- **不可信输入**：zip 拒绝路径穿越/绝对路径/symlink 条目/超限；Git 只浅克隆、不执行仓库脚本、删除 `.git`；Markdown 渲染不使用 `v-html`。
- **凭据**：MCP env 值存 Keychain，数据库只存 key 名；**分发计划 payload 已剥离 env 值**，执行时才从 Keychain 重建；`logs/operations.log` 只记 id/路径/hash，不记 secret。
- **静态审计**：只读、不执行脚本、不联网、无 AI 评分；硬编码凭据只输出类型/行号，**不输出原值**；规则是启发式，不等同完备安全证明。
- **人工验证证据**：`validation_runs` 的 notes/evidence 是用户输入，AgentHub 不做真实性证明；UI 明确提示「这是人工证据，不是自动通过」。
- **已知风险（必须在 UI/使用中留意）**：
  1. MCP headers 当前**明文**入库并写入 Agent 配置；
  2. `/usr/bin/security ... -w <value>` 会让 secret 短暂出现在进程参数；
  3. 快照/备份是含配置文件的普通副本，可能包含 token，且未加密；
  4. 应用**不是 App Sandbox**、未签名/未公证，以当前用户权限运行；
  5. Claude Code / Cursor 标为 `VerificationRequired` 但仍允许写入，使用前请自行确认；
  6. 备份恢复未实现。

完整威胁模型与控制见 [docs/SECURITY.md](docs/SECURITY.md)。

---

## 已知限制

| 类别 | 限制 |
|---|---|
| GUI | 已人工运行验收；无 Tauri 自动化回归；浏览器环境只能显示「请运行 pnpm tauri dev」 |
| 打包/签名 | 0.4.0 DMG 已产出（`AgentHub_0.4.0_aarch64.dmg`）；未签名/未公证 |
| 测试 | 无 `src-tauri` 自动化测试；核心缺纯函数单元测试 |
| MCP | DSH Cordis YAML、TRAE 全局、ZCode 原生项目配置未实现；删除后留空表/空对象；headers 明文 |
| 能力范围 | 插件只读；不执行 skill 脚本；不试连 MCP；不提供市场/云同步/调用统计 |
| 验证平台 | 静态规则非完备安全证明（可能漏报/误报）；MCP/凭据静态 unknown；封闭 Agent 只能人工证据；版本探测依赖 app bundle/CLI 输出；证据为用户输入 |
| 记录 | 扫描不写 `operations`（导入已写）；无通用 tracing 日志；备份无恢复路径 |
| Scope | `installations.scope` 记录 global/custom；项目级安装仍按 custom root 处理 |
| Adapter | Codex 项目级根、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders` 等仍未验证；ChatGPT 只读；Doubao MCP/插件 manual-only；Doubao 本机未安装；Qwen symlink/MCP 生效方式 VR；同名 app 未按 `CFBundleName` 区分 |
| 扫描采纳 | 默认不导入只读来源（builtin/marketplace/workspace/connector）；选中只读来源需显式勾选；导入只复制到中央库、不改 Agent 目录；只读来源默认 273 条不导入 |
| 平台 | 仅 macOS Apple Silicon；Windows/Linux 未验证 |
| 回滚 | 若目标在操作后被外部修改，回滚会拒绝覆盖该项并返回 `partial` |

---

## 排错

### `rustc 1.87` / Tauri 工具链过旧

- 现象：`pnpm tauri dev` 或直接 `cargo build` 报工具链/edition 错误。
- 处理：使用 `source scripts/cargo-env.sh` 后再跑 Tauri；Rust 命令统一用 `./scripts/cargo.sh`。
- 若 `~/.cargo/bin` 不存在，先安装 rustup stable：`curl https://sh.rustup.rs -sSf | sh`，再 `rustup toolchain install stable`。

### Cargo 写入 `~/.cargo` 或 `/tmp` 被拒

- 现象：`Operation not permitted`、`failed to create temp file`。
- 处理：确认 `CARGO_HOME=/tmp/agenthub-cargo` 可写；`./scripts/cargo.sh` 已设置。沙箱/只读环境下先 `mkdir -p /tmp/agenthub-cargo` 并检查权限。

### 目标目录不可写 / 权限不足

- `doctor` 会检查 library 与 data_dir 是否可写；Agents 页每个 root 有 `writable` 标记。
- macOS TCC：首次访问「桌面/文稿/下载」等目录可能弹授权；拒绝后相关路径会显示不可写。
- 不要用 `sudo` 运行 AgentHub；它应以当前用户身份运行。

### Keychain 相关

- `doctor`/Settings 显示 `keychainAvailable`（检测 `/usr/bin/security`）。
- 首次写入 secret 可能弹 Keychain 授权；拒绝会导致 `mcp_set_secret` 失败。
- 若提示「Environment values are not stored in Keychain yet」，先在 MCP 详情里设置对应 envKey。
- 只想验证流程时，用不声明 env 的 server。

### 扫描到意料之外的 Agent

- `find_app` 固定先查 `/Applications`，即使使用 `--home` 也会检测到本机真实 app。
- 这是预期行为；测试/演示时用 `--agents` 显式指定目标，或先只跑 `--dry-run`。

### DMG 打包（已成功；首次沙箱尝试失败）

- 本机完整构建已产出 **0.4.0** `target/release/bundle/dmg/AgentHub_0.4.0_aarch64.dmg`（3.97 MiB，2026-10-20 17:37）与 `target/release/bundle/macos/AgentHub.app`（7.52 MiB，17:36），并已覆盖安装 `/Applications/AgentHub.app`（0.4.0）。
- 首次 DMG 尝试在受限沙箱内因 hdiutil 权限失败（当时目录中只有 `bundle_dmg.sh`）；随后在用户授权的完整权限下重跑成功。若在受限环境复现失败，请在普通桌面会话/有权限终端重跑 `source scripts/cargo-env.sh && pnpm tauri build`，这不是应用代码错误。
- 尚未做 Developer ID 签名与 notarization。

### 打开已安装的 AgentHub.app

```bash
open /Applications/AgentHub.app
```

- 若 Gatekeeper 提示未验证开发者：当前未签名/未公证，需在「系统设置 → 隐私与安全性」中允许，或自行签名后再分发。
- 隔离测试：`AGENTHUB_HOME=/tmp/agenthub-gui/home /Applications/AgentHub.app/Contents/MacOS/agenthub`。

### 数据库被锁 / 两个实例

- apply/rollback 使用 `<library_root>/.agenthub.lock` 跨进程互斥；SQLite 使用 WAL。
- 但 import/设置/MCP 定义写入没有跨进程锁；不要同时用 GUI 与 CLI 做写操作。

### `pnpm test` 的 jsdom 警告

- 输出中的 `Not implemented: Window's scrollTo()` 来自 jsdom 环境，不影响测试结果（8 个文件 / 38 个测试通过）。

### 空白窗口 / 找不到 Tauri 后端

- 浏览器直接打开 Vite 页面会显示「桌面后端不可用：请运行 pnpm tauri dev」；这是预期提示，不是 mock。
- 确认 Tauri 窗口打开且 dev server 端口 1420 未被占用。

---

## 仓库结构

```text
crates/agenthub-core/          # 核心引擎（无 Tauri 依赖）
  src/
    adapters/{mod,builtin}.rs  # AgentAdapter + 11 个适配器（含 Qwen Work / Doubao Work）
    db.rs                      # SCHEMA_V1、迁移、SQL
    library.rs                 # 中央库布局/备份
    mcp.rs                     # MCP codec/Keychain/写入
    model.rs                   # 公共模型（camelCase JSON）
    ops.rs                     # 事务执行/快照/回滚
    scan.rs                    # 只读扫描
    service.rs                 # AgentHub 门面（GUI/CLI 共用）
    skill.rs                   # SKILL.md 解析/导入安全
    util.rs                    # hash/原子写/路径安全
  tests/                       # 38 个集成测试（含 validator 11、scan_adopt 2、adapters_extended 4）
crates/agenthub-cli/           # CLI（1324 行薄前端）
  tests/cli.rs                 # 9 个进程级测试
src-tauri/                     # Tauri 2 命令层（47 个命令）
src/                           # Vue 3 UI（App/router/12 views/21 components）
docs/                          # PRD/架构/数据/适配器/安全/UI/测试/路线图/变更/验收报告/验证方法论
docs/evidence/                 # GUI 验收截图（Dashboard/Onboarding/Health）
scripts/cargo.sh|cargo-env.sh  # 固定 rustup stable + /tmp CARGO_HOME
scripts/real-acceptance.sh     # 真实 Mac 验收脚本（需授权）
```

---

## 文档索引

| 文档 | 内容 | 状态 |
|---|---|---|
| [docs/PRD.md](docs/PRD.md) | 定位、用户、7 条原则、功能 A–G、MVP 边界、验收场景 | ✅ 本次 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 分层、时序、事务/快照、锁、扩展点、已知缺陷 | ✅ 本次 |
| [docs/DATA-MODEL.md](docs/DATA-MODEL.md) | 13 张表、字段/约束/索引、ER、迁移、敏感数据 | ✅ 本次 |
| [docs/ADAPTER-SPEC.md](docs/ADAPTER-SPEC.md) | Trait 语义、11 个适配器矩阵、新增步骤、codec 保真 | ✅ 本次 |
| [docs/SECURITY.md](docs/SECURITY.md) | 威胁模型、已实现控制、剩余风险、确认点 | ✅ 本次 |
| [docs/UI-SPEC.md](docs/UI-SPEC.md) | 9+1 页面、任务流、状态/token、预览确认、实现状态 | ✅ 本次 |
| [docs/TEST-PLAN.md](docs/TEST-PLAN.md) | 测试分层、隔离要求、场景矩阵、真实结果、人工验收 | ✅ 本次 |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phase 0–5、Validation Platform、未来候选、风险登记 | ✅ 本次 |
| [docs/CHANGELOG.md](docs/CHANGELOG.md) | 0.1.0 / 0.2.0 / 0.3.0 / 0.4.0 Added/Changed/Fixed/Known issues | ✅ 本次 |
| [docs/VALIDATION.md](docs/VALIDATION.md) | 验证方法论、规则表、证据语义、健康状态机、限制与路线 | ✅ 新增 |
| [docs/ACCEPTANCE-REPORT.md](docs/ACCEPTANCE-REPORT.md) | 真实 Mac 验收 + GUI 运行 + 构建/测试证据 + 未验证项 | ✅ 新增 |
| [docs/evidence/](docs/evidence/) | GUI 验收截图（Dashboard / Onboarding / Health） | ✅ 新增 |
| [docs/AGENT-COMPATIBILITY.md](docs/AGENT-COMPATIBILITY.md) | 本机 11 个 Agent 路径/格式/证据矩阵（新增 Qwen Work / Doubao Work） | ✅ 本次更新 |
| [docs/REFERENCE-STUDY.md](docs/REFERENCE-STUDY.md) | Agent Skills / MCP 规范与各 Agent 官方文档研究 | 📚 调研同事维护 |
| [docs/compat-evidence/](docs/compat-evidence/README.md) | 脱敏证据、symlink 验证方案、交叉核对 | 📚 调研同事维护 |

---

## 测试与验证

```bash
# Rust（必须用 wrapper）：core 82 + CLI 16 = 98
./scripts/cargo.sh test -p agenthub-core -p agenthub-cli

# 前端：8 文件 / 38 测试
pnpm test

# 构建
pnpm build
pnpm tauri build
```

**当前真实结果（2026-10-20 16:24）**：

- `./scripts/cargo.sh test -p agenthub-core`：**82 passed / 0 failed**（44 unit + 38 integration；新增 scan_adopt 2）。
- `./scripts/cargo.sh test -p agenthub-cli`：**16 passed / 0 failed**（9 旧 + 7 validator CLI）。
- Rust 合计 **98 passed / 0 failed**；`./scripts/cargo.sh check -p agenthub` exit 0。
- `pnpm test`（vitest）：**8 文件 / 38 测试全部通过**。
- `pnpm build`：**通过**（`vue-tsc --noEmit` + Vite build）。
- `pnpm tauri build`（完整构建）：**通过**；0.4.0 产物 `AgentHub.app`（7.52 MiB）与 `AgentHub_0.4.0_aarch64.dmg`（3.97 MiB，17:36–17:37）；`/Applications/AgentHub.app` 已覆盖安装 0.4.0。首次 DMG 尝试因沙箱 hdiutil 权限失败，完整权限重跑成功。
- **真实 Mac 验收**：通过；`skills sync --dry-run` 仅有更新时退出码 0（drift/missing/blocked/conflict 才 2）；回滚精确恢复 v1；卸载后库保留 2 版本；doctor ok。
- **GUI 运行验收**：隔离 home 通过（含 Dashboard 健康摘要/健康页）；截图 `docs/evidence/gui-health.png`。
- **真实 Mac validator 验收**：DB v1→v2、Agent 版本探测、audit/deps/impact/validate 全通过；证据 `/tmp/agenthub-validator-real/`。
- **真实 Mac adapter 验收**：11 适配器、396 技能、Qwen 31 技能/1.3.1、Doubao not_installed；证据 `/tmp/agenthub-adapters-real/`。
- **真实 Mac 批量采纳（0.4.0）**：重新扫描 396 候选 → 默认跳过只读 273 + 去重 11 → 导入 112 个（约 55 MB，失败 0），中央库能力数 2 → **111**（112 条导入按版本共存形成 111 个 capability）；证据 `/tmp/agenthub-adopt-real/`。

> 以上为真实输出摘要；完整证据与残留状态见 [docs/ACCEPTANCE-REPORT.md](docs/ACCEPTANCE-REPORT.md)。「测试通过」不包含签名/公证、GUI 自动化或真实 Keychain 正向写入（DMG 已产出）。

---

## License

MIT（见根目录 `LICENSE` 与 `Cargo.toml` 的 `workspace.package.license`）。
