use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct Money {
    pub amount: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageState {
    pub five_hour_usage: Option<f64>,
    pub five_hour_resets_at: Option<SystemTime>,
    pub weekly_usage: Option<f64>,
    pub weekly_resets_at: Option<SystemTime>,
    pub plan: Option<String>,
    pub account_email: Option<String>,
    pub credits_spent: Option<Money>,
    pub credits_limit: Option<Money>,
    pub credits_total: Option<Money>,
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
