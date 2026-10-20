# AgentHub 测试计划

> 状态基准：2026-10-20 15:56（Asia/Shanghai）。所有结果均为本机真实执行输出摘要，**未通过的测试不写成通过**。真实 Mac 验收见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)。
> 测试隔离原则：**绝不触碰真实 Agent 目录**；所有测试使用临时 home（`tempfile` + `CoreConfig::for_home` / CLI `--home`），唯一例外是只读探测 `/Applications`。

---

## 0. 当前基线（实测）

| 层 | 命令 | 结果 | 说明 |
|---|---|---|---|
| core 单元测试 | `./scripts/cargo.sh test -p agenthub-core`（lib） | `running 0 tests` | 核心库内没有 `#[cfg(test)]` |
| core 集成测试 | 同上（tests/*） | **21 passed / 0 failed** | 8 个测试二进制；含 MCP 分发/移除保真 |
| Rust 测试合并跑 | `./scripts/cargo.sh test -p agenthub-core -p agenthub-cli` | **exit 0**（core 21 + CLI 9 = 30） | 2026-10-20 15:56 重跑 |
| core 文档测试 | 同上（doc-tests） | `0 tests` | — |
| CLI 集成测试 | `./scripts/cargo.sh test -p agenthub-cli` | **9 passed / 0 failed** | 含 dry-run 退出码、conflict、rollback、backup |
| CLI 编译 | `./scripts/cargo.sh check -p agenthub-cli` | 通过 | — |
| Tauri app 构建 | `pnpm tauri build --bundles app` | **通过** | 产物 `target/release/bundle/macos/AgentHub.app` |
| DMG 打包 | `pnpm tauri build`（含 DMG） | ⚠️ 未产出 `.dmg` | 沙箱 hdiutil 权限失败；仅生成 `bundle_dmg.sh`，非代码问题 |
| 前端构建 | `pnpm build` | **通过** | `vue-tsc --noEmit` + Vite build 成功 |
| 前端测试 | `pnpm test`（vitest） | **5 files / 21 tests passed** | status/markdown/format/i18n/app-smoke |
| Tauri 自动化测试 | 未执行 | 未验证 | 无 `src-tauri` 测试；GUI 为人工验收 |
| GUI 运行验收 | 隔离 home | ✅ 通过 | 截图 [gui-dashboard.png](evidence/gui-dashboard.png)、[gui-onboarding.png](evidence/gui-onboarding.png) |
| 真实 Mac 验收 | `scripts/real-acceptance.sh` | ✅ 通过 | 真实 `~/.agenthub` + `~/.codex` + `~/.zcode`；证据见 §0.3 |

> 与早期状态（同日 15:16 首次实测 `0 tests`）相比，集成测试、前端 spec、真实 Mac 验收与 GUI 验收已补齐；15:24–15:56 期间修复了 MCP preflight/env 计划、CLI dry-run，并补齐测试。本文以 15:56 的结果为准。

### 0.1 测试文件清单

| 文件 | 测试数 | 覆盖 |
|---|---|---|
| `crates/agenthub-core/tests/common/mod.rs` | — | 隔离 fixture（临时 home + project；`CoreConfig::for_home`） |
| `crates/agenthub-core/tests/lifecycle.rs` | 2 | 首次启动、库/DB/profiles 初始化、迁移与旧 operation 行解析 |
| `crates/agenthub-core/tests/scan_import.rs` | 4 | 扫描（全局+项目、归属）、文件夹/单 SKILL.md 导入、去重与版本变化、无效 SKILL.md |
| `crates/agenthub-core/tests/distribution.rs` | 5 | 双 Agent copy、重复计划 noop/二次 apply 拒绝、create 回滚、替换回滚、symlink + uninstall 保留库 |
| `crates/agenthub-core/tests/conflict.rs` | 2 | 非托管同名目录 conflict、外部漂移阻止非 force 重装 |
| `crates/agenthub-core/tests/mcp.rs` | 3 | 只读发现、TOML+JSON 分发/移除保真、缺 Keychain 值 blocked |
| `crates/agenthub-core/tests/security_zip.rs` | 4 | 路径穿越、绝对路径、symlink 条目、正常 zip |
| `crates/agenthub-core/tests/git_import.rs` | 1 | 本地 git 仓库候选选择与导入 |
| `crates/agenthub-cli/tests/cli.rs` | 9 | CLI 端到端：导入/扫描、install 门禁与 dry-run、conflict、rollback、sync、mcp discover、backup、JSON/错误 |
| `src/lib/__tests__/{status,markdown,format,i18n}.spec.ts` | 20 | 状态映射/矩阵、Markdown 解析、格式化、zh/en 结构一致性 |
| `src/__tests__/app-smoke.spec.ts` | 1 | 应用冒烟（jsdom） |

### 0.3 真实 Mac 验收（2026-10-20 15:55）

完整报告见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)；证据目录 `/tmp/agenthub-real-acceptance/`，脚本 [`scripts/real-acceptance.sh`](../scripts/real-acceptance.sh)。

| 步骤 | 结果 |
|---|---|
| 真实 `~/.agenthub` 导入唯一 slug `agenthub-accept-155547` | ✅ v1 hash `ba077e7c5ac1…` |
| copy 分发到 `~/.codex/skills` + `~/.zcode/skills` | ✅ 2 items create，字节一致 |
| 扫描 | ✅ Codex/ZCode `inSync=true` |
| 导入 v2 → `skills sync --dry-run` | ✅ updates=1/plans=1，**exit 0**（仅更新时 0；drift/missing/blocked/conflict 才 2） |
| `skills sync --yes` | ✅ 更新到 v2 |
| `skills rollback last` | ✅ 精确恢复 v1 字节 |
| `skills uninstall <ids> --yes` | ✅ 部署删除，库保留 2 版本 |
| backup + doctor | ✅ `backup-20261020-075557`；doctor `ok=true` |
| 目标目录最终状态 | ✅ 已删除（`~/.codex/skills/<slug>`、`~/.zcode/skills/<slug>` 均不存在） |

> 真实 profile 扫描为 367 skills / duplicates=4 / conflicts=7 / invalid=5（既有环境数据，不是验收新增）。

### 0.2 回归记录（已修复）

15:26 首次全量运行时曾有两个失败，现均已修复并纳入回归：

| 曾失败 | 原因 | 修复 |
|---|---|---|
| `mcp_distribution_and_removal_preserve_unrelated_configuration` | MCP preflight 用整文件 hash 对比条目 hash，非 force 移除报 `ExternalModification` | preflight 对 `install_kind=mcp_config` 改为读取并比较目标条目 hash（`ops.rs:220-260`）；`plan_remove` 记录当前条目 hash（`mcp.rs:1002-1060`） |
| `install_requires_yes_and_never_writes_during_dry_run` | `gate_plan` 先判 `!yes`，导致 `--dry-run` 单独使用返回 2 | `gate_plan` 先处理 dry-run 并返回 `plan_exit_code`（`agenthub-cli/src/main.rs:538-546`） |

---

## 1. 测试分层

| 层 | 范围 | 工具/位置 | 现状 |
|---|---|---|---|
| L1 单元 | `util`（hash/路径/原子写）、`skill` frontmatter/解压边界、`mcp` codec（读写/JSON path/TOML 保真）、`status.ts` 映射 | Rust `#[cfg(test)]` / vitest | 🟡 前端 21 个测试；核心仍无纯函数单元测试 |
| L2 集成 | `AgentHub` 全链路：扫描→导入→计划→执行→回滚、冲突/漂移、MCP 读写、迁移 | `crates/agenthub-core/tests/` | ✅ 21 个全部通过 |
| L3 CLI | 真实进程 + 隔离 home：退出码、JSON、门禁、无写入保证 | `crates/agenthub-cli/tests/cli.rs` | ✅ 9 个全部通过 |
| L4 手动验收 | 首次启动、真实 Agent 发现、GUI 交互、真实 Mac 写入、Keychain、权限/重启 | 本文 §5、[ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md) | ✅ 真实 Mac + GUI（隔离 home）人工验收通过；Keychain 正向/自动注入未覆盖 |

### 1.1 建议补充的单元测试

- `hash_dir`：排序稳定、`.DS_Store` 忽略、逃逸 symlink 拒绝、链接目标变化导致 hash 变化。
- `safe_segment`：大小写/空格/斜杠/中文/空值 → `unnamed`。
- `parse_frontmatter`：无 frontmatter、缺闭合、YAML 冒号、BOM、CRLF。
- `extract_archive`：`../`、绝对路径、symlink、NUL、条目数、解压总量（当前仅集成覆盖一部分）。
- `mcp::read/write_entries`：三种路径、TOML 注释保留、JSON 未知键、损坏文件拒绝覆盖、删除空表。
- `matrixCellState`：10 种状态优先级（`src/lib/status.ts`）。

---

## 2. 环境隔离要求（硬性）

1. **永远不要用真实 home 跑写测试**。Rust 集成测试用 `tempfile::tempdir()` + `CoreConfig::for_home(home)`；CLI 用 `--home <tmp>`（等价 `AGENTHUB_HOME`）。
2. **数据目录也在临时目录内**：`for_home` 使 `data_dir=<tmp>/.agenthub`；若用环境变量，显式设置 `AGENTHUB_DATA_DIR`。
3. **安全断言**：测试后检查真实 `~/.codex`、`~/.trae-cn`、`~/.zcode`、`~/.workbuddy`、`~/.dsh`、`~/.agents` 的 mtime/内容未变（可在 CI 外手动做一次基准快照）。
4. **不要写真实 Keychain**：测试只验证「未设置 → blocked」；不要调用 `mcp_set_secret` 写入真实 Keychain。需要正向测试时使用独立 key 前缀并清理，或在 CI 中跳过。
5. **`/Applications` 无法隐藏**：`find_app` 先查 `/Applications`（`adapters/mod.rs:176-197`），临时 home 仍会检测到本机真实安装的 app。测试应：
   - 显式指定 agent key（CLI/plan 参数），不要假设「只有 fixture 里的 Agent」；
   - 或在未来增加 `AGENTHUB_APP_ROOT` 覆盖（当前未实现，`AGENTHUB_EXTRA_APP_DIRS` 只追加）。
6. **Git 测试**：用本地临时仓库（`git init` + 本地路径/clone），不访问网络。
7. **Zip 测试**：夹具在临时目录生成，不依赖 `docs/compat-evidence/` 的真实数据；后者可用于解析兼容性用例（已脱敏）。
8. **CLI 测试**：使用 `CARGO_BIN_EXE_agenthub` 启动真实二进制（现有测试已如此）。

---

## 3. 场景矩阵

### 3.1 Skill 导入

| ID | 场景 | 输入 | 期望 | 覆盖 |
|---|---|---|---|---|
| S1 | 文件夹导入 | 含 `SKILL.md` 的目录 | 成功、库目录 hash 正确、来源=folder | ✅ scan_import |
| S2 | 单 SKILL.md | 一个 `.md` 文件 | 成功、只有 SKILL.md | ✅ scan_import |
| S3 | 多候选目录 | 目录下多个技能 | `Conflict`，列出候选 | 🟡 部分（resolve 逻辑） |
| S4 | 相同内容重复导入 | 同 hash | `created=false`，版本复用 | ✅ scan_import |
| S5 | 不同内容同名 | 同 slug 新内容 | 新版本、旧版本保留、`current_version_id` 更新 | ✅ scan_import |
| S6 | 无效 SKILL.md | 无 frontmatter/缺 description | `InvalidSkill`，不落库/不落版本 | ✅ scan_import |
| S7 | zip 安全 | 路径穿越/绝对路径/symlink/超限 | 拒绝，extraction root 之外无文件 | ✅ security_zip |
| S8 | Git 导入 | 本地仓库、多技能 | 列出候选、按 `--skill-path` 导入、删除 `.git` | ✅ git_import |
| S9 | 无网络 Git | 网络 URL 不可达 | 错误可读，不写库 | ⛔ 未覆盖 |

### 3.2 Skill 分发 / 更新 / 卸载 / 回滚

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| D1 | 双 Agent copy | 两个目标都成功、installations 2 行、hash 匹配 | ✅ distribution |
| D2 | 重复计划 | 第二次为 noop，不重复写 | ✅ distribution |
| D3 | 二次 apply 同一操作 | `Conflict` 拒绝 | ✅ distribution |
| D4 | create 回滚 | 部署目录被删除、installation 删除 | ✅ distribution |
| D5 | 替换回滚 | 恢复旧内容与旧 installation 记录 | ✅ distribution |
| D6 | symlink 安装 | 链接指向库、uninstall 只删链接不动库 | ✅ distribution |
| D7 | 非托管同名目录 | plan=conflict、非 force apply 拒绝 | ✅ conflict |
| D8 | 外部漂移 | drift 报告、非 force 重装拒绝 | ✅ conflict |
| D9 | 一个目标失败 | 其他目标自动恢复、operation=failed | 🟡 逻辑存在；无专门测试（可用不可写目录模拟） |
| D10 | 回滚时目标被外部改 | 该目标回滚失败、整体 partial、不覆盖 | 🟡 本次 harness 观察到 partial；无自动测试 |
| D11 | 项目级安装 | scope=project 的落点与记录 | ⛔ 未实现（installations.scope 恒 global） |
| D12 | 目标不在白名单根 | 拒绝 | ⛔ `target_within_roots` 未接入 |
| D13 | 卸载（按 installation id） | 删除部署、保留库/快照 | ✅ 真实 Mac 验收 + `skills uninstall`（无自动测试） |
| D14 | `sync --dry-run` 仅有更新 | updates/plans 正常，exit 0，不写 | ✅ 真实 Mac 验收；自动测试覆盖 update 场景 |
| D15 | `sync --dry-run` 有 drift/missing/blocked/conflict | exit 2 | 🟡 代码逻辑具备；无专门测试 |
| D16 | operation 审计/导入审计 | `operations` + `logs/operations.log` | ✅ 真实 Mac 验收（日志 16 行）；无自动断言 |

### 3.3 MCP

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| M1 | 只读发现 | 读 `config.toml`/`mcp.json` 条目，不写文件 | ✅ mcp + CLI |
| M2 | TOML 分发保真 | 只增/改 `[mcp_servers.*]`，保留注释/其他表 | ✅ mcp（分发部分通过）；移除失败见 §0.2 |
| M3 | JSON 分发保真 | 只改目标 key，保留未知顶层键 | ✅ mcp + CLI 手动 |
| M4 | 移除仅删托管条目 | 目标 key 消失，其他保留 | 🟡 测试存在但失败（preflight hash 缺陷） |
| M5 | 缺 Keychain 值 | 计划项 blocked；apply 跳过该 Agent（partial），不写 | ✅ mcp |
| M6 | 损坏 JSON/TOML | 拒绝覆盖、原文件不变 | 🟡 代码具备；无专门测试 |
| M7 | 不支持传输 | 计划 blocked（无支持该 transport 的目标） | 🟡 逻辑具备；无专门测试 |
| M8 | 同文件多 server | 第二次分发不覆盖第一个 | 🟡 计划/写入逻辑可支持；测试仅部分 |
| M9 | MCP 回滚 | 恢复配置文件到操作前 | 🟡 harness 实测成功；无自动测试 |
| M10 | env 值不进入 plan | `operations.plan.payload` 无 env 值 | 🟡 代码已剥离（`mcp.rs:883-890`）；无专门断言 |
| M11 | headers 敏感值 | headers 明文（已知风险） | ⛔ 安全测试未覆盖 |

### 3.4 Adapter / 环境异常

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| A1 | Agent 未安装 | status=not_installed，plan 产生 blocked 项，不写 | 🟡 部分 |
| A2 | Agent 目录缺失 | 扫描跳过 root，doc 给出 error 提示 | ✅ scan |
| A3 | 权限不足 | 可读性探测 false；plan blocked；错误可读 | ⛔ 未覆盖 |
| A4 | symlink 目标被替换 | 只信 `symlink_metadata`，不跟随删除 | 🟡 代码具备；无测试 |
| A5 | 同名 app（ChatGPT/Codex） | 按 CFBundleName 区分 | ⛔ 未实现（已知限制） |
| A6 | `VerificationRequired` | 允许安装但 UI 强提示 | 🟡 逻辑允许；UI 待实现 |
| A7 | 自定义 skill 根 | 绝对路径校验、参与扫描/安装 | ⛔ 未覆盖 |

### 3.5 可靠性

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| R1 | 中断（apply 中 kill） | 已写项保留 + 快照可回滚；DB 状态不出现假 success | ⛔ 未覆盖 |
| R2 | 迁移（user_version） | v1 初始化、未来 vN 升级；旧 operation 行可解析 | ✅ lifecycle（仅 v1 + 旧 JSON） |
| R3 | 备份 | 生成 DB 副本 + manifest；列表可读 | ✅ lifecycle/CLI |
| R4 | 从备份恢复 | 恢复 DB/库 | ⛔ 未实现 |
| R5 | 并发（GUI+CLI） | `.agenthub.lock` 串行、无交叉写 | ⛔ 未覆盖 |
| R6 | 重复执行 | 二次 apply/rollback 被拒绝 | ✅ distribution |
| R7 | 磁盘满/只读 | 错误可读、快照不破坏原文件 | ⛔ 未覆盖 |
| R8 | 大 zip/zip bomb | 超过 100MB/4000 条目被拒绝 | 🟡 逻辑具备；未测 100MB 边界 |

---

## 4. 手工验证与真实验收

### 4.1 隔离 home 临时 harness（2026-10-20 15:22–15:25，未入库）

用同一份 `agenthub-core` 编译的临时 harness 与真实 CLI 验证：

| 验证 | 结果 |
|---|---|
| 扫描：`.trae-cn/skills`、`.dsh/skills`、`.agents/skills` fixture | `total=5 duplicates=1 conflicts=0 invalid=1`；同内容跨 Agent 计 duplicate，不同 slug 内容不误报 |
| 导入（AgentDir） | `created=true`、`version=1.0.0`、来源 `agent_dir`、库目录 `library/skills/alpha/<hash12>` |
| 双 Agent 分发（TRAE CN noop + DSH create） | apply `success`；TRAE 已存在同内容 → noop，DSH 写入成功 |
| 外部修改后检查变更 | `drifted=1`；导入新版本后 `update_available=true`、`updates=1` |
| 回滚（目标被外部修改） | 回滚 `partial`，拒绝覆盖被改目标（符合设计） |
| MCP TOML 分发（当时用 `--force`；修复后非 force 亦可） | 成功；`# comment`、`[other]` 保留，新增 `[mcp_servers.srv-toml]` |
| MCP TOML 移除（当时用 `--force`；修复后非 force 亦可） | 成功；条目消失，注释/其他表保留，留下空 `[mcp_servers]` 表 |
| MCP JSON 分发/回滚（当时用 `--force`） | 未知顶层键保留；写入 `mcp.servers.srv-json`；回滚恢复原 JSON 内容 |
| MCP 缺 Keychain 值 | 计划项 `Blocked`；apply `partial`（跳过），不写配置 |
| CLI 全流程 | `doctor`/`agents list`/`skills scan|import|install --yes`/`operations list`/`skills rollback last`/`backup create` 均按预期；`--json` 可解析 |

> harness 未提交到仓库；正式回归以 §0 的测试文件为准。

### 4.2 真实 Mac 验收（2026-10-20 15:55，已入库脚本）

使用 [`scripts/real-acceptance.sh`](../scripts/real-acceptance.sh) 在真实 home 执行，机器所有者已批准。结果：导入→双 Agent copy 分发→扫描 inSync→v2 导入→`sync --dry-run`（updates=1/plans=1，exit 0）→`sync --yes`→`rollback last` 精确 v1→`uninstall` 删除部署且库保留 2 版本→backup/doctor ok→release app 安装。完整证据见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)；证据目录 `/tmp/agenthub-real-acceptance/`。

### 4.3 建议继续固化为自动测试的场景

1. MCP 回滚（现有测试主要覆盖 skill 回滚）。
2. `skills uninstall` / `skills sync` 的 CLI 退出码与“dry-run 不写”断言。
3. operation/import 审计日志与 `installations.scope` / `scan_results.scope` 的数据库断言。
4. 外部漂移后的回滚 partial 行为。
5. MCP plan payload 不含 env 值（安全回归）。

---

## 5. 人工验收步骤

> 首版验收已在真实 Mac 与隔离 home GUI 上完成（见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)）；以下步骤用于复跑/回归。**日常测试仍使用隔离 home；真实 Mac 写入必须获得机器所有者授权**。

### 5.1 准备

```bash
export AGENTHUB_HOME=/tmp/agenthub-accept/home
mkdir -p "$AGENTHUB_HOME/.codex" "$AGENTHUB_HOME/.trae-cn" "$AGENTHUB_HOME/.dsh"
# 放入一个 fixture 技能
mkdir -p "$AGENTHUB_HOME/.codex/skills/demo"
cat > "$AGENTHUB_HOME/.codex/skills/demo/SKILL.md" <<'EOF'
---
name: demo
description: Acceptance test skill.
version: 1.0.0
---
# demo
EOF
BIN=./target/debug/agenthub
```

### 5.2 首版验收场景（对应 PRD §6）

> 已在真实 Mac 执行通过；下表为可复跑步骤（隔离 home 版）。真实 Mac 版使用唯一 slug 的 `scripts/real-acceptance.sh`。

| 步 | CLI 命令 | UI 路径 | 判定 |
|---|---|---|---|
| 1 扫描 | `$BIN --home "$AGENTHUB_HOME" skills scan` | Agent → 扫描 | 报告含 demo；无 Agent 目录写入 |
| 2 导入 | `$BIN --home "$AGENTHUB_HOME" skills import --path "$AGENTHUB_HOME/.codex/skills/demo"` | 技能库 → 导入 | `Imported`，库目录/版本/来源正确 |
| 3 分发 | `$BIN --home "$AGENTHUB_HOME" skills install demo --agents codex,trae-cn --dry-run` 然后 `--yes` | 技能详情 → 选择 Agent → 预览 → 应用 | 计划 create；apply success；两目录存在且 hash 一致 |
| 4 修改库版本 | 改源内容后重新 `skills import --path ...` | 技能库 → 导入新版本 | `update_available=true`；已部署内容不变 |
| 5 一键恢复 | `$BIN --home "$AGENTHUB_HOME" skills rollback last` | 活动 → 操作详情 → 回滚 | 目标恢复；原操作 `rolled_back` |
| 6 卸载 | `$BIN --home "$AGENTHUB_HOME" skills list --json` 取 installation id，再 `$BIN --home "$AGENTHUB_HOME" skills uninstall <id> --yes` | 技能详情 → 卸载 | 部署删除、库与快照保留 |
| 7 MCP（可选） | 先用 Tauri `mcp_upsert` 建 server；`$BIN --home "$AGENTHUB_HOME" mcp distribute srv --agents codex --yes` | MCP → 新建 → 分发 → 预览 | 写入成功、无关配置保留（无 force；env 不进 plan） |

### 5.3 安全与可靠性人工检查

1. 扫描前后对比真实 `~/.codex` 等目录 mtime：不得变化。
2. 计划生成后手工改目标内容 → apply 应报 `ExternalModification`。
3. 目标目录设为只读 → apply 失败且已成功项被恢复。
4. `doctor` 检查 library/database/data_dir/git；破坏 DB 后 `doctor` 应报 error。
5. 断网执行扫描/导入/分发（非 Git）→ 正常。
6. Keychain 未初始化时设置 secret → 错误可读（不泄露值）。

### 5.4 GUI 验收（已完成一次）

隔离 home（`/tmp/agenthub-gui/home`）下人工验收通过：Dashboard 显示真实数据（Agent 6/9、技能 1、安装 2、MCP 1、插件 1、待更新 1、扫描 7 技能/重复 1/冲突 0/无效 0），onboarding 5 步第 1 步正常，中文界面与活动历史正常。截图见 [ACCEPTANCE-REPORT.md §3](ACCEPTANCE-REPORT.md)。仍需补自动化回归（Playwright/XCUITest）与更多页面深度操作。

### 5.5 真实 Mac 验收复跑

```bash
# 需机器所有者授权；会写真实 ~/.agenthub / ~/.codex/skills / ~/.zcode/skills
bash scripts/real-acceptance.sh
```

---

## 6. 未覆盖项 / 风险

| 项 | 风险 | 建议 |
|---|---|---|
| GUI 自动化 | 已人工运行验收，但无 Tauri 自动化回归 | 补 Playwright/XCUITest 或稳定的截图对比 |
| DMG / 签名 / 公证 | 沙箱 hdiutil 失败；未签名 | 在有权限桌面会话重跑 `pnpm tauri build`；配置 Developer ID + notarization |
| Keychain 正向路径 | 未测（避免污染真实 Keychain） | 使用专用测试 service 前缀 + 清理，或标记 manual-only |
| 并发/锁 | 未测 | 两进程同时 apply 的集成测试 |
| 中断恢复 | 未测 | 在 apply 中注入失败/杀进程，验证快照与 DB 状态 |
| 备份恢复 | 未实现 | 先设计恢复流程再测 |
| `/Applications` 探测导致的不确定性 | 测试可能看到真实 app | 增加可覆盖 app root 的测试钩子 |
| 第三方 skill 安全审查 | 无静态分析 | 后续增加脚本/`allowed-tools`/危险命令扫描 |
| 非 macOS 平台 | 未验证 | 明确 MVP 不支持 |
| MCP 真实配置写入 | 验收未写入真实 MCP 配置 | 用隔离 home/config 或单独授权的临时目标测试 |
| 真实 profile 数据噪声 | 扫描 367 技能出现既有 conflicts/duplicates/invalid | 验收/演示时用隔离 home；真实 profile 显示为「需关注」而非缺陷 |
| 审计日志/scope 断言 | 有实现与真实验收，但无自动断言 | 增加 DB/日志断言测试 |

---

## 6.5 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：core 21 + CLI 9 = 30 个 Rust 测试、前端 21 个测试全部通过；`pnpm build`、`pnpm tauri build --bundles app` 通过；**真实 Mac 验收通过**（导入/分发/inSync/更新/回滚/卸载/备份/doctor）；GUI（隔离 home）人工验收通过。
- **未验证**：DMG 打包（沙箱 hdiutil）、签名/公证、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断/磁盘满、备份恢复、非 macOS。
- **已知限制**：核心缺纯函数单元测试；前端无 Tauri 运行时测试；部分场景仅手工（见 §6）；测试受 `/Applications` 真实 app 影响。

## 7. 交叉链接

- 功能验收场景：[PRD.md §6](PRD.md)
- 写入事务与快照机制：[ARCHITECTURE.md §3](ARCHITECTURE.md)
- 安全测试关注点：[SECURITY.md §7](SECURITY.md)
- 适配器证据与路径：[ADAPTER-SPEC.md](ADAPTER-SPEC.md)
