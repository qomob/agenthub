# M2-A 交付报告 — Cost Optimizer 离线规则引擎

- 基准：`ea2429f`（M1 结案后主仓库状态）；本阶段 commit `0948452`、`c51bb7b`
- 授权：负责人裁定 M1 = PASS · CLOSED、M2-A = READY 后，随「推送 + 启动下阶段开发」指令启动
- 范围冻结：**只做 M2-A（core 规则引擎 + 只读 service 路径）**；GUI 面板、CLI `cost report/simulate`、Settings 阈值/窗口 UI 属 M2-B，本轮未动
- 状态四态：**PASS** = 已运行并通过；**FAIL**；**ENV_BLOCKED**；**NOT_RUN** = 范围外

---

## 1. 终态三行

| 项 | 结论 |
|---|---|
| M2-A 规则引擎 | **COMPLETE**（R1–R5 + 确定性 ID + 模拟，166 项 core 测试全绿） |
| M2 整体 | **IN PROGRESS**（M2-B：GUI 面板 / CLI 命令 / Settings UI 未动） |
| v0.6.0 | **FROZEN**（未打 tag、未发布） |

---

## 2. 交付内容（commit `0948452`）

| 文件 | 内容 |
|---|---|
| `crates/agenthub-core/src/optimizer.rs` | 新增：无状态规则引擎（约 1000 行含 13 个单测） |
| `crates/agenthub-core/src/lib.rs` | 注册 `pub mod optimizer` |
| `crates/agenthub-core/src/service.rs` | 只读方法 `optimizer_report()` / `optimizer_report_with_config()` / `optimizer_simulate()` |
| `crates/agenthub-core/tests/optimizer_service.rs` | 新增：4 个 service 层集成测试 |

**未触碰**：Rust 其他模块、DB schema（无迁移）、Tauri 命令、CLI、前端。设计完全遵循 PRD：引擎照 `validation.rs::derive_health` 模式——纯函数输入 `(stack_items, subscriptions, config)`，不触库、不加锁、无网络。

## 3. 五条规则与降级语义

| 规则 | 触发 | 静默条件 | 硬约束落实 |
|---|---|---|---|
| R1 重叠提示 | 同一 `overlap_group` 内 ≥2 个未合并条目 | 用户未维护分组 → 静默（不算缺陷） | 只说「潜在重叠」；组内多条订阅明确写「不自动判定为重复扣费」（P0-2/I5）；不做语义推断 |
| R2 高成本复核 | 月度等效 ≥ 用户按币种设的阈值 | 无阈值配置 → 规则整体静默 | 不称闲置/浪费；hybrid 只比已知固定费并注明「浮动部分未计」 |
| R3 续费提醒 | `renews_on` 在窗口内 / 已过而仍 active | — | 「续期已过未更新」独立成组（数据陈旧可见，m14）；只读展示，无推送 |
| R5 数据缺口 | 缺金额/币种/周期/续期日/类型未知/陈旧续期日 + 有订阅但从未确认的工具 | — | 所有规则的降级出口：`missing_information` 非空、`user_action_required=complete_data` |
| R4 情景模拟 | 用户给出取消/降额/替换/停用动作 | — | 纯算术；**不落库**；notes 首条固定「simulation only: no subscription was changed and no money was saved yet」；按币种分组，跨币种不相加；缺失事实的订阅排除出算术并列入 notes |

规则参数收敛在 `OptimizerConfig`：R2 阈值表（空 = 静默）、R3 窗口（默认 7，clamp 1–30）、`today`（可冻结以支撑确定性测试）。M2-A 阶段 Settings 尚无这些字段的持久化（属 M2-B），因此当前 service 默认配置下 R2 静默、R3 用 7 天窗口——与 PRD「无阈值 → 规则静默关闭」一致，不是缺陷。

## 4. 建议契约（`Recommendation`）落实

- **确定性 ID**：`rec_` + `sha256(规则 | items 排序 | subs 排序 | params)[:12hex]`。同输入同 ID，`full_report_is_byte_stable_for_identical_inputs` 测试断言两次构建的 JSON 字节相等。
- **置信三级**：`input_only`（R1/R3-stale/R5）、`user_configured`（R2/R3-upcoming）、`derived_arithmetic`（R4）。**无 "verified" 级**。
- **节省语义三态**：本引擎只产出 Potential（R4 模拟内）与提示；无 Realized 概念，UI 文案约束留待 M2-B 在面板侧落实。
- 错误处理：引擎不产 Error；`optimizer_simulate` 对未知订阅/条目、非法金额/币种/周期返回 `Error::invalid_input`（D-2 契约，exit 3）。

## 5. 测试证据（17 项新增）

### 引擎单测（13，`optimizer.rs`）

| 测试 | 锁定的规则 |
|---|---|
| r1_stays_silent_without_overlap_groups | R1 静默条件 |
| r1_prompts_for_two_items_in_one_group_without_duplicate_claim | R1 措辞硬约束（断言不含 "double charge"/"replaceable"，含 "not automatically treated as duplicate charges"） |
| r2_silent_without_thresholds_and_fires_at_or_above | R2 静默 + 达阈值触发 |
| r2_ignores_hybrid_floating_part_and_unknowns | hybrid 只计固定费；usage 带金额不进 R2（进 R5） |
| r3_lists_upcoming_and_stale_separately | R3 两个桶分离 |
| r5_buckets_every_gap_type | 缺金额、陈旧续期日分桶；complete_data |
| r5_flags_item_with_subscriptions_never_confirmed | 从未确认条目 |
| deterministic_ids_survive_input_ordering | **ID 与输入顺序无关**（本轮发现的顺序敏感 bug 由它抓出并修复） |
| full_report_is_byte_stable_for_identical_inputs | 字节级可重复（PRD 验收要点） |
| r4_cancel_scenario_produces_single_currency_delta | 取消算术（3000→1000，delta −2000；年 −24000） |
| r4_reduce_and_replace_across_currencies_stay_separate | 多币种不相加（USD 与 CNY 分行） |
| r4_unknown_ids_are_rejected_as_invalid_input | D-2 契约 |
| r4_deactivate_item_cancels_all_active_subscriptions | 停用工具只影响 active 行 |

### Service 集成测试（4，`optimizer_service.rs`）

| 测试 | 锁定的行为 |
|---|---|
| optimizer_report_reads_registered_data_and_r1_silence_without_groups | service 读到的数据与登记一致 |
| optimizer_report_fires_r1_for_shared_group_and_lists_costs | 报告可重复（两次调用 ID 相同） |
| optimizer_simulate_is_pure_arithmetic_and_never_writes | 模拟后订阅仍 active；**audit_events 数量不变**（引擎不写任何东西） |
| optimizer_simulate_rejects_unknown_subscription_as_invalid_input | D-2 三态 |

## 6. 本轮发现并修复的缺陷（测试先行的价值）

1. **R5 ID 顺序敏感**：桶内条目按输入顺序拼接后参与哈希，同数据不同顺序会得到不同 ID——`deterministic_ids_survive_input_ordering` 抓出后，改为排序订阅与桶内条目再哈希。这正是「随机 uuid 被否决、确定性 ID 必须可测」的意义。
2. **金额格式化错误**：R2 说明文最初用 `{:.2}` 格式化整数 minor，输出 `6000 USD` 而非 `60.00 USD`；改为整数除法显式拼分。
3. 首个实现里的 R2 测试期望值本身写错（6000 minor 被我当成 60.00×100），经打印实际输出纠正——引擎语义（minor 单位、除以 100）正确，测试修正。

## 7. 全量回归

| 项目 | 结果 |
|---|---|
| `cargo test -p agenthub-core` | **166 passed / 0 failed**（149 + 13 引擎 + 4 service） |
| `cargo test -p agenthub-cli` | **31 passed / 0 failed** |
| `cargo check -p agenthub --lib` | exit 0 |
| `pnpm test` | 15 files / 93 passed（前端无改动，守门） |
| `pnpm build` | exit 0 |
| `scripts/stack_acceptance.sh` | 43 passed / 0 failed（纯 fixture） |
| 无网络断言 | `optimizer.rs` 无 reqwest/ureq/http/net 导入（grep 证据） |
| 引擎不写断言 | service 测试断言 optimizer 运行前后 `audit_events` 数量不变 |

## 8. 推送与证据登记

- **主仓库已推送**（`qomob/agenthub`，public）：`b5ef694..c51bb7b`，11+2 个 commit。推送前执行机密扫描（token 模式）与个人路径扫描，3 份报告中的本机绝对路径（fixture 目录、真实 home 路径）已替换为 `<FIXTURE_HOME>` / `~/.agenthub` 占位符（commit `69c4359`）；结论不变。
- **私有证据仓库已建立**：`qomob/agenthub-evidence`（Private），对象 `archives/m1/2026-11-12/agenthub-m1-acceptance-20261112.tar.zst`，commit `737ea43`，SHA-256 `8298f372ca4a56722c8b5fd7f29e7e480159175f3de38c89277f4b1347b909a2`，远端 clone 复核 + `zstd -t` PASS。
- **哈希变更如实登记**：上传前扫描发现 42 张原始截图中 24 张混入本机第三方应用通知、且首版包含未脱敏原件，按门禁重新打包并重算哈希；首版 `8f1ad97f…` 作废。未脱敏原件不入任何仓库，仅存采集机。
- 主仓库 `docs/evidence/m1/README.md` 已登记 Storage/Repository/Object/SHA-256/Commit/Verification/Access/Status 八项及哈希变更原因（commit `ea2429f`、`c51bb7b`）。

## 9. 审查修正（commit `82b848f`）

负责人 M2-A 审查指出两项问题，均已修复并测试锁定：

| # | 问题 | 修复 |
|---|---|---|
| P1 | `deltas_between()` 只检查字段齐全，带金额/币种/周期的 `usage` 记录会进模拟合计——取消它可能同时给出节省金额和「已排除」说明 | 引入 `foldable_parts()` 作为**唯一折算资格门**（usage/unknown 不可折算、free 记零、仅 active 参与），R2/R4 算术与 R5 缺口说明共用；模拟算术改用 `money::aggregate_currency()`（与成本卡一致）；缺口说明从「仅受影响」扩展为**所有不可折算的 active 记录**，使「排除」与「计入」永远一致 |
| P2 | `today: None` 时引擎读系统时钟，跨午夜/时区结果不同 | 引擎**不再读时钟**：`build_report` 对未钉日期返回 `INVALID_INPUT`；service 层是唯一解析业务日期的地方（**复验通过**，commit `82b848f`） |
| P1 二审 | `Replace` 补齐原缺失金额后记录进入 AFTER 合计，但说明仍按原始快照称其「已排除」——数字与文字矛盾 | 排除说明**按状态分别声明**：「Excluded from the BEFORE totals…」与「Excluded from the AFTER totals…」各自经同一 `foldable_parts` 门计算；记录补齐后自动从 AFTER 清单消失（**已修复**，commit `e0436f2`） |
| 测试结构 | `r4_usage_with_amount…` 带重复 `#[test]`（执行两次，掩盖同块覆盖）；`r4_unknown_ids…` 丢失 `#[test]`（从未执行） | 两处属性已修正；并补上**真正的 Replace 用例**：原无金额订阅经 Replace 补齐后从 BEFORE 排除清单移入 AFTER 合计（delta +2000） |

新增 5 项测试（含 replace 路径与无时钟规则），先 RED 后 GREEN；core 套件 169 passed。M2-B 范围确认采纳审查口径：GUI 建议面板、CLI `cost report/simulate`、Settings 参数管理，阈值与窗口走同一套 Service 校验与存储。

## 10. 遗留事项

| # | 事项 | 处理 |
|---|---|---|
| L1 | R2 阈值与 R3 窗口尚无 Settings 持久化（无 schema 变更），当前默认 R2 静默 | M2-B 在 Settings UI + 存储落地后接通 `optimizer_report_with_config` |
| L2 | 分支保护（私有仓库 main 禁删/禁强推）需 GitHub Pro | 已登记为部分满足 + 限代措施；如需硬保护请升级账号 |
| L3 | GUI 面板与 CLI `cost report/simulate` | M2-B 范围，未动 |
| L4 | Public 主仓库仍冻结在 v0.5.0 tag；未打新 tag、未发布 | 维持 FROZEN |

---

**M2-A 至此完成，停止等待负责人对 M2-B 的授权审查。**
