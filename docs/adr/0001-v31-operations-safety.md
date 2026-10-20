# ADR-0001（实现仓库）: 在现有 0.4.0 应用内增量实现 v3.1 Phase 1 安全切片

- Status: Accepted — 2026-10-20
- Context：`AgentHub-v3.1/AgentHub-v3.1` 规范包（含其 `docs/adr/0001-audited-baseline.md`）是唯一开发基线。Phase 0 审计确认本机已有可运行的 AgentHub 0.4.0（98 Rust + 38 前端测试通过、真实用户数据 111 skills），且 v3.1 目标 schema 与旧 schema 断代。规范包内的新 workspace 在并行审计期间被另一进程初始化，无法独占；同时 Phase 1 Gate 需要尽快交付可运行、可测试、可恢复的真实功能。
- Decision：
  1. **实现仓库**为现有 0.4.0 仓库；不重建 UI/适配器，采用“增量迁移 + 端口前移”。
  2. 数据模型采用**加法迁移**（`user_version` 2→3）：保留 0.4.0 的 18 张表与全部用户数据，新增 `operation_steps`、`adapter_evidence`、`audit_events`、`schema_migrations` 与 `operations` 审批列；迁移前自动 `VACUUM INTO` 备份并校验 integrity/FK。
  3. Phase 1 优先交付 **F-07 安全核心**（plan hash、approval token、expiry、CAS、journal、recover、idempotency）与 **F-06 证据模型**，而不是先做完整 34 表/内容寻址。
  4. 旧 48 个 Tauri 命令不一次性改签名；新增命令统一 `ApiResult`，旧命令按切片逐步迁移。
  5. 规范包中的新 workspace 决策（其 ADR-0001 D-A）**不在本仓库执行**，保留为后续可选；本 ADR 记录差异与回滚路径。
- Consequences：
  - 用户现有 111 个 Skill 与 129 条历史操作不被破坏；升级可回退（迁移前备份）。
  - 与规范包“34 表一次性 baseline”存在差异；`TRACEABILITY` 中 F-02 标 IN_PROGRESS，不标 VERIFIED。
  - 内容寻址 `blobs/<sha256>`、SQLx、新目录结构推迟到后续切片/阶段，需另立 ADR。
  - 三引擎复用授权见规范包 ADR-0002（用户本人开发）；Phase 1 不调用引擎。
- Migration / rollback：迁移事务 + 迁移前备份；失败不提交；回滚=恢复备份 + 回退代码 commit。
- Test impact：新增 `tests/v31_operations.rs`（9 个用例）；现有 98 测试作为回归基线；CLI/前端契约测试随切片补充。
- Review：用户可在 Phase 1 Gate 后决定是否切换到规范包的新 workspace；届时需迁移本仓库已验证的 contract/journal/recover 实现。
