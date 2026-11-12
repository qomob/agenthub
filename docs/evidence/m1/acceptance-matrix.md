# M1 验收矩阵（证据索引）

把验收结论映射到可核对的证据。结论以报告与负责人裁定为准，本表只负责索引。

## M1-C · 真实应用集成验收

| # | 验收项 | 结论 | 证据 |
|---|---|---|---|
| 1 | 真实 Tauri 应用端到端可用（非 jsdom） | PASS | 全部截图 |
| 2 | 确认流写入并在界面呈现 | PASS | `screenshots/confirm-flow.png` |
| 3 | 订阅编辑字段回填、保存后行值更新 | PASS | M1-C 报告 §V1（完整包 `G3-*`） |
| 4 | 合并过滤非法目标、成功后展示目标 ID | PASS | `screenshots/merge-modal.png` |
| 5 | 撤销合并恢复独立，成本总额不变 | PASS | `screenshots/merge-undo-confirm.png`、`merge-undone.png` |
| 6 | 仍有关联订阅时拒绝删除 | PASS | `screenshots/delete-blocked.png` |
| 7 | 取消危险操作不产生写入 | PASS | M1-C 报告 §V1（Cancel 后条目数不变） |
| 8 | 重复刷新检测幂等 | PASS | 完整包 `G7-refresh-twice.png` |
| 9 | 中英切换全站即时生效 | PASS | 完整包 `G9-english.png`、`G10-*` |
| 10 | 汇总溢出显示错误而非 0.00 | PASS | `screenshots/spending-overflow.png` |
| 11 | 缺金额/币种显示「信息不完整」，usage 不估算 | PASS | `screenshots/incomplete-cost.png` |
| 12 | CLI 与 GUI 同一 fixture 数据一致 | PASS | `cli-vs-db.txt` |
| 13 | 成本汇总陈旧缓存 | **FAIL**（后修复） | `screenshots/stale-cache-before-redacted.png` |
| 14 | `stack list --json` 不含 spending，失败静默 | 记录为已知缺口 | 完整包 `S4-cli-vs-db.txt` |

## M1-C.1 · 缓存失效修复

| # | 验收项 | 结论 | 证据 |
|---|---|---|---|
| 15 | 本应用内删除订阅后立即刷新 | PASS | `screenshots/stale-cache-after.png` |
| 16 | 刷新失败保留上次良好数据 | PASS | `stack-store.spec.ts` |
| 17 | 旧响应不覆盖新响应 | PASS | `stack-store.spec.ts` |
| 18 | 并发读取共享单个在飞请求 | PASS | `stack-store.spec.ts` |

## M1-C.1 Final · 刷新契约

| # | 验收项 | 结论 | 证据 |
|---|---|---|---|
| 19 | 页面驻留 40 秒不自动刷新 | PASS（真实应用） | `resident-t0.png` vs `resident-t40.png`（哈希相同） |
| 20 | 路由重新进入触发读取，机制为组件重挂载 | PASS（真实应用） | 报告 §实验二；`src/App.vue:179` 无 `<KeepAlive>` |
| 21 | 窗口重新获焦触发读取（修复后） | PASS（真实应用） | `focus-refresh.png`（8→9 条，258.00→278.00） |
| 22 | TTL 不再被表述为定时刷新 | PASS | `stack-refresh-contract.spec.ts` + `empty-state-copy-zh.png` |
| 23 | 文案明示 TTL 命中例外 | PASS | 同上第 9 条测试 |
| 24 | 契约测试具备 RED 反证 | PASS | 回退实现/文案分别致 1 failed / 3 failed |

## 全量回归（HEAD 提交时）

| 项目 | 结果 |
|---|---|
| `pnpm test` | 15 files / 93 tests passed |
| `pnpm build` | exit 0 |
| `cargo test -p agenthub-core` | 149 passed / 0 failed |
| `cargo test -p agenthub-cli` | 31 passed / 0 failed |
| `cargo check -p agenthub --lib` | exit 0 |
| `scripts/stack_acceptance.sh` | 43 passed / 0 failed（纯 fixture） |

## 未覆盖 / 明确不声称

- 未验证 Windows / Linux 桌面端。
- 未验证真实第三方 Agent 目录写入（按设计本阶段不执行分发）。
- `onActivated` 分支因项目无 `<KeepAlive>`，当前无真实路由复用路径可验，属预留契约。
- 数值判定均来自全分辨率裁剪；**不用缩略图读数**（曾因此误判「27.50 显示成 2750」为缺陷）。
