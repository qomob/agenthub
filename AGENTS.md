# AGENTS.md — AI 协作仓库地图

> 本文件是给 AI 编码助手（Claude Code、Codex、Cursor、TRAE、DSH 等）看的**仓库导览与硬约束**。人类贡献者请读 [CONTRIBUTING.md](./CONTRIBUTING.md)；领域概念请读 [docs/CONTEXT.md](./docs/CONTEXT.md)。
> 方法借鉴自 juejin-cn/juejin-usage 的做法：把 onboarding 知识显式写成 AI 可读的契约，而不是留在散落的 commit 历史里。

## 仓库一页图

| 路径 | 职责 | 绝不做什么 |
|---|---|---|
| `crates/agenthub-core` | 唯一业务引擎：能力注册表（SQLite）、适配器、事务化分发、验证平台 | 不含 UI / Tauri 代码 |
| `crates/agenthub-cli` | `agenthub` CLI，薄前端，与 GUI 共用同一个 `AgentHub` 服务 | 不绕过 `service.rs` 直接写 Agent 目录 |
| `src-tauri/src/lib.rs` | Tauri 命令层（约 48 个 `#[tauri::command]`），只做参数转换 | 不含业务规则 |
| `src/**` | Vue 3 前端（11 路由 / 12 视图，Pinia + vue-i18n） | 不直接调用 Rust，一律走命令层契约 |
| `docs/**` | PRD / 架构 / 数据模型 / 验证证据，以代码为准维护 | 不写未实现的「已验证」结论 |
| `skillforge/`、`xskill-are/`、`AgentHub-v3.1/` | 参考资料与历史归档 | 不作为运行时依赖引用 |

## 硬约束（违反 = PR 必须改）

1. **写入只走事务**：对真实 Agent 目录的一切写入只能经 `ops.rs` 的 Executor（plan → preflight → snapshot → apply → verify → record）。`adapters/` 只描述「在哪、支持什么」。
2. **单引擎双前端**：GUI 与 CLI 必须行为一致。改 `service.rs` 时检查 CLI 命令与 Tauri 命令两边是否都需要同步。
3. **错误走统一契约**：新错误优先复用 `error.rs` 既有变体；对外 JSON 用 `contract.rs` 的 `ApiResult<T>` 信封（v3.1）。
4. **跨进程互斥**：新的**修改类**操作（写库 / 写 Agent 目录）必须持有 `runtime.rs` 的 runtime lock；只读操作禁止加锁。
5. **本地优先**：不新增任何网络出站（唯一例外是用户显式触发的 `git clone`）、不新增遥测、不收集 prompt / API Key。

## 常用命令

```bash
pnpm install                        # 前端依赖
pnpm test                           # 前端 vitest
pnpm build                          # vue-tsc + vite build
cargo test -p agenthub-core         # 核心引擎测试（约 113 个）
cargo build -p agenthub-cli         # CLI
pnpm tauri build                    # 完整桌面构建
```

> 本机若 `rustc` 解析到 Homebrew 1.87（过旧），用 rustup shim 构建：
> `PATH="$HOME/.cargo/bin:$PATH" cargo test`。

## 开发工作流约定

- **spec → plan → 实现**：跨模块改动先在 PR 描述或 `docs/handoffs/` 写设计要点与计划，再动代码。
- **测试即验收**：每个行为改动附带 Rust 测试（core）或 vitest（前端）；修复 bug 时先写复现测试。
- **文档随代码走**：`docs/ARCHITECTURE.md` 标注「已验证 / 未验证 / 未实现」，改动落地后更新对应结论，禁止超前宣称。
- Commit 用 Conventional Commits（`feat(core): ...`），scope 取 `core|cli|gui|docs|adapters`。
