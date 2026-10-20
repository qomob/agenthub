# AgentHub 产品需求文档（PRD）

> 版本：0.2（Capability Control Center）
> 日期：2026-10-20
> 状态基准：以当前代码与实测为准。**「已实现」不等于「已验收」「已验证」**；本文在每个功能后区分「代码已实现 / 未验证 / 未实现」。
> 关联：[ARCHITECTURE.md](ARCHITECTURE.md)、[ADAPTER-SPEC.md](ADAPTER-SPEC.md)、[TEST-PLAN.md](TEST-PLAN.md)、[AGENT-COMPATIBILITY.md](AGENT-COMPATIBILITY.md)

---

## 1. 产品定位

**一句话**：AgentHub 是 macOS 上的本地 **Agent 能力控制中心（Capability Control Center）**——集中登记散落在 Codex、TRAE、ZCode、WorkBuddy、DSH 等 Agent 目录里的 Skills、MCP 配置和插件清单，事务化分发并回滚，同时用静态审计、依赖诊断、更新影响、人工验证证据与 Agent 版本健康回答「能不能用、安不安全、更新后会不会坏」。

**形态**：Tauri 2 桌面应用（Rust 核心 + Vue 3 UI），纯本地、无账号、无云端；同一 Rust 核心同时服务 GUI 与 CLI。

**不是**：

- 不是 Skill 市场（不提供下载源、不做排行/推荐）；
- 不是 Agent 运行时（不执行 skill 里的脚本、不启动 MCP server）；
- 不是自动安全认证服务（静态审计是规则引擎，不是完备安全证明）；
- 不是云同步工具（MVP 不做多设备、不做 Git 备份）；
- 不是通用文件同步器（只管理白名单内、有适配器证据的路径）。

---

## 2. 目标用户与核心问题

### 2.1 用户

| 用户 | 特征 | 核心诉求 |
|---|---|---|
| 多 Agent 个人用户 | 同时用 2–5 个 Agent，各有自己的 skill 目录 | 一处看清「我有哪些能力、装在哪」 |
| Skill 作者/重度用户 | 自己维护一批 SKILL.md，频繁改版本 | 一次修改，分发到多个 Agent，且能回退 |
| 开发者/技术爱好者 | 愿意看路径、哈希、日志 | 可解释、可审计、不破坏现有配置 |

MVP **不面向**团队协作、企业管控、非 macOS 平台。

### 2.2 核心问题

1. **能力分散**：同一个 skill 在 `~/.codex/skills`、`~/.trae-cn/skills`、`~/.agents/skills`、`~/.workbuddy/skills` 各有一份，内容已经漂移，用户不知道哪份是「最新/正确」的。
2. **复制靠手工**：跨 Agent 安装通常靠 `cp -r` / 手工拖拽，覆盖前不备份，覆盖后无法回退。
3. **格式不统一**：MCP 至少有 `mcpServers` JSON、`mcp.servers` JSON、`[mcp_servers]` TOML、Cordis YAML 四种；直接复制会写坏目标配置。
4. **没有事实记录**：文件系统只能回答「有没有」，不能回答「是谁装的、装的哪个版本、有没有被外部改过」。
5. **安全边界模糊**：第三方 skill 可能带脚本/`allowed-tools`/动态命令注入；MCP 的 stdio 条目等价于任意命令执行。
6. **安装 ≠ 可用**：文件复制成功不代表运行时存在、依赖齐全、Agent 能触发该 skill。
7. **依赖未就绪**：skill 的脚本依赖 `python3`/`node`/包，本机缺失时只有真正运行时才暴露。
8. **更新影响未知**：新版本可能新增脚本、网络访问、依赖或权限；更新前无法判断是否需要重新验证。
9. **「验证过」不可追溯**：口头/记忆中的验证无法绑定到具体版本与 Agent，Agent 升级后也无法感知。

---

## 3. 核心原则（7 条）

> 任务书原文称「六大原则」，但列出的约束实际是 7 条；本文按代码实现归纳为 7 条，并给出对应实现证据。

| # | 原则 | 含义 | 实现证据 | 状态 |
|---|---|---|---|---|
| 1 | **Local First** | 所有数据在本机；无账号、无云端、无遥测 | 仅 SQLite + 本地文件；Keychain 本机；无 HTTP 客户端依赖 | 已验证（代码层） |
| 2 | **Single Source of Truth** | 库内一份 canonical 版本，Agent 目录是部署副本；「装没装」查 `installations` 而非猜文件 | `library/skills/<slug>/<hash12>`；`installations` 唯一键 | 已验证 |
| 3 | **Adapter First** | 每个 Agent 的差异集中在 `AgentAdapter`；执行器不写 Agent 特例 | `adapters/mod.rs:108-173`、`builtin.rs` | 已验证 |
| 4 | **Non-destructive** | 不覆盖非 AgentHub 拥有的目标；写入前快照；删除只删自己装的；不删整个配置文件 | `preflight` 冲突/外部修改检查；`snapshot_target`；MCP 单条替换 | 已验证 |
| 5 | **Capability-based** | 适配器显式声明能力等级（symlink/插件/传输/格式）；未验证能力不得声称可用 | `SymlinkSupport`/`PluginSupport`/`AgentStatus` | 部分（DSH symlink 声明与证据不一致） |
| 6 | **Transactional** | 一次操作要么全部成功，要么逆序补偿；失败不留半配置 | `ops::Executor` saga + 快照 + rollback | 已验证（Skill）；MCP 执行路径有缺陷 |
| 7 | **No Fake Integration** | 没有真实证据/真实实现的集成不展示、不写入；未验证项明确标注 | ChatGPT 只检测不写；DSH MCP 不声明；Claude/Cursor 标 VerificationRequired | 已验证 |
| 8 | **Verified, not assumed** | 安装 ≠ 可用 ≠ 已验证；只有审计/环境/人工证据支持的当前版本才是 `verified` | `validation::derive_health`；安全优先；旧版本证据失效 | 已验证（单测 + 真实 Mac） |

---

## 4. 功能需求 A–G 与当前状态

状态图例：**✅ 已实现（代码可证）** / **🟡 部分实现 / 未验证** / **⛔ 未实现** / **⚠️ 已知缺陷**。
注意：CLI 与 GUI 均已实现。**真实 Mac 验收已完成**（2026-10-20：真实库导入 → 双 Agent 分发 → 扫描 inSync → 更新 → 回滚 → 卸载 → 备份/doctor），GUI 也在隔离 home 下完成运行验收；详见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)。

### A. Agent 检测与能力盘点

**需求**：启动后自动识别本机已安装的 Agent，列出每个 Agent 的 skill 根、MCP 目标、symlink/插件支持、安装证据。

| 子项 | 状态 | 说明 |
|---|---|---|
| 9 个内置适配器检测 | ✅ | `sync_agents()` 每次列表前刷新（`service.rs:132-169`） |
| 展示 evidence（真实路径） | ✅ | `Detection.evidence`，`doctor()` 输出 |
| 自定义 home / app 目录 | ✅ | `AGENTHUB_HOME`、`AGENTHUB_EXTRA_APP_DIRS` |
| 用户覆盖 Agent 路径 / 自定义 skill 根 | ✅ | `agents_update_paths`；custom root 参与扫描与安装；`AgentView.customSkillRoots` 暴露给 UI |
| 项目级扫描根 | ✅ | `set_project_roots` + `project_skill_roots`；扫描结果写 `scope=project`；安装仍按 custom root 处理 |
| 检测可靠性（同名 app 区分） | ⚠️ | 未读 `CFBundleName`；ChatGPT/Codex 同 bundle id 风险未闭合 |
| UI | ⛔ | 页面规格见 [UI-SPEC.md](UI-SPEC.md) |

**验收标准**：`doctor()` 中每个已安装 Agent 至少有一条 evidence；未安装 Agent 状态为 `not_installed` 且不产生写入入口。

### B. Skill 导入与中央库版本管理

**需求**：从文件夹、单文件 `SKILL.md`、zip、Git、Agent 目录导入；按内容哈希去重；保留来源；不覆盖已有版本。

| 子项 | 状态 | 说明 |
|---|---|---|
| 文件夹导入（含多候选冲突提示） | ✅ | `resolve_skill_folder`（`service.rs:812-841`） |
| 单 `SKILL.md` 导入 | ✅ | 复制到 staging 后按普通 skill 处理 |
| zip 导入（路径穿越/符号链接/大小/条目数防护） | ✅ | `skill.rs:373-472`；上限 100MB / 4000 条目 |
| Git 导入（浅克隆、不执行仓库脚本、删 `.git`） | ✅ | `skill::clone_git`；受 `git_import_enabled` 控制 |
| 从 Agent 目录导入 | ✅ | `ImportSource::AgentDir`，记录 agent key |
| 内容去重（同 hash 复用） | ✅ | `UNIQUE(capability_id, content_hash)` |
| 新版本不覆盖旧版本 | ✅ | 仅插入新 version，旧版本保留 |
| SKILL.md 校验（name/description、长度、目录名） | ✅ | 警告级提示 + 硬错误；见 `skill.rs:98-232` |
| 编辑库内 SKILL.md | ⛔ | 只能重新导入；无编辑接口 |
| 删除库版本 / 回收站 | ⛔ | 无删除 API |
| UI/CLI | ⛔ | 无 |

**验收标准**：同一内容导入两次得到 `created=false`；不同内容导入后旧版本仍可在详情页看到；任何导入都不修改源目录与 Agent 目录。

### C. Skill 分发 / 更新 / 卸载（事务化）

**需求**：选择 skill 版本 + 多个 Agent → 预览计划 → 确认执行；支持 symlink/copy；失败自动恢复。

| 子项 | 状态 | 说明 |
|---|---|---|
| 生成计划（create/replace/noop/conflict/blocked） | ✅ | `plan_install`（`service.rs:886-1079`） |
| dry-run 计划落库 | ✅ | `operations` 行，`dry_run=1` |
| 预检（绝对路径/冲突/外部修改） | ✅ | `preflight` |
| copy 安装（staging + rename） | ✅ | `install_copy` |
| symlink 安装 + 校验 | ✅ | `install_symlink` + `verify`；symlink 能力未验证时 warning |
| 不支持 symlink 自动降级 copy | ✅ | ChatGPT → copy（但 ChatGPT 无安装根，实际会 blocked） |
| 失败逆序回滚（自动） | ✅ | `apply` 的补偿逻辑 |
| 手动一键回滚 | ✅ | `rollback_operation` |
| 更新（版本切换） | ✅ | 通过同一 `plan_install(version_id)` 实现，冲突时需 `force` |
| 卸载 | ✅ | `plan_uninstall` + `Remove` |
| 并发外部修改检测 | ✅ | `ExternalModification` |
| 目标路径必须位于白名单根 | ⚠️ | `target_within_roots` 存在但**没有调用方** |
| UI/CLI | ⛔ | 无 |

**验收标准**：一次向 2 个 Agent 分发，任一失败时两个目标都回到原状；回滚后文件内容 hash 与操作前一致；操作记录可在 Activity 中看到。真实 Mac 已验证 2 Agent copy、更新、回滚、卸载。

### D. 快照、回滚与变更检测

| 子项 | 状态 | 说明 |
|---|---|---|
| 每次写入前快照（目录/文件/symlink/absent） | ✅ | `snapshot_target` |
| 快照索引可查询 | ✅ | `snapshots_list` 命令 |
| 操作级回滚（一次） | ✅ | `rollback()` |
| 漂移检测（内容被外部修改） | ✅ | `installation_view` + `check_changes` |
| 缺失检测（部署目录消失） | ✅ | 同上 |
| 「有更新可用」 | ✅ | `update_available`（已部署版本 ≠ 当前版本） |
| 自动扫描外部变更 | 🟡 | 仅在调用 `check_changes`/Dashboard 时计算，无文件 watcher |
| 快照垃圾回收 | ⛔ | 快照永久保留，无清理策略 |
| UI/CLI | ⛔ | 无 |

**验收标准**：修改已部署目录内容后 Dashboard 显示 drift；删除目录后显示 missing；回滚后状态恢复。

### E. MCP 定义、发现与分发

| 子项 | 状态 | 说明 |
|---|---|---|
| MCP 定义 CRUD（stdio/http/sse/streamable_http） | ✅ | `mcp::upsert`，校验 command/url 互斥 |
| 从 Agent 配置发现 MCP | ✅ | 仅支持已声明 codec 的文件；读结构，不落值 |
| 从 Agent 配置导入为定义 | ✅ | `mcp::import`；同名不同内容报冲突 |
| 生成分发计划（create/replace/noop/conflict/blocked） | ✅ | `mcp::plan_distribute` |
| 传输/格式/父目录可写校验 | ✅ | 无可用目标则 blocked |
| Keychain 存 env 值 | ✅ | macOS `security` CLI；`com.agenthub.mcp` |
| 分发执行（真正写配置） | ✅ | kind 白名单含 McpDistribute/McpRemove；MCP preflight 按条目 hash；集成测试与临时 home 验证 TOML/JSON 添加/移除、MCP 回滚 |
| 移除 MCP 条目 | ✅ | 已真正删除条目（`write_entries` 清理 stale key）；删除后 TOML 会留下空 `[mcp_servers]` 表（格式噪音） |
| 多格式 codec（json:mcpServers / json:mcp.servers / toml:mcp_servers） | ✅ | `mcp.rs:142-258` |
| 格式保真 | 🟡 | TOML 保注释；JSON 只保结构（见 ADAPTER-SPEC §6） |
| MCP 试连（initialize/tools list） | ⛔ | 无 MCP 客户端 |
| OAuth / headers 密钥引用 | ⛔ | `credential_refs` 无实现；headers 值明文 |
| DSH Cordis YAML codec | ⛔ | 未实现，DSH 无 MCP 目标 |
| UI | ⛔ | 无 |

**验收标准**：向 Codex 写 stdio server，`config.toml` 其他段与注释保持；写第二个 server 不覆盖第一个；失败时整份配置恢复到操作前；env 值不出现在 UI 列表、`operations.plan` 或日志。当前 MCP 写入由集成测试覆盖；真实 MCP 配置未在验收中改动。

### F. 插件只读清单与 Profiles

| 子项 | 状态 | 说明 |
|---|---|---|
| 插件发现（Codex/TRAE CN/WorkBuddy 等） | ✅ | `sync_plugins`，只读 |
| 插件安装/卸载/启用 | ⛔ | 设计上不做（避免写账号/市场状态） |
| 内置 Profiles 种子（Coding/Research/Writing/Marketing） | ✅ | 4 个空预设 |
| Profile CRUD（条目 + 目标 agent key） | ✅ | `profiles_upsert` / `profiles_delete` |
| Profile 应用（生成安装计划） | ✅ | `plan_profile_apply`；仅 skill，其他 kind 跳过并 warning |
| Profile 应用执行 | ✅ | 复用 install plan 的 apply |
| Profile UI | ⛔ | 无 |

**验收标准**：Profile 只包含已安装 Agent 的目标；不存在任何写插件市场的路径。

### G. 设置、诊断、备份与活动记录

| 子项 | 状态 | 说明 |
|---|---|---|
| 库位置切换（校验空目录、复制旧内容） | ✅ | `change_library_root`；不允许互相嵌套 |
| 默认安装策略 / 语言 / 主题 / 启动扫描 | ✅ | settings KV；语言仅 `zh`/`en`/`system` |
| 诊断（库/DB/data_dir/git/agents） | ✅ | `doctor()` |
| 完整备份（DB VACUUM INTO + manifest） | ✅ | `create_backup` |
| 备份列表 | ✅ | `list_backups` |
| 从备份恢复 | ⛔ | 无代码路径 |
| 活动记录（operations 列表/详情/快照） | ✅ | `list_operations` / `get_operation` |
| 操作日志（import/scan） | 🟡 | 导入已写 `operations`（kind=import）；扫描只刷新 `scan_results`，不写 operations |
| 结构化日志文件 | ✅ | `<library>/logs/operations.log` JSONL：import/operation.start/finish/failed（不含 secret） |
| CLI 覆盖 | 🟡 | `doctor`/`agents list`/`skills list|scan|import|install|sync|rollback`/`mcp list|discover|distribute`/`operations list`/`profiles list`/`backup|backups`；**无 MCP 创建命令、无 settings/profiles apply/agents 编辑命令** |
| UI | ⛔ | 无 |

**验收标准**：`doctor()` 的检查项都有可操作 hint；备份目录含可打开的 `agenthub.db` 与 `manifest.json`；Activity 能还原一次操作的计划与结果。

---

### H. Capability Validator（静态审计 / 依赖 / 更新影响）

**需求**：在不执行 skill 脚本、不联网、无 AI 评分的前提下，给出确定性、可解释的安全与可用性证据。

| 子项 | 状态 | 说明 |
|---|---|---|
| 静态规则审计 | ✅ | `audit.rs`：spec/security/quality/dependency 规则；finding id/severity/category/path/suggestion；输出 pass/warn/fail 与 low..critical |
| 脚本高危模式 | ✅ | `curl|sh`、`wget|sh`、`sudo`、`rm -rf /`、`eval`、`base64 -d`、`os.system`、`subprocess shell=True`、SSH/AWS/私钥/Keychain 访问等 |
| 硬编码密钥 | ✅ | 检测类型/行号；**原值脱敏**，不写入 finding/operations/logs |
| 网络与外泄风险 | ✅ | 远程域名清单、网络+敏感数据组合告警 |
| 权限/缺失引用/symlink 逃逸/依赖清单 | ✅ | world-writable、缺失相对引用、逃逸 symlink、有脚本无 manifest |
| 依赖与环境诊断 | ✅ | shebang runtime 真实 `--version`（2s 超时）+ requirements/pyproject/package.json；MCP/凭据 unknown |
| 更新影响分析 | ✅ | 文件 diff（最多 500 条 + 真实总数）、classes、风险、requiresRevalidation；写入 `update_impacts`，进入 plan warnings |
| 不做 | ⛔ | 不执行脚本、不联网、不装依赖、不自动修复、不做 AI 评分 |

**验收**：审计有稳定 rule id 和可复现排序；密钥不出现在 finding/操作/日志；缺 runtime → deps needs_attention；更新加入脚本/网络/依赖 → requiresRevalidation=true 且 plan 有 warning。

### I. Agent Health Monitor

**需求**：真实记录 Agent 版本与路径健康，首次观测为 baseline，版本变化提示重新扫描/验证。

| 子项 | 状态 | 说明 |
|---|---|---|
| `.app` 版本读取 | ✅ | `CFBundleShortVersionString`：`plutil` → XML 解析 → `mdls` 兜底；不猜测 |
| CLI 版本读取 | ✅ | PATH + `/opt/homebrew/bin` + `/usr/local/bin`；`--version` 2s 超时；DSH fallback |
| baseline / changed | ✅ | 首次观测 `changed=false`；后续版本不同 → `changed=true` + 提示 |
| 同秒排序 | ✅ | `detected_at DESC, rowid DESC`；QA 发现的 same-second bug 已修复并有回归测试 |
| 路径健康 | ✅ | `paths_ok`（skill root 或 MCP target 存在）；无 root 时提示 |
| 各 Agent 支持 | ✅ | 真实 Mac：ChatGPT/Codex/DSH/TRAE CN/WorkBuddy/ZCode 有版本；TRAE 国际/Claude/Cursor 无稳定来源（不伪造） |

**验收**：真实 Mac 能读出 6 个 Agent 版本；首次全部 baseline；版本变化可观测；无版本时显示未知而不是假值。

### J. Validation Evidence 与健康状态机

**需求**：记录真实人工验证证据；健康状态由审计/环境/证据/版本共同推导；安装≠验证。

| 子项 | 状态 | 说明 |
|---|---|---|
| 验证记录 | ✅ | `validation_runs`：agent/case/status/notes/evidence/kind/actor/version |
| 人工证据语义 | ✅ | `kind=manual & passed` 才可贡献 verified；明确不是自动通过；封闭 Agent 无法自动 E2E |
| 状态机 | ✅ | NotDeployed / Failed / NeedsAttention / Verified / Unverified；安全优先；旧版本证据失效 |
| 提交校验 | ✅ | case 必填；status/kind 白名单；agent 必须存在；版本必须属于该 capability |
| 审计 | ✅ | `capability.validation` 写入 JSONL 审计（不含 notes/evidence 内容） |
| 不做 | ⛔ | 不自动判定通过、不做运行时执行、不对 evidence 真实性做证明 |

**验收**：未部署 → not_deployed；仅有安装 → unverified；audit fail + manual passed → needs_attention；当前版本全部安装 Agent manual passed → verified；旧版本 failed 不影响当前版本。

## 5. MVP 边界（明确不在范围）

| 不做 | 理由 |
|---|---|
| 云同步 / 多设备 | 无账号体系；MVP 是 Local First |
| Skill / 插件市场 | 需要服务端、信任与审核体系 |
| 第三方适配器市场（动态加载） | 加载不受信代码违反安全模型；当前 adapter 全部编译进二进制 |
| 调用统计 / 使用分析 | 需要运行时代理，MVP 不介入 Agent 运行 |
| MCP 试连与工具调用 | 需要 MCP client、协议版本分流、进程管理；列为 Phase 3+ |
| DSH Cordis YAML / TRAE 全局 MCP 写入 | 格式证据不足（见 REFERENCE-STUDY §6） |
| 插件安装/启停 | 账号/市场资产，写文件不等于生效 |
| Windows / Linux | 路径、symlink、Keychain 均未验证；MVP 只承诺 macOS Apple Silicon |
| 自动更新应用 / 自动更新 skill | 无签名与更新通道 |
| 编辑 SKILL.md / 删除库版本 | MVP 只做导入与部署；避免内容编辑器复杂度 |
| 执行 skill 脚本 / 运行 MCP | 安全边界之外 |
| AI 安全评分 / 自动判定通过 | 静态审计是确定性规则；不做不可解释的评分，也不把自动结果当验证 |
| 自动端到端测试封闭 Agent | 封闭桌面 Agent 无测试接口；只能记录人工证据（明确标注不是自动通过） |
| 运行时沙箱 / 依赖自动安装 | 验证平台只诊断，不执行/不安装 |
| 完整 i18n 文案 | zh/en 页面文案已基本齐全，仍可能有遗漏 |

---

## 6. 首版验收场景（端到端）

> 目标：扫描 ZCode / TRAE / DSH → 从发现的技能导入库 → 分发到两个 Agent → 修改库版本并看到变更 → 失败一键恢复。
> **执行状态**：本场景已在真实 Mac 通过（用户批准），证据与逐步结果见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)；隔离 home 版本可用 CLI 复跑。下方给出 CLI 与 UI 判定标准。

### 场景 0：环境隔离（必须）

```bash
export AGENTHUB_HOME=/tmp/agenthub-accept/home      # 假 home，不碰真实 Agent 目录
export AGENTHUB_DATA_DIR=/tmp/agenthub-accept/data  # 假数据目录
mkdir -p "$AGENTHUB_HOME/.codex" "$AGENTHUB_HOME/.trae-cn" "$AGENTHUB_HOME/.zcode" "$AGENTHUB_HOME/.dsh"
```

CLI 验收时加 `--home /tmp/agenthub-accept/home`（等价于 `AGENTHUB_HOME`），并**不要**把项目根设到真实项目。注意：`--home` 只隔离 Agent 目录与数据目录，**不会隐藏 `/Applications` 下真实安装的 app**（`find_app` 固定先查 `/Applications`），因此扫描到的 Agent 可能比假 home 里多；验收时显式指定 agent key（如 `codex`、`dsh`）即可避免误写。

### 场景 1：扫描

1. 在 Settings 添加项目根（可选，用于发现项目级技能）。
2. 运行 `agenthub --home "$AGENTHUB_HOME" skills scan`（或调用 `app_scan`）。
3. 预期：
   - Agent 列表包含 ZCode、TRAE CN、DSH（在假 home 下需先按 §0 建目录/放入 fixture skill）；
   - `ScanReport.total_skills` ≥ 1；无效技能进入 `invalid` 且有错误原因；
   - `scan_results` 刷新（Agents 页每个 root 的数量变化）；
   - **没有**任何 Agent 目录被修改（可用 `find $AGENTHUB_HOME -newer marker` 对照）。

判定：扫描前后 `~/.codex`、`~/.trae-cn`、`~/.zcode`、`~/.agents` 的 mtime 与内容不变。

### 场景 2：导入

1. 从扫描结果选择一个 ZCode/TRAE/DSH 技能（或手动选一个含 `SKILL.md` 的目录）。
2. 运行 `agenthub --home "$AGENTHUB_HOME" skills import --path <技能目录>`（CLI 目前支持 folder/zip/SKILL.md/git；`AgentDir` 来源仅 Tauri 命令可用）。
3. 预期：
   - 返回 `created=true`，`version.contentHash` 为 64 位 hex；
   - 库目录出现 `library/skills/<slug>/<hash12>/SKILL.md`；
   - `capability_versions` 多一行，`capability_sources` 记录 `agent_dir`；
   - 源目录未被移动/修改。

### 场景 3：分发到两个 Agent

1. 选择版本，目标 Agent = TRAE CN + DSH（两者都有 `install_roots`）。
2. 运行 `agenthub --home "$AGENTHUB_HOME" skills install <slug> --agents trae-cn,dsh --dry-run` → 得到计划；预期 items 的 `action` 为 `create`（或 `noop`/`conflict` 并带 warning）。
3. 去掉 `--dry-run`、加 `--yes` 执行。
4. 预期：
   - `~/.trae-cn/skills/<slug>/SKILL.md` 与 `~/.dsh/skills/<slug>/SKILL.md` 存在；
   - 目录 hash 与 `version.contentHash` 一致；
   - `installations` 两行，`strategy` 与计划一致；
   - `operations.status=success`，每个 `ItemResult.status=success`。

### 场景 4：修改库版本看到变更

1. 修改源技能内容（例如改 `description`），重新导入 → 生成**新版本**，当前版本指向它。
2. 预期：
   - Skill 列表 `update_available=true`（已部署版本 ≠ 当前版本）；
   - 已部署目录仍是旧内容（不会自动更新）；
   - Dashboard 的 `pending_updates` 计数 +1。

### 场景 5：失败一键恢复

1. 制造失败：把第二个目标目录替换为不可写/被占用的路径（在隔离 home 下用 `chmod 500` 模拟），或让预检报外部修改。
2. 执行 apply。
3. 预期：
   - 返回 `failed`，`diagnostics` 指明失败项；
   - 第一个已成功的目标被**自动恢复**到操作前内容（对比快照 hash）；
   - 若操作已 `success` 后发现漂移，运行 `agenthub --home "$AGENTHUB_HOME" skills rollback last`（或 `operations_rollback`）可恢复到操作前；注意：若目标在操作后被外部修改，回滚会拒绝覆盖该目标并把回滚标记为 `partial`（`ops.rs:697-710`）。
   - 原操作状态变为 `rolled_back`，新建 rollback 操作记录成功项。

### 场景 6（可选）：MCP 计划

- 调用 Tauri 命令 `mcp_upsert` 创建 stdio server（CLI 无创建命令），再 `agenthub mcp distribute <name> --agents codex --dry-run` 生成计划。
- 预期：计划可生成；已有配置文件时 preflight 按**条目 hash** 比较，无需 `--force`（只有真实外部修改/冲突才需要确认）。写入后应验证：无关 TOML 段/注释保留、第二个 server 不覆盖第一个、`operations` 记录 success、MCP 安装行落库。

### 场景 7：验证平台（第二轮）

1. `agenthub health` → 能力健康概览；未部署技能为 `not_deployed`。
2. `agenthub skills audit <slug>` → finding/status/risk；`fail` 退出码 2。
3. `agenthub skills deps <slug>` → runtime/manifest 检查；`needs_attention` 退出码 2；MCP 为 unknown。
4. 导入 v2 → `agenthub skills impact <slug> --to <v2>` → classes/风险/`requiresRevalidation`；分发预览 warnings 含 impact。
5. `agenthub skills validate <slug> --agent codex --case <case> --status passed` → 记录证据；`agenthub health` 中该能力仍按安装/审计/依赖推导（未部署时不会变 verified）。
6. `agenthub health --agents --record` → Agent 版本首次 baseline；再次版本变化时 `changed=true` 并提示重新扫描/验证。

---

## 7. 成功标准（MVP）

| 维度 | 标准 | 当前 |
|---|---|---|
| 覆盖 | 至少 3 个本机 Agent 可检测、可读、可分发 | 代码支持 9 个；本机证据覆盖 6 个 |
| 安全 | 不覆盖非托管目标；任何失败可恢复；不读取凭据文件内容 | 代码具备；MCP env 计划已脱敏；真实 Mac 验收未覆盖失败注入 |
| 可解释 | 每次操作有计划、结果、快照、错误原因 | 具备（缺 import/scan 记录） |
| 可用 | GUI 完成「扫描 → 导入 → 分发 → 更新 → 回滚」 | **已达成（人工验收）**：真实 Mac CLI 全流程 + 隔离 home GUI；自动化 GUI 回归待补 |
| 可验证 | 安装 ≠ 可用；有审计/依赖/人工证据/版本健康 | **已达成**：DB v2 + `/health` 页 + SkillDetail 四区块 + CLI；真实 Mac validator 验收通过 |
| 可解释 | 每个 finding 有 rule id/severity/category/path/suggestion；每个 health 有 reasons | 已实现（见 VALIDATION.md） |
| 诚实 | 未实现/未验证能力在 UI 与文档中明确标注；人工证据不冒充自动通过 | 文档与 UI 文案已标注；规则引擎/人工证据限制已公开 |

---

## 8. 风险与开放问题

| 问题 | 影响 | 处理 |
|---|---|---|
| ~~MCP 执行路径缺陷~~ | 已修复：条目级 preflight + 执行接通 | 回归测试已覆盖；继续补 MCP 回滚测试 |
| 签名/公证未完成 | app 与 DMG 已构建/安装；未签名/未公证 | 配置 Developer ID + notarization |
| 真实 Agent 目录写入风险 | 误写可能影响用户日常使用 | 默认隔离 home；真实验收使用唯一 slug、目标不存在才执行、失败清理，并已获用户授权 |
| Adapter 证据漂移 | 私有格式变化导致写入无效 | 文档标注证据日期；codec 独立测试 |
| DSH symlink 声明与证据不一致 | 默认 copy，浪费已验证能力 | 证据复核后改 `Supported` |
| Keychain 授权 | `security` CLI 首次可能弹授权/拒绝 | 诊断页显示 `keychainAvailable`；失败给出提示 |
| 静态审计误报/漏报 | 规则引擎无法覆盖所有恶意模式 | 明确「启发式、非完备安全证明」；保留 finding 可解释性；后续规则演进 |
| 人工证据可信度 | evidence/notes 是用户输入，可被夸大/伪造 | UI 明确「人工证据不是自动通过」；不据此做安全认证；后续可加 hash/附件 |
| Agent 版本探测失败 | 非标准 bundle/CLI 可能无版本 | 显示未知，不猜测；首次 baseline 不算变更 |
| same-second 版本排序 | 同一秒多次观测可能取错行 | 已修复：`detected_at DESC, rowid DESC` + 回归测试 |

## 9. 已验证 / 未验证 / 已知限制

- **已验证**：真实 Mac 安装/分发/更新/回滚验收；GUI（隔离 home）运行验收；真实 Mac validator 验收（DB v1→v2、真实 Agent 版本、audit/deps/impact/validate）；core 76 + CLI 16 = 92 个 Rust 测试、前端 29 个测试、`pnpm build`、完整 `pnpm tauri build`。
- **未验证**：签名/公证（DMG 已产出）、GUI 自动化、真实 Keychain 正向写入、真实 MCP 配置写入、并发/中断、Windows/Linux、部分 adapter 路径与 symlink 行为。
- **已知限制**：静态审计是启发式规则非完备证明；MCP/凭据静态 unknown；封闭 Agent 只能人工证据且证据为用户输入；版本探测依赖 app bundle/CLI；插件只读、不试连 MCP；备份恢复未实现；MCP headers 明文；详见 §8、[VALIDATION.md](VALIDATION.md) 与 [SECURITY.md](SECURITY.md)。
