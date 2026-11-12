# M1-B 实现报告 — My AI Stack GUI + CLI

- 基准 commit 链（真实 Git 历史，自旧到新）：`fc4088e`（M1-A 数据模型与服务）→ `114794c`（F1 冻结案例回归 + F2 中途迁移回滚证明）→ `48a37af`（N1/N2 严谨性补强，M1-B preflight）→ **`7c059f7` feat: My AI Stack GUI + CLI (M1-B)**（16 文件，+3892/−4）
- 授权：负责人 m01029 裁定，M1-B GO ONLY；完成后停止等待 M1-C 审查
- 状态四态：**PASS** = 已运行并通过；**ENV_BLOCKED** = 环境限制未通过；**FAIL**；**NOT_RUN** = 授权范围外或未执行

## 1. Commit 与文件变更清单

| 文件 | 类型 | 内容 |
|---|---|---|
| `src-tauri/src/lib.rs` | 修改 | 11 个 Tauri 命令（`#[tauri::command(rename_all = "snake_case")]`，对齐既有 56 个命令约定）+ `generate_handler` 注册 |
| `src/lib/types.ts` | 修改 | My AI Stack 全类型（11 个联合类型 + 11 个 camelCase 视图类型） |
| `src/lib/api.ts` | 修改 | 11 个 API 方法，全部走 `callApi` v3.1 信封 |
| `src/lib/stack.ts` | 新增 | 币种指数表、金额整数格式化/解析（无浮点）、i18n key 映射 |
| `src/stores/stack.ts` | 新增 | Pinia store（TTL 缓存、spending 失败独立存入 `spendingError`） |
| `src/router.ts` | 修改 | `/stack`、`/stack/:id` 路由；`navItems` **末尾**追加 `stack`（⌘1-9 按下标，不插中部） |
| `src/components/Icon.vue` | 修改 | 新增 `stack` 图标 |
| `src/i18n/zh.ts` / `en.ts` | 修改 | `nav.stack` + `stack.*` 小节（zh/en 键完全镜像） |
| `src/pages/StackView.vue` | 新增 | Stack 总览页 |
| `src/pages/StackDetailView.vue` | 新增 | 工具详情页（检测/订阅/合并子项） |
| `src/__tests__/stack-ui.spec.ts` | 新增 | 12 个组件测试（UX-1/UX-2/UX-3） |
| `crates/agenthub-cli/src/main.rs` | 修改 | `stack`/`sub` 两组命令（+700/-0，纯新增，无既有代码重排） |
| `crates/agenthub-cli/tests/stack_cli.rs` | 新增 | 9 个 CLI 进程级契约测试 |
| `crates/agenthub-core/src/service.rs` | 修改 | `stack_merge_items` 返回合并后视图（修 stale 行为 bug） |
| `crates/agenthub-core/tests/my_ai_stack.rs` | 修改 | 合并返回视图回归断言 |

## 2. GUI / CLI / Service 接口映射

M1-A 提供的 14 个 Service 方法全部接入（单引擎双前端，前端零业务逻辑）：

| Service（agenthub-core） | Tauri 命令 | Vue API | CLI | 备注 |
|---|---|---|---|---|
| `stack_items()` | `stack_list` | `api.stackList()` | `agenthub stack list`（附 spending 概要） | |
| `stack_item_detail(id)` | `stack_get` | `api.stackGet(id)` | `agenthub stack show <id>` | |
| `stack_upsert_item(u)` | `stack_upsert_item` | `api.stackUpsertItem(u)` | `stack add` / `stack edit` | |
| `stack_confirm_item(id)` | `stack_confirm_item` | `api.stackConfirmItem(id)` | `stack confirm <id>` | |
| `stack_merge_items(child,into)` | `stack_merge_items` | `api.stackMergeItems(c,i)` | `stack merge`（--yes） | |
| `stack_unmerge_item(id)` | `stack_unmerge_item` | `api.stackUnmergeItem(id)` | —（冻结清单：CLI 不暴露） | |
| `stack_delete_item(id)` | `stack_delete_item` | `api.stackDeleteItem(id)` | `stack delete`（--yes） | |
| `stack_refresh_detections()` | `stack_refresh_detections` | `api.stackRefreshDetections()` | （CLI 无独立刷新命令） | |
| `subscription_upsert(u)` | `subscription_upsert` | `api.subscriptionUpsert(u)` | `sub add` / `sub edit` | |
| `subscription_delete(id)` | `subscription_delete` | `api.subscriptionDelete(id)` | `sub delete <id>`（--yes） | |
| `subscriptions()` | — | — | — | **内部间接消费**：`stack_get` detail 与 CLI `sub edit` 预填经 Service 内部调用 |
| `spending_overview()` | `spending_overview` | `api.spendingOverview()` | `stack list` 概要段 | |
| `spending_overview_with_window(n)` | — | — | — | **内部间接消费**：`spending_overview` 命令固定使用 core 默认 7 天窗口 |
| `stack_audit_events(limit)` | — | — | — | **内部间接消费**：M2 消费面，M1-B 未对外暴露（设计内） |

口径说明（M1-C V4 修正）：14 个 Service 方法 = **11 个 Tauri 命令对外暴露 9 个** + 3 个内部间接消费（`subscriptions`/`spending_overview_with_window`/`stack_audit_events`）；CLI 显式命令覆盖其中 10 个 Service 调用。不构成"14 个独立对外接口"。

Tauri 层仅做参数转换 + `state.run_api` 信封包装；不复制业务规则；不绕过 `runtime_guard`/`write_lock`/审计。

## 3. 新增测试数与全量结果

| 套件 | 结果 | 状态 |
|---|---|---|
| `cargo test -p agenthub-core` | **149 passed / 0 failed**（18 目标） | PASS |
| `cargo test -p agenthub-cli` | **31 passed / 0 failed**（含新 `stack_cli` 9 测） | PASS |
| `cargo check -p agenthub --lib`（Tauri） | 0 error / 0 warning | PASS |
| `pnpm test`（vitest） | **13 files / 67 passed / 0 failed**（含新 `stack-ui.spec.ts` 12 测 + `i18n.spec.ts` 键镜像强制） | PASS |
| `pnpm build`（vue-tsc + vite） | 成功 | PASS |

M1-B 新增测试：Rust 10（CLI 9 + 合并返回视图回归 1）+ 前端 12 = **22 个**。

关键 CLI 契约测试（进程级，隔离 FakeHome）：
1. `--json` 信封 camelCase 形状 + 随机 requestId；
2. `sub add` 金额无币种 → `INVALID_INPUT` exit 3，未知金额保持 `null`；
3. `"20.5" USD` → 2050 minor；`"100.5" JPY`（exponent 0）→ 拒绝；
4. `stack delete` / `stack merge` / `sub delete` 无 `--yes` → exit 1 拒绝且数据不变（UX-3 确认契约）；
5. 有订阅的工具 `stack delete` → `RESOURCE_CONFLICT` exit 4；
6. G1 链式合并（已合并子再作源）→ exit 4；
7. **UX-4**：CLI `--json` data 与直接调 Service 的 serde 数据逐字节相等；
8. `stack list` human 模式分币种输出 + "monthly equivalent, not this month's charge" 标签 + incomplete 计数。

## 4. 关键用户流程验收证据（UX-1/UX-2/UX-3）

GUI 证据 = jsdom 挂载**真实组件**（vi.mock 仅替换 Tauri invoke 层）+ vue-tsc 类型检查；非人工视觉走查（NOT_RUN，归 M1-C）。

- **UX-1 成本异常不得伪装为零**（`stack-ui.spec.ts`）：spending 聚合超限返回错误时——items 列表仍可见、显式失败 banner、**断言页面无 `(?!0\.00)` 伪零**；`stores/stack.ts` spendingError 与 items 数据独立；CLI 侧 `stack list` spending 失败打印原因并保留 items 行。
- **UX-2 不完整成本语义**：金额未知渲染 `金额未知` 标签而非 0；缺失字段 chips（`amount/currency/billing_period/billing_type/usage_not_foldable`）由 core 计算（UI 不自算）；分币种各行不混加；"monthly equivalent" 提示非当月扣款。
- **UX-3 检测与合并**：删除/合并/unmerge 全部先过 ConfirmDialog（断言弹窗出现前 API 未调用）；CLI `--yes` 门禁；刷新幂等、已合并条目过滤出合并候选（GUI）+ G1 拒绝（CLI exit4）；unmerge 后 detail 保持（订阅 FK core 层保证）。
- 完整闭环：检测刷新 → 确认清单 → 登记订阅 → 查看成本 → 手动添加/编辑/合并/撤销合并/删除确认/空态引导/异常态/中英文切换，均可在 `/stack` 与 `/stack/:id` 完成。

## 5. 中英文界面与错误态验证

- `i18n.spec.ts`（既有强制测试）通过：`zh.ts`/`en.ts` flatten 键完全镜像。
- `stack-ui.spec.ts` 断言中文文案渲染（`金额未知`、缺失 chips、必填校验文案）。
- 错误态：`normalize()` 将 `RESOURCE_CONFLICT`/`resource conflict` 映射为用户可读文案；`ErrorBanner` 支持重试；`EmptyState` 引导手动添加。

## 6. GUI / CLI 数据一致性（UX-4）

- 同一 fixture 下，CLI `--json` 输出的规范化业务 `data` 与 Service 层直接调用结果**逐字段相等**（`cli_json_data_equals_service_layer_data`，含字段排序规范化）。
- GUI 经同一 Tauri 命令层调同一 Service；requestId 随机值不参与比较。
- 写操作均登记 `audit_events`（GUI/CLI 写后均可追踪）。

## 7. 真实用户目录零触碰

- 全部 Rust 测试使用隔离 FakeHome fixture；前端测试 mock invoke 层。
- 未执行 `real-acceptance.sh`、`v31_acceptance.sh`；无网络调用；未读写真实 `~/.agenthub`、`~/.codex`。
- 未提交/未触碰：`.workbuddy/`、`docs/promotion/`、`docs/proposals/`（保持未跟踪草稿）。
- 仓库 fmt 纪律：本切片 diff 为 `+3892/−4`；`main.rs` 为 `+700/−0` 纯新增（曾发生 `cargo fmt -p` 整包重排事故，已全部还原，见风险 R3）。

## 8. 已知风险、未执行项与 M1-C 判断

风险：
- **R1（中）**：GUI 界面证据为组件级测试，非真机视觉验收 → M1-C 必须做真实界面走查。
- **R2（低）**：CLI 未暴露 unmerge（冻结 8 写命令清单所致），撤销合并仅 GUI 可做 → 已在 `--help` 文案说明。
- **R3（低）**：`cargo fmt -p` 会整包重排未格式化的既有代码（本次已还原）；全仓既有 fmt 差异按裁定不处理，但后续切片禁用包级 fmt。
- **R4（低）**：spending 聚合超限时 CLI `stack list` 返回码不变、仅 human 段失败提示；JSON 模式仅输出 items——与 GUI UX-1 行为一致，但 CLI 用户可能忽略失败行。

未执行（NOT_RUN）：真机 GUI 视觉验收、`pnpm tauri build` 打包、真实用户数据验收、`v31_acceptance.sh` 回归。

**M1-C 判定：READY** —— M1-A（数据层/服务层）与 M1-B（GUI/CLI）已形成可验收的本地用户闭环，22 个新测试全绿；建议 M1-C 覆盖：真机界面走查、UX-1/UX-2 异常态实操、v31 回归、GUI/CLI 真实数据一致性。

按授权要求，本报告交付后停止，等待负责人批准进入 M1-C。
