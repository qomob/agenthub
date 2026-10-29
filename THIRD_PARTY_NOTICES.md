# THIRD_PARTY_NOTICES.md — 第三方许可声明

> 基准日期：2026-10-29。范围：AgentHub 仓库随源码发布的内容（`crates/`、`src/`、`src-tauri/`、`src/` 前端、`docs/`、`scripts/`）。
> 本文件随发布范围变化需同步更新。

## 1. 本仓库许可

AgentHub 以 **MIT License** 发布（2026-10-29 版权所有者确认）。见根目录 `LICENSE`、`Cargo.toml` 的 `workspace.package.license = "MIT"`、`package.json` 的 `"license": "MIT"`。仓库：https://github.com/qomob/agenthub 。

## 2. 已知依赖许可大类

### 2.1 npm 侧（`package.json`）

| 依赖 | 许可 | 备注 |
|---|---|---|
| `lightningcss`（含其传递依赖，如 `@parcel/*` 相关绑定） | **MPL-2.0** | 随 `@tailwindcss/vite` / Tailwind 4 工具链引入，仅构建期使用 |
| `glob` 系列（`glob`、`minimatch` 等传递依赖） | **BlueOak-1.0.0**（部分文件为 ISC/MIT） | isaacs 系包当前惯例许可 |
| `config-chain` | **MIT**（已确认 2026-10-29：`package.json` license 指向 dominictarr/config-chain 的 LICENCE；LICENCE 原文 `Copyright (c) 2011 Dominic Tarr`） | 宽松许可，可随依赖分发 |

其余 npm 依赖（Vue、Pinia、vue-router、vue-i18n、Tauri API/CLI、Vite、Vitest、TypeScript、jsdom 等）按各自 package.json 标注，以 MIT 为主。发布前建议导出完整清单：

```bash
pnpm licenses list > docs/THIRD_PARTY_NPM.txt   # 待执行，需人工复核
```

### 2.2 Rust 侧（`crates/`、`src-tauri/`）

Rust 生态惯例：绝大多数 crates 采用 **MIT OR Apache-2.0 双许可**（serde、tokio、rusqlite、toml_edit、clap 等）。发布前建议用 `cargo about` 或 `cargo license` 生成完整清单并逐项复核（本次未执行）。

## 3. 仓库内存在但不随源码发布的参考目录

以下目录在本仓库工作区中存在，但**不入库、不随开源源码发布**（已由 `.gitignore` 覆盖，发布打包时必须排除）：

| 目录 | 许可情况 | 处置 |
|---|---|---|
| `skillforge/` | **PolyForm Noncommercial 1.0.0**（`skillforge/LICENSE`） | 非商业许可，与 MIT 主仓库不兼容，不纳入发布范围；仅作本地参考 |
| `xskill-are/` | **无 LICENSE 文件；README 徽章自称 MIT 但无许可文本**（2026-10-29 复核确认） | 无许可文本即默认保留所有权利，不纳入发布范围、不引用其代码；如未来需要引用，须先取得作者书面许可或补齐 MIT 声明 |
| `skill-compiler/` | **MIT（Copyright (c) 2026 qomob）** | 虽为 MIT，但属本地参考副本，不作为 AgentHub 发布内容；如未来引用其代码，需按 MIT 保留版权声明 |
| `AgentHub-v3.1/` | 无独立 LICENSE（历史副本） | 本项目的历史版本归档，不纳入发布范围 |

## 4. 结论

- 随源码发布的部分（`crates/`、`src/`、`src-tauri/`、`docs/`、`scripts/`）不包含上述参考目录的代码。
- 若未来从这些目录移植代码进主仓库，必须先解决许可兼容性（PolyForm 不可移植；MIT 移植需保留声明；无 LICENSE 的需取得授权）。
