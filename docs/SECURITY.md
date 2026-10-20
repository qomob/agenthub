# AgentHub 安全设计

> 状态基准：2026-10-20。以 `crates/agenthub-core/src/util.rs`、`skill.rs`、`ops.rs`、`mcp.rs`、`src-tauri/src/lib.rs`、`src-tauri/tauri.conf.json` 的实现为准。
> 标记：**已验证** = 代码实现存在且本次阅读/实测确认；**未验证** = 代码存在但缺少运行验证；**未实现** = 无代码。

---

## 0. 安全立场

1. **Local First，无云端**：没有账号、遥测、云同步；网络访问只发生在用户主动使用 Git 导入时，由本机 `git` 完成（`skill.rs:462-505`）。核心 crate 不依赖任何 HTTP 客户端（`Cargo.toml`/`crates/agenthub-core/Cargo.toml` 无 reqwest/hyper 等）。
2. **默认不破坏**：不覆盖非 AgentHub 拥有的目标；每次外部写入前先快照；失败逆序恢复；删除只针对已记录的安装。
3. **能力诚实**：未验证的适配器能力标 `Unverified`/`VerificationRequired`，不假装可用（见 [ADAPTER-SPEC.md](ADAPTER-SPEC.md)）。
4. **凭据不是内容**：MCP env 值只存 macOS Keychain，数据库只存 key 名；分发计划 payload 已剥离 env、执行时从 Keychain 重建。headers / 快照仍有明文风险（见 §5）。
5. **数据库不是密钥库**：Library、备份、快照、SQLite 都按普通用户文件保护，没有额外加密。

> 重要：AgentHub 目前**不是 App Sandbox 应用**，没有 entitlements、没有代码签名/公证配置、没有内置提权。它以当前用户权限运行，能读写用户可访问的任意路径——安全边界依赖「路径白名单 + 事务 + 用户确认」，而不是操作系统沙箱。这是 MVP 的明确限制。

---

## 1. 资产与信任边界

| 资产 | 位置 | 敏感度 | 谁可读 |
|---|---|---|---|
| Agent 技能目录 | `~/.codex/skills`、`~/.trae-cn/skills`… | 中（可含脚本/提示词） | 当前用户、Agent 本身 |
| Agent MCP 配置 | `~/.codex/config.toml`、`~/Library/Application Support/Trae CN/User/mcp.json`… | **高（可能含 token/headers）** | 当前用户、对应 Agent |
| Keychain 中的 env 值 | macOS Keychain `com.agenthub.mcp` | **高** | 当前用户（Keychain 授权） |
| AgentHub 数据库 | `~/.agenthub/agenthub.db` | 中高（路径、安装记录、MCP headers、计划 JSON） | 当前用户 |
| 技能库/快照/备份 | `~/.agenthub/{library,snapshots,backups}` | 中高（快照可能复制了含 token 的配置） | 当前用户 |
| 导入暂存 | `~/.agenthub/staging` | 中（解压后的第三方内容） | 当前用户 |

**信任边界**：

- **可信**：AgentHub 自身代码、编译进二进制的 9 个 adapter、本机 `git`、`/usr/bin/security`。
- **不可信**：导入的 zip/Git 仓库/文件夹内容、项目级 skill 目录（可能来自陌生仓库）、Agent 配置文件的当前内容（可能被外部改过）、Agent 写入后的运行时行为。
- **半可信**：用户选择的中央库目录、自定义 skill 根（必须校验绝对路径；当前未校验是否位于白名单内）。

---

## 2. 威胁模型

| # | 威胁 | 攻击面 | 已实现控制 | 剩余风险 |
|---|---|---|---|---|
| T1 | **恶意 zip**（路径穿越、zip bomb、符号链接、超大文件） | `skills import --path *.zip` | 条目数 ≤ 4000；解压总量 ≤ 100MB；NUL 路径拒绝；symlink 条目拒绝；`..`/绝对路径（含 `//`、盘符）拒绝；规范化后必须仍在解压根内（`skill.rs:373-472`、`skill.rs:438-460`、`MAX_ARCHIVE_*` 见 `skill.rs:19-21`） | 未限制压缩比（但有 100MB 解压上限）；未做内容静态扫描；脚本文件会被复制进库 |
| T2 | **符号链接逃逸** | 导入目录/库内容/部署目标 | `hash_dir` 拒绝指向技能根之外的 symlink（`util.rs:65-109`）；`copy_dir` 拒绝复制逃逸链接（`util.rs:199-234`）；`remove_path` 用 `symlink_metadata` 不跟随（`util.rs:183-197`）；部署后校验 symlink 解析在库内（`ops.rs:479-521`） | 目标 Agent 是否跟随 symlink 未逐一验证（只有 DSH 标 `Supported`）；symlink 安装本身可能把库路径暴露给 Agent |
| T3 | **路径穿越 / 任意目标写入** | 计划注入、自定义根、MCP target | 目标必须绝对路径且有父/文件名（`ops.rs:210-222`）；自定义根必须绝对路径（`service.rs:269-276`）；MCP target 来自 adapter 的固定 spec；归档路径规范化 | `target_within_roots()` 存在但**没有调用方**（`ops.rs:870-882`）；当前只靠 adapter/root 生成路径，缺少最后一道白名单断言 |
| T4 | **外部篡改**（用户/IDE 在预览后改了文件） | Skill/MCP 目标 | 计划记录 `current_hash`/`entry_hash`；preflight 拒绝 Create/Replace/Remove 与预期不符的目标；回滚拒绝覆盖操作后被修改的目标（`ops.rs:200-292`、`ops.rs:697-710`） | MCP 已改为按条目 hash 比较（`ops.rs:220-260`）；本次测试以非 force 通过 |
| T5 | **凭据泄露（落库/落盘/日志）** | env/headers、快照、备份、进程参数 | env 值只进 Keychain（`mcp.rs:22-105`）；DB 的 `mcp_definitions.env_keys` 只有 key 名；**分发计划 payload 已剥离 env 值**，执行时才从 Keychain 重建（`mcp.rs:883-890`、`mcp.rs:1111-1122`）；审计日志只记 id/路径/hash（`ops.rs:24-44`） | **headers 值明文入库并写入配置**；`security add-generic-password -w <value>` 会让 secret 短暂出现在进程参数中；快照/备份复制含 token 的配置文件且不加密 |
| T6 | **并发写入** | GUI + CLI 同时操作 | 进程内 `write_lock`；apply/rollback 用 `<library_root>/.agenthub.lock` 跨进程互斥（`ops.rs:35-46`）；SQLite WAL + busy_timeout=5s | import/staging、`mcp_upsert`、`mcp_set_secret`、设置写入没有跨进程锁；两个进程可能同时规划同一目标 |
| T7 | **不可信 Git 仓库** | `skills import --git` | 仅接受 https/http/ssh/git@/file:///绝对路径；浅克隆 `--depth 1 --no-tags`；删除 `.git`；不递归 submodule；`GIT_TERMINAL_PROMPT=0`、`GIT_ASKPASS=true`（`skill.rs:462-505`） | 不校验签名/来源；不限制仓库大小；`git` 本身及全局 config 可信；导入内容可能含恶意提示词/脚本，AgentHub 不执行但会分发 |
| T8 | **UI 注入（XSS/危险链接）** | 技能详情、MCP 原始 JSON、路径 | SKILL.md 用自定义块解析 + 文本插值，**不使用 `v-html`**（`src/lib/markdown.ts`）；Tauri CSP `script-src 'self'`、`style-src 'unsafe-inline'`（`tauri.conf.json`）；外部链接通过 `opener` 打开固定官方 URL | 页面已落地但未做运行时注入验证；`style-src 'unsafe-inline'` 是 Tailwind/内联样式需要；未来若渲染 HTML 或用户 URL 需重新评估 |
| T9 | **恶意/损坏配置** | Skill frontmatter、MCP JSON/TOML、Agent 配置 | YAML 解析失败 → 该技能 invalid，不中断其他扫描；JSON/TOML 解析失败 → **拒绝覆盖**（`mcp.rs:213-216`、`mcp.rs:236-241`）；SKILL.md ≤1MB；正文 >100KB 给 warning | 解析器本身（serde_yaml/serde_json/toml_edit）的漏洞属于依赖风险；未做 fuzz |
| T10 | **误写真实 Agent 目录** | 首次使用、测试 | 默认 UI 预览 + 确认；CLI 默认只打印计划（`--yes` 才执行）；测试应使用 `AGENTHUB_HOME`/`--home` | `--home` 不隐藏 `/Applications`，真实 app 仍会被检测；验证不充分时仍可能向 `~/.claude/skills` 等未验证根写入（Claude/Cursor 为 `VerificationRequired` 但允许安装） |

---

## 3. 已实现控制（按执行顺序）

### 3.1 导入阶段

| 控制 | 实现 | 说明 |
|---|---|---|
| 只接受明确来源 | `ImportSource` 枚举 | 文件夹/单文件/zip/Git/Agent 目录；CLI 额外做 `.zip`/`.md` 判定（`agenthub-cli/src/main.rs:839-883`） |
| 解压边界 | `skill.rs:373-460` | 见 T1 |
| 暂存隔离 | `StagingDir` | 所有导入先落 `staging/staging-<uuid>`，Drop 清理（`skill.rs:341-369`） |
| frontmatter 校验 | `read_skill_dir`（`skill.rs:98-233`） | 必须有 `name`/`description`；name ≤64、desc ≤1024 给 warning；SKILL.md ≤1MB 硬限制 |
| 内容哈希 | `hash_dir`（`util.rs:65-109`） | 排序后的相对路径 + 文件字节 + 链接目标；用于去重与漂移 |
| 不执行内容 | 全流程 | 不运行 skill 脚本、不执行 zip 内文件、不运行 Git hooks（clone 不触发） |

### 3.2 规划阶段

- 目标路径由 adapter 的 `install_roots` 或用户 custom root 拼接 slug（`service.rs:982`、`service.rs:1081-1108`）。
- 对每个目标计算当前 hash，与记录/期望比较，产生 `create/replace/noop/conflict/blocked`（`service.rs:986-1039`、`mcp.rs:890-922`）。
- 计划完整落库，CLI/GUI 展示目标、哈希、警告、重启提示；包含 blocked/conflict 时 CLI 拒绝执行（`agenthub-cli/src/main.rs:531-565`）。

### 3.3 执行阶段（ops）

| 控制 | 实现 |
|---|---|
| 路径合法性 | 绝对路径 + 有父/文件名（`ops.rs:215-230`） |
| 冲突确认 | `Conflict` 无 `force` 直接拒绝（`ops.rs:231-238`） |
| 外部修改 | Create/Replace/Remove 与计划 hash 不符则拒绝（`ops.rs:239-258`） |
| 跨进程锁 | `<library_root>/.agenthub.lock`（`ops.rs:35-46`、`ops.rs:92`、`ops.rs:754`） |
| 快照 | 目录复制/文件复制/链接目标记录/absent 记录（`ops.rs:564-638`） |
| 原子写 | 目录：staging + rename；symlink：先建链接再校验；MCP 文件：临时文件 + fsync + rename（`util.rs:236-294`） |
| 写后校验 | SKILL.md 存在 + hash 匹配 + symlink 指向库内（`ops.rs:479-521`）；MCP 回读条目 hash（`mcp.rs:1098-1106`） |
| 失败补偿 | 逆序 undo 已成功项（`ops.rs:135-166`） |
| 回滚保护 | 只允许 success/partial；同一操作只能回滚一次；目标被外部修改则拒绝覆盖（`ops.rs:666-679`、`ops.rs:717-760`） |

### 3.4 MCP 配置安全

- **格式损坏拒绝覆盖**：写入前解析失败 → 报错，不写（`mcp.rs:213-216`、`mcp.rs:236-241`）。
- **只改目标条目**：读全量 → 替换/删除该 key → 写回；未知键保留；TOML 保留注释与其他表（`mcp.rs:203-300`）。
- **不删除整份文件**：移除单条 entry 不 `remove_file`。
- **Keychain**：`/usr/bin/security` 固定路径；service `com.agenthub.mcp`；account `<server_id>/<key>`；只允许设置已声明的 `env_keys`（`service.rs:1884-1897`）。
- **用户可见风险**：分发带 env 的 server 时，计划项含 warning「值会被写入 Agent 配置文件」（`mcp.rs:926-931`）。

### 3.5 Tauri/前端边界

- 命令层只暴露固定 38 个命令（`src-tauri/src/lib.rs`），全部经过 `AgentHub`，不提供任意 shell/文件读写命令。
- Tauri capabilities 仅 `core:default`、`opener:default`、`dialog:default`（`src-tauri/capabilities/default.json`）。
- CSP：`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'`；无远程脚本。
- 文件选择用 dialog 插件；打开链接/路径用 opener，目标来自 adapter 的固定官方 URL 或用户选择的路径。
- Markdown 渲染：自定义解析器 + 文本插值，杜绝 `v-html`（`src/lib/markdown.ts`）。

---

## 4. 权限最小化

| 维度 | 现状 | 评价 |
|---|---|---|
| 进程权限 | 普通用户，无 root | 合理 |
| 读取范围 | 仅 adapter 声明的 skill 根 + MCP 配置文件 + 用户选择目录 | 基本最小；WorkBuddy 扫描根不含凭据目录 |
| 写入范围 | 安装根 / MCP target / 库目录 / 快照 / 备份 | 缺少 `target_within_roots` 断言 |
| 网络 | 默认 0；仅 Git 导入时调用本机 git | 合理 |
| Keychain | 仅 MCP env；按 server/key 命名 | 合理，但 `security` CLI 会短暂暴露 argv |
| 后台常驻 | 无 watcher、无定时任务 | 无额外攻击面 |
| 自动更新 | 无 | 无供应链更新面 |

---

## 5. 数据流与凭据处理（无云端）

```text
用户选择来源
  → staging（本地临时）
  → 校验/哈希
  → library/skills/<slug>/<hash12>（本地，明文）
  → installations（SQLite，本地）
  → 计划（SQLite operations.plan，本地）
  → 目标 Agent 目录（本地，快照在 snapshots/）

MCP env 值：
  UI 输入 → /usr/bin/security add-generic-password（Keychain）
        → 计划阶段：payload 剥离 env（不落 SQLite 明文）
        → 执行时 get_secret → 写入目标 Agent 配置文件（明文，Agent 需要）
审计日志：
  <library>/logs/operations.log ← 仅 id/kind/status/hash/error，不含 secret
```

**没有**任何数据被发送到 AgentHub 服务器、分析服务或第三方；`git clone` 是唯一出站连接（用户显式触发）。

### 5.1 凭据处理规则（目标态）

1. 数据库只存 `env_keys`（key 名）与 `credential_refs`（引用）——**已实现（env）**。
2. headers 值应视为凭据：不应明文入库；当前**未实现**（见 §6.2）。
3. 日志不得包含 env/headers 值；当前 JSONL 审计只写 id/kind/status/hash/error，不写 secret（见 §6.4）。
4. 备份/快照应排除凭据文件；当前**没有排除**——快照是整目录/整文件复制。
5. UI 只显示 key、不显示 value；MCP 列表展示 `envKeys`，详情页应只显示「已设置/未设置」（`SecretState`）。

### 5.2 本次实测的凭据相关行为

- `mcp_upsert` 后，`mcp_definitions.env_keys` 保存 `["SMOKE_MISSING_KEY"]`；Keychain 未设置时 `mcp_secret_states` 返回 `present=false`，分发计划把目标标为 `Blocked`（临时 home 实测）。
- **已修复**：计划阶段用空 env 构建 `PlanItem.payload`（`mcp.rs:883-890`、`mcp.rs:959`），`operations.plan` 不再含 env 值；执行时 `apply_config_item` 从 Keychain 重建完整 entry（`mcp.rs:1111-1122`），并据此校验 entry hash。

---

## 6. 剩余风险与用户确认点

### 6.1 高风险（需在 UI 明示）

| # | 风险 | 证据 | 建议/状态 |
|---|---|---|---|
| R1 | ~~MCP env 值进入 `operations.plan`~~ **已修复** | `mcp.rs:883-890`、`mcp.rs:959`、`mcp.rs:1111-1122` | 计划 payload 不再含 env；执行时从 Keychain 重建。**headers 值仍会进 payload**（见 R2） |
| R2 | **headers 明文入库/入配置** | `mcp.rs:712`（存 headers）、`mcp.rs:466-474`（写入配置） | headers 可能含 `Authorization`；UI 已警示，但仍应改为 Keychain 引用。当前未修复 |
| R3 | **`security` argv 暴露 secret** | `mcp.rs:43-63`：`security add-generic-password ... -w <value>` | 同用户的其他进程可在极短窗口读到 argv；应改用 stdin/交互输入方式（`security` 支持从 stdin 读取时需验证） |
| R4 | **快照/备份含明文凭据** | `snapshot_target` 整文件复制；`create_backup` 复制 DB | 备份目录应与库同权限保护；文档与 UI 应提示「快照可能含配置中的 token」 |
| R5 | ~~MCP 非 force 被误报外部修改~~ **已修复** | `ops.rs:220-260`、`mcp.rs:1002-1060` | MCP preflight 改为按条目 hash；移除计划记录当前条目 hash。回归测试通过 |
| R6 | **`target_within_roots` 未调用** | `ops.rs:870-882` | 建议在 preflight 中按 adapter install roots 做最终断言 |
| R7 | **VerificationRequired 仍可安装** | `service.rs:966-968` | Claude Code/Cursor 会在未验证时写入 `~/.claude/skills`/`~/.cursor/skills`；UI 必须强提示或禁用 |
| R8 | **Windows/Linux 无验证** | 全部 `std::os::unix::fs::symlink`；Keychain 仅 macOS | 非 macOS 上可能直接失败；MVP 仅支持 macOS Apple Silicon |

### 6.2 中风险

| # | 风险 | 说明 |
|---|---|---|
| R9 | 第三方 skill 无静态审查 | 不扫描脚本/`allowed-tools`/动态注入；分发即把内容交给 Agent。UI 预览应至少展示文件清单与脚本 |
| R10 | 未签名 Git 仓库 | 只做浅克隆，不校验 commit 签名/来源可信度 |
| R11 | 导入大小上限 | zip 100MB / 4000 条目；单个 SKILL.md 1MB；无单文件上限（总解压上限覆盖） |
| R12 | 并发锁缺口 | import/设置/MCP 定义写入没有跨进程锁；GUI+CLI 同时操作可能竞争 |
| R13 | GUI 已人工运行验收 | 隔离 home 下 Dashboard/onboarding 正常（见 ACCEPTANCE-REPORT）；仍无自动化注入/XSS 测试 |
| R14 | 依赖供应链 | rusqlite/toml_edit/serde 等为常规依赖；无 `cargo audit` 记录（本次未执行） |
| R15 | 签名/公证 | DMG 已产出；未签名/未公证；分发给他人前需完成 Developer ID + notarization |

### 6.3 用户确认点（产品必须保留）

1. **每次写入前显示计划**：目标路径、动作、哈希、策略、警告、需要重启的 Agent。
2. **强制覆盖需显式勾选**：conflict/force 场景（CLI `--force`，UI 复选框）。
3. **凭据写入警告**：分发带 env 的 MCP 时提示值会进入目标配置文件。
4. **symlink 未验证警告**：目标 Agent 的 symlink 支持为 `Unverified` 时默认 copy 或明确提示。
5. **VerificationRequired 警告**：Claude Code/Cursor 的安装入口需提示「未验证」。
6. **Keychain 不可用**：`keychainAvailable=false` 时禁用 env 保存并说明原因。
7. **中央库迁移**：切换库路径前提示「会复制现有内容到新目录」。
8. **备份/快照说明**：提示包含配置文件副本，可能含敏感值。

### 6.4 日志脱敏（已实现 JSONL 审计）

`<library_root>/logs/operations.log` 以 JSONL 记录：

- `import`：capabilityId / slug / version / hash；
- `operation.start`：operationId / kind / items / force；
- `operation.finish`：operationId / status / items；
- `operation.failed`：operationId / error。

实现：`ops.rs:24-44`；调用点 `ops.rs:117`、`ops.rs:198`、`ops.rs:235`、`service.rs:853`。

- **原则**：只记录 id、路径、哈希、错误类别；**不记录** env/headers 值、Keychain 输出、SKILL.md 正文。
- **现状**：审计日志已覆盖导入与 operation 生命周期；仍无通用 tracing/文件日志，错误细节以 `operations` 表为准。
- **注意**：`operation.failed` 记录 error 字符串；当前错误消息不含 secret，但未来新增错误分支需继续遵守脱敏规则。

---

## 6.5 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：zip 边界（含绝对路径拒绝）、symlink 逃逸拒绝、原子写、快照/回滚、外部修改检测、Keychain 存储与缺失 blocked、损坏配置拒绝覆盖、MCP env 计划脱敏、JSONL 审计；本次测试、真实 Mac 验收与 GUI 验收覆盖。
- **未验证**：真实 Keychain 正向写入（验收避免污染）、真实 MCP 配置写入、GUI 自动化注入测试、并发/中断/磁盘满、非 macOS、签名/公证（DMG 已产出）。
- **已知限制**：headers 明文、secret argv、快照/备份含凭据、应用未沙箱/未签名、备份恢复未实现、签名/公证未验证（DMG 已产出）；见 §6/§7。

## 7. 安全验收清单

- [ ] 恶意 zip 夹具：`../escape`、绝对路径、symlink 条目、>4000 条目、>100MB 解压 → 全部被拒绝且不写目标。
- [ ] 含逃逸 symlink 的技能目录导入 → `hash_dir`/`copy_dir` 报 blocked。
- [ ] 计划生成后外部修改目标 → apply 报 `ExternalModification`；`--force` 时先快照再覆盖。
- [ ] 回滚时目标被外部修改 → 回滚该项失败、整次 `partial`，不覆盖外部修改。
- [ ] MCP 分发带 env：Keychain 缺失 → blocked；设置后 → 计划含 warning；写入后配置文件含值（已知风险，需产品确认）。
- [ ] MCP 配置文件语法损坏 → 拒绝覆盖，原文件字节不变。
- [ ] 并发：GUI 与 CLI 同时 apply → `.agenthub.lock` 串行，不交叉写。
- [ ] Keychain 拒绝授权 → 错误可读、不泄露值、不写坏配置。
- [x] UI：隔离 home 下 onboarding/Dashboard 正常（截图见 [ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)）；XSS 自动化测试仍未覆盖。
- [x] 真实 Mac：导入 → 分发 → 扫描 inSync → 更新 → 回滚 → 卸载 → 备份/doctor 全流程通过（副本见 `scripts/real-acceptance.sh`）。
- [x] DMG 已产出（`AgentHub_0.1.0_aarch64.dmg`，3.77 MiB）；[ ] 签名/公证待完成。
- [ ] 无网络：断网时除 Git 导入外全部功能可用。

---

## 8. 交叉链接

- 威胁对应的写入实现：[ARCHITECTURE.md §3](ARCHITECTURE.md)
- 适配器能力与未验证项：[ADAPTER-SPEC.md](ADAPTER-SPEC.md)
- 数据落库位置与敏感字段：[DATA-MODEL.md §7](DATA-MODEL.md)
- 测试隔离与安全场景：[TEST-PLAN.md](TEST-PLAN.md)
- 本机证据与脱敏规则：[compat-evidence/README.md](compat-evidence/README.md)
