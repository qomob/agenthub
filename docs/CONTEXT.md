# Domain Context — 领域术语表

> 目的：把容易扯皮的模糊概念显式定义成「术语 + 规则/不变量」，供人类与 AI 协作者共用同一语言。新增领域概念时先补表再写代码。

## Capability（能力）

- **Definition**: Agent 侧可安装/可分发的东西的统称，当前包括 **Skill**、**MCP 定义**、**插件清单**。
- **Rules/Invariants**:
  - 每个能力在中央库内有唯一 `id`（slug）与**多版本**存储。
  - 能力的事实记录在 SQLite（capability / version / installation 三类行），文件系统只是载体。

## Central Library（中央库）

- **Definition**: AgentHub 自有的能力存储区，位于 `<library_root>/library/skills/...`，附 snapshots。
- **Rules/Invariants**:
  - 中央库是**唯一事实源**；各 Agent 目录里的是「部署副本」。
  - 卸载部署不删除中央库版本（保留最近版本用于回滚）。

## Agent / Adapter（Agent 与适配器）

- **Definition**: Agent 是本机安装的 AI 编码工具（Codex、TRAE、ZCode、WorkBuddy、DSH…）；Adapter 是它对 AgentHub 的投影。
- **Rules/Invariants**:
  - Adapter 只回答「路径在哪、检测状态、支持什么能力」，**绝不执行写入**；写入只由 `ops.rs` Executor 完成。
  - 新增 Agent = 新增/扩展 adapter 声明，不新建写入路径。

## Installation vs Deployment（登记 vs 部署）

- **Definition**: Installation 是「某能力部署在某 Agent 上」这条记录；Deployment（分发）是产生/移除该记录的写动作。
- **Rules/Invariants**:
  - 部署必须先有 Operation（计划）与人工批准（v3.1），不允许隐式写入。
  - 外部修改检测：目标与记录的 hash 不一致时报 `ExternalModification`，不静默覆盖。

## Operation（操作 / 事务）

- **Definition**: 一次变更的完整生命周期：`plan → preview → approve → apply → verify → record`，失败逆序补偿。
- **Rules/Invariants**:
  - `plan_hash` 绑定计划内容；apply 前重算，不一致即拒绝（`ExternalModification`）。
  - 批准是一次性 approval token（15 分钟过期），不可重放。
  - 修改类 Operation 全程持有跨进程 runtime lock（`~/.agenthub/runtime.lock`，flock 实现，进程死亡自动释放）；只读操作不加锁。同一时刻只允许一个进程执行修改。

## HealthState（验证健康状态）

- **Definition**: 由 audit（静态审计）/ deps（依赖诊断）/ impact（更新影响）/ validations（人工证据）推导出的能力健康结论。
- **Rules/Invariants**:
  - 静态审计是规则引擎，**不是完备安全证明**；不执行脚本、不联网、不做 AI 评分。
  - 系统不自动判定「passed」；人工验证必须由用户显式写入证据。

## Local First（本地优先）

- **Definition**: 数据只在本机：SQLite + 本地文件 + macOS Keychain。
- **Rules/Invariants**:
  - 无账号、无云端、无遥测；唯一出站是用户显式触发的 `git clone`。
  - 不收集 prompt、对话内容、API Key；敏感值优先入 Keychain。

## Runtime Lock（跨进程运行时锁）

- **Definition**: `<data_dir>/runtime.lock` 上的 advisory flock，内容为 `{pid, kind, acquiredAt}` 诊断信息。
- **Rules/Invariants**:
  - 仅修改类入口（apply / rollback / backup / restore）持有，操作结束即释放。
  - OS 保证进程死亡自动解锁，**无陈旧锁文件需要清理**。
  - 忙时返回 `Blocked` 错误并指明持有者（kind + pid），绝不排队等待。
