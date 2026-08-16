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
use crate::state::{Money, UsageState, UsageStatus};

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
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

#[derive(Deserialize)]
struct AccountConfigFile {
    #[serde(rename = "oauthAccount")]
    oauth_account: Option<OauthAccount>,
}

#[derive(Deserialize)]
struct OauthAccount {
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
}

#[derive(Deserialize)]
struct UsageResponse {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
    spend: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Window {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

enum Outcome {
    Ok {
        usage: UsageResponse,
        plan: Option<String>,
        email: Option<String>,
    },
    NeedsLogin,
    Transient {
        message: String,
        plan: Option<String>,
        email: Option<String>,
    },
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

fn load_account_email(path: &Path) -> Option<String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            debug!(path = %path.display(), error = %e, "account config file not readable");
            return None;
        }
    };

    let file: AccountConfigFile = match serde_json::from_slice(&bytes) {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "account config file present but failed to parse");
            return None;
        }
    };

    file.oauth_account.and_then(|a| a.email_address)
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

async fn poll_once(
    client: &reqwest::Client,
    user_agent: &str,
    credentials_path: &Path,
    account_config_path: &Path,
) -> Outcome {
    let Some(creds) = load_credentials(credentials_path) else {
        return Outcome::NeedsLogin;
    };

    let plan = creds.subscription_type;
    let email = load_account_email(account_config_path);

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
            return Outcome::Transient {
                message: e.to_string(),
                plan,
                email,
            };
        }
    };

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        info!("access token rejected (401), needs login");
        return Outcome::NeedsLogin;
    }
    if !response.status().is_success() {
        warn!(status = %response.status(), "usage endpoint returned non-success status");
        return Outcome::Transient {
            message: format!("HTTP {}", response.status()),
            plan,
            email,
        };
    }

    match response.json::<UsageResponse>().await {
        Ok(usage) => Outcome::Ok { usage, plan, email },
        Err(e) => {
            warn!(error = %e, "failed to parse usage response");
            Outcome::Transient {
                message: e.to_string(),
                plan,
                email,
            }
        }
    }
}

fn parse_reset(s: &str) -> Option<SystemTime> {
    OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(SystemTime::from)
}

fn extract_money(v: &serde_json::Value) -> Option<Money> {
    let amount_minor = v.get("amount_minor")?.as_i64()?;
    let exponent = v.get("exponent")?.as_u64()? as i32;
    let currency = v.get("currency")?.as_str()?.to_string();
    Some(Money {
        amount: amount_minor as f64 / 10f64.powi(exponent),
        currency,
    })
}

fn extract_spend_field(spend: &serde_json::Value, key: &str) -> Option<Money> {
    let field = spend.get(key)?;
    if field.is_null() {
        debug!(field = key, "spend field is null");
        return None;
    }
    match extract_money(field) {
        Some(money) => Some(money),
        None => {
            debug!(
                field = key,
                "spend field present but not in expected money shape"
            );
            None
        }
    }
}

pub async fn run(tx: watch::Sender<UsageState>, config: Config) {
    let client = reqwest::Client::builder()
        .timeout(config.request_timeout())
        .build()
        .expect("failed to build HTTP client");

    let version = detect_claude_version(&config.claude_fallback_version).await;
    let user_agent = format!("claude-code/{}", version);
    let credentials_path = config.credentials_path();
    let account_config_path = config.account_config_path();
    let base_interval = config.poll_interval();
    let max_interval = config.max_poll_interval();

    let mut delay = base_interval;

    loop {
        let outcome = poll_once(
            &client,
            &user_agent,
            &credentials_path,
            &account_config_path,
        )
        .await;

        let next_state = match outcome {
            Outcome::Ok { usage, plan, email } => {
                delay = base_interval;
                let five_hour = usage.five_hour.as_ref().and_then(|w| w.utilization);
                let weekly = usage.seven_day.as_ref().and_then(|w| w.utilization);
                let credits_spent = usage
                    .spend
                    .as_ref()
                    .and_then(|s| extract_spend_field(s, "used"));
                let credits_limit = usage
                    .spend
                    .as_ref()
                    .and_then(|s| extract_spend_field(s, "limit"));
                let credits_total = usage
                    .spend
                    .as_ref()
                    .and_then(|s| extract_spend_field(s, "balance"));
                debug!(
                    five_hour = ?five_hour,
                    weekly = ?weekly,
                    plan = ?plan,
                    email = ?email,
                    credits_spent = ?credits_spent,
                    credits_limit = ?credits_limit,
                    credits_total = ?credits_total,
                    "usage updated"
                );
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
                    plan,
                    account_email: email,
                    credits_spent,
                    credits_limit,
                    credits_total,
                    status: UsageStatus::Ok,
                }
            }
            Outcome::Transient {
                message,
                plan,
                email,
            } => {
                delay = (delay * 2).min(max_interval);
                warn!(
                    error = %message,
                    next_retry_secs = delay.as_secs(),
                    "transient error polling usage, backing off"
                );
                let mut state = tx.borrow().clone();
                state.status = UsageStatus::Error(message);
                state.plan = plan;
                state.account_email = email;
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
