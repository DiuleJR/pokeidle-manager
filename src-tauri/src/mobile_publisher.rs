use crate::{accounts::AccountManager, market::MarketRuntime, mobile::MobileSnapshot};
use std::{sync::Arc, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time::MissedTickBehavior};
use tokio_util::sync::CancellationToken;

const COALESCE_QUIET_PERIOD: Duration = Duration::from_millis(100);
const COALESCE_MAX_PERIOD: Duration = Duration::from_millis(250);
const METRICS_REFRESH_PERIOD: Duration = Duration::from_secs(60);
const SNAPSHOT_LOCK_RETRY: Duration = Duration::from_millis(40);

#[derive(Debug)]
pub struct PublishedMobileSnapshot {
    pub snapshot: Arc<MobileSnapshot>,
    pub json: Arc<str>,
}

impl PublishedMobileSnapshot {
    pub fn new(snapshot: MobileSnapshot) -> Result<Self, serde_json::Error> {
        let json: Arc<str> = serde_json::to_string(&snapshot)?.into();
        Ok(Self {
            snapshot: Arc::new(snapshot),
            json,
        })
    }
}

pub fn initial_snapshot(
    accounts: &AccountManager,
    market: &MarketRuntime,
) -> Result<Arc<PublishedMobileSnapshot>, serde_json::Error> {
    let mut snapshot = accounts.mobile_snapshot();
    snapshot.market = market.try_mobile_snapshot().unwrap_or_default();
    Ok(Arc::new(PublishedMobileSnapshot::new(snapshot)?))
}

pub fn spawn_publisher(
    accounts: AccountManager,
    market: MarketRuntime,
    sender: watch::Sender<Arc<PublishedMobileSnapshot>>,
    cancellation: CancellationToken,
) -> JoinHandle<()> {
    let mut changes = accounts.subscribe_mobile_changes();
    let mut market_changes = market.subscribe_changes();
    tokio::spawn(async move {
        let mut metric_refresh = tokio::time::interval(METRICS_REFRESH_PERIOD);
        metric_refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // The first interval tick is immediate; the initial snapshot was
        // already published before this task was started.
        metric_refresh.tick().await;

        let mut revision = sender.borrow().snapshot.revision;
        let mut retry_at = None;
        if !publish(&accounts, &market, &sender, &mut revision, true) {
            retry_at = Some(tokio::time::Instant::now() + SNAPSHOT_LOCK_RETRY);
        }

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                changed = changes.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    if !coalesce_changes(&mut changes, &mut market_changes, &cancellation).await {
                        break;
                    }
                    if !publish(&accounts, &market, &sender, &mut revision, true) {
                        retry_at = Some(tokio::time::Instant::now() + SNAPSHOT_LOCK_RETRY);
                    } else {
                        retry_at = None;
                    }
                }
                changed = market_changes.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    if !coalesce_changes(&mut changes, &mut market_changes, &cancellation).await {
                        break;
                    }
                    if !publish(&accounts, &market, &sender, &mut revision, false) {
                        retry_at = Some(tokio::time::Instant::now() + SNAPSHOT_LOCK_RETRY);
                    } else {
                        retry_at = None;
                    }
                }
                _ = metric_refresh.tick() => {
                    if !publish(&accounts, &market, &sender, &mut revision, true) {
                        retry_at = Some(tokio::time::Instant::now() + SNAPSHOT_LOCK_RETRY);
                    } else {
                        retry_at = None;
                    }
                }
                _ = async {
                    if let Some(deadline) = retry_at {
                        tokio::time::sleep_until(deadline).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    if !publish(&accounts, &market, &sender, &mut revision, true) {
                        retry_at = Some(tokio::time::Instant::now() + SNAPSHOT_LOCK_RETRY);
                    } else {
                        retry_at = None;
                    }
                }
            }
        }
        tracing::debug!("mobile snapshot publisher stopped");
    })
}

async fn coalesce_changes(
    changes: &mut watch::Receiver<u64>,
    market_changes: &mut watch::Receiver<u64>,
    cancellation: &CancellationToken,
) -> bool {
    let started = tokio::time::Instant::now();
    let maximum = started + COALESCE_MAX_PERIOD;
    loop {
        let quiet_deadline = (tokio::time::Instant::now() + COALESCE_QUIET_PERIOD).min(maximum);
        tokio::select! {
            _ = cancellation.cancelled() => return false,
            _ = tokio::time::sleep_until(quiet_deadline) => return true,
            changed = changes.changed() => {
                if changed.is_err() {
                    return false;
                }
            }
            changed = market_changes.changed() => {
                if changed.is_err() {
                    return false;
                }
            }
        }
    }
}

fn publish(
    accounts: &AccountManager,
    market: &MarketRuntime,
    sender: &watch::Sender<Arc<PublishedMobileSnapshot>>,
    revision: &mut u64,
    force: bool,
) -> bool {
    let Some(market_snapshot) = market.try_mobile_snapshot() else {
        return false;
    };
    if !force && sender.borrow().snapshot.market == market_snapshot {
        return true;
    }
    let projection_started = std::time::Instant::now();
    let mut snapshot = accounts.mobile_snapshot();
    snapshot.market = market_snapshot;
    *revision = revision.saturating_add(1);
    snapshot.revision = *revision;
    let projection_us = projection_started.elapsed().as_micros();
    let serialization_started = std::time::Instant::now();
    match PublishedMobileSnapshot::new(snapshot) {
        Ok(published) => {
            let bytes = published.json.len();
            let serialization_us = serialization_started.elapsed().as_micros();
            let published_revision = *revision;
            sender.send_replace(Arc::new(published));
            tracing::debug!(
                revision = published_revision,
                payload_bytes = bytes,
                projection_us,
                serialization_us,
                "mobile snapshot published"
            );
            true
        }
        Err(error) => {
            tracing::warn!(%error, "could not serialize mobile snapshot");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_a_four_account_mobile_snapshot_without_runtime_side_effects() {
        let accounts = AccountManager::default();
        for (id, name) in [
            ("acct-1", "DemoTrainerFour"),
            ("acct-2", "DemoTrainerOne"),
            ("acct-3", "DemoTrainerThree"),
            ("acct-4", "DemoTrainerTwo"),
        ] {
            accounts
                .add(crate::accounts::new_record(
                    id.to_owned(),
                    name.to_owned(),
                    "#fff".to_owned(),
                ))
                .unwrap();
        }

        let iterations = 100;
        let projection_started = std::time::Instant::now();
        let mut snapshot = None;
        for _ in 0..iterations {
            snapshot = Some(accounts.mobile_snapshot());
        }
        let projection_us = projection_started.elapsed().as_micros() / iterations;
        let snapshot = snapshot.expect("100 projections ran");
        assert_eq!(snapshot.accounts.len(), 4);

        let serialization_started = std::time::Instant::now();
        let published = PublishedMobileSnapshot::new(snapshot).unwrap();
        let serialization_us = serialization_started.elapsed().as_micros();
        let payload_bytes = published.json.len();
        assert!(payload_bytes < 8_000);
        println!(
            "[mobile-perf synthetic-4-accounts] payload_bytes={payload_bytes} projection_avg_us={projection_us} serialization_us={serialization_us}"
        );
    }

    #[tokio::test]
    async fn publisher_updates_empty_initial_snapshot_when_four_accounts_arrive() {
        let accounts = AccountManager::default();
        let market = test_market_runtime();
        let initial = initial_snapshot(&accounts, &market).unwrap();
        assert!(initial.snapshot.accounts.is_empty());
        let (sender, mut receiver) = watch::channel(initial);
        let cancellation = CancellationToken::new();
        let task = spawn_publisher(accounts.clone(), market, sender, cancellation.clone());

        for (id, name) in [
            ("acct-1", "DemoTrainerFour"),
            ("acct-2", "DemoTrainerOne"),
            ("acct-3", "DemoTrainerThree"),
            ("acct-4", "DemoTrainerTwo"),
        ] {
            accounts
                .add(crate::accounts::new_record(
                    id.to_owned(),
                    name.to_owned(),
                    "#fff".to_owned(),
                ))
                .unwrap();
        }

        tokio::time::timeout(Duration::from_secs(2), receiver.changed())
            .await
            .expect("publisher updates after accounts arrive")
            .expect("publisher channel remains open");
        let published = receiver.borrow();
        let accounts = &published.snapshot.accounts;
        assert_eq!(accounts.len(), 4);
        for expected_name in [
            "DemoTrainerFour",
            "DemoTrainerOne",
            "DemoTrainerThree",
            "DemoTrainerTwo",
        ] {
            assert!(
                accounts
                    .iter()
                    .any(|account| account.display_name == expected_name)
            );
            assert!(
                published
                    .json
                    .contains(&format!("\"displayName\":\"{expected_name}\""))
            );
        }
        drop(published);

        cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("publisher task joins after cancellation")
            .expect("publisher task exits cleanly");
    }

    fn test_market_runtime() -> MarketRuntime {
        MarketRuntime::new(
            AccountManager::default(),
            std::sync::Arc::new(parking_lot::Mutex::new(
                rusqlite::Connection::open_in_memory().unwrap(),
            )),
        )
    }
}
