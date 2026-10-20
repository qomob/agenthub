# AgentHub 路线图

> 状态基准：2026-10-20。阶段以「可验收的能力」划分，不以日期承诺；未验证的项不写成已完成。
> 图例：**✅ 已完成** / **🟡 进行中 / 部分** / **⛔ 未开始** / **⚠️ 阻塞项**

---

## 1. 阶段总览

| 阶段 | 目标 | 状态 | 验收标准 | 证据 |
|---|---|---|---|---|
| **Phase 0** 调研与边界 | 确认各 Agent 的真实路径/格式/能力，明确 MVP 不做什么 | ✅ 已完成 | 每个目标 Agent 有证据等级；不确定项标 VR/UNKNOWN | `docs/REFERENCE-STUDY.md`、`docs/AGENT-COMPATIBILITY.md`、`docs/compat-evidence/` |
| **Phase 1** 本机能力管理 MVP | 检测 → 扫描 → 导入 → 计划 → 事务分发 → 快照/回滚 | ✅ 已验收（真实 Mac + GUI） | 隔离 home 下可完成「扫描 ZCode/TRAE/DSH → 导入 → 分发两个 Agent → 更新 → 回滚」；真实 Mac 同流程通过 | core 21/CLI 9/前端 21 全绿；[ACCEPTANCE-REPORT.md](ACCEPTANCE-REPORT.md) |
| **Phase 2** MCP 管理 | 定义/发现/Keychain/多格式 codec/分发/移除 | ✅ 核心完成（headers 待加固） | JSON+TOML 分发/移除保真；缺凭据 blocked；env 不进 plan | MCP 3 个集成测试通过；plan payload 脱敏；harness 验证 TOML/JSON/回滚 |
| **Phase 3** 工作流与可视化 | 能力矩阵、Profiles、活动/诊断、插件只读、UI | ✅ 人工验收 | 9 路由 + Onboarding；Profile 应用；诊断可排错；GUI 运行 | 隔离 home GUI 截图；`pnpm build` + 21 前端测试 |
| **Phase 4** 生态与规模化 | 云同步、市场、第三方适配器、调用统计 | ⛔ 未开始 | 有明确安全模型与用户需求后再立项 | 本文 §4 |

---

## 2. Phase 0 — 调研与边界（✅）

**已做**

- 9 个 Agent 的本机路径/检测信号/MCP 格式/插件/热加载/symlink 证据矩阵。
- Agent Skills 规范、MCP 协议版本（2025-06-18 / 2025-11-25 / 2026-07-28）、两个参考项目设计研究。
- 明确 ChatGPT 只读、DSH MCP 不写、Trae/ZCode/WorkBuddy 的证据等级。
- 脱敏证据目录与 symlink 验证方案。

**验收**：每个 adapter 的路径都能在证据文件中定位；「未验证」项有明确标记与验证方案。

**遗留**：Codex 项目级根、Trae CN 项目根、ZCode 远程传输、WorkBuddy `staticHeaders`、非 DSH symlink 行为仍需实测（见 [ADAPTER-SPEC §7](ADAPTER-SPEC.md)）。

---

## 3. Phase 1 — 本机能力管理 MVP（✅ 已验收）

**已做**

| 能力 | 状态 | 说明 |
|---|---|---|
| AgentAdapter 框架 + 9 个适配器 | ✅ | `adapters/mod.rs`、`builtin.rs` |
| 只读扫描 + 缓存 | ✅ | `scan.rs`；duplicates/conflicts 语义已修正 |
| 导入（folder/zip/SKILL.md/Git/AgentDir） | ✅ | 解压安全限制、Git 浅克隆 |
| 中央库版本化 | ✅ | `library/skills/<slug>/<hash12>` |
| 计划 → 预检 → 快照 → 写入 → 校验 | ✅ | copy/symlink；失败补偿 |
| 回滚（操作级） | ✅ | 只允许一次；拒绝覆盖外部修改 |
| CLI（doctor/agents/skills/operations/profiles/backup） | ✅ | 计划门禁、`--json`、退出码 |
| 测试（core 21 + CLI 9 + 前端 21） | ✅ | 全部通过；前端构建通过 |
| GUI（9 路由 + Onboarding） | ✅ | 页面/路由/组件已落地；隔离 home 人工运行验收通过（截图） |
| 真实 Mac 验收 | ✅ | 真实库导入/分发/扫描/更新/回滚/卸载/备份/doctor；见 ACCEPTANCE-REPORT |
| 文档（PRD/架构/数据/适配器/安全/UI/测试/路线图/变更） | ✅ | 本次交付 |

**阶段验收清单**

- [x] 隔离 home 下可扫描、导入、双 Agent 分发、更新检测、回滚（CLI/harness 实测）
- [x] 写入前快照、失败自动恢复、外部修改检测
- [x] 非托管目标冲突拒绝（非 force）
- [x] 自动化测试全绿（core 21/21、CLI 9/9）
- [x] 前端 `pnpm build` 通过
- [x] GUI 运行验收（隔离 home Dashboard/onboarding；真实分发→回滚点击流仍建议补）
- [x] 真实 Mac 写入验收（唯一 slug，部署最终删除、库保留 2 版本）
- [x] `pnpm tauri build --bundles app` 通过并安装 `/Applications/AgentHub.app`
- [ ] `pnpm tauri build` 的 DMG / 签名 / 公证（沙箱 hdiutil 受限）
- [ ] `./scripts/cargo.sh test --workspace` 全绿（当前只跑 core/CLI）

**收尾优先级**

1. DMG/签名/公证（有权限桌面会话）。
2. GUI 自动化回归（Dashboard/分发/回滚点击流）与核心纯函数单元测试。
3. MCP headers 凭据化；备份恢复；`target_within_roots` 接入。
4. 真实 profile 既有 conflicts/duplicates 的提示与清理策略。

---

## 4. Phase 2 — MCP 管理（✅）

**已做**

- `mcp_definitions` 模型 + CRUD + 唯一 name。
- 从 Agent 配置发现（JSON `mcpServers` / `mcp.servers` / TOML `mcp_servers`）。
- Keychain 存 env 值（`com.agenthub.mcp`）；缺值 → blocked。
- 计划（Create/Replace/Noop/Conflict/Blocked）+ 写入（原子写、回读校验）+ 移除。
- 回滚（快照恢复）。
- 多格式 codec：JSON 任意点路径、TOML `mcp_servers`；保留未知键/TOML 注释。

**阶段验收清单**

- [x] JSON/TOML 分发写入（非 force，集成测试）
- [x] 移除目标条目（非 force，集成测试）
- [x] MCP env 值不进入 `operations.plan`（plan payload 剥离，执行时 Keychain 重建）
- [x] 缺 Keychain 值不写配置
- [x] 格式损坏拒绝覆盖（代码具备）
- [x] 非 force 的 create/replace/remove 通过（MCP 3 个测试通过）
- [x] 同文件多 server 不互相覆盖（MCP 集成测试覆盖）
- [ ] 不支持传输/无目标时 blocked 的专门测试
- [ ] headers 凭据化（Keychain 引用）；**当前仍明文**
- [ ] DSH Cordis YAML、Trae CN 全局 MCP、ZCode 原生项目配置的 codec

**Phase 2 完成定义**：MCP 分发/移除在无 force 的正常路径可用且测试全绿（已达成）；env 不进入计划 JSON（已达成）；headers 凭据化（未达成）；三种以上 codec 有保真测试（部分）。

---

## 5. Phase 3 — 工作流与可视化（✅ 人工验收）

| 能力 | 状态 | 说明 |
|---|---|---|
| 能力矩阵（skill × agent） | ✅ | `matrixCellState` + Matrix 页面已实现 |
| Profiles（内置 4 个 + 自定义 + 应用） | ✅ | 后端 + 页面已实现；运行时待验收 |
| 活动与诊断（4 Tab） | ✅ | store/i18n/页面已实现 |
| 插件只读清单 | ✅ | 后端 + 页面已实现 |
| Onboarding 5 步 | ✅ | 页面已实现，由 App.vue 按 initialized 切换 |
| 启动自动扫描 | 🟡 | `App.vue` 按设置触发扫描；核心 `open()` 本身不扫描 |
| 结构化日志 | ✅ | `logs/operations.log` JSONL：import/start/finish/failed |
| 备份恢复 | ⛔ | 只有创建/列表 |

**阶段验收**：GUI 隔离 home 运行验收通过（Dashboard/onboarding/中文/活动）；Profile/Activity/诊断页面已实现。剩余：GUI 自动化回归、更多页面深度操作、启动扫描行为与设置一致性的自动断言。

---

## 6. Phase 4 — 未来候选（⛔ 明确暂不做）

| 候选 | 价值 | 为什么现在不做 | 重新评估条件 |
|---|---|---|---|
| **云同步 / 多设备** | 跨机器共享技能与配置 | 需要账号、加密、冲突合并、隐私模型；与 Local First 冲突 | 有明确多设备用户需求，且完成端到端加密设计 |
| **技能市场** | 发现/一键安装 | 需要服务端、审核、签名、供应链安全；当前导入已覆盖手动机场 | 有可信发布者与签名机制 |
| **第三方适配器市场** | 社区扩展 Agent 支持 | 加载不受信代码违反安全模型；当前 adapter 编译进二进制 | 有签名 + 沙箱 + 权限模型 |
| **调用统计 / 使用分析** | 知道哪些技能常用 | 需要运行时代理/日志采集，涉及隐私；AgentHub 不介入 Agent 运行 | 有本地匿名聚合方案且用户明确同意 |
| **MCP 试连 / 工具面板** | 验证配置是否可用 | 需要 MCP client、协议版本分流（2025-xx vs 2026-07-28）、进程生命周期 | Phase 2 稳定后单独立项 |
| **插件安装/启停** | 统一插件管理 | 账号/市场资产，写文件不等于生效；证据不足 | 某 Agent 提供官方可编程接口并被验证 |
| **Windows / Linux** | 扩大用户 | symlink/Keychain/路径均未验证；工作量大 | 有平台需求与测试机器 |

---

## 7. 风险登记

| # | 风险 | 概率/影响 | 现状 | 缓解 |
|---|---|---|---|---|
| R1 | ~~MCP preflight hash 语义错误~~ **已修复** | — | 15:24–15:26 修复；MCP 测试通过 | 回归保持：preflight 比条目 hash，移除计划记录当前条目 |
| R2 | **私有格式漂移**（Codex/Trae/ZCode/WorkBuddy） | 中 / 高 | 无版本承诺；codec 依赖证据 | 未知键保留；写入前快照；证据日期；错误可回滚 |
| R3 | **MCP 协议换代**（2026-07-28 无状态化） | 中 / 中 | AgentHub 只写客户端配置，不实现协议 | 不把协议版本写进客户端配置；未来 Probe 双路径 |
| R4 | **Keychain 授权/可用性** | 中 / 中 | 依赖 `/usr/bin/security`；可能弹授权或失败 | 诊断显示 `keychainAvailable`；失败不写配置 |
| R5 | **symlink 兼容性** | 中 / 中 | 仅 DSH `Supported`；其余 Unverified | 默认 copy；未验证时警告；不承诺热更新 |
| R6 | **敏感值落库/落盘** | 中 / 高 | env 已从 plan 剥离；**headers 仍明文**；快照/备份含配置 | headers Keychain 化；文档/UI 警告；备份加密或排除（SECURITY R2-R4） |
| R7 | **测试与代码并行导致回归** | 中 / 中 | 代码在文档写作期间多次变化；本轮 15:56 全绿 + 真实验收 | 每轮全量 test；文档标注快照时间 |
| R8 | **GUI 自动化缺失** | 中 / 中 | 只有人工截图验收 | 补 Playwright/XCUITest；保留 UI-SPEC §9 清单 |
| R9 | **误写真实 Agent 目录** | 低 / 高 | 隔离测试 + 一次授权真实验收；目标最终删除 | 预览确认、默认 copy、快照、唯一 slug、失败清理 |
| R10 | **无人值守的格式保真承诺** | 中 / 中 | JSON 会重排缩进；TOML 保注释 | 文档明确「结构保真、格式不保真」；UI diff 提示 |
| R11 | **DMG/签名/公证** | 高 / 中 | 沙箱 hdiutil 失败；未签名 | 有权限桌面会话重跑 `pnpm tauri build`；Developer ID + notarization |

---

## 8. 里程碑判定（不绑定日期）

| 里程碑 | 判定条件 |
|---|---|
| M1「核心可用」 | core+CLI 测试全绿；真实 Mac 五步验收通过（已达成） |
| M2「GUI 可用」 | 9 路由 + Onboarding；隔离 home 运行验收通过；无 mock/死按钮（已达成；自动化待补） |
| M3「MCP 可用」 | 无 force 的分发/移除/回滚全绿；env 不落计划（已达成）；headers 待加固 |
| M4「可发布 0.1.0」 | M1+M2+M3；DMG/签名/公证；已知限制清单公开（当前差 DMG/签名） |

---

## 9. 状态标记（已验证 / 未验证 / 已知限制）

- **已验证**：Phase 0 调研；Phase 1 真实 Mac + GUI 验收；Phase 2 MCP 正常路径与 env 脱敏；Phase 3 页面/构建/GUI 运行；core 21、CLI 9、前端 21 测试通过。
- **未验证**：DMG、签名/公证、GUI 自动化、真实 Keychain 正向、真实 MCP 配置写入、并发/中断、非 macOS；Phase 4 全部未开始。
- **已知限制**：headers 明文、备份恢复未实现、`target_within_roots`、核心单元测试缺失、DMG/签名；见 §7 风险登记。

## 10. 交叉链接

- 当前功能状态：[README.md](../README.md)、[PRD.md](PRD.md)
- 适配器证据与未验证项：[ADAPTER-SPEC.md](ADAPTER-SPEC.md)、[AGENT-COMPATIBILITY.md](AGENT-COMPATIBILITY.md)
- 测试基线：[TEST-PLAN.md](TEST-PLAN.md)
- 安全剩余风险：[SECURITY.md](SECURITY.md)
