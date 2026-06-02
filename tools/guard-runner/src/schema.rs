use crate::bundle::SchemaIssue;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type SchemaSnapshot = BTreeMap<String, String>;

const PREFERRED_ARRAY_KEYS: &[&str] = &["results", "items", "data", "records", "entries", "rows"];

/// Canonical path notation: `results[].field`, never bare `[].field`.
pub fn normalize_schema_path(path: &str, baseline: &SchemaSnapshot, root: Option<&Value>) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || is_invalid_schema_path(trimmed) {
        return trimmed.to_string();
    }
    if trimmed.starts_with("[].") || trimmed == "[]" {
        return resolve_bare_array_path(trimmed, baseline, root);
    }
    trimmed.to_string()
}

pub fn normalize_snapshot(baseline: &SchemaSnapshot, root: &Value) -> SchemaSnapshot {
    let mut out = SchemaSnapshot::new();
    for (path, field_type) in baseline {
        if is_invalid_schema_path(path) {
            continue;
        }
        let normalized = normalize_schema_path(path, baseline, Some(root));
        out.entry(normalized).or_insert_with(|| field_type.clone());
    }
    out
}

pub fn normalize_path_set(
    paths: &BTreeSet<String>,
    baseline: &SchemaSnapshot,
    root: Option<&Value>,
) -> BTreeSet<String> {
    paths
        .iter()
        .map(|path| normalize_schema_path(path, baseline, root))
        .filter(|path| !is_invalid_schema_path(path))
        .collect()
}

fn resolve_bare_array_path(path: &str, baseline: &SchemaSnapshot, root: Option<&Value>) -> String {
    let remainder = path
        .strip_prefix("[].")
        .or_else(|| path.strip_prefix("[]"))
        .unwrap_or(path);
    let remainder = remainder.strip_prefix('.').unwrap_or(remainder);

    let mut candidates: Vec<String> = baseline
        .keys()
        .filter_map(|key| {
            let pos = key.find("[]")?;
            let prefix = &key[..pos];
            if prefix.is_empty() {
                return None;
            }
            if key.ends_with(&format!("[].{remainder}")) || **key == format!("{prefix}[].{remainder}") {
                return Some(format!("{prefix}[].{remainder}"));
            }
            None
        })
        .collect();
    candidates.sort();
    candidates.dedup();

    if candidates.len() == 1 {
        return candidates[0].clone();
    }
    if candidates.len() > 1 {
        for preferred in PREFERRED_ARRAY_KEYS {
            if let Some(found) = candidates.iter().find(|c| c.starts_with(&format!("{preferred}[]."))) {
                return found.clone();
            }
        }
        return candidates[0].clone();
    }

    if let Some(root) = root {
        if let Value::Object(map) = root {
            for preferred in PREFERRED_ARRAY_KEYS {
                if map.get(*preferred).and_then(|v| v.as_array()).is_some() {
                    return format!("{preferred}[].{remainder}");
                }
            }
            if let Some((key, _)) = map.iter().find(|(_, v)| v.is_array()) {
                return format!("{key}[].{remainder}");
            }
        }
        if root.is_array() {
            return format!("items[].{remainder}");
        }
    }

    format!("items[].{remainder}")
}

pub fn normalize_change_message(
    change: &str,
    baseline: &SchemaSnapshot,
    root: Option<&Value>,
) -> String {
    if let Some(rest) = change.strip_prefix("Field removed: ") {
        let (path_part, suffix) = split_change_path_and_suffix(rest);
        let normalized = normalize_schema_path(path_part, baseline, root);
        return if suffix.is_empty() {
            format!("Field removed: {normalized}")
        } else {
            format!("Field removed: {normalized} ({suffix})")
        };
    }
    if let Some(rest) = change.strip_prefix("Type changed: ") {
        let (path_part, suffix) = split_change_path_and_suffix(rest);
        let normalized = normalize_schema_path(path_part, baseline, root);
        return if suffix.is_empty() {
            format!("Type changed: {normalized}")
        } else {
            format!("Type changed: {normalized} ({suffix})")
        };
    }
    if let Some(rest) = change.strip_prefix("Value became null: ") {
        let (path_part, suffix) = split_change_path_and_suffix(rest);
        let normalized = normalize_schema_path(path_part, baseline, root);
        return if suffix.is_empty() {
            format!("Value became null: {normalized}")
        } else {
            format!("Value became null: {normalized} ({suffix})")
        };
    }
    change.to_string()
}

fn split_change_path_and_suffix(rest: &str) -> (&str, &str) {
    if let Some(open) = rest.find(" (") {
        (&rest[..open], rest[open + 2..].trim_end_matches(')'))
    } else {
        (rest, "")
    }
}

fn normalize_issue(
    mut issue: SchemaIssue,
    baseline: &SchemaSnapshot,
    root: Option<&Value>,
) -> SchemaIssue {
    issue.path = normalize_schema_path(&issue.path, baseline, root);
    issue.message = normalize_change_message(&issue.message, baseline, root);
    issue
}

pub fn extract_snapshot(value: &Value, prefix: &str, out: &mut SchemaSnapshot) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.insert(path.clone(), type_name(val));
                extract_snapshot(val, &path, out);
            }
        }
        Value::Array(items) => {
            let items_prefix = if prefix.is_empty() {
                "items[]".to_string()
            } else {
                format!("{prefix}[]")
            };
            for item in items.iter().take(5) {
                extract_snapshot(item, &items_prefix, out);
            }
        }
        _ => {}
    }
}

fn type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(n) => {
            if n.is_f64() || n.is_i64() || n.is_u64() {
                "number".to_string()
            } else {
                "unknown".to_string()
            }
        }
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

pub fn snapshot_from_json_bytes(data: &[u8]) -> Option<SchemaSnapshot> {
    let value: Value = serde_json::from_slice(data).ok()?;
    let mut out = SchemaSnapshot::new();
    extract_snapshot(&value, "", &mut out);
    if out.is_empty() { None } else { Some(out) }
}

#[allow(dead_code)]
pub fn breaking_changes(
    baseline: &SchemaSnapshot,
    current: &SchemaSnapshot,
    required: Option<&std::collections::BTreeSet<String>>,
) -> Vec<String> {
    let mut changes = Vec::new();
    for (path, base_type) in baseline {
        let is_required = required
            .map(|r| r.is_empty() || r.contains(path))
            .unwrap_or(true);
        if !is_required {
            continue;
        }
        match current.get(path) {
            Some(cur_type) => {
                if cur_type == "null" && base_type != "null" {
                    changes.push(format!(
                        "Value became null: {path} ({base_type} → null)"
                    ));
                } else if cur_type != base_type && base_type != "null" && cur_type != "null" {
                    changes.push(format!(
                        "Type changed: {path} ({base_type} → {cur_type})"
                    ));
                }
            }
            None => {
                changes.push(format!("Field removed: {path}"));
            }
        }
    }
    changes.sort();
    changes
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub violations: Vec<SchemaIssue>,
    pub warnings: Vec<SchemaIssue>,
}

#[derive(Debug, Clone)]
enum PathInspection {
    Present(String, Option<String>),
    Missing(Option<String>),
}

pub fn validate_response_contract(
    baseline: &SchemaSnapshot,
    required: Option<&BTreeSet<String>>,
    schema_reviewed: bool,
    suppressed_paths: &BTreeSet<String>,
    body: &[u8],
) -> Option<ValidationResult> {
    let root: Value = serde_json::from_slice(body).ok()?;
    let normalized_baseline = normalize_snapshot(baseline, &root);
    let mut current_snapshot = SchemaSnapshot::new();
    extract_snapshot(&root, "", &mut current_snapshot);
    let mut changes: Vec<String> = Vec::new();
    let normalized_required = required.map(|fields| normalize_path_set(fields, baseline, Some(&root)));
    let effective_required =
        effective_required_fields(&normalized_baseline, normalized_required.as_ref(), schema_reviewed);

    let mut baseline_paths: Vec<_> = normalized_baseline.iter().collect();
    baseline_paths.sort_by(|a, b| a.0.cmp(b.0));
    for (path, expected_type) in baseline_paths {
        if is_invalid_schema_path(path) {
            continue;
        }
        if !effective_required.contains(path.as_str()) {
            continue;
        }
        match inspect_path(path, expected_type, &root) {
            PathInspection::Present(current_type, detail) => {
                let detail_suffix = detail.map(|d| format!(", {d}")).unwrap_or_default();
                if current_type == "null" {
                    if expected_type == "null" {
                        changes.push(format!(
                            "Value became null: {path} (required field is null{detail_suffix})"
                        ));
                    } else {
                        changes.push(format!(
                            "Value became null: {path} ({expected_type} → null{detail_suffix})"
                        ));
                    }
                } else if current_type != *expected_type && expected_type != "null" && current_type != "null" {
                    changes.push(format!(
                        "Type changed: {path} ({expected_type} → {current_type}{detail_suffix})"
                    ));
                }
            }
            PathInspection::Missing(detail) => {
                if let Some(hint) =
                    rename_hint_for_removed_path(path, expected_type, &normalized_baseline, &current_snapshot)
                {
                    if let Some(detail) = detail.filter(|d| !d.is_empty()) {
                        changes.push(format!(
                            "Field removed: {path} ({detail}, possible rename: {hint})"
                        ));
                    } else {
                        changes.push(format!("Field removed: {path} (possible rename: {hint})"));
                    }
                } else if let Some(detail) = detail.filter(|d| !d.is_empty()) {
                    changes.push(format!("Field removed: {path} ({detail})"));
                } else {
                    changes.push(format!("Field removed: {path}"));
                }
            }
        }

        if changes.len() >= 30 {
            break;
        }
    }
    changes.sort();
    let violations: Vec<SchemaIssue> = changes
        .iter()
        .map(|change| {
            normalize_issue(
                issue_from_change(change, &normalized_baseline),
                &normalized_baseline,
                Some(&root),
            )
        })
        .collect();

    let mut warnings = Vec::new();
    if let Some(current) = snapshot_from_json_bytes(body) {
        let mut extra: Vec<_> = current
            .into_iter()
            .filter(|(path, _)| !is_declared_by_schema(path, &normalized_baseline))
            .collect();
        extra.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, actual_type) in extra {
            let normalized_path = normalize_schema_path(&path, &normalized_baseline, Some(&root));
            warnings.push(make_issue(
                "UNEXPECTED_FIELD",
                normalized_path.clone(),
                "not_defined".to_string(),
                actual_type.clone(),
                format!("{normalized_path} is present in response but not in predefined schema."),
                "warning",
            ));
        }
    }

    let mut result = ValidationResult { violations, warnings };
    let normalized_suppressed = normalize_path_set(suppressed_paths, baseline, Some(&root));
    if !normalized_suppressed.is_empty() {
        result.violations.retain(|issue| !is_suppressed(&issue.path, &normalized_suppressed));
        result.warnings.retain(|issue| !is_suppressed(&issue.path, &normalized_suppressed));
    }
    Some(result)
}

fn is_invalid_schema_path(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.is_empty() || trimmed == "." || trimmed == "$" || trimmed == "$."
}

pub fn enforcement_required_fields(
    baseline: &SchemaSnapshot,
    required: Option<&BTreeSet<String>>,
    schema_reviewed: bool,
    schema_declared_paths: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let working = effective_required_fields(baseline, required, schema_reviewed);
    let declared = schema_declared_paths
        .cloned()
        .unwrap_or_else(|| baseline.keys().cloned().collect());
    working
        .into_iter()
        .filter(|path| declared.contains(path))
        .collect()
}

pub fn effective_required_fields(
    baseline: &SchemaSnapshot,
    required: Option<&BTreeSet<String>>,
    schema_reviewed: bool,
) -> BTreeSet<String> {
    if let Some(fields) = required {
        return fields.clone();
    }
    if schema_reviewed {
        return baseline.keys().cloned().collect();
    }
    auto_classify_required_fields(baseline)
}

fn auto_classify_required_fields(baseline: &SchemaSnapshot) -> BTreeSet<String> {
    baseline
        .iter()
        .filter_map(|(path, field_type)| {
            is_likely_required_field(path, field_type).then(|| path.clone())
        })
        .collect()
}

fn is_likely_required_field(path: &str, field_type: &str) -> bool {
    if field_type == "null" {
        return false;
    }

    let segment = path.rsplit('.').next().unwrap_or(path);
    let lower = segment.trim_end_matches("[]").to_lowercase();
    let normalized_path = path.to_lowercase();
    let is_array_item_path = path.contains("[]");

    const OPTIONAL_PATH_HINTS: &[&str] = &[
        "topic_submissions",
        "breadcrumbs",
        "current_user_collections",
        "alternative_slugs",
        "sponsorship",
        "social",
    ];
    const OPTIONAL_LEAF_HINTS: &[&str] = &[
        "description", "note", "subtitle", "avatar", "image", "photo", "metadata", "extra",
        "detail", "bio", "comment", "label", "tag", "color", "icon", "url", "link", "href",
        "thumbnail", "cover", "location", "portfolio", "promoted", "breadcrumb",
    ];

    if OPTIONAL_PATH_HINTS
        .iter()
        .any(|hint| normalized_path.contains(hint))
    {
        return false;
    }
    if OPTIONAL_LEAF_HINTS.iter().any(|hint| lower.contains(hint)) {
        return false;
    }
    if field_type == "array" && !is_array_item_path {
        return false;
    }

    let required_leaf = matches!(
        lower.as_str(),
        "id" | "uuid"
            | "key"
            | "type"
            | "status"
            | "code"
            | "name"
            | "title"
            | "email"
            | "username"
            | "role"
            | "created_at"
            | "createdat"
            | "updated_at"
            | "updatedat"
            | "success"
            | "error"
            | "message"
            | "slug"
    ) || lower.ends_with("_id")
        || (lower.ends_with("_at") && !lower.contains("promoted"));

    if is_array_item_path {
        return required_leaf;
    }
    required_leaf
}

fn inspect_path(path: &str, baseline_type: &str, json: &Value) -> PathInspection {
    let parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();
    if parts.is_empty() {
        return PathInspection::Missing(None);
    }
    inspect_parts(&parts, baseline_type, json)
}

fn inspect_parts(parts: &[String], baseline_type: &str, value: &Value) -> PathInspection {
    if parts.is_empty() {
        return PathInspection::Present(type_name(value), None);
    }
    let part = &parts[0];
    if part.ends_with("[]") {
        let key = part.trim_end_matches("[]");
        let array_value = if key.is_empty() {
            value.as_array()
        } else if key == "items" && value.is_array() {
            value.as_array()
        } else {
            value
                .as_object()
                .and_then(|dict| dict.get(key))
                .and_then(|v| v.as_array())
        };

        let Some(array) = array_value else {
            return PathInspection::Missing(None);
        };
        if parts.len() == 1 {
            return PathInspection::Present("array".to_string(), None);
        }

        let object_elements: Vec<_> = array
            .iter()
            .take(50)
            .filter_map(|item| item.as_object().map(|_| item))
            .collect();
        if object_elements.is_empty() {
            return PathInspection::Missing(None);
        }

        let remaining = &parts[1..];
        let inspections: Vec<(usize, PathInspection)> = object_elements
            .iter()
            .enumerate()
            .map(|(index, element)| (index, inspect_parts(remaining, baseline_type, element)))
            .collect();

        let missing_indices: Vec<usize> = inspections
            .iter()
            .filter_map(|(idx, ins)| matches!(ins, PathInspection::Missing(_)).then_some(*idx))
            .collect();
        let missing_count = missing_indices.len();

        if object_elements.len() >= 2 && missing_count > 0 {
            return PathInspection::Missing(Some(format!(
                "{} of {} array elements are missing this field: {}",
                missing_count,
                object_elements.len(),
                array_index_summary(&missing_indices)
            )));
        }
        if missing_count == inspections.len() {
            return PathInspection::Missing(None);
        }

        let present_types: Vec<String> = inspections
            .iter()
            .filter_map(|(_, ins)| match ins {
                PathInspection::Present(t, _) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let null_indices: Vec<usize> = inspections
            .iter()
            .filter_map(|(idx, ins)| match ins {
                PathInspection::Present(t, _) if t == "null" => Some(*idx),
                _ => None,
            })
            .collect();
        if !null_indices.is_empty() && baseline_type != "null" {
            return PathInspection::Present("null".to_string(), Some(array_index_summary(&null_indices)));
        }
        if present_types.iter().any(|t| t == baseline_type) {
            return PathInspection::Present(baseline_type.to_string(), None);
        }
        let mut uniq = BTreeSet::new();
        for t in &present_types {
            uniq.insert(t.clone());
        }
        if uniq.len() > 1 {
            PathInspection::Present("mixed".to_string(), None)
        } else {
            PathInspection::Present(
                present_types.first().cloned().unwrap_or_else(|| "unknown".to_string()),
                None,
            )
        }
    } else {
        let dict = match value.as_object() {
            Some(d) => d,
            None => return PathInspection::Missing(None),
        };
        let Some(next_value) = dict.get(part) else {
            let hint = rename_hint_for_missing_key(part, baseline_type, dict);
            return PathInspection::Missing(hint);
        };
        inspect_parts(&parts[1..], baseline_type, next_value)
    }
}

fn make_issue(
    code: &str,
    path: String,
    expected: String,
    actual: String,
    message: String,
    severity: &str,
) -> SchemaIssue {
    SchemaIssue {
        code: code.to_string(),
        path,
        expected,
        actual,
        message,
        severity: severity.to_string(),
    }
}

fn is_suppressed(path: &str, suppressed_paths: &BTreeSet<String>) -> bool {
    suppressed_paths
        .iter()
        .any(|suppressed| path.contains(suppressed))
}

fn issue_from_change(change: &str, baseline: &SchemaSnapshot) -> SchemaIssue {
    let (code, path, expected, actual) = if let Some(path) = change.strip_prefix("Field removed: ") {
        let field_path = path.split(" (").next().unwrap_or(path).to_string();
        let expected = baseline.get(&field_path).cloned().unwrap_or_else(|| "unknown".to_string());
        ("MISSING_REQUIRED_FIELD".to_string(), field_path, expected, "missing".to_string())
    } else if let Some(path) = change.strip_prefix("Type changed: ") {
        let field_path = path.split(" (").next().unwrap_or(path).to_string();
        let (expected, actual) = parse_transition(change);
        ("TYPE_MISMATCH".to_string(), field_path, expected, actual)
    } else if let Some(path) = change.strip_prefix("Value became null: ") {
        let field_path = path.split(" (").next().unwrap_or(path).to_string();
        let expected = baseline.get(&field_path).cloned().unwrap_or_else(|| "unknown".to_string());
        ("NULLABILITY_MISMATCH".to_string(), field_path, expected, "null".to_string())
    } else {
        ("SCHEMA_VIOLATION".to_string(), "".to_string(), "unknown".to_string(), "unknown".to_string())
    };
    SchemaIssue {
        code,
        path,
        expected,
        actual,
        message: change.to_string(),
        severity: "high".to_string(),
    }
}

fn parse_transition(change: &str) -> (String, String) {
    let Some(open_idx) = change.rfind('(') else {
        return ("unknown".to_string(), "unknown".to_string());
    };
    let Some(close_idx) = change.rfind(')') else {
        return ("unknown".to_string(), "unknown".to_string());
    };
    if close_idx <= open_idx {
        return ("unknown".to_string(), "unknown".to_string());
    }
    let inner = &change[open_idx + 1..close_idx];
    let Some((left, right)) = inner.split_once("→") else {
        return ("unknown".to_string(), "unknown".to_string());
    };
    let expected = left.trim().to_string();
    let actual = right.split(',').next().unwrap_or(right).trim().to_string();
    (expected, actual)
}

fn is_declared_by_schema(path: &str, baseline: &SchemaSnapshot) -> bool {
    if is_invalid_schema_path(path) {
        return true;
    }
    if baseline.contains_key(path) {
        return true;
    }
    let dotted = format!("{path}.");
    let arrayed = format!("{path}[].");
    baseline
        .keys()
        .any(|k| k.starts_with(&dotted) || k.starts_with(&arrayed))
}

fn array_index_summary(indices: &[usize]) -> String {
    let first_few = indices
        .iter()
        .take(6)
        .map(|idx| format!("#{idx}"))
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = indices.len().saturating_sub(6);
    if remaining > 0 {
        format!("items {first_few}, +{remaining} more")
    } else {
        format!("items {first_few}")
    }
}

fn rename_hint_for_removed_path(
    path: &str,
    baseline_type: &str,
    baseline: &SchemaSnapshot,
    current: &SchemaSnapshot,
) -> Option<String> {
    let removed_parent = parent_path(path);
    let removed_leaf = normalized_leaf(path);
    let mut scored = Vec::<(String, f64)>::new();
    for (candidate, candidate_type) in current {
        if baseline.contains_key(candidate) {
            continue;
        }
        if !types_compatible(candidate_type, baseline_type) {
            continue;
        }
        if parent_path(candidate) != removed_parent {
            continue;
        }
        let score = rename_similarity_score(&removed_leaf, candidate);
        scored.push((candidate.clone(), score));
    }
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.len().cmp(&b.0.len())));
    let best = scored.first()?;
    let second = scored.get(1).map(|s| s.1).unwrap_or(0.0);
    if best.1 < 0.68 || (best.1 - second) < 0.03 {
        return None;
    }
    Some(best.0.clone())
}

fn parent_path(path: &str) -> String {
    path.rsplit_once('.').map(|(p, _)| p.to_string()).unwrap_or_default()
}

fn normalized_leaf(path: &str) -> String {
    path.split('.')
        .last()
        .unwrap_or(path)
        .replace("[]", "")
        .replace('_', "")
        .replace('-', "")
        .to_lowercase()
}

fn rename_similarity_score(removed_leaf: &str, candidate_path: &str) -> f64 {
    let candidate_leaf = normalized_leaf(candidate_path);
    let max_len = removed_leaf.len().max(candidate_leaf.len());
    let distance = levenshtein_distance(removed_leaf, &candidate_leaf);
    let distance_score = if max_len == 0 {
        1.0
    } else {
        (1.0 - (distance as f64 / max_len as f64)).max(0.0)
    };
    let prefix_score = if removed_leaf.starts_with(&candidate_leaf) || candidate_leaf.starts_with(removed_leaf) {
        1.0
    } else {
        0.0
    };
    let contains_score = if removed_leaf.contains(&candidate_leaf) || candidate_leaf.contains(removed_leaf) {
        1.0
    } else {
        0.0
    };
    (distance_score * 0.70) + (prefix_score * 0.20) + (contains_score * 0.10)
}

fn levenshtein_distance(lhs: &str, rhs: &str) -> usize {
    if lhs == rhs {
        return 0;
    }
    if lhs.is_empty() {
        return rhs.len();
    }
    if rhs.is_empty() {
        return lhs.len();
    }
    let a: Vec<char> = lhs.chars().collect();
    let b: Vec<char> = rhs.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        current[0] = i;
        for j in 1..=b.len() {
            let substitution_cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            current[j] = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

fn rename_hint_for_missing_key(
    missing_key: &str,
    baseline_type: &str,
    dict: &serde_json::Map<String, Value>,
) -> Option<String> {
    let mut scored = Vec::<(String, f64)>::new();
    for (key, value) in dict {
        if !types_compatible(&type_name(value), baseline_type) {
            continue;
        }
        let score = rename_similarity_score(
            &normalize_field_name(missing_key),
            &normalize_field_name(key),
        );
        if score >= 0.68 {
            scored.push((key.clone(), score));
        }
    }
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.len().cmp(&b.0.len())));
    let best = scored.first()?;
    let second = scored.get(1).map(|s| s.1).unwrap_or(0.0);
    if (best.1 - second) < 0.03 {
        return None;
    }
    Some(format!("possible rename: {}", best.0))
}

fn normalize_field_name(value: &str) -> String {
    value
        .replace("[]", "")
        .replace('_', "")
        .replace('-', "")
        .to_lowercase()
}

fn types_compatible(lhs: &str, rhs: &str) -> bool {
    if lhs == rhs {
        return true;
    }
    let boolish = ["boolean", "bool"];
    boolish.contains(&lhs) && boolish.contains(&rhs)
}

pub fn extract_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for component in path.split('.') {
        if let Some(open) = component.find('[') {
            let dict_key = &component[..open];
            let close = component.rfind(']')?;
            let idx: usize = component[open + 1..close].parse().ok()?;
            current = current.get(dict_key)?;
            current = current.get(idx)?;
        } else {
            current = current.get(component)?;
        }
    }
    Some(current)
}

pub fn extract_token(body: &[u8], path: &str) -> Option<String> {
    let root: Value = serde_json::from_slice(body).ok()?;
    let value = extract_json_path(&root, path)?;
    value.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_removed_field() {
        let mut baseline = SchemaSnapshot::new();
        baseline.insert("id".into(), "string".into());
        baseline.insert("email".into(), "string".into());
        let mut current = SchemaSnapshot::new();
        current.insert("id".into(), "string".into());
        let changes = breaking_changes(&baseline, &current, None);
        assert!(changes.iter().any(|c| c.contains("email")));
    }

    #[test]
    fn extracts_nested_token() {
        let body = br#"{"data":{"access_token":"abc123"}}"#;
        assert_eq!(
            extract_token(body, "data.access_token").as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn validates_array_elements_and_missing_fields() {
        let baseline = SchemaSnapshot::from([
            ("orders[].id".to_string(), "string".to_string()),
            ("orders[].price".to_string(), "number".to_string()),
        ]);
        let required = BTreeSet::from(["orders[].id".to_string(), "orders[].price".to_string()]);
        let body = br#"{"orders":[{"id":"a","price":10},{"id":"b"}]}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result
            .violations
            .iter()
            .any(|i| i.code == "MISSING_REQUIRED_FIELD" && i.path == "orders[].price"));
    }

    #[test]
    fn validates_type_mismatch_in_array_elements() {
        let baseline = SchemaSnapshot::from([("orders[].price".to_string(), "number".to_string())]);
        let required = BTreeSet::from(["orders[].price".to_string()]);
        let body = br#"{"orders":[{"price":"10"},{"price":"12"}]}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result
            .violations
            .iter()
            .any(|i| i.code == "TYPE_MISMATCH" && i.path == "orders[].price"));
    }

    #[test]
    fn validates_nullability_mismatch() {
        let baseline = SchemaSnapshot::from([("user.id".to_string(), "string".to_string())]);
        let required = BTreeSet::from(["user.id".to_string()]);
        let body = br#"{"user":{"id":null}}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result
            .violations
            .iter()
            .any(|i| i.code == "NULLABILITY_MISMATCH" && i.path == "user.id"));
    }

    #[test]
    fn emits_unexpected_field_warnings() {
        let baseline = SchemaSnapshot::from([("user.id".to_string(), "string".to_string())]);
        let required = BTreeSet::from(["user.id".to_string()]);
        let body = br#"{"user":{"id":"x","nickname":"neo"}}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|i| i.code == "UNEXPECTED_FIELD" && i.path == "user.nickname"));
    }

    #[test]
    fn enforcement_required_fields_skips_undeclared_paths() {
        let baseline = BTreeMap::from([
            ("results[].id".to_string(), "string".to_string()),
            (
                "results[].assignedTo.__type".to_string(),
                "string".to_string(),
            ),
        ]);
        let declared = BTreeSet::from(["results[].id".to_string()]);
        let required = BTreeSet::from([
            "results[].id".to_string(),
            "results[].assignedTo.__type".to_string(),
        ]);
        let enforced = enforcement_required_fields(
            &baseline,
            Some(&required),
            false,
            Some(&declared),
        );
        assert!(enforced.contains("results[].id"));
        assert!(!enforced.contains("results[].assignedTo.__type"));
    }

    #[test]
    fn respects_suppressed_paths_for_violations_and_warnings() {
        let baseline = SchemaSnapshot::from([("user.id".to_string(), "string".to_string())]);
        let required = BTreeSet::from(["user.id".to_string()]);
        let suppressed = BTreeSet::from(["user.id".to_string(), "user.nickname".to_string()]);
        let body = br#"{"user":{"id":null,"nickname":"neo"}}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &suppressed, body).unwrap();
        assert!(result.violations.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn skips_child_required_when_optional_parent_missing() {
        let baseline = SchemaSnapshot::from([("results[].objectProject.createdAt".to_string(), "string".to_string())]);
        let required = BTreeSet::from(["results[].objectProject.createdAt".to_string()]);
        let body = br#"{"results":[{"id":"1"},{"id":"2"}]}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result
            .violations
            .iter()
            .any(|i| i.code == "MISSING_REQUIRED_FIELD" && i.path == "results[].objectProject.createdAt"));
    }

    #[test]
    fn handles_root_array_path_without_empty_dollar_missing() {
        let baseline = SchemaSnapshot::from([("items[].id".to_string(), "string".to_string())]);
        let required = BTreeSet::from(["items[].id".to_string()]);
        let body = br#"[{"id":"1"},{"id":"2"}]"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result.violations.is_empty());
    }

    #[test]
    fn normalizes_legacy_bare_array_paths_to_named_container() {
        let baseline = SchemaSnapshot::from([
            ("[].id".to_string(), "string".to_string()),
            ("results[].currentStatus".to_string(), "string".to_string()),
            ("result.groupId".to_string(), "string".to_string()),
        ]);
        let required = BTreeSet::from([
            "[].id".to_string(),
            "[].currentStatus".to_string(),
            "result.groupId".to_string(),
        ]);
        let body = br#"{"success":true,"result":{},"results":[{"id":"1","currentStatus":"open"},{"id":"2"}]}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(
            result.violations.iter().all(|issue| !issue.path.starts_with("[].")),
            "violations={:?}",
            result.violations
        );
        assert!(
            result.violations.iter().all(|issue| {
                !issue.message.contains("Field removed: [].")
                    && !issue.message.contains("Type changed: [].")
                    && !issue.message.contains("Value became null: [].")
            }),
            "messages={:?}",
            result.violations
        );
        assert!(
            result
                .violations
                .iter()
                .any(|issue| issue.path == "results[].currentStatus" || issue.path == "results[].id"),
            "violations={:?}",
            result.violations
        );
    }

    #[test]
    fn resolves_bare_array_prefix_from_baseline() {
        let baseline = SchemaSnapshot::from([("results[].id".to_string(), "string".to_string())]);
        assert_eq!(
            normalize_schema_path("[].id", &baseline, None),
            "results[].id"
        );
    }

    #[test]
    fn ignores_invalid_required_paths() {
        let baseline = SchemaSnapshot::from([
            ("".to_string(), "string".to_string()),
            (".".to_string(), "string".to_string()),
            ("$".to_string(), "string".to_string()),
            ("user.id".to_string(), "string".to_string()),
        ]);
        let required = BTreeSet::from([
            "".to_string(),
            ".".to_string(),
            "$".to_string(),
            "user.id".to_string(),
        ]);
        let body = br#"{"user":{"id":"abc"}}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result.violations.is_empty());
    }

    #[test]
    fn empty_required_fields_skip_validation() {
        let baseline = SchemaSnapshot::from([("user.id".to_string(), "string".to_string())]);
        let required = BTreeSet::new();
        let body = br#"{"user":{}}"#;
        let result = validate_response_contract(&baseline, Some(&required), true, &BTreeSet::new(), body).unwrap();
        assert!(result.violations.is_empty(), "violations={:?}", result.violations);
    }
}
