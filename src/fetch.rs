use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::sleep;

use crate::state::{UsageState, UsageStatus};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const FALLBACK_CLAUDE_VERSION: &str = "2.1.228";
const POLL_INTERVAL: Duration = Duration::from_secs(90);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    oauth: OauthCredentials,
}

#[derive(Deserialize)]
struct OauthCredentials {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
}

#[derive(Deserialize)]
struct UsageResponse {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

#[derive(Deserialize)]
struct Window {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

enum Outcome {
    Ok(UsageResponse),
    NeedsLogin,
    Transient(String),
}

fn credentials_path() -> PathBuf {
    let home = env::var("HOME").expect("HOME must be set");
    PathBuf::from(home).join(".claude/.credentials.json")
}

fn load_credentials() -> Option<OauthCredentials> {
    let bytes = std::fs::read(credentials_path()).ok()?;
    let file: CredentialsFile = serde_json::from_slice(&bytes).ok()?;
    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;

    if file.oauth.expires_at <= now_millis {
        return None;
    }

    Some(file.oauth)
}

async fn detect_claude_version() -> String {
    let output = Command::new("claude").arg("--version").output().await;
    let version = output.ok().and_then(|o| {
        if !o.status.success() {
            return None;
        }

        String::from_utf8(o.stdout)
            .ok()?
            .split_whitespace()
            .next()
            .map(str::to_owned)
    });

    version.unwrap_or_else(|| FALLBACK_CLAUDE_VERSION.into())
}

async fn poll_once(client: &reqwest::Client, user_agent: &str) -> Outcome {
    let Some(creds) = load_credentials() else {
        return Outcome::NeedsLogin;
    };

    let response = client
        .get(USAGE_URL)
        .bearer_auth(creds.access_token)
        .header("anthropic-beta", OAUTH_BETA)
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => return Outcome::Transient(e.to_string()),
    };

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Outcome::NeedsLogin;
    }
    if !response.status().is_success() {
        return Outcome::Transient(format!("HTTP {}", response.status()));
    }

    match response.json::<UsageResponse>().await {
        Ok(usage) => Outcome::Ok(usage),
        Err(e) => Outcome::Transient(e.to_string()),
    }
}

fn parse_reset(s: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(SystemTime::from)
}

pub async fn run(tx: watch::Sender<UsageState>) {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("failed to build HTTP client");

    let version = detect_claude_version().await;
    let user_agent = format!("claude-code/{}", version);

    let mut delay = POLL_INTERVAL;

    loop {
        let outcome = poll_once(&client, &user_agent).await;

        let next_state = match outcome {
            Outcome::Ok(usage) => {
                delay = POLL_INTERVAL;
                UsageState {
                    five_hour_usage: usage.five_hour.as_ref().and_then(|w| w.utilization),
                    five_hour_resets_at: usage
                        .five_hour
                        .as_ref()
                        .and_then(|w| w.resets_at.as_deref())
                        .and_then(parse_reset),
                    weekly_usage: usage.seven_day.as_ref().and_then(|w| w.utilization),
                    weekly_resets_at: usage
                        .seven_day
                        .as_ref()
                        .and_then(|w| w.resets_at.as_deref())
                        .and_then(parse_reset),
                    status: UsageStatus::Ok,
                }
            }
            Outcome::Transient(msg) => {
                delay = (delay * 2).min(MAX_POLL_INTERVAL);
                let mut state = tx.borrow().clone();
                state.status = UsageStatus::Error(msg);
                state
            }
            Outcome::NeedsLogin => {
                delay = POLL_INTERVAL;
                UsageState {
                    status: UsageStatus::NeedsLogin,
                    ..Default::default()
                }
            }
        };

        if tx.send(next_state).is_err() {
            return;
        }

        sleep(delay).await;
    }
}
