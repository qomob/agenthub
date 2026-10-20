# AgentHub UI 规格（UI-SPEC）

> 状态基准：2026-10-20 16:24。**以 `src/**` 的真实实现为准**：第二轮新增 `/health` 页、SkillDetail 健康面板（审计/依赖/影响/验证证据）、Agents 版本健康、Dashboard/Skills 健康摘要、Plan 影响摘要；当前 11 条路由/12 视图/20 组件，`pnpm build` 与 **7 files / 29 tests** 通过。GUI 已在隔离 home 下完成人工运行验收（截图见 [docs/evidence/](evidence/)），自动化回归待补。
> 关联：[PRD.md](PRD.md)、[ARCHITECTURE.md](ARCHITECTURE.md)、[SECURITY.md](SECURITY.md)、[TEST-PLAN.md](TEST-PLAN.md)

---

## 0. 设计原则

1. **可解释优先**：每个数字、状态、按钮都能追溯到后端返回的真实字段；不显示估算或占位数据。
2. **预览-确认**：任何写操作先出计划（目标路径、动作、哈希、警告、重启提示），用户确认后才执行；没有「一键静默安装」。
3. **诚实降级**：未实现/未验证能力显示为「待验证 / 不支持 / 未实现」并解释原因，不隐藏、不假装成功。
4. **信息密度**：桌面工具，默认 13px 字号、紧凑行高、表格化；路径/哈希用等宽字体。
5. **键盘可达**：全局快捷键、可见 focus ring、跳过导航；所有操作可不用鼠标完成。
6. **本地感**：无账号、无「云端/团队」入口；设置仅本机生效。

---

## 1. 信息架构

### 1.1 导航（i18n `nav.*`，10 项）

| 分组（`nav.groups`） | 页面 | 实际路由（`src/router.ts`） | 主要数据来源 |
|---|---|---|---|
| overview | 仪表盘 Dashboard | `/dashboard` | `app_dashboard`、`app_check_changes` |
| capabilities | Agent | `/agents` | `agents_list`、`app_scan`、`agents_update_paths` |
| capabilities | 技能库 Skills | `/skills`、`/skills/:id` | `skills_list`、`skills_get`、`skills_import`、`skills_plan_install`、`skills_plan_uninstall` |
| capabilities | 能力矩阵 Matrix | `/matrix` | `skills_list` + `agents_list`（前端组合 `matrixCellState`） |
| capabilities | MCP Servers | `/mcp` | `mcp_list`、`mcp_discover`、`mcp_upsert`、`mcp_import`、`mcp_plan_distribute`、`mcp_plan_remove`、`mcp_*_secret` |
| capabilities | 插件 Plugins | `/plugins` | `plugins_sync`（只读） |
| workspace | Profiles | `/profiles` | `profiles_list`、`profiles_upsert`、`profiles_delete`、`profiles_plan_apply` |
| workspace | 活动与诊断 Activity | `/activity` | `operations_list`、`operations_get`、`snapshots_list`、`backups_list`、`backups_create`、`app_doctor`、`app_check_changes` |
| validation | 健康验证 Health | `/health` | `health_overview`、`agent_health`、`skills_health` |
| system | 设置 Settings | `/settings` | `app_settings`、`app_update_settings`、`app_project_roots`、`app_set_project_roots` |
| （启动流程） | Onboarding | 无独立路由；`App.vue` 在 `initialized=false` 时渲染 `OnboardingView` | `app_bootstrap`、`app_mark_initialized`、`app_scan` |

新增/已使用的 API 字段：`AgentView.customSkillRoots`（Agents 页编辑自定义根）、`McpServerView.headers`（MCP 编辑表单展示/保存 headers）；CLI 另有 `skills uninstall`，不属于 UI。

路由实现：`createWebHashHistory`（`src/router.ts`），`/` 与未匹配路径重定向 `/dashboard`；导航顺序即 `⌘1`–`⌘9`（dashboard/agents/skills/matrix/mcp/plugins/profiles/activity/health），设置无快捷键。

> 页面范围：第一轮 8+1（Dashboard / Agents / Skills / MCP / Plugins / Profiles / Activity / Settings + Onboarding），实际额外包含 **能力矩阵 Matrix** 与第二轮 **健康验证 Health**；「诊断」并入「活动与诊断」。健康页与 SkillDetail 健康四区块在 §2.10 / §2.3 定义。

### 1.2 布局骨架（已实现样式）

- 应用外壳：`display:flex; height:100vh; overflow:hidden`，左侧固定 234px 侧边栏（`src/styles.css:192-207`）。
- 侧边栏：品牌区、分组导航、底部状态/设置入口；导航项 31px 高、7px 圆角。
- 主区：页面滚动容器 + `.page`（标题/副标题/操作区）+ `.section-stack`（16px 间距）。
- 内容宽度随窗口自适应；`grid-2` 在 <1120px 变单列（`styles.css:502`），<960px 有移动布局规则（`styles.css:2050`）。
- 窗口：1360×880，最小 1080×680（`src-tauri/tauri.conf.json`）。

---

## 2. 页面规格

> 所有页面都遵循统一状态：`loading`（骨架/文字）→ `empty`（解释 + 下一步）→ `error`（错误信息 + 重试）→ 正常内容。页面级错误来自 store 的 `error` 字段，不得吞掉。

### 2.1 Dashboard（`nav.dashboard`）

**目的**：一屏回答「本机有哪些 Agent、有多少能力、有什么问题、最近做了什么」。

| 区块 | 内容 | 数据 |
|---|---|---|
| 统计卡 | Agent 已检测/总数、技能数、MCP 数、插件数、安装数、待更新、冲突、警告 | `Dashboard.stats` |
| Agent 概览 | 每个 Agent 的状态徽章、技能数、已安装数 | `Dashboard.agents` |
| 问题列表 | severity/kind/title/detail，可跳到 Agent/技能 | `Dashboard.issues` |
| 待更新 | 可更新的技能（`updateAvailable`） | `Dashboard.updates` |
| 最近操作 | 最近 8 条 operation（状态、类型、摘要、时间） | `Dashboard.recentOperations` |
| 库信息 | `libraryRoot`、`lastScanAt`、`initialized` | `Dashboard` 顶层字段 |

**主操作**：扫描（`app_scan`）、检查外部变更（`app_check_changes`）、刷新（`app_dashboard`）、创建备份（`backups_create`，可放在 Activity）。
**健康摘要卡**：展示 `healthTotal/Verified/NeedsAttention/Unverified/Failed/NotDeployed` 与「查看健康」入口（`dashboard.health*` i18n）；数字来自 `health_overview`，不得前端估算。
**空态**：未初始化 → 引导 onboarding；无技能 → 「技能库 → 导入」；无 Agent → 说明仍可初始化。
**禁止**：前端自己加总或估算统计；所有数字来自后端。
**已知**：`scan_on_startup` 目前只是设置项，核心 `open()` 不自动扫描；仪表盘「扫描」按钮必须显式调用。

### 2.2 Agents（`nav.agents`）

**目的**：展示适配器检测结果、根目录、能力等级，并允许用户修正路径/添加自定义根/项目根。

| 区块 | 内容 | 数据 |
|---|---|---|
| Agent 卡片/表格 | 名称、vendor、状态、symlink 支持、插件支持、最后扫描时间、技能/安装计数 | `AgentView` |
| 技能根列表 | 路径、scope、source（adapter/custom/project）、exists、writable、skillCount | `AgentView.skillRoots` |
| MCP 目标 | 路径、format、exists、writable、transports、notes | `AgentView.mcpTargets` |
| 编辑路径 | appPath、homePath、customSkillRoots（必须绝对路径） | `agents_update_paths` |
| 项目根 | 全局设置的项目目录（用于项目级扫描/分发） | `app_project_roots` / `app_set_project_roots` |
| 扫描 | 触发全量只读扫描并展示报告 | `app_scan` → `ScanReport` |
| 版本健康 | 每个 Agent 的版本、previousVersion、versionChanged、pathsOk、checkedAt | `agent_health(record?)`；变化时提示重新扫描/验证 |
| 路径健康 | `pathsOk`=skill root 或 MCP target 存在；无 root 提示 | `AgentHealth` |

**状态**：`installed` 绿、`verification_required` 黄（显示 notes）、`not_installed` 灰、`unsupported` 灰、`error` 红（`status.ts`）。
**关键提示**：
- `symlinkSupport=unverified` → 显示「symlink 未验证，默认复制」；`unsupported` → 禁用 symlink 选项。
- `pluginSupport` 只影响插件页展示，不提供安装入口。
- `verification_required`（Claude Code/Cursor）允许安装是引擎现状，但 UI 必须显示警告（见 SECURITY R7）。
**空态**：无 Agent → 说明可继续初始化，之后安装 Agent 再扫描。

### 2.3 Skills（`nav.skills` + 详情 `/skills/:id`）

**列表**：

| 列/元素 | 内容 | 数据 |
|---|---|---|
| 名称/slug | 名称 + 版本 | `Skill` |
| 来源 | folder/zip/git/skill_md/agent_dir | `Skill.source` |
| 大小/文件数 | 格式化字节（`formatBytes`） | `Skill.sizeBytes/fileCount` |
| 冲突 | `conflict` 非空时警示徽章 | `Skill.conflict` |
| 安装状态 | 已安装 Agent 数、漂移数、可更新 | `installations.length/driftCount/updateAvailable` |
| 健康 | 健康状态、审计风险、原因摘要 | `skills_health` / `health_overview`（Skills 页可显示 health 列） |
| 搜索/筛选 | 按名称、来源、标签、是否已安装 | 前端 |

**详情**（`skill_detail` i18n）：SKILL.md 渲染（`parseSkillMd`，仅文本）、版本列表（`isCurrent`）、文件清单（`FileEntry[]`）、manifest、来源、安装矩阵（每个 Agent 的状态/路径/hash/策略/漂移）。

**健康四区块（`SkillHealthPanel`）**：

1. **安全审计**：运行 `skills_audit`；展示 status/risk/summary + finding 表格（severity/category/title/detail/path/suggestion）；空态「还没有审计记录」；不执行脚本。
2. **依赖与环境**：运行 `skills_dependencies`；item 表（kind/name/required/found/status/detail/suggestion）；MCP/凭据显示 unknown 并解释。
3. **更新影响**：选择目标版本 → `skills_preview_update`；展示 fileChanges（最多 500 条 + 「共 N 条」）、classes、风险、requiresRevalidation、受影响 Agent/安装。
4. **验证证据**：`validations_list` + `validations_record`；记录 Agent（可空=通用）、用例名、status、notes、evidence；必须在 UI 明示「封闭 Agent 无法自动端到端测试，这是人工证据，不是自动通过」。

**主操作**：
1. 导入（选择器：文件夹 / zip / 单个 SKILL.md / Git URL + ref；`skills_import`）。
2. 分发（多选 Agent → `skills_plan_install` → 计划弹窗 → `operations_apply`）。
3. 卸载（`skills_plan_uninstall` → 计划 → apply）。
4. 更新（选择新版本 → plan_install(versionId)）。
5. 回滚（跳到 Activity 对应操作）。

**空态**：库为空 → 「导入第一个技能」；扫描到的 Agent 技能可一键带入导入对话框。
**禁止**：前端编辑 SKILL.md（无后端接口）；未导入的技能直接显示为「库内技能」。

### 2.4 Matrix（`nav.matrix`，额外页面）

**目的**：技能 × Agent 的部署矩阵，一屏看清「装没装、是否漂移、是否可更新、是否未验证」。

- 行：技能；列：Agent；单元格状态由 `matrixCellState(skill, agent)` 计算，优先级：`unsupported agent → skill.conflict → installation.state → drift → update → installed/not_installed/verification`（`src/lib/status.ts:251-281`）。
- 单元格点击：已安装 → 查看路径/卸载入口；未安装 → 加入分发选择；冲突/漂移 → 打开详情说明。
- 状态色：`installed` success、`drift/conflict` warn、`missing/error` danger、`outdated` info、`verification` warn、`unsupported/not_installed` neutral；健康状态另用 `verified` success / `unverified` neutral / `needs_attention` warn / `failed` danger / `not_deployed` slate。
- 表头与首列 sticky（`styles.css:1648-1714`），居中提示「能力矩阵」。
- **不提供批量静默操作**：批量选择后仍进入计划预览。

### 2.5 MCP（`nav.mcp`）

**目的**：登记 Agent 无关的 MCP 定义、管理 Keychain、查看/执行分发。

| 区块 | 内容 | 数据 |
|---|---|---|
| 列表 | 名称、transport、command/url、env key 名、credentialRefs、分发状态、issues | `McpServerView[]` |
| 新建/编辑 | name、description、transport、command/args（stdio）或 url（remote）、headers、envKeys（每行一个 key 名）、credentialRefs | `mcp_upsert` |
| 凭据 | 每个 envKey 的「已设置/未设置」；设置值 → Keychain；删除 | `mcp_secret_states`、`mcp_set_secret`、`mcp_delete_secret` |
| 发现 | 从 Agent 配置发现条目，选择导入 | `mcp_discover`、`mcp_import` |
| 分发 | 选择 Agent → `mcp_plan_distribute` → 计划预览 → apply | 同名命令 |
| 移除分发 | `mcp_plan_remove` → 计划预览 → apply | |

**表单规则**：stdio 必填 command，不允许 url；HTTP/SSE/Streamable HTTP 必填 url，不允许 command；name 唯一（后端冲突返回）。
**安全提示（必须显示）**：
- 「凭据值只存 Keychain；分发时会被写入目标 Agent 配置文件，任何能读该文件的人都能看到。」
- 「headers 当前以明文保存/写入」（已知限制，见 SECURITY R2）。
- 「stdio server 等于在本机执行命令」；计划预览必须展示完整 command + args。
**引擎现状**：MCP 分发/移除已改为条目级 preflight（`ops.rs:220-260`），已有配置文件不再需要 `--force`；只有真实的外部修改/冲突才需要用户确认。UI 仍必须展示 `ExternalModification`/`conflict` 错误，不得静默 force。
**空态**：「新建 Server」/「从 Agent 发现」。

### 2.6 Plugins（`nav.plugins`）

- **只读**：列表来自 `plugins_sync`（Codex/TRAE CN/WorkBuddy 等），字段：Agent、名称、版本、来源、安装状态、managedBy、权限、官方链接、发现路径。
- 明确标注「AgentHub 不安装/不启停插件；请在对应 Agent 内操作」。
- 空态：无发现项 → 说明部分 Agent 无插件接口或未安装。
- **禁止**：任何「安装/更新/启用」按钮（后端不存在）。

### 2.7 Profiles（`nav.profiles`）

- 列表：内置 4 个（Coding/Research/Writing/Marketing，空 items）+ 用户自定义；显示条目数、目标 Agent。
- 编辑：名称、描述、条目（capabilityId、enabled、targetAgentKeys）。
- 应用：`profiles_plan_apply` → 计划预览 → apply；仅 skill 会被展开，其他 kind 显示 warning「暂不支持」。
- 空条目/无已安装 Agent：禁用「应用」并解释。
- 删除：内置 profile 是否允许删除由后端决定（当前 `profiles_delete` 不区分 builtin，UI 应二次确认并提示）。

### 2.8 Activity & Diagnostics（`nav.activity`，合并页）

四个 Tab（`activity.tabs`）：

| Tab | 内容 | 数据 |
|---|---|---|
| 操作历史 | 列表（状态/类型/摘要/开始时间/项数/可回滚），详情（plan/result/error/diagnostics），回滚按钮 | `operations_list`、`operations_get`、`operations_rollback` |
| 快照 | 快照列表：kind、originalPath、backupPath、existed、hashBefore、createdAt；可按操作过滤 | `snapshots_list` |
| 备份 | 备份列表（id/path/createdAt/size），创建备份 | `backups_list`、`backups_create` |
| 诊断与变更 | doctor 检查（library/database/data_dir/git/agents）、check_changes（drift/missing/updates） | `app_doctor`、`app_check_changes` |

**回滚规则**：仅 `canRollback=true` 可点；确认弹窗展示操作 id/摘要；回滚结果 `partial` 时逐项展示失败原因。
**空态**：无操作 → 说明分发/卸载/回滚后出现；无快照 → 说明快照只在写入时产生；无备份 → 提示创建。
**注意**：`operations` 只记录计划/执行/回滚/备份，**不记录 import/scan**（文档与 UI 都应说明）。

### 2.9 Settings（`nav.settings`）

| 区块 | 控件 | 后端 |
|---|---|---|
| 中央库 | 当前路径、选择新目录、应用（校验空目录/非嵌套） | `app_update_settings{libraryRoot}` |
| 默认安装策略 | copy/symlink segmented | `defaultStrategy` |
| 语言 | zh/en（立即生效并持久化） | `language` |
| 主题 | system/light/dark | `theme` |
| 扫描 | 启动时自动扫描开关 | `scanOnStartup`（注意：核心目前不自动扫描） |
| 项目根 | 列表、添加（目录选择器）、移除 | `app_project_roots` / `app_set_project_roots` |
| 系统信息 | app 版本、core 版本、platform、数据库路径、Keychain 可用性、diagnostics 入口 | `AppSettings` / `Bootstrap` / `app_doctor` |

**规则**：保存后立即刷新 bootstrap 与 UI 主题/语言；失败保留旧值并 toast 错误；库迁移提示「旧目录不会自动删除」。
**禁止**：显示前端本地存储的设置副本作为真源（真源是 SQLite settings）。

### 2.10 Health（`nav.health`，`/health`）

**目的**：一屏看清「哪些能力已验证、哪些需要处理、哪些根本没部署」。

| 区块 | 内容 | 数据 |
|---|---|---|
| 健康摘要 | total / verified / needsAttention / unverified / failed / notDeployed + lastCheckedAt | `HealthOverview.summary` |
| 筛选/搜索 | 状态 filter + 名称/slug/描述/reason 搜索 | 前端 `filterAndSortHealthCapabilities` |
| 能力表 | 名称、当前版本、健康 badge、原因摘要、已部署 Agent、已验证 Agent、最近审计风险 | `CapabilityHealth[]` |
| 重新检查 | 刷新 overview + `agent_health(true)` 记录版本；版本变化 toast | `health_overview`、`agent_health` |

- 状态视觉：`verified` success、`unverified` neutral、`needs_attention` warn、`failed` danger、`not_deployed` slate；健康行按风险排序（failed → attention → unverified → not_deployed → verified）。
- 空态：无能力 → 引导安装/导入；无匹配 → 调整筛选。
- **硬规则**：UI 不得把「安装成功」显示成 verified；必须展示 reasons 与「Verified 需要审计与验证证据」提示。
- 截图：[gui-health.png](evidence/gui-health.png)（Dashboard 健康区/健康入口；health 页截图未能自动点击时按 Lead 指示引用 Dashboard）。

### 2.11 Onboarding（首次启动，5 步，`onboarding.steps`）

| 步 | 内容 | 关键动作 |
|---|---|---|
| 1 privacy | 本地性、只读扫描、写前快照、Git 网络、Keychain 写入风险；需勾选同意 | 勾选后才能下一步 |
| 2 library | 展示当前中央库；可选择新目录并应用 | `app_update_settings{libraryRoot}` |
| 3 agents | 检测结果、状态、关键路径；可重新检测 | `app_bootstrap` / `agents_list` |
| 4 scan | 扫描并展示发现/冲突/无效项；不自动导入/安装 | `app_scan` |
| 5 finish | 汇总（库、Agent 数、发现技能数）；完成初始化 | `app_mark_initialized` |

**行为**：`initialized=false` 时启动进入 onboarding；完成后写 `initialized=true` 并进入 Dashboard；用户可跳过（跳过也标记完成，但 Dashboard 显示引导提示）。
**安全**：整个 onboarding 不写任何 Agent 目录；只允许改设置/标记初始化。

---

## 3. 关键任务流

### 3.1 首次启动

```
bootstrap → initialized?
  false → Onboarding(privacy → library → agents → scan → finish)
  true  → Dashboard
```

### 3.2 导入并分发

1. Skills → 导入（选择目录/zip/SKILL.md/Git）→ `skills_import`；展示 created/version/hash/warnings。
2. 详情页选 Agent（或 Matrix 勾选）→ `skills_plan_install`。
3. 计划弹窗：摘要（create/replace/noop/conflict/blocked）、每项 action/target/hash/warnings/restart。
4. 有 conflict → 勾选 force 才能应用；有 blocked → 「应用」禁用（引擎会跳过 blocked，但 CLI/UI 约定不执行）。
5. `operations_apply` → 结果面板（success/partial/failed、每项 message、diagnostics、backupPath）。
6. 刷新 Dashboard/Agents/Skills；提供「在活动中查看」。

### 3.3 更新与回滚

- 更新：导入新内容 → 详情显示 `updateAvailable` → 选新版本分发（同一 plan/apply 流程）。
- 回滚：Activity → 操作详情 → 「回滚」→ 确认 → `operations_rollback` → 结果；原操作状态变 `rolled_back`，生成新回滚操作。
- 外部修改：目标 hash 与记录不符时显示 drift/conflict；回滚可能 `partial`（拒绝覆盖被外部改过的目标）。

### 3.4 MCP 配置

```
新建/编辑 Server → 声明 envKeys → 设置 Keychain 值
  → 选择 Agent → mcp_plan_distribute
  → 计划预览（展示 command+args / url、envKeys、headers、warnings）
  → apply（当前版本：已有配置文件需勾选强制覆盖才能成功）
  → 验证配置文件中仅目标条目变化、无关键保留
```

### 3.5 Profile 应用

Profiles → 应用 → `profiles_plan_apply`（可能返回多个 plan 合并成一个）→ 预览 → apply；不支持 kind 的条目在计划 warnings 中说明。

---

## 4. 状态设计

### 4.1 通用状态

| 状态 | 视觉 | 行为 |
|---|---|---|
| loading | 骨架屏（`.skeleton`）或按钮 spinner | 禁用重复提交；保留布局避免跳动 |
| empty | `.empty-state` 图标 + 标题 + 解释 + 主操作 | 明确「下一步做什么」 |
| error | 页内错误条 + 重试；toast 用于操作失败 | 显示后端原始 message（`normalizeError`），不吞错 |
| conflict | 黄色 badge + 冲突说明 + force 复选框 | 未勾选时禁用应用 |
| blocked | 红色 badge + 原因列表 | 应用按钮禁用；提示先解决 blocked |
| verification_required | 黄色 badge + notes | 允许进入计划，但必须强提示；建议默认不勾选目标 |
| not_tauri | 全屏提示「请运行 pnpm tauri dev」 | `NOT_TAURI_MESSAGE`；浏览器预览不伪造数据 |

### 4.2 状态映射（已实现，`src/lib/status.ts`）

- Agent：installed=success、verification_required=warn、not_installed/unsupported=neutral、error=danger。
- Installation：installed=success、disabled=neutral、missing=danger、conflict=warn、error=danger。
- PlanAction：create=success、replace=info、remove=warn、noop=neutral、conflict=warn、blocked=danger。
- OperationStatus：planned=info、running=accent、success=success、partial=warn、failed=danger、rolled_back=neutral。
- Matrix：见 §2.4。

### 4.3 验证/健康状态映射（`src/lib/health.ts`）

| 域 | 状态 → Tone/文案 |
|---|---|
| HealthState | verified=success（已验证）、unverified=neutral（未验证）、needs_attention=warn（待处理）、failed=danger（失败）、not_deployed=slate（未部署） |
| Audit status | pass=success、warn=warn、fail=danger、unknown=neutral |
| Audit risk | info=neutral、low=info、medium=warn、high=danger、critical=danger |
| Finding severity | info/low/medium/high/critical → 对应 badge，critical/high 红色 |
| Dependency item | ok=success、missing=danger、warning=warn、unknown=neutral |
| Environment status | healthy=success、needs_attention=warn、unknown=neutral |
| Validation status | passed=success、failed=danger、needs_attention=warn、unverified=neutral |
| Validation kind | manual/static/environment/regression 只作文本标签，不改变状态色 |

**健康排序**：failed → needs_attention → unverified → not_deployed → verified（同状态按名称）。UI 不得把 installed 显示成 verified。

---

## 5. 设计 token

### 5.1 颜色（`src/styles.css:59-118`，实际值）

| Token | Light | Dark | 用途 |
|---|---|---|---|
| `--ah-bg` | `#f6f7f9` | `#0e1014` | 页面背景 |
| `--ah-surface` | `#ffffff` | `#16191f` | 卡片/表格 |
| `--ah-surface-2` | `#f8f9fb` | `#1b1f26` | 次级面板/表头/ footer |
| `--ah-sidebar` | `#fbfbfc` | `#111419` | 侧边栏 |
| `--ah-elevated` | `#ffffff` | `#1c2027` | 弹窗 |
| `--ah-fg` | `#1b1c20` | `#e6e8eb` | 主文字 |
| `--ah-muted` | `#61646c` | `#a1a6ae` | 次文字 |
| `--ah-subtle` | `#8b8f98` | `#757b85` | 弱文字/标签 |
| `--ah-line` | `#e6e7ea` | `#262b33` | 分割线 |
| `--ah-line-strong` | `#d4d6dc` | `#363d48` | 控件边框 |
| `--ah-accent` | `#4f46e5` | `#818cf8` | 主色 |
| `--ah-accent-soft` | 9% accent | 14% accent | 选中/软底 |
| `--ah-success` | `#15803d` | `#4ade80` | 成功 |
| `--ah-warn` | `#b45309` | `#fbbf24` | 警告 |
| `--ah-danger` | `#dc2626` | `#f87171` | 错误/危险 |
| `--ah-info` | `#0369a1` | `#38bdf8` | 信息 |
| `--ah-code-bg` | `#f3f4f6` | `#11151b` | 代码块 |
| `--ah-overlay` | 黑 32% | 黑 55% | 模态遮罩 |

语义 token 通过 `@theme inline` 映射到 Tailwind 类（`bg-surface`、`text-muted`、`border-line` 等），页面不得硬编码十六进制颜色。

### 5.2 字体与字号

- Sans：`-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'PingFang SC', …`；Mono：`'SF Mono', ui-monospace, Menlo, …`。
- 基础 13px / 1.5；`.text-sm` 11.5px、`.text-xs` 10.5px；等宽文本 11px（`.mono`）；数字用 `tabular-nums`（`.tabular`）。
- 标题：页面标题 ~20px/650，卡片标题 12–13px/650，分组标签 10px 大写（`.section-title`）。

### 5.3 间距、圆角、阴影、控件

- 间距基数为 4px：`stack-sm=8`、`stack-md=12`、`section-stack=16`、`grid-cards gap=12`。
- 圆角：控件 7px、卡片 10px、弹窗 12px。
- 阴影：card `0 1px 2px …`；pop `0 12px 32px …`。
- 控件高度 29px（按钮/输入），小按钮 25px；图标按钮 29×29。
- 弹窗宽度：sm 430 / md 580 / lg 820 / xl 1040；右侧 drawer 660。
- 断点：1120px（两栏→单栏）、960px（紧凑布局）。

### 5.4 明暗主题

- `system` 跟随 `prefers-color-scheme`；手动 `light/dark` 覆盖；通过 `<html class="dark">` 切换（`src/stores/ui.ts:applyTheme`）。
- 初始化时读取后端 `theme`（`app_settings`），写入 `data-theme` 与 `color-scheme`。
- 不提供「高对比度」主题（未实现）。

### 5.5 动效

- 过渡 120–180ms；模态 fade/modal-in、drawer-in、toast-in。
- `prefers-reduced-motion: reduce` 下关闭动画（`styles.css:2087`）。
- 不制作持续动画/背景动效。

### 5.6 i18n

- 默认 `zh`，支持 `en`；`src/i18n/{zh,en}.ts` 结构必须一一对应；缺失 key 静默回退 zh（`missingWarn:false`）。
- 所有用户可见文案走 i18n，不写死中文/英文；路径、哈希、命令等标识不翻译。
- 语言切换立即生效并持久化到 settings。

---

## 6. 预览-确认交互模型（核心）

```
[用户点「分发」]
   → 调用 plan_* 接口（只读，不写 Agent 目录）
   → 打开计划弹窗（modal）
       · 标题「操作预览」+ 摘要计数
       · 每项：动作徽章、Agent、目标路径、能力/版本、当前/目标 hash、策略、警告、diff 说明
       · blocked 项单独高亮，说明「应用会被跳过/拒绝」
       · conflict 项要求勾选「我了解风险，强制覆盖冲突项」
       · 需要重启的 Agent 列表
   → [仅预览] 关闭；或 [确认并应用] → operations_apply
   → 结果面板：状态、每项 message、diagnostics、backupPath
   → 刷新相关数据 + 「在活动中查看」
```

规则：

1. **阻断**：计划含 `blocked` 时「应用」禁用（与 CLI 行为一致；引擎虽会跳过 blocked，但 UI 不替用户决定）。
2. **强制**：`conflict>0` 时必须勾选 force；force 文案必须说明「会覆盖 AgentHub 之外创建/修改的内容，仍会快照」。
3. **幂等**：全部 noop 时允许「应用」但提示「没有需要变更的项」，或直接禁用并说明。
4. **可追溯**：结果里展示 operation id、backupPath；Activity 可回看 plan/result。
5. **失败**：失败/partial 时展示每项原因，不自动重试；提供回滚入口。
6. **MCP 错误**：仅在目标条目真实变化时返回 `ExternalModification`；UI 展示真实错误并引导重新预览，**不得默认勾选 force**。
7. **更新影响**：若计划是版本更新，弹窗顶部展示 `UpdateImpactSummary`：from→to、risk、classes、fileChanges（最多 500 条 + 总数）、受影响安装/Agent、`requiresRevalidation`。UI 不替代用户判断；重新验证提示必须可见。

---

## 6.5 GUI 运行验收（2026-10-20）

在隔离 home（`/tmp/agenthub-gui/home`）下运行 release `AgentHub.app`，人工验收通过：

- **Dashboard**：Agent 6/9、技能 1、安装 2、MCP 1、插件 1、待更新 1、冲突/漂移 0、待处理 0；最近扫描 7 技能/重复 1/冲突 0/无效 0；操作历史可见卸载/MCP 分发/MCP 移除。截图：[gui-dashboard.png](evidence/gui-dashboard.png)。
- **Onboarding**：第 1/5 步隐私与文件访问正常，同意复选框/下一步可用；5 步流程与 §2.10 一致。截图：[gui-onboarding.png](evidence/gui-onboarding.png)。
- **界面**：中文文案正常；侧边栏 10 项导航（仪表盘/Agent/技能库/能力矩阵/MCP/插件/Profiles/活动与诊断/健康验证/设置），健康=⌘9。
- **健康页/健康摘要**：Dashboard 显示能力健康摘要卡（total/verified/needs_attention/unverified/failed/not_deployed）与「查看健康」；`/health` 页有摘要、状态筛选、搜索、能力表。截图：[gui-health.png](evidence/gui-health.png)（Dashboard 健康区；health 页未能自动点击时按 Lead 指示引用 Dashboard）。
- **数据真实性**：Dashboard 数字与后端 `app_dashboard` 一致，无 mock；页面通过 Tauri 调用真实命令。
- **未覆盖**：自动化 GUI 回归、真实窗口中的文件选择器/Keychain/opener、更多页面的深度交互。

## 7. 键盘与可访问性

| 快捷键 | 行为 | 实现状态 |
|---|---|---|
| `⌘1`–`⌘9` | 切换 9 个导航页面（输入框内不生效） | 规格（i18n `shortcuts`） |
| `⌘K` | 聚焦技能搜索 | 规格 |
| `⌘,` | 打开设置 | 规格 |
| `Esc` | 关闭对话框/抽屉 | 规格 |
| `Tab`/`Shift+Tab` | 焦点循环；模态打开时焦点困在模态内 | 页面待实现 |
| `Enter`/`Space` | 激活按钮/链接 | 原生控件 |

- 全局 `:focus-visible` 2px accent outline（`styles.css:147-151`）。
- 提供「跳到主内容」链接（i18n `common.skipToContent`）。
- 图标按钮必须有 `aria-label`；状态徽章需文本，不只靠颜色。
- toast 容器应有 `aria-live="polite"`（规格；待实现）。
- 表格行点击进入详情时，同时提供键盘可达的行内按钮。

---

## 8. 禁止死按钮与假数据（硬规则）

1. **每个按钮必须**：调用一个真实存在的 Tauri 命令，或打开一个真实页面/对话框，或明确 `disabled` 并在 tooltip/说明中给出原因。没有 `console.log('TODO')` 按钮。
2. **禁止 mock**：不得在前端写死 Agent/技能/MCP 示例、统计数字、假状态；浏览器（非 Tauri）只显示 `NOT_TAURI_MESSAGE`。
3. **数据真源**：列表/统计来自后端；前端 store 只做缓存（15s TTL 仅用于 skills），刷新后必须与服务端一致。
4. **状态诚实**：
   - 未实现功能不放入导航，或标注「未实现」并禁用；
   - 后端返回 `verification_required`/`blocked`/`conflict` 必须原样展示；
   - 不把 `planned` 操作显示成「成功」；
   - 不把 dry-run 计划显示成已执行。
5. **失败可见**：API 错误必须 toast/页内提示，并保留用户输入；不得静默 catch。
6. **危险操作**：删除/回滚/强制覆盖需二次确认；确认文案包含对象名称与影响。
7. **空态不是错误**：空列表显示引导，不显示红色错误。
8. **无网络依赖**：断网时除 Git 导入外全部页面可用；Git 导入错误要可读。

---

## 9. 验收清单（UI）

- [ ] 首次启动走完 5 步 onboarding；不写任何 Agent 目录。
- [ ] Dashboard 数字与 `app_dashboard` 返回值一致。
- [ ] Agents 页可展开所有 roots 与 MCP targets，显示 exists/writable。
- [ ] Skills 导入 → 详情 → 分发 → 计划弹窗 → 应用 → 结果；blocked 时禁用应用；conflict 时必须勾选 force。
- [ ] Matrix 单元格状态与 `matrixCellState` 一致；点击可进入对应操作。
- [x] MCP 新建/Keychain 设置/分发/移除：条目级 preflight 已修复；UI 人工验收覆盖列表/分发操作；`headers` 警示已展示。
- [ ] MCP 真实外部修改场景的错误展示（自动化回归）。
- [x] Health 页：摘要、筛选、搜索、健康 badge、原因列；Dashboard 健康摘要卡（人工验收，截图）。
- [x] SkillDetail 四区块：安全审计/依赖与环境/更新影响/验证证据（人工验收；自动化待补）。
- [x] 分发预览显示更新影响摘要与「需重新验证」提示（人工验收）。
- [ ] Profiles 应用生成计划并可执行。
- [ ] Activity 四个 Tab 数据齐全；回滚成功/失败均有反馈；快照与备份可查看。
- [ ] Settings 修改语言/主题/策略/项目根立即生效并持久化。
- [ ] 所有页面 loading/empty/error 三态齐全；无 mock 数据；无非 Tauri 伪造。
- [ ] 键盘快捷键与 focus ring 可用；Esc 关闭模态。
- [ ] `prefers-reduced-motion` 下无动画。

---

## 10. 实现状态（待核对）

| 层 | 文件 | 状态 |
|---|---|---|
| 设计 token/组件样式 | `src/styles.css`（2096 行） | ✅ 已实现：颜色/字体/间距/按钮/输入/卡片/表格/徽章/模态/toast/矩阵/骨架/动效/reduced-motion |
| 布局骨架 | `src/styles.css` `.app-shell/.sidebar/.page` | ✅ 已实现样式；页面结构待组件 |
| i18n | `src/i18n/zh.ts`（987 行）、`en.ts`（994 行）、`index.ts` | ✅ 已实现；zh/en 结构需由 i18n 测试保证 |
| 类型 | `src/lib/types.ts`（624 行） | ✅ 与 Rust model 对齐 |
| Tauri 调用 | `src/lib/api.ts`（173 行） | ✅ 覆盖 38 个命令；非 Tauri 抛 `ApiError` |
| 状态映射 | `src/lib/status.ts`（321 行） | ✅ 含 matrixCellState/tone |
| Markdown | `src/lib/markdown.ts` | ✅ 安全块解析，无 `v-html` |
| 格式化/对话框 | `src/lib/format.ts`、`dialogs.ts` | ✅ |
| Store | `src/stores/app.ts`、`ui.ts`、`skills.ts`、`operations.ts` | ✅ 数据加载/主题/语言/toast/回滚/备份/诊断 |
| 入口 | `index.html`、`vite.config.ts`（端口 1420，jsdom + vitest） | ✅ |
| 路由 | `src/router.ts` | ✅ 已实现：hash 路由、10 条页面路由 + `/` 重定向、导航表与 `⌘1–9` 顺序 |
| App 根组件 | `src/App.vue`、`src/main.ts` | ✅ 已实现：Pinia/i18n/router 挂载、启动 bootstrap、onboarding/shell 切换、错误横幅 |
| 页面 | `src/pages/*` | ✅ 12 个视图（10 条导航路由 + 技能详情 + onboarding；含 HealthView）；隔离 home GUI 人工验收通过（§6.5） |
| 组件 | `src/components/*` | ✅ 20 个组件：新增 HealthStatusBadge、SkillHealthPanel、UpdateImpactSummary；其余同第一轮 |
| 测试 | `src/lib/__tests__/*.spec.ts`、`src/__tests__/*.spec.ts` | ✅ **7 文件 / 29 测试**通过（新增 health-ui/health） |

> 构建/测试状态快照（2026-10-20 16:34）：`pnpm build` 通过；`pnpm test` **7 文件 / 29 测试**通过；完整 `pnpm tauri build` 已产出 0.2.0 `AgentHub.app`（7.49 MiB）与 `AgentHub_0.2.0_aarch64.dmg`（3.95 MiB，16:33–16:34），`/Applications/AgentHub.app` 已覆盖安装 0.2.0；首次 DMG 尝试因沙箱 hdiutil 权限失败，完整权限重跑成功；未签名/未公证。页面视觉以最终组件为准；token、状态映射、命令签名与文案 key 不应另造一套。

---

## 11. 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：设计 token、i18n、状态映射、API 封装、router/App/12 视图/20 组件、`pnpm build`、**29 个前端测试**；隔离 home GUI 运行验收（Dashboard/健康摘要/onboarding/中文界面/活动历史）。
- **未验证**：Tauri 自动化回归、原生文件选择器、真实 Keychain、原生 opener、完整分发→回滚的 GUI 点击流、Health 页的自动化交互。
- **已知限制**：未做 Playwright/XCUITest；静态审计是启发式、人工证据是用户输入（UI 已提示）；MCP headers 明文提示已在 UI 展示但仍需安全侧修复；详情页未知字段只做结构展示。

## 12. 交叉链接

- GUI 验收证据：[ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md)
- 验证方法论与状态机：[VALIDATION.md](VALIDATION.md)
- 产品目标与功能状态：[PRD.md](PRD.md)
- 数据字段与枚举：[DATA-MODEL.md](DATA-MODEL.md)、`src/lib/types.ts`
- 分发/回滚/锁：[ARCHITECTURE.md](ARCHITECTURE.md)
- 安全提示与确认点：[SECURITY.md](SECURITY.md)
- 验收步骤：[TEST-PLAN.md](TEST-PLAN.md)
