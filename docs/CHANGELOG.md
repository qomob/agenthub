# 更新日志

本项目遵循「没有验证过的能力不写成已提供」的原则；版本号从 `0.1.0` 起。

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

### Known issues

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

### Verification status

- `./scripts/cargo.sh test -p agenthub-core -p agenthub-cli`：**exit 0**；core 21 + CLI 9（共 30）全部通过。
- `pnpm test`（vitest）：**5 文件 / 21 测试全部通过**。
- `pnpm build`：**通过**（`vue-tsc --noEmit` + Vite build）。
- 完整 `pnpm tauri build`：**通过**；产出 `AgentHub.app`（7.14 MiB）与 `AgentHub_0.1.0_aarch64.dmg`（3.77 MiB，15:59）；`/Applications/AgentHub.app` 已覆盖安装。首次 DMG 尝试沙箱 hdiutil 失败，完整权限重跑成功。
- **真实 Mac 验收**：导入 → 双 Agent copy 分发 → 扫描 inSync → v2 更新 → 回滚精确 v1 → 卸载（库保留 2 版本）→ backup/doctor 全通过；见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)。
- **GUI 验收**：隔离 home 下 Dashboard/onboarding 正常（截图 `docs/evidence/`）。
- 本文档中的「已验证」仅指本次真实执行/测试/验收；未验证项见状态标记说明。

## 状态标记说明

- **已验证**：代码路径 + 真实命令/测试输出（core 21、CLI 9、前端 21、`pnpm build`、`pnpm tauri build --bundles app`、真实 Mac 验收、GUI 运行验收）。
- **未验证**：签名/公证（DMG 已产出）、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断、非 macOS、部分 adapter 路径。
- **已知限制**：见上文 Known issues（headers 明文、secret argv、备份/快照含凭据、备份恢复、项目 scope、签名/公证等）。
