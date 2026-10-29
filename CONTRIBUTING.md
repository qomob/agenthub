# Contributing Guide

AI 协作者请先读 [AGENTS.md](./AGENTS.md)（仓库地图与硬约束）；领域概念见 [docs/CONTEXT.md](./docs/CONTEXT.md)。

## 环境要求

- Node.js >= 20、pnpm；Rust stable（rustc >= 1.89，见 `rust-toolchain.toml`）。
- 前端：`pnpm install`；核心：`cargo build -p agenthub-core`。
- 本机若 `rustc --version` 显示 Homebrew 1.87（过旧），改用 rustup shim：
  `PATH="$HOME/.cargo/bin:$PATH" cargo test`。

## 提 Issue 前先分流

| 场景 | 去处 |
|---|---|
| 装不上、面板打不开、CLI 报错 | 先看 README「排错」与 `docs/handoffs/LATEST.md` |
| 不确定是不是 Bug、想先讨论方案 | Discussions |
| 可复现的缺陷 | Bug 报告，务必写清**端（GUI/CLI）、版本、操作系统、复现步骤** |
| 新功能 / 改进建议 | 需求与改进，附「想回答用户什么问题」 |

## 分支规范

`<area>` ∈ `core` | `cli` | `gui` | `docs` | `adapters`（跨模块取影响最大者）。

| 类型 | 格式 | 示例 |
|------|------|------|
| 新功能 | `feat/<area>/<short-desc>` | `feat/cli/service-status` |
| 修 bug | `fix/<area>/<short-desc>` | `fix/core/rollback-order` |
| 文档 / 构建 | `chore/<short-desc>` | `chore/readme-privacy` |

跨模块改动在 PR 正文写清影响范围（例如：改了 `service.rs`，CLI 与 Tauri 命令层是否都同步）。

## Commit 规范

[Conventional Commits](https://www.conventionalcommits.org/zh-hans/v1.0.0/)，主语用英文：

```text
<type>(<scope>): <subject>
# feat(core): add cross-process runtime lock for mutating operations
# fix(gui): keep dashboard stats in sync after adopt
```

`type`：`feat | fix | docs | style | refactor | perf | test | build | ci | chore`。

## 提 PR 清单

- [ ] `cargo test -p agenthub-core` 全绿（当前约 113 个）；
- [ ] 前端改动另跑 `pnpm test` 与 `pnpm build`；
- [ ] 行为改动附带测试；修 bug 先写复现测试；
- [ ] 触碰真实 Agent 目录写入的，必须走 `ops.rs` Executor 并持有 runtime lock；
- [ ] 更新 `docs/ARCHITECTURE.md` 的「已验证 / 未验证 / 未实现」标记，**不做超前宣称**；
- [ ] PR 描述写「对用户的影响」，而不是罗列实现细节。

## 开发工作流

1. **spec → plan → 实现**：跨模块改动先写设计要点（PR 描述或 `docs/handoffs/`）。
2. **单引擎双前端**：改 `service.rs` 后检查 CLI 与 Tauri 两端命令是否同步。
3. **证据留档**：真实验收截图/输出放 `docs/evidence/`，方法论写进 `docs/VALIDATION.md`。
4. **测试即验收**：每个行为改动附带测试（core 走 Rust 测试，前端走 vitest）；修 bug 先写复现测试。

## 漏洞私密报告（占位）

**请不要用公开 Issue 报告安全漏洞**（凭据泄露、路径穿越、写入越界、供应链等）。安全问题的报告渠道（安全邮箱 / GitHub Security Advisories / 私密联系方式）**待维护者确认后在此填入**，当前暂未配置。

- 报告时请附：影响面（GUI / CLI / core / 适配器）、复现步骤或 PoC、涉及的版本（`package.json` / `Cargo.toml`）。
- 在收到维护者回应前，请不要公开披露细节。
- 非私密的安全设计讨论参见 [docs/SECURITY.md](./docs/SECURITY.md)（威胁模型与已知风险）。

## 行为准则（占位）

本项目尚未发布正式 Contributor Covenant；在此前，请遵守基本约定：对事不对人、尊重不同经验水平的贡献者、讨论聚焦技术与用户价值。正式行为准则文本待维护者确认后补充。
