# M0 Baseline Audit — My AI Stack / Cost Optimizer 前置审计

> 审计日期：2026-11-11 · 基准：`master@b5ef694`（tag `v0.5.0`）· 方法：4 个只读审计通道（核心引擎 / CLI-Tauri-Vue / 文档 / 安全-脚本-git）+ Lead 抽查复核。
> 本文档只陈述已验证的仓库事实；每一节标注证据来源（文件:行号 或命令）。标注「实测」的数字为本次审计直接验证，非文档转抄。

---

## 1. Git / 版本 / 安全修复状态（任务 A 第 1 项）

| 事实 | 值 | 证据 |
|---|---|---|
| HEAD | `b5ef694`（"chore: remove landing site from repository (local-only)"） | `git rev-parse HEAD` |
| 分支 | `master` | `git branch --show-current` |
| tag | **仅 `v0.5.0`**（指向 b5ef694）；无 v0.5.1 tag | `git tag -l` |
| `v0.5.0..HEAD` 提交 | **空**（HEAD 即 tag 指向的提交） | `git log --oneline v0.5.0..HEAD` |
| 工作区未跟踪 | `.workbuddy/`、`docs/promotion/`（本次未触碰） | `git status --porcelain` |
| 版本号三处 | `Cargo.toml:6`、`package.json:4`、`src-tauri/tauri.conf.json:4` 均为 **0.5.0** | grep 实测 |

### 1.1 v0.5.1 安全修复：真实状态为「已发布于 v0.5.0 tag 内」

- 修复提交 `58ef234`（2026-10-29，"fix(core): harden runtime lock, cross-process mutex coverage and diagnostics redaction"）是 HEAD 的祖先（`git merge-base --is-ancestor` 验证），即**已随 v0.5.0 一起打 tag 并发布 GitHub Release**（PROJECT-HANDOVER 记录 DMG sha256 `cd2533…68ac`）。
- 改动 20 文件 +1711/−34，新增 `runtime.rs`（158 行）、`redact.rs`（194 行）、`tests/runtime_lock.rs`、`tests/safety_fixes.rs`。
- **版本错位**：`docs/CHANGELOG.md:5` 把这些修复记为「0.5.1 — 2026-10-29（生产安全修复切片，未发布）」，但代码已在 v0.5.0 tag 内发布。即 "0.5.1" 只是 CHANGELOG 文档切片，没有对应版本号或 tag。
- **对本次开发的影响**：
  1. 不构成 M1/M2 的技术阻断——修复代码本身已生效。
  2. 但在发布任何包含新模块的版本前，必须先由负责人裁决版本号序列（推荐：把 CHANGELOG 0.5.1 条目并入 0.5.0 的描述修正，或直接发 0.5.1 tag；版本号需同步 bump 三处文件）。这属于负责人决策项，不由本方案擅自处理。
  3. 交接文档「先发布 0.5.1」的指示与 tag 事实冲突，已在 §7 不一致清单登记。
- **D-3 裁定结果（M0-R 冻结，2026-11-11）**：批准方案 A——修复归档至 0.5.0（`docs/CHANGELOG.md` 原 0.5.1 节已改题为「0.5.0（生产安全修复切片）— 随 v0.5.0 tag 一并发布，未单独发版」），不补造 0.5.1 Release、不移动既有 tag；My AI Stack 后续版本按 **0.6.0** 规划，发布需单独授权。

## 2. 核心引擎事实（`crates/agenthub-core`）

### 2.1 服务与双前端共用

- `pub struct AgentHub { config: CoreConfig, db: Database, write_lock: Mutex<()> }`（`crates/agenthub-core/src/service.rs:86-91`）；`write_lock` 为进程内串行写锁。
- 构造：`AgentHub::open(config)`（service.rs:94-108）→ create_dir_all → `Database::open` → `seed_defaults()`（service.rs:135-153）→ `library()?.init()` → `sync_agents()` → `seed_profiles()`。
- CLI 侧：`crates/agenthub-cli/src/main.rs:667-673` `open_hub()` → `Ctx { hub, json }`；CLI 是**单文件** `main.rs`（2209 行），29 个 `cmd_*` 函数，clap 子命令树 `Commands` 枚举（main.rs:70-116）：doctor / diagnostics / agents{list,inspect} / skills{list,scan,import,install,uninstall,sync,rollback,audit,deps,impact,validate,adopt} / mcp{list,discover,distribute} / operations{list,preview,approve,apply,recover} / profiles{list} / health / backup{create,restore} / backups{list}。退出码契约 main.rs:37-46（0/1/2/3/4/5）。
- Tauri 侧：`src-tauri/src/lib.rs:14-17` `AppState { hub: Mutex<Option<AgentHub>>, init_error }`；执行器 `run<T>`（legacy `Result<T,String>`，lib.rs:41-51）与 `run_api<T>`（`ApiResult<T>` v3.1 信封，lib.rs:56-79）。命令薄壳实测 **55 个** `#[tauri::command]`（grep 实测；AGENTS.md 写"约 48"已过时），`generate_handler!` 清单 lib.rs:557-613。
- **错误契约双轨并存**：旧命令走 `Result<T,String>`，v3.1 命令（operations_preview/approve/apply/recover、agents_inspect、backups_restore）走 `ApiResult<T>`。PRODUCTION-SAFETY 把统一列为遗留项 R4。**新增模块命令应直接走 ApiResult 信封**（docs/adr/0001 :9 既定策略）。

### 2.2 SQLite schema / 迁移 / 备份

- PRAGMA（`db.rs:610-616`）：`busy_timeout 5s`、`journal_mode=WAL`、`synchronous=NORMAL`、`foreign_keys=ON`。
- 三段 schema 常量：`SCHEMA_V1`（db.rs:14-193，13 表：settings/agents/capabilities/capability_versions/capability_sources/installations/mcp_definitions/plugin_records/profiles/profile_items/operations/snapshots/scan_results）、`SCHEMA_V2`（db.rs:195-258，5 张验证平台表）、`SCHEMA_V3`（db.rs:263-317：schema_migrations、operation_steps、adapter_evidence、audit_events + operations 8 个幂等加列 db.rs:322-352）。
- 关键约束：`capabilities.kind CHECK IN ('skill','mcp','plugin')`（db.rs:37）——**新业务实体不能塞进 capabilities 表**；`installations` 唯一键 `(agent_id,target_path,config_key)`（db.rs:96）。
- 迁移机制：`Database::migrate()`（db.rs:626-674）读 `PRAGMA user_version`，逐版本 `unchecked_transaction` 内 `execute_batch(SCHEMA_VN)` + `pragma_update(user_version,N)` + commit。**v3 起强制规则：迁移前必须 `backup_before_migration`**（db.rs:648-664，注释 db.rs:649 "migration must be preceded by a recoverable DB copy"）；迁移后 `integrity_check` 失败返回 `Error::Verification`（db.rs:665-671）。
- 迁移前备份：`backup_before_migration`（db.rs:679-716）`VACUUM INTO` 到 `backups/pre-migration-v{from}-{ts}-{id8}/` + manifest（注意 manifest 的 `to_version` **硬编码 3**（db.rs:705），做 v4 时需参数化——登记为实施前置项）。
- 用户备份：`Library::create_backup`（library.rs:125-208）`VACUUM INTO` + `BackupManifest` + `restrict_owner_permissions`（0700/0600）+ 在 operations 表登记 `"backup"` 操作；恢复 `restore_backup`（service.rs:2031+）安全序列：validate→integrity→新建安全备份→原子替换 DB→清理 WAL/SHM（service.rs:3444-3449）→失败回滚。
- 事务用法：全库共 5 处 `unchecked_transaction`——db.rs 4 处（迁移 v1/v2/v3 + `replace_scan_results` db.rs:1874-1896）+ service.rs 1 处（`upsert_profile` service.rs:2579 的 profile items 批量替换）；其余为单语句。

### 2.3 跨进程锁与写路径

- `runtime.rs`：fs2 advisory flock，锁文件 `<data_dir>/runtime.lock`（runtime.rs:36）；锁文件体 `{pid,kind,acquiredAt}` 仅诊断；**Drop 只 unlock 永不删除**（unlink race 修复，runtime.rs:19-23）；忙时返回 `Error::blocked`（runtime.rs:96-106）。
- `AgentHub::runtime_guard()`（service.rs:219-232）：线程内重入计数（`RUNTIME_HELD`）；文档注释明确 "read-only paths must not call this"（service.rs:220-221）。集成测试 `tests/runtime_lock.rs` 佐证：外部持锁时写操作报 busy 而读操作正常。
- 修改类操作的双层锁模式（模板 `upsert_profile` service.rs:2546-2548）：先 `write_lock.lock()` 再 `runtime_guard()`。
- ⚠️ `create_backup`/`restore_backup` 直调 `runtime::acquire` 绕过 `runtime_guard`（service.rs:2011-2012、2033-2034），**不可重入**——新代码不得照抄此形式。
- 只读操作（list_agents/scan/dashboard 等）不加锁。

### 2.4 Executor（`ops.rs`）

- `Executor { db, library }`（ops.rs:180-183）；`apply(plan, force)`（ops.rs:186-377）：kind 白名单 → preflight（:379）→ `acquire_lock(library)`（:58，库级文件锁 `.agenthub.lock`）→ audit_log → 逐 item `insert_operation_step` → `apply_item`（:474，内部 `snapshot_target` :774 → `apply_mcp_item`/`install_symlink`/`install_copy`）→ `verify`（:689，写后哈希校验）→ 失败逆序补偿 `undo_item`（:291-299，只回滚内容仍匹配自己写入的目标）→ Success/Partial。
- plan 阶段不在 ops.rs：各 `plan_*` 收敛到 `persist_plan`（service.rs:1643-1690，生成 plan_hash、approval TTL）。
- **适用范围**：真实 Agent 目录写入（skill/MCP/profile 安装卸载分发）。**本方案的应用自身数据库写入（stack/subscription）不属于 Agent 目录写入，不走此 Executor**，但仍按契约持锁并以 `audit_events` 记录变更（D-1 冻结：不写 operations、不扩展 OperationKind，见 ARCHITECTURE_PROPOSAL §5）。这一区分是主控提示词约束 B 的关键：不得误用分发 Executor。

### 2.5 契约与错误

- `ApiResult<T>`（contract.rs:153-162，camelCase）：`{ok, data?, error?, requestId}`；`request_id()` = uuid v4（contract.rs:10-12）。
- `ApiError { code, message, retryable, details? }`（contract.rs:116-124）。
- `ErrorCode` 18 个（contract.rs:17-37）：InvalidInput / UnsupportedCapability / UnverifiedAdapter / PermissionDenied / PathEscape / InvalidSkill / McpVersionMismatch / SecretLocked / PlanStale / ExternalModification / PartialApply / RollbackBlocked / ExecutorUnavailable / EvalInconclusive / BudgetExceeded / UpstreamTimeout / RateLimited / Internal；CLI 退出码映射 contract.rs:64-81。
- `Error` 14 变体（error.rs:8-37）：Io/Database/Json/Yaml/Toml/Zip/InvalidSkill/Conflict/NotFound/Unsupported/Verification/Blocked/ExternalModification/Message。无 "already exists" 专用变体（重复键走 Conflict 或 Database）。

### 2.6 检测能力（My AI Stack Inventory 的数据源）

- 11 个内置适配器（`adapters/builtin.rs:22-36` `all_adapters()`）：codex(:82)、chatgpt(:190)、trae(:247)、trae-cn(:327)、zcode(:503)、workbuddy(:584)、dsh(:704)、claude-code(:792)、cursor(:865)、qwen-work(:936)、doubao-work(:1070)。
- 检测原理：`detect(home)` 读目录约定（`~/.<dot>` + /Applications 的 app bundle，helper `detect_dirs` builtin.rs:42-56）与专用路径（如 claude-code 的 `~/.claude` 与 `~/.claude.json` builtin.rs:817-834）。返回 `Detection { status, app_path, home_path, evidence, notes }`（adapters/mod.rs:22-29）。
- 可写安装根仅 Codex/TRAE/TRAE CN/ZCode/WorkBuddy/DSH；ChatGPT=Unsupported；Claude Code/Cursor=VerificationRequired（ADAPTER-SPEC §6）。
- 扫描 `scan::scan_agents`（scan.rs:18-233）：**纯只读**（模块头注释 scan.rs:1-4），唯一副作用是刷新 `scan_results` 表；产出 `DiscoveredSkill` 等结构（model.rs:422-476）。
- 服务层检测入口（My AI Stack 可复用）：`sync_agents`（service.rs:157-215，稳态变化检测后才写库）、`list_agents`（:352-432）、`agent_inspect`（:318-330）、`scan`（:615-629）、`check_changes`（:2134-2181）、`dashboard`（:2185-2284）、`agent_health_report`（:3174-3243）。

### 2.7 测试基线

- core `#[test]` **109 个**（grep 实测；src 50 + tests/ 59），`#[tokio::test]` 0（全同步）。代表：`tests/runtime_lock.rs`、`tests/v31_operations.rs`、`tests/validator.rs`（含 user_version/table_count helper :131-138）。
- CLI 测试 22 个（crates/agenthub-cli/tests/）。
- 前端 vitest：`src/__tests__/`（app-smoke、health-ui）、`src/components/__tests__/`（PlanDialog、RecoveryReportView）、`src/lib/__tests__/`（8 个纯函数 spec）——共 12 个 spec 文件。测试通过 `vi.mock('../lib/api')` 整模块 mock（health-ui.spec.ts:69-113 模板），不直接 mock `@tauri-apps/api/core`。

## 3. 前端事实（`src/`）

- 路由 `src/router.ts`（183 行）：hash history；11 条路由（/dashboard /agents /skills + /skills/:id 详情 /matrix /mcp /plugins /profiles /activity /health /settings）；`navItems`（router.ts:26-106）同时驱动侧边栏（`AppSidebar.vue:48-63`）与 ⌘1-9 快捷键（App.vue:101-107）。新增导航入口改 3 处：router.ts navItems + i18n en/zh 的 nav 段 +（可选）Icon.vue。
- Stores（Pinia setup 风格）：app.ts（bootstrap/dashboard/scan，175 行）、operations.ts（170 行）、skills.ts（44 行，15s TTL 缓存模板）、ui.ts（toasts/theme/locale）。
- 12 视图；CRUD 模板 `ProfilesView.vue`（loading/error/EmptyState/编辑 BaseModal :330-411/删除 ConfirmDialog :413）；详情模板 `SkillDetailView.vue`（useRoute().params.id :42-52）；现有代码**无独立 edit 视图**，编辑一律 BaseModal 表单。
- i18n：`src/i18n/index.ts`（默认 zh，fallback zh）；en.ts 是 `typeof zh` 的结构镜像——**新增 key 必须 zh 先写、en 镜像**，否则 vue-tsc 报错。
- 设计系统：`src/styles.css`（2163 行）Tailwind v4 + CSS 变量 token（`@theme` :13-24、`--ah-*` :26-60、`.dark` 暗色覆盖）；语义 class（.btn/.card/.input/.field/.notice）。
- 可复用组件：BaseModal / ConfirmDialog / DataTable（泛型）/ StatCard / PageHeader / EmptyState / LoadingState / ErrorBanner / StatusBadge / ToastHost。
- api 层：`src/lib/api.ts` 唯一 invoke 面（`call`/`callApi` :140-166）；`types.ts` 手写镜像 core model.rs（camelCase）；`envelope.ts` 解析 ApiResult 信封。

## 4. Tauri 权限与网络能力

- `src-tauri/capabilities/default.json`：仅 `core:default`、`opener:default`、`dialog:default`——**无 shell/fs/http 插件**。
- CSP 严格（tauri.conf.json:27，`script-src 'self'`）。
- 一方代码零 HTTP 客户端（grep reqwest/ureq/fetch 仅命中测试 fixture 与审计规则文本）；Cargo.lock 中的 reqwest 0.13.5 仅由 tauri crate 传递引入，非一方依赖。
- 唯一出站例外：用户显式 `git clone`（skill.rs:473-509）与 opener 打系统浏览器。
- **结论：应用当前完全离线，M1/M2 默认离线约束在现有架构上天然成立，无需新增任何权限。**

## 5. 敏感数据处理

- `redact.rs`：诊断导出专用脱敏（redact_url :23 / redact_args :92 / fold_home :125，"export-only" :16-17）；唯一调用点 `export_diagnostics`（service.rs:2290-2302）。
- MCP secrets 只存 Keychain（`mcp.rs:22` KEYCHAIN_SERVICE="com.agenthub.mcp"），DB 只存 key 名（mcp.rs:156）；含引号值回退 argv 模式为文档化残余风险（SECURITY.md R3）。
- grep 未发现 key/secret/prompt 写入日志的代码。**新模块设计上不接触任何 secret/prompt/账单凭证**（见 SECURITY_AND_PRIVACY_REVIEW）。

## 6. 脚本与环境

| 脚本 | 行为 | 对真实环境的接触 |
|---|---|---|
| `scripts/cargo.sh`（5 行） | source cargo-env.sh 后 exec cargo | 构建写 target/ 与 CARGO_HOME，无用户数据接触 |
| `scripts/cargo-env.sh` | 前置 ~/.cargo/bin、RUSTUP_TOOLCHAIN=stable、CARGO_HOME=/tmp/agenthub-cargo | 绕过 Homebrew rustc 1.87（过旧，编不了 Tauri 2.12） |
| `scripts/v31_acceptance.sh`（200 行） | **完全隔离、对真实环境只读**：全部写 /tmp/agenthub-v31-acceptance；AGENTHUB_HOME=$LOG/home | 唯一接触：拷贝 `~/.agenthub/agenthub.db(+wal/shm)` 到 /tmp 做迁移测试（:36-39）；real-scan 用 symlink 只读扫描真实 agent 目录（:181-195）。**P0-5 注意**：该脚本仍会**读取**真实 `~/.agenthub` 数据库副本与本机 Agent 目录（只读），不满足本模块新验收的"纯 fixture 不读真实环境"要求 → M1-C 新增独立 fixture 脚本（ACCEPTANCE §4） |
| `scripts/real-acceptance.sh`（204 行） | **不是只读**（头部注释要求机器所有者显式批准） | 写真实 `~/.codex/skills/`(:14)、`~/.zcode/skills/`(:15)、真实 `~/.agenthub/library`(:183)，并 `rm -rf /Applications/AgentHub.app` 安装 .app（:201-202） |

- AGENTHUB_HOME 机制：`service.rs:58-65`（env 覆盖 home；`AGENTHUB_DATA_DIR` 覆盖数据目录；CLI `--home` 同义 main.rs:59）。所有 agent 目录相对 home 解析 → 指向 /tmp 即整体隔离。
- 已知环境限制：① 沙箱内 `security` 命令被拒 → Keychain 测试 `tests/mcp.rs:406` 在无 GUI 会话必失败且**会中止 cargo test 后续套件**（PRODUCTION-SAFETY :37、PROJECT-HANDOVER :114）——复跑需正常终端或按 `--test` 单测；该失败是测试环境限制而非产品缺陷。② rustc 需经 `scripts/cargo.sh`。③ `.cargo-home/` 为历史遗留（gitignore:41）。

## 7. 文档 ↔ 代码不一致清单（发现即登记，不擅自按旧文档改代码）

| # | 不一致 | 文档说 | 代码/实测 | 影响 |
|---|---|---|---|---|
| D1 | DB 版本 | DATA-MODEL/ARCHITECTURE 写 DB v2 | 实际 `SCHEMA_V3`（db.rs:263）+ user_version=3 | 新迁移从 v4 起，方案以 db.rs 为准 |
| D2 | Tauri 命令数 | AGENTS/ARCHITECTURE 写 48；VALIDATION:341 写 47 | **实测 55**（lib.rs:557-613） | 文档数字过时，不影响设计 |
| D3 | 测试数 | TEST-PLAN 写 98 | core 109 + CLI 22 + 前端 12 spec | 同上 |
| D4 | 迁移前备份 | DATA-MODEL §9 写"未实现" | 已实现（db.rs:679-716，v3 起强制） | 新迁移直接沿用 |
| D5 | 跨进程锁覆盖 | ARCHITECTURE §5 写 5 条路径无锁（0.4 快照） | v0.5.1 已全部持 runtime lock | 读 ARCHITECTURE §5 时以 runtime.rs 为准 |
| D6 | MCP headers | DATA-MODEL 写明文存储 | 0.5.0 起 Keychain 化，DB 只存 key 名 | 无影响 |
| D7 | CLI 命令清单 | CLI.md 未记录 `diagnostics` 命令 | CHANGELOG 0.5.0 已加，main.rs:951 | 文档缺口，登记待修 |
| D8 | 版本号 | CHANGELOG "0.5.1 未发布"；HANDOVER「先发布 0.5.1」 | 修复代码已随 v0.5.0 tag 发布；无 0.5.1 tag | **已裁决（D-3 方案 A，M0-R）**：归档至 0.5.0，不补造 Release |
| D9 | 错误契约 | 交接文档称已统一 ApiResult | 双轨并存：旧 48+ 命令仍 `Result<T,String>`（PRODUCTION-SAFETY R4） | 新模块命令直接走 ApiResult，不迁移旧命令 |

## 8. 成本/订阅领域现状

- docs/ 全量 grep「订阅/花费/成本/subscription/billing/cost/pricing/计费/预算」：**零条用户成本相关设计**（仅 MCP 协议层 subscriptions/listen 等无关命中）。
- PRD §5 明确把「调用统计（需运行时代理）」列为 MVP 边界外；ROADMAP Phase 5（生态规模化）未立项。
- **结论：My AI Stack 与 Cost Optimizer 是全新绿地模块；引擎从不执行技能、不联网、无运行时代理，因此任何"用量/账单"数据在本产品边界内只能来自用户手动登记。** 这一定位直接写进两份 PRD 并作为 Cost Optimizer 证据等级的基础。

## 9. 复用结论速览（详见 ARCHITECTURE_PROPOSAL）

| 现有能力 | 复用方式 |
|---|---|
| `AgentHub` 单引擎 + ApiResult 信封 | 新方法全部挂在 AgentHub，走 v3.1 契约 |
| SCHEMA_V3 迁移序列 + 迁移前备份 | 新增 SCHEMA_V4 加法迁移（含 backup_before_migration 参数化） |
| runtime_guard + write_lock 双锁模式 | stack/subscription 写方法照 `upsert_profile` 模板 |
| 11 适配器 detect + sync_agents | Inventory 的 detected 事实源，只读复用 |
| CLI gate_plan/--json 与 Tauri run_api | 新命令双侧同步暴露 |
| Vue CRUD 模板（ProfilesView/BaseModal/ConfirmDialog/DataTable/i18n 镜像） | My AI Stack 页面 |
| validation.rs 纯函数推导模式（derive_health） | Cost Optimizer 规则引擎照此做无状态纯函数 |
| AGENTHUB_HOME 隔离 + v31_acceptance 模式 | 全部测试隔离 |
