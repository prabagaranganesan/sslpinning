mod bundle;
mod report;
mod runner;
mod schema;
mod submit;

use anyhow::{Context, Result};
use bundle::{PrintResponses, RunSummary};
use clap::Parser;
use report::print_deployment_summary;
use runner::{load_bundle_from_api, load_bundle_from_file, run_bundle};
use submit::{login, submit_run};
use std::collections::BTreeSet;

#[derive(Parser, Debug)]
#[command(name = "proxyhawk-guard", about = "Headless ProxyHawk Guard runner for CI")]
struct Args {
    /// Path to guard session bundle JSON (optional if --session-id + API auth provided)
    #[arg(long, env = "GUARD_BUNDLE_PATH")]
    bundle: Option<String>,

    /// Guard session UUID — fetch bundle from ProxyHawk backend when --bundle omitted
    #[arg(long, env = "PROXYHAWK_GUARD_SESSION_ID")]
    session_id: Option<String>,

    /// Optional deploy host override. Only endpoints captured on this host are rewritten;
    /// login/auth and endpoints on other hosts keep their recorded URLs.
    #[arg(long, env = "GUARD_BASE_URL")]
    base_url: Option<String>,

    /// ProxyHawk API base URL for bundle fetch / run submit
    #[arg(long, env = "PROXYHAWK_API_BASE_URL")]
    api_base_url: Option<String>,

    /// Bearer token (optional if email/password provided)
    #[arg(long, env = "PROXYHAWK_API_TOKEN")]
    api_token: Option<String>,

    #[arg(long, env = "PROXYHAWK_API_EMAIL")]
    api_email: Option<String>,

    #[arg(long, env = "PROXYHAWK_API_PASSWORD")]
    api_password: Option<String>,

    /// POST run results to backend after execution
    #[arg(long, default_value_t = true)]
    submit: bool,

    /// Per-request timeout seconds
    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,

    /// Print JSON summary to stdout
    #[arg(long, default_value_t = false)]
    json: bool,

    /// When to print response bodies in logs: never, failures (broken/errors), or all
    #[arg(long, value_enum, default_value_t = PrintResponses::All, env = "GUARD_PRINT_RESPONSES")]
    print_responses: PrintResponses,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let bundle = load_bundle(&args).await?;
    let base_url = args
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let summary = run_bundle(&bundle, base_url, args.timeout_secs).await?;

    if args.json {
        let json_summary = json_summary(&summary, args.print_responses);
        println!("{}", serde_json::to_string_pretty(&json_summary)?);
    } else {
        print_human(&summary, args.print_responses);
    }

    if args.submit {
        let api_base = args
            .api_base_url
            .as_deref()
            .context("PROXYHAWK_API_BASE_URL required when --submit")?;
        let token = resolve_token(&args, api_base).await?;
        submit_run(api_base, &token, &summary).await?;
        eprintln!("Submitted Guard run to backend (outcome={})", summary.outcome());
    }

    std::process::exit(summary.exit_code());
}

async fn load_bundle(args: &Args) -> Result<bundle::HeadlessBundle> {
    if let Some(path) = &args.bundle {
        return load_bundle_from_file(path).await;
    }
    let session_id = args
        .session_id
        .as_deref()
        .context("Provide --bundle or --session-id")?;
    let api_base = args
        .api_base_url
        .as_deref()
        .context("PROXYHAWK_API_BASE_URL required to fetch bundle")?;
    let token = resolve_token(args, api_base).await?;
    load_bundle_from_api(api_base, &token, session_id).await
}

async fn resolve_token(args: &Args, api_base: &str) -> Result<String> {
    if let Some(token) = &args.api_token {
        return Ok(token.clone());
    }
    let email = args
        .api_email
        .as_deref()
        .context("Set PROXYHAWK_API_TOKEN or PROXYHAWK_API_EMAIL + PROXYHAWK_API_PASSWORD")?;
    let password = args
        .api_password
        .as_deref()
        .context("Set PROXYHAWK_API_PASSWORD")?;
    login(api_base, email, password).await
}

fn print_human(summary: &RunSummary, print_responses: PrintResponses) {
    eprintln!(
        "ProxyHawk Guard headless run — {} ({})",
        summary.session_name, summary.session_id
    );
    eprintln!(
        "Results: total={} ok={} broken={} errors={} outcome={}",
        summary.total,
        summary.ok,
        summary.broken,
        summary.errors,
        summary.outcome()
    );
    for entry in &summary.results {
        eprintln!(
            "  [{}] {} {} — {}{}",
            entry.status,
            entry.method,
            entry.url,
            entry
                .status_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".into()),
            entry
                .error
                .as_ref()
                .map(|e| format!(" ({e})"))
                .unwrap_or_default()
        );
        if entry.violations.is_empty() {
            let mut seen = BTreeSet::new();
            for change in &entry.breaking_changes {
                if seen.insert(change) {
                    eprintln!("    break: {change}");
                }
            }
        }
        for issue in &entry.violations {
            eprintln!(
                "    violation: {}: {}",
                issue.code, issue.message
            );
        }
        for issue in &entry.warnings {
            eprintln!(
                "    warning: {}: {}",
                issue.code, issue.message
            );
        }
        if should_print_response(print_responses, &entry.status) {
            if let Some(preview) = &entry.response_preview {
                eprintln!("    response:");
                for line in preview.lines() {
                    eprintln!("      {line}");
                }
            }
        }
    }

    print_deployment_summary(summary);
}

fn should_print_response(mode: PrintResponses, status: &str) -> bool {
    match mode {
        PrintResponses::Never => false,
        PrintResponses::All => true,
        PrintResponses::Failures => matches!(status, "broken" | "networkError" | "authExpired"),
    }
}

fn json_summary(summary: &RunSummary, print_responses: PrintResponses) -> serde_json::Value {
    let results: Vec<serde_json::Value> = summary
        .results
        .iter()
        .map(|entry| {
            let mut obj = serde_json::json!({
                "endpointId": entry.endpoint_id,
                "method": entry.method,
                "url": entry.url,
                "status": entry.status,
                "statusCode": entry.status_code,
                "breakingChanges": entry.breaking_changes,
                "violations": entry.violations,
                "warnings": entry.warnings,
                "error": entry.error,
            });
            if should_print_response(print_responses, &entry.status) {
                if let Some(preview) = &entry.response_preview {
                    obj["responsePreview"] = serde_json::Value::String(preview.clone());
                }
            }
            obj
        })
        .collect();
    serde_json::json!({
        "sessionId": summary.session_id,
        "sessionName": summary.session_name,
        "runAt": summary.run_at,
        "total": summary.total,
        "ok": summary.ok,
        "broken": summary.broken,
        "errors": summary.errors,
        "outcome": summary.outcome(),
        "results": results,
    })
}
