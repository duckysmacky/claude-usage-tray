use std::time::SystemTime;

use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip};
use tokio::sync::mpsc;

use crate::config::DisplayConfig;
use crate::state::{Money, UsageState, UsageStatus};

const NEEDS_LOGIN_MSG: &str = "Not logged in - run `claude auth` to authenticate";

pub struct UsageTray {
    state: UsageState,
    quit_tx: mpsc::UnboundedSender<()>,
    display: DisplayConfig,
}

impl UsageTray {
    pub fn new(quit_tx: mpsc::UnboundedSender<()>, display: DisplayConfig) -> Self {
        Self {
            state: UsageState::default(),
            quit_tx,
            display,
        }
    }

    pub fn set_state(&mut self, state: UsageState) {
        self.state = state;
    }
}

impl ksni::Tray for UsageTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "Claude Usage".into()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        match self.state.status {
            UsageStatus::Loading => Status::Passive,
            UsageStatus::Ok => Status::Active,
            UsageStatus::NeedsLogin | UsageStatus::Error(_) => Status::NeedsAttention,
        }
    }

    fn icon_name(&self) -> String {
        icon_for(&self.state.status).into()
    }

    fn attention_icon_name(&self) -> String {
        icon_for(&self.state.status).into()
    }

    fn tool_tip(&self) -> ToolTip {
        let description = match &self.state.status {
            UsageStatus::Error(e) => format!("Unreachable: {}", escape(e)),
            UsageStatus::NeedsLogin => NEEDS_LOGIN_MSG.into(),
            UsageStatus::Ok | UsageStatus::Loading => String::new(),
        };
        ToolTip {
            title: "Claude Usage".into(),
            description,
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![header_item(&self.state.status), MenuItem::Separator];

        let account = account_rows(&self.state, &self.display);
        if !account.is_empty() {
            items.extend(account);
            items.push(MenuItem::Separator);
        }

        let usage = usage_rows(&self.state, &self.display);
        if !usage.is_empty() {
            items.extend(usage);
            items.push(MenuItem::Separator);
        }

        let credits = credits_rows(&self.state, &self.display);
        if !credits.is_empty() {
            items.extend(credits);
            items.push(MenuItem::Separator);
        }

        items.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut UsageTray| {
                    let _ = tray.quit_tx.send(());
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

fn icon_for(status: &UsageStatus) -> &'static str {
    match status {
        UsageStatus::Loading => "image-loading",
        UsageStatus::Ok => "utilities-system-monitor",
        UsageStatus::NeedsLogin => "dialog-password",
        UsageStatus::Error(_) => "dialog-error",
    }
}

fn header_item(status: &UsageStatus) -> MenuItem<UsageTray> {
    StandardItem {
        label: "Claude Usage".into(),
        icon_name: icon_for(status).into(),
        enabled: false,
        activate: Box::new(|_| {}),
        ..Default::default()
    }
    .into()
}

fn label_item(label: String) -> MenuItem<UsageTray> {
    StandardItem {
        label,
        enabled: false,
        activate: Box::new(|_| {}),
        ..Default::default()
    }
    .into()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn account_rows(state: &UsageState, display: &DisplayConfig) -> Vec<MenuItem<UsageTray>> {
    let cfg = &display.account;
    let mut rows = Vec::new();
    if cfg.show && cfg.show_plan {
        let plan = state
            .plan
            .as_deref()
            .map(capitalize)
            .unwrap_or_else(|| "—".into());
        rows.push(label_item(format!("Plan: {}", plan)));
    }
    if cfg.show && cfg.show_email {
        rows.push(label_item(
            state.account_email.clone().unwrap_or_else(|| "—".into()),
        ));
    }
    rows
}

fn money_line(label: &str, money: Option<&Money>, stale: bool) -> String {
    match money {
        Some(m) => {
            let suffix = if stale { " (last known)" } else { "" };
            format!("{}: {:.2} {}{}", label, m.amount, m.currency, suffix)
        }
        None => format!("{}: —", label),
    }
}

fn credits_rows(state: &UsageState, display: &DisplayConfig) -> Vec<MenuItem<UsageTray>> {
    let cfg = &display.credits;
    let stale = matches!(state.status, UsageStatus::Error(_));
    let mut rows = Vec::new();
    if cfg.show && cfg.show_spent {
        rows.push(label_item(money_line(
            "Spent",
            state.credits_spent.as_ref(),
            stale,
        )));
    }
    if cfg.show && cfg.show_limit {
        rows.push(label_item(money_line(
            "Limit",
            state.credits_limit.as_ref(),
            stale,
        )));
    }
    if cfg.show && cfg.show_total {
        rows.push(label_item(money_line(
            "Total credits",
            state.credits_total.as_ref(),
            stale,
        )));
    }
    rows
}

fn usage_line(label: &str, pct: Option<f64>, reset: Option<SystemTime>, stale: bool) -> String {
    match pct {
        Some(pct) => {
            let reset = reset.map(fmt_reset).unwrap_or_else(|| "unknown".into());
            let suffix = if stale { " (last known)" } else { "" };
            format!("{}: {:.0}% · resets {}{}", label, pct, reset, suffix)
        }
        None => format!("{}: no data", label),
    }
}

fn usage_rows(state: &UsageState, display: &DisplayConfig) -> Vec<MenuItem<UsageTray>> {
    let cfg = &display.usage;
    if !cfg.show || (!cfg.show_five_hour && !cfg.show_weekly) {
        return Vec::new();
    }

    match &state.status {
        UsageStatus::Loading => vec![label_item("Loading...".into())],
        UsageStatus::NeedsLogin => vec![label_item(NEEDS_LOGIN_MSG.into())],
        UsageStatus::Error(e) => {
            let mut rows = Vec::new();
            if cfg.show_five_hour && state.five_hour_usage.is_some() {
                rows.push(label_item(usage_line(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    true,
                )));
            }
            if cfg.show_weekly && state.weekly_usage.is_some() {
                rows.push(label_item(usage_line(
                    "Weekly",
                    state.weekly_usage,
                    state.weekly_resets_at,
                    true,
                )));
            }
            if rows.is_empty() {
                rows.push(label_item(format!("Error: {}", escape(e))));
            }
            rows
        }
        UsageStatus::Ok => {
            let mut rows = Vec::new();
            if cfg.show_five_hour {
                rows.push(label_item(usage_line(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    false,
                )));
            }
            if cfg.show_weekly {
                rows.push(label_item(usage_line(
                    "Weekly",
                    state.weekly_usage,
                    state.weekly_resets_at,
                    false,
                )));
            }
            rows
        }
    }
}

fn fmt_reset(at: SystemTime) -> String {
    match at.duration_since(SystemTime::now()) {
        Ok(d) => {
            let secs = d.as_secs();
            if secs < 60 {
                "in <1m".into()
            } else if secs < 3600 {
                format!("in {}m", secs / 60)
            } else if secs < 86400 {
                format!("in {}h {}m", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("in {}d {}h", secs / 86400, (secs % 86400) / 3600)
            }
        }
        Err(_) => "now".into(),
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
