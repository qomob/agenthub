# AgentHub 验证平台（Validation Platform）

> 版本：0.2.0 开发线
> 状态基准：2026-10-20 16:24（Asia/Shanghai），以 `crates/agenthub-core/src/{audit,deps,impact,validation,agent_health}.rs` 与当前测试为准。
> 一句话：AgentHub 不只回答「装没装」，还回答「能不能用、安不安全、更新后会不会坏、凭什么说它验证过」。

---

## 0. 结论摘要

1. **安装 ≠ 可用 ≠ 已验证**。`installations` 只证明文件存在；`HealthState` 由审计、依赖环境、人工验证证据与当前版本共同推导，安装本身永远不会产生 `verified`。
2. **四层证据互补**：
   - 静态审计（确定性规则，不执行脚本、不联网、无 AI 评分）；
   - 依赖/环境诊断（真实探测运行时，解析 manifest，MCP/凭据保持 unknown）；
   - 更新影响分析（版本 diff + 风险类别 + 受影响安装 + 是否需重新验证）；
   - 人工验证证据（按 Agent/用例记录，明确不是自动通过）。
3. **安全优先**：`audit=fail` 或 `deps=needs_attention` 时，即使人工记录 passed，健康状态仍是 `needs_attention`；只有当前版本、**所有已安装 Agent** 都有 manual passed 证据，才是 `verified`。
4. **UI/CLI 均已落地**：`/health` 页、SkillDetail 四区块、Agents 版本健康、Dashboard/Skills 健康摘要、分发预览影响摘要；CLI `health` 与 `skills audit|deps|impact|validate`。
5. **真实 Mac 验证**：真实 `~/.agenthub` 自动迁移到 DB v2；`health` total=2/not_deployed=2；真实审计 memory-maker = warn/medium、8 findings；deps = healthy（node ok、MCP unknown）；impact = medium/skill_md_changed/requiresRevalidation=true；manual passed 证据写入成功。证据 `/tmp/agenthub-validator-real/`。
6. **测试基线**：core **76 passed**（44 unit + 32 integration，validator 11）、CLI **16 passed**、前端 **7 files / 29 tests**；Rust 合计 92。最新结果见 §11。

---

## 1. 为什么需要验证平台

| 痛点 | 仅靠安装/同步无法解决 | 验证平台如何回答 |
|---|---|---|
| 安装 ≠ 可用 | 文件复制成功不代表 Agent 能触发、脚本有运行时、依赖齐全 | 静态审计 + 依赖诊断 + 人工证据 → `HealthState` |
| 依赖未就绪 | `SKILL.md` 引用了 `python3`/`node`/包，但本机没装 | 真实探测 shebang runtime（`--version`，2s 超时）、解析 requirements/pyproject/package.json |
| 安全未知 | 第三方 skill 可能含 `curl \| sh`、`sudo`、硬编码密钥、外泄 | 确定性规则审计，finding 含 id/severity/category/path/suggestion |
| 更新影响未知 | 新版本可能加脚本、加网络域名、加依赖、改权限 | 版本 diff + classes + 风险 + `requiresRevalidation`，进入分发计划 warnings |
| 「谁验证过」不可追溯 | 口头说能用，换机器/换版本就失效 | `validation_runs` 记录 Agent/用例/状态/证据；旧版本证据自动失效 |
| Agent 升级后静默失效 | App 升级可能改变 skill 加载行为 | `agent_versions` 记录版本变化（首次为 baseline），提示重新扫描/验证 |

**定位边界**：验证平台是「能力控制中心」的验证层，**不重复开发安装/同步**（安装/分发仍由成熟的 adapter + ops 事务负责）。它不做运行时沙箱，也不执行 skill 脚本。

---

## 2. 证据层级

```text
                     ┌──────────────────────────────┐
                     │ Capability Health (derived)  │
                     │ verified / unverified /      │
                     │ needs_attention / failed /   │
                     │ not_deployed                 │
                     └──────────────▲───────────────┘
                                    │ 只读取当前版本证据
        ┌───────────────────────────┼───────────────────────────┐
        │                           │                           │
┌───────┴────────┐        ┌─────────┴─────────┐       ┌─────────┴─────────┐
│ Static Audit   │        │ Dependency/Env    │       │ Manual Evidence   │
│ audit.rs       │        │ deps.rs           │       │ validation_runs   │
│ 规则/不执行    │        │ 真实 runtimes     │       │ 每 Agent/每用例   │
└───────┬────────┘        └─────────┬─────────┘       └─────────┬─────────┘
        │                           │                           │
        └───────────────┬───────────┴───────────────┬───────────┘
                        │                           │
                ┌───────▼────────┐          ┌───────▼────────┐
                │ Update Impact  │          │ Agent Versions │
                │ impact.rs      │          │ agent_health   │
                │ diff/classes   │          │ 版本基线/变化  │
                └────────────────┘          └────────────────┘
```

| 层 | 输入 | 是否执行代码 | 是否联网 | 输出 |
|---|---|---|---|---|
| Static Audit | 库内版本目录（`library/skills/<slug>/<hash12>`） | 否 | 否 | `CapabilityAudit`（status/risk/findings） |
| Dependencies | 版本目录 + 本机 PATH/二进制 | 只执行 `--version`（白名单探测），不执行 skill 脚本 | 否 | `DependencyReport`（status/risk/items） |
| Update Impact | 两个版本目录 | 否 | 否 | `UpdateImpact`（fileChanges/classes/risk/scope） |
| Manual Evidence | 用户输入 | 否 | 否 | `ValidationRun`（agent/case/status/notes/evidence） |
| Agent Health | `.app` bundle / CLI `--version` | 只执行 `--version`，2s 超时 | 否 | `AgentVersion`（version/changed） |

---

## 3. 静态审计（`audit.rs`）

### 3.1 性质

- **确定性**：同一输入产生相同 finding 排序与结果；无随机、无 AI 评分。
- **只读**：只读取版本目录内的文本/权限/链接；不执行脚本、不联网、不写入被审目录。
- **可解释**：每个 finding 都有稳定 rule id、severity、category、标题、detail、path、suggestion。
- **有界**：单文件扫描上限 4MB、超大文件阈值 5MB、文件数上限 300、缺失引用最多 20 条、每条规则最多 20 条 finding。
- **不跟随 symlink**：符号链接只按链接本身处理；逃出技能目录的链接报 `security.symlink_escape`。

### 3.2 规则表

> severity 顺序：`info < low < medium < high < critical`。`category` 是 finding 的分类（`spec`/`quality`/`security`/`dependency`/`compatibility`）。

| Rule id | 类别 | 严重度 | 触发条件（摘要） |
|---|---|---|---|
| `spec.missing_skill_md` | spec | critical | 缺少可读的常规 `SKILL.md`（symlink 不算） |
| `spec.missing_name` | spec | high | frontmatter 缺少非空 `name` |
| `spec.missing_description` | spec | high | frontmatter 缺少非空 `description` |
| `spec.short_description` | spec | low | description 过短（规范建议写清用途与触发场景） |
| `spec.no_license` | spec | info | 无 `license` 字段且无 LICENSE 文件 |
| `files.many` | quality | low | 文件数 > 300 |
| `files.large` | quality | low | 单文件 > 5MB |
| `files.world_writable` | security | medium | 文件权限含 world-writable |
| `security.symlink_escape` | security | high | 链接解析到技能目录之外 |
| `scripts.present` | quality | info | 发现脚本类文件（信息级） |
| `scripts.shell` | quality | medium | 存在 shell 脚本 |
| `scripts.python` / `scripts.node` / `scripts.interpreter` | quality | low | 存在解释器脚本（按 shebang 归类） |
| `script.pipe_to_shell` | security | high | 管道输入到 shell（泛化检测） |
| `script.curl_pipe` / `script.wget_pipe` | security | high | `curl`/`wget` 输出被管道给其他命令 |
| `script.sudo` | security | high | 脚本请求 `sudo` |
| `script.rm_rf_root` | security | critical | 递归删除根路径（`rm -rf /` 等） |
| `script.chmod_777` | security | high | 授予 world-writable 权限 |
| `script.eval` | security | high | 动态 `eval` |
| `script.base64_decode` | security | high | `base64 -d/--decode` 解码后执行风险 |
| `script.os_system` | security | high | Python `os.system(` |
| `script.subprocess_shell_true` | security | high | `subprocess(... shell=True)` |
| `security.ssh_access` | security | high | 访问 `~/.ssh` / SSH key |
| `security.aws_credentials` | security | high | 访问 `.aws/credentials` |
| `security.private_key_access` | security | high | 访问 `id_rsa` / `id_ed25519` |
| `security.keychain_access` | security | high | 访问 macOS Keychain |
| `security.passwd_access` | security | high | 访问 `/etc/passwd` / `/etc/shadow` |
| `security.hardcoded_secret` | security | critical | 检测到硬编码凭据（**值被脱敏，只保留类型/行号**） |
| `network.remote_access` | security | medium | 出现远程域名（列出数量与前几个） |
| `network.exfiltration_risk` | security | high | 网络访问与敏感数据组合/可疑外传模式 |
| `references.missing_file` | quality | medium | `SKILL.md` 引用的相对文件不存在 |
| `dependency.no_manifest` | dependency | low | 有脚本但无任何依赖清单 |

> 规则是**启发式**，不是完备安全证明；允许存在 false positive/negative。finding 的 `detail` 中包含脚本行号与截断片段；**含凭据的行只显示 `[line contains a credential; value redacted]`**。

### 3.3 status / risk 推导

| 最高 severity | status | risk |
|---|---|---|
| critical 或 high | `fail` | critical / high |
| medium | `warn` | medium |
| low / info / 无 finding | `pass` | low |

> `status=fail` 在健康状态机中优先于人工 passed → `needs_attention`。审计失败不会被「功能能用」抵消。

---

## 4. 依赖与环境诊断（`deps.rs`）

### 4.1 探测内容

| kind | 来源 | 检测方式 | 可能状态 |
|---|---|---|---|
| `runtime` | 脚本 shebang（`#!/usr/bin/env python3` 等） | `find_binary` 查 PATH + `/opt/homebrew/bin` + `/usr/local/bin`；`--version`，**2 秒超时** | `ok` / `missing` |
| `binary` | shebang/脚本显式引用的可执行文件 | 同上 | `ok` / `missing` |
| `manifest` | `requirements.txt`、`pyproject.toml`、`package.json`、`setup.py`、`setup.cfg`、`Pipfile`、`environment.yml`、`go.mod`、`Cargo.toml`、`Gemfile` | 文本解析；只读取声明，不安装 | `ok` / `unknown` |
| `package` | requirements / pyproject / package.json 中的依赖名 | 只记录声明与版本约束，**不安装、不解析已安装包** | `unknown` |
| `mcp` | `SKILL.md` 提到 MCP server | 静态识别，无法验证 server 是否可用 | `unknown` |
| `credential` | 文本引用凭据环境变量 | 只记录 key/引用，不读取值 | `unknown` |

### 4.2 status / risk 推导

| 条件 | status | risk | summary 示例 |
|---|---|---|---|
| 存在 `missing`（运行时/二进制缺失） | `needs_attention` | medium | `N missing dependency item(s): ...` |
| 无 missing，且存在可检查项但无 warning | `healthy` | low | `environment healthy: N runtime/manifest check(s) passed; M package(s) declared (installation not checked)` |
| 无 missing，有 warning | `healthy` | medium | `environment ready with N warning(s): ...` |
| 没有任何可检查项，或只有无法静态验证的 MCP/package | `unknown` | low | `... cannot be verified statically` |

> `needs_attention` 会让健康状态至少 `needs_attention`（安全/可用性优先），人工 passed 不能覆盖缺失运行时。

### 4.3 Python/Node 兼容性

- shebang 带版本（如 `python3.11`）时会记录 `required_version`，并尝试探测对应/兼容解释器。
- `python3` 缺失但只有 `python` 存在时，给出明确 warning 与安装建议。
- `package.json` 的 `engines.node` 作为 Node 版本要求展示（不自动安装）。
- MCP/凭据保持 `unknown`：静态检查无法证明运行时可连接或凭据有效。

---

## 5. 更新影响分析（`impact.rs`）

### 5.1 输入/输出

- 输入：同一 capability 的两个版本目录（通常 from=已安装版本，to=当前库版本）。
- 文件比较：按内容字节比较（symlink 按链接目标字符串），权限变化单独检测。
- `file_changes` 最多 **500** 条；摘要保留**真实总数**（UI 显示「仅显示前 N 条，共 M 条」）。
- 新版本会再跑一次静态审计，把 `severity >= medium` 的 finding 带入 impact。
- 结果写入 `update_impacts`（按 capability/from/to 缓存；重复预览复用并重新附加 affected scope）。

### 5.2 classes

| class | 含义 |
|---|---|
| `docs_only` | 只有文档类变更（不改 SKILL.md/脚本/依赖/网络） |
| `skill_md_changed` | `SKILL.md` 变更 |
| `new_script` / `removed_script` / `script_changed` | 新增/删除/修改脚本 |
| `new_network_domain` / `removed_network_domain` | 新增/移除远程域名 |
| `new_dependency` / `removed_dependency` | 依赖清单变化 |
| `new_binary` | 新增二进制 |
| `files_added` / `files_removed` | 文件增删 |
| `large_change` | 大量/大体积变更 |

### 5.3 risk 与 requiresRevalidation

- risk 综合新版本审计最高 severity 与变更类型；impact risk **最高到 high**（具体 finding 仍可能是 critical）。
- `requiresRevalidation = true` 当：SKILL.md、脚本、域名、依赖、风险上升、二进制、权限变化等。
- **只有 `docs_only` 且其他无变化**才可能不需要重新验证。
- 分发计划生成时，若检测到 from≠to，会自动调用 `preview_update` 并写入 plan warnings：
  - `Update impact: ... (risk ..., N affected installation(s), revalidation required: ...)`
  - `requiresRevalidation` 为 true 时追加「apply 后需重新记录验证证据」。
- `affectedInstallationCount`/`affectedAgentKeys`/`affectedProjectRoots` 来自当前安装记录。

---

## 6. 人工验证证据（`validation_runs`）

### 6.1 记录字段

| 字段 | 说明 |
|---|---|
| `capability_id` / `version_id` | 证据绑定到**具体版本**；旧版本证据不参与当前健康 |
| `agent_id` | 可选；为空表示 Agent 无关（静态/通用） |
| `kind` | `manual` / `static` / `environment` / `regression` |
| `status` | `passed` / `failed` / `needs_attention` / `unverified` |
| `case_name` | 真实验收用例名（必填） |
| `notes` | 环境/观察结果（用户输入） |
| `evidence` | 日志片段或链接（用户输入，存本机数据库） |
| `actor` | 固定 `user`（当前无多用户体系） |
| `created_at` | 记录时间 |

### 6.2 语义（必须写进 UI/文档）

- 封闭桌面 Agent **无法被 AgentHub 自动端到端测试**；manual passed 是**真实人工证据**，不是自动通过。
- 只有 `kind=manual` 且 `status=passed` 能贡献 `verified`；`static`/`environment`/`regression` 的 passed 不替代人工证据。
- 当前版本 + **每个已安装 Agent** 都有 manual passed → `verified`；缺少一个 → `unverified`（并列出已验证/未验证 Agent）。
- 任何当前版本 `failed` → `failed`（优先于其他状态）。
- evidence/notes 是**用户输入**，AgentHub 不做内容真实性证明；UI 必须提示「请如实填写」，并说明这是证据记录而不是系统自动结论。

### 6.3 CLI/UI 记录方式

```bash
# 记录一次真实人工验证（version 默认当前版本；agent 可省略）
agenthub skills validate memory-maker \
  --agent codex --case "generate invoice PDF" --status passed \
  --notes "Codex 26.x, macOS 15.8" --evidence "log excerpt or link"

# 查看健康（记录前后计数变化）
agenthub health
agenthub health --json
```

UI：SkillDetail →「验证证据」区块；会显示「封闭 Agent 无法自动端到端测试，此记录是真实人工证据，不是自动通过」。

---

## 7. 健康状态机（`validation.rs`）

### 7.1 状态定义

| 状态 | 含义 |
|---|---|
| `not_deployed` | 没有安装到任何 Agent（无论审计/证据如何） |
| `failed` | 当前版本存在 `failed` 验证记录 |
| `needs_attention` | 审计 fail 或依赖 needs_attention（安全/环境问题优先） |
| `verified` | 当前版本、全部已安装 Agent 都有 manual passed |
| `unverified` | 已安装，但证据不完整（或只有静态检查） |

### 7.2 推导优先级（从上到下，先命中先返回）

```text
1. installed_agents 为空                          → not_deployed
2. 当前版本存在 status=failed 的 validation run  → failed
3. audit.status == "fail"                         → needs_attention
4. deps.status == "needs_attention"               → needs_attention
5. 所有 installed agents 都有 manual passed        → verified
6. 部分 agents 有 manual passed                    → unverified（列出两方名单）
7. 其他（无 manual 证据）                          → unverified
```

补充规则：

- **版本过滤**：若 audit/deps 提供当前 version_id，只考虑同 version 的 validation；旧版本证据失效。
- **去重**：`installed_agents` 去重、去空白；重复安装不会要求重复证据。
- **Agent 身份**：优先 `agent_key`，其次 `agent_name`，最后 `agent_id`；manual 证据必须有可识别 Agent 才能计入。
- **人工不能覆盖安全失败**：`audit fail` + manual passed → `needs_attention`（有自动测试）。
- **安装不等于验证**：无 manual 证据时永远 `unverified`（有自动测试）。

### 7.3 真实 Mac 示例（`/tmp/agenthub-validator-real/health*.json`）

| 能力 | 安装数 | 审计 | 依赖 | 人工证据 | 健康 |
|---|---|---|---|---|---|
| `memory-maker` | 0 | warn/medium | healthy | 1 条 manual passed | `not_deployed` |
| `agenthub-accept-155547` | 0 | warn/medium | — | 0 | `not_deployed` |

> 即使有 manual passed，**未部署**仍显示 `not_deployed`；这避免把「验证过但没装」误当成可用。

---

## 8. Agent Health / 版本观测（`agent_health.rs`）

### 8.1 版本探测

| 来源 | 方法 | 限制 |
|---|---|---|
| `.app` bundle | 读 `Contents/Info.plist` 的 `CFBundleShortVersionString`：`plutil -extract ... raw` → XML 解析 → `mdls kMDItemVersion` 兜底 | 需要 bundle 仍在原路径；不猜测、不伪造 |
| CLI | 在 PATH + `/opt/homebrew/bin` + `/usr/local/bin` 找可执行文件，执行 `--version`，**2s 超时**；读取 stdout/stderr 的第一个版本 token | CLI 可能不输出版本/输出非标准格式；DSH 作为 app 无版本时的 fallback |
| 其他 | 无 | 返回 None，不显示假版本 |

### 8.2 变更语义

- 首次观测写入 baseline（`changed=false`），**不算变更**。
- 之后版本与上一条不同 → `changed=true`，`previousVersion` 保留旧值，note 提示重新扫描/重新记录验证。
- 同一秒多次写入：`ORDER BY detected_at DESC, rowid DESC` 用 `rowid` 决胜（QA 发现的 same-second ordering bug 已修复，有回归测试）。
- `agent_versions` 持久化；`health --agents` 只读，`health --agents --record` 写入一条观测。

### 8.3 真实 Mac 观测（`agents.json`，2026-10-20 16:19）

| Agent | 版本 | 备注 |
|---|---|---|
| ChatGPT | `26.1013.40934` | `/Applications/ChatGPT.app` |
| Codex | `26.623.70822` | 旧 Codex.app（与 ChatGPT 同 bundle id 的兼容副本） |
| DeepSeek Harness | `0.2.1-rc.2` | CLI `dsh --version` fallback |
| TRAE CN | `3.4.4` | app bundle |
| WorkBuddy | `5.8.3` | app bundle |
| ZCode | `3.16.2` | app bundle |
| TRAE (International) | 未输出 | 仅配置残留 |
| Claude Code / Cursor | 无 | 当前无版本来源 |

> 首次运行全部为 baseline（`changed=false`）；后续版本变化才会触发提示。

---

## 9. 数据与 API

### 9.1 DB v2（`PRAGMA user_version = 2`）

| 表 | 用途 |
|---|---|
| `capability_audits` | 静态/环境审计结果与 findings |
| `dependency_reports` | 依赖/环境检查结果 |
| `update_impacts` | 版本更新影响（payload 缓存） |
| `validation_runs` | 人工/静态/环境/回归验证证据 |
| `agent_versions` | Agent 版本观测与变化 |

详见 [DATA-MODEL.md](DATA-MODEL.md)。v1 → v2 自动迁移、幂等；真实 `~/.agenthub` 已验证 `user_version=2`。

### 9.2 Tauri 命令（新增 9 个，共 47）

`skills_audit`、`skills_dependencies`、`skills_audit_history`、`skills_preview_update`、`skills_health`、`health_overview`、`validations_list`、`validations_record`、`agent_health`。

### 9.3 CLI

| 命令 | 说明 | 退出码 |
|---|---|---|
| `health` | 能力健康概览 | 0 |
| `health --agents` | Agent 版本健康（只读） | 0 |
| `health --agents --record` | 记录版本观测（隐含 `--agents`） | 0 |
| `skills audit <skill>` | 静态审计 | `fail` → 2；否则 0 |
| `skills deps <skill>` | 依赖/环境诊断 | `needs_attention` → 2；否则 0 |
| `skills impact <skill>` | 更新影响 | 0 |
| `skills validate <skill> --case ... --status ...` | 记录验证证据 | 0，输入非法 → 1 |

---

## 10. 限制与非目标

| 限制 | 说明 |
|---|---|
| 规则引擎不是完备安全证明 | 只能发现已知模式；混淆、动态生成、二进制 payload 可能漏报；也可能误报 |
| 不执行技能脚本 | 不运行 `curl`、不执行 `scripts/*`；依赖诊断只跑 `--version` |
| MCP/凭据静态 unknown | 无法证明 server 可连接、凭据有效；需在 MCP 管理器中单独验证 |
| 封闭 Agent 无法自动 E2E | manual evidence 是当前唯一 `verified` 路径；不能自动保证 Agent 内部行为 |
| 版本探测依赖 app bundle/CLI | 端口/重命名/非标准打包可能导致 None；不猜测 |
| 证据是用户输入 | `notes`/`evidence` 真实性由用户负责；AgentHub 不做签名/防伪 |
| 仅 macOS | 依赖 `plutil`/`mdls`/`.app`/macOS 路径；未验证其他平台 |
| 体积/条数上限 | 单文件 4MB、超大 5MB、文件 300、缺失引用 20、每条规则 20、impact 文件列表 500（总数为真实值） |

**非目标**：运行时沙箱、MCP 试连、自动执行 skill、AI 打分、网络声誉/云扫描、自动安装依赖。

---

## 11. 测试与真实证据

### 11.1 自动化（2026-10-20 16:24 实测）

| 范围 | 结果 |
|---|---|
| `./scripts/cargo.sh test -p agenthub-core` | **76 passed / 0 failed**（44 unit + 32 integration；validator 11 全启用） |
| `./scripts/cargo.sh test -p agenthub-cli` | **16 passed / 0 failed**（9 旧 + 7 validator CLI） |
| Rust 合计 | **92 passed / 0 failed** |
| `pnpm test` | **7 files / 29 tests passed** |
| `pnpm build` | 通过 |
| `./scripts/cargo.sh check -p agenthub` | exit 0 |

关键回归：

- `latest_agent_version_tie_returns_stale_row_within_same_second`：同一秒版本行用 rowid 决胜。
- `manual_pass_cannot_override_a_failed_static_audit`、`audit_and_operation_metadata_never_store_raw_secrets`。
- `v2_migration_rebuilds_tables_and_preserves_v1_data`。
- `update_impact_reports_classes_scope_and_install_plan_warning`。

### 11.2 真实 Mac 证据（`/tmp/agenthub-validator-real/`）

| 文件 | 内容 |
|---|---|
| `agents.json` | 9 Agent 版本/状态/pathsOk（见 §8.3） |
| `audit-memory.json` / `audit-accept.json` | warn/medium、8 findings（3 medium/3 low/2 info） |
| `deps-memory.json` | healthy；node 24.19.0 ok；MCP unknown |
| `impact-memory.json` | medium、`skill_md_changed`、requiresRevalidation=true、0 installations |
| `validate.json` | manual passed 证据（memory-maker / codex） |
| `health1.json` / `health2.json` | total=2、not_deployed=2；证据记录前后计数不虚增 |
| `*.err` | 全部为空 |

真实 `~/.agenthub` 已验证 `PRAGMA user_version=2`，五张新表存在。GUI 健康摘要/健康页截图：[gui-health.png](evidence/gui-health.png)（health 页未自动点击时按 Lead 指示引用 Dashboard）。

---

## 12. 后续路线

1. **自动回归**：对可脚本化 Agent 增加受控 E2E；封闭 Agent 保持 manual。
2. **多 Agent 基准**：同一 skill 在 Codex/TRAE/ZCode/WorkBuddy/DSH 的结果矩阵与差异报告。
3. **CI**：validator 规则 fixture、health 状态机、migration、CLI 退出码纳入 CI；macOS runner。
4. **证据增强**：可选证据 hash/附件（仍不做「自动通过」）。
5. **规则演进**：规则版本化、可解释豁免（用户确认）、误报反馈。

---

## 13. 交叉链接

- 能力健康数据/表：[DATA-MODEL.md](DATA-MODEL.md)
- 静态审计安全边界：[SECURITY.md](SECURITY.md)
- UI 健康页/预览：[UI-SPEC.md](UI-SPEC.md)
- 测试清单/真实证据：[TEST-PLAN.md](TEST-PLAN.md)
- Agent 版本探测与 adapter：[ADAPTER-SPEC.md](ADAPTER-SPEC.md)
- 真实 Mac 验收：[ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)
