# AgentHub 规范研究：Agent Skills / MCP / 各 Agent 公开文档 / 参考项目

> 版本：v1（2026-10-20）
> 作者：规范研究员（spec-researcher）
> 研究方法：只读取一手公开资料（官方规范站、官方文档站、官方 GitHub 仓库/生成式 schema）；GitHub API 仅用于读取 README/源码/schema。所有链接均为公开 URL。
> 约定：**UNKNOWN = 本次未能从一手资料确认**（不是「不存在」）。标注「本机观察」的内容仅作旁证，需由负责本机核验的同事复述确认。
> 本文件不写产品代码，仅提供规范依据与设计含义。

---

## 0. 一句话结论（先看这里）

1. **Skill 有真正的开放规范**（Agent Skills，agentskills.io）：`name` 1–64 字符、小写字母/数字/连字符、不得以连字符开头结尾、不得连续 `--`、**必须与父目录同名**；`description` 1–1024 字符且必填。这两条是硬校验，其余字段可选。— 见 §1。
2. **渐进式加载是三层**：元数据（name+description，约 50–100 tokens）→ 激活时加载 SKILL.md 正文（建议 <5000 tokens / <500 行）→ 按需读取 `scripts/`、`references/`、`assets/` 资源。— 见 §1.3。
3. **MCP 规范只定义协议，不定义客户端配置文件格式**。`mcpServers` / `mcp_servers` / `mcp.servers` / Cordis YAML 全是各家客户端自己的 de-facto 格式。这是 AgentHub 必须做「多格式适配层」的根本原因。— 见 §2、§5。
4. **MCP 正在换代**：最新规范是 **2026-07-28**，移除了 `initialize` 握手、`Mcp-Session-Id`、HTTP GET SSE 与 SSE 续传，改为完全无状态 + 每请求 `_meta` 携带版本/能力 + 新增 `server/discover`；而当前绝大多数 Agent 仍实现 **2025-06-18 / 2025-11-25** 的「有握手 + 有 session」版本。适配引擎必须**按版本分流**，不能只写一套。— 见 §2.3、§2.4。
5. **ChatGPT 与 Codex 不是一回事**：ChatGPT 的 Skills/Plugins 是**账号云端**资产（网页上传→扫描→Canva 式安装），没有文档化的本地 `SKILL.md` 目录；自定义 MCP 只支持**远程 HTTPS**（本地服务须走 Secure MCP Tunnel），桌面端只在「插件里的 local MCP app」形态下运行本地 MCP。Codex CLI 才是**本地文件式**：`~/.codex/skills/`、`~/.codex/config.toml` 的 `[mcp_servers.*]`。— 见 §3.1、§3.2。
6. **国内四家路线已收敛到同一套**：TRAE、ZCode、WorkBuddy/CodeBuddy 都以 `SKILL.md` 为核心（ZCode 原生 MCP 键名是 `mcp.servers`，WorkBuddy 自定义连接器官方指向 CodeBuddy 的 `mcpServers` 文档）；且 **TRAE 与 DSH 都原生扫描 `.agents/skills/`**；ZCode 甚至内置「从 Claude Code / Codex / OpenCode / `.agents` 导入」。`.agents/` 已成为跨客户端约定。— 见 §1.4、§3.3–§3.6、§4。
7. **参考项目最值得抄的不是 UI，而是三件事**：适配器注册表（key + global/project 双路径 + 递归开关 + 路径覆盖）、部署记录表（`skill_targets(skill_id, tool, target_path, mode, status, source_hash)`）、以及「不拥有目标就不覆盖」的冲突策略（`TARGET_CONFLICT`）。— 见 §4。
8. **ChatGPT（及 WorkBuddy 的 MCP）的 UI-only 配置决定了 MVP 边界**：这两家没有稳定公开的本地配置文件，MVP 应只做「只读探测 + 导出/引导」，不做写入。— 见 §6、§7。

---

## 1. Agent Skills 规范（Anthropic 发起，现为开放标准）

### 1.1 一手来源

| 内容 | URL |
| --- | --- |
| 规范主页（本报告主要依据） | https://agentskills.io/specification |
| 如何为客户端加 Skills 支持（实现指南） | https://agentskills.io/client-implementation/adding-skills-support |
| Skill 创作最佳实践 | https://agentskills.io/skill-creation/best-practices |
| 官方示例仓库 + spec 目录 | https://github.com/anthropics/skills |
| Anthropic 工程博客（设计动机） | https://anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills |
| Claude Code 侧实现（含 spec 之外的扩展字段） | https://code.claude.com/docs/en/skills |

### 1.2 目录结构（规范原文）

```text
skill-name/
├── SKILL.md          # Required: metadata + instructions
├── scripts/          # Optional: executable code
├── references/       # Optional: documentation
├── assets/           # Optional: templates, resources
└── ...               # Any additional files or directories
```

要点：**唯一必需文件是 `SKILL.md`**；`scripts/`、`references/`、`assets/` 是规范推荐的约定目录，不是强制的。任意其它文件/目录允许存在。

### 1.3 frontmatter 字段（规范原文表格 + 摘录）

| 字段 | 必填 | 约束（规范原文） |
| --- | --- | --- |
| `name` | 是 | Max 64 characters. Lowercase letters, numbers, and hyphens only. Must not start or end with a hyphen. |
| `description` | 是 | Max 1024 characters. Non-empty. Describes what the skill does and when to use it. |
| `license` | 否 | License name or reference to a bundled license file. |
| `compatibility` | 否 | Max 500 characters. Indicates environment requirements (intended product, system packages, network access, etc.). |
| `metadata` | 否 | Arbitrary key-value mapping（string → string）。 |
| `allowed-tools` | 否 | Space-separated string of pre-approved tools the skill may use. (Experimental) |

`name` 字段的完整规则（规范正文，比表格更细）：

- 1–64 字符
- 只允许 unicode lowercase alphanumeric（`a-z`、`0-9`）与连字符 `-`
- 不得以 `-` 开头或结尾
- **不得包含连续连字符 `--`**
- **必须与父目录名一致**（"Must match the parent directory name"）

`description` 的完整规则：1–1024 字符、非空、**同时写清「做什么」与「什么时候用」**，并建议包含具体触发关键词（规范给了 good/poor 对照）。

最小可用示例（规范原文）：

```yaml
---
name: skill-name
description: A description of what this skill does and when to use it.
---
```

**注意「6 个字段」的口径**：Claude Code 官方文档明确说「Restricting frontmatter to the spec's six fields avoids the unexpected-key error」，即 spec 六字段 = `name`、`description`、`license`、`compatibility`、`metadata`、`allowed-tools`；其它（`disable-model-invocation`、`user-invocable`、`context`、`agent`、`model`、`paths`、`arguments` 等）都是 **Claude Code 扩展**，跨 Agent 分发时应视为可选且不可假设生效。

### 1.4 渐进式加载（progressive disclosure）

规范原文三层：

1. **Metadata（~100 tokens）**：所有 skill 的 `name` + `description` 在启动时加载；
2. **Instructions（建议 <5000 tokens）**：SKILL.md 正文在 skill 被激活时加载；
3. **Resources（按需）**：`scripts/`、`references/`、`assets/` 中的文件只在被引用/需要时加载。

配套建议：主 `SKILL.md` 保持 **<500 行**；引用文件用相对路径、保持「一度深」（"Keep file references one level deep from `SKILL.md`. Avoid deeply nested reference chains."）。

客户端实现指南（`adding-skills-support`）把同一机制写成可落地五步，**这部分对 AgentHub 适配层最有价值**：

| 步骤 | 关键要求（原文摘录/转述） |
| --- | --- |
| 1. Discover | 扫描 **project-level** 与 **user-level** 两个 scope；每个 scope 同时扫「本客户端原生目录」与 **`.agents/skills/` 跨客户端约定**；跳过 `.git/`、`node_modules/`；建议深度 4–6、上限 ~2000 目录；碰撞时 **project 覆盖 user**，并记录 warning |
| 2. Parse | 解析 `---` 之间的 YAML；**宽容校验**：name 与目录名不符/超 64 字符 → 警告但仍加载；缺 description 或 YAML 不可解析 → 跳过并记录；对「值里带冒号」的坏 YAML 可尝试加引号/块标量兜底 |
| 3. Disclose | 目录只放 `name` + `description`（可选 `location`），约 50–100 tokens/skill；被禁用/无权限/`disable-model-invocation` 的 skill **完全不出现**；无 skill 时不要输出空目录 |
| 4. Activate | 模型自判（读文件或专用 `activate_skill` 工具）或用户显式触发（`/skill-name`、`$skill-name`）；推荐用结构化标签包裹并列出 `scripts/` 等资源但**不预读**；若权限系统会拦文件读取，**把 skill 目录加入 allowlist** |
| 5. Maintain | 上下文压缩时**保护 skill 内容不被裁剪**；同一 session 内去重、避免重复注入；可选 subagent 隔离执行 |

跨客户端约定（原文）：「The `.agents/skills/` paths have emerged as a widely-adopted convention for cross-client skill sharing.」并提到部分实现还会扫 `.claude/skills/` 做兼容。

### 1.5 安全边界（规范层与实现层的硬约束）

规范本身不设权限模型，安全边界来自**客户端实现约定 + 生态实践**，可归纳为 6 条：

1. **项目级 skill = 不可信输入**。实现指南原文建议：「Consider gating project-level skill loading on a trust check — only load them if the user has marked the project folder as trusted. This prevents untrusted repositories from silently injecting instructions into the agent's context.」
2. **`allowed-tools` 是「预授权」而非「限制」**。Claude Code 文档：`allowed-tools` 让 skill 在当轮免确认使用所列工具，**不会限制其它工具可用**；且「A skill can grant itself broad tool access, so review the `allowed-tools` of skills checked into a repository」——即仓库里的 skill 可自我提权，必须人工审阅。
3. **正文里的动态命令注入 = 任意命令执行**。Claude Code 的 `` !`cmd` `` 语法会在渲染 skill 时执行 shell（受权限规则/信任弹窗约束），这解释为何 AgentHub 做「安装预览」时必须展示脚本与注入片段。
4. **上传即代码**。ChatGPT 的 skill 上传会先扫描，命中风险会标 `Needs Review` 或 `Blocked`（https://help.openai.com/en/articles/20001066-skills-in-chatgpt）；WorkBuddy 文档直接警告第三方 Skill 可能有「恶意提示词注入、越权访问、后门程序」。
5. **MCP 侧的对应风险**（同属「Agent 扩展面」）：本地 stdio server 一键配置 = 任意命令执行，规范级安全文档要求**显示将执行的完整命令行、显式同意、高亮 `sudo`/`rm -rf` 等危险模式、沙箱最小权限**（见 §2.6）。
6. **凭据不进 SKILL.md / 不进日志**：参考项目把 token 放 OS keychain、把 `secret` 从仓库备份中排除（§4.1），这是可借鉴的默认策略。

### 1.6 对 AgentHub 的设计含义

- **校验器（Validator）必须存在且可解释**：硬错误 = 无 `SKILL.md` / YAML 不可解析 / name 非法或与目录名不符（按规范）/ description 缺失或超 1024；警告 = 各 Agent 更严的限制（见 §5.3 的「各 Agent 额外限制」）。
- **单一真源 + 部署副本**：库里的 skill 只存一份（canonical），每个 Agent 的目录是「部署目标」，用记录表而非内容推断来管理（§4.1 的 `skill_targets`）。
- **默认 symlink、可回退 copy**：macOS/Linux 用 symlink（改一处全生效）；Windows 目录 symlink 需权限，退化为 junction，再退化 copy；跨盘/网络盘/WSL 必须 copy（§4.1）。
- **不覆盖非我拥有的目录**：目标已存在且无部署记录 → 拒绝并报「目标冲突」，而不是静默覆盖（§4.1 的 `TARGET_CONFLICT`）。
- **安装前预览**：展示 SKILL.md 全文摘要 + 脚本清单 + 相对引用 + 是否有 `allowed-tools`/动态注入，作为 MVP 的安全底线。

---

## 2. MCP 规范（Model Context Protocol）

### 2.1 一手来源与版本矩阵

| 版本 | 状态 | 关键特征 | URL |
| --- | --- | --- | --- |
| 2024-11-05 | 历史 | 初版；**HTTP+SSE** 传输（已被取代） | https://modelcontextprotocol.io/specification/2024-11-05 |
| 2025-03-26 | 历史 | 引入 **Streamable HTTP**；旧 SSE 开始 deprecated | https://modelcontextprotocol.io/specification/2025-03-26 |
| 2025-06-18 | **主流实现基线** | 有 `initialize` 握手 + session；stdin/stdout 与 Streamable HTTP；elicitation 等 | https://modelcontextprotocol.io/specification/2025-06-18 |
| 2025-11-25 | 主流 | 在 2025-06-18 上增量（tasks 等）；安全最佳实践以此版文档发布 | https://modelcontextprotocol.io/specification/2025-11-25 |
| **2026-07-28** | **最新** | **无状态化**：删握手/session/GET SSE/续传，新增 `server/discover`、MRTR、`subscriptions/listen`、`resultType` | https://modelcontextprotocol.io/specification/2026-07-28 |
| 变更记录 | — | 2026-07-28 的完整 key changes | https://modelcontextprotocol.io/specification/2026-07-28/changelog |

> 结论：AgentHub 的 MCP 适配层要么**同时实现两代语义**，要么明确声明只支持 2025-xx（并检测 2026-07-28 server 后降级提示）。只写一套会在 6–12 个月内踩坑。

### 2.2 JSON-RPC 生命周期与能力协商（以 2025-06-18 为准）

一手：https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle

三个阶段：**Initialization → Operation → Shutdown**。原文要点：

- 初始化必须是第一次交互；客户端发 `initialize`，携带 `protocolVersion`、`capabilities`、`clientInfo`；服务端回 `protocolVersion`、`capabilities`、`serverInfo`、可选 `instructions`；客户端再发 `notifications/initialized`。
- 版本协商：服务端若不支持请求版本，**必须**回一个自己支持的版本；客户端不支持则应断开。
- 能力协商（capability negotiation）关键项：

| 侧 | capability | 含义 |
| --- | --- | --- |
| Client | `roots` | 提供文件系统 roots |
| Client | `sampling` | 支持服务端发起的 LLM 采样 |
| Client | `elicitation` | 支持服务端向用户追问 |
| Server | `prompts` | 提示模板 |
| Server | `resources` | 可读资源（含 `subscribe`、`listChanged`） |
| Server | `tools` | 可调用工具（含 `listChanged`） |
| Server | `logging` | 结构化日志 |
| Server | `completions` | 参数补全 |

- 关闭：stdio 由客户端「先关 stdin → 等退出 → 必要时 SIGTERM → SIGKILL」；HTTP 关闭连接。
- 超时：每个请求 SHOULD 设超时，超时后发 `notifications/cancelled` 并停止等待。

### 2.3 stdio vs Streamable HTTP vs 旧 HTTP+SSE

一手：https://modelcontextprotocol.io/specification/2025-06-18/basic/transports

| 维度 | stdio | Streamable HTTP | HTTP+SSE（旧，2024-11-05） |
| --- | --- | --- | --- |
| 进程模型 | 客户端启动子进程 | 独立 HTTP 服务，可多客户端 | 独立 HTTP 服务 |
| 消息分帧 | **换行分隔的 JSON-RPC**，消息内不得含换行；stdout 只能写合法 MCP 消息，日志走 stderr | 每个 JSON-RPC 消息一次 **HTTP POST** 到单一 MCP endpoint；响应为 `application/json` 或 `text/event-stream` | 两个端点：GET `/sse` 建流 + POST 发消息 |
| 服务端→客户端 | 同一 stdout | POST 响应内 SSE；另有 GET 开独立 SSE 流（可 405） | 长连接 SSE |
| 会话 | 进程即会话 | 可选 `Mcp-Session-Id` 头；404 需重初始化；DELETE 主动终止 | 以 endpoint 事件建立会话 |
| 断线续传 | 不适用 | 可选 SSE `id` + `Last-Event-ID` 续传 | 不适用 |
| 安全要求 | 子进程继承用户权限 | 服务端 **MUST 校验 `Origin`** 防 DNS rebinding；本地 **SHOULD** 只绑 127.0.0.1；**SHOULD** 认证 | 同左 |
| HTTP 头 | — | 后续请求 **MUST** 带 `MCP-Protocol-Version`；`Accept` 需同时列 `application/json` 与 `text/event-stream` | — |
| 现状 | 事实标准 | **推荐** | **deprecated**（2025-03-26 起；2026-07-28 正式列为 Deprecated） |

客户端兼容旧 server 的探测流程（规范原文）：先 POST `initialize`；若 4xx（405/404），改 GET 期待首事件为 `endpoint`，判定为旧 SSE。AgentHub 若做 MCP 客户端探测，可直接复用此逻辑。

### 2.4 2026-07-28 的破坏性变化（必须知道的 9 条）

一手：https://modelcontextprotocol.io/specification/2026-07-28/changelog

1. **删除协议级 session 与 `Mcp-Session-Id`**；list 端点不再随连接变化；跨调用状态改用服务端签发 handle 作为普通工具参数。
2. **完全无状态**：删除 `initialize` / `notifications/initialized`；每个请求在 `_meta` 里带 `io.modelcontextprotocol/protocolVersion`、`.../clientCapabilities`（SHOULD 还带 `clientInfo`），结果 `_meta` 带 `serverInfo`；版本不匹配返回 `UnsupportedProtocolVersionError`。
3. 新增 **`server/discover`**（server MUST 实现）用于版本/能力/身份发现，也可作为 stdio 向后兼容探测。
4. 用 **`subscriptions/listen`**（单条长生命 POST 响应流）取代 HTTP GET 与 `resources/subscribe`。
5. 删除 `ping`、`logging/setLevel`、`notifications/roots/list_changed`；日志级别改为每请求 `_meta.logLevel`。
6. tasks 移出核心，成为官方扩展（`tasks/get`、`tasks/update`）。
7. **MRTR（Multi Round-Trip Requests）**取代服务端主动请求（`roots/list`、`sampling/createMessage`、`elicitation/create`）：server 返回 `resultType: "input_required"` + `inputRequests`，client 重试原请求并带 `inputResponses`。
8. 所有结果新增必填 `resultType`（`complete` / `input_required`）；旧 server 缺省按 `complete` 处理。
9. 删除 SSE 续传/重放（`Last-Event-ID`、事件 id）；断流即丢请求，客户端必须用新 id 重发。

附带信号：MCP 生态出现 **「Skills over MCP」** 工作组（2026-07-28 index 的 Extensions 列表提到 "Skills over MCP: Rich, structured instructions for agent workflows, discovered and consumed through MCP"）。若该扩展成熟，未来 Skill 分发可能走 MCP 而不是文件——这是 AgentHub 中期需要跟踪的架构风险（见 §7）。

### 2.5 关键澄清：MCP 规范不定义客户端配置文件

通读 2025-06-18 与 2026-07-28 的 spec 目录（basic / transports / server / client），**没有任何一节定义客户端如何保存 server 配置**。spec 只定义消息、传输、能力、授权。所谓 `mcpServers` JSON、Codex 的 `[mcp_servers]` TOML、ZCode 的 `mcp.servers`、DSH 的 Cordis YAML，全部是**客户端私有格式**（部分借用了 Claude Desktop 2024 年形成的 de-facto 约定）。

因此 AgentHub 的正确抽象是：

```
AgentHub 内部统一模型（ServerDef）
   → Codec/Adapter（每客户端一个）
   → 写回该客户端的私有格式（含备份、原子写、diff 预览）
```

而不是「生成一份标准 MCP JSON 到处塞」。

### 2.6 MCP 安全基线（客户端视角）

一手：https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices

对 AgentHub（本质是「配置管理器 + 可能内置试连」）最相关的四条：

1. **本地 stdio server 的一键配置 = 任意代码执行**。要求：**展示完整、不截断的命令行**（含参数）、明确标识风险、显式同意、可取消；高亮 `sudo`、`rm -rf`、网络与外目录访问；MCP server 与客户端同权限运行，应沙箱/最小权限。
2. **Streamable HTTP 服务端必须校验 `Origin` 防 DNS rebinding**，本地只绑 `127.0.0.1`；客户端应把远程 server 当不可信。
3. **禁止 token passthrough**：MCP server MUST NOT 接受非签发给自己的 token。
4. **OAuth 侧风险**：confused deputy、SSRF（阻断私网/ link-local / 云元数据地址）、会话劫持、授权 URL scheme 校验（只允许 http(s)，拒绝 `javascript:`/`data:`/`file:`，禁止用 shell 打开 URL）。

### 2.7 对 AgentHub 的设计含义

- **传输枚举固定为** `stdio` / `streamable-http` / `sse(legacy)`（+ 可选的 `ws`，仅 Claude Code 有）。UI 上必须让用户能选「旧 SSE」，因为存量 server 仍有。
- **试连（Probe）功能**：内置一个最小 MCP client 做 `initialize`→`tools/list`→断开，用于「配置对了吗」。注意：对 2026-07-28 server 应走 `server/discover`；两条路径都要有超时与取消。
- **能力协商结果可作为 UI 信息**：连上后展示 server 的 `tools/resources/prompts` 能力与工具数（Claude Code 的 `/mcp` 面板就是这个信息架构）。
- **命令预览是安全底线**：保存前必须展示 `command + args` 的完整可执行串；对 `env` 里的密钥只显示 key、不显示 value。
- **版本分流**：内部 `protocolVersion` 协商逻辑独立于各 Agent 的配置格式，避免把「协议版本」误写进客户端配置（多数客户端配置里没有这个字段）。

---

## 3. 各 Agent 公开文档（Skills / MCP / 插件）

> 说明：本节只记录**官方文档可证**的内容。「本机观察」列仅作旁证，最终以负责本机核验的同事结论为准。

### 3.1 OpenAI Codex（CLI / IDE 扩展 / Codex 桌面）

**官方文档可达性说明**：`https://developers.openai.com/codex/config-reference` 与 `/codex/skills` 在本次研究网络中返回 **403 Forbidden**（Cloudflare 拦截，非内容不存在）。因此本节用三类一手替代证据：
(a) `openai/codex` 仓库**生成的配置 JSON Schema**（`codex-rs/core/config.schema.json`，由 `ConfigToml` 生成，注释明说「We generate a JSON Schema for `~/.codex/config.toml`」）；
(b) 同一仓库**历史版 `docs/skills.md`**（commit `2c6995c`，标题 "Skills (experimental)"）；
(c) 本机 `~/.codex/config.toml` 与 `~/.codex/skills/`（只读观察）。

**Skills（Codex）**

- 支持本地文件式 skill，路径 **`$CODEX_HOME/skills/**/SKILL.md`（默认 `~/.codex/skills`，递归）**；隐藏项与符号链接被跳过；只认文件名恰为 `SKILL.md` 的文件。
- 历史文档的字段限制：`name` 非空 **≤100 字符**、`description` 非空 **≤500 字符**，均会「sanitized to one line」；多余 key 忽略；**正文不注入上下文**。
- 注入方式：把 `name + description + SKILL.md 绝对路径` 作为运行时 `## Skills` 段（附在 `AGENTS.md` 之后），一条一行。
- 使用：消息里 `$<skill-name>` 引用；TUI 里 `/skills` 浏览；非法 skill 会弹阻塞式启动提示（列出路径与错误），可 dismiss 后忽略。
- 当前版本证据：schema 里存在顶层 `skills`（`SkillsConfig`），含 `bundled`、`config[]`（按 `path` 或 `name` 选择 + `enabled`）、`include_instructions`、`max_context_tokens`（默认占上下文 2%，上限 10000 tokens）——说明 Codex 现在已内建 skill 配置（不再是纯实验开关）。
- 本机观察：`~/.codex/skills/` 存在（`codex-primary-runtime`、`playwright`…），且 `config.toml` 有 `[features]` 段但**没有** `skills = true`，可推测该 flag 已默认开启或 GA（**待确认**）。

**MCP（Codex）**

- 配置文件：`~/.codex/config.toml`（`$CODEX_HOME/config.toml`）。
- 格式：TOML 表 **`[mcp_servers.<name>]`**；env 用**嵌套表** `[mcp_servers.<name>.env]`。
- 由 schema 确认的字段（`RawMcpServerConfig`，`additionalProperties: false`）：
  - 本地：`command`、`args[]`、`env{}`、`env_vars[]`、`cwd`、`startup_timeout_sec` / `startup_timeout_ms`、`tool_timeout_sec`、`enabled`、`required`、`startup_readiness`、`enabled_tools[]` / `disabled_tools[]`、`tools{}`、`default_tools_approval_mode`、`supports_parallel_tool_calls`、`omit_tools_from[]`、`tool_input_schema_max_bytes`
  - 远程：`url`、`http_headers{}`、`env_http_headers{}`、`bearer_token_env_var`、`oauth{}`（`client_id`/`client_secret`/`callback_port`/`callback_url`/`authorization_server_issuer`）、`auth`（`oauth` | `chatgpt` | `ema_auth`）、`oauth_resource`、`scopes[]`
- 顶层还有 MCP 全局项：`mcp_oauth_callback_port`、`mcp_oauth_callback_url`、`mcp_oauth_credentials_store`（**默认存 OS keyring**，可选 file）、`mcp_optional_startup_grace_ms`（默认 1000ms）、`mcp_enterprise_managed_auth`。
- 本机样例（只读，旁证）：

```toml
[mcp_servers.node_repl]
args = []
command = "/Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl"
startup_timeout_sec = 120

[mcp_servers.node_repl.env]
# （具体值已略）
```

- 插件：`[plugins."<plugin>@<marketplace>"] enabled = true`；`[marketplaces.<name>] source_type = "local" | ...`。`PluginsConfigToml` 另有 `_default` 控制默认激活。

**结论**：Codex 是**双文件式**（skills 目录 + TOML 配置），且 MCP 字段是「超集」——比通用 `mcpServers` 多出 OAuth/工具过滤/超时/审批策略。AgentHub 若要完整支持 Codex，内部的 `ServerDef` 必须容纳这些扩展字段（至少保留未知键透传，避免丢配置）。

### 3.2 ChatGPT（Desktop / Web）

一手：

- Connected apps：https://help.openai.com/en/articles/11487775-connected-apps-in-chatgpt
- Plugins（含 Skills）：https://help.openai.com/en/articles/20001256-plugins-in-chatgpt
- Skills in ChatGPT：https://help.openai.com/en/articles/20001066-skills-in-chatgpt
- Connecting custom MCP servers：https://help.openai.com/en/articles/12584461-connecting-custom-mcp-servers-in-chatgpt

**Apps / Connectors / 插件**

- 三层概念（官方 FAQ）：「An **app** connects ChatGPT to an external service… A **plugin** packages capabilities into a workflow and can include one or more apps, skills, or both.」
- 账号授权：在 **Settings → Plugins** 安装，按需 `Connect` 并走 provider OAuth；企业工作区由 admin 控制可用性与角色权限（`Use plugins` / `Upload plugins` / `Create plugins with MCPs` / `Share plugins` / `Publish plugins`）。
- 权限：应用权限分档（Always ask / Allow read actions / Allow low-risk actions / Allow all actions），与 workspace 策略叠加。

**Skills**

- 位置：**Plugins 目录的 Skills 标签页**；分类 Installed / Created by me / Shared with me / Shared by workspace；账号默认带 `skill-creator`。
- 创建/安装方式：Create with chat、**Create with editor（chatgpt.com/skills/editor）**、**Upload from your computer**、安装别人分享的 skill、workspace 安装。
- 上传会**先扫描**：多数立即可用，可疑标 `Needs Review`，有风险标 `Blocked`（不能被使用）。
- **没有任何文档化的「本地 `SKILL.md` 文件夹」路径**。也就是说 ChatGPT 桌面端**不是文件式**：skill 是账号/工作区资产，通过 UI/上传管理。
- 管理：admin Skills 页可 Review/Download/Change owner/Delete，统计 `Invocations (30d)`；Enterprise/Edu 有 Compliance Platform 审计。

**MCP（本地 vs 远程）**

- 自定义 MCP server 在 **Plugins → + → Add custom MCP server** 配置：名称、可选描述、**Connection 里填「public HTTPS MCP endpoint URL」，或选 Tunnel（Secure MCP Tunnel / tunnel_id）**；配置认证；勾选风险确认；`Create as a plugin` 后扫描工具。
- 官方 FAQ 明确：**「Can I connect to a local MCP server? Not directly. ChatGPT connects to remote MCP servers.」** 本地/内网/开发机上的 server 必须经 **Secure MCP Tunnel**。
- 另一条官方口径（Plugins 文）：插件可包含 **local MCP apps that run on your computer**，并在 **ChatGPT Desktop** 上运行；「saving a plugin with local apps to your account does not make those tools available on web or mobile」。→ 即桌面端存在「插件内置本地 MCP」的受控通道，但**不是**用户直接写配置文件。
- 工具快照：管理员发布后 ChatGPT 使用**冻结的工具快照**，server 后续变更需 admin 手动 Refresh/Publish；不兼容变更会导致调用报错。

**结论**：ChatGPT 适配器 = **只读能力探测 + 导出/分享链接 + 引导安装**，MVP **不做本地文件写入**（会破坏其账号资产模型，且没有稳定格式）。与 Codex 必须分开建模（同一账号下 Codex 的 skill 也是「separately managed」）。

### 3.3 TRAE（trae.ai / trae.cn；IDE，TraeWork 已并入 TRAE）

一手：https://docs.trae.cn/ide_skills · https://docs.trae.cn/ide_model-context-protocol · https://docs.trae.cn/ide_add-mcp-servers
（国际站同构页面：https://docs.trae.ai/ide/skills · https://docs.trae.ai/ide/model-context-protocol）

**产品形态**：文档导航明确「重磅更新：TraeCode 与 TraeWork 已融合为全新的 TRAE」，并保留 `TraeWork` 独立入口；对 AgentHub 而言按「一个 TRAE」登记即可，但 UI 上可区分 IDE / Work 两个入口（同一套 skills 目录）。

**Skills**

- 定义：`SKILL.md`；技能需包含 `SKILL.md`，可选脚本/模板/风格指南。
- 项目技能目录：**`<project>/.trae/skills/`**。
- 全局技能目录：**macOS/Linux `~/.trae-cn/skills`**（中文版文档明确；国际版应为 `~/.trae/skills`，**待官方国际站确认**）；Windows `%userprofile%/.trae-cn/skills`。
- 目录结构示例（官方）：`SKILL.md` + `examples/` + `templates/` + `resources/`（与 agentskills 的 scripts/references/assets 命名不同，但都是「可选目录」）。
- `.agents/skills/`：**官方支持**——文档专节「使用 .agents 技能目录」，说明其由 Agent Skills 规范约定；在 `设置 > 技能与命令 > 导入设置` 打开「启用 .agents 技能目录」开关；**同名时 `.trae/skills/` 优先于 `.agents/skills/`**。
- 安装：可从**插件市场**安装；也可「导入本地技能」上传单个 `SKILL.md` 或含 `SKILL.md` 的 `.zip`，TRAE 会解析并预填名称/描述/指令。
- 触发：模型按 description 自动调用，或用户显式指名（「用 codemap 技能…」）。**官方明确 Skill vs Rule 的区别**：Rule 全量加载常驻上下文，Skill 按需加载。

**MCP**

- 支持类型：**stdio**（本地）、**HTTP（SSE）**（本地/远程）、**Streamable HTTP**（本地/远程）。
- 协议能力：Requests/Responses、Timeouts、stdio/Streamable HTTP/HTTP+SSE 向后兼容、tools list/call/listChanged、工具定义字段（name/title/description/inputSchema/outputSchema）、Text + Structured Content、Logging。
- 配置：JSON，顶层 **`mcpServers`**；项目级在 **`<project>/.trae/mcp.json`**（需先在设置里开启「项目级 MCP」）；全局 MCP 通过 `设置 > MCP` 写入（**全局 mcp.json 路径官方未写明 → UNKNOWN**）。
- stdio 字段：`command`（必填，**不能含空格**，需在 PATH 或用绝对路径）、`args`（字符串数组）、`env`（字符串到字符串）。
- 远程字段：`url`（必填，http/https）、`headers`（可选，鉴权等）。
- 特殊约定：**超时通过 `env`/`headers` 传**——`START_MCP_TIMEOUT_MS`、`RUN_MCP_TIMEOUT_MS`；变量引用仅支持 `${workspaceFolder}`（在 args 中替换为项目根）。
- 注意：**项目级 MCP 会自动连接**。官方警告「打开一个项目，就等于连上了它配置里写的所有 MCP 服务……所以打开来路不明的仓库之前，建议先看一眼它的 `.trae/mcp.json`」——这与 Skill 的 trust 问题是同一类风险。

### 3.4 ZCode（智谱 Z.ai，GLM 编码 Agent）

一手：https://zcode.z.ai/cn/docs/skill · https://zcode.z.ai/cn/docs/mcp-services（英文：`/en/docs/skill`、`/en/docs/mcp-services`）

**Skills**

- 用户级目录：**`~/.zcode/skills/<skill-name>/SKILL.md`**（目录名即技能名，聊天里用 `$skill-name` 或 `/` 菜单「技能」组引用）。
- 格式硬限制（官方原文，**比 Agent Skills 规范更严/更怪**）：
  - frontmatter 必须含 `name` 与 `description`，缺失则该技能**被忽略**并在诊断里说明；
  - `description` 上限 **1024 字符**，超限「**整个技能会被丢弃**」而不是截断；
  - `SKILL.md` 正文超过 **100KB 会被截断加载**。
- 上下文预算：每轮把所有已启用技能的元数据注入模型（**单条 description 摘要最多 250 字符**）；总量超出固定预算时降级为「只保留技能名」，自动触发率明显下降 → 建议少而精。
- 插件技能：`skills/<技能名>/SKILL.md` **必须单层目录**，嵌套分组目录不被识别。
- 从外部导入：**官方内置「导入」**，可扫描 Claude Code、Codex CLI、OpenClaw、Augment、Windsurf 等的技能目录，导入方式二选一：
  - **软链（symlink）**：跟随来源变更，但依赖来源路径持续可用；
  - **复制（copy）**：独立副本，与来源解耦。
  导入目标可选「全局」或「当前项目」。
- 远程开发：支持把本地 Skill 同步（复制）到 SSH/WSL 远端。

**MCP**

- 读取路径与键名（官方表格，**四套并存**）：

| 作用域 | 文件 | 配置键 |
| --- | --- | --- |
| 用户 | `~/.zcode/cli/config.json` | `mcp.servers` |
| 工作区 | `<项目根>/.zcode/config.json` | `mcp.servers` |
| 用户 · `.agents` 兼容 | `~/.agents/mcp.json` | `mcpServers` |
| 工作区 · `.agents` 兼容 | `<项目根>/.agents/mcp.json` | `mcpServers` |

- 原生格式示例（官方）：

```json
{ "mcp": { "servers": { "memory": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"], "env": {} } } } }
```

- 优先级（官方）：工作区先于用户加载；**同作用域内只要 `.zcode` 读到任何 MCP 服务，同层的 `.agents/mcp.json` 就整体被跳过（不合并）**；`.agents` 仅作兜底。面板写入永远落到 `.zcode`。
- 启用状态：停用时向该 server 配置对象写 **`"enable": false`**（缺省即启用）。
- 类型：stdio（command/args/env）、HTTP、SSE；远程地址 + 可选 **headers**；OAuth 服务在列表行点「打开授权」，用系统浏览器授权后自动重连。
- 导入外部 Agent 的 MCP：扫描 **`~/.claude/settings.json`、`~/.codex/config.toml`、`~/.config/opencode/opencode.json`、`~/.agents/mcp.json`**；导入后写入 ZCode 自身 `.zcode` 配置，**不改原文件**。
- 工作区 MCP 会自动连接 + 官方安全警告（同 TRAE）。
- 补充：`zcode-configuration-guide` 内置技能集中解释「技能、命令、MCP、Hooks、插件、AGENTS.md」六类扩展的位置/作用域/冲突规则——这是很好的 IA 参考。

### 3.5 WorkBuddy（腾讯，CodeBuddy 家族）

一手：https://www.codebuddy.cn/docs/workbuddy/... （产品线含 IDE / 插件 / CLI / WorkBuddy / 小程序 / 移动端 / Enterprise）

**归属核实**：WorkBuddy 文档挂在 **codebuddy.cn** 域下，服务协议链接指向 `rule.tencent.com`、隐私指向 `privacy.qq.com` → **腾讯产品**（CodeBuddy 的办公 Agent 形态）。

**Skills（官方叫「技能」）**

- 入口：左侧「专家·技能·连接器」→「技能」页；分「技能市场」（推荐 / SkillHub / 套件）+「已安装（本地技能）」。
- 获取方式：市场一键安装、对话安装（仅市场）、**上传技能（拖拽本地技能包）**、**创建技能**（描述需求自动生成）、查找技能。
- 管理：启用/关闭开关**不改动技能包本身的 `SKILL.md`**，状态记在用户全局配置并可多端同步；关闭 ≠ 卸载；支持批量卸载。
- 官方原话有一条重要线索：「**从 `.agents` 等目录复制安装的技能**始终保持原始内容」——说明其安装链路会处理 `.agents` 目录（导入来源之一），与 TRAE/DSH 的 `.agents` 约定一致。**WorkBuddy 的全局本地技能目录路径官方未给出 → UNKNOWN**（第三方项目登记为 `~/.workbuddy/skills`，未验证）。
- 安全：官方明确警告第三方 Skill 可能含「恶意提示词注入、越权访问、后门程序」，要求审阅来源、权限申请与脚本内容；「优先使用官方推荐 Skill」。

**Connectors / MCP**

- 官方定义（连接器页）：「从实现形态来看，连接器分为 **MCP + CLI**（标准化协议）和 **Skill + CLI**（内置脚本）两种形式」。
- 已有连接器：QQ 邮箱、腾讯文档、腾讯乐享、腾讯会议、TAPD、腾讯网盘等；支持**自定义连接器**：「自定义连接器的配置方式与 MCP 配置类似」，并链接到 **CodeBuddy IDE 的 MCP 文档**。
- CodeBuddy IDE 的 MCP 格式（同一厂商，官方）：JSON 顶层 **`mcpServers`**，条目含 `type: "stdio"`、`command`、`args`、`env`、`description`（示例见文档）。
- 授权：每连接器独立 OAuth/扫码授权；权限边界「每个连接器独立授权 / 不主动定时抓取 / 不超出已有权限范围」；可随时禁用或解绑。
- **WorkBuddy 的 MCP 配置文件路径与精确 schema 官方未公开 → UNKNOWN**（UI 为主）。

### 3.6 DeepSeek Harness（DSH）

一手（官方文档站）：

- Skills 子系统：https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/skills
- MCP 接入（memory 示例）：https://deepseek-harness.github.io/deepseek-harness/en/guide/mcp-memory
- 插件配置（Cordis）：https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/config
- 仓库：https://github.com/deepseek-ai/deepseek-harness

**Skills 机制（官方规范级描述）**

- 是一个 **Provider 注册表**：`ctx.skills` 合并 local / embedded / remote provider；host + per-scope 分层，同名「最近的 scope 层直接胜出」，同层内按 `rank` → provider 顺序 → local 顺序。
- **本地发现优先级（rank）——这是 DSH 适配器最该抄的部分**：

| Rank | source | root |
| --- | --- | --- |
| 100 | `project-dsh` | `<projectRoot>/.dsh/skills` |
| 200 | `project-agents` | `<projectRoot>/.agents/skills` |
| 300 | `custom` | `Config.customSkillDirs` |
| 400 | `user-dsh` | `<dshHome>/skills` |
| 500 | `user-agents` | `<agentsHome>/skills` |
| 600 | `bundled` | `Config.bundledSkillDir` |

  （project root = 最近的含 `.git` 祖先；没有则 cwd。user DSH root 跳过 `.system` 子目录。）
- **形态**：接受目录包 `<name>/SKILL.md` 与**扁平文件 `<name>.md`**；**不支持嵌套递归 `**/SKILL.md`**。
- **命名**：kebab-case，正则 `^[a-z0-9]+(?:-[a-z0-9]+)*$`（比 Agent Skills 规范略严：不允许 `--`、不允许首尾 `-`，只允许 ASCII）。
- **frontmatter 扩展**：读 `disable-model-invocation` 与 `user-invocable`（缺省 true），归一化为 `{modelInvocable, userInvocable}` 双开关。
- **模型可见目录**：只注入 `name` + `description`（XML 转义），**不含 body、路径、来源**；`catalogDescriptionMaxLength` 默认 **500**（最小 3）。目录变化通过 digest 比对后整段替换注入；不完整快照保留 last-good。
- `skill({name})` 工具加载时重新读 body 并按 cwd 重新发现；返回 `<skill_content>` / `<skill_resources>` / `<skill_instructions>`。
- 热更新：chokidar 监听已存在 root（目录/扁平条目增删、直接 skill 条目变更）；watcher 失败只让当前观测「不完整」，不隐藏可直接加载的 candidates。

**MCP（DSH）**

- 走 Cordis 插件的 YAML 配置，**不是 JSON**。示例（官方「Bring another MCP server」）：

```yaml
- insert:
    - id: memory-my-server
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: my-memory
        transport: stdio
        command: my-memory-mcp
        args: []
        env: {}
        cwd: !!js process.cwd()
```

  远程改为 `transport: streamable-http`、`url`、`headers`。
- 工具命名空间：`mcp__<serverName>__<tool>`。
- 生效方式：`dsh web --patch <file.cordis.yml>`；持久化则把 patch 合并进 **`$DSH_HOME/profiles/<name>/cordis.patch.yml`**（单 profile）或 **`$DSH_HOME/cordis.patch.yml`**（全机器）。
- 安全：stdio 桥接在启动子进程前**移除名字像凭据的环境变量与所有 `DSH_*` 变量**；需要额外 secret 时显式写进 `config.env`。
- 健壮性：stdio server 崩溃会按 backoff 自动重连并重同步工具；重连预算耗尽后注销工具并停止重连，直到 reload/restart；初始发现是异步的。

**profiles 结构（官方 + 本机观察）**

- 官方：profile 目录 `$DSH_HOME/profiles/<name>/`；用户图层 `cordis.patch.yml`；全局层 `$DSH_HOME/cordis.patch.yml`。
- 本机观察（只读，旁证）：`~/.dsh/profiles/{web,desktop}/` 各含 `cordis.yml`（4 行，注释说明「空 entry list，树由 patches 组合」）、`cordis.patch.yml`、`package.json`（`dsh.profile.bundles` 列出 `@deepseek-ai/dsh-base`、`@deepseek-ai/dsh-web-app`）、`pnpm-lock.yaml`、`pnpm-workspace.yaml`、`node_modules`。
- 本机技能来源：`~/.dsh/skills` **不存在**，而 `~/.agents/skills/` 存在且内容与本会话技能目录一致 → 实际生效的是 rank 500 的 `user-agents`（与文档一致）。**最终结论由负责本机的同事确认。**

### 3.7 各 Agent 形态速查

| Agent | Skill 是否文件式 | Skill 用户级路径（官方） | MCP 配置形态 | 是否支持本地 MCP |
| --- | --- | --- | --- | --- |
| Claude Code / Code | 是 | `~/.claude/skills`、`.claude/skills`、（企业/插件/额外目录） | JSON `.mcp.json` / `~/.claude.json`，`mcpServers` | 是（stdio） |
| Claude Desktop | 否（本次未研究其文件式 skill；文件式路径在 Claude Code 侧） | — | JSON `claude_desktop_config.json`，`mcpServers` | 是（stdio，官方 quickstart） |
| OpenAI Codex | **是** | `~/.codex/skills`（递归） | TOML `[mcp_servers.*]` | 是 |
| ChatGPT | **否（账号云端）** | 无 | UI（远程 URL / Tunnel / OAuth） | **不直接支持**（Tunnel 或插件内 local app） |
| TRAE | 是 | `~/.trae-cn/skills`（CN）/ `~/.trae/skills`（国际，待确认） | JSON `mcpServers`（`.trae/mcp.json`） | 是 |
| ZCode | 是 | `~/.zcode/skills` | JSON `mcp.servers`（+`.agents/mcp.json`） | 是 |
| WorkBuddy | 是（技能包，UI 安装） | UNKNOWN | UI 连接器 / 自定义（类 `mcpServers`） | 是（MCP+CLI 连接器） |
| CodeBuddy IDE | UNKNOWN | `~/.codebuddy/skills`（第三方登记，未验证） | JSON `mcpServers` | 是 |
| DSH | 是 | `~/.dsh/skills` + `~/.agents/skills` | YAML Cordis `insert` 行 + `@deepseek-ai/dsh-mcp-client` | 是 |

---

## 4. 参考项目研究（只借鉴思路，不复制代码）

### 4.1 `xingkongliang/skills-manager`

- 仓库：https://github.com/xingkongliang/skills-manager （MIT；Rust 桌面应用；约 6.5k stars / 527 forks，2026-10 数据）
- 站点：https://skillsmanager.dev
- 技术栈（README 原文 Tech Stack）：React 19 + TypeScript + Vite + Tailwind（前端）、**Tauri 2**（桌面）、**Rust**（后端）、**SQLite（rusqlite）**、react-i18next。
- 定位：跨 50+ 编程 Agent 的 skill 管理/同步/组织，中心库默认 **`~/.skills-manager`**，可自定义；CLI 与桌面共用同一 Rust core + SQLite。

**4.1.1 适配器注册表设计（源码 `src-tauri/src/core/tool_adapters.rs`）**

`ToolAdapter` 结构（字段级摘录，非逐字复制）：

| 字段 | 作用 |
| --- | --- |
| `key` / `display_name` | 稳定标识 + 展示名 |
| `relative_skills_dir` | 相对 HOME 的全局技能目录（如 `.claude/skills`） |
| `relative_detect_dir` | 判断「该 Agent 是否已安装」的探测目录（如 `.claude`） |
| `additional_scan_dirs` | **只用于发现、不用于部署**的额外目录（如 Codex 也扫 `.agents/skills`） |
| `override_skills_dir` | 用户覆盖的绝对路径（Settings 里改路径） |
| `is_custom` | 用户自定义 Agent |
| `recursive_scan` | 是否递归找含 `SKILL.md` 的目录（如 Hermes Agent 有嵌套分类目录） |
| `project_relative_skills_dir` | 项目级路径与全局不同的情况（如 OpenCode：全局 `~/.config/opencode/skills` vs 项目 `.opencode/skills`） |
| `category` | 纯 UI 分组（Coding / Lobster） |

配套：`home() / select_existing_or_default()` 处理「目录不存在时选默认」；`is_installed()` 用 detect dir 探测；`custom_tools()` 从 DB 读自定义工具；`find_adapter(key)`；`enabled_installed_adapters()` 只列「用户启用 + 已安装」。源码里还有大量**带 issue/PR 编号的注释**解释路径为何如此（例如 Codex 的 `.agents/skills` 只是发现兜底、`AGENT_SKILLS_PATH` 是待落地的上游提案）——这种「路径决策留痕」非常值得抄。

**4.1.2 数据模型（`src-tauri/src/core/migrations.rs`，SQLite）**

核心表（字段摘录）：

- `skills`：`id`、`name`、`description`、`source_type`、`source_ref` / `source_ref_resolved` / `source_subpath` / `source_branch` / `source_revision` / `remote_revision`、`central_path`（UNIQUE）、`content_hash`、`enabled`、`status`、`update_status`、`last_checked_at`、`last_check_error`、`created_at/updated_at`。
- **`skill_targets`：`id`、`skill_id`(FK)、`tool`、`target_path`、`mode`（symlink/copy）、`status`、`synced_at`、`last_error`、`source_hash`，且 `UNIQUE(skill_id, tool)`。** ← 这是「部署事实表」，AgentHub 必抄。
- `discovered_skills`：扫描到的外来 skill（tool、found_path、name_guess、fingerprint、found_at、imported_skill_id）——支持「Agent 目录里已有但我没装的 skill」展示。
- `settings`、`skillssh_cache`、`projects`（含 `workspace_type`、`linked_agent_key`、`disabled_path`）、`skill_tags`。
- `scenarios` / `scenario_skills` / `scenario_skill_tools` / `active_scenario`：**Preset（预设）** = 技能分组 + 每个 tool 的启用状态（`scenario_skill_tools.tool + enabled`）。
- `audit_log`（带 `idx_audit_log_ts`）。
- `pending_conflicts`（v6→v7）：本地 UI 缓存；**真源是 git commit trailers + `refs/skills-manager/conflict/*`**，启动与每次 merge 后可从 git 重建。

**4.1.3 symlink vs copy（`src-tauri/src/core/sync_engine.rs`）**

- 模式只有两种：`SyncMode::Symlink` / `SyncMode::Copy`，DB 里用字符串 `"symlink"` / `"copy"` 记录。
- **只信 `symlink_metadata`**（不跟随链接）来分类目标：区分「真目录」「目录符号链接」「Windows junction」「文件」；源码注释明确「a row saying `symlink` must never authorize deleting a real directory」——即**记录不能授权破坏**。
- Windows：先 `symlink_dir`；无权限则退 **junction**（本地 NTFS 无需特权）；再失败才 **copy**。junction 在 std 里表现为 symlink，因此仍记为 symlink 模式。
- 防御：`src` 与 `dst` 相同/互相包含、dst 是 src 的符号链接等情况**直接拒绝**，避免「删 dst 把 src 删了」。
- 幂等判断：symlink 模式校验「链接是否指向 source」；copy 模式用 `source_hash` 决定是否重拷（保留「总是重拷」的历史行为）。
- `ReplacePolicy`：`Recorded{mode}`（按记录）、`UserConfirmed`（用户确认）、`Adopt`（把已有目录收编进库）等——**没有一种策略允许静默覆盖外来目录**。

**4.1.4 备份 / 回滚 / 冲突（`src-tauri/src/core/git_backup.rs`）**

- 备份 = 一个普通 Git 仓库（可 `git clone`，无锁定）；快照 = annotated tag（`sm-v-*`）+ 历史记录；`Back Up Now` 立即执行；后台「停止编辑几分钟后」自动 commit+push。
- **skill-aware merge**：按 skill 合并而非按行（一端改名 + 另一端改内容可干净合并）；真正的冲突**从不阻塞也不覆盖**：冲突 skill 保持本地版本、标 `Needs attention`，其余照常同步；用户选 **keep mine / use remote / keep both**，**选择前先自动 snapshot** → 每步可撤销。
- 冲突状态持久化在 **commit trailer + `refs/skills-manager/conflict/*`**，`pending_conflicts` 只是投影；`SYNC_CONFLICT` 错误码给 UI。
- 包含范围：skills、tags、presets、per-agent 开关；**不含 secrets、机器相关 wiring、`>100MB` 的 skill**；SQLite DB 不进 Git（元数据可从文件重建）。
- 认证：GitHub device flow；token 存 **OS keychain**（不落文件/仓库配置）。

**4.1.5 UI 信息架构（可借鉴）**

侧边栏：Library（中心库）/ Global Workspace（按 Agent 分页，含「非本工具安装」的 skill）/ Project Workspaces（项目级，双向同步）/ Linked Workspaces（任意目录作为 skills root）/ Backup / Settings。
组件模式：skill 卡片 + **每个已启用 Agent 的图标徽章**（点徽章即时 install/remove，徽章反映实时同步态）；`+ Add Skills` 统一选择器（选库 + 多 Agent chips + 批量）；Preset pill（一键激活一组，✓/计数徽章）；Filters（source/tag/Untagged）；Skill 详情（读 `SKILL.md`/`README`、来源元数据、与上游 diff）。
另外两个「产品化」细节：**「让 Agent 管理 skills」**（把 `manage-skills` skill 部署到目标 Agent，Agent 通过 CLI 驱动本体，而不是背后直接写目录）；**CLI 与桌面共用同一 core/DB**，CLI 二进制由 app 发布到 `~/.skills-manager/bin/`。

### 4.2 `MichengAI/dsh-skills-manager`

- 仓库：https://github.com/MichengAI/dsh-skills-manager （Apache-2.0；npm `@michengai/dsh-skills-manager`；DSH **Web 插件**，社区维护，非 DeepSeek 官方）
- 形态：DSH 插件，安装 `dsh plugin --profile web add @michengai/dsh-skills-manager@latest`；源码 `src/`，严格 TypeScript。
- 模块划分（源码树，共 125 个 blob）：`core.ts`、`client.ts`、`client-types.ts`、`host-types.ts`、`plugin-ui.ts`、`catalog-skill.ts`、`skills-sh-catalog.ts`、`clawhub-catalog.ts`、`readonly-discovery.ts`、`repositories.ts`、`repository-ui.ts`、`repository-updates.ts`、`destination-picker.ts`、`destination-types.ts`、`source-select-model.ts`、`source-select.ts`、`types.ts`、`plugin-updater.ts`、`plugin-update-ui.ts`、`plugin-update-model.ts`。

**4.2.1 Agent 路径注册表（README「Supported Agent directories」）**

表结构 = `Source | Global directory | Environment variable | Project directory`，共 18 个来源，默认路径 + **环境变量覆盖**（覆盖「Agent 主 home 根目录」，插件再扫其下 `skills`）。节选（原文）：

| Source | Global | Env | Project |
| --- | --- | --- | --- |
| DSH | `~/.dsh/skills` | `DSH_HOME` | `<project>/.dsh/skills` |
| Shared Agent | `~/.agents/skills` | `DSH_AGENTS_HOME` | `<project>/.agents/skills` |
| Codex | `~/.codex/skills` | `DSH_CODEX_HOME` | `<project>/.codex/skills` |
| Claude Code | `~/.claude/skills` | `DSH_CLAUDE_HOME` | `<project>/.claude/skills` |
| Gemini | `~/.gemini/skills` | `DSH_GEMINI_HOME` | `<project>/.gemini/skills` |
| OpenCode | `~/.config/opencode/skills` | `DSH_OPENCODE_HOME` | `<project>/.opencode/skills` |
| Cursor | `~/.cursor/skills` | `DSH_CURSOR_HOME` | `<project>/.cursor/skills` |
| Copilot | `~/.copilot/skills` | `DSH_COPILOT_HOME` | `<project>/.github/skills` |
| Windsurf | `~/.codeium/windsurf/skills`、`~/.windsurf/skills` | `..._HOME`、`..._USER_HOME` | `<project>/.windsurf/skills` |
| Trae / Trae CN | `~/.trae/skills` / `~/.trae-cn/skills` | `DSH_TRAE_HOME` / `DSH_TRAE_CN_HOME` | `<project>/.trae/skills` / `.trae-cn/skills` |
| CodeBuddy | `~/.codebuddy/skills` | `DSH_CODEBUDDY_HOME` | `<project>/.codebuddy/skills` |
| WorkBuddy | `~/.workbuddy/skills` | `DSH_WORKBUDDY_HOME` | `<project>/.workbuddy/skills` |
| Qoder / Qoder CN CLI / Qoder CN | `~/.qoder/skills` / `~/.qoder-cn/skills` / `~/.lingma/skills` | … | … |

明确边界（原文）：「Ordinary project-root `<project>/skills` directories are not scanned, so repository release resources are not exposed as Agent skills.」；OpenClaw/Clawdbot 只支持用户级；缺失目录不展示。

**4.2.2 写边界与生命周期（这是最值得抄的部分）**

- 可写目标**只有三类**：用户级 DSH、Shared Agent（`~/.agents/skills`）、已注册工作区的 `.dsh/skills`；**Codex/Claude 等外部 Agent 目录只读**；插件管理缓存不可写；任意路径/未注册工作区不可写；缺失目录**确认后**才创建。
- 「Registered workspaces」：只扫描选中的工作区；Git 仓库用最近 Git root，普通目录用工作区目录（不要求 `git init`）。
- 「Independent copies」：同名 skill 多副本时，禁用某个副本会让另一个接替，面板标明当前生效来源；全部禁用才算关掉。
- 「Trash」：DSH/Shared Agent 的技能删除先进 **Trash**，可从管理器恢复；**不会自动清理历史备份**（明确说会占磁盘）。
- Repository 安装：公开 GitHub；安装**不覆盖本地文件**；更新前预览 diff、保护本地修改、确认后先备份再更新；支持 rollback；卸载进 Trash；**失败扫描保留旧目录**；archive 上限 **32MiB**；不自动更新、不支持私有凭据。
- 只读发现：`readonly-discovery.ts`——外部 Agent 的 skill 只读展示、可启停（启停写 DSH 配置而非改源文件）。

**4.2.3 与 skills-manager 的差异**

| 维度 | skills-manager | dsh-skills-manager |
| --- | --- | --- |
| 宿主 | 独立 Tauri 桌面（跨 Agent 管理） | DSH Web 插件（为 DSH 引入外部技能） |
| 写入对象 | 多 Agent 目录（symlink/copy 部署） | 仅 DSH/Shared Agent/已注册工作区（外部只读） |
| 数据库 | SQLite 事实表（skills/targets/audit…） | 文件系统为主 + 记录；无 SQLite |
| 备份 | 内建 Git 备份 + 快照 + 冲突解决 UI | 每个 skill 的备份/rollback + Trash |
| 分发 | marketplace（skills.sh）+ Git/zip/.skill | 公开 GitHub 仓库安装 + skills.sh/clawhub 目录浏览 |
| 关闭语义 | 删除 deployment（target 行） | 写 DSH 配置开关，不动源文件 |

### 4.3 可复用设计 vs AgentHub 必须自研

**值得直接借鉴（思路层）**

1. **适配器 = 数据**（key、global/project 双路径、detect dir、副作用扫描目录、递归开关、路径覆盖），并允许用户自定义工具/覆盖路径。
2. **部署事实表**（`skill_targets(skill_id, agent, path, mode, status, hash)` + `UNIQUE`），所有「装没装/怎么装的」问题查表，而不是猜文件系统。
3. **默认 symlink、copy 兜底、junction 过渡**；只信 `symlink_metadata`；记录不得授权破坏真实目录。
4. **不覆盖外来目标**：`TARGET_CONFLICT` + 明确冲突列表（path 作为数据返回）。
5. **冲突不阻塞**：其余照常同步，冲突项保留本地 + Needs attention + 选择前 snapshot。
6. **Trash 优先于 delete**；卸载/删除可恢复。
7. **备份用普通 Git**（用户可 clone，无锁定）+ keychain 存 token + secrets 永不入备份。
8. **安装前预览**（SKILL.md/README、来源元数据、与上游 diff）。
9. **「让 Agent 管理自己」**：提供一个 `manage-skills` skill + 本地 CLI，Agent 通过受控接口操作，而不是各自乱写目录。
10. **路径决策留痕**：在注册表条目旁写清「为什么是这个路径 / 上游 issue 编号」。

**AgentHub 必须自己实现（参考项目没有或不够）**

1. **多 Agent 分发矩阵**：一个 skill × N 个 Agent × 2 个 scope，按 Agent 能力（symlink? 递归? 扁平 md? 命名/描述限制?）生成**不同的落盘形态**（例如 DSH 允许 `<name>.md` 扁平，Claude 要目录；Codex 历史上 name≤100/desc≤500）。参考项目多为「同构复制」，没有 per-agent 内容适配。
2. **事务回滚**：跨多个 Agent + 多文件（JSON/TOML/YAML + 目录树）的「全成功或全回滚」。参考项目是「逐 target」操作 + 备份，不是事务。需要：计划（plan）→ 预检 → 逐操作 journal → 失败逆序补偿 → 校验。
3. **MCP 多格式适配**：参考项目只管 skill 文件，**都不管 MCP**。AgentHub 需要 MCP codec（JSON/TOML/YAML/UI-only）+ 字段映射 + 未知键透传 + 敏感值处理 + 试连。
4. **格式保真写入**：TOML（Codex）与带注释的 YAML/JSONC 很难「无副作用」编辑——需要决定「保格式」还是「重排 + 备份 + diff 预览」。
5. **安全审阅流水线**：安装前静态检查（脚本、`allowed-tools`、动态注入、可疑命令）+ 来源签名/哈希。
6. **跨 Agent 冲突策略**：同一 skill 在 N 个 Agent 里被本地改过时的三方合并/提示（参考项目只在「库 ↔ git」层面做）。

---

## 5. MCP 配置格式对照表（核心交付物）

> 证据等级：**A = 官方规范/官方文档**；**B = 官方仓库生成物（schema/源码）**；**C = 本机样例（待确认）**；**UNKNOWN = 未确认**。
> 注意：**MCP 规范本身不定义该表**（§2.5），以下是各客户端私有格式的对照。

### 5.1 主表

| 客户端 | 配置文件（官方） | 顶层 key | stdio 字段名 | env 传递 | 远程 url/headers | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json`（Win `%APPDATA%\Claude\...`） | `mcpServers` | `command`、`args` | `env` 对象（字符串→字符串） | 官方 quickstart 只演示 stdio；远程/`url`/`headers` **UNKNOWN** | A（MCP docs）+ A（MCP docs，路径） |
| Claude Code | `.mcp.json`（项目）、`~/.claude.json`（local/user）、`--mcp-config` | `mcpServers` | `type`（`stdio`/`http`/`sse`/`ws`）、`command`、`args`、`env` | `env` 对象；支持 `${VAR}`/`${VAR:-default}` 展开 | `url` + `headers` + `headersHelper`；`type: http`（`streamable-http` 为别名） | A |
| OpenAI Codex | `~/.codex/config.toml`（`$CODEX_HOME`） | **`[mcp_servers.<name>]`**（TOML 表） | `command`、`args`、`env`、`cwd`、`startup_timeout_sec`/`_ms`、`tool_timeout_sec`、`enabled`、`required`、`enabled_tools`/`disabled_tools`、`startup_readiness` | **嵌套表 `[mcp_servers.<name>.env]`**；另有 `env_vars`（继承环境变量）与全局 `shell_environment_policy` | `url` + `http_headers` + `env_http_headers` + `bearer_token_env_var` + `oauth{}` + `auth`（`oauth`/`chatgpt`/`ema_auth`） | B（生成的 `config.schema.json`）+ C（本机 config.toml 的 `[mcp_servers.*]` 与 `.env`） |
| TRAE | 项目 `<project>/.trae/mcp.json`；全局路径 **UNKNOWN**（UI 管理） | `mcpServers` | `command`（必填，不能含空格）、`args`、`env` | `env` 对象；**超时也走 env**：`START_MCP_TIMEOUT_MS`、`RUN_MCP_TIMEOUT_MS` | `url`（必填 http/https）、`headers`；超时也可放 headers | A |
| ZCode | 用户 `~/.zcode/cli/config.json`；工作区 `<project>/.zcode/config.json`；兼容 `~/.agents/mcp.json` / `<project>/.agents/mcp.json` | **`mcp.servers`**（原生）；`.agents` 文件用 `mcpServers` | `command`、`args`、`env`；`enable: false` 表示停用 | `env` 对象 | `url` + `headers`（官方 UI 说明；原生 JSON 远程字段结构 **待确认**）；另有 `oauth` 开关语义（UI 授权） | A |
| WorkBuddy | UI 连接器；自定义连接器 **类 MCP 配置**（官方指向 CodeBuddy IDE 文档）；文件路径 **UNKNOWN** | 参考 `mcpServers` | 参考 `type: "stdio"`、`command`、`args`、`env` | `env` 对象 | 参考 `url`/`headers`；OAuth 走 UI | A（连接器形态）+ A（CodeBuddy 格式）+ UNKNOWN（路径） |
| CodeBuddy IDE（同族） | 设置内 JSON 配置（路径未写明） | `mcpServers` | `type`、`command`、`args`、`env` | `env` 对象 | `url`/`headers`（推断，文档示例仅 stdio） | A（stdio 示例） |
| DSH | `$DSH_HOME/profiles/<name>/cordis.yml` / `cordis.patch.yml`；全局 `$DSH_HOME/cordis.patch.yml`；`--patch` | YAML 数组 `- insert: [{ id, name, config }]` | `name: '@deepseek-ai/dsh-mcp-client'`、`config.serverName`、`config.transport: stdio`、`config.command`、`args`、`env`、`cwd` | `config.env` map（**启动前会剥离 `DSH_*` 与类凭据环境变量**） | `transport: streamable-http` + `url` + `headers` | A + C（本机 profile 结构） |
| ChatGPT | **无本地配置文件** | — | — | — | UI 填 **public HTTPS endpoint** 或 **Tunnel**；OAuth | A |

### 5.2 差异要点（写适配器时逐条对照）

1. **顶层 key 至少有 4 种**：`mcpServers`（Claude/TRAE/CodeBuddy、`.agents`）、`mcp_servers`（Codex TOML）、`mcp.servers`（ZCode 原生）、Cordis `insert` 行（DSH）。→ 一个 codec 一种，不要试图统一写。
2. **env 的形态**：JSON 对象 / TOML 嵌套表 / YAML map；且 Codex 额外有 `env_vars`（只列变量名、继承进程环境），DSH 会**主动剥离**凭据类变量——「env 语义不同」比「字段名不同」更危险。
3. **url/headers 的支持度差异大**：Claude Desktop 官方只演示 stdio；Codex 最全（header + env header + OAuth + keyring）；TRAE/ZCode/WorkBuddy 只到 `url`+`headers`。
4. **超时字段五花八门**：Codex `startup_timeout_sec`/`tool_timeout_sec`；TRAE 用 env/headers 传 `*_TIMEOUT_MS`；Claude Code 的 server 条目支持 `timeout` 与 `alwaysLoad`（官方 ws 段落明确「与 http 相同的字段」）；DSH 由插件 config 决定。→ 内部模型建议 `timeout_startup_ms` / `timeout_tool_ms`，落盘时按 codec 翻译。
5. **启用/停用表达**：ZCode `"enable": false`；Codex `enabled = false`；Claude Code 用 settings 的 `disabledMcpServers`/`disabledMcpjsonServers`；DSH 用插件行 `disabled`。→ 不能假设「删掉即停用」。
6. **只读/不可写客户端**：ChatGPT（无本地文件）、WorkBuddy（UI-only 为主）→ AgentHub 只做「导出为可粘贴 JSON / 引导」。
7. **安全默认**：Codex OAuth 凭据默认 keyring；Claude Code 会告警 hidden whitespace、保留名、`${VAR}` 未展开；DSH 剥离凭据变量。→ AgentHub 的 env/headers 处理应默认「只显示 key、不显示 value、不写日志」。

### 5.3 各 Agent 对 Skill 的额外限制（与规范不同处）

| Agent | 比 Agent Skills 规范更严/不同的点 | 证据 |
| --- | --- | --- |
| Codex | 历史文档：name ≤100、description ≤500，单行化；**只注入 name/desc/path**；递归扫描；实验期需 `[features] skills = true` | https://github.com/openai/codex/blob/2c6995ca4dfc23b93db311b59c1b4ead464658b1/docs/skills.md |
| DSH | name 必须匹配 `^[a-z0-9]+(?:-[a-z0-9]+)*$`（仅 ASCII kebab）；**不支持嵌套递归**；支持扁平 `<name>.md`；目录描述默认截 500 字符；双开关 `disable-model-invocation`/`user-invocable` | https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/skills |
| ZCode | description >1024 **整技能丢弃**；正文 >100KB 截断；目录元数据里 desc 摘要最多 250 字符；插件技能必须单层目录 | https://zcode.z.ai/cn/docs/skill |
| Claude Code | spec 六字段之外的扩展字段（`disable-model-invocation`、`user-invocable`、`context`、`agent`、`model`、`paths`、`arguments`…）；`allowed-tools` 支持 YAML 列表 | https://code.claude.com/docs/en/skills |
| TRAE | 目录布局示例用 `examples/`/`templates/`/`resources/`；`.agents/skills` 需开关且 `.trae/skills` 优先；可从 zip/SKILL.md 导入 | https://docs.trae.cn/ide_skills |
| ChatGPT | 无文件路径；上传要过扫描（Needs Review/Blocked）；workspace 权限模型 | https://help.openai.com/en/articles/20001066-skills-in-chatgpt |
| WorkBuddy | 技能包上传/市场安装；开关不改源文件；`.agents` 复制来源 | https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Skills-Market |

---

## 6. UNKNOWN / 需本机验证清单

> 交付给负责本机核验的同事：以下条目无法从公开一手资料确认，不能写成「已确认」。

**高优先级（影响 MVP 写入能力）**

1. **Codex 现行官方文档**（`developers.openai.com/codex/config-reference`、`/codex/skills`）本次被 403 拦截；现行 Skills 的字段限制/路径/是否仍需 feature flag，需本机版本文档或 `codex --version` 实测。已知：本机 `~/.codex/skills/` 存在、`config.toml` 无 `skills = true`。
2. **Codex 项目级 skills 路径**：`<project>/.codex/skills` 是否现行支持（历史文档只说 user-level；第三方适配器同时登记 global/project）。需实测。
3. **Claude Desktop 配置是否支持 `url`/`headers`**（远程 server）：官方 connect-local-servers 只演示 stdio；需查现行 Claude Desktop 帮助或实测。
4. **TRAE 全局 `mcp.json` 的精确路径**（官方只在 UI 里管理，未给路径）；以及国际版 `~/.trae/skills` vs `~/.trae-cn/skills` 的差异。
5. **WorkBuddy 的本地技能目录与 MCP 配置落盘位置**（官方 UI-only；第三方登记 `~/.workbuddy/skills` 未验证）。
6. **ZCode 原生 `mcp.servers` 中远程 server 的字段名**（`url`/`headers` 是否与 `.agents/mcp.json` 完全一致）。
7. **DSH 本机 skills 实际生效根**：`~/.dsh/skills` 不存在、`~/.agents/skills` 存在；需确认 `agentsHome` 配置与 rank 500 是否就是唯一来源。

**中优先级（影响兼容性）**

8. ChatGPT 桌面端「插件内 local MCP app」的配置文件/打包格式（官方只说在插件里，未公开 schema）。
9. CodeBuddy IDE 全局 MCP 配置文件路径与是否支持远程字段（文档只给 stdio 示例）。
10. 各客户端对「符号链接 skill 目录」的支持差异（Claude Code 文档说支持 symlink 目录；其它家多为 UNKNOWN）。
11. Claude Code 的 skill 热更新行为（官方提到 watch `.claude/skills/`，但不同加载位置行为不同）→ 部署后是否需要重启 Agent。
12. macOS 上各 Agent 的实际安装位置（`/Applications/*.app`）与 CLI 是否随 app 提供。

---

## 7. 对 AgentHub 的设计含义（MVP 建议与风险）

### 7.1 MVP 建议（按优先级）

**P0 — 必须做对**

1. **统一领域模型**：`Skill`（canonical，含源/哈希/校验结果）、`AgentAdapter`（路径+能力+限制）、`Deployment`（skill×agent×scope×path×mode×status×hash）、`McpServer`（transport+字段+凭据引用）、`Workspace`、`AuditLog`。
2. **校验器**：严格按 Agent Skills 规范（name 64/字符集/首尾/`--`/目录名；desc 1024）+ 目标 Agent 附加限制（§5.3）做成「错误/警告」两级。
3. **Skill 部署**：全局+项目双 scope；symlink 优先、copy 兜底（Windows junction 再退 copy）；**不覆盖非我拥有的目标**（`TARGET_CONFLICT`）；全部操作写 audit。
4. **MCP 适配**：至少覆盖 **Claude Code（JSON `mcpServers`）、Codex（TOML）、TRAE（JSON `mcpServers`）、ZCode（JSON `mcp.servers`）、DSH（YAML insert）** 五个 codec；统一内部模型；未知键透传；写入前 diff + 备份 + 原子替换。
5. **安全基线**：stdio 命令完整预览 + 显式确认；env/headers 值不在 UI 列表与日志中出现；安装前展示脚本/`allowed-tools`/动态注入告警。

**P1 — 差异化**

6. **试连（Probe）**：stdio 启动 → `initialize`（2025-xx）或 `server/discover`（2026-07-28）→ `tools/list` → 断开；展示工具数与能力。内置超时/取消/进程清理（SIGTERM→SIGKILL）。
7. **Trash + 回滚**：卸载进 Trash；每次写入前 snapshot；支持一键恢复。
8. **分发矩阵视图**：skill × agent 的矩阵（已装/未装/有冲突/被外部修改），并显示「实际生效来源」（借鉴 dsh-skills-manager 的 independent copies 语义）。
9. **导入器**：从 Claude/Codex/TRAE/ZCode/.agents 导入既有 skill 与 MCP（ZCode 已证明这条路可行，且给出 4 个「事实标准」来源）。

**P2 — 以后**

10. Git 备份/多端同步（skills-manager 的 skill-aware merge + refs + snapshot 是成熟方案，但工程量大）。
11. 市场/仓库安装（skills.sh、GitHub registry）。
12. 「让 Agent 管理 skills」的 CLI + manage-skills skill。

### 7.2 关键风险（按严重度）

| 风险 | 说明 | 缓解 |
| --- | --- | --- |
| **格式漂移** | Codex/ChatGPT/TRAE/ZCode 的私有格式无版本承诺（Codex MCP schema 已有 30+ 字段且还在变） | codec 独立版本化；未知键透传；写入前快照；文档注明「best-effort」 |
| **MCP 协议换代** | 2026-07-28 无状态化与主流 2025-xx 不兼容；2025-06-18 与 2025-11-25 也有差异 | 内部只存「用户意图」；协议版本由目标 Agent 决定；Probe 双路径 |
| **TOML/YAML 重写破坏** | 用通用解析器重写会丢注释/顺序/`!!js` 标签 | 优先「结构化编辑 + 保留原文 diff」；不行则全量重排 + 备份 + 明确告知 |
| **破坏 Agent 资产** | ChatGPT/WorkBuddy 是账号/UI 资产，无文件格式；写文件无效甚至有害 | MVP 只读/导出；不做写入 |
| **并发与外部修改** | 用户 IDE/Agent 同时在改 `.claude/skills`；watcher 与写操作竞态 | 写入前读取+hash 校验（乐观并发）；冲突不阻塞、保留本地 |
| **符号链接安全** | 记录与实际不一致、目标被替换成真实目录 | 只信 `symlink_metadata`；记录不授权删除真实目录；拒绝 src/dst 互相包含 |
| **凭据泄露** | MCP env/headers 里全是 token | 只存引用（keychain/占位符）；日志脱敏；备份排除 secrets |
| **同一 skill 多副本** | 用户级与项目级、`.agents` 与原生目录并存 | 明确优先级（project > user；原生 > `.agents`，参考 TRAE/DSH rank）；UI 显示生效来源 |
| **Windows** | symlink 需权限；路径长度；WSL 与 Windows 目录互操作 | junction 兜底 → copy；WSL 场景检测并提示 |
| **Skill 即代码** | 安装第三方 skill 等于执行其脚本 | 安装预览 + 静态检查 + trust gate（项目级默认需要用户确认） |

### 7.3 一句话给架构组

> AgentHub 的核心不是「文件同步器」，而是 **「多 Agent 能力注册表 + 事务化的配置写入器 + 可解释的校验器」**。参考项目已经证明：路径注册表、部署事实表、不覆盖外来目标、Trash/快照，是这类产品能活下来的最小集合；而 MCP 多格式与跨 Agent 事务，是 AgentHub 必须自己补的两块。

---

## 附录 A：一手链接清单

**Agent Skills**
- https://agentskills.io/specification
- https://agentskills.io/client-implementation/adding-skills-support
- https://agentskills.io/skill-creation/best-practices
- https://github.com/anthropics/skills
- https://anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
- https://code.claude.com/docs/en/skills
- https://code.claude.com/docs/en/mcp

**MCP**
- https://modelcontextprotocol.io/specification/2026-07-28
- https://modelcontextprotocol.io/specification/2026-07-28/changelog
- https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle
- https://modelcontextprotocol.io/specification/2025-06-18/basic/transports
- https://modelcontextprotocol.io/specification/2026-07-28/basic/transports
- https://modelcontextprotocol.io/specification/2024-11-05/basic/transports
- https://modelcontextprotocol.io/docs/2025-11-25/tutorials/security/security_best_practices
- https://modelcontextprotocol.io/docs/develop/connect-local-servers

**各家 Agent**
- OpenAI Codex：https://developers.openai.com/codex/config-reference （本次 403）；https://github.com/openai/codex/blob/main/codex-rs/core/config.schema.json；https://github.com/openai/codex/blob/2c6995ca4dfc23b93db311b59c1b4ead464658b1/docs/skills.md
- ChatGPT：https://help.openai.com/en/articles/20001066-skills-in-chatgpt ；https://help.openai.com/en/articles/20001256-plugins-in-chatgpt ；https://help.openai.com/en/articles/12584461-connecting-custom-mcp-servers-in-chatgpt ；https://help.openai.com/en/articles/11487775-connected-apps-in-chatgpt
- TRAE：https://docs.trae.cn/ide_skills ；https://docs.trae.cn/ide_model-context-protocol ；https://docs.trae.cn/ide_add-mcp-servers ；https://docs.trae.ai/ide/skills
- ZCode：https://zcode.z.ai/cn/docs/skill ；https://zcode.z.ai/cn/docs/mcp-services
- WorkBuddy：https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Skills-Market ；https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Connector ；https://www.codebuddy.cn/docs/ide/User-guide/MCP
- DSH：https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/skills ；https://deepseek-harness.github.io/deepseek-harness/en/guide/mcp-memory ；https://deepseek-harness.github.io/deepseek-harness/en/develop/basic/config ；https://github.com/deepseek-ai/deepseek-harness

**参考项目**
- https://github.com/xingkongliang/skills-manager （README / `src-tauri/src/core/tool_adapters.rs` / `migrations.rs` / `sync_engine.rs` / `git_backup.rs`）
- https://github.com/MichengAI/dsh-skills-manager （README / `src/`）

## 附录 B：术语

- **Skill**：目录 + `SKILL.md`（元数据 + 指令），按需加载。
- **Agent Skills**：`skill` 的开放格式规范（agentskills.io）。
- **Progressive disclosure**：元数据 → 正文 → 资源 三层加载。
- **MCP**：JSON-RPC 的 Model Context Protocol；Host/Client/Server。
- **Streamable HTTP**：2025-03-26 起的 MCP 远程传输（取代 2024-11-05 的 HTTP+SSE）。
- **Deployment / Target**：某个 skill 在某个 Agent 某个目录下的一次落盘记录。
- **Codec（本报告用法）**：内部统一模型 ↔ 某客户端私有配置格式的互转器。
