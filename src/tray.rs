use std::time::SystemTime;

use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip};
use tokio::sync::mpsc;

use crate::config::{DisplayConfig, UsageViewMode};
use crate::icons;
use crate::state::{Money, UsageState, UsageStatus};

const NEEDS_LOGIN_MSG: &str = "Not logged in or session expired";

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
        icons::icon_name(&self.state.status).into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        icons::pixmap(&self.state.status)
    }

    fn attention_icon_name(&self) -> String {
        icons::icon_name(&self.state.status).into()
    }

    fn attention_icon_pixmap(&self) -> Vec<ksni::Icon> {
        icons::pixmap(&self.state.status)
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

fn header_item(status: &UsageStatus) -> MenuItem<UsageTray> {
    StandardItem {
        label: "Claude Usage".into(),
        icon_name: icons::icon_name(status).into(),
        icon_data: icons::png(status),
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

fn normal_item(label: String) -> MenuItem<UsageTray> {
    StandardItem {
        label,
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
    if matches!(state.status, UsageStatus::NeedsLogin) {
        return Vec::new();
    }

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
    if matches!(state.status, UsageStatus::NeedsLogin) {
        return Vec::new();
    }

    let cfg = &display.credits;
    let stale = matches!(state.status, UsageStatus::Error(_));
    let mut rows = Vec::new();
    if cfg.show && cfg.show_spent {
        rows.push(normal_item(money_line(
            "Spent",
            state.credits_spent.as_ref(),
            stale,
        )));
    }
    if cfg.show && cfg.show_limit {
        rows.push(normal_item(money_line(
            "Limit",
            state.credits_limit.as_ref(),
            stale,
        )));
    }
    if cfg.show && cfg.show_total {
        rows.push(normal_item(money_line(
            "Total credits",
            state.credits_total.as_ref(),
            stale,
        )));
    }
    rows
}

fn usage_bar(pct: f64) -> String {
    const WIDTH: usize = 10;

    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * WIDTH as f64).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(WIDTH - filled))
}

fn usage_lines(
    label: &str,
    pct: Option<f64>,
    reset: Option<SystemTime>,
    stale: bool,
    view_mode: UsageViewMode,
) -> Vec<String> {
    match pct {
        Some(pct) => {
            let reset = reset.map(fmt_reset).unwrap_or_else(|| "unknown".into());
            let suffix = if stale { " (last known)" } else { "" };
            match view_mode {
                UsageViewMode::Simple => vec![format!(
                    "{}: {:.0}% · resets {}{}",
                    label, pct, reset, suffix
                )],
                UsageViewMode::Bars => vec![
                    format!("{}: {} {:.0}%", label, usage_bar(pct), pct),
                    format!("resets {}{}", reset, suffix),
                ],
            }
        }
        None => vec![format!("{}: no data", label)],
    }
}

fn usage_items(
    label: &str,
    pct: Option<f64>,
    reset: Option<SystemTime>,
    stale: bool,
    view_mode: UsageViewMode,
) -> Vec<MenuItem<UsageTray>> {
    let prominent_first_row = view_mode == UsageViewMode::Simple || pct.is_some();

    usage_lines(label, pct, reset, stale, view_mode)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if prominent_first_row && index == 0 {
                normal_item(line)
            } else {
                label_item(line)
            }
        })
        .collect()
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
                rows.extend(usage_items(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    true,
                    cfg.view_mode,
                ));
            }
            if cfg.show_weekly && state.weekly_usage.is_some() {
                rows.extend(usage_items(
                    "Weekly",
                    state.weekly_usage,
                    state.weekly_resets_at,
                    true,
                    cfg.view_mode,
                ));
            }
            if rows.is_empty() {
                rows.push(label_item(format!("Error: {}", escape(e))));
            }
            rows
        }
        UsageStatus::Ok => {
            let mut rows = Vec::new();
            if cfg.show_five_hour {
                rows.extend(usage_items(
                    "5-hour",
                    state.five_hour_usage,
                    state.five_hour_resets_at,
                    false,
                    cfg.view_mode,
                ));
            }
            if cfg.show_weekly {
                rows.extend(usage_items(
                    "Weekly",
                    state.weekly_usage,
                    state.weekly_resets_at,
                    false,
                    cfg.view_mode,
                ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_account_and_credits_when_login_is_required() {
        let state = UsageState {
            status: UsageStatus::NeedsLogin,
            ..Default::default()
        };
        let display = DisplayConfig::default();

        assert!(account_rows(&state, &display).is_empty());
        assert_eq!(usage_rows(&state, &display).len(), 1);
        assert!(credits_rows(&state, &display).is_empty());
    }

    #[test]
    fn formats_simple_usage_without_a_bar() {
        assert_eq!(
            usage_lines("5-hour", Some(42.0), None, false, UsageViewMode::Simple),
            ["5-hour: 42% · resets unknown"]
        );
    }

    #[test]
    fn formats_bar_and_reset_as_separate_rows() {
        assert_eq!(
            usage_lines("Weekly", Some(42.0), None, false, UsageViewMode::Bars),
            ["Weekly: ████░░░░░░ 42%", "resets unknown"]
        );
    }

    #[test]
    fn makes_only_the_bar_row_prominent() {
        let items = usage_items("Weekly", Some(42.0), None, false, UsageViewMode::Bars);

        let MenuItem::Standard(bar) = &items[0] else {
            panic!("bar row should be a standard item");
        };
        let MenuItem::Standard(reset) = &items[1] else {
            panic!("reset row should be a standard item");
        };

        assert!(bar.enabled);
        assert!(!reset.enabled);
    }

    #[test]
    fn makes_the_simple_usage_row_prominent() {
        let items = usage_items("5-hour", Some(42.0), None, false, UsageViewMode::Simple);

        let MenuItem::Standard(usage) = &items[0] else {
            panic!("usage row should be a standard item");
        };

        assert!(usage.enabled);
    }

    #[test]
    fn makes_the_title_and_credits_normal_items() {
        let MenuItem::Standard(title) = header_item(&UsageStatus::Ok) else {
            panic!("title should be a standard item");
        };

        let mut display = DisplayConfig::default();
        display.credits.show = true;
        let credits = credits_rows(&UsageState::default(), &display);

        assert!(title.enabled);
        assert_eq!(credits.len(), 3);
        assert!(
            credits
                .iter()
                .all(|item| { matches!(item, MenuItem::Standard(credit) if credit.enabled) })
        );
    }

    #[test]
    fn clamps_percentage_bar_to_its_bounds() {
        assert_eq!(usage_bar(-10.0), "░░░░░░░░░░");
        assert_eq!(usage_bar(150.0), "██████████");
    }
}
