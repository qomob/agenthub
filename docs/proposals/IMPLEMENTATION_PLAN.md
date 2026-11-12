# Implementation Plan — M0 → M3 分阶段实施（M0 冻结草案）

> 设计基准 commit：master@b5ef694（tag v0.5.0）。文中行号引用均以此 commit 为准，代码演进后行号可能失准。

> 每个阶段含：目标 / 修改文件范围 / 依赖 / 风险 / 测试 / 验收标准 / 回滚方式 / 是否需负责人批准。
> 全局前提：所有测试用隔离 `AGENTHUB_HOME`；Rust 构建走 `./scripts/cargo.sh`；不碰真实 `~/.codex` / `~/.agenthub`；M0 之外每阶段开始前需负责人批准该阶段计划。

## M0 — 只读审计与设计冻结（本文档组）

- 目标：事实基线 + 8 份设计文档 + 对抗检查（已完成）。
- 范围：仅新增 `docs/proposals/*.md`（8 份，经负责人批准写入）。
- 验收：负责人审阅并给出 GO/修改意见；三个决策点已裁定（M0-R 冻结）：
  1. **D-1 CLI 写命令形态**（有条件批准）：本地 CRUD 直接执行，不走 approval token 流；删除/合并等高影响操作需显式确认，CLI 非交互高影响命令必须 `--yes`；**变更记录只写 `audit_events`，不写 operations、不扩展 OperationKind**（契约核验：前端 types.ts:29-41 严格联合类型 + parse 回退误标 + plan/approval 语义绑定，ARCHITECTURE §5）。未来扩展 OperationKind 需先核验消费者/状态机/恢复逻辑兼容。
  2. **D-2 冲突错误码**（原方案不批准，修订冻结）：新增 `Error::InvalidInput` 与 `Error::ResourceConflict` 变体 + `ErrorCode::ResourceConflict`（exit 4），三态区分（参数非法/不存在/存在但不能操作）；既有 From 映射与 PLAN_STALE 零改动（ARCHITECTURE §7）。
  3. **D-3 版本序列**（批准方案 A）：安全修复已随 v0.5.0 tag 发布，docs/CHANGELOG.md 0.5.1 节已归档至 0.5.0（未单独发版、不移动 tag）；My AI Stack 后续版本按 **0.6.0** 规划，发布需单独授权。
- 风险：无（只读+文档修订）。
- 回滚：git revert docs/ 与 docs/CHANGELOG.md 修订。

## M1-A — 数据模型与核心服务

- 目标：SCHEMA_V4 迁移 + db.rs 行结构/访问器 + service.rs 新区段（stack/subscriptions/spending 读写）+ money 计算模块 + 单元/集成测试。
- 文件范围：
  - `crates/agenthub-core/src/db.rs`：`SCHEMA_V4` 常量、`migrate()` v4 分支、`backup_before_migration` 参数化（P-1）、`StackItemRow/StackDetectionRow/SubscriptionRow` + list/upsert/delete/replace_detections 访问器
  - `crates/agenthub-core/src/model.rs`：`StackItemView/StackItemDetail/SubscriptionView/SpendingOverview/CurrencyBreakdown/StackItemUpsert/SubscriptionUpsert/StackRefreshReport` 等 serde struct
  - `crates/agenthub-core/src/money.rs`（新，~120 行）：Money/折算/聚合纯函数（i64 + checked 算术，M6）
  - `crates/agenthub-core/src/service.rs`：`// -- stacks / subscriptions --` 区段（读 6 + 写 6 方法，§3 接口草案）
  - `crates/agenthub-core/src/error.rs`：**新增两个变体**（D-2 冻结）：`InvalidInput(String)` + `invalid_input()` 构造 → 既有 `ErrorCode::InvalidInput`；`ResourceConflict(String)` + `resource_conflict()` 构造。既有变体/构造零改动；`Error::invalid()`（实际映射 InvalidSkill 的语义陷阱）本模块禁用。
  - `crates/agenthub-core/src/contract.rs`：ErrorCode **追加** `ResourceConflict` + `exit_code()` 追加分支（=> 4，与 PlanStale 同族）；既有 18 个代码与其映射零改动。
  - 测试：`crates/agenthub-core/tests/stack_subscriptions.rs`（新）+ `tests/stack_migration.rs`（新）+ error/contract 加法映射测试
- 依赖：M0-R 批准（D-1/D-2/D-3 已冻结）。
- 风险：迁移参数化改动 `backup_before_migration` 签名（低，内部函数，有 3 个既有调用点需同步）；错误契约加法扩展对现有 match 穷尽性影响（低——`From<&Error>` match 需追加新变体臂，属编译器强制检查的加法改动）。
- 测试：迁移 fixture（V1-V3 数据→v4，表数量/integrity/备份/user_version 断言）+ **P0-5 恢复路径**（V4 库 restore 回 V3 备份→user_version=3→可再迁移；旧版本读到 V4 库不降级不破坏）；CRUD 全生命周期；金额折算表驱动测试（含除不尽取整 + i64 边界 + **JSON 2^53-1 校验** + 聚合取整顺序 3.11 用例 + JPY exponent）；聚合规则 9 条逐一断言（含 stale_renewals、status=unknown、period unknown 不折算）；merge/unmerge = 不迁移数据的组聚合断言（合并前后成本一致 + unmerge 幂等恢复）+ 环/多级/merged 子条目 upsert 订阅的 ResourceConflict；**合并 → 刷新检测 → 仍 1 条目**（3.8）+ **连续两次 refresh 幂等**（3.8b，adapter_key UPSERT 稳定身份）；检测刷新挂靠规则与不碰用户字段（S2 门禁）；删除预检（任何状态订阅 / 有 merged 子项 → ResourceConflict）；并发双写（两线程 + runtime.lock 外部占用场景）。
- 验收标准：core 测试 ≥ 新增 30 个全绿；既有 109 个 core 测试零回归；`./scripts/cargo.sh test -p agenthub-core` 通过（Keychain 触点测试用 `--test` 分片规避）。
- 回滚：revert 提交即可；迁移产生的备份保留；user_version 不前进（未发布版本下无真实库升级风险，测试库可弃）。**P0-5 冻结：发布后 SCHEMA_V4 升级不存在 git revert 式数据库降级**，降级唯一路径 = restore 迁移前备份（测试覆盖该路径）。
- 需批准：阶段计划批准（M0 已含 D-1/D-2 则自动放行）。

## M1-B — GUI / CLI

- 目标：Tauri 11 个 stack/subscription 命令 + Vue 页面/路由/store/i18n + CLI 2 只读 + 8 写子命令（optimizer 命令与 cost CLI 全部在 M2-B，不提前——M9 对齐）。
- 文件范围：
  - `src-tauri/src/lib.rs`：11 个命令薄壳 + generate_handler 注册（`stack_list / stack_get / stack_upsert_item / stack_confirm_item / stack_merge_items / stack_unmerge_item / stack_delete_item / stack_refresh_detections / subscription_upsert / subscription_delete / spending_overview`；`optimizer_report / optimizer_simulate` 归 M2-B）
  - `src/lib/api.ts`：+stack 段；`src/lib/types.ts`：镜像 struct + **ErrorCode 联合类型追加 `RESOURCE_CONFLICT`**（D-2 冻结；ErrorBanner 增补对应文案）
  - `src/router.ts`：navItems + /stack + /stack/:id 路由；`src/components/Icon.vue`：stack 图标（可选）
  - `src/pages/StackOverviewView.vue`、`src/pages/StackDetailView.vue`（新）；`src/stores/stack.ts`（新，照 skills.ts TTL 缓存模板）
  - `src/i18n/zh.ts` + `en.ts`：nav/stack/spending 全节（zh 先写、en 镜像）
  - `crates/agenthub-cli/src/main.rs`：Commands 枚举 + run() 分发 + cmd_stack_*（集中一个区段）：只读 `stack list [--json]`、`stack show <id> [--json]`（2 条，spending 概要并入 `stack list` 输出）；写 `stack add|edit|merge|confirm|delete`、`sub add|edit|delete`（**8 条**，D-1 形态：delete/merge 非交互必须 `--yes`）；`map_api_error` 追加 `RESOURCE_CONFLICT` → exit 4 映射（D-2）。`cost report/simulate` 归 M2-B。
- 依赖：M1-A 合入。
- 风险：⌘1-9 快捷键按 navItems 数组下标映射（App.vue:101-105），**缓解：新导航项追加在 navItems 末尾（下标 ≥9），绝不插在中部**；CLI main.rs 进一步膨胀（控制 ~200 行）；i18n 遗漏导致 vue-tsc 失败（类型系统天然强制，反而安全）。
- 测试：vitest 页面冒烟（照 health-ui.spec.ts 的 vi.mock('../lib/api') 模板）：空态/列表/详情/编辑表单/删除确认/spending 展示（USD30 非当月扣款文案断言/CNY 分组）；CLI `--json` 信封与 GUI 同构断言（**比较规范化业务 data 字段，requestId 不计入**——ACCEPTANCE §2.13 冻结口径）。
- 验收标准：`pnpm test` 全绿；`pnpm build`（vue-tsc + vite）通过；手动冒烟清单（空态→添加→登记订阅→查看 spending）走查记录。
- 回滚：revert；路由/导航为加法，无兼容包袱。
- 需批准：是。

## M1-C — 测试与验收

- 目标：补齐 ACCEPTANCE_MATRIX 全矩阵执行 + 证据归档。
- 范围：测试补充 + **`scripts/stack_acceptance.sh`（新，纯 fixture：隔离临时 `AGENTHUB_HOME`、测试构造 fixture 库，不读真实 `~/.agenthub`/Agent 目录——P0-5）** + `docs/proposals/ACCEPTANCE_MATRIX.md` 填写"实际结果/证据"列；不修业务代码（缺陷另开修复切片）。
- 依赖：M1-B。
- 风险：沙箱 Keychain 失败中断套件（缓解：`--test` 分片 + 正常终端复跑并四态标记）。
- 验收：矩阵全部场景标注四态 [✅通过/❌失败/⚠️环境限制/⏳未执行]，无"未跑标过"，❌ 必须附失败分析与修复切片。
- 回滚：N/A（纯测试产出）。
- 需批准：验收结论提交批准（GO/NO-GO 进入 M2）。

## M2-A — 优化规则引擎

- 目标：`crates/agenthub-core/src/optimizer.rs`（R1-R5 + Recommendation 契约 + simulate）+ service 两个只读方法 + Settings 参数（阈值/窗口/overlap_group 编辑入口的 core 侧）。
- 文件：optimizer.rs（新 ~350 行）、model.rs（Recommendation 等）、service.rs（+2 方法）、settings 扩展（AppSettings 加 2 个可选字段，向后兼容 serde default）。
- 依赖：M1-C GO。
- 风险：低（纯函数，无 IO）；主要风险是规则语义被误读——用 PRD 措辞硬约束写进测试断言。
- 测试：规则触发/静默矩阵；确定性（同输入两次输出相等）；降级（missing_information 非空）；simulate 算术（含多币种、缺失字段不参与）；无网络断言。
- 验收：新测试 ≥ 20 个全绿 + 现有零回归。
- 回滚：revert。
- 需批准：是（规则语义终稿）。

## M2-B — 优化建议 UI

- 目标：GUI 建议面板（StackOverview 内 tab 或独立页）+ Tauri `optimizer_report / optimizer_simulate` 命令（+2）+ CLI `cost report/simulate`（只读，+2 cmd）+ Potential/Confirmed/Realized 三态展示 + 数据缺口跳转补全。
- 文件：src/pages（+OptimizerView.vue 或扩展）、stores/stack.ts 扩展、i18n、api.ts、main.rs +2 cmd。
- 依赖：M2-A。
- 风险：低；文案合规（禁"已节省"）靠测试断言。
- 验收：vitest 冒烟 + 文案断言 + CLI/GUI 同构。
- 回滚：revert。
- 需批准：是。

## M2-C — 端到端验收

- 目标：全矩阵复跑 + REAL 数据零接触断言 + 文档同步（ARCHITECTURE/SECURITY/CHANGELOG 增补"已验证"结论）。
- 依赖：M2-B。
- 验收：ACCEPTANCE_MATRIX M2 部分全绿或三态标注；`./scripts/v31_acceptance.sh` 零回归（其迁移测试需感知 user_version=4——**注意**：该脚本对 ~/.agenthub 副本做 v2→v3 迁移断言，v4 引入后需在 M2-C 检查脚本对 user_version 的假设，只读确认后再改，改动需批准）。
- 回滚：N/A。
- 需批准：终验报告。

## M3 — TokenPlan 在线价格服务（仅规划，不实施）

- 规划边界：价格目录须带 source_url/verified_at/validity/stale；未核验价格不得进入确定性推荐；任何网络出站需单独 ADR + 安全评估 + 用户授权 UI。本阶段只产出规划文档，不改代码、不打开网络权限。

## 阶段间依赖图

```
M0 ──批准──▶ M1-A ──▶ M1-B ──▶ M1-C ──GO──▶ M2-A ──▶ M2-B ──▶ M2-C ──▶ (M3 规划)
```

## 执行命令备忘（全部隔离）

```bash
./scripts/cargo.sh test -p agenthub-core --test stack_subscriptions --test stack_migration   # M1-A
./scripts/cargo.sh test -p agenthub-core -p agenthub-cli                                    # 全量（--test 分片绕开 mcp Keychain）
pnpm test && pnpm build                                                                     # 前端
AGENTHUB_HOME=/tmp/test/home ./scripts/v31_acceptance.sh                                     # 既有回归参考（注意：脚本会读取真实 ~/.agenthub 副本与 Agent 目录，仅 M2-C 回归用）
# 本模块新增验收：纯 fixture 脚本（隔离临时 AGENTHUB_HOME + 测试构造 fixture 库，M1-C 交付，P0-5）
# 不执行 real-acceptance.sh（需机器所有者显式批准且写真实目录）
```
