mod config;
mod fetch;
mod state;
mod tray;

use ksni::{Handle, TrayMethods};
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use state::UsageState;
use tray::UsageTray;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = config::load();

    let (state_tx, state_rx) = watch::channel(UsageState::default());
    let (quit_tx, mut quit_rx) = mpsc::unbounded_channel();

    let handle = match UsageTray::new(quit_tx, config.display.clone())
        .spawn()
        .await
    {
        Ok(handle) => handle,
        Err(e) => {
            error!(error = %e, "failed to spawn tray (no StatusNotifierItem host running?)");
            return Err(e.into());
        }
    };
    info!("tray spawned");

    let fetch_task = tokio::spawn(fetch::run(state_tx, config.clone()));
    let bridge_task = tokio::spawn(bridge(state_rx, handle.clone()));

    tokio::select! {
        _ = quit_rx.recv() => info!("quit requested via tray menu"),
        _ = tokio::signal::ctrl_c() => info!("received ctrl-c, shutting down"),
        _ = fetch_task => warn!("fetch task exited unexpectedly"),
        _ = bridge_task => warn!("bridge task exited unexpectedly"),
    }

    handle.shutdown().await;
    info!("shutdown complete");
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn bridge(mut rx: watch::Receiver<UsageState>, handle: Handle<UsageTray>) {
    while rx.changed().await.is_ok() {
        let next = rx.borrow_and_update().clone();

        if handle.update(move |t| t.set_state(next)).await.is_none() {
            warn!("tray handle closed, stopping state bridge");
            break;
        }
    }
}
