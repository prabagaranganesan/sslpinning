use crate::bundle::RunSummary;
use anyhow::{Context, Result};
use reqwest::header::AUTHORIZATION;
use reqwest::Client;
use serde_json::json;

pub async fn submit_run(api_base: &str, token: &str, summary: &RunSummary) -> Result<()> {
    let url = format!("{}/api/v1/guard/runs", api_base.trim_end_matches('/'));
    let payload = json!({
        "sessionId": summary.session_id,
        "sessionName": summary.session_name,
        "runAt": summary.run_at,
        "results": summary.results.iter().map(|r| json!({
            "endpointId": r.endpoint_id,
            "method": r.method,
            "url": r.url,
            "status": r.status,
            "statusCode": r.status_code,
            "breakingChanges": r.breaking_changes,
            "error": r.error,
        })).collect::<Vec<_>>()
    });

    let client = Client::new();
    let resp = client
        .post(url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("ngrok-skip-browser-warning", "true")
        .json(&payload)
        .send()
        .await
        .context("submit run")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("submit run failed: {body}");
    }
    Ok(())
}

pub async fn login(api_base: &str, email: &str, password: &str) -> Result<String> {
    let url = format!("{}/api/v1/auth/login", api_base.trim_end_matches('/'));
    let client = Client::new();
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("ngrok-skip-browser-warning", "true")
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .context("login")?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.context("parse login response")?;
    if !status.is_success() {
        anyhow::bail!("login failed: {body}");
    }
    body.get("accessToken")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("login response missing accessToken"))
}
