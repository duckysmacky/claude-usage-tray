use std::time::SystemTime;

use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip};
use tokio::sync::mpsc;

use crate::config::DisplayConfig;
use crate::state::{UsageState, UsageStatus};

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
        items.extend(usage_rows(&self.state, &self.display));
        items.push(MenuItem::Separator);
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
    match &state.status {
        UsageStatus::Loading => vec![label_item("Loading...".into())],
        UsageStatus::NeedsLogin => vec![label_item(NEEDS_LOGIN_MSG.into())],
        UsageStatus::Error(e) => {
            let mut rows = Vec::new();
            if display.show_five_hour_usage && state.five_hour_usage.is_some() {
                rows.push(label_item(usage_line(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    true,
                )));
            }
            if display.show_weekly_usage && state.weekly_usage.is_some() {
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
            if display.show_five_hour_usage {
                rows.push(label_item(usage_line(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    false,
                )));
            }
            if display.show_weekly_usage {
                rows.push(label_item(usage_line(
                    "Weekly",
                    state.weekly_usage,
                    state.weekly_resets_at,
                    false,
                )));
            }
            if rows.is_empty() {
                rows.push(label_item("No usage metrics enabled".into()));
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
