# Security & Privacy Review — My AI Stack / Cost Optimizer（M0 冻结草案）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 基线：docs/SECURITY.md（立场 4 条、T1-T13 威胁模型、§3.6-3.8 的 0.5.x 修复记录）。本文只评估**新增面**，结论均为设计期评审，标注「实现时复核」的项在对应阶段执行。

## 1. 数据最小化审查

新表列逐一审查（对照 DATA_MODEL_PROPOSAL §2）：

| 风险类别 | 结论 |
|---|---|
| 支付凭证 / 卡号 / 账户密码 / API Key / 账单访问凭证 | **表结构无此列；PRD 明确禁止保存**。表单不提供输入位，后端 upsert 校验白名单字段（多余字段拒绝，防未来 UI 误加） |
| 用户 Prompt / 会话内容 | 不接触（模块无任何读取 agent 会话/日志的代码路径） |
| MCP Keychain 值 | 不读取（stack_detections.evidence_json 只存路径字符串证据，与 mcp_secret_states 无交集；路径本身按"准身份数据"处理，见"检测路径数据"行） |
| 检测路径数据 | 本机文件路径属**准身份数据**（可推断用户软件使用习惯）——仅存于本地 SQLite（evidence_json），不写日志、不进诊断导出（S8）、不进审计 payload；与现有 list_agents 同级暴露面，无新增网络/导出通道 |
| 金额/订阅信息 | 属用户个人财务偏好数据：本地存储、进入 backups_create 的全库备份（本地文件 0700/0600，restrict_owner_permissions library.rs:152——可接受，登记于下表） |

**实现时复核 R-P1**：subscription upsert 的入参 struct 必须是封闭 serde struct（deny_unknown_fields 或显式字段校验），防止意外透传任意 JSON。

## 2. 权限与网络面

- 新增 Tauri 命令属 `core:default` 权限范畴，**capabilities/default.json 不改**（无 shell/fs/http 需求）。
- 无新增 Rust 依赖（不引任何网络 crate）；optimizer 与 money 计算纯 std。
- 无新增遥测/埋点；audit_events 只在本地 DB。
- **实现时复核 R-P2**：CI/验收中加断言性检查——M1/M2 阶段 `grep -r "reqwest\|ureq\|http client" crates/agenthub-core/src/{money,optimizer}.rs` 应为空；v31_acceptance 扩展时保持 /tmp 隔离。

## 3. 威胁模型增量（对照 SECURITY.md T1-T13 风格）

| # | 威胁 | 场景 | 缓解 | 残余风险 |
|---|---|---|---|---|
| S1 | 迁移破坏既有库 | v3→v4 迁移中途崩溃/磁盘满 | 事务内 DDL + 迁移前 VACUUM INTO 备份 + integrity 校验失败即 `Error::Verification`（db.rs:665-671）+ 失败不提交 | 备份占用磁盘（单文件 ~MB 级，可接受） |
| S2 | 检测误删用户财务数据 | sync_agents/refresh 逻辑 bug | 检测路径只写 stack_detections；代码评审门禁：stack_items/subscription_records 无任何被检测流程 DELETE 的语句；FK RESTRICT | 依赖评审+测试断言 |
| S3 | 多币种误加导致错误财务结论 | 汇总实现 bug | 按币种分组聚合；核心单测断言「CNY 不进 USD 合计」；i64 整数算术（checked，M6） | — |
| S4 | 财务数据被误展示为"已节省" | UI 语义 bug | Potential/Confirmed/Realized 三态分离（COST_OPTIMIZER_PRD §3）；文案进测试断言 | — |
| S5 | 并发写丢更新 | GUI+CLI 同时开、双击提交 | write_lock 进程内串行 + runtime.lock 跨进程 flock + busy 显式报错；重复提交幂等（upsert 以 id 为键，重复 create 生成新 id 而非覆盖） | — |
| S6 | 合并操作产生环 | A merge B 且 B merge A；或 A(已持有子项) 再作为子项被并入 C（形成 A→B、C→A 深度 2 链） | 服务层预检 merged_into 链深度（≤1 层）：**①子条目不可再作为合并目标；②已持有 merged 子项的条目不可再作为合并子项**（M4 补充）→ Error::invalid | — |
| S7 | 删除级联意外扩散 | 删 stack_item 带走订阅 | 删除前服务层校验无订阅 → Conflict→invalid；FK RESTRICT 物理兜底 | — |
| S8 | 诊断导出泄露新增数据 | export_diagnostics 包含订阅信息 | 评估：订阅金额/名称非凭据级敏感，redact.rs 现有规则（URL/args/secret 标志）不适用；**决策：诊断导出默认不包含 stack/subscription 表**（export_diagnostics 现按模块组织，新增模块不自动加入导出；如未来加入需先过本文件评审） | 未来扩展时复核 |
| S9 | 备份恢复把旧订阅数据当权威 | 用户回滚库后重复登记 | 登记为产品文案注意项（Spending 页显示数据时间）；无技术缓解需求 | — |
| S10 | UI 注入（订阅名/备注含 HTML/JS） | 用户输入恶意串 | 沿用现有规范：Vue 插值、禁 v-html（markdown.ts 同一原则）；不新增 innerHTML 路径 | — |
| S11 | 场景模拟被误解为真实操作 | 用户以为 R4 真的取消了订阅 | 纯只读算术 + UI 固定声明条 | — |
| S12 | 真实 Agent 目录被本模块意外写入 | 误用 Executor/直接写 home | 本模块零 fs 写入路径（除 db 迁移备份，其目标在 data_dir/backups）；验收测试断言 home 目录哈希不变 | — |

## 4. 与既有安全修复的关系

- v0.5.1 修复内容（runtime.lock unlink、互斥覆盖补全、诊断脱敏、备份权限、迁移回滚）已在 master 生效（M0_BASELINE_AUDIT §1.1）；新模块**直接受益**且不触碰这些路径。
- 版本号错位（CHANGELOG 0.5.1 vs tag v0.5.0）属发布管理问题，移交负责人（不影响本模块安全 posture）。

## 5. 隐私声明素材（供未来产品页/README 使用，非本次实施内容）

"My AI Stack 与 Cost Optimizer 的全部数据保存在本机 SQLite 数据库中；应用不联网、不上传、不读取你的 API 密钥或对话内容；订阅信息仅在你手动登记后存在，可随时完整删除。"

## 6. 需要在实现阶段执行的检查清单

- [ ] R-P1 封闭入参 struct（M1-A）
- [ ] R-P2 无网络依赖断言（M1-C/M2-C）
- [ ] S2 门禁测试：检测刷新不修改 stack_items 用户字段、不动 subscriptions（M1-A）
- [ ] S8 检查 export_diagnostics 输出不含新表内容（M1-C）
- [ ] S12 home 目录哈希不变断言（M1-C，复用 real-acceptance 的只读扫描思路但不写真实目录——用 AGENTHUB_HOME 隔离）
- [ ] 迁移测试：含 V1-V3 数据 fixture 升 v4，行数+完整性断言（M1-A）
