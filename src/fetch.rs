use std::path::Path;
use std::time::SystemTime;

use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::state::{UsageState, UsageStatus};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

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

fn load_credentials(path: &Path) -> Option<OauthCredentials> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "credentials file not readable");
            return None;
        }
    };

    let file: CredentialsFile = match serde_json::from_slice(&bytes) {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "credentials file present but failed to parse");
            return None;
        }
    };

    let now_millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;

    if file.oauth.expires_at <= now_millis {
        debug!("credentials expired");
        return None;
    }

    Some(file.oauth)
}

async fn detect_claude_version(fallback: &str) -> String {
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

    match version {
        Some(v) => {
            info!(version = %v, "detected installed claude code version");
            v
        }
        None => {
            warn!(
                fallback,
                "could not detect claude code version, using fallback"
            );
            fallback.into()
        }
    }
}

async fn poll_once(client: &reqwest::Client, user_agent: &str, credentials_path: &Path) -> Outcome {
    let Some(creds) = load_credentials(credentials_path) else {
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
        Err(e) => {
            warn!(error = %e, "usage request failed");
            return Outcome::Transient(e.to_string());
        }
    };

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        info!("access token rejected (401), needs login");
        return Outcome::NeedsLogin;
    }
    if !response.status().is_success() {
        warn!(status = %response.status(), "usage endpoint returned non-success status");
        return Outcome::Transient(format!("HTTP {}", response.status()));
    }

    match response.json::<UsageResponse>().await {
        Ok(usage) => Outcome::Ok(usage),
        Err(e) => {
            warn!(error = %e, "failed to parse usage response");
            Outcome::Transient(e.to_string())
        }
    }
}

fn parse_reset(s: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(SystemTime::from)
}

pub async fn run(tx: watch::Sender<UsageState>, config: Config) {
    let client = reqwest::Client::builder()
        .timeout(config.request_timeout())
        .build()
        .expect("failed to build HTTP client");

    let version = detect_claude_version(&config.claude_fallback_version).await;
    let user_agent = format!("claude-code/{}", version);
    let credentials_path = config.credentials_path();
    let base_interval = config.poll_interval();
    let max_interval = config.max_poll_interval();

    let mut delay = base_interval;

    loop {
        let outcome = poll_once(&client, &user_agent, &credentials_path).await;

        let next_state = match outcome {
            Outcome::Ok(usage) => {
                delay = base_interval;
                let five_hour = usage.five_hour.as_ref().and_then(|w| w.utilization);
                let weekly = usage.seven_day.as_ref().and_then(|w| w.utilization);
                debug!(five_hour = ?five_hour, weekly = ?weekly, "usage updated");
                UsageState {
                    five_hour_usage: five_hour,
                    five_hour_resets_at: usage
                        .five_hour
                        .as_ref()
                        .and_then(|w| w.resets_at.as_deref())
                        .and_then(parse_reset),
                    weekly_usage: weekly,
                    weekly_resets_at: usage
                        .seven_day
                        .as_ref()
                        .and_then(|w| w.resets_at.as_deref())
                        .and_then(parse_reset),
                    status: UsageStatus::Ok,
                }
            }
            Outcome::Transient(msg) => {
                delay = (delay * 2).min(max_interval);
                warn!(
                    error = %msg,
                    next_retry_secs = delay.as_secs(),
                    "transient error polling usage, backing off"
                );
                let mut state = tx.borrow().clone();
                state.status = UsageStatus::Error(msg);
                state
            }
            Outcome::NeedsLogin => {
                delay = base_interval;
                UsageState {
                    status: UsageStatus::NeedsLogin,
                    ..Default::default()
                }
            }
        };

        if tx.send(next_state).is_err() {
            debug!("state receiver dropped, stopping fetch loop");
            return;
        }

        sleep(delay).await;
    }
}
