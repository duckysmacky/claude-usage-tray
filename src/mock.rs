// TEMPORARY: stands in for the real usage fetcher. Delete this file (and its
// `mod mock;` / spawn call in main.rs) once credential reading + the Anthropic
// API call land.

use std::time::{Duration, SystemTime};

use tokio::sync::watch;
use tokio::time::interval;

use crate::state::{UsageState, UsageStatus};

pub async fn run(tx: watch::Sender<UsageState>) {
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut tick = 0u32;
    let mut ticker = interval(Duration::from_secs(5));

    loop {
        ticker.tick().await;
        tick += 1;

        let state = match tick % 8 {
            6 => UsageState {
                status: UsageStatus::NeedsLogin,
                ..Default::default()
            },
            7 => UsageState {
                status: UsageStatus::Error("could not reach Anthropic API".into()),
                ..Default::default()
            },
            _ => UsageState {
                five_hour_usage: Some(((tick * 13) % 100) as f64),
                five_hour_resets_at: Some(SystemTime::now() + Duration::from_secs(60 * 86)),
                weekly_usage: Some(((tick * 7) % 100) as f64),
                weekly_resets_at: Some(
                    SystemTime::now() + Duration::from_secs(3 * 86400 + 4 * 3600),
                ),
                status: UsageStatus::Ok,
            },
        };

        if tx.send(state).is_err() {
            return;
        }
    }
}
