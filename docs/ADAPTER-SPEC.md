# AgentHub 适配器规范（Adapter Spec）

> 状态基准：2026-10-20，以 `crates/agenthub-core/src/adapters/mod.rs` 与 `adapters/builtin.rs` 为准；路径证据来自 [AGENT-COMPATIBILITY.md](AGENT-COMPATIBILITY.md) 与 [compat-evidence/](compat-evidence/README.md)。
> 原则：**没有证据的接口不得虚构**。适配器只能声明代码里真实存在、且能被证据支持的能力；未验证的一律标 `Unverified` / `VerificationRequired`。

---

## 0. 结论摘要

1. Adapter 是**只读描述层**：只回答「检测信号、skill 根、安装根、MCP 目标、项目根、插件发现、symlink/插件支持」，不执行写入（`adapters/mod.rs:1-8`）。
2. 9 个内置适配器：Codex、ChatGPT、TRAE (International)、TRAE CN、ZCode、WorkBuddy、DSH、Claude Code、Cursor（`builtin.rs:22-34`）。
3. **真正具备可用安装根**的只有：Codex、TRAE、TRAE CN、ZCode、WorkBuddy、DSH；ChatGPT 明确 `Unsupported`；Claude Code / Cursor 为 `VerificationRequired`（安装路径存在但未验证）。
4. 写入统一由 `ops`/`mcp` 完成；adapter 不碰文件。因此「新增 Agent」= 实现 trait + 注册 + 补证据 + 补测试，不需要改执行器。
5. **已验证实现**：DSH 的 `symlink_support` 已按本机证据改为 `Supported`（`builtin.rs:725-729`），因此 DSH 目标使用 symlink 时不会再被降级为 copy。
6. **未闭合风险**：Codex 的 `detect()` 只检查 `~/.codex` 或 `Codex.app` 是否存在，**没有按 `CFBundleName` 区分** ChatGPT.app 与 Codex.app；兼容性报告 §0.8 指出的同 bundle id 风险尚未在代码里闭合（`builtin.rs:118-125`）。

---

## 1. Trait 接口与语义

定义位置：`crates/agenthub-core/src/adapters/mod.rs:108-173`。

```rust
pub trait AgentAdapter: Send + Sync {
    fn id(&self) -> &'static str;          // 稳定适配器 id，写入 agents.adapter_id
    fn key(&self) -> &'static str;         // 稳定 Agent key，写入 agents.key
    fn name(&self) -> &'static str;        // 展示名
    fn vendor(&self) -> &'static str;
    fn symlink_support(&self) -> SymlinkSupport;
    fn plugin_support(&self) -> PluginSupport;
    fn official_url(&self) -> Option<&'static str> { None }
    fn requires_restart_note(&self) -> Option<&'static str> { None }
    fn detect(&self, home: &Path) -> Detection;
    fn skill_roots(&self, home: &Path) -> Vec<PathBuf>;
    fn install_roots(&self, home: &Path) -> Vec<PathBuf> { self.skill_roots(home) }
    fn mcp_target_specs(&self, home: &Path) -> Vec<McpTargetSpec>;
    fn project_skill_roots(&self, project: &Path) -> Vec<PathBuf> { Vec::new() }
    fn project_mcp_target_specs(&self, project: &Path) -> Vec<McpTargetSpec> { Vec::new() }
    fn discover_plugins(&self, home: &Path, agent_id: &str) -> Vec<DiscoveredPlugin> { Vec::new() }
    // 提供默认实现：
    fn resolved_skill_roots(&self, home: &Path, record: Option<&AgentRecord>) -> Vec<SkillRootSpec>;
    fn resolved_mcp_targets(&self, home: &Path) -> Vec<McpTargetSpec>;
}
```

### 1.1 方法逐个说明

| 方法 | 语义 | 调用方 | 约束 |
|---|---|---|---|
| `id` | 适配器稳定标识（`builtin.codex`） | `find_adapter`、DB `adapter_id` | 一经发布不得改名；改 id 会让已有 `agents.adapter_id` 找不到适配器 |
| `key` | Agent 稳定 key（`codex`） | DB 唯一键、`find_adapter` 回退匹配、profile 的 `target_agents` | 同上；profile 存的是 key |
| `name`/`vendor` | 展示信息 | `AgentView`、`doctor` | 可改文案，不可用于逻辑 |
| `detect(home)` | 判断是否安装，返回 `Detection{status, app_path, home_path, evidence, notes}` | `sync_agents`、`list_agents`、`doctor`、`plan_install` | **只能读**；`home` 可注入（测试/`AGENTHUB_HOME`） |
| `skill_roots(home)` | 扫描根（可含只读内置目录） | `scan`、`list_agents` | 可以不存在；扫描时跳过 |
| `install_roots(home)` | 允许**新建**技能目录的根 | `pick_install_root`（`service.rs:1097`） | 必须排除只读目录（内置技能、marketplace 缓存） |
| `mcp_target_specs(home)` | 全局 MCP 配置文件与其格式/传输 | `list_agents`、`mcp::discover`、`mcp::plan_distribute` | `format` 必须是 codec 支持的字符串；`transports` 必须真实支持 |
| `project_skill_roots(project)` | 项目级扫描根 | `scan`、`list_agents` | 仅用于已登记 `project_roots` 的项目 |
| `project_mcp_target_specs(project)` | 项目级 MCP 目标 | `mcp` 发现/分发 | 同上 |
| `discover_plugins` | 只读插件清单 | `sync_plugins` | 只允许 `DiscoveryOnly`/`FileManaged`/`ManualOnly` 调用（`service.rs:1634-1657`） |
| `symlink_support` | symlink 安装是否被目标 Agent 跟随 | `plan_install`（`service.rs:950-964`） | `Unsupported` → 强制降级 copy；`Unverified` → 保留 symlink 但加 warning |
| `plugin_support` | 插件管理等级 | 插件页、`sync_plugins` | 见 §3 |
| `requires_restart_note` | 写文件后是否需要重启/新会话 | `OperationPlan.requires_restart_agents` | 仅文案，不执行重启 |
| `official_url` | 官方文档链接 | 插件页/详情 | 只用于外链 |

### 1.2 默认实现的两个组合方法

```text
resolved_skill_roots(home, record):
  adapter.skill_roots(home)  → 每条记为 (scope=global, source="adapter")
  + record.custom_skill_roots → 去重后记为 (scope=custom, source="custom")

resolved_mcp_targets(home):
  = mcp_target_specs(home)
```

实现：`adapters/mod.rs:143-172`。项目级根由 `service.rs`/`scan.rs` 在调用侧追加（`scope=project, source="project"`），不在默认实现里。

### 1.3 数据结构约束

| 结构 | 关键字段 | 校验 |
|---|---|---|
| `Detection` | `status`、`app_path`、`home_path`、`evidence`、`notes` | `evidence` 必须是真实存在的路径字符串；`NotInstalled` 用 `Detection::not_installed` |
| `SkillRootSpec` | `path`、`scope`、`source` | `source` 目前只会是 `adapter`/`custom`/`project`；**没有 adapter 产出 `builtin`**，所以 `scan.rs:106-108` 的只读提示永远不会触发（已知缺陷） |
| `McpTargetSpec` | `path`、`format`、`scope`、`transports`、`notes` | `to_model()` 用文件父目录判断 `writable`（`adapters/mod.rs:77-92`）；父目录不存在时 `writable=false` |
| `SkillRoot` | 由 `SkillRootSpec::to_model(skill_count)` 生成 | `exists`/`writable` 是实时探测，不是缓存 |

---

## 2. 能力等级定义

### 2.1 安装能力（本实现的实际分级）

| 等级 | 定义 | UI 允许 | 当前 Agent |
|---|---|---|---|
| **native**（可编程接口） | 目标 Agent 提供官方 CLI/API 管理 skill/MCP，AgentHub 可调用而非直接写文件 | 尚未实现（没有任何适配器实现 CLI 调用） | 证据上 Codex、DSH 具备，但代码未接入 |
| **file**（文件式） | 有稳定文件路径，AgentHub 可原子写/复制，重启后生效 | 安装、卸载、预览 | Codex、TRAE、TRAE CN、ZCode、WorkBuddy、DSH |
| **manual**（手动） | 无本地文件入口，只能在目标应用内操作 | 只读展示 + 引导/导出 | ChatGPT（`plugin_support=ManualOnly`） |
| **discovery-only**（只读发现） | 能发现，但不写入目标 | 展示 | TRAE/TRAE CN/WorkBuddy/DSH/Claude Code 的插件；技能仍可 file 安装 |
| **unsupported** | 明确不支持 | 灰置/隐藏写入入口 | ChatGPT 的本地 skill/MCP |
| **verification-required** | 路径/行为只有间接证据，需实测 | 禁用或强提示后允许 | Claude Code、Cursor（`AgentStatus::VerificationRequired`） |

> 注意：`plan_install` 把 `Installed` **和** `VerificationRequired` 都视为可安装（`service.rs:966-968`）。因此 Claude Code / Cursor 目前**会真的写入** `~/.claude/skills` / `~/.cursor/skills`，只是检测状态标为「需验证」。这是产品决策点，必须在 UI 上明确提示。

### 2.2 symlink 支持

| 值 | 含义 | 代码来源 |
|---|---|---|
| `Supported` | 有直接证据表明目标 Agent 会跟随 symlink | 当前没有任何适配器使用（DSH 证据已 VERIFIED，但代码仍写 `Unverified`） |
| `Unsupported` | 明确不跟随 | ChatGPT |
| `Unverified` | 无直接证据；创建后需人工确认发现结果 | Codex、TRAE、TRAE CN、ZCode、WorkBuddy、DSH、Claude Code、Cursor |

`plan_install` 的行为：`Unsupported` 自动改 copy 并加 warning；`Unverified` 保留 symlink，但 warning 明确要求人工确认；`Supported` 直接执行（`service.rs:949-965`）。

### 2.3 插件支持

| `PluginSupport` | 语义 | 当前 Agent |
|---|---|---|
| `NativeManaged` | 有官方插件管理接口 | 无（未实现） |
| `FileManaged` | 可通过配置文件启用/禁用 | Codex |
| `DiscoveryOnly` | 只读发现 | TRAE、TRAE CN、WorkBuddy、DSH、Claude Code |
| `ManualOnly` | 只能应用内手动管理 | ChatGPT |
| `Unsupported` | 无插件概念 | 无 |
| `Unverified` | 未确认 | ZCode、Cursor |

---

## 3. 当前 9 个适配器矩阵（代码事实）

> 「安装根」一列决定 `plan_install` 的落点；「MCP 目标」一列决定能否分发 MCP；「symlink」列是代码声明值，不是实测值。

### 3.1 路径与能力

| # | key / id | 展示名 | detect 信号（代码） | skill 扫描根（全局） | 安装根 | MCP 目标（全局 / 项目） | symlink | plugin | 证据 |
|---|---|---|---|---|---|---|---|---|---|
| 1 | `codex` / `builtin.codex` | Codex | `~/.codex` 或 `/Applications/Codex.app` | `~/.codex/skills`、`~/.agents/skills`（共享根，仅扫描） | `~/.codex/skills` | `~/.codex/config.toml`（`toml:mcp_servers`，stdio）；无项目目标 | Unverified | FileManaged | [§3.1](AGENT-COMPATIBILITY.md)、[codex evidence](compat-evidence/codex/README.md) |
| 2 | `chatgpt` / `builtin.chatgpt` | ChatGPT | `/Applications/ChatGPT.app` | 无 | 无 | 无 | Unsupported | ManualOnly | [§3.2](AGENT-COMPATIBILITY.md)、[chatgpt evidence](compat-evidence/chatgpt-desktop/README.md) |
| 3 | `trae` / `builtin.trae` | TRAE (International) | `~/.trae` 或 `/Applications/Trae.app` | `~/.trae/skills`、`~/.trae/builtin_skills`；项目 `.trae/skills` + `.agents/skills` | `~/.trae/skills` | 全局无；项目 `<project>/.trae/mcp.json`（`json:mcpServers`，sse/streamable_http） | Unverified | DiscoveryOnly | [§3.4](AGENT-COMPATIBILITY.md)、[trae-intl evidence](compat-evidence/trae-intl/README.md) |
| 4 | `trae-cn` / `builtin.trae-cn` | TRAE CN | `~/.trae-cn` 或 `/Applications/Trae CN.app` | `~/.trae-cn/skills`、`~/.trae/skills`（peer）、`~/.agents/skills`、`~/.trae-cn/builtin_skills`、`~/.trae-cn/work/<workspace>/<skill>`（仅有技能时） | `~/.trae-cn/skills` | 全局 `~/Library/Application Support/Trae CN/User/mcp.json`（`json:mcpServers`，sse/streamable_http）；项目 `<project>/.trae/mcp.json` | Unverified | DiscoveryOnly | [§3.3](AGENT-COMPATIBILITY.md)、[trae-cn evidence](compat-evidence/trae-cn/README.md) |
| 5 | `zcode` / `builtin.zcode` | ZCode | `~/.zcode` 或 `/Applications/ZCode.app` | `~/.zcode/skills` | `~/.zcode/skills` | 全局 `~/.zcode/cli/config.json`（`json:mcp.servers`，stdio/http/streamable_http）；项目仅 `<project>/.agents/mcp.json`（`json:mcpServers`，代码当前未声明 `.zcode/mcp.json`） | Unverified | Unverified | [§3.5](AGENT-COMPATIBILITY.md)、[zcode evidence](compat-evidence/zcode/README.md) |
| 6 | `workbuddy` / `builtin.workbuddy` | WorkBuddy | `~/.workbuddy` 或 `/Applications/WorkBuddy.app` | `~/.workbuddy/skills`、`~/.workbuddy/skills-marketplace/skills`、`~/.workbuddy/connectors/skills` | `~/.workbuddy/skills` | 全局 `~/.workbuddy/connectors/default/mcp.json`（`json:mcpServers`，http/streamable_http/sse）；无项目目标 | Unverified | DiscoveryOnly | [§3.6](AGENT-COMPATIBILITY.md)、[workbuddy evidence](compat-evidence/workbuddy/README.md) |
| 7 | `dsh` / `builtin.dsh` | DeepSeek Harness | `~/.dsh` 或 `/Applications/DeepSeek Harness.app` | `~/.dsh/skills`、`~/.agents/skills` | `~/.dsh/skills` | **无**（DSH MCP 是 Cordis YAML，未实现 codec） | **Supported** | DiscoveryOnly | [§3.7](AGENT-COMPATIBILITY.md)、[dsh evidence](compat-evidence/dsh/README.md) |
| 8 | `claude-code` / `builtin.claude-code` | Claude Code | `~/.claude` 或 `~/.claude.json` → 状态恒 `VerificationRequired` | `~/.claude/skills` | 默认同 skill roots | 全局 `~/.claude.json`（`json:mcpServers`，stdio/http）；项目 `<project>/.claude/skills` | Unverified | DiscoveryOnly | [§3.8](AGENT-COMPATIBILITY.md) |
| 9 | `cursor` / `builtin.cursor` | Cursor | `~/.cursor` 或 `/Applications/Cursor.app` → 状态恒 `VerificationRequired` | `~/.cursor/skills` | 默认同 skill roots | 全局 `~/.cursor/mcp.json`（`json:mcpServers`，stdio/http） | Unverified | Unverified | [REFERENCE-STUDY §3.7](REFERENCE-STUDY.md)（无本机证据） |

### 3.2 项目级路径

| Agent | `project_skill_roots` | `project_mcp_target_specs` |
|---|---|---|
| Codex | 无（未声明） | 无（未声明） |
| ChatGPT | 无 | 无 |
| TRAE | `<project>/.trae/skills`、`<project>/.agents/skills` | `<project>/.trae/mcp.json` |
| TRAE CN | `<project>/.trae/skills`、`<project>/.agents/skills` | `<project>/.trae/mcp.json` |
| ZCode | `<project>/.zcode/skills` | `<project>/.agents/mcp.json`（代码现状；官方原生项目配置见 REFERENCE-STUDY §3.4，尚未建模） |
| WorkBuddy | `<project>/.workbuddy/skills` | 无 |
| DSH | `<project>/.dsh/skills`、`<project>/.agents/skills` | 无 |
| Claude Code | `<project>/.claude/skills` | 无 |
| Cursor | 无 | 无 |

> Codex 未声明 `<project>/.codex/skills` 与项目 MCP，原因是本机证据只到「全局根 VERIFIED、项目根 VR」（AGENT-COMPATIBILITY §2.1）。这是「证据不足就不声明」的示例。

---

## 4. 新增 Agent 的步骤与验收清单

### 4.1 步骤

1. **取证**：先更新 [AGENT-COMPATIBILITY.md](AGENT-COMPATIBILITY.md)（该文件由调研同事维护）与 `docs/compat-evidence/<agent>/README.md`，写清：
   - 检测信号（app bundle / CLI / 配置目录），以及**同名 app 的区分方式**；
   - 全局与项目级 skill 根，哪个是**可写安装根**；
   - MCP 文件路径、顶层键、支持传输；
   - symlink / 热加载 / 插件能力的证据等级。
2. **实现**：在 `adapters/builtin.rs` 增加 `struct XxxAdapter`，实现 `AgentAdapter`：
   - `detect()` 必须返回 `evidence`（真实存在路径）；只有间接线索时返回 `Detection` 但把状态设为 `VerificationRequired`；
   - `skill_roots()` 与 `install_roots()` 分离，只读目录不要放进 install_roots；
   - `mcp_target_specs()` 的 `format` 必须落在已支持 codec 内（`json:*` 或 `toml:mcp_servers`），传输必须真实。
3. **注册**：加入 `all_adapters()`（`builtin.rs:22-34`）。`find_adapter()` 同时按 `key`/`id` 匹配（`builtin.rs:36-38`）。
4. **文档**：更新本文件 §3 矩阵；在 `docs/AGENT-COMPATIBILITY.md` 补证据链接；更新 README 功能状态。
5. **测试**：至少覆盖
   - `detect()` 对「装了 / 没装 / 只有历史配置」三种 fixture 的返回；
   - `resolved_skill_roots()` 去重与 custom root 追加；
   - `mcp_target_specs()` 的 format/transport 与真实文件样例（用 `docs/compat-evidence/` 的脱敏片段）。
6. **验收**：`./scripts/cargo.sh test -p agenthub-core` 通过；`doctor()` 能列出该 Agent 的 evidence；在测试 home（`AGENTHUB_HOME`）下 import → plan → apply → rollback 全链路可跑（不碰真实 Agent 目录）。

### 4.2 验收清单

- [ ] `id`/`key` 全小写、稳定、唯一；`key` 与已有 profile 的 `target_agents` 兼容。
- [ ] `detect()` 不读取凭据文件内容；不写任何文件。
- [ ] `install_roots()` 不含只读/内置目录。
- [ ] MCP `format` 有对应 codec 测试；不支持时明确不声明（不要声明后再报 unsupported）。
- [ ] `symlink_support` 与证据等级一致；无证据一律 `Unverified`。
- [ ] 重启提示与实际（热加载/需重启）一致，不夸大。
- [ ] 文档矩阵、证据文件、README 状态表三处同步。
- [ ] 不引入未经验证的「native 管理」声称（当前没有适配器调用任何 Agent CLI）。

---

## 5. 禁止虚构接口的规则

1. **Trait 是封闭集合**：新增能力必须改 trait 并说明调用方/执行方；不允许在 adapter 里偷偷执行写入。
2. **路径必须有证据**：没有本机或官方文档证据的路径，不写进 `skill_roots`/`mcp_target_specs`；可以写进 `notes` 作为待验证项。
3. **状态必须诚实**：`Supported` 只用于有直接证据的项；`VerificationRequired` 不要伪装成 `Installed`。
4. **不要复用 `find_app` 猜测同名应用**：`find_app` 只判断路径存在（`adapters/mod.rs:176-197`），不读 `CFBundleName`；对同 bundle id 的 app（ChatGPT/Codex）必须先解析 Info.plist 或使用配置目录作为主信号，否则视为未实现。
5. **不要声称「已支持某 Agent 的 MCP」而没有 codec 测试**：当前 DSH 的 Cordis YAML、TRAE 的 `User/mcp.json`（全局）都未声明，这是刻意留白。
6. **不要在 adapter 里写死用户 home**：所有路径必须从传入的 `home` 派生，测试才能隔离（`adapters/mod.rs:199-202`）。

---

## 6. MCP 格式 codec 与保真写入

### 6.1 支持的格式

| format 字符串 | 顶层键/路径 | 解析器 | 写回器 | 用于 |
|---|---|---|---|---|
| `json:mcpServers` | `mcpServers` | `serde_json`（`read_entries`） | `serde_json::to_string_pretty` + 原子写 | Codex 项目、TRAE、WorkBuddy、Claude、Cursor |
| `json:mcp.servers` | `mcp.servers` | 同上（按 `.` 拆段） | 同上 | ZCode 全局 |
| `json:<任意路径>` | 任意点路径 | 同上 | 同上 | 预留扩展 |
| `toml:mcp_servers` | `[mcp_servers.<name>]` | `toml_edit` | `toml_edit` + 原子写 | Codex 全局 |

实现：`mcp.rs:23`、`mcp.rs:111-122`、`mcp.rs:142-258`。`supported_format()` 接受任何 `json:` 前缀，因此新 JSON 键路径无需改代码；TOML 只有固定一种。

### 6.2 条目构建规则（`mcp.rs:426-471`）

| 传输 | 写入字段 | 说明 |
|---|---|---|
| `stdio` | `command`、`args`（非空才写）、`env`（有 Keychain 值才写） | 不写 `type` |
| `http` | `url`、`type: "http"`、`headers`（非空才写） | JSON 目标 |
| `sse` | `url`、`type: "sse"`、`headers` | JSON 目标 |
| `streamable_http` | `url`、`type: "streamable-http"`、`headers` | JSON 目标 |
| 任意远程写 TOML | `url`、`type: "streamable-http"`、`headers` | `type_for()` 对 TOML 固定返回 `streamable-http`（`mcp.rs:417-424`） |

`env` 的值来自 Keychain；`headers` 的值来自 `mcp_definitions.headers`（明文）。**没有脱敏层**。

### 6.3 保真写入的边界（重要）

| 场景 | 行为 | 保真程度 |
|---|---|---|
| JSON 文件已有其他键 | 反序列化整份 → 改目标路径 → pretty 序列化 | 键/值保留，**缩进、空行、注释、键顺序可能变化**（`preserve_order` 只保插入顺序） |
| JSON 文件语法损坏 | `read_entries` 报错；写回前再次解析失败则拒绝覆盖（`mcp.rs:213-216`） | 不会破坏原文件 |
| TOML 文件已有注释/其他表 | `toml_edit` 原地修改 `mcp_servers` 表 | 未触及部分**保留注释与格式**（`mcp.rs:230-253`） |
| TOML 文件语法损坏 | 拒绝覆盖并返回错误 | 不会破坏原文件 |
| 删除单个 entry | 读全量 → 过滤该 key → 写回；`write_entries` 会清理不在列表中的 key（JSON `retain` / TOML `table.remove`）（`mcp.rs:203-300`） | 实测可真正删除；TOML 会留下空 `[mcp_servers]` 表，JSON 留空 `servers` 对象 |
| 目标文件是 symlink | 快照阶段记链接；`apply_mcp_item` 会先 `remove_path` 再写（`ops.rs:422-426`） | 链接被替换为普通文件（行为需在测试中固定） |
| 目标父目录不存在 | `plan_distribute` 不会选该目标（要求父目录存在且可写，`mcp.rs:834-842`） | 不创建目录 |
| 未知字段透传 | JSON 条目整体保留（payload 是完整 entry）；TOML 由 `toml_edit` 保留未改表 | 条目内未知键会保留；**但 AgentHub 内部模型不解释它们** |

> 结论：**JSON 是「结构保真、格式不保真」；TOML 是「结构 + 注释保真」**。文档与 UI 不应承诺字节级保真。

### 6.4 当前 codec 的未验证点

| 项 | 风险 |
|---|---|
| WorkBuddy 远程条目字段 | 本机证据使用 `staticHeaders`（[AGENT-COMPATIBILITY §3.6](AGENT-COMPATIBILITY.md)），codec 写 `headers`；字段名可能不被识别 |
| ZCode 远程条目 `type` | 本机证据没有显式 `type` 字段，codec 会写入 `type`；是否被忽略未知 |
| TRAE 远程传输 | 证据未区分 SSE / streamable HTTP；codec 按传输写 `type`，但 TRAE 可能不读 |
| DSH | 无 codec；MCP 完全不可分发 |
| 全局 TRAE MCP | 未建模（只有项目级 `.trae/mcp.json`） |
| ChatGPT | 无本地配置，永不写入 |

---

## 7. 已知未验证项（按 Agent）

| Agent | 未验证/限制 | 建议验证方式 |
|---|---|---|
| Codex | 项目级 `<project>/.codex/skills` 与项目 MCP 未声明；ChatGPT.app/Codex.app 同名 bundle 未区分；热加载行为未验证 | 临时 `CODEX_HOME` + `/tmp` 项目探针（见 [symlink-test-plan](compat-evidence/symlink-test-plan.md)） |
| ChatGPT | 仅检测 app 存在；不读账号状态；无 skill/MCP | 无（设计上不写） |
| TRAE | 国际版全局 MCP 未建模（CN 已建模 `Library/Application Support/Trae CN/User/mcp.json`）；项目根按本机证据用 `.trae/skills` + `.agents/skills`；国际版 app 未安装 | GUI 打开 `/tmp` 项目；检查 `.trae/mcp.json` 是否生效 |
| TRAE CN | 同上；`~/.trae-cn/work/*` 作为扫描根是启发式；`installed-plugins.json` 不代表本地已安装 | GUI + `.skill-list-cache` 对照 |
| ZCode | 远程 `type` 字段语义；全局根当前为空；代码只声明项目 `.agents/mcp.json`，官方 `.zcode/config.json` / `.zcode/mcp.json` 尚未建模 | GUI 打开 `/tmp` 项目 |
| WorkBuddy | `staticHeaders` vs `headers`；项目根未声明（证据 VR）；MCP 生效方式 | GUI + 缓存文件观察 |
| DSH | symlink 已声明 `Supported`（代码与证据一致）；MCP Cordis YAML 未实现；`~/.dsh/skills` 本机不存在时安装根是否被自动创建 | 新会话观察 catalog + symlink 探针 |
| Claude Code | 状态恒 `VerificationRequired`；安装会真的写 `~/.claude/skills`；`~/.claude.json` 结构与 `mcpServers` 未验证 | 本机未安装，建议先只读 |
| Cursor | 无本机证据；状态 `VerificationRequired`；插件/项目根未声明 | 需安装后实测 |

---

## 8. 已知实现缺陷（与 adapter/MCP 相关）

| 缺陷 | 证据 | 影响 |
|---|---|---|
| MCP 删除留空表/空对象 | `mcp.rs:203-277` | 删除条目后 TOML 保留空 `[mcp_servers]`，JSON 保留空 `servers` 对象（格式噪音） |
| MCP 删除留空表 | `mcp.rs:203-300` | 删除条目后 TOML 保留空 `[mcp_servers]`，JSON 保留空 `servers` 对象（格式噪音） |
| `scan_results.scope` 恒 NULL | `db.rs:1366` | 扫描结果无法按 scope 过滤 |
| Codex 项目级根未声明 | `builtin.rs:127-137` | `<project>/.codex/skills` 不参与扫描/安装（证据仅 VR，属刻意留白） |
| 同名 app 未按 CFBundleName 区分 | `builtin.rs:118-125`、`adapters/mod.rs:176-197` | ChatGPT.app 存在时 Codex 的 app 检测可能被误判（但 `~/.codex` 仍是主信号） |

---

## 8.5 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：trait 契约与执行分离；9 个 adapter 的 id/key/roots/格式由代码定义；Codex/WorkBuddy 的发现与 MCP 写入路径有集成测试；DSH symlink 声明已与证据一致。
- **未验证**：Codex 项目级根、Trae CN 项目根、ZCode 远程传输/原生项目配置、WorkBuddy `staticHeaders`、Claude/Cursor 实际安装、非 DSH symlink 行为。
- **已知限制**：代码不调用任何 Agent CLI（无 native 管理）；ChatGPT 只读；同名 app 未按 CFBundleName 区分；MCP 删除留空表；见 §8。

## 9. 交叉链接

- 兼容性总表与证据：[AGENT-COMPATIBILITY.md](AGENT-COMPATIBILITY.md)、[compat-evidence/README.md](compat-evidence/README.md)
- 规范依据（Agent Skills / MCP / 各 Agent 官方文档）：[REFERENCE-STUDY.md](REFERENCE-STUDY.md)
- 写入事务与快照：[ARCHITECTURE.md §3](ARCHITECTURE.md)
- 安全边界：[SECURITY.md](SECURITY.md)
