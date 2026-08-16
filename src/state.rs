use std::time::SystemTime;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageState {
    pub five_hour_usage: Option<f64>,
    pub five_hour_resets_at: Option<SystemTime>,
    pub weekly_usage: Option<f64>,
    pub weekly_resets_at: Option<SystemTime>,
    pub plan: Option<String>,
    pub account_email: Option<String>,
    pub status: UsageStatus,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum UsageStatus {
    #[default]
    Loading,
    Ok,
    NeedsLogin,
    Error(String),
}
