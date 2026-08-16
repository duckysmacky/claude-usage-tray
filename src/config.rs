use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const CONFIG_DIR: &str = ".config/claude-usage-tray";
const CONFIG_FILE: &str = "config.toml";

const DEFAULT_POLL_INTERVAL_SECS: u64 = 90;
const DEFAULT_MAX_POLL_INTERVAL_SECS: u64 = 600;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
const DEFAULT_CREDENTIALS_PATH: &str = "~/.claude/.credentials.json";
const DEFAULT_CLAUDE_FALLBACK_VERSION: &str = "2.1.228";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub poll_interval_secs: u64,
    pub max_poll_interval_secs: u64,
    pub request_timeout_secs: u64,
    pub credentials_path: String,
    pub claude_fallback_version: String,
    pub display: DisplayConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            poll_interval_secs: DEFAULT_POLL_INTERVAL_SECS,
            max_poll_interval_secs: DEFAULT_MAX_POLL_INTERVAL_SECS,
            request_timeout_secs: DEFAULT_REQUEST_TIMEOUT_SECS,
            credentials_path: DEFAULT_CREDENTIALS_PATH.into(),
            claude_fallback_version: DEFAULT_CLAUDE_FALLBACK_VERSION.into(),
            display: DisplayConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub show_five_hour_usage: bool,
    pub show_weekly_usage: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            show_five_hour_usage: true,
            show_weekly_usage: true,
        }
    }
}

impl Config {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_interval_secs)
    }

    pub fn max_poll_interval(&self) -> Duration {
        Duration::from_secs(self.max_poll_interval_secs)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    pub fn credentials_path(&self) -> PathBuf {
        expand_tilde(&self.credentials_path)
    }
}

fn home_dir() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME must be set"))
}

fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home_dir().join(rest),
        None => PathBuf::from(path),
    }
}

fn config_path() -> PathBuf {
    home_dir().join(CONFIG_DIR).join(CONFIG_FILE)
}

pub fn load() -> Config {
    let path = config_path();

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str(&contents) {
            Ok(config) => {
                info!(path = %path.display(), "loaded config");
                config
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "config file invalid, using defaults for this run");
                Config::default()
            }
        },
        Err(_) => {
            let config = Config::default();
            if let Err(e) = write_default(&path, &config) {
                warn!(path = %path.display(), error = %e, "could not write default config file");
            } else {
                info!(path = %path.display(), "created default config file");
            }
            config
        }
    }
}

fn write_default(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml = toml::to_string_pretty(config).expect("Config always serializes");
    std::fs::write(path, toml)
}
