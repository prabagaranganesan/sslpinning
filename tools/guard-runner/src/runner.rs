use crate::bundle::{is_login_endpoint, rewrite_url, substitute_env, HeadlessBundle, HeadlessEndpoint, RunEntry, RunSummary, SchemaIssue};
use crate::schema::{extract_token, validate_response_contract};
use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Method, StatusCode};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use url::Url;

const SKIP_HEADERS: &[&str] = &["host", "content-length", "transfer-encoding", "connection"];

pub async fn run_bundle(bundle: &HeadlessBundle, base_url: Option<&str>, timeout_secs: u64) -> Result<RunSummary> {
    let session_id = bundle
        .resolved_session_id()
        .ok_or_else(|| anyhow!("bundle missing sessionId"))?
        .to_string();

    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let mut bearer: Option<String> = std::env::var("GUARD_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let mut results = Vec::new();

    let mut endpoints: Vec<_> = bundle.endpoints.iter().filter(|e| e.is_enabled()).collect();
    // Login POST first, then other anchors, then the rest. DELETE logout anchors run last.
    endpoints.sort_by_key(|e| {
        if is_login_endpoint(e) {
            0
        } else if e.auth_anchor && e.method.eq_ignore_ascii_case("DELETE") {
            3
        } else if e.auth_anchor {
            1
        } else {
            2
        }
    });
    for ep in &endpoints {
        let url_str = match rewrite_url(&ep.url, base_url, ep) {
            Ok(u) => substitute_env(&u),
            Err(e) => {
                results.push(ReplayOutcome {
                    status: "networkError".into(),
                    status_code: None,
                    breaking_changes: vec![],
                    violations: vec![],
                    warnings: vec![],
                    error: Some(format!("Invalid URL: {e}")),
                    raw_body: None,
                }.into_run_entry(ep, &ep.url));
                continue;
            }
        };
        let entry = replay_endpoint(&client, ep, &url_str, bearer.as_deref()).await;
        if is_login_endpoint(ep) {
            if let Some(token) = extract_session_token(ep, entry.raw_body.as_deref()) {
                bearer = Some(token);
            }
        } else if entry.status == "ok" {
            if let Some(token) = extract_session_token(ep, entry.raw_body.as_deref()) {
                bearer = Some(token);
            }
        }
        results.push(entry.into_run_entry(ep, &url_str));
    }

    let ok = results.iter().filter(|r| r.status == "ok").count();
    let broken = results.iter().filter(|r| r.status == "broken").count();
    let errors = results.iter().filter(|r| r.status == "networkError" || r.status == "authExpired").count();

    Ok(RunSummary {
        session_id,
        session_name: bundle.resolved_session_name().to_string(),
        run_at: chrono::Utc::now().to_rfc3339(),
        total: results.len(),
        ok,
        broken,
        errors,
        results,
    })
}

struct ReplayOutcome {
    status: String,
    status_code: Option<i32>,
    breaking_changes: Vec<String>,
    violations: Vec<SchemaIssue>,
    warnings: Vec<SchemaIssue>,
    error: Option<String>,
    raw_body: Option<String>,
}

impl ReplayOutcome {
    fn into_run_entry(self, ep: &HeadlessEndpoint, url: &str) -> RunEntry {
        let response_preview = self.raw_body.as_deref().map(|body| {
            preview_response_body(body, is_login_endpoint(ep))
        });
        RunEntry {
            endpoint_id: ep.id.clone(),
            method: ep.method.clone(),
            url: url.to_string(),
            status: self.status,
            status_code: self.status_code,
            breaking_changes: self.breaking_changes,
            violations: self.violations,
            warnings: self.warnings,
            error: self.error,
            response_preview,
            impact_criticality: ep.impact_criticality.clone(),
            effective_release_gate: ep.effective_release_gate_policy().as_str().to_string(),
        }
    }
}

const RESPONSE_PREVIEW_MAX: usize = 6_000;

pub fn preview_response_body(body: &str, redact_secrets: bool) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty body)".to_string();
    }
    if redact_secrets {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let redacted = redact_sensitive_value(value);
            if let Ok(pretty) = serde_json::to_string_pretty(&redacted) {
                return truncate_preview(&pretty);
            }
        }
    } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Ok(pretty) = serde_json::to_string_pretty(&value) {
            return truncate_preview(&pretty);
        }
    }
    truncate_preview(trimmed)
}

fn truncate_preview(body: &str) -> String {
    if body.len() <= RESPONSE_PREVIEW_MAX {
        return body.to_string();
    }
    format!(
        "{}\n... (truncated, {} bytes total)",
        &body[..RESPONSE_PREVIEW_MAX],
        body.len()
    )
}

fn redact_sensitive_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let redacted = if is_sensitive_key(&key) {
                    serde_json::Value::String("***".to_string())
                } else {
                    redact_sensitive_value(val)
                };
                out.insert(key, redacted);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(redact_sensitive_value).collect())
        }
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    [
        "token",
        "sessiontoken",
        "session_token",
        "accesstoken",
        "access_token",
        "refreshtoken",
        "refresh_token",
        "password",
        "secret",
        "authorization",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|hint| lower.contains(hint))
}

async fn replay_endpoint(
    client: &Client,
    ep: &HeadlessEndpoint,
    url_str: &str,
    bearer: Option<&str>,
) -> ReplayOutcome {
    let parsed = match Url::parse(url_str) {
        Ok(u) => u,
        Err(e) => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: None,
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some(format!("Invalid URL: {e}")),
                raw_body: None,
            };
        }
    };

    let method = match Method::from_bytes(ep.method.to_uppercase().as_bytes()) {
        Ok(m) => m,
        Err(_) => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: None,
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some(format!("Unsupported method {}", ep.method)),
                raw_body: None,
            };
        }
    };

    let headers = match build_headers(ep, bearer) {
        Ok(h) => h,
        Err(e) => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: None,
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some(e),
                raw_body: None,
            };
        }
    };

    let mut req = client.request(method.clone(), parsed.clone()).headers(headers);

    if method != Method::GET {
        if let Some(body) = ep.body.as_deref() {
            let resolved = substitute_env(body);
            req = req.body(resolved);
            if ep.headers.keys().all(|k| k.to_lowercase() != "content-type") {
                req = req.header(CONTENT_TYPE, "application/json");
            }
        }
    }

    let started = Instant::now();
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: None,
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some(e.to_string()),
                raw_body: None,
            };
        }
    };

    let status = resp.status();
    let status_code = status.as_u16() as i32;
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: Some(status_code),
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some(e.to_string()),
                raw_body: None,
            };
        }
    };

    let _elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let raw_body = String::from_utf8_lossy(&bytes).to_string();

    let is_success = status.is_success() || status == StatusCode::NOT_MODIFIED;

    if !is_success {
        return ReplayOutcome {
            status: if status_code == 401 || status_code == 403 {
                "authExpired".into()
            } else {
                "networkError".into()
            },
            status_code: Some(status_code),
            breaking_changes: vec![],
            violations: vec![],
            warnings: vec![],
            error: Some(format!("HTTP {status_code}")),
            raw_body: Some(raw_body),
        };
    }

    // Empty or not-modified bodies skip schema diff.
    if status == StatusCode::NOT_MODIFIED || bytes.is_empty() {
        return ReplayOutcome {
            status: "ok".into(),
            status_code: Some(status_code),
            breaking_changes: vec![],
            violations: vec![],
            warnings: vec![],
            error: None,
            raw_body: if raw_body.is_empty() { None } else { Some(raw_body) },
        };
    }

    let baseline = match ep.baseline_schema.as_ref() {
        Some(b) if !b.is_empty() => b,
        _ => {
            return ReplayOutcome {
                status: "ok".into(),
                status_code: Some(status_code),
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: None,
                raw_body: Some(raw_body),
            };
        }
    };
    let required = crate::schema::enforcement_required_fields(
        baseline,
        ep.required_field_set().as_ref(),
        ep.schema_reviewed,
        ep.schema_declared_path_set().as_ref(),
    );
    let suppressed: BTreeSet<_> = ep.suppressed_paths.iter().cloned().collect();
    let validation = match validate_response_contract(
        baseline,
        Some(&required),
        ep.schema_reviewed,
        &suppressed,
        &bytes,
    ) {
        Some(v) => v,
        None => {
            return ReplayOutcome {
                status: "networkError".into(),
                status_code: Some(status_code),
                breaking_changes: vec![],
                violations: vec![],
                warnings: vec![],
                error: Some("Response is not valid JSON".into()),
                raw_body: Some(raw_body),
            };
        }
    };
    let breaking_changes: Vec<String> = validation.violations.iter().map(|v| v.message.clone()).collect();
    let status = if breaking_changes.is_empty() { "ok" } else { "broken" };
    ReplayOutcome {
        status: status.into(),
        status_code: Some(status_code),
        breaking_changes,
        violations: validation.violations,
        warnings: validation.warnings,
        error: None,
        raw_body: Some(raw_body),
    }
}

fn extract_session_token(ep: &HeadlessEndpoint, body: Option<&str>) -> Option<String> {
    let body = body?;
    let mut paths: Vec<&str> = Vec::new();
    if let Some(path) = ep.auth_token_path.as_deref().filter(|s| !s.is_empty()) {
        paths.push(path);
    }
    paths.extend([
        "access_tokened",
        "accessToken",
        "access_token",
        "result.sessionToken",
        "sessionToken",
        "token",
    ]);
    for path in paths {
        if let Some(token) = extract_token(body.as_bytes(), path) {
            return Some(token);
        }
    }
    None
}

fn build_headers(ep: &HeadlessEndpoint, bearer: Option<&str>) -> Result<HeaderMap, String> {
    let ci_token = std::env::var("GUARD_AUTH_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let mut headers = HeaderMap::new();
    for (key, value) in &ep.headers {
        let lower = key.to_lowercase();
        if SKIP_HEADERS.contains(&lower.as_str()) {
            continue;
        }
        if ci_token.is_some() && (lower == "authorization" || lower == "x-user-token") {
            continue;
        }
        let resolved = substitute_env(value);
        let name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| e.to_string())?;
        let val = HeaderValue::from_str(&resolved).map_err(|e| e.to_string())?;
        headers.insert(name, val);
    }

    let token = ci_token.as_deref().or(bearer);
    if let Some(token) = token {
        let header_name = ep
            .auth_token_header_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("Authorization");
        let name = HeaderName::from_bytes(header_name.as_bytes()).unwrap_or(AUTHORIZATION);
        let lower = header_name.to_lowercase();
        let value = if lower == "authorization" {
            format!("Bearer {token}")
        } else {
            token.to_string()
        };
        headers.insert(
            name,
            HeaderValue::from_str(&value).map_err(|e| e.to_string())?,
        );
        // Upkeep uses x-user-token for session tokens.
        if lower != "x-user-token" {
            headers.insert(
                HeaderName::from_static("x-user-token"),
                HeaderValue::from_str(token).map_err(|e| e.to_string())?,
            );
        }
    }

    Ok(headers)
}

pub async fn load_bundle_from_file(path: &str) -> Result<HeadlessBundle> {
    let data = tokio::fs::read(path).await.context("read bundle file")?;
    serde_json::from_slice(&data).context("parse bundle JSON")
}

#[cfg(test)]
mod integration_tests {
    use crate::bundle::HeadlessBundle;
    use crate::run_bundle;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn replays_and_detects_schema_break() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "1"
            })))
            .mount(&server)
            .await;

        let bundle = HeadlessBundle {
            version: 1,
            session_id: "00000000-0000-0000-0000-000000000001".into(),
            session_name: "Test".into(),
            endpoints: vec![crate::bundle::HeadlessEndpoint {
                id: "ep1".into(),
                method: "GET".into(),
                url: format!("{}/users", server.uri()),
                headers: Default::default(),
                body: None,
                auth_anchor: false,
                auth_token_path: None,
                auth_token_header_name: None,
                enabled: true,
                baseline_schema: Some([("id".into(), "string".into()), ("email".into(), "string".into())].into()),
                required_fields: Some(vec!["id".into(), "email".into()]),
                schema_declared_paths: None,
                schema_reviewed: true,
                suppressed_paths: vec![],
                impact_criticality: Some("high".into()),
                business_flow: None,
                release_gate_policy: Some("blocker".into()),
                release_gate_policy_pinned: true,
            }],
        };

        let summary = run_bundle(&bundle, None, 10).await.unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.broken, 1);
    }
}

pub async fn load_bundle_from_api(api_base: &str, token: &str, session_id: &str) -> Result<HeadlessBundle> {
    let url = format!(
        "{}/api/v1/guard/sessions/{}/bundle",
        api_base.trim_end_matches('/'),
        session_id
    );
    let client = Client::new();
    let resp = client
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("ngrok-skip-browser-warning", "true")
        .send()
        .await
        .context("fetch bundle")?;
    if resp.status() == StatusCode::NOT_FOUND {
        anyhow::bail!("Guard session bundle not found — export/sync from ProxyHawk app first");
    }
    if !resp.status().is_success() {
        anyhow::bail!("fetch bundle failed: HTTP {}", resp.status());
    }
    let data = resp.bytes().await?;
    serde_json::from_slice(&data).context("parse bundle JSON from API")
}
