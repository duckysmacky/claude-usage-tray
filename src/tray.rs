use std::time::SystemTime;

use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip};
use tokio::sync::mpsc;

use crate::state::{UsageState, UsageStatus};

pub struct UsageTray {
    state: UsageState,
    quit_tx: mpsc::UnboundedSender<()>,
}

impl UsageTray {
    pub fn new(quit_tx: mpsc::UnboundedSender<()>) -> Self {
        Self {
            state: UsageState::default(),
            quit_tx,
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
        ToolTip {
            title: "Claude Usage".into(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![
            header_item(&self.state.status),
            MenuItem::Separator
        ];
        items.extend(usage_rows(&self.state));
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

fn usage_rows(state: &UsageState) -> Vec<MenuItem<UsageTray>> {
    match &state.status {
        UsageStatus::Loading => vec![
            label_item("Loading...".into())
        ],
        UsageStatus::NeedsLogin => vec![
            label_item("Not logged in - run `claude auth` to authenticate".into())
        ],
        UsageStatus::Error(e) => vec![
            label_item(format!("Error: {}", escape(e)))
        ],
        UsageStatus::Ok => {
            let five_hour = state
                .five_hour_usage
                .map(|pct| {
                    let reset = state
                        .five_hour_resets_at
                        .map(fmt_reset)
                        .unwrap_or_else(|| "unknown".into());
                    format!("5-hour: {:.0}% · resets {}", pct, reset)
                })
                .unwrap_or_else(|| "5-hour: no data".into());

            let weekly = state
                .weekly_usage
                .map(|pct| {
                    let reset = state
                        .weekly_resets_at
                        .map(fmt_reset)
                        .unwrap_or_else(|| "unknown".into());
                    format!("Weekly: {:.0}% · resets {}", pct, reset)
                })
                .unwrap_or_else(|| "Weekly: no data".into());

            vec![
                label_item(five_hour),
                label_item(weekly)
            ]
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
