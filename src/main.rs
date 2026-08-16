mod fetch;
mod state;
mod tray;

use ksni::{Handle, TrayMethods};
use tokio::sync::{mpsc, watch};

use state::UsageState;
use tray::UsageTray;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (state_tx, state_rx) = watch::channel(UsageState::default());
    let (quit_tx, mut quit_rx) = mpsc::unbounded_channel();

    let handle = UsageTray::new(quit_tx).spawn().await?;
    let fetch_task = tokio::spawn(fetch::run(state_tx));
    let bridge_task = tokio::spawn(bridge(state_rx, handle.clone()));

    tokio::select! {
        _ = quit_rx.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
        _ = fetch_task => {}
        _ = bridge_task => {}
    }

    handle.shutdown().await;
    Ok(())
}

async fn bridge(mut rx: watch::Receiver<UsageState>, handle: Handle<UsageTray>) {
    while rx.changed().await.is_ok() {
        let next = rx.borrow_and_update().clone();

        if handle.update(move |t| t.set_state(next)).await.is_none() {
            break;
        }
    }
}
