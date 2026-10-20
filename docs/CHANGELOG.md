# 更新日志

本项目遵循「没有验证过的能力不写成已提供」的原则；版本号从 `0.1.0` 起。

## 0.3.0 — 2026-10-20

新增两个 Agent 适配器（Qwen Work / Doubao Work），适配器总数 **9 → 11**；新增 4 个集成测试与真实 Mac 适配器验收。

> 产物状态：**0.3.0 `.app`（7.50 MiB）与 `AgentHub_0.3.0_aarch64.dmg`（3.96 MiB）已于 2026-10-20 16:47–16:48 构建**，`/Applications/AgentHub.app` 已覆盖安装 0.3.0。签名/公证未做。

### Added

- **Qwen Work（千问工作，`qwen-work` / `builtin.qwen-work`，Alibaba）**：
  - 检测 `/Applications/QwenWorkCN.app`（也支持 Qwen Work.app / QwenWork.app）或 `~/.qwenworkcn`；本机 **VERIFIED**（版本 1.3.1）。
  - 技能根 `~/.qwenworkcn/skills`（本机真实 31 个技能：create-skill/dingtalk-*/pptx/xlsx/pdf/media-*/qwenwork-guidance 等）；无已验证项目级根。
  - 插件目录常量 `~/.qwenworkcn/plugins`、`~/.qwenworkcn/plugins-custom`（目录可不存在；manifest `.codex-plugin/plugin.json`），只读发现。
  - 自定义 MCP `~/.qwenworkcn/mcp.json`（JSON `mcpServers`；stdio command/args/env，HTTP/streamable-http url/type）；路径来自 app.asar 常量 `CUSTOM_MCP_CONFIG_PATH=constants.r("mcp.json")`，写入/移除保真由 `adapters_extended.rs` fixture 验证。
  - symlink/热加载/MCP 真实生效方式 **VR**；无 CLI。
- **Doubao Work（豆包工作，`doubao-work` / `builtin.doubao-work`，ByteDance）**：
  - 检测候选 app 名 + `~/Library/Application Support/Doubao|豆包`、`~/.doubao`；本机 **NOT_INSTALLED**（只有豆包浏览器），存在时状态 **VerificationRequired**。
  - 无已验证全局技能根；workspace 本地 `.user_skills`（社区指南）与 `.doubao/skills`（代码识别）；仅当用户注册 workspace 为 custom root 后可分发，否则 plan **blocked**。
  - MCP/插件 **manual-only**：应用 UI「技能 · 连接器 · 伙伴」新建连接器/由 AI 安装；AgentHub 不写任何本地 MCP/插件文件。
  - symlink/热加载 **VR**；参考官网/社区指南见 `AGENT-COMPATIBILITY.md §3.10`。
- **测试**：新增 `crates/agenthub-core/tests/adapters_extended.rs` 4 个用例（注册表含 11、Qwen 检测/扫描/插件发现、Qwen MCP 写入/移除保真、Doubao workspace 扫描/无根 blocked/注册 custom root 后部署）。
- **真实 Mac 适配器验收**：`agents list` 11 个（Qwen installed / Doubao not_installed）；`skills scan` 396 技能（invalid 5、conflicts 7），Qwen 31 个；`health --agents --record` 新增 Qwen 1.3.1。证据 `/tmp/agenthub-adapters-real/`。
- 文档：AGENT-COMPATIBILITY / ADAPTER-SPEC / README / PRD / ROADMAP / TEST-PLAN 增加 Qwen/Doubao。

### Changed

- 适配器总数 9 → 11；UI Agents 列表自动显示，无需新页面。
- README 支持 Agent 表新增 Qwen Work（本机 VERIFIED）与 Doubao Work（NOT_INSTALLED/VR）。
- 版本 bump 到 **0.3.0**（Cargo.toml/package.json/tauri.conf.json）。

### Fixed

- 本轮无代码缺陷修复；新增测试覆盖两个 adapter 的检测/扫描/MCP/项目根行为。

### Known issues

| 严重度 | 问题 | 参考 |
|---|---|---|
| 中 | Qwen Work：symlink 行为、热加载、`mcp.json` 改动生效方式均 VR；MCP 写入只在 fixture 验证，未改真实配置 | AGENT-COMPATIBILITY §3.9 |
| 中 | Doubao Work 本机未安装；workspace `.user_skills` 仅社区指南；MCP/插件 manual-only；无全局安装根 | AGENT-COMPATIBILITY §3.10 |
| 中 | 未签名/未公证（0.3.0 DMG 已产出并安装） | 本文件版本状态 |
| 低 | Qwen 插件目录本机不存在（代码常量）；Doubao 无版本来源 | ADAPTER-SPEC §3.5 |

### Verification status

- `./scripts/cargo.sh test -p agenthub-core`：**80 passed / 0 failed**（44 unit + 36 integration；含 adapters_extended 4）。
- `./scripts/cargo.sh test -p agenthub-cli`：**16 passed / 0 failed**；Rust 合计 **96 passed**。
- `pnpm test`：**7 files / 29 tests passed**；`pnpm build` 通过。
- 真实 Mac 适配器证据：`/tmp/agenthub-adapters-real/`（agents/scan/agent-health JSON，err 为空）。
- 未验证：Qwen symlink/热加载/MCP 生效方式；Doubao 全部（本机未安装）；签名/公证。

---

## 0.2.0 — 2026-10-20

产品定位升级为 **AgentHub · Capability Control Center**。第二轮落地 **Capability Validation Platform**：DB v2、静态审计、依赖/环境诊断、更新影响、人工验证证据、Agent Health Monitor、CLI/UI 新命令与页面。安装/同步继续复用第一轮事务引擎，不重复实现。

> 版本状态：`Cargo.toml`/`package.json`/`tauri.conf.json` 已 bump 到 **0.2.0**；0.2.0 `.app`（7.49 MiB）与 `AgentHub_0.2.0_aarch64.dmg`（3.95 MiB）已于 16:33–16:34 构建，`/Applications/AgentHub.app` 已覆盖安装 0.2.0。签名/公证未做；旧 0.1.0 DMG 仍在 bundle 目录（可清理，产物以 0.2.0 为准）。

### Added

- **Capability Validator**（`audit.rs`）：确定性静态审计；spec/security/quality/dependency 规则；finding（id/severity/category/title/detail/path/suggestion）、status `pass|warn|fail`、risk `low..critical`；不执行脚本、不联网、无 AI 评分；凭据值脱敏。
- **依赖与环境诊断**（`deps.rs`）：shebang runtime 真实探测（`--version`，2s 超时）、`requirements.txt`/`pyproject.toml`/`package.json` 解析、Python/Node 兼容性提示；MCP/凭据静态 `unknown`；status `healthy|needs_attention|unknown`。
- **更新影响分析**（`impact.rs`）：版本文件 diff（最多 500 条 + 真实总数）、classes（`docs_only`/`skill_md_changed`/`new_script`/`removed_script`/`script_changed`/`new_network_domain`/`removed_network_domain`/`new_dependency`/`removed_dependency`/`new_binary`/`files_added`/`files_removed`/`large_change`）、风险、`requiresRevalidation`；写入 `update_impacts`；分发计划 warnings 显示。
- **人工验证证据**（`validation_runs`）：agent/case/status/notes/evidence/kind；明确「封闭 Agent 无法自动 E2E，这是人工证据不是自动通过」；只有 manual passed 可贡献 verified。
- **健康状态机**（`validation.rs`）：`NotDeployed` / `Failed` / `NeedsAttention` / `Verified` / `Unverified`；安装≠验证；audit fail 或 deps needs_attention 优先；只有当前版本全部安装 Agent manual passed 才 verified；旧版本证据失效。
- **Agent Health Monitor**（`agent_health.rs` + `agent_versions`）：读取 `.app` 的 `CFBundleShortVersionString`（plutil→XML→mdls）与 CLI `--version`；首次 baseline；版本变化提示重新扫描/验证；同秒用 rowid 决胜。
- **DB v2**：新增 `capability_audits`、`dependency_reports`、`update_impacts`、`validation_runs`、`agent_versions`；v1 自动升级、幂等；真实 `~/.agenthub` 已验证。
- **Tauri 命令（47，+9）**：`skills_audit`、`skills_dependencies`、`skills_audit_history`、`skills_preview_update`、`skills_health`、`health_overview`、`validations_list`、`validations_record`、`agent_health`。
- **CLI**：`health`（`--agents`/`--record`）、`skills audit|deps|impact|validate`；audit fail → 退出码 2，deps needs_attention → 2。
- **UI**：`/health` 页（摘要/筛选/搜索/能力表/重新检查）；SkillDetail 四区块（安全审计/依赖与环境/更新影响/验证证据）；Agents 版本健康；Dashboard 健康摘要与 Skills 健康列；分发预览 `UpdateImpactSummary` 与「需重新验证」提示。
- **测试**：core unit 44、integration validator 11；CLI validator 7；前端 health/health-ui 测试；same-second 排序回归。
- 文档：新增 [VALIDATION.md](VALIDATION.md)；更新 README/PRD/ARCHITECTURE/DATA-MODEL/ADAPTER-SPEC/SECURITY/UI-SPEC/TEST-PLAN/ROADMAP；截图 `docs/evidence/gui-health.png`。

### Changed

- 产品定位：能力管理器 → **Capability Control Center**；README/PRD 增加验证工作流「导入 → 审计 → 依赖 → 分发（含影响）→ 验证证据 → 更新 → 回滚」。
- `service.rs` 分发计划在版本更新时自动计算 impact，并把摘要/`requiresRevalidation` 写入 plan warnings。
- `installations.scope` 已记录（global/custom）；`scan_results.scope` 已写入；import 已写 operations + JSONL 审计（第一轮末尾）。
- MCP env 不再进入 `operations.plan`（第一轮末尾修复，0.2 沿用）。
- Agent 版本观测首次为 baseline；版本变化只提示重新扫描/验证，不自动阻断。

### Fixed

- **same-second Agent 版本排序**（QA 发现）：`agent_versions` 读取/列表改为 `detected_at DESC, rowid DESC`；同秒多次写入取最新一条，回归用例已启用。
- **人工 passed 不能覆盖 audit fail**：健康推导安全优先，`manual_pass_cannot_override_a_failed_static_audit` 回归。
- **静态审计密钥泄露风险**：finding/操作/日志不写原值（redacted），`...never_store_raw_secrets` 回归。
- **DB v1→v2**：旧库自动建新表并保留数据（`v2_migration_rebuilds_tables_and_preserves_v1_data`）。
- 第一轮遗留（MCP preflight/CLI dry-run/DMG/安装）保持修复状态。

### Known issues

| 严重度 | 问题 | 参考 |
|---|---|---|
| 中 | 静态审计是启发式规则，可能漏报/误报；不是完备安全证明 | VALIDATION §10 |
| 中 | 人工证据 notes/evidence 是用户输入，不做真实性/防伪证明 | VALIDATION §6 |
| 中 | MCP/凭据依赖仍为静态 `unknown`；未试连/未验证凭据有效性 | SECURITY R15 |
| 中 | Agent 版本探测依赖 app bundle/CLI 输出；非标准打包可能 unknown | ADAPTER-SPEC §3.5 |
| 中 | MCP headers 仍明文；`security -w` argv；快照/备份含配置 | SECURITY R2-R4 |
| 中 | 未签名/未公证（0.2.0 DMG 已产出并安装） | ROADMAP R11 |
| 低 | 项目级安装仍按 custom root 处理；`target_within_roots` 未接入；备份恢复未实现 | ARCHITECTURE §9 |
| 低 | 无 Tauri GUI 自动化（Health/分发点击流）；仅人工截图 | TEST-PLAN §6 |

### Verification status

- 0.2.0 产物：`AgentHub.app`（7.49 MiB）与 `AgentHub_0.2.0_aarch64.dmg`（3.95 MiB）已于 2026-10-20 16:33–16:34 构建；`/Applications/AgentHub.app` 已覆盖安装 0.2.0。签名/公证未做。
- `./scripts/cargo.sh test -p agenthub-core`：**76 passed / 0 failed**（44 unit + 32 integration，validator 11）。
- `./scripts/cargo.sh test -p agenthub-cli`：**16 passed / 0 failed**。
- Rust 合计 **92 passed / 0 failed**；`./scripts/cargo.sh check -p agenthub` exit 0。
- `pnpm test`：**7 files / 29 tests passed**；`pnpm build` 通过。
- 真实 Mac validator：DB v1→v2、Agent 版本、audit/deps/impact/validate；证据 `/tmp/agenthub-validator-real/`。
- GUI（隔离 home）：Dashboard 健康摘要/onboarding/中文界面/活动历史；截图 `docs/evidence/`。
- 未验证：签名/公证、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断/磁盘满、非 macOS。

---

## 0.1.0 — 2026-10-20

首个可运行版本：本机多 Agent 能力管理核心、CLI、Tauri 命令层、GUI、测试基线与文档。已完成真实 Mac 验收与 GUI 运行验收；MCP 正常路径可用；DMG 已产出，headers 凭据化与签名/公证仍未完成。

### Added

**核心引擎（`agenthub-core`）**

- Agent 适配器框架 `AgentAdapter` + 9 个内置适配器：Codex、ChatGPT、TRAE (International)、TRAE CN、ZCode、WorkBuddy、DeepSeek Harness、Claude Code、Cursor（`adapters/mod.rs`、`adapters/builtin.rs`）。
- 只读扫描：全局/项目/自定义根，`SKILL.md` 解析、内容 hash、`scan_results` 缓存、duplicates/conflicts/invalid 统计（`scan.rs`、`skill.rs`）。
- 技能导入：文件夹、单 `SKILL.md`、zip（路径穿越/symlink/大小/条目数防护）、Git 浅克隆、Agent 目录；内容去重与版本化（`service.rs:614`、`skill.rs:373`、`skill.rs:462`）。
- 中央库：`library/skills/<slug>/<hash12>` 不可变版本、staging、snapshots、backups、logs 目录布局（`library.rs`）。
- 事务化分发：计划（create/replace/noop/conflict/blocked）→ 预检 → 快照 → copy/symlink 写入 → hash 校验 → 记录；失败逆序补偿（`ops.rs:79`）。
- 回滚：基于快照与 `previous_installation`，限制 success/partial、一次性、拒绝覆盖外部修改（`ops.rs:717`）。
- SQLite 持久化：SCHEMA_V1（13 张表）、`PRAGMA user_version` 迁移、WAL/外键/WAL busy timeout（`db.rs:14`、`db.rs:444`）。
- MCP：定义 CRUD、从 Agent 配置只读发现、Keychain env 值、分发/移除计划与写入、多格式 codec（`json:mcpServers`、`json:mcp.servers`、`json:<path>`、`toml:mcp_servers`）（`mcp.rs`）。
- Profiles：4 个内置空预设 + 自定义 CRUD + 应用计划（`service.rs:1702`、`service.rs:1775`）。
- 插件只读发现（Codex/TRAE CN/WorkBuddy）与 `plugin_records` 同步（`service.rs:1628`）。
- 诊断 `doctor()`、Dashboard 聚合、`check_changes()` 漂移/缺失/更新检测、完整备份（VACUUM INTO + manifest）（`service.rs:1371`、`service.rs:1422`、`service.rs:1523`、`library.rs:122`）。
- 安全工具：目录哈希（逃逸 symlink 拒绝）、`symlink_metadata` 删除、原子写、staging + rename、路径规范化（`util.rs`）。

**CLI（`agenthub`）**

- 全局 `--home <dir>`（等价 `AGENTHUB_HOME`）与 `--json`。
- 命令：`doctor`、`agents list`、`skills list|scan|import|install|uninstall|sync|rollback`、`mcp list|discover|distribute`、`operations list`、`profiles list`、`backup create`、`backups list`。
- `skills uninstall <installation-id...>`：按安装 id 卸载，支持 `--dry-run/--yes/--force`。
- 变更门禁：默认只打印计划，`--yes` 才执行；`--dry-run` 只预览；blocked/conflict 返回退出码 2（`agenthub-cli/src/main.rs:531-565`）。
- 退出码：0 成功、1 失败、2 部分成功/需确认。

**Tauri 命令层**

- 38 个命令覆盖 bootstrap/settings/dashboard/doctor/agents/scan/changes/skills/operations/snapshots/backups/MCP/plugins/profiles（`src-tauri/src/lib.rs`）。
- 单例 `AppState`，初始化失败返回可读错误。

**前端基础（`src/`）**

- 设计 token 与组件样式：明暗主题、按钮/输入/卡片/表格/徽章/模态/toast/矩阵/骨架/动效/reduced-motion（`src/styles.css`）。
- i18n：zh/en 全量页面文案（`src/i18n/`），默认 zh。
- Tauri API 类型封装（`src/lib/api.ts`，38 命令）、状态/色调解算（`src/lib/status.ts`）、安全 Markdown 块解析（`src/lib/markdown.ts`，无 `v-html`）。
- Pinia stores：app/ui/skills/operations（主题/语言/toast/dashboard/scan/回滚/备份/doctor/changes）。
- Vite 配置（端口 1420、vitest jsdom、`src/**/*.spec.ts`）。
- 页面/路由/组件：`App.vue`、`main.ts`、`router.ts`、11 个视图（含 Skill 详情、Matrix、Onboarding）与 17 个组件；`pnpm build` 通过。
- 隔离 home 下 GUI 运行验收通过（Dashboard/onboarding/中文界面/活动历史）；截图见 `docs/evidence/`。

**测试**

- `agenthub-core/tests/`：lifecycle、scan_import、distribution、conflict、mcp、security_zip、git_import，共 21 个集成测试。
- `agenthub-cli/tests/cli.rs`：9 个真实进程端到端测试。
- `src/**/*.spec.ts`：5 个前端测试文件 / 21 个测试（status、markdown、format、i18n、app-smoke）。
- 共同 fixture 使用临时 home，不触碰真实 Agent 目录（`tests/common/mod.rs`）。
- 真实 Mac 验收脚本 `scripts/real-acceptance.sh`（唯一 slug、目标不存在才执行、失败清理）。

**文档**

- 新增 `README.md`、`docs/PRD.md`、`ARCHITECTURE.md`、`DATA-MODEL.md`、`ADAPTER-SPEC.md`、`SECURITY.md`、`UI-SPEC.md`、`TEST-PLAN.md`、`ROADMAP.md`、`CHANGELOG.md`、`ACCEPTANCE-REPORT.md`。
- 交叉链接现有研究：`REFERENCE-STUDY.md`、`AGENT-COMPATIBILITY.md`、`compat-evidence/`。

### Changed

- 扫描冲突语义：从「同 hash 跨 Agent 计 conflict」改为「同 slug 不同 hash 计 conflict；同内容跨 Agent 计 duplicate（正常分发）」（`scan.rs:202-226`）。
- TRAE CN 扫描根补齐 peer 目录 `.trae/skills` 与共享根 `.agents/skills`；新增全局 MCP 目标 `~/Library/Application Support/Trae CN/User/mcp.json`（`builtin.rs:373-435`）。
- Codex 扫描根补充 `.agents/skills`；安装根仍为 `~/.codex/skills`（`builtin.rs:127-137`）。
- DSH `symlink_support` 从 `Unverified` 改为 `Supported`（本机证据；`builtin.rs:725-729`）。
- MCP 写回：`write_entries` 会清理不在目标列表中的 stale key（JSON `retain`、TOML `table.remove`），`remove_entry` 真正生效（`mcp.rs:203-300`）。
- MCP 执行路径接通：`Executor::apply` 的 kind 白名单加入 `McpDistribute`/`McpRemove`，`apply_item` 允许 `mcp_config`（`ops.rs:80-90`、`ops.rs:314-320`）。
- 预检/执行对 blocked 项改为「跳过并记录 skipped」，整体 `partial`（`ops.rs:113-132`、`ops.rs:200-292`）；CLI 仍在门禁处拒绝含 blocked 的计划。
- CLI 从 3 行占位升级为完整薄前端（约 1324 行），与 GUI 共享同一个 `AgentHub`；新增 `skills uninstall`。
- MCP 计划 payload 剥离 env 值（`mcp.rs:883-890`），执行时从 Keychain 重建完整 entry（`mcp.rs:1111-1122`）；`operations.plan` 不再含 env 明文。
- `installations.scope` 记录规划 scope（global/custom；项目根按 custom 处理），`scan_results.scope` 写入 global/project/custom。
- 导入成功后写入 `kind=import` 的 `operations` 行，并追加审计日志。
- 新增 `<library>/logs/operations.log` JSONL 审计：`operation.start/finish/failed` 与 `import`（不记录 secret）。
- Tauri 38 命令新增/变化字段：`AgentView.customSkillRoots`、`McpServerView.headers`，UI 已使用。
- release `AgentHub.app`（7.14 MiB）与 `AgentHub_0.1.0_aarch64.dmg`（3.77 MiB，2026-10-20 15:59）构建通过；`/Applications/AgentHub.app` 已用该构建覆盖安装（0.1.0，`com.agenthub.desktop`）。首次 DMG 尝试因沙箱 hdiutil 权限失败，完整权限重跑成功。

### Fixed

- MCP 移除此前只 upsert、不删除条目（no-op）：`write_entries` 现清理 stale key，临时 harness 与集成测试均验证 TOML/JSON 删除生效。
- MCP preflight 对 `mcp_config` 改为按条目 hash 比较，并在移除计划中记录当前条目 hash；已有配置文件不再需要 `--force`（回归测试通过）。
- CLI `gate_plan` 先处理 `--dry-run` 再判断 `--yes`；`--dry-run` 现在按计划内容返回 0/2。
- 前端补齐 `SettingsView.vue` 后 `pnpm build` 通过（`vue-tsc --noEmit` + Vite）。
- MCP 分发/移除此前被 `OperationKind` 白名单与 `install_kind` 双重拒绝：现已接通。
- `scan_results` 刷新语义、同内容跨 Agent 误报 conflict 的问题；`scan_results.scope` 现在写入。
- 导入/扫描审计空白：导入写 operations；操作生命周期写 JSONL 审计。
- zip 绝对路径/UNC/盘符条目现在在 `safe_relative_path` 直接拒绝。
- DSH symlink 能力声明与本机证据不一致。
- 原子写失败时的回滚路径（临时文件/备份文件清理）与 symlink 删除不跟随链接。

### Known issues（0.1.0 历史；其中 env plan/log/import/GUI 等已在 0.2.0 修复，当前以 0.2.0 为准）

| 严重度 | 问题 | 参考 |
|---|---|---|
| 中 | CLI `--dry-run` 的退出码语义已修复，但「同时给 `--dry-run` 与 `--yes`」的行为仍需在帮助文档中说明 | `agenthub-cli/src/main.rs:538-546` |
| 高 | MCP headers 明文入库并写入配置 | SECURITY R2 |
| 中 | `security` CLI 的 `-w <value>` 会让 secret 短暂出现在进程 argv | SECURITY R3 |
| 中 | 快照/备份包含 Agent 配置副本，可能含 token；未加密 | SECURITY R4 |
| 中 | `target_within_roots()` 未接入 preflight | SECURITY R6 |
| 中 | `VerificationRequired`（Claude Code/Cursor）仍允许安装 | PRD E / SECURITY R7 |
| 中 | GUI 已人工运行验收，但无自动化 GUI 回归 | UI-SPEC §10、TEST-PLAN §6 |
| 中 | 扫描仍不写 operations（只刷新 scan_results） | DATA-MODEL §8 |
| 中 | 项目级安装仍按 custom root 处理，无独立 project scope 流程 | ARCHITECTURE §9 |
| 低 | 完整备份无恢复路径 | ROADMAP Phase 3 |
| 低 | MCP 删除后 TOML 留空 `[mcp_servers]` 表、JSON 留空对象 | ADAPTER-SPEC §8 |
| 低 | Codex 项目级根、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders` 仍未验证 | ADAPTER-SPEC §7 |
| 中 | DMG 已产出，但未签名/未公证 | ROADMAP R11、ACCEPTANCE-REPORT |
| 低 | `doctor().home` 实际返回 data_dir（历史命名） | DATA-MODEL / ACCEPTANCE-REPORT |

### Verification status（第一轮基线，历史；当前见 0.2.0）

- `./scripts/cargo.sh test -p agenthub-core -p agenthub-cli`：**exit 0**；core 21 + CLI 9（共 30）全部通过。
- `pnpm test`（vitest）：**5 文件 / 21 测试全部通过**。
- `pnpm build`：**通过**（`vue-tsc --noEmit` + Vite build）。
- 完整 `pnpm tauri build`：**通过**；产出 `AgentHub.app`（7.14 MiB）与 `AgentHub_0.1.0_aarch64.dmg`（3.77 MiB，15:59）；`/Applications/AgentHub.app` 已覆盖安装。首次 DMG 尝试沙箱 hdiutil 失败，完整权限重跑成功。
- **真实 Mac 验收**：导入 → 双 Agent copy 分发 → 扫描 inSync → v2 更新 → 回滚精确 v1 → 卸载（库保留 2 版本）→ backup/doctor 全通过；见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)。
- **GUI 验收**：隔离 home 下 Dashboard/onboarding 正常（截图 `docs/evidence/`）。
- 本文档中的「已验证」仅指本次真实执行/测试/验收；未验证项见状态标记说明。

## 状态标记说明

- **已验证**：代码路径 + 真实命令/测试输出（core 80、CLI 16、前端 29、`pnpm build`、完整 `pnpm tauri build`、第一轮真实 Mac 安装验收、第二轮真实 Mac validator 验收、第三轮真实 adapter 验收、GUI 运行验收）。
- **未验证**：签名/公证（DMG 已产出）、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断、非 macOS、部分 adapter 路径。
- **已知限制**：见上文 Known issues（静态审计启发式、人工证据用户输入、headers 明文、secret argv、备份/快照含凭据、备份恢复、项目 scope、签名/公证等）。
