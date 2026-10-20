# AgentHub 测试计划

> 状态基准：2026-10-20 17:32（Asia/Shanghai）。所有结果均为本机真实执行输出摘要，**未通过的测试不写成通过**。真实 Mac 验收见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)。
> 测试隔离原则：**绝不触碰真实 Agent 目录**；所有测试使用临时 home（`tempfile` + `CoreConfig::for_home` / CLI `--home`），唯一例外是只读探测 `/Applications`。

---

## 0. 当前基线（实测）

| 层 | 命令 | 结果 | 说明 |
|---|---|---|---|
| core 单元测试 | `./scripts/cargo.sh test -p agenthub-core`（lib） | **44 passed / 0 failed** | audit/deps/impact/validation/agent_health + 既有模块单测 |
| core 集成测试 | 同上（tests/*） | **38 passed / 0 failed** | 10 个测试二进制；新增 scan_adopt 2；validator 11 |
| core 合计 | 同上 | **82 passed / 0 failed** | 44 unit + 38 integration |
| core 文档测试 | 同上（doc-tests） | `0 tests` | — |
| CLI 集成测试 | `./scripts/cargo.sh test -p agenthub-cli` | **16 passed / 0 failed** | 9 旧 + 7 validator CLI（`validator_cli.rs`） |
| Rust 合计 | 两条命令 | **98 passed / 0 failed** | 2026-10-20 17:32 重跑 |
| Tauri 编译 | `./scripts/cargo.sh check -p agenthub` | **exit 0** | 47 个命令 |
| 前端构建 | `pnpm build` | **通过** | `vue-tsc --noEmit` + Vite build |
| 前端测试 | `pnpm test`（vitest） | **8 files / 38 tests passed** | 新增 health/health-ui/discovered |
| Tauri 自动化测试 | 未执行 | 未验证 | 无 `src-tauri` 测试；GUI 为人工验收 |
| GUI 运行验收 | 隔离 home | ✅ 通过 | Dashboard/health/onboarding 截图见 `docs/evidence/` |
| 真实 Mac（第一轮） | `scripts/real-acceptance.sh` | ✅ 通过 | 安装/分发/更新/回滚/卸载；证据 `/tmp/agenthub-real-acceptance/` |
| 真实 Mac（validator） | 人工 CLI 序列 | ✅ 通过 | DB v1→v2、Agent 版本、audit/deps/impact/validate；证据 `/tmp/agenthub-validator-real/` |
| DMG | `pnpm tauri build` | ✅ 已产出 0.4.0 | `AgentHub_0.4.0_aarch64.dmg`（3.97 MiB，17:37）；`/Applications` 已安装 0.4.0 |

> 与第一轮（core 21/CLI 9/前端 21）相比，本轮新增 core 44 个 unit + validator 11 个 integration、adapters_extended 4 个、scan_adopt 2 个、CLI validator 7 个、前端 17 个测试。本文以 17:32 的结果为准。

### 0.1 测试文件清单

| 文件/位置 | 测试数 | 覆盖 |
|---|---|---|
| `crates/agenthub-core/src/**`（内嵌 unit） | 44 | audit 10、deps 8、impact 7、validation 13、agent_health 6 等 |
| `crates/agenthub-core/tests/lifecycle.rs` | 2 | 首次启动、库/DB/profiles、v1→v2 迁移 |
| `crates/agenthub-core/tests/scan_import.rs` | 4 | 扫描/导入/去重/无效 SKILL.md |
| `crates/agenthub-core/tests/distribution.rs` | 5 | 双 Agent copy、noop、回滚、symlink/uninstall |
| `crates/agenthub-core/tests/conflict.rs` | 2 | 非托管同名目录、外部漂移 |
| `crates/agenthub-core/tests/mcp.rs` | 3 | 只读发现、TOML+JSON 分发/移除、缺 Keychain 值 |
| `crates/agenthub-core/tests/security_zip.rs` | 4 | 路径穿越、绝对路径、symlink、正常 zip |
| `crates/agenthub-core/tests/git_import.rs` | 1 | 本地 git 导入 |
| `crates/agenthub-core/tests/validator.rs` | **11** | 静态审计、依赖、影响、健康、Agent 版本、DB v2、same-second 排序、密钥脱敏 |
| `crates/agenthub-core/tests/adapters_extended.rs` | **4** | 注册表含 11；Qwen 检测/扫描/插件；Qwen MCP 写入/移除保真；Doubao workspace 扫描/无根 blocked/注册 custom root 后部署 |
| `crates/agenthub-core/tests/scan_adopt.rs` | **2** | 跨 Agent 去重 + 只读跳过 + dryRun 不写 + 幂等 + include-read-only；按 agent 过滤 |
| `crates/agenthub-cli/tests/cli.rs` | 9 | 扫描/导入/install 门禁/rollback/sync/mcp/backup |
| `crates/agenthub-cli/tests/validator_cli.rs` | **7** | `health`、`skills audit|deps|impact|validate`、退出码、JSON |
| `src/lib/__tests__/{status,markdown,format,i18n,health}.spec.ts` | 28（合计） | 状态映射、Markdown、格式化、i18n、health 映射/筛选 |
| `src/__tests__/{app-smoke,health-ui}.spec.ts` | 8（合计） | 应用冒烟、Health 页/组件 |

> 前端合计 8 文件 / 38 测试；上表拆分仅说明覆盖来源。

### 0.2 回归记录（已修复）

| 曾失败/风险 | 原因 | 修复与回归 |
|---|---|---|
| `mcp_distribution_and_removal_preserve_unrelated_configuration`（第一轮） | MCP preflight 用整文件 hash 对比条目 hash | preflight 改按条目 hash（`ops.rs:220-260`）；`plan_remove` 记录当前条目 hash |
| `install_requires_yes_and_never_writes_during_dry_run`（第一轮） | `gate_plan` 先判 `!yes`，`--dry-run` 返回 2 | `gate_plan` 先处理 dry-run；测试通过 |
| **same-second Agent 版本排序（QA 发现）** | 同一秒多条 `agent_versions` 以 `detected_at` 排序时可能取到旧行 | `latest_agent_version`/`list_agent_version_changes` 使用 `detected_at DESC, rowid DESC`；回归用例 `latest_agent_version_tie_returns_stale_row_within_same_second` 已启用并通过 |
| 人工 passed 覆盖 audit fail | 早期健康推导只看人工证据 | `validation::derive_health` 安全优先；`manual_pass_cannot_override_a_failed_static_audit` |

### 0.3 真实 Mac 验收（第一轮，2026-10-20 15:55）

完整报告见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)；证据 `/tmp/agenthub-real-acceptance/`，脚本 [`scripts/real-acceptance.sh`](../scripts/real-acceptance.sh)。

| 步骤 | 结果 |
|---|---|
| 真实 `~/.agenthub` 导入唯一 slug `agenthub-accept-155547` | ✅ v1 hash `ba077e7c5ac1…` |
| copy 分发到 `~/.codex/skills` + `~/.zcode/skills` | ✅ 2 items create，字节一致 |
| 扫描 | ✅ Codex/ZCode `inSync=true` |
| 导入 v2 → `skills sync --dry-run` | ✅ updates=1/plans=1，**exit 0** |
| `skills sync --yes` / `rollback last` | ✅ 更新 v2 / 精确恢复 v1 |
| `skills uninstall <ids> --yes` | ✅ 部署删除，库保留 2 版本 |
| backup + doctor | ✅ `backup-20261020-075557`；doctor `ok=true` |

> 真实 profile 扫描为 367 skills / duplicates=4 / conflicts=7 / invalid=5（既有环境数据）。

### 0.4 真实 Mac validator 验收（2026-10-20 16:19）

证据目录 `/tmp/agenthub-validator-real/`（JSON + 空 err），机器所有者已授权。完整方法论见 [VALIDATION.md](VALIDATION.md)。

| 步骤 | 结果 |
|---|---|
| 真实 `~/.agenthub` v1→v2 | ✅ `PRAGMA user_version=2`；五张新表存在；v1 数据保留 |
| `health` | ✅ total=2、not_deployed=2（memory-maker + acceptance slug） |
| `health --agents --record` | ✅ 9 Agent；真实版本 ChatGPT/Codex/DSH/TRAE CN/WorkBuddy/ZCode；TRAE 国际/Claude/Cursor 无版本 |
| `skills audit memory-maker` | ✅ warn/medium、8 findings（3 medium/3 low/2 info；缺失引用/网络）；`audit-accept` 同结构 |
| `skills deps memory-maker` | ✅ healthy；node 24.19.0 ok；MCP unknown |
| `skills impact memory-maker` | ✅ medium、`skill_md_changed`、requiresRevalidation=true、0 installations |
| `skills validate memory-maker --agent codex --status passed` | ✅ 写入 `validation_runs`；健康仍 not_deployed（未部署优先） |

> 真实环境 2 个能力均未部署，所以 health 全为 not_deployed；人工证据写入不会把未部署能力变成 verified。

### 0.5 真实 Mac 适配器验收（2026-10-20 16:44）

证据 `/tmp/agenthub-adapters-real/`（agents/scan/agent-health JSON，err 为空），机器所有者已授权。

| 验证 | 结果 |
|---|---|
| `agents list` | ✅ **11 个适配器**；`qwen-work` = installed（`~/.qwenworkcn/skills` exists）、`doubao-work` = not_installed |
| `skills scan` | ✅ 真实 **396** 个技能（invalid 5、conflicts 7、duplicates 3）；Qwen Work **31** 个（create-skill 1.2.1、dingtalk-* 1.0.64.1、pptx 2.3.3、xlsx 1.0.2、pdf、media-*、qwenwork-guidance 等） |
| `health --agents --record` | ✅ 新增 **Qwen Work 1.3.1**；ChatGPT 26.1013.40934、Codex 26.623.70822、DSH 0.2.1-rc.2、TRAE CN 3.4.4、WorkBuddy 5.8.3、ZCode 3.16.2；Doubao/TRAE 国际/Claude/Cursor 无版本 |
| Qwen MCP | 路径 `~/.qwenworkcn/mcp.json` 本机不存在；写入/移除保真由 fixture 测试覆盖（未改真实配置） |
| Doubao | 本机未安装；workspace `.user_skills` 由 fixture + custom root 测试覆盖 |

---

### 0.6 真实 Mac 批量采纳验收（2026-10-20 17:32）

用户反馈「技能库始终只有两个，实际本机不同 Agent 安装的技能很多」；修复后真实机器执行 `skills adopt`，证据 `/tmp/agenthub-adopt-real/`（scan2/preview2/adopt JSON，err 为空）。

| 指标 | 值 |
|---|---|
| 重新扫描候选 | **396**（invalid 5、conflicts 7、duplicates 3） |
| `skills adopt --dry-run` | 导入预览 **112**；skippedReadOnly **273**、skippedDuplicate **11**、skippedExisting 0、failed 0 |
| `skills adopt --yes` | 实导 **112** 个（约 **55 MB**，`57,583,223` bytes），失败 **0** |
| 中央库 capability | **2 → 111**（112 条导入按版本共存形成 111 个 capability；versions 116、agent_dir sources 112） |
| 去重 | 同一 `(slug, contentHash)` 跨 Agent 只导一次；重复运行幂等 |
| 只读来源 | 默认跳过 builtin/marketplace/workspace/connector（本机 273 条） |
| 审计 | 写 `skills.adopt_scan` JSONL（不含文件内容） |

> 这次修复把「扫描发现」做成 Skills 页一等公民；真实数字证明中央库不再只有手动导入的 2 条。

---

---

## 1. 测试分层

| 层 | 范围 | 工具/位置 | 现状 |
|---|---|---|---|
| L1 单元 | audit/deps/impact/validation/agent_health、`util`、`skill`、`mcp` codec、前端 status/health/markdown/format/i18n | Rust `#[cfg(test)]` / vitest | ✅ core 44、前端 38 |
| L2 集成 | `AgentHub` 全链路 + validator/DB v2/健康证据 + 扩展 adapter + 扫描采纳 | `crates/agenthub-core/tests/` | ✅ 38 个全部通过（validator 11；adapters_extended 4；scan_adopt 2） |
| L3 CLI | 真实进程 + 隔离 home：旧命令 + `health`/`audit|deps|impact|validate` 退出码/JSON | `crates/agenthub-cli/tests/{cli,validator_cli}.rs` | ✅ 16 个全部通过（7 新） |
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
9. **验证平台测试**：审计/依赖测试只用临时 fixture 目录；`--version` 探测使用显式路径/测试 runtime，避免依赖宿主 PATH；不写真实 Keychain。

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

### 3.4b 扩展 Adapter（Qwen Work / Doubao Work）

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| A8 | 注册表包含 11 个 adapter | qwen-work / doubao-work 存在 | ✅ adapters_extended |
| A9 | Qwen 检测/扫描/插件 | installed；扫到 `~/.qwenworkcn/skills`；插件只读发现 | ✅ adapters_extended + 真实扫描 31 |
| A10 | Qwen MCP 写入/移除 | 保留无关键与既有 server；移除只删目标条目 | ✅ adapters_extended |
| A11 | Doubao 无全局根 | plan blocked；不猜测/创建全局目录 | ✅ adapters_extended |
| A12 | Doubao workspace 扫描 + 注册 custom root | 扫到 `.user_skills`；注册 `.doubao/skills` 后可部署 | ✅ adapters_extended |
| A13 | 真实 Mac Qwen 版本/技能 | 1.3.1；31 技能；health 记录 baseline | ✅ `/tmp/agenthub-adapters-real/` |
| A14 | 真实 Mac Doubao | not_installed；无版本 | ✅ `/tmp/agenthub-adapters-real/` |

### 3.5 验证平台（Validator / Health）

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| V1 | 静态审计：`curl|sh`/`sudo`/`rm -rf /`/`eval`/硬编码密钥/网络 | fail 或 warn；finding 有 id/severity/category/path；密钥原值不出现 | ✅ audit 单测 + validator 集成 + CLI |
| V2 | 审计干净技能 | pass/low；无严重 finding | ✅ validator |
| V3 | 缺失引用 / world-writable / symlink 逃逸 | 对应 finding；不跟随 symlink | ✅ audit 单测 |
| V4 | deps：缺 runtime | needs_attention；CLI 退出码 2 | ✅ deps 单测 + CLI |
| V5 | deps：requirements/pyproject/package.json | 解析声明；包状态 unknown | ✅ deps 单测 + 集成 |
| V6 | impact：SKILL.md/脚本/网络/依赖变化 | classes 正确；requiresRevalidation=true；plan warning | ✅ impact 单测 + 集成 |
| V7 | 健康：未部署 | not_deployed | ✅ validation 单测 |
| V8 | 健康：安装但无证据 | unverified（绝不 verified） | ✅ validation 单测 |
| V9 | 健康：audit fail + manual passed | needs_attention | ✅ validation 单测 + 集成 |
| V10 | 健康：所有安装 Agent manual passed | verified | ✅ validation 单测 + 集成 |
| V11 | 旧版本证据 | 不参与当前版本；旧 failed 忽略 | ✅ validation 单测 |
| V12 | DB v1→v2 | user_version=2、五表存在、旧数据保留 | ✅ validator 集成 |
| V13 | Agent 版本 baseline/变化/同秒排序 | 首次 false；变化 true；同秒 rowid 取最新 | ✅ agent_health 单测 + validator |
| V14 | 审计/操作/日志不含密钥原值 | 全链路断言 | ✅ validator `...never_store_raw_secrets` |

### 3.5b 扫描发现与批量采纳（0.4.0）

| ID | 场景 | 期望 | 覆盖 |
|---|---|---|---|
| D17 | 扫描发现分组 | 按 (slug, contentHash) 跨 Agent 合并；已在库/未入库/只读标记正确 | ✅ 前端 discovered.spec；真实 scan |
| D18 | 批量采纳默认跳过只读 | builtin/marketplace/workspace/connector 不计入 | ✅ scan_adopt |
| D19 | 去重与幂等 | 同内容多 Agent 一次；重复运行 skipped_existing/duplicate | ✅ scan_adopt |
| D20 | dry-run | 不写中央库，只返回 planned | ✅ scan_adopt + CLI |
| D21 | 按 Agent 过滤 | 只采纳指定 agent | ✅ scan_adopt |
| D22 | include-read-only | 显式勾选后才导入只读来源 | ✅ scan_adopt |
| D23 | 真实 396→112/111 | 失败 0、55MB、库能力 2→111 | ✅ `/tmp/agenthub-adopt-real/` |

### 3.6 可靠性

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

### 4.3 真实 Mac validator 验收（2026-10-20 16:19）

真实 `~/.agenthub`（用户已授权）执行：

- DB v1→v2 自动迁移，`PRAGMA user_version=2`，五张新表存在，v1 数据保留。
- `health` total=2 / not_deployed=2；`health --agents --record` 读出 6 个 Agent 真实版本（ChatGPT/Codex/DSH/TRAE CN/WorkBuddy/ZCode），其余显示未知。
- `skills audit memory-maker` = warn/medium、8 findings（3 medium/3 low/2 info：缺失引用、网络访问、无 manifest/无 license、脚本形态）。
- `skills deps memory-maker` = healthy（node 24.19.0 ok、MCP unknown）。
- `skills impact memory-maker` = medium、`skill_md_changed`、requiresRevalidation=true、0 installations。
- `skills validate memory-maker --agent codex --status passed` 写入成功；health 仍 not_deployed（未部署优先）。

证据：`/tmp/agenthub-validator-real/{agents,audit-memory,audit-accept,deps-memory,impact-memory,validate,health1,health2}.json`，err 均为空。完整方法论 [VALIDATION.md](VALIDATION.md)。

### 4.4 建议继续固化为自动测试的场景

1. MCP 回滚（现有测试主要覆盖 skill 回滚）。
2. `skills uninstall` / `skills sync` CLI 退出码与“dry-run 不写”断言。
3. operation/import 审计日志与 `installations.scope` / `scan_results.scope` 的数据库断言。
4. 外部漂移后的回滚 partial 行为。
5. 真实 Agent bundle 版本探测的 mac-only 集成（当前用 fake bundle + 单测）。
6. Health UI 的交互回归（当前只有 health/health-ui 纯函数/组件测试）。

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

### 5.2b 验证平台验收（隔离 home）

```bash
$BIN --home "$AGENTHUB_HOME" health
$BIN --home "$AGENTHUB_HOME" health --agents --record
$BIN --home "$AGENTHUB_HOME" skills audit demo
$BIN --home "$AGENTHUB_HOME" skills deps demo
$BIN --home "$AGENTHUB_HOME" skills impact demo
$BIN --home "$AGENTHUB_HOME" skills validate demo --agent codex --case "smoke" --status passed
```

判定：`health` 五个计数与 `health_overview` 一致；audit fail → exit 2；deps needs_attention → exit 2；validate 写入后 `validations_list` 可见；未部署能力仍 not_deployed。

### 5.3 安全与可靠性人工检查

1. 扫描前后对比真实 `~/.codex` 等目录 mtime：不得变化。
2. 计划生成后手工改目标内容 → apply 应报 `ExternalModification`。
3. 目标目录设为只读 → apply 失败且已成功项被恢复。
4. `doctor` 检查 library/database/data_dir/git；破坏 DB 后 `doctor` 应报 error。
5. 断网执行扫描/导入/分发（非 Git）→ 正常。
6. Keychain 未初始化时设置 secret → 错误可读（不泄露值）。
7. 含硬编码密钥的 fixture → audit fail，且 UI/操作/日志/DB 均不出现原值。
8. 缺 runtime 的脚本 → deps needs_attention，health 为 needs_attention（即使 manual passed）。
9. 同一秒两次 Agent 版本记录 → 最新一次被读取（rowid 决胜）。

### 5.4 GUI 验收（已完成一次）

隔离 home（`/tmp/agenthub-gui/home`）下人工验收通过：Dashboard 显示真实数据（Agent 6/9、技能 1、安装 2、MCP 1、插件 1、待更新 1、扫描 7 技能/重复 1/冲突 0/无效 0），onboarding 5 步第 1 步正常，中文界面与活动历史正常；第二轮新增 Dashboard 健康摘要与 `/health` 页（摘要/筛选/搜索/能力表），截图 [gui-health.png](evidence/gui-health.png)（Dashboard 健康区；health 页未自动点击时按 Lead 指示引用）。截图见 [ACCEPTANCE-REPORT.md §3](ACCEPTANCE-REPORT.md)。仍需补自动化回归（Playwright/XCUITest）与更多页面深度操作。

### 5.5 真实 Mac 验收复跑

```bash
# 需机器所有者授权；会写真实 ~/.agenthub / ~/.codex/skills / ~/.zcode/skills
bash scripts/real-acceptance.sh
```

---

## 6. 未覆盖项 / 风险

| 项 | 风险 | 建议 |
|---|---|---|
| GUI 自动化 | 已人工运行验收（含 Health/Dashboard 健康摘要），但无 Tauri 自动化回归 | 补 Playwright/XCUITest 或稳定的截图对比 |
| 签名 / 公证 | DMG 已产出；未签名/未公证 | 配置 Developer ID + notarization；验证 Gatekeeper |
| Keychain 正向路径 | 未测（避免污染真实 Keychain） | 使用专用测试 service 前缀 + 清理，或标记 manual-only |
| 并发/锁 | 未测 | 两进程同时 apply 的集成测试 |
| 中断恢复 | 未测 | 在 apply 中注入失败/杀进程，验证快照与 DB 状态 |
| 备份恢复 | 未实现 | 先设计恢复流程再测 |
| `/Applications` 探测导致的不确定性 | 测试可能看到真实 app | 增加可覆盖 app root 的测试钩子 |
| 第三方 skill 安全审查 | 无静态分析 | 后续增加脚本/`allowed-tools`/危险命令扫描 |
| 非 macOS 平台 | 未验证 | 明确 MVP 不支持 |
| 静态审计漏报/误报 | 规则启发式，无对抗样本全量评估 | 规则 fixture + 误报反馈；明确非完备证明 |
| 人工证据可信度 | notes/evidence 用户输入 | 不作为安全认证；后续可加 hash/附件 |
| Agent 版本探测异常 | 非标准 bundle/CLI | 显示未知；真实多版本回归 |
| 自动 E2E（封闭 Agent） | 无接口 | 保持 manual evidence；不伪造自动通过 |
| MCP 真实配置写入 | 验收未写入真实 MCP 配置 | 用隔离 home/config 或单独授权的临时目标测试 |
| 真实 profile 数据噪声 | 扫描 367 技能出现既有 conflicts/duplicates/invalid | 验收/演示时用隔离 home；真实 profile 显示为「需关注」而非缺陷 |
| 审计日志/scope 断言 | 有实现与真实验收，但无自动断言 | 增加 DB/日志断言测试 |

---

## 6.5 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：core **82**（44 unit + 38 integration）、CLI **16**、前端 **38**（Rust 合计 98）全部通过；`pnpm build`、完整 `pnpm tauri build`（.app + .dmg）通过；第一轮真实 Mac 安装/分发/更新/回滚；第二轮真实 Mac validator（DB v1→v2、Agent 版本、audit/deps/impact/validate）；GUI（隔离 home，含健康摘要）人工验收通过。
- **未验证**：Qwen symlink/热加载/MCP 真实生效方式；Doubao 全部（本机未安装）；签名/公证（0.4.0 DMG 已产出）、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断/磁盘满、备份恢复、非 macOS。
- **已知限制**：静态审计是启发式；人工证据是用户输入；MCP/凭据静态 unknown；前端无 Tauri 运行时测试；部分场景仅手工（见 §6）；测试受 `/Applications` 真实 app 影响。

## 7. 交叉链接

- 功能验收场景：[PRD.md §6](PRD.md)
- 写入事务与快照机制：[ARCHITECTURE.md §3](ARCHITECTURE.md)
- 安全测试关注点：[SECURITY.md §7](SECURITY.md)
- 适配器证据与路径：[ADAPTER-SPEC.md](ADAPTER-SPEC.md)
