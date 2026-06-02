use crate::bundle::{ImpactCriticality, ReleaseGatePolicy, RunEntry, RunSummary, SchemaIssue};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViolationGroup {
    pub key: String,
    pub endpoint_label: String,
    pub criticality: ImpactCriticality,
    pub release_gate: ReleaseGatePolicy,
    pub missing_items: Option<usize>,
    pub total_items: Option<usize>,
    pub child_field_count: usize,
}

pub fn build_violation_groups(summary: &RunSummary) -> Vec<ViolationGroup> {
    let mut groups: BTreeMap<(String, String), ViolationGroup> = BTreeMap::new();
    let mut child_paths: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for entry in &summary.results {
        if entry.release_gate_policy() == ReleaseGatePolicy::Ignore {
            continue;
        }
        let endpoint_label = format!("{} {}", entry.method, short_url(&entry.url));
        let criticality = entry.endpoint_criticality();
        let release_gate = entry.release_gate_policy();

        for issue in &entry.violations {
            let group_key = violation_group_key(&issue.path);
            let map_key = (entry.endpoint_id.clone(), group_key.clone());
            let group = groups.entry(map_key.clone()).or_insert_with(|| ViolationGroup {
                key: group_key.clone(),
                endpoint_label: endpoint_label.clone(),
                criticality,
                release_gate,
                missing_items: None,
                total_items: None,
                child_field_count: 0,
            });

            if issue.path != group_key && issue.path.starts_with(&format!("{group_key}.")) {
                child_paths
                    .entry(map_key.clone())
                    .or_default()
                    .insert(issue.path.clone());
            }

            if let Some((missing, total)) = parse_array_missing(&issue.message) {
                let replace = match (group.missing_items, group.total_items) {
                    (Some(current_missing), Some(current_total)) => {
                        missing > current_missing
                            || (missing == current_missing && total > current_total)
                    }
                    _ => true,
                };
                if replace {
                    group.missing_items = Some(missing);
                    group.total_items = Some(total);
                }
            }
        }
    }

    let mut out: Vec<ViolationGroup> = groups
        .into_iter()
        .map(|(map_key, mut group)| {
            group.child_field_count = child_paths.get(&map_key).map(|paths| paths.len()).unwrap_or(0);
            group
        })
        .collect();

    out.sort_by(|a, b| {
        b.criticality
            .cmp(&a.criticality)
            .then_with(|| b.release_gate.cmp(&a.release_gate))
            .then_with(|| {
                b.missing_items
                    .unwrap_or(0)
                    .cmp(&a.missing_items.unwrap_or(0))
            })
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// Group array-item violations under `prefix[].field` (e.g. `results[].objectProject`).
pub fn violation_group_key(path: &str) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return path.to_string();
    }
    if let Some(array_idx) = parts.iter().position(|part| part.ends_with("[]")) {
        if array_idx + 1 >= parts.len() {
            return path.to_string();
        }
        return format!("{}.{}", parts[array_idx], parts[array_idx + 1]);
    }
    path.to_string()
}

pub fn parse_array_missing(message: &str) -> Option<(usize, usize)> {
    let marker = " of ";
    let start = message.find('(')?;
    let inner = message.get(start + 1..)?.trim_end_matches(')');
    let of_idx = inner.find(marker)?;
    let missing_str = inner[..of_idx].trim();
    let rest = inner[of_idx + marker.len()..].trim();
    let total_str = rest.split_whitespace().next()?;
    let missing: usize = missing_str.parse().ok()?;
    let total: usize = total_str.parse().ok()?;
    Some((missing, total))
}

pub fn print_deployment_summary(summary: &RunSummary) {
    let groups = build_violation_groups(summary);
    let blocker_groups: Vec<_> = groups
        .iter()
        .filter(|group| group.release_gate == ReleaseGatePolicy::Blocker)
        .collect();
    let warning_groups: Vec<_> = groups
        .iter()
        .filter(|group| group.release_gate == ReleaseGatePolicy::Warning)
        .collect();

    eprintln!();
    if !blocker_groups.is_empty() {
        eprintln!(
            "❌ HOLD DEPLOYMENT — {} schema violation{} on blocker endpoints",
            blocker_groups.len(),
            if blocker_groups.len() == 1 { "" } else { "s" }
        );
        eprintln!();
        for group in &blocker_groups {
            eprintln!("{}", format_group_line(group));
        }
    } else if !warning_groups.is_empty() {
        eprintln!(
            "⚠️ REVIEW DEPLOYMENT — {} schema violation{} on warning-grade endpoints",
            warning_groups.len(),
            if warning_groups.len() == 1 { "" } else { "s" }
        );
        eprintln!();
        for group in &warning_groups {
            eprintln!("{}", format_group_line(group));
        }
    } else if summary.blocker_failures() > 0 {
        eprintln!("❌ HOLD DEPLOYMENT — blocker endpoint failures detected (no structured schema violations)");
    } else if summary.warning_failures() > 0 {
        eprintln!("⚠️ REVIEW DEPLOYMENT — warning-grade endpoint failures detected (no structured schema violations)");
    } else if summary.broken > 0 || summary.errors > 0 {
        eprintln!("✅ CLEAR FOR DEPLOYMENT — issues only on ignored endpoints");
    } else {
        eprintln!("✅ CLEAR FOR DEPLOYMENT — all endpoints passed schema checks");
        return;
    }

    if !groups.is_empty() {
        eprintln!();
        eprintln!("── Full violation details ──────────────────────────");
        print_violation_details(summary);
    }
}

fn format_group_line(group: &ViolationGroup) -> String {
    let mut suffix = String::new();
    if let (Some(missing), Some(total)) = (group.missing_items, group.total_items) {
        suffix.push_str(&format!(" ({missing}/{total} items missing"));
        if group.child_field_count > 0 {
            suffix.push_str(&format!(", +{} child fields", group.child_field_count));
        }
        suffix.push(')');
    } else if group.child_field_count > 0 {
        suffix.push_str(&format!(" (+{} child fields)", group.child_field_count));
    }

    format!(
        "{} {:<8} {} — {}{suffix}",
        group.criticality.icon(),
        group.criticality.label(),
        group.key,
        group.endpoint_label,
    )
}

fn print_violation_details(summary: &RunSummary) {
    for entry in &summary.results {
        if entry.release_gate_policy() == ReleaseGatePolicy::Ignore {
            continue;
        }
        if entry.violations.is_empty() && entry.breaking_changes.is_empty() {
            continue;
        }
        eprintln!();
        eprintln!(
            "  [{}] {} {} — {} ({})",
            entry.status,
            entry.method,
            entry.url,
            format_status_code(entry),
            format_entry_context(entry)
        );
        for issue in &entry.violations {
            eprintln!("    violation: {}: {}", issue.code, issue.message);
        }
        if entry.violations.is_empty() {
            for change in &entry.breaking_changes {
                eprintln!("    break: {change}");
            }
        }
        for issue in &entry.warnings {
            eprintln!("    warning: {}: {}", issue.code, issue.message);
        }
    }
    eprintln!();
}

fn format_entry_context(entry: &RunEntry) -> String {
    let criticality = entry.endpoint_criticality().label();
    let gate = entry.release_gate_policy().as_str();
    format!("{criticality} criticality, {gate} gate")
}

fn format_status_code(entry: &RunEntry) -> String {
    entry
        .status_code
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".into())
}

fn short_url(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{RunEntry, RunSummary, SchemaIssue};

    fn issue(path: &str, message: &str) -> SchemaIssue {
        SchemaIssue {
            code: "MISSING_REQUIRED_FIELD".into(),
            path: path.into(),
            expected: "string".into(),
            actual: "missing".into(),
            message: message.into(),
            severity: "high".into(),
        }
    }

    fn entry(id: &str, criticality: &str, gate: &str, violations: Vec<SchemaIssue>) -> RunEntry {
        RunEntry {
            endpoint_id: id.into(),
            method: "POST".into(),
            url: "https://api.example.com/search".into(),
            status: "broken".into(),
            status_code: Some(200),
            breaking_changes: vec![],
            violations,
            warnings: vec![],
            error: None,
            response_preview: None,
            impact_criticality: Some(criticality.into()),
            effective_release_gate: gate.into(),
        }
    }

    fn summary_with(entries: Vec<RunEntry>) -> RunSummary {
        RunSummary {
            session_id: "sid".into(),
            session_name: "Test".into(),
            run_at: "now".into(),
            total: entries.len(),
            ok: 0,
            broken: entries.len(),
            errors: 0,
            results: entries,
        }
    }

    #[test]
    fn uses_endpoint_criticality_not_missing_counts() {
        let summary = summary_with(vec![entry(
            "ep1",
            "medium",
            "warning",
            vec![issue(
                "results[].userAssignedTo.username",
                "Field removed: results[].userAssignedTo.username (2 of 20 array elements are missing this field: items #3, #14)",
            )],
        )]);
        let groups = build_violation_groups(&summary);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].criticality, ImpactCriticality::Medium);
        assert_eq!(groups[0].release_gate, ReleaseGatePolicy::Warning);
    }

    #[test]
    fn groups_by_endpoint_and_field() {
        let violations = [
            issue(
                "results[].objectProject.createdAt",
                "Field removed: results[].objectProject.createdAt (19 of 20 array elements are missing this field: items #0, #1)",
            ),
            issue(
                "results[].objectProject.updatedAt",
                "Field removed: results[].objectProject.updatedAt (19 of 20 array elements are missing this field: items #0, #1)",
            ),
            issue(
                "results[].requiresSignature",
                "Field removed: results[].requiresSignature (17 of 20 array elements are missing this field: items #0, #1)",
            ),
        ];
        let summary = summary_with(vec![entry("ep1", "high", "blocker", violations.to_vec())]);
        let groups = build_violation_groups(&summary);
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.criticality == ImpactCriticality::High));
        assert!(groups.iter().all(|g| g.release_gate == ReleaseGatePolicy::Blocker));
    }

    #[test]
    fn parses_array_missing_counts() {
        assert_eq!(
            parse_array_missing(
                "Field removed: results[].objectProject.createdAt (19 of 20 array elements are missing this field: items #0, #1)"
            ),
            Some((19, 20))
        );
    }
}
