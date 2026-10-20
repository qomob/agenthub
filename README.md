# AgentHub

> 本机多 Agent 能力管理中心：把散落在 Codex、TRAE、ZCode、WorkBuddy、DeepSeek Harness 等 Agent 目录里的 **Skills / MCP / 插件清单** 集中登记，通过适配器与事务化写入分发到指定 Agent，出错可回滚。
>
> 状态快照：**2026-10-20 15:32（Asia/Shanghai）**。核心库、CLI、Tauri 命令层与前端页面均已落地；core 21、CLI 9、前端 21 个测试全部通过，`pnpm build` 通过。GUI 运行时（Tauri 窗口内全链路）与 `pnpm tauri build` 尚未验收。详见 [已知限制](#已知限制) 与 [docs/CHANGELOG.md](docs/CHANGELOG.md)。

AgentHub 是 **本地优先** 的 macOS 桌面应用（Tauri 2 + Vue 3 + Rust + SQLite），无账号、无云端、无遥测。GUI 与 CLI 共用同一个 `agenthub-core` 引擎。

---

## 目录

- [状态标记](#状态标记)
- [核心原则](#核心原则)
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

- **已验证**：本次通读代码 + 在隔离 home 中用 CLI/harness 跑通的行为（扫描、导入、分发、回滚、备份、诊断；MCP TOML/JSON 写入）。测试：core 21、CLI 9、前端 21 全部通过；`pnpm build` 通过。
- **未验证**：Tauri 窗口内的 GUI 全链路、`pnpm tauri build` 打包/签名、真实 Agent 目录写入、Keychain 正向写入、非 macOS 平台、部分 adapter 路径（Codex 项目级、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders`）。
- **已知限制**：见下节；敏感数据（env 计划明文、headers 明文、secret argv、快照/备份含 token）与安全边界见 [docs/SECURITY.md](docs/SECURITY.md)。

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

完整定义见 [docs/PRD.md](docs/PRD.md)。

---

## 功能状态

状态：✅ 已实现（代码可证）/ 🟡 部分或未验证 / ⛔ 未实现 / ⚠️ 已知缺陷。

### Phase 1 — 本机能力管理（核心已完成，收尾中）

| 能力 | 状态 | 说明 |
|---|---|---|
| 9 个 Agent 检测与 roots 盘点 | ✅ | `crates/agenthub-core/src/adapters/builtin.rs` |
| 只读扫描（全局/项目/自定义） | ✅ | 同内容跨 Agent 计 duplicate，同 slug 不同内容计 conflict |
| 导入：folder / zip / SKILL.md / Git / Agent 目录 | ✅ | zip 有路径穿越/symlink/大小/条目数防护 |
| 中央库版本化（内容寻址） | ✅ | `library/skills/<slug>/<hash12>/` |
| 事务分发（copy / symlink） | ✅ | 预检 + 快照 + 校验 + 失败补偿 |
| 更新检测 / 漂移 / 缺失检测 | ✅ | Dashboard 与 `check_changes` |
| 操作级回滚 | ✅ | 每个操作最多一次；拒绝覆盖外部修改 |
| CLI | ✅ | 计划门禁、`--json`、退出码 0/1/2 |
| 集成测试 | ✅ | core 21/21、CLI 9/9、前端 21/21 通过（2026-10-20 15:32 实测） |
| GUI | 🟡 | 10 条路由 + 11 个视图 + 17 组件已实现，`pnpm build` 通过；运行时未验收 |

### Phase 2 — MCP（代码完成，正常路径有缺陷）

| 能力 | 状态 | 说明 |
|---|---|---|
| MCP 定义 CRUD（stdio/http/sse/streamable_http） | ✅ | 名称唯一；command/url 互斥校验 |
| 从 Agent 配置只读发现 | ✅ | `json:mcpServers`、`json:mcp.servers`、`toml:mcp_servers` |
| Keychain 存 env 值 | ✅ | service `com.agenthub.mcp`；缺值 → blocked |
| 多格式写回 + 保真 | ✅ | TOML 保留注释/其他表；JSON 保留未知键（格式会重排） |
| 分发 / 移除 / 回滚 | ✅ | 正常路径已可用；已有配置文件时按条目 hash 检测，无需 `--force` |
| DSH Cordis YAML / TRAE 全局 / ZCode 原生项目配置 | ⛔ | 未声明 codec |

### Phase 3 — 工作流与可视化（部分）

| 能力 | 状态 | 说明 |
|---|---|---|
| 能力矩阵（skill × agent） | ✅ | 页面与 `matrixCellState` 已实现 |
| Profiles（4 内置 + 自定义 + 应用） | ✅ | 应用复用安装计划 |
| 插件只读清单 | ✅ | Codex/TRAE CN/WorkBuddy；不安装/不启停 |
| 活动与诊断（4 Tab） | ✅ | 操作/快照/备份/诊断与变更 |
| Onboarding 5 步 | ✅ | 隐私 → 库 → Agent → 扫描 → 完成 |
| 启动自动扫描 | 🟡 | UI 会按设置调用 scan；核心 `open()` 本身不扫描 |
| 结构化日志 / 备份恢复 | ⛔ | `logs/` 空；备份只能创建/列出 |

> 详细页面规格见 [docs/UI-SPEC.md](docs/UI-SPEC.md)，阶段路线见 [docs/ROADMAP.md](docs/ROADMAP.md)。

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

> **当前快照的构建状态**：`pnpm build` 已通过（`vue-tsc --noEmit` + Vite，dist 生成）；`pnpm tauri build` 依赖 Rust 打包与签名环境，本快照未执行验证。

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

# 更新/漂移检查：生成计划（不写）
$BIN skills sync --dry-run
$BIN skills sync --yes

# 回滚最近一次可回滚操作（或指定 operation id）
$BIN skills rollback last
$BIN skills rollback <operation-id>

# 操作历史 / Profiles / 备份
$BIN operations list --limit 20
$BIN profiles list
$BIN backup create
$BIN backups list

# MCP：列出、发现、分发（CLI 没有「创建 MCP 定义」命令）
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
├── agenthub.db                  # SQLite：agents/capabilities/versions/installations/
│                                #   mcp_definitions/operations/snapshots/scan_results/...
├── library/
│   └── skills/<slug>/<hash12>/  # 不可变技能版本（SKILL.md + 资源）
├── snapshots/<operation-id>/    # 每次写入前的目标备份（目录/文件）
├── backups/backup-YYYYMMDD-HHMMSS/
│   ├── agenthub.db              # VACUUM INTO 的一致性数据库副本
│   └── manifest.json
├── staging/                     # 导入/解压临时目录（自动清理）
└── logs/                        # 预留；当前没有写入方
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

- **纯本地**：无账号、无遥测、无云同步；唯一出站是用户显式触发的 `git clone`。
- **写入前预览**：GUI 弹计划、CLI 默认只打印计划；目标、动作、哈希、警告、重启提示全部可见。
- **快照与回滚**：每次写入前快照；失败自动逆序恢复；操作级回滚需显式确认。
- **不可信输入**：zip 拒绝路径穿越/绝对路径/symlink 条目/超限；Git 只浅克隆、不执行仓库脚本、删除 `.git`；Markdown 渲染不使用 `v-html`。
- **凭据**：MCP env 值存 Keychain，数据库只存 key 名；UI/日志原则是不显示值。
- **已知风险（必须在 UI/使用中留意）**：
  1. MCP headers 当前**明文**入库并写入 Agent 配置；
  2. 分发计划 JSON 可能包含从 Keychain 取出的 env 值（SQLite 明文）；
  3. `/usr/bin/security ... -w <value>` 会让 secret 短暂出现在进程参数；
  4. 快照/备份是含配置文件的普通副本，可能包含 token，且未加密；
  5. 应用**不是 App Sandbox**、未签名/未公证，以当前用户权限运行；
  6. Claude Code / Cursor 标为 `VerificationRequired` 但仍允许写入，使用前请自行确认。

完整威胁模型与控制见 [docs/SECURITY.md](docs/SECURITY.md)。

---

## 已知限制

| 类别 | 限制 |
|---|---|
| GUI | 页面已构建，但未做运行时端到端验收；浏览器环境只能显示「请运行 pnpm tauri dev」 |
| 测试 | Tauri 无自动化测试；GUI 运行时未验收；核心缺纯函数单元测试 |
| MCP | DSH Cordis YAML、TRAE 全局、ZCode 原生项目配置未实现；删除后会留下空表/空对象 |
| MCP 敏感数据 | env 值进入 `operations.plan`；headers 明文；详见 SECURITY |
| 能力范围 | 插件只读；不执行 skill 脚本；不试连 MCP；不提供市场/云同步/调用统计 |
| 记录 | 导入与扫描不写 `operations`；无结构化日志；备份无恢复路径 |
| Adapter | Codex 项目级根、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders` 等仍未验证；ChatGPT 只读；同名 app 未按 `CFBundleName` 区分 |
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

### 数据库被锁 / 两个实例

- apply/rollback 使用 `<library_root>/.agenthub.lock` 跨进程互斥；SQLite 使用 WAL。
- 但 import/设置/MCP 定义写入没有跨进程锁；不要同时用 GUI 与 CLI 做写操作。

### `pnpm test` 的 jsdom 警告

- 输出中的 `Not implemented: Window's scrollTo()` 来自 jsdom 环境，不影响测试结果（5 个文件 / 21 个测试通过）。

### 空白窗口 / 找不到 Tauri 后端

- 浏览器直接打开 Vite 页面会显示「桌面后端不可用：请运行 pnpm tauri dev」；这是预期提示，不是 mock。
- 确认 Tauri 窗口打开且 dev server 端口 1420 未被占用。

---

## 仓库结构

```text
crates/agenthub-core/          # 核心引擎（无 Tauri 依赖）
  src/
    adapters/{mod,builtin}.rs  # AgentAdapter + 9 个适配器
    db.rs                      # SCHEMA_V1、迁移、SQL
    library.rs                 # 中央库布局/备份
    mcp.rs                     # MCP codec/Keychain/写入
    model.rs                   # 公共模型（camelCase JSON）
    ops.rs                     # 事务执行/快照/回滚
    scan.rs                    # 只读扫描
    service.rs                 # AgentHub 门面（GUI/CLI 共用）
    skill.rs                   # SKILL.md 解析/导入安全
    util.rs                    # hash/原子写/路径安全
  tests/                       # 21 个集成测试
crates/agenthub-cli/           # CLI（1324 行薄前端）
  tests/cli.rs                 # 9 个进程级测试
src-tauri/                     # Tauri 2 命令层（38 个命令）
src/                           # Vue 3 UI（主体已实现，SettingsView 待补）
docs/                          # PRD/架构/数据/适配器/安全/UI/测试/路线图/变更
scripts/cargo.sh|cargo-env.sh  # 固定 rustup stable + /tmp CARGO_HOME
```

---

## 文档索引

| 文档 | 内容 | 状态 |
|---|---|---|
| [docs/PRD.md](docs/PRD.md) | 定位、用户、7 条原则、功能 A–G、MVP 边界、验收场景 | ✅ 本次 |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 分层、时序、事务/快照、锁、扩展点、已知缺陷 | ✅ 本次 |
| [docs/DATA-MODEL.md](docs/DATA-MODEL.md) | 13 张表、字段/约束/索引、ER、迁移、敏感数据 | ✅ 本次 |
| [docs/ADAPTER-SPEC.md](docs/ADAPTER-SPEC.md) | Trait 语义、9 个适配器矩阵、新增步骤、codec 保真 | ✅ 本次 |
| [docs/SECURITY.md](docs/SECURITY.md) | 威胁模型、已实现控制、剩余风险、确认点 | ✅ 本次 |
| [docs/UI-SPEC.md](docs/UI-SPEC.md) | 9+1 页面、任务流、状态/token、预览确认、实现状态 | ✅ 本次 |
| [docs/TEST-PLAN.md](docs/TEST-PLAN.md) | 测试分层、隔离要求、场景矩阵、真实结果、人工验收 | ✅ 本次 |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phase 0–4、验收标准、未来候选、风险登记 | ✅ 本次 |
| [docs/CHANGELOG.md](docs/CHANGELOG.md) | 0.1.0 Added/Changed/Fixed/Known issues | ✅ 本次 |
| [docs/AGENT-COMPATIBILITY.md](docs/AGENT-COMPATIBILITY.md) | 本机 9 个 Agent 路径/格式/证据矩阵 | 📚 调研同事维护 |
| [docs/REFERENCE-STUDY.md](docs/REFERENCE-STUDY.md) | Agent Skills / MCP 规范与各 Agent 官方文档研究 | 📚 调研同事维护 |
| [docs/compat-evidence/](docs/compat-evidence/README.md) | 脱敏证据、symlink 验证方案、交叉核对 | 📚 调研同事维护 |

---

## 测试与验证

```bash
# Rust（必须用 wrapper）
./scripts/cargo.sh test -p agenthub-core
./scripts/cargo.sh test -p agenthub-cli

# 前端（当前无 spec，会 exit 1）
pnpm test
```

**当前真实结果（2026-10-20 15:32）**：

- `agenthub-core`：21 个集成测试，**21 通过 / 0 失败**。
- `agenthub-cli`：9 个集成测试，**9 通过 / 0 失败**。
- `pnpm test`（vitest）：5 个文件 / 21 个测试，**全部通过**。
- `pnpm build`：**通过**（`vue-tsc --noEmit` + Vite build）。
- `pnpm tauri build`：本快照未执行（未验证打包/签名）。

> 以上为真实输出摘要；「测试通过」仅指上述命令与快照，不包含 GUI 运行时、真实 Agent 写入或签名打包。修复/剩余项见 [docs/TEST-PLAN.md](docs/TEST-PLAN.md) 与 [docs/ROADMAP.md](docs/ROADMAP.md)。

---

## License

MIT（见 `Cargo.toml` 的 `workspace.package.license`）。
