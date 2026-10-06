mod accounts;
mod assets;
mod automations;
mod browser;
mod community;
mod connection;
mod domain;
mod events;
mod inspector;
mod logging;
mod market;
mod metrics;
#[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
mod mobile;
#[cfg(all(debug_assertions, feature = "mobile-local-server"))]
mod mobile_publisher;
#[cfg(all(debug_assertions, feature = "mobile-local-server"))]
mod mobile_server;
mod mock;
mod persistence;
mod protocol;

use accounts::AccountManager;
use connection::ConnectionManager;
use domain::{
    AccountMode, AccountRecord, AccountRuntimeState, AccountSnapshot, CaptureMode, ConnectionOwner,
    ConnectionStatus, HuntTimerState, MAX_ACCOUNTS,
};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{
    Emitter, Manager, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::PageLoadEvent,
};
use tokio::sync::Semaphore;

#[cfg(not(feature = "community-build"))]
compile_error!("This source export requires the explicit community-build feature.");

#[cfg(all(not(debug_assertions), feature = "mobile-local-server"))]
compile_error!("mobile-local-server is development-only");
const DEFAULT_GAME_URL: &str = "https://pokeidle.io/app";
const DEFAULT_STARTUP_BROWSER_CONCURRENCY: u8 = 1;
const MAX_STARTUP_BROWSER_CONCURRENCY: u8 = MAX_ACCOUNTS as u8;
const BRAVE_DOWNLOAD_URL: &str = "https://brave.com/pt-br/download/";
static APPLICATION_STARTED: OnceLock<Instant> = OnceLock::new();

// Browser contains a few development-only validation instrumentation points.
// The public Community export does not include that harness, so keep the
// instrumentation permanently disabled while retaining ordinary recovery.
pub(crate) const fn recovery_validation_mode_active() -> bool {
    false
}

fn startup_mark(event: &str) {
    let elapsed_ms = APPLICATION_STARTED
        .get()
        .map(|started| started.elapsed().as_millis());
    tracing::info!(event, ?elapsed_ms, "startup");
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    minimize_to_tray: bool,
    game_url: String,
    #[serde(default = "default_startup_browser_concurrency")]
    startup_browser_concurrency: u8,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct LocalAutomationPreferences {
    potion_ids: Vec<u64>,
    /// The order is the configured Ball priority for this account.
    ball_ids: Vec<u64>,
    /// Manager-local capture mode: the game only receives individual Ball
    /// throws, so this setting must survive independently for each account.
    #[serde(default)]
    capture_mode: CaptureMode,
}
impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            minimize_to_tray: true,
            game_url: DEFAULT_GAME_URL.into(),
            startup_browser_concurrency: default_startup_browser_concurrency(),
        }
    }
}
fn default_startup_browser_concurrency() -> u8 {
    DEFAULT_STARTUP_BROWSER_CONCURRENCY
}

fn normalize_startup_browser_concurrency(value: u8) -> u8 {
    if (1..=MAX_STARTUP_BROWSER_CONCURRENCY).contains(&value) {
        value
    } else {
        DEFAULT_STARTUP_BROWSER_CONCURRENCY
    }
}

impl PersistedSettings {
    fn normalized(mut self) -> Self {
        self.startup_browser_concurrency =
            normalize_startup_browser_concurrency(self.startup_browser_concurrency);
        self
    }

    fn startup_bootstrap_concurrency(&self) -> usize {
        self.startup_browser_concurrency as usize
    }
}
#[derive(Clone)]
struct AppCore {
    lifecycle: community::ApplicationLifecycle,
    accounts: AccountManager,
    settings: Arc<Mutex<PersistedSettings>>,
    database: Arc<Mutex<Connection>>,
    app_data_dir: PathBuf,
    diagnostic: Arc<Mutex<browser::IntegrationDiagnostic>>,
    assets: assets::GameAssetService,
    market: market::MarketRuntime,
    bootstrap_gate: Arc<Semaphore>,
    bootstrap_scheduled: Arc<AtomicBool>,
    #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
    mobile_bridge: Arc<Mutex<Option<MobileBridgeRuntime>>>,
}

#[cfg(all(debug_assertions, feature = "mobile-local-server"))]
struct MobileBridgeRuntime {
    cancellation: tokio_util::sync::CancellationToken,
    server: mobile_server::MobileServerHandle,
    publisher: tokio::task::JoinHandle<()>,
}

#[cfg(all(debug_assertions, feature = "mobile-local-server"))]
impl MobileBridgeRuntime {
    async fn shutdown(self) {
        self.cancellation.cancel();
        self.server.shutdown().await;
        if let Err(error) = self.publisher.await {
            tracing::warn!(%error, "mobile publisher task did not join cleanly");
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCoreStatus {
    ready: bool,
    error: Option<String>,
}

#[derive(Clone)]
struct AppCoreReadiness(Arc<Mutex<AppCoreStatus>>);

impl Default for AppCoreReadiness {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(AppCoreStatus {
            ready: false,
            error: None,
        })))
    }
}

impl AppCoreReadiness {
    fn set(&self, status: AppCoreStatus) {
        *self.0.lock() = status;
    }

    fn snapshot(&self) -> AppCoreStatus {
        self.0.lock().clone()
    }
}

impl AppCore {
    fn new(
        database: Connection,
        app_data_dir: PathBuf,
        lifecycle: community::ApplicationLifecycle,
    ) -> Result<Self, rusqlite::Error> {
        startup_mark("AppCore construction start");
        startup_mark("app settings load start");
        let settings = load_settings(&database)?;
        startup_mark("app settings load end");
        startup_mark("persisted accounts load start");
        let accounts = AccountManager::default();
        for record in load_accounts(&database)? {
            let account_id = record.id.clone();
            accounts
                .add(record)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            if let Some(timer) = load_hunt_timer(&database, &account_id)? {
                accounts
                    .restore_hunt_timer(&account_id, timer)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
            }
            let rules = load_auto_buy_rules(&database, &account_id)?;
            accounts
                .restore_auto_buy_rules(&account_id, rules)
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let preferences = load_automation_preferences(&database, &account_id)?;
            accounts
                .restore_automation_preferences(
                    &account_id,
                    preferences.potion_ids,
                    preferences.ball_ids,
                    preferences.capture_mode,
                )
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
        }
        startup_mark("persisted accounts load end");
        let database = Arc::new(Mutex::new(database));
        let startup_bootstrap_concurrency = settings.startup_bootstrap_concurrency();
        startup_mark("market runtime hydration start");
        let market = market::MarketRuntime::new(accounts.clone(), database.clone());
        startup_mark("market runtime hydration end");
        startup_mark("asset service setup start");
        let assets = assets::GameAssetService::new(&app_data_dir)
            .map_err(|_| rusqlite::Error::InvalidQuery)?;
        startup_mark("asset service setup end");
        let core = Self {
            lifecycle,
            accounts: accounts.clone(),
            settings: Arc::new(Mutex::new(settings)),
            market,
            database,
            app_data_dir: app_data_dir.clone(),
            assets,
            diagnostic: Arc::new(Mutex::new(browser::IntegrationDiagnostic::waiting())),
            // This limiter is created once for this application session. Changing the
            // preference applies at the next startup, never to tasks already running.
            bootstrap_gate: Arc::new(Semaphore::new(startup_bootstrap_concurrency)),
            bootstrap_scheduled: Arc::new(AtomicBool::new(false)),
            #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
            mobile_bridge: Arc::new(Mutex::new(None)),
        };
        startup_mark("AppCore construction end");
        Ok(core)
    }
    fn shutdown(&self) {
        tracing::info!("starting controlled application shutdown");
        self.lifecycle.shutdown();
        #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
        if let Some(bridge) = self.mobile_bridge.lock().as_ref() {
            bridge.cancellation.cancel();
        }
        persist_pending_hunt_timer_changes_sync(self);
        self.market.shutdown();
        self.accounts.stop_all();
        let _ = self.database.lock().execute_batch("PRAGMA optimize;");
    }

    #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
    async fn start_mobile_bridge(&self) -> Result<(), String> {
        if self.lifecycle.is_shutting_down() {
            return Err("mobile bridge cannot start while the Manager is shutting down".into());
        }
        if self.mobile_bridge.lock().is_some() {
            return Ok(());
        }

        let initial = mobile_publisher::initial_snapshot(&self.accounts, &self.market)
            .map_err(|error| format!("could not create mobile snapshot: {error}"))?;
        let (snapshot_sender, snapshot_receiver) = tokio::sync::watch::channel(initial);
        let cancellation = tokio_util::sync::CancellationToken::new();
        let authorized: mobile_server::MobileAuthorizationCheck = Arc::new(|| true);
        let server = mobile_server::MobileServerHandle::start(
            snapshot_receiver,
            self.accounts.clone(),
            self.market.clone(),
            self.assets.clone(),
            authorized,
            cancellation.clone(),
        )
        .await
        .map_err(|error| format!("could not bind local mobile server: {error}"))?;
        let publisher = mobile_publisher::spawn_publisher(
            self.accounts.clone(),
            self.market.clone(),
            snapshot_sender,
            cancellation.clone(),
        );
        let runtime = MobileBridgeRuntime {
            cancellation,
            server,
            publisher,
        };
        let mut runtime = Some(runtime);
        let rejected = {
            let mut bridge = self.mobile_bridge.lock();
            if bridge.is_some() {
                false
            } else if self.lifecycle.is_shutting_down() {
                true
            } else {
                *bridge = runtime.take();
                false
            }
        };
        if let Some(runtime) = runtime {
            runtime.shutdown().await;
        }
        if rejected {
            return Err("mobile bridge startup was cancelled by Manager shutdown".into());
        }
        Ok(())
    }

    #[cfg(not(all(debug_assertions, feature = "mobile-local-server")))]
    async fn start_mobile_bridge(&self) -> Result<(), String> {
        Ok(())
    }

    #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
    async fn stop_mobile_bridge(&self) {
        let bridge = self.mobile_bridge.lock().take();
        if let Some(bridge) = bridge {
            bridge.shutdown().await;
        }
    }

    #[cfg(not(all(debug_assertions, feature = "mobile-local-server")))]
    async fn stop_mobile_bridge(&self) {}

    #[cfg(all(debug_assertions, feature = "mobile-local-server"))]
    async fn join_mobile_bridge(&self) {
        self.stop_mobile_bridge().await;
    }

    #[cfg(not(all(debug_assertions, feature = "mobile-local-server")))]
    async fn join_mobile_bridge(&self) {}
}
async fn recover_lost_browser_owner(
    core: AppCore,
    account_id: String,
    owner_lost: browser::BrowserOwnerLost,
    lifecycle_epoch: u64,
    lifecycle_cancellation: tokio_util::sync::CancellationToken,
) {
    if lifecycle_cancellation.is_cancelled()
        || !core
            .accounts
            .lifecycle_is_current(&account_id, lifecycle_epoch)
    {
        return;
    }
    tracing::info!(account_id, reason = %owner_lost.reason, "browser owner lost; starting background recovery");
    if let Some(bootstrap) = owner_lost.bootstrap {
        let _ = core
            .accounts
            .transition(&account_id, AccountRuntimeState::BackgroundConnecting);
        let result = tokio::select! {
            _ = lifecycle_cancellation.cancelled() => return,
            result = tokio::time::timeout(
                Duration::from_secs(25),
                ConnectionManager::connect(bootstrap, core.accounts.clone()),
            ) => result,
        };
        match result {
            Ok(Ok(connection)) => {
                if core
                    .accounts
                    .attach_background_if_lifecycle_current(
                        &account_id,
                        lifecycle_epoch,
                        connection,
                    )
                    .is_ok()
                {
                    tracing::info!(
                        account_id,
                        "browser owner recovered directly into Background after Rust welcome"
                    );
                    return;
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(account_id, %error, "direct Rust recovery after browser owner loss failed; falling back to profile bootstrap");
            }
            Err(_) => tracing::warn!(
                account_id,
                "direct Rust recovery after browser owner loss timed out; falling back to profile bootstrap"
            ),
        }
    } else {
        tracing::warn!(
            account_id,
            "browser owner loss had no reusable session material; falling back to profile bootstrap"
        );
    }

    if lifecycle_cancellation.is_cancelled()
        || !core
            .accounts
            .lifecycle_is_current(&account_id, lifecycle_epoch)
    {
        return;
    }

    // A stale ephemeral session is recoverable. Reuse the isolated persistent
    // profile through the existing non-interactive bootstrap; it will surface
    // LoginRequired only when the profile itself can no longer authenticate.
    let _ = core
        .accounts
        .transition(&account_id, AccountRuntimeState::BrowserBootstrap);
    let settings = core.settings.lock().clone();
    browser::start_observer(
        core.app_data_dir.clone(),
        account_id.clone(),
        settings.game_url,
        browser::BrowserSessionMode::BackgroundBootstrap,
        core.accounts.clone(),
        core.database.clone(),
        core.diagnostic.clone(),
        None,
        lifecycle_cancellation.clone(),
        None,
        None,
    )
    .await;
    if !lifecycle_cancellation.is_cancelled()
        && core
            .accounts
            .lifecycle_is_current(&account_id, lifecycle_epoch)
    {
        core.accounts.browser_transfer_failed(
            &account_id,
            "A recuperação em background terminou sem anexar um owner; tente reconectar a conta."
                .into(),
        );
    }
}

fn mode_uses_startup_bootstrap_gate(mode: browser::BrowserSessionMode) -> bool {
    matches!(mode, browser::BrowserSessionMode::BackgroundBootstrap)
}

fn launch_session(
    core: AppCore,
    account_id: String,
    mode: browser::BrowserSessionMode,
    lifecycle_guard: Option<tokio::sync::OwnedMutexGuard<()>>,
) {
    launch_session_with_executable(core, account_id, mode, lifecycle_guard, None);
}

fn launch_session_with_executable(
    core: AppCore,
    account_id: String,
    mode: browser::BrowserSessionMode,
    lifecycle_guard: Option<tokio::sync::OwnedMutexGuard<()>>,
    resolved_executable: Option<std::path::PathBuf>,
) {
    startup_mark("account bootstrap scheduled");
    let settings = core.settings.lock().clone();
    let base = core.app_data_dir.clone();
    let provisional_profile = matches!(mode, browser::BrowserSessionMode::Interactive)
        .then(|| base.join("profiles").join(&account_id));
    let provisional_profile_preexisted = provisional_profile
        .as_ref()
        .is_some_and(|profile_path| profile_path.exists());
    let provisional_process = matches!(mode, browser::BrowserSessionMode::Interactive)
        .then(|| Arc::new(tokio::sync::Mutex::new(None)));
    let accounts = core.accounts.clone();
    let database = core.database.clone();
    let diagnostic = core.diagnostic.clone();
    let gate = core.bootstrap_gate.clone();
    let recovery_core = core.clone();
    // Invalidate a stale observer before waiting for its per-account guard.
    // Otherwise an observer waiting for browser/CDP input can hold the guard
    // indefinitely and make an explicit "Fazer login" request appear inert.
    let Some((lifecycle_epoch, lifecycle_cancellation)) = accounts.start_lifecycle(&account_id)
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let _lifecycle_guard = match lifecycle_guard {
            Some(guard) => guard,
            None => accounts.lifecycle_lock(&account_id).lock_owned().await,
        };
        // A newer launch may have superseded this one while it waited for the
        // previous session to release the lock. Only the newest request starts.
        if !accounts.lifecycle_is_current(&account_id, lifecycle_epoch) {
            return;
        }
        if matches!(mode, browser::BrowserSessionMode::Interactive)
            && accounts.begin_browser_login(&account_id).is_err()
        {
            return;
        }
        if matches!(mode, browser::BrowserSessionMode::BackgroundBootstrap)
            && let Ok(Some(background)) = accounts.take_background_for_transition(&account_id)
        {
            background.shutdown().await;
        }
        let observer_account_id = account_id.clone();
        startup_mark("account runtime bootstrap start");
        let observer_accounts = accounts.clone();
        let (owner_loss_sender, mut owner_loss_receiver) =
            if browser::is_interactive_owner_mode(mode) {
                let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
        let observer_diagnostic = diagnostic.clone();
        let observer = browser::start_observer(
            base.clone(),
            observer_account_id,
            settings.game_url,
            mode,
            observer_accounts,
            database.clone(),
            diagnostic.clone(),
            owner_loss_sender,
            lifecycle_cancellation.clone(),
            provisional_process.clone(),
            resolved_executable,
        );
        // Only background bootstrap is serialized. A user explicitly opening
        // two profiles must not be made to wait behind an unrelated startup.
        if mode_uses_startup_bootstrap_gate(mode) {
            let Ok(_permit) = gate.acquire_owned().await else {
                return;
            };
            observer.await;
        } else {
            observer.await;
        }
        if matches!(mode, browser::BrowserSessionMode::Interactive)
            && !lifecycle_cancellation.is_cancelled()
            && accounts.lifecycle_is_current(&account_id, lifecycle_epoch)
        {
            let account_persisted = match database.lock().query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)",
                [&account_id],
                |row| row.get::<_, bool>(0),
            ) {
                Ok(persisted) => persisted,
                Err(error) => {
                    tracing::warn!(account_id, %error, "could not verify whether provisional account was persisted; preserving its data");
                    true
                }
            };
            let diagnostic_confirms_welcome = {
                let diagnostic = observer_diagnostic.lock();
                diagnostic.account_id.as_deref() == Some(account_id.as_str())
                    && diagnostic.welcome_received
            };
            let startup_failed_before_authentication =
                !account_persisted && !diagnostic_confirms_welcome;
            if startup_failed_before_authentication {
                // Interactive mode is used only for a just-created provisional
                // account. Never apply this rollback to an existing profile.
                if accounts.remove(&account_id).is_ok() {
                    tracing::warn!(
                        account_id,
                        "removing provisional account after browser startup failure"
                    );
                }
                let process_stopped = if let Some(process_slot) = provisional_process.as_ref()
                    && let Some(mut child) = process_slot.lock().await.take()
                {
                    browser::terminate_recovery_process_tree(&mut child).await
                } else {
                    true
                };
                if process_stopped
                    && !provisional_profile_preexisted
                    && let Some(profile_path) = provisional_profile.as_ref()
                    && profile_path.exists()
                    && let Err(error) = std::fs::remove_dir_all(profile_path)
                {
                    tracing::warn!(account_id, %error, "could not remove provisional browser profile after startup failure");
                }
            }
        }
        if mode_uses_startup_bootstrap_gate(mode) {
            let runtime = accounts
                .snapshots()
                .into_iter()
                .find(|snapshot| snapshot.account.id == account_id)
                .map(|snapshot| snapshot.account.runtime);
            if matches!(
                runtime,
                Some(AccountRuntimeState::BrowserBootstrap | AccountRuntimeState::WaitingForLogin)
            ) {
                tracing::warn!(
                    account_id,
                    "background profile bootstrap ended without a connected session"
                );
                let _ = accounts.transition(&account_id, AccountRuntimeState::LoginRequired);
            }
        }
        if browser::is_interactive_owner_mode(mode) {
            if let Some(owner_lost) = owner_loss_receiver
                .as_mut()
                .and_then(|receiver| receiver.try_recv().ok())
            {
                recover_lost_browser_owner(
                    recovery_core,
                    account_id,
                    owner_lost,
                    lifecycle_epoch,
                    lifecycle_cancellation,
                )
                .await;
                return;
            }
            // Any failure before Browser ownership is proven restores the already
            // connected Rust session when one survived; otherwise leave an
            // actionable Error instead of a perpetual Transition.
            accounts.restore_background_owner(&account_id);
            let failure_reason = {
                let diagnostic = observer_diagnostic.lock();
                (diagnostic.account_id.as_deref() == Some(account_id.as_str())
                    && matches!(diagnostic.lifecycle, browser::BrowserLifecycle::Error))
                .then(|| diagnostic.message.clone())
                .filter(|message| !message.trim().is_empty())
                .unwrap_or_else(|| {
                    "A sessão do navegador terminou antes de confirmar hello → welcome.".into()
                })
            };
            accounts.browser_transfer_failed(&account_id, failure_reason);
        }
    });
}
/// Called by the mounted frontend after its first paint. Keeping browser work
/// out of Tauri's synchronous setup path prevents a headless Brave startup
/// from competing with the first WebView/React render.
fn schedule_restored_bootstraps(core: &AppCore) -> Vec<AccountSnapshot> {
    if core.bootstrap_scheduled.swap(true, Ordering::AcqRel) {
        return core.accounts.snapshots();
    }
    start_session_recovery_monitor(core.clone());
    startup_mark("frontend first paint; account bootstraps scheduling");
    for account_id in core.accounts.ids() {
        let _ = core
            .accounts
            .transition(&account_id, AccountRuntimeState::BrowserBootstrap);
        launch_session(
            core.clone(),
            account_id,
            browser::BrowserSessionMode::BackgroundBootstrap,
            None,
        );
    }
    startup_mark("all account bootstraps scheduled after first paint");
    core.market.start();
    core.accounts.snapshots()
}

/// Reuses the account's persistent Brave profile after an authenticated socket
/// is rejected. The browser bootstrap is headless and serialized with startup
/// bootstraps; if it cannot authenticate, start_observer moves the account to
/// LoginRequired and this monitor will not retry until another rejection.
fn start_session_recovery_monitor(core: AppCore) {
    let cancellation = core.lifecycle.token();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut silent_login_attempted = std::collections::HashSet::<String>::new();
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                _ = interval.tick() => {}
            }
            for snapshot in core.accounts.snapshots() {
                let account_id = snapshot.account.id;
                if snapshot.account.status == ConnectionStatus::Online {
                    // A confirmed online session begins a new recovery cycle.
                    silent_login_attempted.remove(&account_id);
                    continue;
                }
                if should_attempt_silent_login_recovery(
                    &snapshot.account.runtime,
                    silent_login_attempted.contains(&account_id),
                ) {
                    if core
                        .accounts
                        .transition(&account_id, AccountRuntimeState::BrowserBootstrap)
                        .is_ok()
                    {
                        silent_login_attempted.insert(account_id.clone());
                        tracing::info!(
                            account_id,
                            "starting one automatic silent login recovery with the persistent browser profile"
                        );
                        launch_session(
                            core.clone(),
                            account_id,
                            browser::BrowserSessionMode::BackgroundBootstrap,
                            None,
                        );
                    }
                    continue;
                }
                if snapshot.account.runtime != AccountRuntimeState::RenewingSession {
                    continue;
                }
                if core
                    .accounts
                    .transition(&account_id, AccountRuntimeState::BrowserBootstrap)
                    .is_ok()
                {
                    tracing::info!(
                        account_id,
                        "starting silent session renewal with persistent browser profile"
                    );
                    launch_session(
                        core.clone(),
                        account_id,
                        browser::BrowserSessionMode::BackgroundBootstrap,
                        None,
                    );
                }
            }
        }
    });
}

fn should_attempt_silent_login_recovery(
    runtime: &AccountRuntimeState,
    already_attempted: bool,
) -> bool {
    *runtime == AccountRuntimeState::LoginRequired && !already_attempted
}
fn load_accounts(database: &Connection) -> Result<Vec<AccountRecord>, rusqlite::Error> {
    let mut statement = database.prepare(
        "SELECT id, nick, local_alias, card_color FROM accounts ORDER BY created_at ASC",
    )?;
    statement
        .query_map([], |row| {
            Ok(AccountRecord {
                id: row.get(0)?,
                nick: row.get(1)?,
                local_alias: row.get(2)?,
                card_color: row.get(3)?,
                status: ConnectionStatus::Offline,
                mode: AccountMode::Background,
                runtime: AccountRuntimeState::Offline,
                owner: ConnectionOwner::None,
            })
        })?
        .collect()
}
fn load_hunt_timer(
    database: &Connection,
    account_id: &str,
) -> Result<Option<HuntTimerState>, rusqlite::Error> {
    let row = database
        .query_row(
            "SELECT hunt_slug, started_at_ms, revision FROM account_hunt_state WHERE account_id = ?1",
            [account_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((hunt_slug, started_at_ms, revision)) = row else {
        return Ok(None);
    };
    let (Ok(started_at_ms), Ok(revision)) = (u64::try_from(started_at_ms), u64::try_from(revision))
    else {
        tracing::warn!(%account_id, "ignoring invalid persisted hunt timer");
        return Ok(None);
    };
    if hunt_slug.trim().is_empty() || revision == 0 {
        tracing::warn!(%account_id, "ignoring invalid persisted hunt timer");
        return Ok(None);
    }
    Ok(Some(HuntTimerState {
        hunt_slug,
        started_at_ms,
        revision,
    }))
}
fn persist_hunt_timer(
    database: &Connection,
    change: &accounts::HuntTimerPersistenceChange,
) -> Result<(), rusqlite::Error> {
    let started_at_ms = i64::try_from(change.timer.started_at_ms)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let revision = i64::try_from(change.timer.revision)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    database.execute(
        "INSERT INTO account_hunt_state(account_id, hunt_slug, started_at_ms, revision) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account_id) DO UPDATE SET hunt_slug = excluded.hunt_slug, started_at_ms = excluded.started_at_ms, revision = excluded.revision WHERE excluded.revision > account_hunt_state.revision",
        (
            &change.account_id,
            &change.timer.hunt_slug,
            started_at_ms,
            revision,
        ),
    )?;
    Ok(())
}
fn persist_pending_hunt_timer_changes_sync(core: &AppCore) {
    let changes = core.accounts.pending_hunt_timer_changes();
    if changes.is_empty() {
        return;
    }
    let mut persisted = Vec::with_capacity(changes.len());
    {
        let database = core.database.lock();
        for change in &changes {
            match persist_hunt_timer(&database, change) {
                Ok(()) => persisted.push(change.clone()),
                Err(error) => tracing::warn!(
                    account_id = %change.account_id,
                    %error,
                    "could not persist hunt timer during shutdown"
                ),
            }
        }
    }
    core.accounts.acknowledge_hunt_timer_changes(&persisted);
}
async fn persist_pending_hunt_timer_changes(core: &AppCore) -> bool {
    let changes = core.accounts.pending_hunt_timer_changes();
    if changes.is_empty() {
        return true;
    }
    let database = core.database.clone();
    let write_changes = changes.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let database = database.lock();
        let mut persisted = Vec::with_capacity(write_changes.len());
        let mut failed = false;
        for change in &write_changes {
            match persist_hunt_timer(&database, change) {
                Ok(()) => persisted.push(change.clone()),
                Err(error) => {
                    failed = true;
                    tracing::warn!(
                        account_id = %change.account_id,
                        %error,
                        "could not persist hunt timer"
                    );
                }
            }
        }
        Ok::<_, rusqlite::Error>((persisted, failed))
    })
    .await;
    match result {
        Ok(Ok((persisted, failed))) => {
            core.accounts.acknowledge_hunt_timer_changes(&persisted);
            !failed
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "could not persist hunt timer changes");
            false
        }
        Err(error) => {
            tracing::warn!(%error, "hunt timer persistence worker failed");
            false
        }
    }
}
fn start_hunt_timer_persistence(core: AppCore) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let cancellation = core.lifecycle.token();
        let mut changes = core.accounts.subscribe_hunt_timer_changes();
        // Also flush state established before this task subscribed (for
        // example, the first account welcome arriving during app startup).
        let mut initial_or_retry_flush = true;
        loop {
            if !initial_or_retry_flush {
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        let _ = persist_pending_hunt_timer_changes(&core).await;
                        break;
                    }
                    changed = changes.changed() => {
                        if changed.is_err() {
                            break;
                        }
                    }
                }
            }
            initial_or_retry_flush = false;
            if !persist_pending_hunt_timer_changes(&core).await {
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        let _ = persist_pending_hunt_timer_changes(&core).await;
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        initial_or_retry_flush = true;
                    }
                }
            }
        }
    })
}
fn remove_persisted_account(
    database: &Connection,
    account_id: &str,
) -> Result<(), rusqlite::Error> {
    // Keep the Brave profile directory untouched. Only Manager-owned database
    // records are removed here; a profile can contain a valuable login session.
    database.execute(
        "DELETE FROM automation_settings WHERE account_id = ?1",
        [account_id],
    )?;
    database.execute(
        "DELETE FROM market_sniper_rules WHERE account_id = ?1",
        [account_id],
    )?;
    database.execute(
        "DELETE FROM market_purchase_history WHERE account_id = ?1",
        [account_id],
    )?;
    database.execute(
        "DELETE FROM account_history WHERE account_id = ?1",
        [account_id],
    )?;
    database.execute(
        "DELETE FROM account_hunt_state WHERE account_id = ?1",
        [account_id],
    )?;
    database.execute("DELETE FROM accounts WHERE id = ?1", [account_id])?;
    Ok(())
}
fn load_auto_buy_rules(
    database: &Connection,
    account_id: &str,
) -> Result<Vec<crate::domain::AutoBuyRule>, rusqlite::Error> {
    let value = database
        .query_row(
            "SELECT config_json FROM automation_settings WHERE account_id = ?1 AND automation_key = 'auto_buy_rules'",
            [account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|json| serde_json::from_str::<Vec<crate::domain::AutoBuyRule>>(&json).ok())
        .unwrap_or_default())
}
fn persist_auto_buy_rules(
    database: &Connection,
    account_id: &str,
    rules: &[crate::domain::AutoBuyRule],
) -> Result<(), rusqlite::Error> {
    let config = serde_json::to_string(rules).map_err(|_| rusqlite::Error::InvalidQuery)?;
    database.execute(
        "INSERT INTO automation_settings(account_id, automation_key, enabled, config_json) VALUES (?1, 'auto_buy_rules', ?2, ?3) ON CONFLICT(account_id, automation_key) DO UPDATE SET enabled = excluded.enabled, config_json = excluded.config_json",
        (
            account_id,
            rules.iter().any(|rule| rule.enabled) as i64,
            config,
        ),
    )?;
    Ok(())
}
fn load_automation_preferences(
    database: &Connection,
    account_id: &str,
) -> Result<LocalAutomationPreferences, rusqlite::Error> {
    let value = database
        .query_row(
            "SELECT config_json FROM automation_settings WHERE account_id = ?1 AND automation_key = 'combat_preferences'",
            [account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|json| serde_json::from_str::<LocalAutomationPreferences>(&json).ok())
        .unwrap_or_default())
}
fn persist_automation_preferences(
    database: &Connection,
    account_id: &str,
    preferences: &LocalAutomationPreferences,
) -> Result<(), rusqlite::Error> {
    let config = serde_json::to_string(preferences).map_err(|_| rusqlite::Error::InvalidQuery)?;
    database.execute(
        "INSERT INTO automation_settings(account_id, automation_key, enabled, config_json) VALUES (?1, 'combat_preferences', 1, ?2) ON CONFLICT(account_id, automation_key) DO UPDATE SET config_json = excluded.config_json",
        (account_id, config),
    )?;
    Ok(())
}
/// `until_capture` turns itself off only after a confirmed game event.  That
/// transition originates in the account worker rather than a UI command, so
/// persist it opportunistically when the frontend requests its regular state
/// snapshot.  Locks are deliberately never nested here: account state is
/// copied first, then each small SQLite write happens independently.
fn persist_pending_capture_mode_changes(core: &AppCore) {
    let changes = core.accounts.pending_capture_mode_changes();
    if changes.is_empty() {
        return;
    }

    let mut persisted_account_ids = Vec::with_capacity(changes.len());
    for (account_id, capture_mode) in changes {
        let result: Result<(), rusqlite::Error> = (|| {
            let database = core.database.lock();
            let mut preferences = load_automation_preferences(&database, &account_id)?;
            preferences.capture_mode = capture_mode;
            persist_automation_preferences(&database, &account_id, &preferences)
        })();
        match result {
            Ok(()) => persisted_account_ids.push(account_id),
            Err(error) => {
                tracing::warn!(%account_id, %error, "could not persist completed capture mode")
            }
        }
    }
    core.accounts
        .acknowledge_capture_mode_changes(&persisted_account_ids);
}
fn load_settings(database: &Connection) -> Result<PersistedSettings, rusqlite::Error> {
    let mut settings = PersistedSettings::default();
    if let Some(value) = database
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'minimize_to_tray'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        settings.minimize_to_tray = value == "true";
    }
    if let Some(value) = database
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'game_url'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        settings.game_url = value;
    }
    if let Some(value) = database
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'startup_browser_concurrency'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        settings.startup_browser_concurrency = value
            .parse::<u8>()
            .unwrap_or_else(|_| default_startup_browser_concurrency());
    }
    Ok(settings.normalized())
}
fn save_settings(core: &AppCore, settings: PersistedSettings) -> Result<(), String> {
    let settings = settings.normalized();
    let database = core.database.lock();
    database.execute("INSERT INTO app_settings(key, value) VALUES ('minimize_to_tray', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [settings.minimize_to_tray.to_string()]).map_err(|error| error.to_string())?;
    database.execute("INSERT INTO app_settings(key, value) VALUES ('game_url', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [&settings.game_url]).map_err(|error| error.to_string())?;
    database.execute("INSERT INTO app_settings(key, value) VALUES ('startup_browser_concurrency', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [settings.startup_browser_concurrency.to_string()]).map_err(|error| error.to_string())?;
    *core.settings.lock() = settings;
    Ok(())
}
fn shutdown(app: &tauri::AppHandle) {
    if let Some(lifecycle) = app.try_state::<community::ApplicationLifecycle>() {
        lifecycle.shutdown();
    }
    if let Some(core) = app.try_state::<AppCore>() {
        let core = core.inner().clone();
        core.shutdown();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            core.join_mobile_bridge().await;
            let _ = app.remove_tray_by_id("main-tray");
            app.exit(0);
        });
    } else {
        let _ = app.remove_tray_by_id("main-tray");
        app.exit(0);
    }
}

async fn initialize_application_core(
    app: tauri::AppHandle,
    lifecycle: community::ApplicationLifecycle,
    readiness: AppCoreReadiness,
) {
    let worker_app = app.clone();
    let worker_lifecycle = lifecycle.clone();
    let core_task = tauri::async_runtime::spawn_blocking(move || {
        startup_mark("app data directory preparation start");
        let data_dir = worker_app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        startup_mark("app data directory preparation end");
        startup_mark("SQLite open and migrations start");
        let database = persistence::open_and_migrate(&data_dir.join("pokeidle-manager.db"))
            .map_err(|error| error.to_string())?;
        startup_mark("SQLite open and migrations end");
        AppCore::new(database, data_dir, worker_lifecycle).map_err(|error| error.to_string())
    });

    let core_result = core_task.await;
    let core = match core_result {
        Ok(Ok(core)) => core,
        Ok(Err(error)) => {
            lifecycle.shutdown();
            tracing::error!(%error, "application core initialization failed");
            readiness.set(AppCoreStatus {
                ready: false,
                error: Some("Não foi possível preparar os dados do aplicativo.".into()),
            });
            let _ = app.emit("app-core-state-changed", readiness.snapshot());
            return;
        }
        Err(error) => {
            lifecycle.shutdown();
            tracing::error!(%error, "application core worker failed");
            readiness.set(AppCoreStatus {
                ready: false,
                error: Some("Não foi possível preparar os dados do aplicativo.".into()),
            });
            let _ = app.emit("app-core-state-changed", readiness.snapshot());
            return;
        }
    };

    if lifecycle.is_shutting_down() {
        core.shutdown();
        return;
    }
    if let Err(error) = core.start_mobile_bridge().await {
        tracing::warn!(%error, "could not start the local mobile bridge");
    }

    readiness.set(AppCoreStatus {
        ready: true,
        error: None,
    });
    startup_mark("application core ready");
    let _ = app.emit("app-core-state-changed", readiness.snapshot());
    let _ = app.manage(core.clone());
    let mut hunt_timer_persistence = start_hunt_timer_persistence(core.clone());
    let lifecycle_token = lifecycle.token();
    tokio::select! {
        _ = lifecycle_token.cancelled() => {},
        _ = &mut hunt_timer_persistence => {},
    }
    let _ = hunt_timer_persistence.await;
}

#[tauri::command]
fn dashboard(state: tauri::State<'_, AppCore>) -> Result<Vec<AccountSnapshot>, String> {
    persist_pending_capture_mode_changes(state.inner());
    Ok(state.accounts.snapshots())
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntegrationSnapshot {
    diagnostic: browser::IntegrationDiagnostic,
    accounts: Vec<AccountSnapshot>,
}
#[tauri::command]
fn integration_snapshot(state: tauri::State<'_, AppCore>) -> Result<IntegrationSnapshot, String> {
    persist_pending_capture_mode_changes(state.inner());
    Ok(IntegrationSnapshot {
        diagnostic: state.diagnostic.lock().clone(),
        accounts: state.accounts.snapshots(),
    })
}
#[tauri::command]
async fn market_snapshot(
    state: tauri::State<'_, AppCore>,
) -> Result<market::MarketSnapshot, String> {
    let market = state.market.clone();
    tauri::async_runtime::spawn_blocking(move || {
        market
            .try_snapshot()
            .ok_or_else(|| "O estado do Mercado está ocupado; a leitura será repetida.".to_owned())
    })
    .await
    .map_err(|error| format!("Falha ao consultar o Mercado: {error}"))?
}
#[tauri::command]
async fn market_diagnostics(
    state: tauri::State<'_, AppCore>,
) -> Result<market::MarketDiagnosticsSnapshot, String> {
    let market = state.market.clone();
    tauri::async_runtime::spawn_blocking(move || market.diagnostics())
        .await
        .map_err(|error| format!("Falha ao consultar o diagnóstico do Mercado: {error}"))
}
#[tauri::command]
async fn set_market_reader(
    state: tauri::State<'_, AppCore>,
    account_id: Option<String>,
) -> Result<(), String> {
    let market = state.market.clone();
    tauri::async_runtime::spawn_blocking(move || market.set_reader(account_id))
        .await
        .map_err(|error| format!("Falha ao atualizar o leitor do Mercado: {error}"))?
}
#[tauri::command]
async fn save_market_sniper_rule(
    state: tauri::State<'_, AppCore>,
    rule: market::MarketSniperRule,
) -> Result<(), String> {
    let market = state.market.clone();
    tauri::async_runtime::spawn_blocking(move || market.save_rule(rule))
        .await
        .map_err(|error| format!("Falha ao salvar a regra do Mercado: {error}"))?
}
#[tauri::command]
async fn delete_market_sniper_rule(
    state: tauri::State<'_, AppCore>,
    rule_id: String,
) -> Result<(), String> {
    let market = state.market.clone();
    tauri::async_runtime::spawn_blocking(move || market.delete_rule(&rule_id))
        .await
        .map_err(|error| format!("Falha ao remover a regra do Mercado: {error}"))?
}
#[tauri::command]
fn schedule_restored_account_bootstraps(
    state: tauri::State<'_, AppCore>,
) -> Result<Vec<AccountSnapshot>, String> {
    Ok(schedule_restored_bootstraps(state.inner()))
}
#[tauri::command]
fn set_auto_sale(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .accounts
        .set_auto_sale(&account_id, enabled)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn set_native_automation(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    field: String,
    enabled: bool,
) -> Result<(), String> {
    let field = match field.as_str() {
        "autoPotion" | "autoRevive" | "autoVendaLoot" | "autoVoltarHunt" => field,
        _ => return Err("Automação nativa não permitida".into()),
    };
    if field == "autoVendaLoot" {
        return state
            .accounts
            .set_auto_sale(&account_id, enabled)
            .map_err(|error| error.to_string());
    }
    state
        .accounts
        .set_automation_value(&account_id, &field, serde_json::Value::Bool(enabled))
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn set_native_automation_ids(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    field: String,
    ids: Vec<u64>,
) -> Result<(), String> {
    let kind = match field.as_str() {
        "potionIds" => crate::domain::AutoBuyKind::Item,
        "ballIds" => crate::domain::AutoBuyKind::Ball,
        _ => return Err("Lista de automação nativa não permitida".into()),
    };
    state
        .accounts
        .set_automation_ids(&account_id, kind, ids.clone())
        .map_err(|error| error.to_string())?;
    let snapshot = state
        .accounts
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.account.id == account_id)
        .ok_or_else(|| "Conta não encontrada".to_string())?;
    let mut preferences = load_automation_preferences(&state.database.lock(), &account_id)
        .map_err(|error| error.to_string())?;
    preferences.potion_ids = snapshot.state.automation.potion_ids;
    preferences.ball_ids = snapshot.state.automation.ball_ids;
    match field.as_str() {
        "potionIds" => preferences.potion_ids = ids,
        "ballIds" => preferences.ball_ids = ids,
        _ => unreachable!("field was validated above"),
    }
    persist_automation_preferences(&state.database.lock(), &account_id, &preferences)
        .map_err(|error| error.to_string())?;
    let rules = state
        .accounts
        .auto_buy_rules(&account_id)
        .map_err(|error| error.to_string())?;
    persist_auto_buy_rules(&state.database.lock(), &account_id, &rules)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn set_native_hp_threshold(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    percent: u8,
) -> Result<(), String> {
    if !(10..=100).contains(&percent) || percent % 10 != 0 {
        return Err("O limiar de HP deve estar entre 10% e 100%, em passos de 10%.".into());
    }
    state
        .accounts
        .set_automation_value(
            &account_id,
            "hpLimiar",
            serde_json::json!(f64::from(percent) / 100.0),
        )
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn select_hunt(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    slug: String,
) -> Result<(), String> {
    state
        .accounts
        .select_hunt(&account_id, slug)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn go_center(state: tauri::State<'_, AppCore>, account_id: String) -> Result<(), String> {
    state
        .accounts
        .go_center(&account_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn cancel_navigation(state: tauri::State<'_, AppCore>, account_id: String) -> Result<(), String> {
    state
        .accounts
        .cancel_navigation(&account_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn sell_pokemon(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    pokemon_id: u64,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("Confirmação explícita necessária para vender Pokémon.".into());
    }
    state
        .accounts
        .sell_pokemon(&account_id, pokemon_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn sell_all_pokemon(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("Confirmação explícita necessária para vender todo o Depot.".into());
    }
    state
        .accounts
        .sell_all_pokemon(&account_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn set_capture_mode(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    mode: crate::domain::CaptureMode,
) -> Result<(), String> {
    state
        .accounts
        .set_capture_mode(&account_id, mode.clone())
        .map_err(|error| error.to_string())?;
    let mut preferences = load_automation_preferences(&state.database.lock(), &account_id)
        .map_err(|error| error.to_string())?;
    preferences.capture_mode = mode;
    persist_automation_preferences(&state.database.lock(), &account_id, &preferences)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn set_auto_buy_enabled(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    kind: crate::domain::AutoBuyKind,
    enabled: bool,
) -> Result<(), String> {
    state
        .accounts
        .set_auto_buy_enabled(&account_id, kind, enabled)
        .map_err(|error| error.to_string())?;
    let rules = state
        .accounts
        .auto_buy_rules(&account_id)
        .map_err(|error| error.to_string())?;
    persist_auto_buy_rules(&state.database.lock(), &account_id, &rules)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn update_auto_buy_rule(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    kind: crate::domain::AutoBuyKind,
    item_id: u64,
    minimum: u64,
    quantity: u64,
) -> Result<(), String> {
    state
        .accounts
        .update_auto_buy_rule(&account_id, kind, item_id, minimum, quantity)
        .map_err(|error| error.to_string())?;
    let rules = state
        .accounts
        .auto_buy_rules(&account_id)
        .map_err(|error| error.to_string())?;
    persist_auto_buy_rules(&state.database.lock(), &account_id, &rules)
        .map_err(|error| error.to_string())
}
#[tauri::command]
async fn open_account_browser(
    state: tauri::State<'_, AppCore>,
    account_id: String,
) -> Result<(), String> {
    let lifecycle_guard = state
        .accounts
        .lifecycle_lock(&account_id)
        .lock_owned()
        .await;
    let manager = browser::WindowsBraveManager::new(state.app_data_dir.clone());
    let executable = manager.detect_brave().map_err(|error| {
        tracing::info!(%error, "browser preflight: unavailable for existing account browser open");
        error.to_string()
    })?;
    let background = state
        .accounts
        .begin_browser_transfer(&account_id)
        .map_err(|error| error.to_string())?;
    background.shutdown().await;
    launch_session_with_executable(
        state.inner().clone(),
        account_id,
        browser::BrowserSessionMode::InteractiveOwner,
        Some(lifecycle_guard),
        Some(executable),
    );
    Ok(())
}
#[tauri::command]
async fn reconnect_browser_control(
    state: tauri::State<'_, AppCore>,
    account_id: String,
) -> Result<(), String> {
    let lifecycle_guard = state
        .accounts
        .lifecycle_lock(&account_id)
        .lock_owned()
        .await;
    let executable = browser::WindowsBraveManager::new(state.app_data_dir.clone())
        .detect_brave()
        .map_err(|error| {
            tracing::info!(%error, "browser preflight: unavailable for browser reconnect");
            error.to_string()
        })?;
    state
        .accounts
        .begin_browser_reconnect(&account_id)
        .map_err(|error| error.to_string())?;
    launch_session_with_executable(
        state.inner().clone(),
        account_id.clone(),
        browser::BrowserSessionMode::ReconnectExistingOwner,
        Some(lifecycle_guard),
        Some(executable),
    );

    // The observer runs in the background, so returning immediately makes a
    // failed CDP reattachment look like a successful (but inert) button click.
    // Wait for the owner handoff to be proven, or surface the observer's actual
    // failure reason to the caller/UI.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = state
            .accounts
            .snapshots()
            .into_iter()
            .find(|snapshot| snapshot.account.id == account_id)
            .ok_or_else(|| "A conta não está mais disponível para reconexão.".to_owned())?;

        if snapshot.account.owner == ConnectionOwner::Browser
            && snapshot.account.status == ConnectionStatus::Online
        {
            return Ok(());
        }

        if snapshot.account.status == ConnectionStatus::Error {
            let reason = snapshot
                .state
                .disconnect_reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| "O navegador não confirmou a reconexão.".into());
            return Err(format!("Reconexão falhou: {reason}"));
        }

        if Instant::now() >= deadline {
            return Err(
                "A reconexão ainda não foi confirmada após 30 segundos. Aguarde a atualização do estado antes de tentar novamente.".into(),
            );
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tauri::command]
fn open_brave_download_page(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    app.opener()
        .open_url(BRAVE_DOWNLOAD_URL, None::<&str>)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn return_account_to_background(
    state: tauri::State<'_, AppCore>,
    account_id: String,
) -> Result<(), String> {
    state
        .accounts
        .request_background_handoff(&account_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn login_account(state: tauri::State<'_, AppCore>, account_id: String) -> Result<(), String> {
    let manager = browser::WindowsBraveManager::new(state.app_data_dir.clone());
    let executable = manager.detect_brave().map_err(|error| {
        tracing::info!(%error, "browser preflight: unavailable for account login");
        error.to_string()
    })?;
    launch_session_with_executable(
        state.inner().clone(),
        account_id,
        // Login recovery should reuse the persistent profile invisibly, transfer
        // the authenticated session to the Rust owner, and close the controlled
        // browser instead of requiring the user to open the profile manually.
        browser::BrowserSessionMode::BackgroundBootstrap,
        None,
        Some(executable),
    );
    Ok(())
}
#[tauri::command]
fn remove_account(
    state: tauri::State<'_, AppCore>,
    account_id: String,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("Confirmação explícita necessária para remover a conta do Manager.".into());
    }
    state
        .accounts
        .remove(&account_id)
        .map_err(|error| error.to_string())?;
    remove_persisted_account(&state.database.lock(), &account_id).map_err(|error| error.to_string())
}
#[tauri::command]
fn set_protocol_inspector_enabled(
    state: tauri::State<'_, AppCore>,
    enabled: bool,
) -> Result<(), String> {
    state.accounts.set_inspector_enabled(enabled);
    Ok(())
}
#[tauri::command]
fn protocol_inspector_frames(
    state: tauri::State<'_, AppCore>,
    account_id: String,
) -> Result<Vec<inspector::ProtocolFrame>, String> {
    state
        .accounts
        .protocol_frames(&account_id)
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn clear_protocol_inspector_frames(
    state: tauri::State<'_, AppCore>,
    account_id: String,
) -> Result<(), String> {
    state
        .accounts
        .clear_protocol_frames(&account_id)
        .map_err(|error| error.to_string())
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum StartAccountResult {
    Started { account_id: String },
    BrowserNotFound,
    InvalidInstallation,
    LimitReached,
}

#[tauri::command]
fn start_real_account(state: tauri::State<'_, AppCore>) -> Result<StartAccountResult, String> {
    if state.accounts.count() >= MAX_ACCOUNTS {
        return Ok(StartAccountResult::LimitReached);
    }
    let manager = browser::WindowsBraveManager::new(state.app_data_dir.clone());
    let executable = match manager.detect_brave() {
        Ok(executable) => {
            tracing::info!("browser preflight: available");
            executable
        }
        Err(browser::BrowserError::NotFound) => {
            tracing::info!("browser preflight: not found");
            return Ok(StartAccountResult::BrowserNotFound);
        }
        Err(browser::BrowserError::InvalidInstallation(_)) => {
            tracing::warn!("browser preflight: invalid installation");
            return Ok(StartAccountResult::InvalidInstallation);
        }
        Err(error) => {
            tracing::warn!(%error, "browser preflight failed");
            return Err(error.to_string());
        }
    };
    let account_id = uuid::Uuid::new_v4().to_string();
    let record = accounts::new_record(
        account_id.clone(),
        format!("Login {}", &account_id[..8]),
        "#3b82f6".into(),
    );
    state
        .accounts
        .add(record)
        .map_err(|error| error.to_string())?;
    launch_session_with_executable(
        state.inner().clone(),
        account_id.clone(),
        browser::BrowserSessionMode::Interactive,
        None,
        Some(executable),
    );
    Ok(StartAccountResult::Started { account_id })
}
#[tauri::command]
fn enable_mock_mode(state: tauri::State<'_, AppCore>) -> Result<Vec<AccountSnapshot>, String> {
    mock::seed_four_accounts(&state.accounts);
    Ok(state.accounts.snapshots())
}
#[tauri::command]
fn advance_mock_mode(
    tick: u64,
    state: tauri::State<'_, AppCore>,
) -> Result<Vec<AccountSnapshot>, String> {
    mock::advance(&state.accounts, tick);
    Ok(state.accounts.snapshots())
}
#[tauri::command]
fn settings_snapshot(state: tauri::State<'_, AppCore>) -> Result<PersistedSettings, String> {
    Ok(state.settings.lock().clone())
}

/// Metadata and images are intentionally requested after the UI is mounted.
/// Nothing here participates in account bootstrap or the first WebView paint.
#[tauri::command]
async fn game_item_catalog(
    state: tauri::State<'_, AppCore>,
) -> Result<assets::GameItemCatalog, String> {
    state.assets.item_catalog().await
}

#[tauri::command]
async fn where_to_hunt_reference(
    state: tauri::State<'_, AppCore>,
) -> Result<serde_json::Value, String> {
    state.assets.where_to_hunt_reference().await
}

#[tauri::command]
async fn resolve_game_asset(
    state: tauri::State<'_, AppCore>,
    asset_path: String,
) -> Result<assets::ResolvedAsset, String> {
    state.assets.resolve_asset(asset_path).await
}

#[tauri::command]
async fn resolve_pokemon_sprite(
    state: tauri::State<'_, AppCore>,
    looktypes: Vec<u64>,
) -> Result<Option<assets::PokemonSprite>, String> {
    state.assets.pokemon_sprite(looktypes).await
}
#[tauri::command]
fn save_app_settings(
    settings: PersistedSettings,
    state: tauri::State<'_, AppCore>,
) -> Result<(), String> {
    save_settings(&state, settings)
}

#[tauri::command]
fn app_core_status(readiness: tauri::State<'_, AppCoreReadiness>) -> AppCoreStatus {
    readiness.snapshot()
}

pub fn run() {
    logging::init();
    let _ = APPLICATION_STARTED.set(Instant::now());
    startup_mark("application start");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .on_page_load(|_, payload| {
            if payload.event() == PageLoadEvent::Started {
                startup_mark("webview page load started");
            }
        })
        .setup(|app| {
            startup_mark("Tauri setup callback started");
            let lifecycle = community::ApplicationLifecycle::default();
            let readiness = AppCoreReadiness::default();
            let _ = app.manage(lifecycle.clone());
            let _ = app.manage(readiness.clone());
            startup_mark("tray menu creation start");
            let open =
                MenuItem::with_id(app, "open", "Abrir Pokeidle Manager", true, None::<&str>)?;
            let pause = MenuItem::with_id(
                app,
                "pause",
                "Pausar/Retomar automações",
                true,
                None::<&str>,
            )?;
            let disconnect =
                MenuItem::with_id(app, "disconnect", "Desconectar todas", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &disconnect, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => shutdown(app),
                    "pause" | "disconnect" => {
                        tracing::info!(action = event.id().as_ref(), "tray command requested")
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            startup_mark("tray menu creation end");
            startup_mark("Tauri setup callback complete");
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(initialize_application_core(
                app_handle, lifecycle, readiness,
            ));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = window
                    .app_handle()
                    .try_state::<AppCore>()
                    .is_some_and(|core| core.settings.lock().minimize_to_tray);
                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    shutdown(&window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            dashboard,
            app_core_status,
            integration_snapshot,
            market_snapshot,
            market_diagnostics,
            set_market_reader,
            save_market_sniper_rule,
            delete_market_sniper_rule,
            schedule_restored_account_bootstraps,
            set_auto_sale,
            set_native_automation,
            set_native_automation_ids,
            set_native_hp_threshold,
            select_hunt,
            go_center,
            cancel_navigation,
            sell_pokemon,
            sell_all_pokemon,
            set_capture_mode,
            set_auto_buy_enabled,
            update_auto_buy_rule,
            open_account_browser,
            reconnect_browser_control,
            open_brave_download_page,
            return_account_to_background,
            login_account,
            remove_account,
            set_protocol_inspector_enabled,
            protocol_inspector_frames,
            clear_protocol_inspector_frames,
            start_real_account,
            enable_mock_mode,
            advance_mock_mode,
            settings_snapshot,
            game_item_catalog,
            where_to_hunt_reference,
            resolve_game_asset,
            resolve_pokemon_sprite,
            save_app_settings
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| tracing::error!(%error, "failed to run Pokeidle Manager"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::ServerFrame;

    #[test]
    fn start_account_result_exposes_stable_preflight_statuses() {
        assert_eq!(
            serde_json::to_value(StartAccountResult::Started {
                account_id: "test-account".into()
            })
            .unwrap(),
            serde_json::json!({"status": "started", "accountId": "test-account"})
        );
        assert_eq!(
            serde_json::to_value(StartAccountResult::BrowserNotFound).unwrap(),
            serde_json::json!({"status": "browserNotFound"})
        );
        assert_eq!(
            serde_json::to_value(StartAccountResult::InvalidInstallation).unwrap(),
            serde_json::json!({"status": "invalidInstallation"})
        );
        assert_eq!(
            serde_json::to_value(StartAccountResult::LimitReached).unwrap(),
            serde_json::json!({"status": "limitReached"})
        );
    }

    #[test]
    fn brave_download_url_is_fixed_to_the_official_brazilian_site() {
        assert_eq!(BRAVE_DOWNLOAD_URL, "https://brave.com/pt-br/download/");
    }

    #[test]
    fn silent_login_recovery_is_one_shot_until_online_cycle_resets_it() {
        assert!(should_attempt_silent_login_recovery(
            &AccountRuntimeState::LoginRequired,
            false
        ));
        assert!(!should_attempt_silent_login_recovery(
            &AccountRuntimeState::LoginRequired,
            true
        ));
        assert!(!should_attempt_silent_login_recovery(
            &AccountRuntimeState::BrowserBootstrap,
            false
        ));
    }

    #[test]
    fn default_settings_minimize_to_tray() {
        assert!(PersistedSettings::default().minimize_to_tray);
        assert_eq!(
            PersistedSettings::default().startup_browser_concurrency,
            DEFAULT_STARTUP_BROWSER_CONCURRENCY
        );
    }

    #[test]
    fn hunt_timer_persists_and_same_hunt_welcome_restores_original_start() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO accounts(id, nick, card_color, created_at) VALUES ('a', 'Alpha', '#fff', unixepoch())",
                [],
            )
            .unwrap();
        let saved = accounts::HuntTimerPersistenceChange {
            account_id: "a".into(),
            timer: HuntTimerState {
                hunt_slug: "ancient_pupitar".into(),
                started_at_ms: 1_700_000_000_000,
                revision: 3,
            },
        };
        persist_hunt_timer(&database, &saved).unwrap();

        let manager = AccountManager::default();
        manager
            .add(accounts::new_record(
                "a".into(),
                "Alpha".into(),
                "#fff".into(),
            ))
            .unwrap();
        manager
            .restore_hunt_timer("a", load_hunt_timer(&database, "a").unwrap().unwrap())
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"huntSlug":"ancient_pupitar","noCentro":false}}"#,
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            manager.snapshots()[0].state.hunt_started_at_ms,
            Some(saved.timer.started_at_ms)
        );
        assert!(manager.pending_hunt_timer_changes().is_empty());
    }

    #[test]
    fn stale_hunt_timer_write_cannot_replace_newer_a_to_b_to_a_revision() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO accounts(id, nick, card_color, created_at) VALUES ('a', 'Alpha', '#fff', unixepoch())",
                [],
            )
            .unwrap();
        let newer = accounts::HuntTimerPersistenceChange {
            account_id: "a".into(),
            timer: HuntTimerState {
                hunt_slug: "ancient_pupitar".into(),
                started_at_ms: 3_000,
                revision: 3,
            },
        };
        let stale = accounts::HuntTimerPersistenceChange {
            account_id: "a".into(),
            timer: HuntTimerState {
                hunt_slug: "shellder".into(),
                started_at_ms: 2_000,
                revision: 2,
            },
        };
        persist_hunt_timer(&database, &newer).unwrap();
        persist_hunt_timer(&database, &stale).unwrap();

        assert_eq!(load_hunt_timer(&database, "a").unwrap(), Some(newer.timer));
    }

    #[test]
    fn removing_account_also_removes_its_persisted_hunt_timer() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO accounts(id, nick, card_color, created_at) VALUES ('a', 'Alpha', '#fff', unixepoch())",
                [],
            )
            .unwrap();
        persist_hunt_timer(
            &database,
            &accounts::HuntTimerPersistenceChange {
                account_id: "a".into(),
                timer: HuntTimerState {
                    hunt_slug: "ancient_pupitar".into(),
                    started_at_ms: 1_000,
                    revision: 1,
                },
            },
        )
        .unwrap();

        remove_persisted_account(&database, "a").unwrap();

        assert!(load_hunt_timer(&database, "a").unwrap().is_none());
    }

    #[test]
    fn startup_browser_concurrency_accepts_only_the_supported_range() {
        for value in 1..=MAX_STARTUP_BROWSER_CONCURRENCY {
            assert_eq!(normalize_startup_browser_concurrency(value), value);
        }
        assert_eq!(normalize_startup_browser_concurrency(0), 1);
        assert_eq!(normalize_startup_browser_concurrency(5), 1);
    }

    #[test]
    fn corrupt_or_missing_startup_concurrency_falls_back_to_one() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO app_settings(key, value) VALUES ('startup_browser_concurrency', 'not-a-number')",
                [],
            )
            .unwrap();
        assert_eq!(
            load_settings(&database)
                .unwrap()
                .startup_browser_concurrency,
            1
        );
    }

    #[test]
    fn startup_concurrency_round_trips_from_persisted_settings() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO app_settings(key, value) VALUES ('startup_browser_concurrency', '3')",
                [],
            )
            .unwrap();
        assert_eq!(
            load_settings(&database)
                .unwrap()
                .startup_browser_concurrency,
            3
        );
    }

    #[tokio::test]
    async fn startup_bootstrap_gate_honors_each_supported_limit_and_releases_permits() {
        for limit in [1_usize, 2, 4] {
            let gate = Arc::new(Semaphore::new(limit));
            let mut permits = Vec::new();
            for _ in 0..limit {
                permits.push(gate.clone().acquire_owned().await.unwrap());
            }
            assert!(gate.clone().try_acquire_owned().is_err());
            drop(permits.pop());
            assert!(gate.clone().try_acquire_owned().is_ok());
        }

        async fn simulated_bootstrap(
            gate: Arc<Semaphore>,
            succeeds: bool,
        ) -> Result<(), &'static str> {
            let _permit = gate.acquire_owned().await.unwrap();
            if succeeds {
                Ok(())
            } else {
                Err("bootstrap failed")
            }
        }

        let gate = Arc::new(Semaphore::new(1));
        assert!(simulated_bootstrap(gate.clone(), true).await.is_ok());
        assert!(gate.clone().try_acquire_owned().is_ok());
        assert!(simulated_bootstrap(gate.clone(), false).await.is_err());
        assert!(gate.clone().try_acquire_owned().is_ok());
    }

    #[tokio::test]
    async fn queued_startup_bootstraps_all_begin_after_earlier_permits_finish() {
        let gate = Arc::new(Semaphore::new(2));
        let (started_sender, mut started_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut tasks = Vec::new();

        for account_index in 0..MAX_ACCOUNTS {
            let gate = gate.clone();
            let started_sender = started_sender.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = gate.acquire_owned().await.unwrap();
                started_sender.send(account_index).unwrap();
                tokio::task::yield_now().await;
            }));
        }
        drop(started_sender);

        let mut started = Vec::new();
        while let Some(account_index) = started_receiver.recv().await {
            started.push(account_index);
        }
        for task in tasks {
            task.await.unwrap();
        }

        started.sort_unstable();
        assert_eq!(started, (0..MAX_ACCOUNTS).collect::<Vec<_>>());
    }

    #[test]
    fn only_automatic_background_bootstrap_uses_the_startup_gate() {
        assert!(mode_uses_startup_bootstrap_gate(
            browser::BrowserSessionMode::BackgroundBootstrap
        ));
        assert!(!mode_uses_startup_bootstrap_gate(
            browser::BrowserSessionMode::Interactive
        ));
        assert!(!mode_uses_startup_bootstrap_gate(
            browser::BrowserSessionMode::InteractiveOwner
        ));
        assert!(!mode_uses_startup_bootstrap_gate(
            browser::BrowserSessionMode::ReconnectExistingOwner
        ));
    }

    #[test]
    fn local_multi_account_preferences_round_trip_without_session_material() {
        let database = Connection::open_in_memory().unwrap();
        persistence::migrate(&database).unwrap();
        database
            .execute(
                "INSERT INTO accounts(id, nick, card_color, created_at) VALUES ('a', 'Alpha', '#fff', unixepoch())",
                [],
            )
            .unwrap();
        let preferences = LocalAutomationPreferences {
            potion_ids: vec![204, 202],
            ball_ids: vec![4, 3, 2],
            capture_mode: CaptureMode::Continuous,
        };
        persist_automation_preferences(&database, "a", &preferences).unwrap();
        assert_eq!(
            load_automation_preferences(&database, "a")
                .unwrap()
                .ball_ids,
            vec![4, 3, 2]
        );
        assert_eq!(
            load_automation_preferences(&database, "a")
                .unwrap()
                .capture_mode,
            CaptureMode::Continuous
        );
        let rules = vec![crate::domain::AutoBuyRule {
            kind: crate::domain::AutoBuyKind::Ball,
            item_id: 4,
            minimum: 100,
            quantity: 500,
            enabled: true,
            status: None,
        }];
        persist_auto_buy_rules(&database, "a", &rules).unwrap();
        let restored = load_auto_buy_rules(&database, "a").unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].item_id, 4);

        let mut disabled_rules = rules;
        disabled_rules[0].enabled = false;
        persist_auto_buy_rules(&database, "a", &disabled_rules).unwrap();
        let stored_enabled: i64 = database
            .query_row(
                "SELECT enabled FROM automation_settings WHERE account_id = 'a' AND automation_key = 'auto_buy_rules'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_enabled, 0);
    }
}
