//! Capability health derivation from audits, dependency reports and evidence.
//!
//! The derivation is intentionally conservative: being installed is never
//! treated as being verified. Only manual `passed` evidence for *every*
//! installed agent on the *current* version reaches `HealthyState::Verified`.

use std::collections::BTreeSet;

use crate::model::{CapabilityAudit, DependencyReport, HealthState, ValidationRun};

/// Derive a capability health state from the latest deterministic evidence.
/// Manual validation evidence is per-agent and is the only way to reach
/// `verified` for closed desktop agents.
///
/// Version filtering: when `audit` or `deps` exposes a current version id, only
/// validation runs with the same `version_id` are considered. Without any
/// version evidence all supplied runs are considered (the caller is expected to
/// pass runs for the current version). Runs for older versions are ignored.
pub fn derive_health(
    audit: Option<&CapabilityAudit>,
    deps: Option<&DependencyReport>,
    validations: &[ValidationRun],
    installed_agents: &[String],
) -> (HealthState, Vec<String>) {
    let installed = installed_agents
        .iter()
        .map(|agent| agent.trim().to_string())
        .filter(|agent| !agent.is_empty())
        .collect::<Vec<String>>();
    let mut seen = BTreeSet::new();
    let installed: Vec<String> = installed
        .into_iter()
        .filter(|agent| seen.insert(agent.clone()))
        .collect();

    if installed.is_empty() {
        return (
            HealthState::NotDeployed,
            vec!["尚未分发到任何 Agent".to_string()],
        );
    }

    let current_version = audit
        .map(|audit| audit.version_id.as_str())
        .filter(|version| !version.trim().is_empty())
        .or_else(|| {
            deps.map(|deps| deps.version_id.as_str())
                .filter(|version| !version.trim().is_empty())
        });

    let relevant: Vec<&ValidationRun> = match current_version {
        Some(version) => validations
            .iter()
            .filter(|run| run.version_id == version)
            .collect(),
        None => validations.iter().collect(),
    };

    // 1. Any failed run for the current version is fatal evidence.
    let failed: Vec<&ValidationRun> = relevant
        .iter()
        .copied()
        .filter(|run| run.status == "failed")
        .collect();
    if !failed.is_empty() {
        let mut reasons = vec![format!(
            "当前版本存在 {} 条失败的验收记录",
            failed.len()
        )];
        for run in failed.iter().take(10) {
            reasons.push(format!(
                "Agent {} 的验收用例 '{}' 状态为 failed",
                agent_identity(run),
                case_name(run)
            ));
        }
        return (HealthState::Failed, reasons);
    }

    // 2. Security/environment problems always need attention, even when the
    //    feature was manually verified: passing a task does not make a skill
    //    safe and does not install missing runtimes.
    if let Some(audit) = audit {
        if audit.status == "fail" {
            return (
                HealthState::NeedsAttention,
                vec![format!("静态审计失败: {}", audit.summary)],
            );
        }
    }
    if let Some(deps) = deps {
        if deps.status == "needs_attention" {
            return (
                HealthState::NeedsAttention,
                vec![format!("环境检查需要关注: {}", deps.summary)],
            );
        }
    }

    // 3. Verified requires manual `passed` evidence for every installed agent.
    let passed_agents: BTreeSet<String> = relevant
        .iter()
        .filter(|run| run.status == "passed" && run.kind == "manual")
        .filter_map(|run| agent_identity_opt(run))
        .collect();
    let missing_agents: Vec<&String> = installed
        .iter()
        .filter(|agent| !passed_agents.contains(agent.as_str()))
        .collect();

    if missing_agents.is_empty() {
        return (
            HealthState::Verified,
            vec![format!(
                "所有 {} 个安装 Agent 均通过当前版本手动验收: {}",
                installed.len(),
                installed.join(", ")
            )],
        );
    }

    // 4. Partial manual evidence is still unverified, but state exactly who is
    //    verified and who is missing so the UI can guide the user.
    if !passed_agents.is_empty() {
        let verified_installed: Vec<&String> = installed
            .iter()
            .filter(|agent| passed_agents.contains(agent.as_str()))
            .collect();
        let missing: Vec<&String> = missing_agents;
        let mut reasons = vec![format!(
            "已通过当前版本手动验收的 Agent: {}",
            verified_installed
                .iter()
                .map(|agent| agent.as_str())
                .collect::<Vec<&str>>()
                .join(", ")
        )];
        reasons.push(format!(
            "尚未通过当前版本手动验收的 Agent: {}",
            missing
                .iter()
                .map(|agent| agent.as_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ));
        return (HealthState::Unverified, reasons);
    }

    // 5. No manual pass evidence at all. Static health alone stays unverified.
    let mut reasons = Vec::new();
    match audit {
        Some(audit) => reasons.push(format!(
            "仅静态检查通过（状态 '{}'），缺少端到端验证证据",
            audit.status
        )),
        None => reasons.push("缺少静态审计结果，也缺少端到端验证证据".to_string()),
    }
    if let Some(deps) = deps {
        reasons.push(format!("环境检查状态为 '{}'", deps.status));
    }
    reasons.push(format!(
        "已安装但尚未通过当前版本手动验收的 Agent: {}",
        installed.join(", ")
    ));
    (HealthState::Unverified, reasons)
}

fn agent_identity(run: &ValidationRun) -> String {
    agent_identity_opt(run).unwrap_or_else(|| "未知 Agent".to_string())
}

fn agent_identity_opt(run: &ValidationRun) -> Option<String> {
    for candidate in [
        run.agent_key.as_deref(),
        run.agent_name.as_deref(),
        run.agent_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn case_name(run: &ValidationRun) -> &str {
    let name = run.case_name.trim();
    if name.is_empty() {
        "(unnamed case)"
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(status: &str, version_id: &str, summary: &str) -> CapabilityAudit {
        CapabilityAudit {
            id: "audit-1".to_string(),
            capability_id: "cap-1".to_string(),
            version_id: version_id.to_string(),
            version_label: "1.0.0".to_string(),
            kind: "static".to_string(),
            status: status.to_string(),
            risk: if status == "fail" { "high" } else { "low" }.to_string(),
            summary: summary.to_string(),
            findings: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn deps(status: &str, version_id: &str) -> DependencyReport {
        DependencyReport {
            capability_id: "cap-1".to_string(),
            version_id: version_id.to_string(),
            status: status.to_string(),
            risk: "low".to_string(),
            summary: "runtime check".to_string(),
            items: Vec::new(),
            checked_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn run(
        version_id: &str,
        agent: Option<&str>,
        kind: &str,
        status: &str,
        case: &str,
    ) -> ValidationRun {
        ValidationRun {
            id: format!("run-{version_id}-{agent:?}-{case}-{status}"),
            capability_id: "cap-1".to_string(),
            version_id: version_id.to_string(),
            version_label: "1.0.0".to_string(),
            agent_id: agent.map(|agent| format!("agent-{agent}")),
            agent_key: agent.map(str::to_string),
            agent_name: agent.map(|agent| format!("{agent} app")),
            kind: kind.to_string(),
            status: status.to_string(),
            case_name: case.to_string(),
            notes: String::new(),
            evidence: String::new(),
            actor: "tester".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn installed(agents: &[&str]) -> Vec<String> {
        agents.iter().map(|agent| agent.to_string()).collect()
    }

    #[test]
    fn no_installations_is_not_deployed() {
        let (state, reasons) = derive_health(None, None, &[], &[]);
        assert_eq!(state, HealthState::NotDeployed);
        assert!(reasons[0].contains("尚未分发"));
    }

    #[test]
    fn failed_current_version_wins_over_audit_failure() {
        let audit = audit("fail", "v-current", "missing checks");
        let validations = vec![run(
            "v-current",
            Some("codex"),
            "manual",
            "failed",
            "open-project",
        )];
        let (state, reasons) = derive_health(Some(&audit), None, &validations, &installed(&["codex"]));
        assert_eq!(state, HealthState::Failed);
        assert!(reasons.iter().any(|reason| reason.contains("codex")));
        assert!(reasons.iter().any(|reason| reason.contains("open-project")));
    }

    #[test]
    fn stale_version_failure_is_ignored() {
        let audit = audit("pass", "v-current", "ok");
        let validations = vec![
            run("v-old", Some("codex"), "manual", "failed", "old-case"),
            run("v-current", Some("codex"), "manual", "passed", "new-case"),
        ];
        let (state, _) = derive_health(Some(&audit), None, &validations, &installed(&["codex"]));
        assert_eq!(state, HealthState::Verified);
    }

    #[test]
    fn all_installed_agents_manually_passed_is_verified() {
        let audit = audit("pass", "v-current", "ok");
        let deps = deps("healthy", "v-current");
        let validations = vec![
            run("v-current", Some("codex"), "manual", "passed", "case-1"),
            run("v-current", Some("zcode"), "manual", "passed", "case-2"),
        ];
        let (state, reasons) = derive_health(
            Some(&audit),
            Some(&deps),
            &validations,
            &installed(&["codex", "zcode"]),
        );
        assert_eq!(state, HealthState::Verified);
        assert!(reasons[0].contains("codex"));
        assert!(reasons[0].contains("zcode"));
    }

    #[test]
    fn partial_manual_evidence_is_unverified_with_both_lists() {
        let audit = audit("pass", "v-current", "ok");
        let validations = vec![run("v-current", Some("codex"), "manual", "passed", "case-1")];
        let (state, reasons) = derive_health(
            Some(&audit),
            None,
            &validations,
            &installed(&["codex", "zcode"]),
        );
        assert_eq!(state, HealthState::Unverified);
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("已通过") && reason.contains("codex")));
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("尚未") && reason.contains("zcode")));
    }

    #[test]
    fn installed_without_evidence_is_unverified_never_verified() {
        let audit = audit("pass", "v-current", "ok");
        let (state, reasons) = derive_health(
            Some(&audit),
            None,
            &[],
            &installed(&["codex"]),
        );
        assert_eq!(state, HealthState::Unverified);
        assert!(reasons.iter().any(|reason| reason.contains("缺少端到端验证证据")));
        assert!(reasons.iter().any(|reason| reason.contains("codex")));
    }

    #[test]
    fn audit_failure_is_needs_attention() {
        let audit = audit("fail", "v-current", "critical shell script");
        let (state, reasons) = derive_health(Some(&audit), None, &[], &installed(&["codex"]));
        assert_eq!(state, HealthState::NeedsAttention);
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("静态审计失败") && reason.contains("critical shell script")));
    }

    #[test]
    fn dependency_needs_attention_is_needs_attention() {
        let audit = audit("pass", "v-current", "ok");
        let deps = deps("needs_attention", "v-current");
        let (state, reasons) = derive_health(
            Some(&audit),
            Some(&deps),
            &[],
            &installed(&["codex"]),
        );
        assert_eq!(state, HealthState::NeedsAttention);
        assert!(reasons.iter().any(|reason| reason.contains("环境检查")));
    }

    #[test]
    fn missing_audit_is_unverified() {
        let (state, reasons) = derive_health(None, None, &[], &installed(&["codex"]));
        assert_eq!(state, HealthState::Unverified);
        assert!(reasons.iter().any(|reason| reason.contains("缺少静态审计结果")));
    }

    #[test]
    fn non_manual_pass_does_not_verify() {
        let audit = audit("pass", "v-current", "ok");
        let validations = vec![run(
            "v-current",
            Some("codex"),
            "static",
            "passed",
            "smoke",
        )];
        let (state, _) = derive_health(
            Some(&audit),
            None,
            &validations,
            &installed(&["codex"]),
        );
        assert_eq!(state, HealthState::Unverified);
    }

    #[test]
    fn uses_deps_version_when_audit_is_absent() {
        let deps = deps("healthy", "v-current");
        let validations = vec![
            run("v-old", Some("codex"), "manual", "failed", "old"),
            run("v-current", Some("codex"), "manual", "passed", "current"),
        ];
        let (state, _) = derive_health(None, Some(&deps), &validations, &installed(&["codex"]));
        assert_eq!(state, HealthState::Verified);
    }

    #[test]
    fn duplicate_installed_agents_are_deduplicated() {
        let audit = audit("pass", "v-current", "ok");
        let validations = vec![run("v-current", Some("codex"), "manual", "passed", "case")];
        let (state, reasons) = derive_health(
            Some(&audit),
            None,
            &validations,
            &installed(&["codex", "codex", " "]),
        );
        assert_eq!(state, HealthState::Verified);
        assert!(reasons[0].contains("1 个安装 Agent"));
    }
    #[test]
    fn manual_pass_cannot_override_a_failed_audit() {
        let audit = audit("fail", "v-current", "hardcoded secret");
        let validations = vec![run("v-current", Some("codex"), "manual", "passed", "case")];
        let (state, reasons) = derive_health(
            Some(&audit),
            None,
            &validations,
            &installed(&["codex"]),
        );
        assert_eq!(state, HealthState::NeedsAttention);
        assert!(reasons.iter().any(|reason| reason.contains("静态审计失败")));
    }
}
