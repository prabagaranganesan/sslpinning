use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum PrintResponses {
    Never,
    Failures,
    #[default]
    All,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeadlessBundle {
    pub version: u32,
    #[serde(default, alias = "sessionId")]
    pub session_id: String,
    #[serde(default, alias = "sourceSessionName")]
    pub session_name: String,
    #[serde(default)]
    pub endpoints: Vec<HeadlessEndpoint>,
}

impl HeadlessBundle {
    pub fn resolved_session_id(&self) -> Option<&str> {
        if self.session_id.is_empty() {
            None
        } else {
            Some(self.session_id.as_str())
        }
    }

    pub fn resolved_session_name(&self) -> &str {
        if self.session_name.is_empty() {
            "Guard Session"
        } else {
            &self.session_name
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeadlessEndpoint {
    pub id: String,
    pub method: String,
    #[serde(default, alias = "urlString")]
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default, alias = "requestBody")]
    pub body: Option<String>,
    #[serde(default, alias = "isAuthAnchor")]
    pub auth_anchor: bool,
    #[serde(default, alias = "authTokenPath")]
    pub auth_token_path: Option<String>,
    #[serde(default, alias = "authTokenHeaderName")]
    pub auth_token_header_name: Option<String>,
    #[serde(default = "default_true", alias = "isEnabled")]
    pub enabled: bool,
    #[serde(default, alias = "baselineSchema")]
    pub baseline_schema: Option<BTreeMap<String, String>>,
    #[serde(default, alias = "requiredFields")]
    pub required_fields: Option<Vec<String>>,
    #[serde(default, alias = "schemaDeclaredPaths")]
    pub schema_declared_paths: Option<Vec<String>>,
    #[serde(default, alias = "schemaReviewed")]
    pub schema_reviewed: bool,
    #[serde(default, alias = "suppressedSchemaBreakPaths")]
    pub suppressed_paths: Vec<String>,
    #[serde(default, alias = "impactCriticality")]
    pub impact_criticality: Option<String>,
    #[serde(default, alias = "businessFlow")]
    pub business_flow: Option<String>,
    #[serde(default, alias = "releaseGatePolicy")]
    pub release_gate_policy: Option<String>,
    #[serde(default, alias = "releaseGatePolicyPinned")]
    pub release_gate_policy_pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReleaseGatePolicy {
    Blocker,
    Warning,
    Ignore,
}

impl ReleaseGatePolicy {
    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "blocker" => Self::Blocker,
            "ignore" => Self::Ignore,
            _ => Self::Warning,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocker => "blocker",
            Self::Warning => "warning",
            Self::Ignore => "ignore",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactCriticality {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

impl ImpactCriticality {
    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "critical" => Self::Critical,
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "CRITICAL",
            Self::High => "HIGH",
            Self::Medium => "MEDIUM",
            Self::Low => "LOW",
            Self::Unknown => "UNSET",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Critical | Self::High => "🔴",
            Self::Medium => "🟡",
            Self::Low => "🟢",
            Self::Unknown => "⚪",
        }
    }
}

impl HeadlessEndpoint {
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn schema_declared_path_set(&self) -> Option<BTreeSet<String>> {
        self.schema_declared_paths
            .as_ref()
            .map(|v| v.iter().cloned().collect())
    }

    pub fn required_field_set(&self) -> Option<BTreeSet<String>> {
        self.required_fields.as_ref().map(|v| v.iter().cloned().collect())
    }

    pub fn impact_criticality(&self) -> ImpactCriticality {
        self.impact_criticality
            .as_deref()
            .map(ImpactCriticality::from_str)
            .unwrap_or(ImpactCriticality::Unknown)
    }

    /// Mirrors Mac app `effectiveReleaseGatePolicy`.
    pub fn effective_release_gate_policy(&self) -> ReleaseGatePolicy {
        if self.release_gate_policy.as_deref() == Some("ignore") {
            return ReleaseGatePolicy::Ignore;
        }
        if self.release_gate_policy_pinned {
            return ReleaseGatePolicy::from_str(self.release_gate_policy.as_deref().unwrap_or("warning"));
        }
        if let Some(criticality) = self.impact_criticality.as_deref() {
            return match criticality.to_lowercase().as_str() {
                "critical" | "high" => ReleaseGatePolicy::Blocker,
                "medium" | "low" => ReleaseGatePolicy::Warning,
                _ => ReleaseGatePolicy::Warning,
            };
        }
        if let Some(flow) = self.business_flow.as_deref() {
            return match flow.to_lowercase().as_str() {
                "checkout" | "login" | "billing" => ReleaseGatePolicy::Blocker,
                _ => ReleaseGatePolicy::Warning,
            };
        }
        ReleaseGatePolicy::from_str(self.release_gate_policy.as_deref().unwrap_or("warning"))
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaIssue {
    pub code: String,
    pub path: String,
    pub expected: String,
    pub actual: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunEntry {
    pub endpoint_id: String,
    pub method: String,
    pub url: String,
    pub status: String,
    pub status_code: Option<i32>,
    pub breaking_changes: Vec<String>,
    pub violations: Vec<SchemaIssue>,
    pub warnings: Vec<SchemaIssue>,
    pub error: Option<String>,
    /// Truncated response body for local/CI debugging (not submitted to backend).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact_criticality: Option<String>,
    pub effective_release_gate: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub session_id: String,
    pub session_name: String,
    pub run_at: String,
    pub total: usize,
    pub ok: usize,
    pub broken: usize,
    pub errors: usize,
    pub results: Vec<RunEntry>,
}

impl RunSummary {
    pub fn outcome(&self) -> &'static str {
        if self.blocker_failures() > 0 {
            "broken"
        } else if self.warning_failures() > 0 {
            "risky"
        } else {
            "ok"
        }
    }

    pub fn exit_code(&self) -> i32 {
        if self.blocker_failures() > 0 {
            1
        } else {
            0
        }
    }

    pub fn blocker_failures(&self) -> usize {
        self.results.iter().filter(|entry| entry.is_blocker_failure()).count()
    }

    pub fn warning_failures(&self) -> usize {
        self.results.iter().filter(|entry| entry.is_warning_failure()).count()
    }
}

impl RunEntry {
    pub fn release_gate_policy(&self) -> ReleaseGatePolicy {
        ReleaseGatePolicy::from_str(&self.effective_release_gate)
    }

    pub fn endpoint_criticality(&self) -> ImpactCriticality {
        self.impact_criticality
            .as_deref()
            .map(ImpactCriticality::from_str)
            .unwrap_or(ImpactCriticality::Unknown)
    }

    pub fn is_blocker_failure(&self) -> bool {
        self.release_gate_policy() == ReleaseGatePolicy::Blocker && self.is_failure_status()
    }

    pub fn is_warning_failure(&self) -> bool {
        self.release_gate_policy() == ReleaseGatePolicy::Warning && self.is_failure_status()
    }

    fn is_failure_status(&self) -> bool {
        matches!(self.status.as_str(), "broken" | "networkError" | "authExpired")
    }
}

/// POST login/auth endpoints keep their captured host (e.g. demo-api) while API calls
/// rewrite to the deployed `--base-url` (e.g. staging-gw).
pub fn is_login_endpoint(ep: &HeadlessEndpoint) -> bool {
    if !ep.method.eq_ignore_ascii_case("POST") {
        return false;
    }
    let path = ep.url.to_lowercase();
    [
        "/auth",
        "login",
        "signin",
        "sign-in",
        "oauth/token",
        "/token",
        "authenticate",
    ]
    .iter()
    .any(|k| path.contains(k))
}

pub fn normalize_deploy_base_url(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("deploy base URL is empty");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.trim_end_matches('/').to_string());
    }
    Ok(format!("https://{}", trimmed.trim_start_matches('/')))
}

pub fn rewrite_url(original: &str, base_url: Option<&str>, ep: &HeadlessEndpoint) -> anyhow::Result<String> {
    let Some(base) = base_url else {
        return Ok(original.to_string());
    };
    if is_login_endpoint(ep) {
        return Ok(original.to_string());
    }
    let base_normalized = normalize_deploy_base_url(base)?;
    let orig = url::Url::parse(original)?;
    let base_parsed = url::Url::parse(&base_normalized)?;
    // Mixed-host sessions (e.g. demo-api login + staging-gw APIs): only rewrite endpoints
    // captured on the deploy target host. Other hosts stay as recorded.
    if orig.host_str() != base_parsed.host_str() {
        return Ok(original.to_string());
    }
    let mut out = base_parsed.join(orig.path())?;
    if let Some(q) = orig.query() {
        out.set_query(Some(q));
    }
    Ok(out.to_string())
}

pub fn substitute_env(input: &str) -> String {
    let mut out = input.to_string();
    for (key, value) in std::env::vars() {
        out = out.replace(&format!("${{{}}}", key), &value);
        out = out.replace(&format!("${}", key), &value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ep(method: &str, url: &str) -> HeadlessEndpoint {
        HeadlessEndpoint {
            id: "ep".into(),
            method: method.into(),
            url: url.into(),
            headers: Default::default(),
            body: None,
            auth_anchor: false,
            auth_token_path: None,
            auth_token_header_name: None,
            enabled: true,
            baseline_schema: None,
            required_fields: None,
            schema_declared_paths: None,
            schema_reviewed: false,
            suppressed_paths: vec![],
            impact_criticality: None,
            business_flow: None,
            release_gate_policy: None,
            release_gate_policy_pinned: false,
        }
    }

    #[test]
    fn rewrite_preserves_path() {
        let ep = sample_ep("GET", "https://old.example.com/v1/users?id=1");
        let url = rewrite_url(
            "https://old.example.com/v1/users?id=1",
            Some("https://old.example.com"),
            &ep,
        )
        .unwrap();
        assert_eq!(url, "https://old.example.com/v1/users?id=1");
    }

    #[test]
    fn rewrite_keeps_login_host() {
        let ep = sample_ep("POST", "https://demo-api.example.com/api/v1/auth?");
        let url = rewrite_url(
            "https://demo-api.example.com/api/v1/auth?",
            Some("https://staging.example.com"),
            &ep,
        )
        .unwrap();
        assert_eq!(url, "https://demo-api.example.com/api/v1/auth?");
    }

    #[test]
    fn rewrite_keeps_other_host_endpoints() {
        let ep = sample_ep("GET", "https://demo-api.example.com/api/v1/users/me");
        let url = rewrite_url(
            "https://demo-api.example.com/api/v1/users/me",
            Some("https://staging.example.com"),
            &ep,
        )
        .unwrap();
        assert_eq!(url, "https://demo-api.example.com/api/v1/users/me");
    }

    #[test]
    fn rewrite_accepts_host_only_base_url() {
        let ep = sample_ep("GET", "https://staging.example.com/api/items");
        let url = rewrite_url(
            "https://staging.example.com/api/items",
            Some("staging.example.com"),
            &ep,
        )
        .unwrap();
        assert_eq!(url, "https://staging.example.com/api/items");
    }
}
