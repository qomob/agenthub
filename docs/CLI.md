# AgentHub CLI

`agenthub` CLI 与桌面 GUI 共用同一个 `agenthub-core` 引擎，行为一致。构建：`./scripts/cargo.sh build -p agenthub-cli`（产物 `target/debug/agenthub`）。


CLI 是同一引擎的薄前端，**默认只打印计划，不会写 Agent 目录**。全局参数：`--home <DIR>`（等价 `AGENTHUB_HOME`，用于隔离测试）、`--json`。

退出码：`0` 成功；`1` 失败；`2` 部分成功 / 计划包含 blocked/conflict / 缺少 `--yes`。

```bash
BIN=./target/debug/agenthub

# 诊断与 Agent 盘点
$BIN doctor
$BIN agents list

# 扫描（只读）
$BIN skills scan

# 导入（文件夹 / .zip / SKILL.md / Git）
$BIN skills import --path /path/to/my-skill
$BIN skills import --path /path/to/skill.zip
$BIN skills import --path /path/to/SKILL.md
$BIN skills import --git https://github.com/org/repo --ref main --skill-path skills/foo

# 列出库内技能
$BIN skills list

# 分发：先预览（退出码 2），确认后加 --yes
$BIN skills install <skill-id-or-slug> --agents codex,workbuddy --dry-run
$BIN skills install <skill-id-or-slug> --agents codex,workbuddy --yes

# 扫描发现 → 批量导入中央库（0.4.0）
# 先用 `skills scan` 生成最近一次扫描结果
$BIN skills adopt --dry-run                 # 预览将导入多少条（默认跳过只读来源）
$BIN skills adopt --agents codex,workbuddy --max 100 --yes
$BIN skills adopt --include-read-only --dry-run  # 显式包含 builtin/marketplace/workspace/connector

# 更新/漂移检查：生成计划（不写）
# 注意：仅有更新时 --dry-run 退出码 0；drift/missing/blocked/conflict 才 2
$BIN skills sync --dry-run
$BIN skills sync --yes

# 卸载：按 installation id（来自 skills list --json 的 installations[].id）
$BIN skills uninstall <installation-id> [<installation-id> ...] --dry-run
$BIN skills uninstall <installation-id> [<installation-id> ...] --yes

# 回滚最近一次可回滚操作（或指定 operation id）
$BIN skills rollback last
$BIN skills rollback <operation-id>

# 操作历史 / Profiles / 备份
$BIN operations list --limit 20
$BIN profiles list
$BIN backup create
$BIN backups list

# 健康与验证（第二轮新增）
$BIN health                         # 能力健康概览（verified/unverified/needs_attention/failed/not_deployed）
$BIN health --agents                # Agent 版本健康（只读）
$BIN health --agents --record       # 记录一次版本观测（首次为 baseline）
$BIN skills audit <skill>           # 静态安全审计；status=fail → 退出码 2
$BIN skills deps <skill>            # 依赖/环境诊断；needs_attention → 退出码 2
$BIN skills impact <skill> --to <version-id>   # 更新影响（默认 to=当前版本）
$BIN skills validate <skill> --agent codex --case "generate invoice PDF" --status passed \
  --notes "Codex 26.x" --evidence "log excerpt or link"

# MCP：列出、发现、分发（CLI 没有「创建 MCP 定义」命令；env 值不会进入 plan）
$BIN mcp list
$BIN mcp discover --agent codex
$BIN mcp distribute <server-id-or-name> --agents codex --dry-run
$BIN mcp distribute <server-id-or-name> --agents codex --yes
```

隔离测试（推荐先这样跑）：

```bash
export AGENTHUB_HOME=/tmp/agenthub-demo/home
mkdir -p "$AGENTHUB_HOME/.codex"
$BIN --home "$AGENTHUB_HOME" doctor
```

> 注意：`--home` 只隔离 Agent 目录与数据目录，**不会隐藏 `/Applications` 下真实安装的 app**；扫描可能列出本机真实 Agent。执行分发时显式指定 agent key，避免误写。

---

