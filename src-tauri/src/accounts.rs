#[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
use crate::mobile::{MobileAccountProjection, MobileSnapshot};
use crate::{
    connection::BackgroundConnection,
    domain::{
        AccountActivity, AccountDataRevisions, AccountDepotPokemon, AccountHuntOption,
        AccountHuntSpecies, AccountInventoryEntry, AccountLiveHunt, AccountLiveMetrics,
        AccountLiveSnapshot, AccountLiveState, AccountMode, AccountRecord, AccountRuntimeState,
        AccountSnapshot, AccountState, AutoBuyKind, AutoBuyRule, CaptureMode, ConnectionOwner,
        ConnectionStatus, HuntSession, HuntTimerState, MAX_ACCOUNTS, PendingNavigationIntent,
        Pokemon, PotionUsageSource, VersionedAccountRead, WildPokemon,
    },
    events::EventBus,
    inspector::{self, ProtocolDirection, ProtocolFrame},
    market::MarketSignal,
    metrics::MetricsEngine,
    protocol::{
        ClientFrame, RemoteAutomation, RemoteHunt, RemotePokemonPatch, RemoteState, ServerFrame,
    },
};
use parking_lot::Mutex;
use serde_json::{Map, Value};
#[cfg(debug_assertions)]
use std::sync::atomic::AtomicU64 as ValidationAtomicU64;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum BrowserControl {
    HandoffToBackground,
    SendFrame(ClientFrame),
    /// Reloads the managed game page without sending a game-protocol frame.
    /// This control is exposed only to isolated debug validation.
    ReloadPage,
    /// Closes only the Brave instance launched and owned by this runtime. This
    /// is deliberately a CDP command, never a process-name kill, so personal
    /// Brave windows are not affected during shutdown or account removal.
    CloseManagedBrowser,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandTransport {
    Background,
    Browser,
}
/// The only route by which Manager commands leave an account runtime. Command
/// producers intentionally do not know who owns the game's single WebSocket.
struct AccountCommandDispatcher;
impl AccountCommandDispatcher {
    fn route(runtime: &AccountRuntime) -> Result<CommandTransport, AccountError> {
        command_transport_for(
            runtime.account.owner.clone(),
            runtime.background.is_some(),
            runtime.browser_control.is_some(),
            runtime.browser_transport_ready,
        )
    }
    fn send(runtime: &AccountRuntime, command: ClientFrame) -> Result<(), AccountError> {
        Self::dispatch(runtime, command, |transport, command| match transport {
            CommandTransport::Background => runtime
                .background
                .as_ref()
                .expect("route verified Background transport")
                .dispatch(command)
                .map_err(|error| AccountError::NotFound(error.to_string())),
            CommandTransport::Browser => runtime
                .browser_control
                .as_ref()
                .expect("route verified Browser transport")
                .send(BrowserControl::SendFrame(command))
                .map_err(|_| AccountError::NotFound("Sessão do navegador foi encerrada".into())),
        })
    }
    fn dispatch(
        runtime: &AccountRuntime,
        command: ClientFrame,
        send: impl FnOnce(CommandTransport, ClientFrame) -> Result<(), AccountError>,
    ) -> Result<(), AccountError> {
        Self::dispatch_with_route(runtime, command, Self::route, send)
    }
    fn dispatch_with_route(
        runtime: &AccountRuntime,
        command: ClientFrame,
        route: impl FnOnce(&AccountRuntime) -> Result<CommandTransport, AccountError>,
        send: impl FnOnce(CommandTransport, ClientFrame) -> Result<(), AccountError>,
    ) -> Result<(), AccountError> {
        #[cfg(debug_assertions)]
        if runtime.validation_read_only.load(Ordering::Acquire)
            && !command.is_read_only_validation_query()
        {
            if is_known_mutating_validation_command(&command) {
                runtime
                    .validation_command_counters
                    .mutable_attempts
                    .fetch_add(1, Ordering::AcqRel);
                runtime
                    .validation_command_counters
                    .mutable_blocked
                    .fetch_add(1, Ordering::AcqRel);
            }
            return Err(AccountError::ValidationReadOnly);
        }
        #[cfg(debug_assertions)]
        let mutable = is_known_mutating_validation_command(&command);
        let result = send(route(runtime)?, command);
        #[cfg(debug_assertions)]
        if mutable && result.is_ok() {
            runtime
                .validation_command_counters
                .mutable_sent
                .fetch_add(1, Ordering::AcqRel);
        }
        result
    }
}

#[cfg(debug_assertions)]
fn is_known_mutating_validation_command(command: &ClientFrame) -> bool {
    matches!(
        command,
        ClientFrame::HuntSelect { .. }
            | ClientFrame::CenterGo
            | ClientFrame::AutoSet { .. }
            | ClientFrame::BallThrow { .. }
            | ClientFrame::AutoSaleLoot { .. }
            | ClientFrame::ShopBuy { .. }
            | ClientFrame::SellPokemon { .. }
            | ClientFrame::SellAllPokemons
            | ClientFrame::MarketBuy { .. }
            | ClientFrame::FriendRequest { .. }
    )
}

/// Debug-only, read-only view of the transport state used by the account
/// command dispatcher. Presence/readiness are reported separately from the
/// selected transport so a handoff cannot be mistaken for an active owner.
#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTransportDiagnostics {
    pub account_id: String,
    pub owner: ConnectionOwner,
    pub browser_control_attached: bool,
    pub browser_transport_ready: bool,
    pub background_transport_attached: bool,
    /// The transport the dispatcher would route a command to now. `None`
    /// means the account has no dispatchable owner (including Transition).
    pub selected_transport: Option<AccountDiagnosticTransport>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ValidationCommandCountersSnapshot {
    pub mutable_attempts: u64,
    pub mutable_blocked: u64,
    pub mutable_sent: u64,
}

#[cfg(debug_assertions)]
#[derive(Default)]
struct ValidationCommandCounters {
    mutable_attempts: ValidationAtomicU64,
    mutable_blocked: ValidationAtomicU64,
    mutable_sent: ValidationAtomicU64,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountDiagnosticTransport {
    Browser,
    Background,
}
fn command_transport_for(
    owner: ConnectionOwner,
    has_background: bool,
    has_browser: bool,
    browser_ready: bool,
) -> Result<CommandTransport, AccountError> {
    match owner {
        ConnectionOwner::Background if has_background => Ok(CommandTransport::Background),
        ConnectionOwner::Browser if has_browser && browser_ready => Ok(CommandTransport::Browser),
        ConnectionOwner::Transition => Err(AccountError::NotFound(
            "Conta em transferência; aguarde a conclusão".into(),
        )),
        ConnectionOwner::Background => Err(AccountError::NotFound(
            "Conta Background sem transporte disponível".into(),
        )),
        ConnectionOwner::Browser => Err(AccountError::NotFound(
            "Transporte do navegador ainda não está pronto".into(),
        )),
        ConnectionOwner::None => Err(AccountError::NotFound(
            "Conta offline ou login necessário".into(),
        )),
    }
}

#[derive(Clone)]
struct AutomationMutation {
    field: String,
    value: Value,
}
struct InFlightAutomationMutation {
    mutation: AutomationMutation,
    sent_at_ms: u64,
}
#[derive(Default)]
struct AutomationWriteQueue {
    pending: VecDeque<AutomationMutation>,
    in_flight: Option<InFlightAutomationMutation>,
}
#[derive(Clone)]
struct AutoBuyInFlight {
    kind: AutoBuyKind,
    item_id: u64,
    sent_at_ms: u64,
    confirmed: bool,
}
#[derive(Clone)]
struct PendingPokemonSale {
    pokemon_id: u64,
    sent_at_ms: u64,
}
#[derive(Clone)]
struct CaptureCorpse {
    slot: u64,
    name: String,
    species_id: Option<u64>,
    died_at_server_time: u64,
    expires_at_server_time: u64,
    attempt_in_flight: bool,
    retry_after_server_time: u64,
}
const COMMAND_CONFIRMATION_TIMEOUT_MS: u64 = 8_000;
const CAPTURE_WINDOW_MS: u64 = 30_000;
const BALL_COOLDOWN_MS: u64 = 1_200;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("Limite de 4 contas atingido. Remova uma conta para adicionar outra.")]
    LimitReached,
    #[error("A conta {0} já existe.")]
    Duplicate(String),
    #[error("Conta não encontrada: {0}")]
    NotFound(String),
    #[cfg(debug_assertions)]
    #[error("Comando de alteração bloqueado no modo de validação somente leitura")]
    ValidationReadOnly,
    #[cfg(debug_assertions)]
    #[error("Operação disponível somente no modo de validação isolada")]
    ValidationModeRequired,
}
pub struct AccountRuntime {
    pub account: AccountRecord,
    state: AccountState,
    metrics: MetricsEngine,
    data_revisions: AccountDataRevisions,
    revision_clock: Arc<AtomicU64>,
    pub bus: EventBus,
    background: Option<BackgroundConnection>,
    browser_control: Option<mpsc::UnboundedSender<BrowserControl>>,
    browser_transport_ready: bool,
    inspector_frames: VecDeque<ProtocolFrame>,
    latest_server_automation: Option<Map<String, Value>>,
    automation_writes: AutomationWriteQueue,
    auto_buy_in_flight: Option<AutoBuyInFlight>,
    corpses: VecDeque<CaptureCorpse>,
    last_ball_throw_at_server_time: Option<u64>,
    navigation_sent_at_ms: Option<u64>,
    /// Inventory is authoritative, but an initial/reconnected full snapshot is
    /// not evidence that a Potion was used.
    inventory_baseline_ready: bool,
    inventory_resync_pending: bool,
    /// `k:"compra"` confirms our own item purchase but has no item id. Suppress
    /// one matching inventory reconciliation for the known local request.
    suppress_potion_delta_for_item: Option<u64>,
    /// Depot changes stay authoritative. These ids only let us reconcile an
    /// explicitly confirmed individual sale, never optimistically.
    pending_pokemon_sale: Option<PendingPokemonSale>,
    inventory_refresh_pending: bool,
    /// Only automatic capture completion needs deferred persistence. Explicit
    /// user changes are persisted by the command that receives the request.
    capture_mode_dirty: bool,
    hunt_timer: Option<HuntTimerState>,
    hunt_timer_revision: u64,
    hunt_timer_dirty: bool,
    hunt_session_revision: u64,
    hunt_session_dirty: bool,
    #[cfg(debug_assertions)]
    validation_read_only: Arc<AtomicBool>,
    #[cfg(debug_assertions)]
    validation_command_counters: Arc<ValidationCommandCounters>,
}

/// Fields that the on-demand depot command actually exposes. Runtime-only
/// Pokémon data such as HP/XP changes continuously during combat and must not
/// invalidate the much larger depot response.
#[derive(Debug, PartialEq)]
struct DepotPokemonProjection {
    id: u64,
    name: String,
    level: u32,
    types: Vec<String>,
    power: Option<u8>,
    quality: Option<f64>,
    note: Option<f64>,
    iv_total: Option<u64>,
    shiny: bool,
    species_id: Option<u64>,
    looktype: Option<u64>,
    look_shiny: Option<u64>,
}

impl From<&Pokemon> for DepotPokemonProjection {
    fn from(pokemon: &Pokemon) -> Self {
        Self {
            id: pokemon.id,
            name: pokemon.name.clone(),
            level: pokemon.level,
            types: pokemon.types.clone(),
            power: pokemon.potencia,
            quality: pokemon.quality,
            note: pokemon.nota,
            iv_total: pokemon.iv_total,
            shiny: pokemon.shiny,
            species_id: pokemon.species_id,
            looktype: pokemon.looktype,
            look_shiny: pokemon.look_shiny,
        }
    }
}

fn same_depot_projection(left: &[Pokemon], right: &[Pokemon]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            DepotPokemonProjection::from(left) == DepotPokemonProjection::from(right)
        })
}

impl AccountRuntime {
    fn new(account: AccountRecord) -> Self {
        Self::with_validation_guard(account, Arc::new(AtomicBool::new(false)))
    }
    fn with_validation_guard(
        account: AccountRecord,
        _validation_read_only: Arc<AtomicBool>,
    ) -> Self {
        Self {
            account,
            state: AccountState::default(),
            metrics: MetricsEngine::default(),
            data_revisions: AccountDataRevisions::default(),
            revision_clock: Arc::new(AtomicU64::new(0)),
            bus: EventBus::new(),
            background: None,
            browser_control: None,
            browser_transport_ready: false,
            inspector_frames: VecDeque::with_capacity(inspector::FRAME_CAPACITY),
            latest_server_automation: None,
            automation_writes: AutomationWriteQueue::default(),
            auto_buy_in_flight: None,
            corpses: VecDeque::new(),
            last_ball_throw_at_server_time: None,
            navigation_sent_at_ms: None,
            inventory_baseline_ready: false,
            inventory_resync_pending: false,
            suppress_potion_delta_for_item: None,
            pending_pokemon_sale: None,
            inventory_refresh_pending: false,
            capture_mode_dirty: false,
            hunt_timer: None,
            hunt_timer_revision: 0,
            hunt_timer_dirty: false,
            hunt_session_revision: 0,
            hunt_session_dirty: false,
            #[cfg(debug_assertions)]
            validation_read_only: _validation_read_only,
            #[cfg(debug_assertions)]
            validation_command_counters: Arc::new(ValidationCommandCounters::default()),
        }
    }
    fn next_data_revision(&self) -> u64 {
        self.revision_clock
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                Some(revision.saturating_add(1))
            })
            .unwrap_or(u64::MAX)
            .saturating_add(1)
    }
    fn initialize_data_revisions(&mut self) {
        let revision = self.next_data_revision();
        self.data_revisions = AccountDataRevisions {
            depot: revision,
            inventory: revision,
            hunt_options: revision,
        };
    }
    fn reconcile_hunt_timer(&mut self, hunt_slug: &str, observed_at_ms: u64) {
        if self
            .hunt_timer
            .as_ref()
            .is_some_and(|timer| timer.hunt_slug == hunt_slug)
        {
            self.state.hunt_started_at_ms =
                self.hunt_timer.as_ref().map(|timer| timer.started_at_ms);
            return;
        }
        let started_at_ms = self.hunt_timer.as_ref().map_or(observed_at_ms, |timer| {
            observed_at_ms.max(timer.started_at_ms.saturating_add(1))
        });
        let revision = self.hunt_timer_revision.saturating_add(1);
        let timer = HuntTimerState {
            hunt_slug: hunt_slug.to_owned(),
            started_at_ms,
            revision,
        };
        self.state.hunt_started_at_ms = Some(timer.started_at_ms);
        self.hunt_timer_revision = revision;
        self.hunt_timer = Some(timer);
        self.hunt_timer_dirty = true;
    }
    fn clear_hunt_timer(&mut self) {
        if self.hunt_timer.is_none() && self.state.hunt_started_at_ms.is_none() {
            return;
        }
        self.hunt_timer_revision = self.hunt_timer_revision.saturating_add(1);
        self.hunt_timer = None;
        self.state.hunt_started_at_ms = None;
        self.hunt_timer_dirty = true;
    }
    fn apply_state(&mut self, incoming: RemoteState, initialize_hunt: bool) {
        let inventory_patch = incoming.items.is_some();
        let balls_patch = incoming.balls.is_some();
        macro_rules! copy {
            ($field:ident) => {
                if incoming.$field.is_some() {
                    self.state.$field = incoming.$field;
                }
            };
        }
        copy!(level);
        copy!(xp);
        copy!(gold);
        copy!(diamonds);
        copy!(orbs);
        copy!(vip_until);
        if let Some(server_now) = incoming.server_now {
            self.state.server_now = Some(server_now);
            self.state.server_offset_ms = Some(
                (server_now as i128)
                    .saturating_sub(now_ms() as i128)
                    .clamp(i64::MIN as i128, i64::MAX as i128) as i64,
            );
        }
        if let Some(event) = incoming.xp_event {
            self.state.xp_bonus.event_trainer_pct = Some(event.trainer_pct.unwrap_or(0.0));
            self.state.xp_bonus.event_pokemon_pct = Some(event.pokemon_pct.unwrap_or(0.0));
        }
        if incoming.guild_bonus_pct.is_some() {
            self.state.xp_bonus.guild_rank_pct = incoming.guild_bonus_pct;
        }
        if let Some(guild) = incoming.guild {
            self.state.guild_boost_until = guild.boost_until;
        }
        if let (Some(boost_until), Some(server_now)) =
            (self.state.guild_boost_until, self.state.server_now)
        {
            self.state.xp_bonus.guild_boost_active = Some(boost_until > server_now);
        }
        if let Some(twitch) = incoming.twitch {
            match twitch.watching {
                Some(false) => self.state.xp_bonus.twitch_pct = Some(0.0),
                Some(true) => {
                    self.state.xp_bonus.twitch_pct =
                        Some(twitch.current_pct.or(twitch.pct).unwrap_or(0.0).max(0.0));
                }
                None => {
                    if let Some(pct) = twitch.current_pct.or(twitch.pct) {
                        self.state.xp_bonus.twitch_pct = Some(pct.max(0.0));
                    }
                }
            }
        }
        if let Some(center_free_at) = incoming.center_free_at {
            self.state.center_free_at = Some(center_free_at);
            self.state.combat_lock_until = Some(center_free_at);
        }
        self.refresh_combat_lock();
        if let Some(loja) = incoming.loja {
            self.state.vip_data_available = true;
            // Confirmed from a real `welcome`: the VIP status lives inside
            // `estado.loja`, rather than relying on the legacy top-level
            // fields alone. Keep other store fields opaque until needed.
            if let Some(vip_until) = loja.get("vipAte").and_then(Value::as_u64) {
                self.state.vip_until = Some(vip_until);
            }
            if let Some(vip_active) = loja.get("vip").and_then(Value::as_bool) {
                self.state.vip_active = Some(vip_active);
            }
            self.state.vip_store = Some(loja);
        }
        // `estado.huntSlug` alone is not the confirmation for a user-requested
        // hunt change. Only the initial welcome may establish the baseline;
        // subsequent selections are committed by the `k:"hunt"` event.
        let initial_hunt_slug = initialize_hunt
            .then(|| incoming.hunt_slug.clone())
            .flatten();
        if initialize_hunt {
            copy!(hunt_slug);
        }
        if let Some(hunt_slug) = initial_hunt_slug.as_deref() {
            self.reconcile_hunt_timer(hunt_slug, now_ms());
        }
        if let Some(no_centro) = incoming.no_centro {
            let was_in_center = self.state.no_centro;
            self.state.no_centro = no_centro;
            if no_centro {
                self.state.pending_navigation = None;
                self.navigation_sent_at_ms = None;
                if !was_in_center {
                    self.metrics = MetricsEngine::default();
                    self.clear_hunt_timer();
                }
            }
        }
        if self.state.no_centro {
            self.state.activity = AccountActivity::PokemonCenter;
        } else {
            if let Some(hunt_slug) = self.state.hunt_slug.clone() {
                if self.state.hunt_session.is_none() {
                    self.state.hunt_session = Some(HuntSession {
                        hunt_slug: hunt_slug.clone(),
                        started_at_ms: now_ms(),
                        ..Default::default()
                    });
                } else if let Some(session) = self.state.hunt_session.as_mut() {
                    // A welcome after reconnect can report a newer hunt; keep
                    // the continuous session totals and update only its label.
                    session.hunt_slug = hunt_slug.clone();
                }
                if initialize_hunt || incoming.no_centro == Some(false) {
                    self.state.activity = AccountActivity::Farming {
                        hunt_slug: hunt_slug.clone(),
                    };
                }
            } else if incoming.no_centro.is_some() {
                self.state.activity = AccountActivity::Idle;
            }
        }
        copy!(active_id);
        if let Some(items) = incoming.items {
            if items != self.state.items {
                self.data_revisions.inventory = self.next_data_revision();
            }
            self.observe_potion_inventory_delta(&items, initialize_hunt);
            self.state.items = items;
            self.inventory_baseline_ready = true;
            self.inventory_resync_pending = false;
            self.suppress_potion_delta_for_item = None;
        }
        if let Some(balls) = incoming.balls {
            if balls != self.state.balls {
                self.data_revisions.inventory = self.next_data_revision();
            }
            self.state.balls = balls;
        }
        if let Some(pokemons) = incoming.pokemons {
            let projected: Vec<Pokemon> = pokemons
                .into_iter()
                .map(|p| Pokemon {
                    id: p.id,
                    name: p.name,
                    level: p.level,
                    hp: p.hp,
                    max_hp: p.max_hp,
                    xp: p.xp,
                    xp_level: p.xp_level,
                    xp_next: p.xp_next,
                    quality: p.quality,
                    potencia: p.potencia,
                    poder: p.poder,
                    nota: p.nota,
                    shiny: p.shiny.unwrap_or(false),
                    species_id: p.species_id,
                    looktype: p.looktype,
                    look_shiny: p.look_shiny,
                    held_item_id: p.held_item_id,
                    types: p.tipos.unwrap_or_default(),
                    iv_total: p.ivs.as_ref().and_then(|ivs| {
                        Some(ivs.hp? + ivs.atk? + ivs.def? + ivs.sp_atk? + ivs.sp_def? + ivs.speed?)
                    }),
                    ivs: p.ivs.map(|ivs| crate::domain::PokemonIvs {
                        hp: ivs.hp,
                        atk: ivs.atk,
                        def: ivs.def,
                        sp_atk: ivs.sp_atk,
                        sp_def: ivs.sp_def,
                        speed: ivs.speed,
                    }),
                    patch_fields: Map::new(),
                })
                .collect();
            if !same_depot_projection(&self.state.pokemon, &projected) {
                self.data_revisions.depot = self.next_data_revision();
            }
            self.state.pokemon = projected;
            self.inventory_refresh_pending = false;
        }
        if let Some(patches) = incoming.pokemon_patches {
            if self.apply_pokemon_patches(patches) {
                self.data_revisions.depot = self.next_data_revision();
            }
        }
        if let Some(wild) = incoming.selvagem {
            self.state.wild = Some(WildPokemon {
                slot: wild.slot,
                species_id: wild.species_id,
                name: wild.name,
                level: wild.level,
                hp: wild.hp,
                max_hp: wild.max_hp,
            });
        }
        if let Some(automation) = incoming.automation {
            self.apply_automation(automation);
        }
        if inventory_patch || balls_patch {
            self.complete_auto_buy_after_inventory(inventory_patch, balls_patch);
        }
        self.process_due_actions();
    }
    fn observe_potion_inventory_delta(
        &mut self,
        updated_items: &std::collections::BTreeMap<String, u64>,
        initialize_hunt: bool,
    ) {
        if initialize_hunt || !self.inventory_baseline_ready || self.inventory_resync_pending {
            return;
        }
        let ignored_item = self.suppress_potion_delta_for_item;
        for potion_id in &self.state.automation.potion_ids {
            if Some(*potion_id) == ignored_item {
                continue;
            }
            let key = potion_id.to_string();
            let before = self.state.items.get(&key).copied().unwrap_or(0);
            let after = updated_items.get(&key).copied().unwrap_or(0);
            if after >= before {
                continue;
            }
            let used = before.saturating_sub(after);
            self.state.active_potion_id = Some(*potion_id);
            self.state.active_potion_source = Some(PotionUsageSource::InventoryDelta);
            for _ in 0..used {
                self.metrics.record_potion(*potion_id);
            }
            tracing::debug!(account_id = %self.account.id, potion_id, used, "Potion use inferred from reconciled inventory decrease");
        }
    }
    fn refresh_combat_lock(&mut self) {
        self.state.combat_locked = self
            .state
            .combat_lock_until
            .zip(self.state.server_offset_ms)
            .is_some_and(|(deadline, offset)| estimated_server_now(offset) < deadline);
    }
    fn apply_pokemon_patches(&mut self, patches: Vec<RemotePokemonPatch>) -> bool {
        let mut changed = false;
        for patch in patches {
            let Some(pokemon) = self
                .state
                .pokemon
                .iter_mut()
                .find(|pokemon| pokemon.id == patch.id)
            else {
                continue;
            };
            let previous = DepotPokemonProjection::from(&*pokemon);
            merge_pokemon_patch(pokemon, patch.fields);
            changed |= DepotPokemonProjection::from(&*pokemon) != previous;
        }
        changed
    }
    fn apply_automation(&mut self, automation: Value) {
        let Some(raw) = automation.as_object().cloned() else {
            return;
        };
        let typed = serde_json::from_value::<RemoteAutomation>(automation).unwrap_or_default();
        self.latest_server_automation = Some(raw.clone());
        {
            macro_rules! auto {
                ($field:ident) => {
                    if typed.$field.is_some() {
                        self.state.automation.$field = typed.$field;
                    }
                };
            }
            auto!(auto_potion);
            auto!(hp_threshold);
            auto!(auto_revive);
            auto!(auto_sale_loot);
            auto!(auto_lock_shiny);
            auto!(auto_lock_nota9);
            auto!(auto_lock_nota_min);
            auto!(auto_lock_p5);
            auto!(auto_return_hunt);
            if let Some(potion_ids) = typed.potion_ids {
                self.state.automation.potion_ids = potion_ids;
                self.reconcile_auto_buy_selection(AutoBuyKind::Item);
            }
            if let Some(ball_ids) = typed.ball_ids {
                self.state.automation.ball_ids = ball_ids;
                self.reconcile_auto_buy_selection(AutoBuyKind::Ball);
            }
            if let Some(revive_ids) = typed.revive_ids {
                self.state.automation.revive_ids = revive_ids;
            }
        }
        let confirmed = self
            .automation_writes
            .in_flight
            .as_ref()
            .is_some_and(|in_flight| {
                raw.get(&in_flight.mutation.field) == Some(&in_flight.mutation.value)
            });
        if confirmed {
            tracing::debug!(account_id = %self.account.id, "automation mutation confirmed and released");
            self.automation_writes.in_flight = None;
            self.state.automation_error = None;
            let _ = self.send_next_automation();
        }
    }
    fn queue_automation_change(&mut self, field: &str, value: Value) -> Result<(), AccountError> {
        if self.latest_server_automation.is_none() {
            return Err(AccountError::NotFound(
                "Aguardando automation autoritativa do servidor".into(),
            ));
        }
        self.automation_writes
            .pending
            .push_back(AutomationMutation {
                field: field.to_owned(),
                value,
            });
        tracing::debug!(account_id = %self.account.id, field, "automation mutation queued");
        self.send_next_automation()
    }
    /// Keeps local refill rules aligned with the authoritative `auto.set`
    /// selection. A selected replacement inherits the previous active rule's
    /// limits; removed items remain saved but cannot keep buying in the
    /// background.
    fn reconcile_auto_buy_selection(&mut self, kind: AutoBuyKind) {
        let selected_ids = match kind {
            AutoBuyKind::Item => self.state.automation.potion_ids.clone(),
            AutoBuyKind::Ball => self.state.automation.ball_ids.clone(),
        };
        let selected_ids = selected_ids
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let Some(template) = self
            .state
            .auto_buy_rules
            .iter()
            .find(|rule| rule.kind == kind && rule.enabled)
            .cloned()
        else {
            return;
        };

        for rule in self
            .state
            .auto_buy_rules
            .iter_mut()
            .filter(|rule| rule.kind == kind)
        {
            rule.enabled = selected_ids.contains(&rule.item_id);
            rule.status = None;
        }
        for item_id in selected_ids {
            if self
                .state
                .auto_buy_rules
                .iter()
                .any(|rule| rule.kind == kind && rule.item_id == item_id)
            {
                continue;
            }
            self.state.auto_buy_rules.push(AutoBuyRule {
                kind: kind.clone(),
                item_id,
                minimum: template.minimum,
                quantity: template.quantity,
                enabled: true,
                status: None,
            });
        }
    }
    fn send_next_automation(&mut self) -> Result<(), AccountError> {
        if self.automation_writes.in_flight.is_some() {
            return Ok(());
        }
        while let Some(mutation) = self.automation_writes.pending.pop_front() {
            let Some(mut payload) = self.latest_server_automation.clone() else {
                self.state.automation_error = Some("Automation autoritativa ausente".into());
                return Err(AccountError::NotFound(
                    "Automation autoritativa ausente".into(),
                ));
            };
            payload.insert(mutation.field.clone(), mutation.value.clone());
            tracing::debug!(account_id = %self.account.id, field = %mutation.field, "automation frame send requested");
            match AccountCommandDispatcher::send(
                self,
                ClientFrame::AutoSet {
                    automation: payload,
                },
            ) {
                Ok(()) => {
                    tracing::debug!(account_id = %self.account.id, field = %mutation.field, "automation frame sent; awaiting confirmation");
                    self.automation_writes.in_flight = Some(InFlightAutomationMutation {
                        mutation,
                        sent_at_ms: now_ms(),
                    });
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(account_id = %self.account.id, field = %mutation.field, %error, "automation mutation failed before confirmation; releasing queue");
                    self.state.automation_error = Some(error.to_string());
                }
            }
        }
        Ok(())
    }
    fn estimated_server_now(&self) -> u64 {
        estimated_server_now(self.state.server_offset_ms.unwrap_or(0))
    }
    fn clear_uncertain_game_state(&mut self) {
        // After a full/rebound game socket we cannot prove that old corpses or
        // an old navigation request still exist on the authoritative page.
        self.corpses.clear();
        self.state.capture_queue_len = 0;
        self.state.pending_navigation = None;
        self.state.pending_hunt_slug = None;
        self.navigation_sent_at_ms = None;
        self.auto_buy_in_flight = None;
        self.inventory_resync_pending = true;
        self.suppress_potion_delta_for_item = None;
        self.pending_pokemon_sale = None;
        self.inventory_refresh_pending = false;
    }
    fn process_due_actions(&mut self) {
        #[cfg(debug_assertions)]
        if self.validation_read_only.load(Ordering::Acquire) {
            return;
        }
        self.refresh_combat_lock();
        let now = self.estimated_server_now();
        if self
            .automation_writes
            .in_flight
            .as_ref()
            .is_some_and(|in_flight| {
                now_ms().saturating_sub(in_flight.sent_at_ms) >= COMMAND_CONFIRMATION_TIMEOUT_MS
            })
        {
            let field = self
                .automation_writes
                .in_flight
                .take()
                .map(|in_flight| in_flight.mutation.field)
                .unwrap_or_default();
            tracing::warn!(account_id = %self.account.id, field, "automation confirmation timed out; releasing queue");
            self.state.automation_error = Some(
                "A confirmação da automação expirou; as próximas ações foram liberadas.".into(),
            );
            let _ = self.send_next_automation();
        }
        if self.pending_pokemon_sale.as_ref().is_some_and(|pending| {
            now_ms().saturating_sub(pending.sent_at_ms) >= COMMAND_CONFIRMATION_TIMEOUT_MS
        }) {
            self.pending_pokemon_sale = None;
        }
        self.corpses
            .retain(|corpse| now < corpse.expires_at_server_time);
        self.state.capture_queue_len = self.corpses.len();
        self.pump_pending_navigation();
        self.pump_auto_buy();
        self.pump_capture_worker(now);
    }
    fn pump_pending_navigation(&mut self) {
        if self.state.combat_locked || self.navigation_sent_at_ms.is_some() {
            return;
        }
        let Some(intent) = self.state.pending_navigation.clone() else {
            return;
        };
        if matches!(
            self.account.owner,
            ConnectionOwner::Transition | ConnectionOwner::None
        ) {
            return;
        }
        let command = match &intent {
            PendingNavigationIntent::Hunt { slug } => {
                ClientFrame::HuntSelect { slug: slug.clone() }
            }
            PendingNavigationIntent::Center => ClientFrame::CenterGo,
        };
        tracing::debug!(account_id = %self.account.id, ?intent, "pending navigation frame send requested");
        match AccountCommandDispatcher::send(self, command) {
            Ok(()) => {
                self.navigation_sent_at_ms = Some(now_ms());
                self.state.navigation_error = None;
                if let PendingNavigationIntent::Hunt { slug } = intent {
                    self.state.pending_hunt_slug = Some(slug);
                }
            }
            Err(error) => {
                self.state.navigation_error = Some(error.to_string());
                tracing::warn!(account_id = %self.account.id, %error, "pending navigation is waiting for a usable transport");
            }
        }
    }
    fn pump_auto_buy(&mut self) {
        if let Some(in_flight) = self.auto_buy_in_flight.as_ref() {
            if now_ms().saturating_sub(in_flight.sent_at_ms) < COMMAND_CONFIRMATION_TIMEOUT_MS {
                return;
            }
            let failed = self.auto_buy_in_flight.take().expect("checked above");
            if let Some(rule) = self
                .state
                .auto_buy_rules
                .iter_mut()
                .find(|rule| rule.kind == failed.kind && rule.item_id == failed.item_id)
            {
                rule.status = Some("Compra sem confirmação; reposição pausada.".into());
                rule.enabled = false;
            }
            return;
        }
        let configured_ids = self
            .state
            .automation
            .potion_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let configured_ball_ids = self
            .state
            .automation
            .ball_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let Some(rule) = self
            .state
            .auto_buy_rules
            .iter()
            .find(|rule| {
                if !rule.enabled || !(1..=9_999).contains(&rule.quantity) {
                    return false;
                }
                let selected = match rule.kind {
                    AutoBuyKind::Item => configured_ids.contains(&rule.item_id),
                    AutoBuyKind::Ball => configured_ball_ids.contains(&rule.item_id),
                };
                if !selected {
                    return false;
                }
                let stock = match rule.kind {
                    AutoBuyKind::Item => self.state.items.get(&rule.item_id.to_string()),
                    AutoBuyKind::Ball => self.state.balls.get(&rule.item_id.to_string()),
                }
                .copied()
                .unwrap_or(0);
                stock < rule.minimum
            })
            .cloned()
        else {
            return;
        };
        let kind = match rule.kind {
            AutoBuyKind::Item => "item",
            AutoBuyKind::Ball => "ball",
        };
        match AccountCommandDispatcher::send(
            self,
            ClientFrame::ShopBuy {
                kind: kind.into(),
                id: rule.item_id,
                qty: rule.quantity,
            },
        ) {
            Ok(()) => {
                tracing::debug!(account_id = %self.account.id, kind, item_id = rule.item_id, "auto buy frame sent; awaiting compra and inventory patch");
                self.auto_buy_in_flight = Some(AutoBuyInFlight {
                    kind: rule.kind.clone(),
                    item_id: rule.item_id,
                    sent_at_ms: now_ms(),
                    confirmed: false,
                });
                if let Some(current) =
                    self.state.auto_buy_rules.iter_mut().find(|current| {
                        current.kind == rule.kind && current.item_id == rule.item_id
                    })
                {
                    current.status = Some("Aguardando confirmação da compra...".into());
                }
            }
            Err(error) => {
                if let Some(current) =
                    self.state.auto_buy_rules.iter_mut().find(|current| {
                        current.kind == rule.kind && current.item_id == rule.item_id
                    })
                {
                    current.status = Some(format!("Compra aguardando transporte: {error}"));
                }
            }
        }
    }
    fn complete_auto_buy_after_inventory(&mut self, inventory_patch: bool, balls_patch: bool) {
        let Some(in_flight) = self.auto_buy_in_flight.as_ref() else {
            return;
        };
        let matching_patch = matches!(in_flight.kind, AutoBuyKind::Item) && inventory_patch
            || matches!(in_flight.kind, AutoBuyKind::Ball) && balls_patch;
        if !in_flight.confirmed || !matching_patch {
            return;
        }
        let completed = self.auto_buy_in_flight.take().expect("checked above");
        if let Some(rule) = self
            .state
            .auto_buy_rules
            .iter_mut()
            .find(|rule| rule.kind == completed.kind && rule.item_id == completed.item_id)
        {
            rule.status = Some("Estoque reconciliado.".into());
        }
    }
    fn enqueue_corpse(&mut self, death: &crate::protocol::DeathEvent) {
        if death.quem != "selvagem" {
            return;
        }
        let now = self.estimated_server_now();
        self.corpses.retain(|corpse| corpse.slot != death.slot);
        self.corpses.push_back(CaptureCorpse {
            slot: death.slot,
            name: death.name.clone().unwrap_or_else(|| "Selvagem".into()),
            species_id: death.species_id,
            died_at_server_time: now,
            expires_at_server_time: now.saturating_add(CAPTURE_WINDOW_MS),
            attempt_in_flight: false,
            retry_after_server_time: now,
        });
        self.corpses
            .make_contiguous()
            .sort_by_key(|corpse| corpse.died_at_server_time);
        self.state.capture_queue_len = self.corpses.len();
    }
    fn resolve_capture_result(&mut self, slot: u64, success: bool) {
        self.last_ball_throw_at_server_time = Some(self.estimated_server_now());
        self.corpses.retain(|corpse| corpse.slot != slot);
        self.state.capture_queue_len = self.corpses.len();
        if success && self.state.capture_mode == CaptureMode::UntilCapture {
            self.state.capture_mode = CaptureMode::Off;
            self.capture_mode_dirty = true;
        }
    }
    fn pump_capture_worker(&mut self, now: u64) {
        if self.state.capture_mode == CaptureMode::Off
            || matches!(
                self.account.owner,
                ConnectionOwner::Transition | ConnectionOwner::None
            )
            || self
                .last_ball_throw_at_server_time
                .is_some_and(|last| now < last.saturating_add(BALL_COOLDOWN_MS))
        {
            return;
        }
        let Some(index) = self
            .corpses
            .iter()
            .position(|corpse| !corpse.attempt_in_flight && now >= corpse.retry_after_server_time)
        else {
            return;
        };
        let Some(ball_id) = self
            .state
            .automation
            .ball_ids
            .iter()
            .copied()
            .find(|ball_id| {
                self.state
                    .balls
                    .get(&ball_id.to_string())
                    .copied()
                    .unwrap_or(0)
                    > 0
            })
        else {
            return;
        };
        let slot = self.corpses[index].slot;
        match AccountCommandDispatcher::send(self, ClientFrame::BallThrow { ball_id, slot }) {
            Ok(()) => {
                self.last_ball_throw_at_server_time = Some(now);
                self.corpses[index].attempt_in_flight = true;
                self.state.capture_error = None;
            }
            Err(error) => {
                self.corpses[index].retry_after_server_time = now.saturating_add(BALL_COOLDOWN_MS);
                self.state.capture_error = Some(error.to_string());
            }
        }
    }
    fn ingest(&mut self, frame: ServerFrame) {
        match &frame {
            ServerFrame::Welcome(welcome) => {
                self.apply_state(welcome.estado.clone(), true);
                let projected = welcome
                    .hunts
                    .iter()
                    .map(|hunt| Self::hunt_entry(hunt))
                    .collect();
                if projected != self.state.hunts {
                    self.data_revisions.hunt_options = self.next_data_revision();
                    self.state.hunts = projected;
                }
                if let Some(nick) = &welcome.estado.nick {
                    self.account.nick = nick.clone();
                }
                self.account.status = ConnectionStatus::Online;
            }
            ServerFrame::State(state) => self.apply_state(state.estado.clone(), false),
            ServerFrame::FieldInit(_) => {}
            ServerFrame::Battle(battle) => {
                for event in &battle.events {
                    if let crate::protocol::BattleEvent::Ball {
                        ball_id,
                        slot,
                        sucesso,
                        ..
                    } = event
                    {
                        // Server event identifies the ball actually consumed.
                        self.state.active_ball_id = Some(*ball_id);
                        // This applies equally to an observed manual launch and
                        // to the capture worker: the game has one cooldown.
                        self.resolve_capture_result(*slot, *sucesso);
                    }
                    if let crate::protocol::BattleEvent::Death(death) = event {
                        self.enqueue_corpse(death);
                    }
                    if matches!(event, crate::protocol::BattleEvent::Purchase { .. }) {
                        if let Some(in_flight) = self.auto_buy_in_flight.as_mut() {
                            in_flight.confirmed = true;
                            if in_flight.kind == AutoBuyKind::Item {
                                self.suppress_potion_delta_for_item = Some(in_flight.item_id);
                            }
                        }
                    }
                    if matches!(event, crate::protocol::BattleEvent::Sale { .. }) {
                        if let Some(pending) = self.pending_pokemon_sale.take() {
                            let pokemon_id = pending.pokemon_id;
                            let previous_len = self.state.pokemon.len();
                            self.state
                                .pokemon
                                .retain(|pokemon| pokemon.id != pokemon_id);
                            if self.state.pokemon.len() != previous_len {
                                self.data_revisions.depot = self.next_data_revision();
                            }
                            if self.state.active_id == Some(pokemon_id) {
                                self.state.active_id = None;
                            }
                        }
                    }
                    if let crate::protocol::BattleEvent::HuntSelected { slug, .. } = event {
                        let changed = self.state.hunt_slug.as_deref() != Some(slug);
                        if changed {
                            self.metrics = MetricsEngine::default();
                        }
                        self.reconcile_hunt_timer(slug, now_ms());
                        self.state.hunt_slug = Some(slug.clone());
                        self.state.pending_hunt_slug = None;
                        self.state.pending_navigation = None;
                        self.navigation_sent_at_ms = None;
                        self.state.no_centro = false;
                        self.state.activity = AccountActivity::Farming {
                            hunt_slug: slug.clone(),
                        };
                        if changed {
                            if let Some(session) = self.state.hunt_session.as_mut() {
                                session.hunt_slug = slug.clone();
                            } else {
                                self.state.hunt_session = Some(HuntSession {
                                    hunt_slug: slug.clone(),
                                    started_at_ms: now_ms(),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    if matches!(event, crate::protocol::BattleEvent::Center { .. }) {
                        self.state.no_centro = true;
                        self.state.activity = AccountActivity::PokemonCenter;
                        self.metrics = MetricsEngine::default();
                        self.clear_hunt_timer();
                    }
                    self.metrics.on_battle_event(
                        event,
                        !self.state.no_centro && self.account.status == ConnectionStatus::Online,
                    );
                    if let Some(session) = self.state.hunt_session.as_mut() {
                        match event {
                            crate::protocol::BattleEvent::Death(death)
                                if death.quem == "selvagem" =>
                            {
                                session.kills += 1;
                                session.trainer_xp =
                                    session.trainer_xp.saturating_add(death.trainer_xp);
                                session.pokemon_xp =
                                    session.pokemon_xp.saturating_add(death.pokemon_xp);
                                session.xp_obtained =
                                    session.xp_obtained.saturating_add(if death.pokemon_xp > 0 {
                                        death.pokemon_xp
                                    } else {
                                        death.trainer_xp
                                    });
                                session.gold_combat =
                                    session.gold_combat.saturating_add(death.ouro);
                                session.gold_auto_sale =
                                    session.gold_auto_sale.saturating_add(death.auto_sale_gold);
                                for drop in &death.drops {
                                    let quantity = if drop.gained > 0 {
                                        drop.gained
                                    } else {
                                        drop.quantity
                                    };
                                    *session.drops.entry(drop.nome.clone()).or_default() +=
                                        quantity;
                                }
                            }
                            crate::protocol::BattleEvent::Ball {
                                ball_id,
                                sucesso,
                                shiny,
                                ..
                            } => {
                                *session.balls_used.entry(ball_id.to_string()).or_default() += 1;
                                if *sucesso {
                                    session.captures += 1;
                                    if *shiny {
                                        session.shinies_captured += 1;
                                    }
                                }
                                if *shiny {
                                    session.shinies_seen += 1;
                                }
                            }
                            crate::protocol::BattleEvent::Fled { shiny: true, .. } => {
                                session.shinies_seen += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        for event in EventBus::from_frame(&frame) {
            self.bus.publish(event);
        }
        self.process_due_actions();
    }
    fn hunt_entry(hunt: &RemoteHunt) -> crate::domain::HuntCatalogEntry {
        crate::domain::HuntCatalogEntry {
            slug: hunt.slug.clone(),
            name: hunt.name.clone(),
            area: hunt.area.clone(),
            level: hunt.nivel,
            total_spawns: hunt.total_spawns,
            region: hunt.regiao.clone(),
            looktype: hunt.looktype,
            species: hunt
                .especies
                .iter()
                .map(|species| crate::domain::HuntSpeciesEntry {
                    species_id: species
                        .get("pokeId")
                        .or_else(|| species.get("speciesId"))
                        .or_else(|| species.get("id"))
                        .and_then(Value::as_u64),
                    weight: species
                        .get("pontos")
                        .or_else(|| species.get("weight"))
                        .or_else(|| species.get("peso"))
                        .and_then(Value::as_f64),
                })
                .collect(),
        }
    }
    fn snapshot(&mut self) -> AccountSnapshot {
        self.process_due_actions();
        self.state.command_transport_available = AccountCommandDispatcher::route(self).is_ok();
        let metrics = self
            .metrics
            .view(!self.state.no_centro && self.account.status == ConnectionStatus::Online);
        AccountSnapshot {
            account: self.account.clone(),
            state: self.state.clone(),
            metrics: crate::domain::AccountMetrics {
                xp_per_hour: metrics.xp_per_hour,
                gold_per_hour: metrics.gold_per_hour,
                kills: metrics.kills,
                captures: metrics.captures,
                potions_used: self.metrics.potions().clone(),
                potions_per_hour: self.metrics.potions_per_hour(),
                potion_usage_per_hour: self.metrics.potion_usage_per_hour(),
                balls_used: self.metrics.balls().clone(),
            },
        }
    }

    fn live_snapshot(&mut self) -> AccountLiveSnapshot {
        // Preserve the legacy snapshot side effects: deferred automation work,
        // transport availability and rolling metrics are all refreshed here.
        self.process_due_actions();
        self.state.command_transport_available = AccountCommandDispatcher::route(self).is_ok();
        let metric_view = self
            .metrics
            .view(!self.state.no_centro && self.account.status == ConnectionStatus::Online);
        let current_potion_id = self.state.active_potion_id.or_else(|| {
            self.state.automation.potion_ids.iter().copied().find(|id| {
                self.state
                    .items
                    .get(&id.to_string())
                    .copied()
                    .unwrap_or_default()
                    > 0
            })
        });
        let current_ball_id = self.state.active_ball_id.or_else(|| {
            self.state.automation.ball_ids.iter().copied().find(|id| {
                self.state
                    .balls
                    .get(&id.to_string())
                    .copied()
                    .unwrap_or_default()
                    > 0
            })
        });
        let active_pokemon = self
            .state
            .active_id
            .and_then(|active_id| {
                self.state
                    .pokemon
                    .iter()
                    .find(|pokemon| pokemon.id == active_id)
            })
            .cloned();
        let active_hunt = self.state.hunt_slug.as_ref().and_then(|slug| {
            self.state
                .hunts
                .iter()
                .find(|hunt| hunt.slug == *slug)
                .map(|hunt| AccountLiveHunt {
                    slug: hunt.slug.clone(),
                    name: hunt.name.clone(),
                    area: hunt.area.clone(),
                    region: hunt.region.clone(),
                })
        });
        let state = &self.state;
        AccountLiveSnapshot {
            account: self.account.clone(),
            state: AccountLiveState {
                level: state.level,
                xp: state.xp,
                gold: state.gold,
                diamonds: state.diamonds,
                orbs: state.orbs,
                vip_until: state.vip_until,
                vip_active: state.vip_active,
                server_now: state.server_now,
                vip_data_available: state.vip_data_available,
                xp_bonus: state.xp_bonus.clone(),
                guild_boost_until: state.guild_boost_until,
                center_free_at: state.center_free_at,
                combat_lock_until: state.combat_lock_until,
                server_offset_ms: state.server_offset_ms,
                combat_locked: state.combat_locked,
                command_transport_available: state.command_transport_available,
                hunt_slug: state.hunt_slug.clone(),
                hunt_started_at_ms: state.hunt_started_at_ms,
                pending_hunt_slug: state.pending_hunt_slug.clone(),
                pending_navigation: state.pending_navigation.clone(),
                navigation_error: state.navigation_error.clone(),
                no_centro: state.no_centro,
                active_id: state.active_id,
                active_ball_id: state.active_ball_id,
                active_potion_id: state.active_potion_id,
                active_potion_source: state.active_potion_source.clone(),
                active_pokemon,
                active_hunt,
                items_total_quantity: state
                    .items
                    .values()
                    .fold(0_u64, |total, quantity| total.saturating_add(*quantity)),
                wild: state.wild.clone(),
                automation: state.automation.clone(),
                automation_error: state.automation_error.clone(),
                auto_buy_rules: state.auto_buy_rules.clone(),
                capture_mode: state.capture_mode.clone(),
                capture_queue_len: state.capture_queue_len,
                capture_error: state.capture_error.clone(),
                hunt_session: state.hunt_session.clone(),
                activity: state.activity.clone(),
                disconnect_reason: state.disconnect_reason.clone(),
                reconnect_attempt: state.reconnect_attempt,
                reconnect_started_at_ms: state.reconnect_started_at_ms,
                reconnect_duration_ms: state.reconnect_duration_ms,
                last_reconnected_at_ms: state.last_reconnected_at_ms,
            },
            metrics: AccountLiveMetrics {
                xp_per_hour: metric_view.xp_per_hour,
                gold_per_hour: metric_view.gold_per_hour,
                kills: metric_view.kills,
                captures: metric_view.captures,
                potions_used: self.metrics.potions().clone(),
                potions_per_hour: self.metrics.potions_per_hour(),
                potion_usage_per_hour: self.metrics.potion_usage_per_hour(),
                balls_used: self.metrics.balls().clone(),
            },
            current_potion_id,
            current_potion_quantity: current_potion_id
                .and_then(|id| state.items.get(&id.to_string()).copied())
                .unwrap_or_default(),
            current_ball_id,
            current_ball_quantity: current_ball_id
                .and_then(|id| state.balls.get(&id.to_string()).copied())
                .unwrap_or_default(),
            revisions: self.data_revisions,
        }
    }
}
#[derive(Clone)]
pub struct AccountManager {
    runtimes: Arc<Mutex<HashMap<String, AccountRuntime>>>,
    data_revision_clock: Arc<AtomicU64>,
    #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
    mobile_snapshot_revision: Arc<AtomicU64>,
    #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
    mobile_changes: watch::Sender<u64>,
    lifecycle_locks: Arc<Mutex<HashMap<String, AccountLifecycleControl>>>,
    inspector_enabled: Arc<AtomicBool>,
    shutting_down: Arc<AtomicBool>,
    market_signals: broadcast::Sender<MarketSignal>,
    hunt_timer_notifications: watch::Sender<u64>,
    hunt_session_notifications: watch::Sender<u64>,
    #[cfg(debug_assertions)]
    validation_read_only: Arc<AtomicBool>,
    #[cfg(debug_assertions)]
    validation_command_counters: Arc<ValidationCommandCounters>,
}

struct MobileChangeOnDrop(Option<watch::Sender<u64>>);

impl Drop for MobileChangeOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = &self.0 {
            sender.send_modify(|revision| *revision = revision.wrapping_add(1));
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuntTimerPersistenceChange {
    pub account_id: String,
    pub timer: HuntTimerState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuntSessionPersistenceChange {
    pub account_id: String,
    pub revision: u64,
    pub session: Option<HuntSession>,
}
#[derive(Clone)]
struct AccountLifecycleControl {
    lock: Arc<tokio::sync::Mutex<()>>,
    epoch: u64,
    cancellation: CancellationToken,
}
impl Default for AccountManager {
    fn default() -> Self {
        let (market_signals, _) = broadcast::channel(128);
        let (hunt_timer_notifications, _) = watch::channel(0);
        let (hunt_session_notifications, _) = watch::channel(0);
        #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
        let (mobile_changes, _) = watch::channel(0);
        Self {
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            data_revision_clock: Arc::new(AtomicU64::new(0)),
            #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
            mobile_snapshot_revision: Arc::new(AtomicU64::new(0)),
            #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
            mobile_changes,
            lifecycle_locks: Arc::new(Mutex::new(HashMap::new())),
            inspector_enabled: Arc::new(AtomicBool::new(false)),
            shutting_down: Arc::new(AtomicBool::new(false)),
            market_signals,
            hunt_timer_notifications,
            hunt_session_notifications,
            #[cfg(debug_assertions)]
            validation_read_only: Arc::new(AtomicBool::new(false)),
            #[cfg(debug_assertions)]
            validation_command_counters: Arc::new(ValidationCommandCounters::default()),
        }
    }
}
impl AccountManager {
    /// Shared debug-only fail-closed command guard for isolated validation
    /// builds. Existing and subsequently-added runtimes observe the same
    /// atomic value without coordinating through account-map locks.
    #[cfg(debug_assertions)]
    pub fn set_validation_read_only(&self, read_only: bool) {
        self.validation_read_only
            .store(read_only, Ordering::Release);
    }

    /// Gracefully stops one account's Background connection for an isolated
    /// validation run. It leaves the manager-wide lifecycle available so the
    /// runner can start this account again. Browser observers/processes are
    /// only signaled; their task/process handles belong to the orchestrator.
    #[cfg(debug_assertions)]
    pub async fn shutdown_account_and_wait(&self, account_id: &str) -> Result<(), AccountError> {
        if !self.validation_read_only.load(Ordering::Acquire) {
            return Err(AccountError::ValidationModeRequired);
        }

        self.cancel_lifecycle(account_id);
        let (connection, browser_control) = {
            let _mobile_change = self.mobile_change_guard();
            let mut runtimes = self.runtimes.lock();
            let runtime = runtimes
                .get_mut(account_id)
                .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
            runtime.account.runtime = AccountRuntimeState::Stopping;
            runtime.account.owner = ConnectionOwner::None;
            let connection = runtime.background.take();
            let browser_control = runtime.browser_control.take();
            runtime.browser_transport_ready = false;
            runtime.account.status = ConnectionStatus::Offline;
            runtime.account.runtime = AccountRuntimeState::Offline;
            (connection, browser_control)
        };

        if let Some(control) = browser_control {
            let _ = control.send(BrowserControl::CloseManagedBrowser);
        }
        if let Some(connection) = connection {
            connection.shutdown().await;
        }
        Ok(())
    }

    /// Stops every account for the final validation shutdown. Unlike
    /// `shutdown_account_and_wait`, this latches the manager against future
    /// lifecycle starts and is therefore not suitable between per-account runs.
    #[cfg(debug_assertions)]
    pub async fn shutdown_all_and_wait(&self) -> Result<(), AccountError> {
        if !self.validation_read_only.load(Ordering::Acquire) {
            return Err(AccountError::ValidationModeRequired);
        }

        let (connections, browser_controls) = {
            let _mobile_change = self.mobile_change_guard();
            self.shutting_down.store(true, Ordering::Release);
            for control in self.lifecycle_locks.lock().values_mut() {
                control.epoch = control.epoch.wrapping_add(1);
                control.cancellation.cancel();
            }

            let mut connections = Vec::new();
            let mut browser_controls = Vec::new();
            let mut runtimes = self.runtimes.lock();
            for runtime in runtimes.values_mut() {
                runtime.account.runtime = AccountRuntimeState::Stopping;
                runtime.account.owner = ConnectionOwner::None;
                if let Some(connection) = runtime.background.take() {
                    connections.push(connection);
                }
                if let Some(control) = runtime.browser_control.take() {
                    browser_controls.push(control);
                }
                runtime.browser_transport_ready = false;
                runtime.account.status = ConnectionStatus::Offline;
                runtime.account.runtime = AccountRuntimeState::Offline;
            }
            (connections, browser_controls)
        };

        for control in browser_controls {
            let _ = control.send(BrowserControl::CloseManagedBrowser);
        }
        for connection in connections {
            connection.shutdown().await;
        }
        Ok(())
    }

    /// Requests a reload of an account's managed Browser page. No game frame is
    /// constructed or dispatched by this API.
    #[cfg(debug_assertions)]
    pub fn request_browser_reload(&self, account_id: &str) -> Result<(), AccountError> {
        if !self.validation_read_only.load(Ordering::Acquire) {
            return Err(AccountError::ValidationModeRequired);
        }
        let runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Browser {
            return Err(AccountError::NotFound(
                "A conta não está sob ownership do Browser".into(),
            ));
        }
        runtime
            .browser_control
            .as_ref()
            .ok_or_else(|| AccountError::NotFound("Controle do Browser indisponível".into()))?
            .send(BrowserControl::ReloadPage)
            .map_err(|_| AccountError::NotFound("Sessão do Browser foi encerrada".into()))
    }

    fn mark_mobile_changed(&self) {
        #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
        self.mobile_changes
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        #[cfg(not(any(test, all(debug_assertions, feature = "mobile-local-server"))))]
        let _ = self;
    }

    fn mobile_change_guard(&self) -> MobileChangeOnDrop {
        #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
        {
            MobileChangeOnDrop(Some(self.mobile_changes.clone()))
        }
        #[cfg(not(any(test, all(debug_assertions, feature = "mobile-local-server"))))]
        {
            let _ = self;
            MobileChangeOnDrop(None)
        }
    }

    #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
    pub fn subscribe_mobile_changes(&self) -> watch::Receiver<u64> {
        self.mobile_changes.subscribe()
    }

    /// One async serialization point per account. The map lock is released
    /// before the caller awaits the returned lock, so no synchronous mutex is
    /// held across I/O or a browser lifecycle.
    pub fn lifecycle_lock(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.lifecycle_locks
            .lock()
            .entry(account_id.to_owned())
            .or_insert_with(|| AccountLifecycleControl {
                lock: Arc::new(tokio::sync::Mutex::new(())),
                epoch: 0,
                cancellation: CancellationToken::new(),
            })
            .lock
            .clone()
    }
    pub fn start_lifecycle(&self, account_id: &str) -> Option<(u64, CancellationToken)> {
        if self.shutting_down.load(Ordering::Acquire)
            || !self.runtimes.lock().contains_key(account_id)
        {
            return None;
        }
        let mut controls = self.lifecycle_locks.lock();
        let control =
            controls
                .entry(account_id.to_owned())
                .or_insert_with(|| AccountLifecycleControl {
                    lock: Arc::new(tokio::sync::Mutex::new(())),
                    epoch: 0,
                    cancellation: CancellationToken::new(),
                });
        // Starting a replacement must wake the previous task before the new
        // task waits for its per-account lifecycle lock.
        control.cancellation.cancel();
        control.epoch = control.epoch.wrapping_add(1);
        control.cancellation = CancellationToken::new();
        Some((control.epoch, control.cancellation.clone()))
    }
    pub fn lifecycle_is_current(&self, account_id: &str, epoch: u64) -> bool {
        let current = self
            .lifecycle_locks
            .lock()
            .get(account_id)
            .is_some_and(|control| control.epoch == epoch && !control.cancellation.is_cancelled());
        current && self.runtimes.lock().contains_key(account_id)
    }
    fn cancel_lifecycle(&self, account_id: &str) {
        if let Some(control) = self.lifecycle_locks.lock().get_mut(account_id) {
            control.epoch = control.epoch.wrapping_add(1);
            control.cancellation.cancel();
        }
    }
    pub fn set_inspector_enabled(&self, enabled: bool) {
        self.inspector_enabled.store(enabled, Ordering::Release);
        if !enabled {
            for runtime in self.runtimes.lock().values_mut() {
                runtime.inspector_frames.clear();
            }
        }
    }
    /// A single atomic branch keeps Inspector-off WebSocket reception allocation-free.
    pub fn record_protocol_frame(&self, account_id: &str, direction: ProtocolDirection, raw: &str) {
        if !self.inspector_enabled.load(Ordering::Acquire) {
            return;
        }
        let frame = inspector::sanitized_frame(direction, raw);
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        if runtime.inspector_frames.len() == inspector::FRAME_CAPACITY {
            runtime.inspector_frames.pop_front();
        }
        runtime.inspector_frames.push_back(frame);
    }
    pub fn protocol_frames(&self, account_id: &str) -> Result<Vec<ProtocolFrame>, AccountError> {
        if !self.inspector_enabled.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        self.runtimes
            .lock()
            .get(account_id)
            .map(|runtime| runtime.inspector_frames.iter().cloned().collect())
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))
    }
    pub fn clear_protocol_frames(&self, account_id: &str) -> Result<(), AccountError> {
        self.runtimes
            .lock()
            .get_mut(account_id)
            .map(|runtime| runtime.inspector_frames.clear())
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))
    }
    pub fn set_auto_sale(&self, account_id: &str, enabled: bool) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        tracing::debug!(
            account_id,
            enabled,
            "dedicated auto sale frame send requested"
        );
        match AccountCommandDispatcher::send(runtime, ClientFrame::AutoSaleLoot { ativo: enabled })
        {
            Ok(()) => Ok(()),
            Err(error) => {
                runtime.state.automation_error = Some(error.to_string());
                Err(error)
            }
        }
    }
    pub fn set_automation_value(
        &self,
        account_id: &str,
        field: &str,
        value: Value,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.queue_automation_change(field, value)
    }
    pub fn set_automation_ids(
        &self,
        account_id: &str,
        kind: AutoBuyKind,
        ids: Vec<u64>,
    ) -> Result<(), AccountError> {
        let field = match kind {
            AutoBuyKind::Item => "potionIds",
            AutoBuyKind::Ball => "ballIds",
        };
        let requested_ids = ids.clone();
        let value = Value::Array(ids.into_iter().map(Value::from).collect());
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.queue_automation_change(field, value)?;
        // The server remains authoritative for the actual selected ids. This
        // prepares the local refill rule for the requested replacement while
        // `pump_auto_buy` still refuses it until the matching server state
        // arrives.
        let prior_ids = match kind {
            AutoBuyKind::Item => std::mem::replace(
                &mut runtime.state.automation.potion_ids,
                requested_ids.clone(),
            ),
            AutoBuyKind::Ball => std::mem::replace(
                &mut runtime.state.automation.ball_ids,
                requested_ids.clone(),
            ),
        };
        runtime.reconcile_auto_buy_selection(kind.clone());
        match kind {
            AutoBuyKind::Item => runtime.state.automation.potion_ids = prior_ids,
            AutoBuyKind::Ball => runtime.state.automation.ball_ids = prior_ids,
        }
        runtime.process_due_actions();
        drop(runtimes);
        self.mark_mobile_changed();
        Ok(())
    }
    /// Sets one last-write-wins navigation intent. A combat lock postpones the
    /// send; the server remains authoritative for the visible destination.
    pub fn select_hunt(&self, account_id: &str, slug: String) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if !runtime.state.hunts.iter().any(|hunt| hunt.slug == slug) {
            return Err(AccountError::NotFound(format!(
                "Hunt indisponível para {account_id}: {slug}"
            )));
        }
        runtime.refresh_combat_lock();
        runtime.state.pending_navigation = Some(PendingNavigationIntent::Hunt { slug });
        runtime.state.navigation_error = None;
        runtime.navigation_sent_at_ms = None;
        runtime.pump_pending_navigation();
        Ok(())
    }
    pub fn go_center(&self, account_id: &str) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.refresh_combat_lock();
        runtime.state.pending_navigation = Some(PendingNavigationIntent::Center);
        runtime.state.navigation_error = None;
        runtime.navigation_sent_at_ms = None;
        runtime.pump_pending_navigation();
        Ok(())
    }
    pub fn cancel_navigation(&self, account_id: &str) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.pending_navigation = None;
        runtime.state.pending_hunt_slug = None;
        runtime.navigation_sent_at_ms = None;
        runtime.state.navigation_error = None;
        Ok(())
    }
    /// Resets the local session summary without sending a game command. The
    /// currently selected hunt timer is intentionally left unchanged.
    pub fn reset_hunt_session(&self, account_id: &str) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.hunt_session = runtime
            .state
            .hunt_slug
            .clone()
            .map(|hunt_slug| HuntSession {
                hunt_slug,
                started_at_ms: now_ms(),
                ..Default::default()
            });
        runtime.hunt_session_revision = runtime.hunt_session_revision.saturating_add(1);
        runtime.hunt_session_dirty = true;
        drop(runtimes);
        self.hunt_session_notifications
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        self.mark_mobile_changed();
        Ok(())
    }
    pub fn sell_pokemon(&self, account_id: &str, pokemon_id: u64) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        AccountCommandDispatcher::send(runtime, ClientFrame::SellPokemon { pokemon_id })?;
        runtime.pending_pokemon_sale = Some(PendingPokemonSale {
            pokemon_id,
            sent_at_ms: now_ms(),
        });
        Ok(())
    }
    pub fn sell_all_pokemon(&self, account_id: &str) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        AccountCommandDispatcher::send(runtime, ClientFrame::SellAllPokemons)?;
        runtime.inventory_refresh_pending = true;
        Ok(())
    }
    pub fn set_capture_mode(
        &self,
        account_id: &str,
        mode: CaptureMode,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.capture_mode = mode;
        runtime.state.capture_error = None;
        runtime.process_due_actions();
        Ok(())
    }
    pub fn set_auto_buy_enabled(
        &self,
        account_id: &str,
        kind: AutoBuyKind,
        enabled: bool,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;

        if enabled {
            let configured_ids = match kind {
                AutoBuyKind::Item => runtime.state.automation.potion_ids.clone(),
                AutoBuyKind::Ball => runtime.state.automation.ball_ids.clone(),
            };
            if configured_ids.is_empty() {
                return Err(AccountError::NotFound(
                    "Selecione ao menos um item antes de ativar a compra automática".into(),
                ));
            }
            for item_id in configured_ids {
                if let Some(rule) = runtime
                    .state
                    .auto_buy_rules
                    .iter_mut()
                    .find(|rule| rule.kind == kind && rule.item_id == item_id)
                {
                    rule.enabled = true;
                    rule.status = None;
                } else {
                    runtime.state.auto_buy_rules.push(AutoBuyRule {
                        kind: kind.clone(),
                        item_id,
                        minimum: 500,
                        quantity: 1000,
                        enabled: true,
                        status: None,
                    });
                }
            }
        } else {
            // A category toggle is global to the account. Disabling must also
            // cover stale rules from items no longer selected in automation.
            for rule in runtime
                .state
                .auto_buy_rules
                .iter_mut()
                .filter(|rule| rule.kind == kind)
            {
                rule.enabled = false;
                rule.status = None;
            }
        }
        runtime.process_due_actions();
        Ok(())
    }
    pub fn update_auto_buy_rule(
        &self,
        account_id: &str,
        kind: AutoBuyKind,
        item_id: u64,
        minimum: u64,
        quantity: u64,
    ) -> Result<(), AccountError> {
        if !(1..=9_999).contains(&quantity) {
            return Err(AccountError::NotFound(
                "Quantidade por compra deve estar entre 1 e 9999".into(),
            ));
        }
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if let Some(rule) = runtime
            .state
            .auto_buy_rules
            .iter_mut()
            .find(|rule| rule.kind == kind && rule.item_id == item_id)
        {
            rule.minimum = minimum;
            rule.quantity = quantity;
            rule.status = None;
        } else {
            runtime.state.auto_buy_rules.push(AutoBuyRule {
                kind,
                item_id,
                minimum,
                quantity,
                enabled: false,
                status: None,
            });
        }
        runtime.process_due_actions();
        Ok(())
    }
    pub fn auto_buy_rules(&self, account_id: &str) -> Result<Vec<AutoBuyRule>, AccountError> {
        self.runtimes
            .lock()
            .get(account_id)
            .map(|runtime| runtime.state.auto_buy_rules.clone())
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))
    }
    /// Restores only local Manager preferences. It does not send a game frame;
    /// the next authoritative estado/welcome will reconcile live inventory.
    pub fn restore_auto_buy_rules(
        &self,
        account_id: &str,
        rules: Vec<AutoBuyRule>,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.auto_buy_rules = rules;
        Ok(())
    }
    pub fn restore_automation_preferences(
        &self,
        account_id: &str,
        potion_ids: Vec<u64>,
        ball_ids: Vec<u64>,
        capture_mode: CaptureMode,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.automation.potion_ids = potion_ids;
        // Order is significant: it is the selected Ball priority.
        runtime.state.automation.ball_ids = ball_ids;
        runtime.state.capture_mode = capture_mode;
        Ok(())
    }
    /// Capture-until can turn itself off when the game confirms a successful
    /// capture. The application persists that transition on its next regular
    /// snapshot, so restarting never brings a completed one-shot launch back.
    pub fn pending_capture_mode_changes(&self) -> Vec<(String, CaptureMode)> {
        self.runtimes
            .lock()
            .values()
            .filter(|runtime| runtime.capture_mode_dirty)
            .map(|runtime| {
                (
                    runtime.account.id.clone(),
                    runtime.state.capture_mode.clone(),
                )
            })
            .collect()
    }
    pub fn acknowledge_capture_mode_changes(&self, account_ids: &[String]) {
        let mut runtimes = self.runtimes.lock();
        for account_id in account_ids {
            if let Some(runtime) = runtimes.get_mut(account_id) {
                runtime.capture_mode_dirty = false;
            }
        }
    }
    pub fn websocket_disconnected(
        &self,
        account_id: &str,
        reason: String,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.disconnect_reason = Some(reason);
        runtime.state.reconnect_started_at_ms = Some(now_ms());
        runtime.state.reconnect_attempt = 0;
        runtime.clear_uncertain_game_state();
        Ok(())
    }
    pub fn clear_uncertain_game_state(&self, account_id: &str) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Transition {
            runtime.clear_uncertain_game_state();
        }
        Ok(())
    }
    pub fn reconnect_attempt(&self, account_id: &str, attempt: u32) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.reconnect_attempt = attempt;
        Ok(())
    }
    pub fn websocket_reconnected(&self, account_id: &str) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        let now = now_ms();
        runtime.state.reconnect_duration_ms = runtime
            .state
            .reconnect_started_at_ms
            .map(|started| now.saturating_sub(started));
        runtime.state.last_reconnected_at_ms = Some(now);
        runtime.state.reconnect_attempt = 0;
        Ok(())
    }
    pub fn add(&self, record: AccountRecord) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        if runtimes.len() >= MAX_ACCOUNTS {
            return Err(AccountError::LimitReached);
        }
        if runtimes
            .values()
            .any(|runtime| runtime.account.nick.eq_ignore_ascii_case(&record.nick))
        {
            return Err(AccountError::Duplicate(record.nick));
        }
        #[cfg(debug_assertions)]
        let mut runtime =
            AccountRuntime::with_validation_guard(record, Arc::clone(&self.validation_read_only));
        #[cfg(debug_assertions)]
        {
            runtime.validation_command_counters = Arc::clone(&self.validation_command_counters);
        }
        #[cfg(not(debug_assertions))]
        let mut runtime = AccountRuntime::new(record);
        runtime.revision_clock = Arc::clone(&self.data_revision_clock);
        runtime.initialize_data_revisions();
        runtimes.insert(runtime.account.id.clone(), runtime);
        drop(runtimes);
        self.mark_mobile_changed();
        Ok(())
    }
    pub fn restore_hunt_timer(
        &self,
        account_id: &str,
        timer: HuntTimerState,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.hunt_timer_revision = timer.revision;
        if timer.hunt_slug.is_empty() {
            runtime.state.hunt_started_at_ms = None;
            runtime.hunt_timer = None;
        } else {
            runtime.state.hunt_started_at_ms = Some(timer.started_at_ms);
            runtime.hunt_timer = Some(timer);
        }
        runtime.hunt_timer_dirty = false;
        Ok(())
    }
    pub fn restore_hunt_session(
        &self,
        account_id: &str,
        session: Option<HuntSession>,
        revision: u64,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.state.hunt_session = session;
        runtime.hunt_session_revision = revision;
        runtime.hunt_session_dirty = false;
        Ok(())
    }
    pub fn pending_hunt_timer_changes(&self) -> Vec<HuntTimerPersistenceChange> {
        self.runtimes
            .lock()
            .values()
            .filter(|runtime| runtime.hunt_timer_dirty)
            .map(|runtime| HuntTimerPersistenceChange {
                account_id: runtime.account.id.clone(),
                timer: runtime.hunt_timer.clone().unwrap_or(HuntTimerState {
                    hunt_slug: String::new(),
                    started_at_ms: 0,
                    revision: runtime.hunt_timer_revision,
                }),
            })
            .collect()
    }
    pub fn acknowledge_hunt_timer_changes(&self, changes: &[HuntTimerPersistenceChange]) {
        let mut runtimes = self.runtimes.lock();
        for change in changes {
            if let Some(runtime) = runtimes.get_mut(&change.account_id)
                && runtime.hunt_timer_revision == change.timer.revision
                && runtime
                    .hunt_timer
                    .as_ref()
                    .is_none_or(|timer| timer == &change.timer)
            {
                runtime.hunt_timer_dirty = false;
            }
        }
    }
    pub fn subscribe_hunt_timer_changes(&self) -> watch::Receiver<u64> {
        self.hunt_timer_notifications.subscribe()
    }
    pub fn pending_hunt_session_changes(&self) -> Vec<HuntSessionPersistenceChange> {
        self.runtimes
            .lock()
            .values()
            .filter(|runtime| runtime.hunt_session_dirty)
            .map(|runtime| HuntSessionPersistenceChange {
                account_id: runtime.account.id.clone(),
                revision: runtime.hunt_session_revision,
                session: runtime.state.hunt_session.clone(),
            })
            .collect()
    }
    pub fn acknowledge_hunt_session_changes(&self, changes: &[HuntSessionPersistenceChange]) {
        let mut runtimes = self.runtimes.lock();
        for change in changes {
            if let Some(runtime) = runtimes.get_mut(&change.account_id)
                && runtime.hunt_session_revision == change.revision
                && runtime.state.hunt_session == change.session
            {
                runtime.hunt_session_dirty = false;
            }
        }
    }
    pub fn subscribe_hunt_session_changes(&self) -> watch::Receiver<u64> {
        self.hunt_session_notifications.subscribe()
    }
    /// Removes only Manager state. The profile directory is intentionally kept
    /// so a user can opt to reuse it later; profile deletion needs a separate,
    /// explicit confirmation and is never implied by removing an account.
    pub fn remove(&self, account_id: &str) -> Result<(), AccountError> {
        self.cancel_lifecycle(account_id);
        let mut runtimes = self.runtimes.lock();
        let mut runtime = runtimes
            .remove(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if let Some(connection) = runtime.background.take() {
            connection.cancel();
        }
        if let Some(control) = runtime.browser_control.take() {
            let _ = control.send(BrowserControl::CloseManagedBrowser);
        }
        drop(runtime);
        drop(runtimes);
        self.mark_mobile_changed();
        Ok(())
    }
    /// The local UUID stays the runtime key. Nick is only display/duplicate
    /// identity metadata, verified before a welcome frame can mutate runtime
    /// state or SQLite.
    pub fn apply_authoritative_nick(
        &self,
        account_id: &str,
        nick: &str,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        if runtimes.values().any(|runtime| {
            runtime.account.id != account_id && runtime.account.nick.eq_ignore_ascii_case(nick)
        }) {
            return Err(AccountError::Duplicate(nick.to_owned()));
        }
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.account.nick = nick.to_owned();
        drop(runtimes);
        self.mark_mobile_changed();
        Ok(())
    }
    pub fn ingest(&self, account_id: &str, frame: ServerFrame) -> Result<(), AccountError> {
        let market_signal = MarketSignal::from_server_frame(account_id, &frame);
        let (hunt_timer_changed, hunt_session_changed) = {
            let mut runtimes = self.runtimes.lock();
            let runtime = runtimes
                .get_mut(account_id)
                .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
            let previous_timer = runtime.hunt_timer.clone();
            let previous_session = runtime.state.hunt_session.clone();
            runtime.ingest(frame);
            let hunt_session_changed = runtime.state.hunt_session != previous_session;
            if hunt_session_changed {
                runtime.hunt_session_revision = runtime.hunt_session_revision.saturating_add(1);
                runtime.hunt_session_dirty = true;
            }
            (runtime.hunt_timer != previous_timer, hunt_session_changed)
        };
        self.mark_mobile_changed();
        if hunt_timer_changed {
            self.hunt_timer_notifications
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        if hunt_session_changed {
            self.hunt_session_notifications
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        if let Some(signal) = market_signal {
            if let Err(error) = self.market_signals.send(signal) {
                tracing::debug!(%error, "market signal had no active runtime receiver");
            }
        }
        Ok(())
    }
    /// MarketRuntime is a command producer like the existing automations: it
    /// never sees transports, and this keeps Browser/Background ownership
    /// authoritative in AccountCommandDispatcher.
    pub fn send_market_command(
        &self,
        account_id: &str,
        command: ClientFrame,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        AccountCommandDispatcher::send(runtime, command)
    }
    pub fn subscribe_market_signals(&self) -> broadcast::Receiver<MarketSignal> {
        self.market_signals.subscribe()
    }
    pub fn begin_browser_login(&self, account_id: &str) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.account.status = ConnectionStatus::LoginRequired;
        runtime.account.mode = AccountMode::Browser;
        runtime.account.runtime = AccountRuntimeState::WaitingForLogin;
        runtime.account.owner = ConnectionOwner::None;
        Ok(())
    }
    /// Starts an explicit CDP reattachment only from an errored Browser
    /// session with no active transport. The caller serializes the observer
    /// through `lifecycle_lock`; this guard prevents retry from stealing a
    /// healthy Browser or Background owner.
    pub fn begin_browser_reconnect(&self, account_id: &str) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.mode != AccountMode::Browser
            || runtime.account.status != ConnectionStatus::Error
            || runtime.account.owner != ConnectionOwner::None
            || runtime.background.is_some()
        {
            return Err(AccountError::NotFound(
                "Reconexão disponível somente para uma conta Browser em erro, sem outro owner ativo."
                    .into(),
            ));
        }
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        runtime.account.owner = ConnectionOwner::Transition;
        runtime.account.mode = AccountMode::Transitioning;
        runtime.account.runtime = AccountRuntimeState::Reconnecting;
        runtime.account.status = ConnectionStatus::Connecting;
        Ok(())
    }
    pub fn take_background_for_transition(
        &self,
        account_id: &str,
    ) -> Result<Option<BackgroundConnection>, AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Transition {
            return Ok(None);
        }
        Ok(runtime.background.take())
    }
    /// CDP/session control loss is not proof that the game's WebSocket closed.
    /// Mark the account unavailable for commands but do not start a second
    /// transport; a later explicit login can safely establish ownership.
    pub fn browser_control_unavailable(&self, account_id: &str, reason: String) {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        runtime.account.owner = ConnectionOwner::None;
        runtime.account.mode = AccountMode::Browser;
        runtime.account.runtime = AccountRuntimeState::Error;
        runtime.account.status = ConnectionStatus::Error;
        runtime.state.disconnect_reason = Some(reason);
    }
    pub fn browser_handoff_failed_after_close(&self, account_id: &str, reason: String) {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        runtime.account.owner = ConnectionOwner::Transition;
        runtime.account.mode = AccountMode::Transitioning;
        runtime.account.runtime = AccountRuntimeState::Reconnecting;
        runtime.account.status = ConnectionStatus::Connecting;
        runtime.state.disconnect_reason = Some(reason);
    }
    /// Stop the Rust owner before launching a Browser owner. Returning the
    /// transport lets the async caller cancel and join it without holding the
    /// account-state mutex across an await.
    pub fn begin_browser_transfer(
        &self,
        account_id: &str,
    ) -> Result<BackgroundConnection, AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Background || runtime.background.is_none() {
            return Err(AccountError::NotFound(format!(
                "{account_id} não está ativo em Background"
            )));
        }
        runtime.account.owner = ConnectionOwner::Transition;
        runtime.account.mode = AccountMode::Transitioning;
        runtime.account.runtime = AccountRuntimeState::PreparingHandoff;
        runtime.account.status = ConnectionStatus::Connecting;
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        runtime.background.take().ok_or_else(|| {
            AccountError::NotFound(format!("{account_id} não tem transporte Background"))
        })
    }
    pub fn attach_browser_control(
        &self,
        account_id: &str,
        control: mpsc::UnboundedSender<BrowserControl>,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.browser_control = Some(control);
        runtime.browser_transport_ready = false;
        Ok(())
    }
    pub fn set_browser_transport_ready(
        &self,
        account_id: &str,
        ready: bool,
    ) -> Result<(), AccountError> {
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.browser_transport_ready = ready;
        Ok(())
    }
    /// Called only after the managed page observed `welcome`.
    pub fn complete_browser_owner(&self, account_id: &str) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.background.is_some() {
            return Err(AccountError::NotFound(
                "Transporte Background ainda ativo; Browser não pode assumir o owner".into(),
            ));
        }
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.account.mode = AccountMode::Browser;
        runtime.account.runtime = AccountRuntimeState::BrowserConnected;
        runtime.account.status = ConnectionStatus::Online;
        runtime.state.disconnect_reason = None;
        runtime.state.reconnect_started_at_ms = None;
        runtime.state.reconnect_attempt = 0;
        Ok(())
    }
    /// A failed browser bootstrap must leave the proven Rust connection untouched.
    pub fn restore_background_owner(&self, account_id: &str) {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        if runtime.account.owner == ConnectionOwner::Transition && runtime.background.is_some() {
            runtime.account.owner = ConnectionOwner::Background;
            runtime.account.mode = AccountMode::Background;
            runtime.account.runtime = AccountRuntimeState::Background;
            runtime.account.status = ConnectionStatus::Online;
        }
    }
    pub fn browser_transfer_failed(&self, account_id: &str, reason: String) {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        if runtime.account.owner == ConnectionOwner::Transition && runtime.background.is_none() {
            runtime.browser_control = None;
            runtime.browser_transport_ready = false;
            runtime.account.owner = ConnectionOwner::None;
            runtime.account.mode = AccountMode::Browser;
            runtime.account.runtime = AccountRuntimeState::Error;
            runtime.account.status = ConnectionStatus::Error;
            runtime.state.disconnect_reason = Some(reason);
        }
    }
    pub fn request_background_handoff(&self, account_id: &str) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        // A Browser owner-loss signal may already have started recovery by the
        // time a stale UI button is clicked. That is a recoverable transition,
        // never an actionable "session ended" error for the user.
        if runtime.account.owner == ConnectionOwner::Transition
            && runtime.account.runtime == AccountRuntimeState::Reconnecting
        {
            return Ok(());
        }
        if runtime.account.owner != ConnectionOwner::Browser {
            return Err(AccountError::NotFound(format!(
                "{account_id} não está ativo no navegador"
            )));
        }
        let Some(control) = runtime.browser_control.as_ref() else {
            return Err(AccountError::NotFound(
                "Sessão do navegador indisponível".into(),
            ));
        };
        runtime.account.owner = ConnectionOwner::Transition;
        runtime.account.mode = AccountMode::Transitioning;
        runtime.account.runtime = AccountRuntimeState::PreparingHandoff;
        runtime.account.status = ConnectionStatus::Connecting;
        if control.send(BrowserControl::HandoffToBackground).is_err() {
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.account.mode = AccountMode::Browser;
            runtime.account.runtime = AccountRuntimeState::BrowserConnected;
            runtime.account.status = ConnectionStatus::Online;
            return Err(AccountError::NotFound(
                "Sessão do navegador foi encerrada".into(),
            ));
        }
        Ok(())
    }
    /// Atomically leaves Browser ownership when CDP, the managed target, the
    /// game WebSocket, or the controlled Brave process disappears. The boolean
    /// makes the signal idempotent: several close notifications can race, but
    /// exactly the first Browser -> Transition change starts recovery.
    pub fn browser_owner_lost(
        &self,
        account_id: &str,
        reason: String,
    ) -> Result<bool, AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Browser {
            return Ok(false);
        }
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        runtime.account.owner = ConnectionOwner::Transition;
        runtime.account.mode = AccountMode::Transitioning;
        runtime.account.runtime = AccountRuntimeState::Reconnecting;
        runtime.account.status = ConnectionStatus::Connecting;
        runtime.state.disconnect_reason = Some(reason);
        runtime.state.reconnect_started_at_ms = Some(now_ms());
        runtime.state.reconnect_attempt = 0;
        // Keep metrics and configured automations intact. Only ephemeral game
        // work that cannot be proven after a full transport rebound is reset.
        runtime.clear_uncertain_game_state();
        Ok(true)
    }
    /// Enter Background recovery from a proven Browser loss, or finish an
    /// already-started Browser→Background transition once its target is closed.
    pub fn begin_browser_recovery(
        &self,
        account_id: &str,
        reason: String,
    ) -> Result<bool, AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        match runtime.account.owner {
            ConnectionOwner::Browser => {
                runtime.browser_control = None;
                runtime.browser_transport_ready = false;
                runtime.account.owner = ConnectionOwner::Transition;
                runtime.account.mode = AccountMode::Transitioning;
                runtime.account.runtime = AccountRuntimeState::Reconnecting;
                runtime.account.status = ConnectionStatus::Connecting;
                runtime.state.disconnect_reason = Some(reason);
                runtime.state.reconnect_started_at_ms = Some(now_ms());
                runtime.state.reconnect_attempt = 0;
                runtime.clear_uncertain_game_state();
                Ok(true)
            }
            ConnectionOwner::Transition
                if runtime.background.is_none()
                    && matches!(
                        runtime.account.runtime,
                        AccountRuntimeState::PreparingHandoff
                            | AccountRuntimeState::BackgroundConnecting
                    ) =>
            {
                runtime.account.runtime = AccountRuntimeState::Reconnecting;
                runtime.account.status = ConnectionStatus::Connecting;
                runtime.state.disconnect_reason = Some(reason);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn restore_browser_owner(&self, account_id: &str) {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let Some(runtime) = runtimes.get_mut(account_id) else {
            return;
        };
        if runtime.account.owner == ConnectionOwner::Transition {
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.account.mode = AccountMode::Browser;
            runtime.account.runtime = AccountRuntimeState::BrowserConnected;
            runtime.account.status = ConnectionStatus::Online;
        }
    }
    pub fn transition(
        &self,
        account_id: &str,
        runtime_state: AccountRuntimeState,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        runtime.account.runtime = runtime_state.clone();
        match runtime_state {
            AccountRuntimeState::Background => {
                runtime.account.status = ConnectionStatus::Online;
                runtime.account.mode = AccountMode::Background;
                runtime.account.owner = ConnectionOwner::Background;
            }
            AccountRuntimeState::WaitingForLogin | AccountRuntimeState::LoginRequired => {
                runtime.account.status = ConnectionStatus::LoginRequired;
                runtime.account.mode = AccountMode::Browser;
                runtime.account.owner = ConnectionOwner::None;
                runtime.state.pending_navigation = None;
                runtime.state.pending_hunt_slug = None;
                runtime.navigation_sent_at_ms = None;
            }
            AccountRuntimeState::RenewingSession => {
                runtime.account.status = ConnectionStatus::Connecting;
                runtime.account.mode = AccountMode::Transitioning;
                runtime.account.owner = ConnectionOwner::Transition;
            }
            AccountRuntimeState::PreparingHandoff
            | AccountRuntimeState::BackgroundConnecting
            | AccountRuntimeState::Reconnecting
            | AccountRuntimeState::BrowserBootstrap => {
                runtime.account.status = ConnectionStatus::Connecting;
                runtime.account.mode = AccountMode::Transitioning;
                runtime.account.owner = ConnectionOwner::Transition;
                runtime.browser_transport_ready = false;
            }
            AccountRuntimeState::Error => runtime.account.status = ConnectionStatus::Error,
            AccountRuntimeState::Offline | AccountRuntimeState::Stopping => {
                runtime.account.status = ConnectionStatus::Offline
            }
            _ => runtime.account.status = ConnectionStatus::Connecting,
        }
        Ok(())
    }
    pub fn attach_background(
        &self,
        account_id: &str,
        connection: BackgroundConnection,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.background.is_some()
            || matches!(
                runtime.account.runtime,
                AccountRuntimeState::Offline | AccountRuntimeState::Stopping
            )
        {
            return Err(AccountError::NotFound(
                "Conta fora de uma transição ativa ou já conectada; Background rejeitado".into(),
            ));
        }
        runtime.background = Some(connection);
        runtime.account.status = ConnectionStatus::Online;
        runtime.account.mode = AccountMode::Background;
        runtime.account.runtime = AccountRuntimeState::Background;
        runtime.account.owner = ConnectionOwner::Background;
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        Ok(())
    }
    pub fn attach_background_if_lifecycle_current(
        &self,
        account_id: &str,
        epoch: u64,
        connection: BackgroundConnection,
    ) -> Result<(), AccountError> {
        let _mobile_change = self.mobile_change_guard();
        let controls = self.lifecycle_locks.lock();
        let valid = controls
            .get(account_id)
            .is_some_and(|control| control.epoch == epoch && !control.cancellation.is_cancelled());
        if !valid {
            return Err(AccountError::NotFound(
                "Tentativa de conexão obsoleta; lifecycle já avançou".into(),
            ));
        }
        let mut runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get_mut(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        if runtime.account.owner != ConnectionOwner::Transition
            || runtime.account.runtime == AccountRuntimeState::Offline
            || runtime.account.runtime == AccountRuntimeState::Stopping
            || runtime.background.is_some()
        {
            return Err(AccountError::NotFound(
                "Conta deixou de estar em uma transição válida para Background".into(),
            ));
        }
        runtime.background = Some(connection);
        runtime.account.status = ConnectionStatus::Online;
        runtime.account.mode = AccountMode::Background;
        runtime.account.runtime = AccountRuntimeState::Background;
        runtime.account.owner = ConnectionOwner::Background;
        runtime.browser_control = None;
        runtime.browser_transport_ready = false;
        Ok(())
    }
    pub fn stop_all(&self) {
        let _mobile_change = self.mobile_change_guard();
        self.shutting_down.store(true, Ordering::Release);
        for control in self.lifecycle_locks.lock().values_mut() {
            control.epoch = control.epoch.wrapping_add(1);
            control.cancellation.cancel();
        }
        let mut runtimes = self.runtimes.lock();
        for runtime in runtimes.values_mut() {
            runtime.account.runtime = AccountRuntimeState::Stopping;
            runtime.account.owner = ConnectionOwner::None;
            if let Some(connection) = runtime.background.take() {
                connection.cancel();
            }
            if let Some(control) = runtime.browser_control.take() {
                let _ = control.send(BrowserControl::CloseManagedBrowser);
            }
            runtime.browser_transport_ready = false;
            runtime.account.status = ConnectionStatus::Offline;
            runtime.account.runtime = AccountRuntimeState::Offline;
        }
    }
    pub fn ids(&self) -> Vec<String> {
        self.runtimes.lock().keys().cloned().collect()
    }
    pub fn snapshots(&self) -> Vec<AccountSnapshot> {
        self.runtimes
            .lock()
            .values_mut()
            .map(AccountRuntime::snapshot)
            .collect()
    }
    pub fn live_snapshots(&self) -> Vec<AccountLiveSnapshot> {
        self.runtimes
            .lock()
            .values_mut()
            .map(AccountRuntime::live_snapshot)
            .collect()
    }
    /// Small lifecycle-only metadata for automatic monitors. This must not
    /// call `snapshot()`, which clones all depot and hunt data.
    pub fn runtime_metadata(&self) -> Vec<(AccountRecord, Option<String>)> {
        self.runtimes
            .lock()
            .values()
            .map(|runtime| {
                (
                    runtime.account.clone(),
                    runtime.state.disconnect_reason.clone(),
                )
            })
            .collect()
    }
    pub fn runtime_metadata_for(
        &self,
        account_id: &str,
    ) -> Option<(AccountRecord, Option<String>)> {
        self.runtimes.lock().get(account_id).map(|runtime| {
            (
                runtime.account.clone(),
                runtime.state.disconnect_reason.clone(),
            )
        })
    }
    pub fn market_metadata(&self) -> Vec<AccountMarketMetadata> {
        self.runtimes
            .lock()
            .values_mut()
            .map(|runtime| {
                runtime.state.command_transport_available =
                    AccountCommandDispatcher::route(runtime).is_ok();
                AccountMarketMetadata {
                    account: runtime.account.clone(),
                    command_transport_available: runtime.state.command_transport_available,
                    server_offset_ms: runtime.state.server_offset_ms,
                    gold: runtime.state.gold.unwrap_or(0),
                    orbs: runtime.state.orbs.unwrap_or(0),
                }
            })
            .collect()
    }
    pub fn market_metadata_for(&self, account_id: &str) -> Option<AccountMarketMetadata> {
        self.runtimes.lock().get_mut(account_id).map(|runtime| {
            runtime.state.command_transport_available =
                AccountCommandDispatcher::route(runtime).is_ok();
            AccountMarketMetadata {
                account: runtime.account.clone(),
                command_transport_available: runtime.state.command_transport_available,
                server_offset_ms: runtime.state.server_offset_ms,
                gold: runtime.state.gold.unwrap_or(0),
                orbs: runtime.state.orbs.unwrap_or(0),
            }
        })
    }
    pub fn automation_ids(&self, account_id: &str) -> Result<(Vec<u64>, Vec<u64>), AccountError> {
        let runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        Ok((
            runtime.state.automation.potion_ids.clone(),
            runtime.state.automation.ball_ids.clone(),
        ))
    }
    pub fn account_depot(
        &self,
        account_id: &str,
        known_revision: Option<u64>,
    ) -> Result<VersionedAccountRead<Vec<AccountDepotPokemon>>, AccountError> {
        let runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        let revision = runtime.data_revisions.depot;
        let changed = known_revision != Some(revision);
        let data = changed.then(|| {
            runtime
                .state
                .pokemon
                .iter()
                .map(|pokemon| AccountDepotPokemon {
                    id: pokemon.id.to_string(),
                    name: pokemon.name.clone(),
                    level: pokemon.level,
                    types: pokemon.types.clone(),
                    locked: false,
                    power: pokemon.potencia,
                    quality: pokemon.quality,
                    note: pokemon.nota,
                    iv_total: pokemon.iv_total,
                    shiny: pokemon.shiny,
                    species_id: pokemon.species_id,
                    looktype: pokemon.looktype,
                    look_shiny: pokemon.look_shiny,
                })
                .collect()
        });
        Ok(VersionedAccountRead {
            account_id: account_id.to_owned(),
            revision,
            changed,
            data,
        })
    }
    pub fn account_inventory(
        &self,
        account_id: &str,
        known_revision: Option<u64>,
    ) -> Result<VersionedAccountRead<Vec<AccountInventoryEntry>>, AccountError> {
        let runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        let revision = runtime.data_revisions.inventory;
        let changed = known_revision != Some(revision);
        let data = changed.then(|| {
            let mut entries =
                Vec::with_capacity(runtime.state.items.len() + runtime.state.balls.len());
            entries.extend(runtime.state.items.iter().map(|(id, quantity)| {
                let name = account_inventory_item_name(id, false);
                AccountInventoryEntry {
                    id: id.clone(),
                    asset_key: id.clone(),
                    category: if matches!(
                        id.as_str(),
                        "200" | "201" | "202" | "203" | "204" | "70070"
                    ) {
                        "potions"
                    } else {
                        "other"
                    }
                    .into(),
                    name,
                    quantity: *quantity,
                }
            }));
            entries.extend(runtime.state.balls.iter().map(|(id, quantity)| {
                let entry_id = format!("ball-{id}");
                AccountInventoryEntry {
                    id: entry_id.clone(),
                    asset_key: entry_id,
                    name: account_inventory_item_name(id, true),
                    quantity: *quantity,
                    category: "balls".into(),
                }
            }));
            entries
        });
        Ok(VersionedAccountRead {
            account_id: account_id.to_owned(),
            revision,
            changed,
            data,
        })
    }
    pub fn account_hunt_options(
        &self,
        account_id: &str,
        known_revision: Option<u64>,
    ) -> Result<VersionedAccountRead<Vec<AccountHuntOption>>, AccountError> {
        let runtimes = self.runtimes.lock();
        let runtime = runtimes
            .get(account_id)
            .ok_or_else(|| AccountError::NotFound(account_id.to_owned()))?;
        let revision = runtime.data_revisions.hunt_options;
        let changed = known_revision != Some(revision);
        let data = changed.then(|| {
            runtime
                .state
                .hunts
                .iter()
                .map(|hunt| AccountHuntOption {
                    slug: hunt.slug.clone(),
                    name: hunt.name.clone(),
                    area: hunt.area.clone(),
                    level: hunt.level,
                    total_spawns: hunt.total_spawns,
                    region: hunt.region.clone(),
                    looktype: hunt.looktype,
                    species: hunt
                        .species
                        .iter()
                        .map(|species| AccountHuntSpecies {
                            species_id: species.species_id,
                            weight: species.weight,
                        })
                        .collect(),
                })
                .collect()
        });
        Ok(VersionedAccountRead {
            account_id: account_id.to_owned(),
            revision,
            changed,
            data,
        })
    }
    /// Captures the owner and dispatcher-selected transport for every account
    /// under one account-map lock. Available only in debug builds and does not
    /// process actions, send commands, or expose credentials/session material.
    /// Result ordering is unspecified; use `account_id` as the stable key.
    #[cfg(debug_assertions)]
    pub fn transport_diagnostics(&self) -> Vec<AccountTransportDiagnostics> {
        self.runtimes
            .lock()
            .values()
            .map(|runtime| AccountTransportDiagnostics {
                account_id: runtime.account.id.clone(),
                owner: runtime.account.owner.clone(),
                browser_control_attached: runtime.browser_control.is_some(),
                browser_transport_ready: runtime.browser_transport_ready,
                background_transport_attached: runtime.background.is_some(),
                selected_transport: AccountCommandDispatcher::route(runtime).ok().map(
                    |transport| match transport {
                        CommandTransport::Browser => AccountDiagnosticTransport::Browser,
                        CommandTransport::Background => AccountDiagnosticTransport::Background,
                    },
                ),
            })
            .collect()
    }

    /// Snapshot of blocked mutating attempts observed by the account
    /// dispatcher while validation read-only mode is active.
    #[cfg(debug_assertions)]
    pub fn validation_command_counters(&self) -> ValidationCommandCountersSnapshot {
        ValidationCommandCountersSnapshot {
            mutable_attempts: self
                .validation_command_counters
                .mutable_attempts
                .load(Ordering::Acquire),
            mutable_blocked: self
                .validation_command_counters
                .mutable_blocked
                .load(Ordering::Acquire),
            mutable_sent: self
                .validation_command_counters
                .mutable_sent
                .load(Ordering::Acquire),
        }
    }
    /// Builds the limited mobile read model without calling `snapshot()`: this
    /// path does not process actions, inspect command routing, clone account
    /// state, touch persistence, or mutate rolling metrics.
    #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
    pub fn mobile_snapshot(&self) -> MobileSnapshot {
        let manager_timestamp_ms = now_ms();
        let projections: Vec<MobileAccountProjection> = {
            let runtimes = self.runtimes.lock();
            runtimes
                .values()
                .map(|runtime| {
                    crate::mobile::capture_account_projection(
                        &runtime.account,
                        &runtime.state,
                        &runtime.metrics,
                        manager_timestamp_ms,
                    )
                })
                .collect()
        };
        let accounts = projections
            .into_iter()
            .map(MobileAccountProjection::into_account)
            .collect();
        let revision = self
            .mobile_snapshot_revision
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                Some(revision.saturating_add(1))
            })
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        MobileSnapshot::from_accounts(revision, manager_timestamp_ms, accounts)
    }

    /// Projects only one bounded inventory page while holding the account map
    /// lock. It never clones an AccountState or exposes runtime-only fields.
    #[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
    pub fn mobile_inventory_page(
        &self,
        account_id: &str,
        kind: crate::mobile::MobileInventoryKind,
        offset: usize,
        limit: usize,
        query: &str,
        category: Option<crate::mobile::MobileInventoryCategory>,
        pokemon_type: &str,
        minimum_iv: Option<u64>,
        order: &str,
        market_names: &HashMap<u64, String>,
        item_categories: &HashMap<u64, String>,
    ) -> Option<crate::mobile::MobileInventoryPage> {
        use crate::mobile::{
            MobileInventoryCategory as Category, MobileInventoryItem, MobileInventoryKind as Kind,
            MobileInventoryPage, MobileInventoryPokemon,
        };
        use std::collections::BTreeMap;

        let runtimes = self.runtimes.lock();
        let runtime = runtimes.get(account_id)?;
        let needle = query.trim().to_lowercase();

        let (total, items, pokemon, types) = match kind {
            Kind::Items => {
                // These maps are compact ID -> count projections. Merge only
                // those pairs, then filter and paginate before allocating DTOs.
                let mut stacks = BTreeMap::<u64, (u64, Category)>::new();
                for (raw_id, quantity) in &runtime.state.items {
                    let Ok(id) = raw_id.parse::<u64>() else {
                        continue;
                    };
                    let category = item_categories
                        .get(&id)
                        .map(|category| match category.as_str() {
                            "heal" => Category::Potion,
                            "stone" => Category::Stone,
                            _ => Category::Other,
                        })
                        .unwrap_or_else(|| inventory_item_category(id, false));
                    stacks.insert(id, (*quantity, category));
                }
                for (raw_id, quantity) in &runtime.state.balls {
                    let Ok(id) = raw_id.parse::<u64>() else {
                        continue;
                    };
                    stacks.insert(id, (*quantity, Category::Ball));
                }

                let filtered: Vec<_> = stacks
                    .into_iter()
                    .filter(|(id, (_, item_category))| {
                        category.is_none_or(|selected| selected == *item_category)
                            && (needle.is_empty()
                                || market_names
                                    .get(id)
                                    .map_or_else(|| inventory_item_name(*id), String::clone)
                                    .to_lowercase()
                                    .contains(&needle))
                    })
                    .collect();
                let total = filtered.len();
                let items = filtered
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(|(id, (quantity, category))| MobileInventoryItem {
                        id: id.to_string(),
                        name: market_names
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| inventory_item_name(id)),
                        category,
                        quantity,
                        asset_path: None,
                    })
                    .collect();
                (
                    total,
                    items,
                    Vec::<MobileInventoryPokemon>::new(),
                    Vec::new(),
                )
            }
            Kind::Pokemon => {
                // Inventory pages are bounded, but the filtered index is sorted
                // here so mobile pagination matches the desktop inventory order.
                let mut available_types = std::collections::BTreeSet::new();
                let mut filtered: Vec<_> = runtime
                    .state
                    .pokemon
                    .iter()
                    .filter(|entry| {
                        available_types.extend(entry.types.iter().cloned());
                        (needle.is_empty() || entry.name.to_lowercase().contains(&needle))
                            && (pokemon_type.is_empty()
                                || entry.types.iter().any(|value| value == pokemon_type))
                            && minimum_iv
                                .is_none_or(|minimum| entry.iv_total.unwrap_or(0) >= minimum)
                    })
                    .collect();
                match order {
                    "quality" => filtered.sort_by(|a, b| {
                        b.quality
                            .unwrap_or_default()
                            .total_cmp(&a.quality.unwrap_or_default())
                    }),
                    "power" => filtered.sort_by(|a, b| {
                        b.poder
                            .unwrap_or_default()
                            .cmp(&a.poder.unwrap_or_default())
                    }),
                    "note" => filtered.sort_by(|a, b| {
                        b.nota
                            .unwrap_or_default()
                            .total_cmp(&a.nota.unwrap_or_default())
                    }),
                    "iv" => filtered.sort_by(|a, b| {
                        b.iv_total
                            .unwrap_or_default()
                            .cmp(&a.iv_total.unwrap_or_default())
                    }),
                    "type" => filtered.sort_by(|a, b| a.types.join("/").cmp(&b.types.join("/"))),
                    // The protocol does not currently provide a capture timestamp.
                    // Keep authoritative order for "recent", like desktop's fallback.
                    "level" => filtered.sort_by(|a, b| b.level.cmp(&a.level)),
                    "recent" => {}
                    _ => {}
                }
                let total = filtered.len();
                let pokemon = filtered
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .map(|entry| MobileInventoryPokemon {
                        id: entry.id.to_string(),
                        name: entry
                            .name
                            .chars()
                            .filter(|character| !character.is_control())
                            .take(80)
                            .collect(),
                        level: entry.level,
                        shiny: entry.shiny,
                        species_id: entry.species_id,
                        looktype: entry.looktype,
                        look_shiny: entry.look_shiny,
                        quality: entry.quality,
                        note: entry.nota,
                        power: entry.poder,
                        iv_total: entry.iv_total,
                        types: entry
                            .types
                            .iter()
                            .take(4)
                            .map(|value| {
                                value
                                    .chars()
                                    .filter(|character| !character.is_control())
                                    .take(24)
                                    .collect()
                            })
                            .collect(),
                    })
                    .collect();
                (
                    total,
                    Vec::<MobileInventoryItem>::new(),
                    pokemon,
                    available_types.into_iter().collect(),
                )
            }
        };

        Some(MobileInventoryPage {
            account_id: runtime.account.id.clone(),
            kind,
            offset,
            limit,
            total,
            items,
            pokemon,
            types,
        })
    }

    pub fn count(&self) -> usize {
        self.runtimes.lock().len()
    }
}

#[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
fn inventory_item_category(item_id: u64, is_ball: bool) -> crate::mobile::MobileInventoryCategory {
    use crate::mobile::MobileInventoryCategory as Category;
    if is_ball || (1..=5).contains(&item_id) {
        Category::Ball
    } else if matches!(item_id, 200..=204 | 70_070) {
        Category::Potion
    } else if item_id == 70_032 {
        // This is the only stone ID confirmed in the bundled client extensions.
        Category::Stone
    } else {
        Category::Other
    }
}

/// Compact account fields consumed by the market worker. Periodic market
/// checks should not clone the full depot/hunt snapshot.
#[derive(Clone, Debug)]
pub struct AccountMarketMetadata {
    pub account: AccountRecord,
    pub command_transport_available: bool,
    pub server_offset_ms: Option<i64>,
    pub gold: u64,
    pub orbs: u64,
}

fn account_inventory_item_name(id: &str, is_ball: bool) -> String {
    let parsed = id.parse::<u64>().ok();
    match (is_ball, parsed) {
        (true, Some(1)) => "Poké Ball".into(),
        (true, Some(2)) => "Great Ball".into(),
        (true, Some(3)) => "Super Ball".into(),
        (true, Some(4)) => "Ultra Ball".into(),
        (true, Some(5)) => "Beast Ball".into(),
        (true, _) => format!("Ball #{id}"),
        (false, Some(200)) => "Small Potion".into(),
        (false, Some(201)) => "Great Potion".into(),
        (false, Some(202)) => "Ultra Potion".into(),
        (false, Some(203)) => "Hyper Potion".into(),
        (false, Some(204)) => "Ultimate Potion".into(),
        (false, Some(70_070)) => "Golden Potion".into(),
        (false, _) => format!("Item #{id}"),
    }
}

#[cfg(any(test, all(debug_assertions, feature = "mobile-local-server")))]
fn inventory_item_name(item_id: u64) -> String {
    match item_id {
        1 => "Poké Ball".into(),
        2 => "Great Ball".into(),
        3 => "Super Ball".into(),
        4 => "Ultra Ball".into(),
        5 => "Beast Ball".into(),
        200 => "Small Potion".into(),
        201 => "Great Potion".into(),
        202 => "Ultra Potion".into(),
        203 => "Hyper Potion".into(),
        204 => "Ultimate Potion".into(),
        70_070 => "Golden Potion".into(),
        70_032 => "Shiny Stone ROCK".into(),
        _ => format!("Item #{item_id}"),
    }
}
fn estimated_server_now(server_offset_ms: i64) -> u64 {
    let local_now = now_ms();
    if server_offset_ms >= 0 {
        local_now.saturating_add(server_offset_ms as u64)
    } else {
        local_now.saturating_sub(server_offset_ms.unsigned_abs())
    }
}
fn merge_pokemon_patch(pokemon: &mut Pokemon, mut fields: Map<String, Value>) {
    if let Some(name) = fields.get("nome").and_then(Value::as_str) {
        pokemon.name = name.to_owned();
    }
    if let Some(level) = fields
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        pokemon.level = level;
    }
    if let Some(hp) = fields.get("hp").and_then(Value::as_u64) {
        pokemon.hp = hp;
    }
    if let Some(max_hp) = fields.get("maxHp").and_then(Value::as_u64) {
        pokemon.max_hp = max_hp;
    }
    if let Some(xp) = fields.get("xp").and_then(Value::as_u64) {
        pokemon.xp = Some(xp);
    }
    if let Some(xp_level) = fields.get("xpNivel").and_then(Value::as_u64) {
        pokemon.xp_level = Some(xp_level);
    }
    if let Some(xp_next) = fields.get("xpProximo").and_then(Value::as_u64) {
        pokemon.xp_next = Some(xp_next);
    }
    if let Some(quality) = fields.get("quality").and_then(Value::as_f64) {
        pokemon.quality = Some(quality);
    }
    if let Some(potencia) = fields
        .get("potencia")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
    {
        pokemon.potencia = Some(potencia);
    }
    if let Some(poder) = fields.get("poder").and_then(Value::as_u64) {
        pokemon.poder = Some(poder);
    }
    if let Some(nota) = fields.get("nota").and_then(Value::as_f64) {
        pokemon.nota = Some(nota);
    }
    if let Some(shiny) = fields.get("shiny").and_then(Value::as_bool) {
        pokemon.shiny = shiny;
    }
    if let Some(species_id) = fields.get("speciesId").and_then(Value::as_u64) {
        pokemon.species_id = Some(species_id);
    }
    if let Some(looktype) = fields.get("looktype").and_then(Value::as_u64) {
        pokemon.looktype = Some(looktype);
    }
    if let Some(look_shiny) = fields.get("lookShiny").and_then(Value::as_u64) {
        pokemon.look_shiny = Some(look_shiny);
    }
    if let Some(held_item_id) = fields.get("heldItemId").and_then(Value::as_u64) {
        pokemon.held_item_id = Some(held_item_id);
    }
    if let Some(types) = fields
        .get("tipos")
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
    {
        pokemon.types = types;
    }
    if let Some(ivs) = fields
        .get("ivs")
        .and_then(|value| serde_json::from_value::<crate::protocol::RemoteIvs>(value.clone()).ok())
    {
        let current = pokemon.ivs.get_or_insert_with(Default::default);
        if ivs.hp.is_some() {
            current.hp = ivs.hp;
        }
        if ivs.atk.is_some() {
            current.atk = ivs.atk;
        }
        if ivs.def.is_some() {
            current.def = ivs.def;
        }
        if ivs.sp_atk.is_some() {
            current.sp_atk = ivs.sp_atk;
        }
        if ivs.sp_def.is_some() {
            current.sp_def = ivs.sp_def;
        }
        if ivs.speed.is_some() {
            current.speed = ivs.speed;
        }
        pokemon.iv_total = match (
            current.hp,
            current.atk,
            current.def,
            current.sp_atk,
            current.sp_def,
            current.speed,
        ) {
            (Some(hp), Some(atk), Some(def), Some(sp_atk), Some(sp_def), Some(speed)) => {
                Some(hp + atk + def + sp_atk + sp_def + speed)
            }
            _ => None,
        };
    }
    fields.remove("id");
    pokemon.patch_fields.extend(fields);
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn new_record(id: String, nick: String, color: String) -> AccountRecord {
    AccountRecord {
        id,
        nick,
        local_alias: None,
        card_color: color,
        status: ConnectionStatus::Offline,
        mode: AccountMode::Background,
        runtime: AccountRuntimeState::Offline,
        owner: ConnectionOwner::None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspector::ProtocolDirection;

    #[test]
    fn four_account_limit_and_stable_ids_are_enforced() {
        let manager = AccountManager::default();
        for id in ["a", "b", "c", "d"] {
            manager
                .add(new_record(id.into(), format!("nick-{id}"), "#fff".into()))
                .unwrap();
        }
        assert!(matches!(
            manager.add(new_record("e".into(), "nick-e".into(), "#fff".into())),
            Err(AccountError::LimitReached)
        ));
        manager.remove("b").unwrap();
        let ids = manager.ids();
        assert_eq!(manager.count(), 3);
        assert!(ids.contains(&"a".to_owned()));
        assert!(ids.contains(&"c".to_owned()));
        assert!(ids.contains(&"d".to_owned()));
    }

    #[test]
    fn live_snapshot_is_allowlisted_and_includes_one_active_pokemon() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "Trainer A".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"level":50,"activeId":2,"items":{"200":3},"balls":{"4":8},"automation":{"potionIds":[200],"ballIds":[4]},"pokemons":[{"id":1,"nome":"Pupitar","level":40,"hp":10,"maxHp":20},{"id":2,"nome":"Tyranitar","level":50,"hp":20,"maxHp":30,"quality":1.7}],"huntSlug":"ancient_pupitar"},"hunts":[{"slug":"ancient_pupitar","nome":"Ancient Pupitar","area":"kanto","especies":[{"pokeId":248,"pontos":1}]}]}"#,
                )
                .unwrap(),
            )
            .unwrap();

        let live = manager.live_snapshots().remove(0);
        assert_eq!(
            live.state.active_pokemon.as_ref().unwrap().name,
            "Tyranitar"
        );
        assert_eq!(live.current_potion_id, Some(200));
        assert_eq!(live.current_potion_quantity, 3);
        assert_eq!(live.current_ball_id, Some(4));
        assert_eq!(live.current_ball_quantity, 8);
        assert_eq!(
            live.state.active_hunt.as_ref().unwrap().name,
            "Ancient Pupitar"
        );

        let json = serde_json::to_value(live).unwrap();
        let state = json.get("state").unwrap();
        for forbidden in ["pokemon", "hunts", "items", "balls"] {
            assert!(
                state.get(forbidden).is_none(),
                "unexpected field: {forbidden}"
            );
        }
        assert!(state.get("active_pokemon").is_some());
        assert!(state.get("active_hunt").is_some());
    }

    #[test]
    fn on_demand_collection_reads_return_data_only_for_a_new_revision() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "Trainer A".into(), "#fff".into()))
            .unwrap();
        let frame = || {
            ServerFrame::parse(
                r#"{"t":"welcome","estado":{"activeId":1,"items":{"200":3},"balls":{"4":8},"pokemons":[{"id":1,"nome":"Pupitar","level":40,"hp":10,"maxHp":20}]},"hunts":[{"slug":"ancient_pupitar","nome":"Ancient Pupitar","area":"kanto","especies":[{"pokeId":248,"pontos":1}]}]}"#,
            )
            .unwrap()
        };
        manager.ingest("a", frame()).unwrap();

        let depot = manager.account_depot("a", None).unwrap();
        let inventory = manager.account_inventory("a", None).unwrap();
        let hunts = manager.account_hunt_options("a", None).unwrap();
        assert!(depot.changed && depot.data.as_ref().is_some_and(|data| data.len() == 1));
        assert!(inventory.changed && inventory.data.as_ref().is_some_and(|data| data.len() == 2));
        assert!(hunts.changed && hunts.data.as_ref().is_some_and(|data| data.len() == 1));
        let depot_json = serde_json::to_value(&depot).unwrap();
        assert_eq!(depot_json["accountId"], "a");
        assert!(depot_json["data"].is_array());
        assert_eq!(depot_json["data"][0]["id"], "1");
        assert_eq!(depot_json["data"][0]["locked"], false);
        let inventory_json = serde_json::to_value(&inventory).unwrap();
        assert_eq!(inventory_json["data"][0]["assetKey"], "200");
        let hunts_json = serde_json::to_value(&hunts).unwrap();
        assert_eq!(
            hunts_json["data"][0]["totalSpawns"],
            serde_json::Value::Null
        );
        assert_eq!(hunts_json["data"][0]["species"][0]["speciesId"], 248);

        let depot_same = manager.account_depot("a", Some(depot.revision)).unwrap();
        let inventory_same = manager
            .account_inventory("a", Some(inventory.revision))
            .unwrap();
        let hunts_same = manager
            .account_hunt_options("a", Some(hunts.revision))
            .unwrap();
        assert!(!depot_same.changed && depot_same.data.is_none());
        assert!(!inventory_same.changed && inventory_same.data.is_none());
        assert!(!hunts_same.changed && hunts_same.data.is_none());

        manager.ingest("a", frame()).unwrap();
        assert_eq!(
            manager
                .account_depot("a", Some(depot.revision))
                .unwrap()
                .revision,
            depot.revision
        );
        assert_eq!(
            manager
                .account_inventory("a", Some(inventory.revision))
                .unwrap()
                .revision,
            inventory.revision
        );
        assert_eq!(
            manager
                .account_hunt_options("a", Some(hunts.revision))
                .unwrap()
                .revision,
            hunts.revision
        );

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"estado","estado":{"items":{"200":4},"pkMud":[{"id":1,"hp":9}]}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let depot_changed = manager.account_depot("a", Some(depot.revision)).unwrap();
        let inventory_changed = manager
            .account_inventory("a", Some(inventory.revision))
            .unwrap();
        assert!(!depot_changed.changed && depot_changed.data.is_none());
        assert!(inventory_changed.changed && inventory_changed.data.is_some());
        assert_eq!(depot_changed.revision, depot.revision);
        assert!(inventory_changed.revision > inventory.revision);
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"estado","estado":{"items":{"200":4},"pkMud":[{"id":1,"hp":9}]}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            manager.account_depot("a", None).unwrap().revision,
            depot.revision
        );
        assert!(manager.account_inventory("a", None).unwrap().revision > inventory.revision);

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"pkMud":[{"id":1,"level":41}]}}"#)
                    .unwrap(),
            )
            .unwrap();
        let visible_depot_change = manager.account_depot("a", Some(depot.revision)).unwrap();
        assert!(visible_depot_change.changed && visible_depot_change.data.is_some());
        assert!(visible_depot_change.revision > depot.revision);
    }

    #[test]
    fn on_demand_collection_reads_reject_unknown_accounts() {
        let manager = AccountManager::default();
        assert!(manager.account_depot("missing", None).is_err());
        assert!(manager.account_inventory("missing", None).is_err());
        assert!(manager.account_hunt_options("missing", None).is_err());
    }

    #[test]
    fn collection_revisions_do_not_repeat_after_account_recreation() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "Trainer A".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"pokemons":[{"id":1,"nome":"Pupitar","level":40,"hp":10,"maxHp":20}]}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let previous = manager.account_depot("a", None).unwrap().revision;
        manager.remove("a").unwrap();
        manager
            .add(new_record("a".into(), "Trainer A".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"pokemons":[{"id":2,"nome":"Tyranitar","level":50,"hp":20,"maxHp":30}]}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let replacement = manager.account_depot("a", Some(previous)).unwrap();
        assert!(replacement.changed);
        assert!(replacement.revision > previous);
        assert_eq!(replacement.data.as_ref().unwrap()[0].id, "2");
    }

    #[test]
    fn browser_reconnect_is_only_available_without_an_existing_owner() {
        let manager = AccountManager::default();
        manager
            .add(new_record(
                "browser".into(),
                "DemoTrainerOne".into(),
                "#fff".into(),
            ))
            .unwrap();
        manager.browser_control_unavailable("browser", "CDP perdido".into());

        manager.begin_browser_reconnect("browser").unwrap();
        let snapshot = manager.snapshots().remove(0);
        assert_eq!(snapshot.account.owner, ConnectionOwner::Transition);
        assert_eq!(snapshot.account.mode, AccountMode::Transitioning);
        assert_eq!(snapshot.account.runtime, AccountRuntimeState::Reconnecting);
        assert_eq!(snapshot.account.status, ConnectionStatus::Connecting);
        assert_eq!(
            snapshot.state.disconnect_reason.as_deref(),
            Some("CDP perdido")
        );

        let (control, _receiver) = mpsc::unbounded_channel();
        manager.attach_browser_control("browser", control).unwrap();
        manager.browser_transfer_failed("browser", "CDP reconexão falhou".into());
        let snapshot = manager.snapshots().remove(0);
        assert_eq!(snapshot.account.owner, ConnectionOwner::None);
        assert_eq!(snapshot.account.mode, AccountMode::Browser);
        assert_eq!(snapshot.account.runtime, AccountRuntimeState::Error);
        assert_eq!(snapshot.account.status, ConnectionStatus::Error);
        assert_eq!(
            snapshot.state.disconnect_reason.as_deref(),
            Some("CDP reconexão falhou")
        );
        assert!(!manager.transport_diagnostics()[0].browser_control_attached);
    }

    #[test]
    fn browser_reconnect_cannot_steal_healthy_browser_or_background_owners() {
        let manager = AccountManager::default();
        for id in ["browser", "background"] {
            manager
                .add(new_record(id.into(), id.into(), "#fff".into()))
                .unwrap();
        }
        {
            let mut runtimes = manager.runtimes.lock();
            let browser = runtimes.get_mut("browser").unwrap();
            browser.account.mode = AccountMode::Browser;
            browser.account.owner = ConnectionOwner::Browser;
            browser.account.status = ConnectionStatus::Online;
            browser.account.runtime = AccountRuntimeState::BrowserConnected;
            let background = runtimes.get_mut("background").unwrap();
            background.account.status = ConnectionStatus::Error;
        }

        assert!(manager.begin_browser_reconnect("browser").is_err());
        assert!(manager.begin_browser_reconnect("background").is_err());
        let snapshots = manager.snapshots();
        assert!(
            snapshots
                .iter()
                .all(|snapshot| { snapshot.account.owner != ConnectionOwner::Transition })
        );
    }

    #[test]
    fn mobile_inventory_pages_filter_before_pagination_and_omit_runtime_fields() {
        use crate::mobile::{MobileInventoryCategory as Category, MobileInventoryKind as Kind};

        let manager = AccountManager::default();
        manager
            .add(new_record(
                "a".into(),
                "DemoTrainerFour".into(),
                "#fff".into(),
            ))
            .unwrap();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.state.items.insert("204".into(), 3);
            runtime.state.items.insert("70032".into(), 1);
            runtime.state.items.insert("999999".into(), 8);
            runtime.state.balls.insert("5".into(), 12);
            runtime.state.pokemon.push(Pokemon {
                id: 42,
                name: "Shiny Ralts".into(),
                level: 17,
                hp: 10,
                max_hp: 20,
                xp: Some(123),
                xp_level: None,
                xp_next: None,
                quality: Some(91.5),
                potencia: Some(5),
                poder: Some(500),
                nota: Some(9.1),
                shiny: true,
                species_id: Some(280),
                looktype: Some(301),
                look_shiny: Some(302),
                held_item_id: Some(4),
                types: vec!["psychic".into(), "fairy".into()],
                iv_total: Some(120),
                ivs: None,
                patch_fields: Default::default(),
            });
        }

        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Items,
                0,
                1,
                "",
                Some(Category::Potion),
                "",
                None,
                "level",
                &HashMap::new(),
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "Ultimate Potion");
        assert_eq!(page.items[0].quantity, 3);
        let catalog_categories = HashMap::from([
            (204, "heal".to_owned()),
            (70_032, "stone".to_owned()),
            (999_999, "heal".to_owned()),
        ]);
        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Items,
                0,
                10,
                "",
                Some(Category::Potion),
                "",
                None,
                "level",
                &HashMap::new(),
                &catalog_categories,
            )
            .unwrap();
        assert_eq!(page.total, 2);
        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Items,
                0,
                10,
                "stone",
                None,
                "",
                None,
                "level",
                &HashMap::new(),
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].category, Category::Stone);

        let enriched_names = HashMap::from([(999_999, "Rare Festival Bicycle".to_owned())]);
        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Items,
                0,
                1,
                "festival bicycle",
                None,
                "",
                None,
                "level",
                &enriched_names,
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].name, "Rare Festival Bicycle");

        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Pokemon,
                0,
                10,
                "shiny",
                None,
                "",
                None,
                "level",
                &HashMap::new(),
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.pokemon[0].species_id, Some(280));
        assert!(page.pokemon[0].shiny);
        let page = manager
            .mobile_inventory_page(
                "a",
                Kind::Pokemon,
                0,
                10,
                "",
                None,
                "psychic",
                Some(100),
                "quality",
                &HashMap::new(),
                &HashMap::new(),
            )
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.pokemon[0].look_shiny, Some(302));
        assert_eq!(page.pokemon[0].iv_total, Some(120));
        assert!(page.types.contains(&"fairy".to_owned()));
        let json = serde_json::to_value(page).unwrap();
        let serialized = json.to_string();
        for forbidden in ["xp", "heldItemId", "nota", "patchFields"] {
            assert!(
                !serialized.contains(forbidden),
                "unexpected field: {forbidden}"
            );
        }
        assert!(
            manager
                .mobile_inventory_page(
                    "missing",
                    Kind::Items,
                    0,
                    10,
                    "",
                    None,
                    "",
                    None,
                    "level",
                    &HashMap::new(),
                    &HashMap::new()
                )
                .is_none()
        );
    }

    #[test]
    fn mobile_projection_handles_zero_and_four_accounts_without_snapshot_side_effects() {
        let manager = AccountManager::default();
        let empty = manager.mobile_snapshot();
        assert_eq!(empty.schema_version, 2);
        assert_eq!(empty.aggregate.total_accounts, 0);
        assert!(empty.accounts.is_empty());
        assert_eq!(manager.mobile_snapshot().revision, empty.revision + 1);

        for (id, nick, gold) in [
            ("d", "DemoTrainerTwo", 40),
            ("b", "DemoTrainerOne", 20),
            ("c", "DemoTrainerThree", 30),
            ("a", "DemoTrainerFour", 10),
        ] {
            manager
                .add(new_record(id.into(), nick.into(), "#fff".into()))
                .unwrap();
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut(id).unwrap();
            runtime.account.status = if id == "a" {
                ConnectionStatus::Online
            } else {
                ConnectionStatus::Offline
            };
            runtime.account.owner = ConnectionOwner::Transition;
            runtime.state.gold = Some(gold);
            runtime.state.orbs = Some(2);
            runtime.state.automation.ball_ids = vec![5, 4];
            runtime.state.automation.potion_ids = vec![204];
            runtime.browser_transport_ready = id == "a";
        }
        let mut changes = manager.subscribe_mobile_changes();
        changes.borrow_and_update();
        let mut market_signals = manager.subscribe_market_signals();
        let snapshot = manager.mobile_snapshot();
        assert_eq!(snapshot.aggregate.total_accounts, 4);
        assert_eq!(snapshot.aggregate.online_accounts, 1);
        assert_eq!(snapshot.aggregate.gold_total, 100);
        assert_eq!(snapshot.aggregate.orbs_total, 8);
        assert_eq!(
            snapshot
                .accounts
                .iter()
                .map(|account| account.display_name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "DemoTrainerFour",
                "DemoTrainerOne",
                "DemoTrainerThree",
                "DemoTrainerTwo"
            ]
        );
        assert!(
            snapshot
                .accounts
                .iter()
                .all(|account| account.connection_owner
                    == crate::mobile::MobileConnectionOwner::Transition)
        );
        assert!(
            !changes.has_changed().unwrap(),
            "read-only projection emits no state change"
        );
        assert!(
            market_signals.try_recv().is_err(),
            "projection emits no Market signal"
        );
        let runtimes = manager.runtimes.lock();
        for runtime in runtimes.values() {
            assert_eq!(runtime.account.owner, ConnectionOwner::Transition);
            assert_eq!(runtime.state.automation.ball_ids, vec![5, 4]);
            assert_eq!(runtime.state.automation.potion_ids, vec![204]);
            assert!(runtime.background.is_none());
            assert!(runtime.browser_control.is_none());
            assert_eq!(runtime.browser_transport_ready, runtime.account.id == "a");
        }
    }

    #[test]
    fn lifecycle_lock_is_shared_per_account_and_isolated_between_accounts() {
        let manager = AccountManager::default();
        let first = manager.lifecycle_lock("a");
        let same = manager.lifecycle_lock("a");
        let other = manager.lifecycle_lock("b");
        assert!(Arc::ptr_eq(&first, &same));
        assert!(!Arc::ptr_eq(&first, &other));
    }

    #[test]
    fn lifecycle_epoch_cancels_stale_sessions_on_remove_and_shutdown() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        let (epoch, cancellation) = manager.start_lifecycle("a").unwrap();
        assert!(manager.lifecycle_is_current("a", epoch));

        manager.remove("a").unwrap();
        assert!(cancellation.is_cancelled());
        assert!(!manager.lifecycle_is_current("a", epoch));

        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        let (new_epoch, new_cancellation) = manager.start_lifecycle("a").unwrap();
        assert!(new_epoch > epoch);
        assert!(!new_cancellation.is_cancelled());
        assert!(manager.lifecycle_is_current("a", new_epoch));

        manager.stop_all();
        assert!(new_cancellation.is_cancelled());
        assert!(manager.start_lifecycle("a").is_none());
    }

    #[tokio::test]
    async fn replacement_lifecycle_cancels_the_previous_owner_before_lock_wait() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();

        let lock = manager.lifecycle_lock("a");
        let old_guard = lock.clone().lock_owned().await;
        let (old_epoch, old_cancellation) = manager.start_lifecycle("a").unwrap();
        let (new_epoch, new_cancellation) = manager.start_lifecycle("a").unwrap();

        assert!(old_cancellation.is_cancelled());
        assert!(new_epoch > old_epoch);
        assert!(!new_cancellation.is_cancelled());
        assert!(manager.lifecycle_is_current("a", new_epoch));

        drop(old_guard);
        let _new_guard = lock.lock_owned().await;
        assert!(manager.lifecycle_is_current("a", new_epoch));
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn validation_account_shutdown_is_reusable_and_joins_owned_background_tasks() {
        use std::sync::atomic::AtomicUsize;

        let manager = AccountManager::default();
        manager.set_validation_read_only(true);
        manager
            .add(new_record(
                "browser".into(),
                "browser-nick".into(),
                "#fff".into(),
            ))
            .unwrap();
        manager
            .add(new_record(
                "background".into(),
                "background-nick".into(),
                "#fff".into(),
            ))
            .unwrap();
        let (browser_control, mut browser_events) = mpsc::unbounded_channel();
        let (browser_epoch, browser_cancellation) = manager.start_lifecycle("browser").unwrap();
        let (_, background_cancellation) = manager.start_lifecycle("background").unwrap();
        let completed_tasks = Arc::new(AtomicUsize::new(0));
        {
            let mut runtimes = manager.runtimes.lock();
            let browser = runtimes.get_mut("browser").unwrap();
            browser.account.owner = ConnectionOwner::Browser;
            browser.account.mode = AccountMode::Browser;
            browser.account.runtime = AccountRuntimeState::BrowserConnected;
            browser.account.status = ConnectionStatus::Online;
            browser.browser_control = Some(browser_control);
            browser.browser_transport_ready = true;

            let background = runtimes.get_mut("background").unwrap();
            background.account.owner = ConnectionOwner::Background;
            background.account.mode = AccountMode::Background;
            background.account.runtime = AccountRuntimeState::Background;
            background.account.status = ConnectionStatus::Online;
            background.background = Some(crate::connection::BackgroundConnection::test_connection(
                completed_tasks.clone(),
            ));
        }

        manager.shutdown_account_and_wait("browser").await.unwrap();

        assert!(browser_cancellation.is_cancelled());
        assert!(!manager.lifecycle_is_current("browser", browser_epoch));
        assert!(matches!(
            browser_events.try_recv(),
            Ok(BrowserControl::CloseManagedBrowser)
        ));
        let restarted_browser_lifecycle = manager.start_lifecycle("browser");
        assert!(restarted_browser_lifecycle.is_some());

        manager
            .shutdown_account_and_wait("background")
            .await
            .unwrap();
        manager
            .shutdown_account_and_wait("background")
            .await
            .unwrap();
        assert!(background_cancellation.is_cancelled());
        assert_eq!(completed_tasks.load(Ordering::Acquire), 2);
        let runtimes = manager.runtimes.lock();
        for runtime in runtimes.values() {
            assert_eq!(runtime.account.owner, ConnectionOwner::None);
            assert_eq!(runtime.account.runtime, AccountRuntimeState::Offline);
            assert_eq!(runtime.account.status, ConnectionStatus::Offline);
            assert!(runtime.background.is_none());
            assert!(runtime.browser_control.is_none());
            assert!(!runtime.browser_transport_ready);
        }
        drop(runtimes);
        assert!(manager.start_lifecycle("background").is_some());
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn validation_global_shutdown_latches_all_account_lifecycles() {
        let manager = AccountManager::default();
        manager.set_validation_read_only(true);
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        let (_, cancellation) = manager.start_lifecycle("a").unwrap();

        manager.shutdown_all_and_wait().await.unwrap();

        assert!(cancellation.is_cancelled());
        assert!(manager.start_lifecycle("a").is_none());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn validation_dispatch_counters_count_only_mutable_attempts_and_never_sends_them() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        manager.set_validation_read_only(true);
        let (control, mut events) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(control);
            runtime.browser_transport_ready = true;
        }

        manager
            .send_market_command(
                "a",
                ClientFrame::MarketItems {
                    elemento: String::new(),
                    categoria: String::new(),
                },
            )
            .unwrap();
        assert!(matches!(
            manager.set_auto_sale("a", true),
            Err(AccountError::ValidationReadOnly)
        ));
        assert!(matches!(
            manager.send_market_command(
                "a",
                ClientFrame::MarketBuy {
                    id: 10,
                    qtd: 1,
                    preco: 1,
                    moeda: crate::protocol::Currency::Gold,
                },
            ),
            Err(AccountError::ValidationReadOnly)
        ));

        assert_eq!(
            manager.validation_command_counters(),
            ValidationCommandCountersSnapshot {
                mutable_attempts: 2,
                mutable_blocked: 2,
                mutable_sent: 0,
            }
        );
        assert!(matches!(
            events.try_recv(),
            Ok(BrowserControl::SendFrame(_))
        ));
        assert!(
            events.try_recv().is_err(),
            "blocked frames never reach Browser"
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn validation_browser_reload_is_gated_and_uses_control_channel_only() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        let (control, mut events) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(control);
        }

        assert!(matches!(
            manager.request_browser_reload("a"),
            Err(AccountError::ValidationModeRequired)
        ));
        manager.set_validation_read_only(true);
        manager.request_browser_reload("a").unwrap();
        assert!(matches!(events.try_recv(), Ok(BrowserControl::ReloadPage)));

        assert!(matches!(
            manager.request_browser_reload("missing"),
            Err(AccountError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn lifecycle_lock_keeps_a_second_owner_transition_waiting() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick-a".into(), "#fff".into()))
            .unwrap();
        let first_lock = manager.lifecycle_lock("a");
        let held = first_lock.clone().lock_owned().await;
        let waiting_lock = manager.lifecycle_lock("a");
        let (started_tx, mut started_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async move {
            let _guard = waiting_lock.lock_owned().await;
            let _ = started_tx.send(());
        });
        tokio::task::yield_now().await;
        assert!(matches!(
            started_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        drop(held);
        started_rx.await.unwrap();
        waiter.await.unwrap();
    }

    #[test]
    fn authoritative_identity_rejects_a_duplicate_without_rekeying_runtime() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "Alpha".into(), "#fff".into()))
            .unwrap();
        manager
            .add(new_record("b".into(), "Login b".into(), "#fff".into()))
            .unwrap();
        assert!(matches!(
            manager.apply_authoritative_nick("b", "alpha"),
            Err(AccountError::Duplicate(_))
        ));
        let snapshot = manager
            .snapshots()
            .into_iter()
            .find(|snapshot| snapshot.account.id == "b")
            .unwrap();
        assert_eq!(snapshot.account.nick, "Login b");
    }

    #[test]
    fn four_runtime_state_patches_remain_isolated() {
        let manager = AccountManager::default();
        for (id, level, gold) in [
            ("a", 11, 101_u64),
            ("b", 22, 202),
            ("c", 33, 303),
            ("d", 44, 404),
        ] {
            manager
                .add(new_record(id.into(), id.into(), "#fff".into()))
                .unwrap();
            manager
                .ingest(
                    id,
                    ServerFrame::parse(&format!(
                        r#"{{"t":"estado","estado":{{"level":{level},"gold":{gold}}}}}"#
                    ))
                    .unwrap(),
                )
                .unwrap();
        }
        let snapshots = manager.snapshots();
        for (id, level, gold) in [
            ("a", 11, 101_u64),
            ("b", 22, 202),
            ("c", 33, 303),
            ("d", 44, 404),
        ] {
            let snapshot = snapshots
                .iter()
                .find(|entry| entry.account.id == id)
                .unwrap();
            assert_eq!(snapshot.state.level, Some(level));
            assert_eq!(snapshot.state.gold, Some(gold));
        }
    }

    #[test]
    fn reconnecting_one_of_four_accounts_does_not_reset_the_others() {
        let manager = AccountManager::default();
        for id in ["a", "b", "c", "d"] {
            manager
                .add(new_record(id.into(), id.into(), "#fff".into()))
                .unwrap();
        }
        manager
            .websocket_disconnected("c", "teste de isolamento".into())
            .unwrap();
        for snapshot in manager.snapshots() {
            if snapshot.account.id == "c" {
                assert_eq!(snapshot.state.reconnect_attempt, 0);
                assert_eq!(
                    snapshot.state.disconnect_reason.as_deref(),
                    Some("teste de isolamento")
                );
            } else {
                assert!(snapshot.state.disconnect_reason.is_none());
                assert!(snapshot.state.reconnect_started_at_ms.is_none());
            }
        }
    }

    #[test]
    fn browser_owner_loss_is_deduplicated_and_leaves_other_accounts_untouched() {
        let manager = AccountManager::default();
        for id in ["browser", "background-a", "background-b", "background-c"] {
            manager
                .add(new_record(id.into(), id.into(), "#fff".into()))
                .unwrap();
        }
        {
            let mut runtimes = manager.runtimes.lock();
            let browser = runtimes.get_mut("browser").unwrap();
            browser.account.owner = ConnectionOwner::Browser;
            browser.account.mode = AccountMode::Browser;
            browser.account.runtime = AccountRuntimeState::BrowserConnected;
            browser.account.status = ConnectionStatus::Online;
            browser.state.gold = Some(123);
            browser.state.automation.potion_ids = vec![204];
            for id in ["background-a", "background-b", "background-c"] {
                let runtime = runtimes.get_mut(id).unwrap();
                runtime.account.owner = ConnectionOwner::Background;
                runtime.account.mode = AccountMode::Background;
                runtime.account.runtime = AccountRuntimeState::Background;
                runtime.account.status = ConnectionStatus::Online;
            }
        }

        assert!(
            manager
                .begin_browser_recovery("browser", "target destroyed".into())
                .unwrap()
        );
        assert!(
            !manager
                .begin_browser_recovery("browser", "duplicate target event".into())
                .unwrap()
        );

        let snapshots = manager.snapshots();
        let browser = snapshots
            .iter()
            .find(|snapshot| snapshot.account.id == "browser")
            .unwrap();
        assert_eq!(browser.account.owner, ConnectionOwner::Transition);
        assert_eq!(browser.account.mode, AccountMode::Transitioning);
        assert_eq!(browser.account.runtime, AccountRuntimeState::Reconnecting);
        assert_eq!(browser.account.status, ConnectionStatus::Connecting);
        assert_eq!(browser.state.gold, Some(123));
        assert_eq!(browser.state.automation.potion_ids, vec![204]);
        for snapshot in snapshots
            .iter()
            .filter(|snapshot| snapshot.account.id != "browser")
        {
            assert_eq!(snapshot.account.owner, ConnectionOwner::Background);
            assert_eq!(snapshot.account.runtime, AccountRuntimeState::Background);
            assert_eq!(snapshot.account.status, ConnectionStatus::Online);
        }
    }

    #[test]
    fn inspector_is_opt_in_bounded_and_sanitized() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager.record_protocol_frame(
            "a",
            ProtocolDirection::ClientToServer,
            r#"{"token":"must-not-buffer"}"#,
        );
        assert!(manager.protocol_frames("a").unwrap().is_empty());

        manager.set_inspector_enabled(true);
        for index in 0..=inspector::FRAME_CAPACITY {
            manager.record_protocol_frame(
                "a",
                ProtocolDirection::ServerToClient,
                &format!(r#"{{"n":{index},"token":"secret"}}"#),
            );
        }
        let frames = manager.protocol_frames("a").unwrap();
        assert_eq!(frames.len(), inspector::FRAME_CAPACITY);
        assert!(frames.iter().all(|frame| !frame.payload.contains("secret")));
        assert!(frames.first().unwrap().payload.contains(r#""n":1"#));

        manager.set_inspector_enabled(false);
        assert!(manager.protocol_frames("a").unwrap().is_empty());
    }
    #[test]
    fn automation_snapshot_merges_and_does_not_discard_unknown_fields() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "n".into(), "#fff".into()));
        runtime.apply_state(
            serde_json::from_str(r#"{"automation":{"autoPotion":true,"hpLimiar":0.4,"potionIds":[204],"pokemonTravado":[99]}}"#).unwrap(),
            false,
        );
        assert_eq!(runtime.state.automation.auto_potion, Some(true));
        assert_eq!(runtime.state.automation.hp_threshold, Some(0.4));
        assert_eq!(
            runtime.latest_server_automation.as_ref().unwrap()["pokemonTravado"],
            serde_json::json!([99])
        );
        runtime.apply_state(serde_json::from_str(r#"{"gold":12}"#).unwrap(), false);
        assert_eq!(runtime.state.gold, Some(12));
        assert_eq!(
            runtime.latest_server_automation.as_ref().unwrap()["pokemonTravado"],
            serde_json::json!([99])
        );
    }
    #[test]
    fn potion_and_ball_lists_use_full_authoritative_auto_set_snapshots_in_selected_order() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "n".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.latest_server_automation = Some(Map::from_iter([
            ("potionIds".into(), serde_json::json!([204])),
            ("ballIds".into(), serde_json::json!([4])),
            ("pokemonTravado".into(), serde_json::json!([99])),
        ]));
        runtime
            .queue_automation_change("potionIds", serde_json::json!([204, 202]))
            .unwrap();
        let Ok(BrowserControl::SendFrame(ClientFrame::AutoSet { automation })) =
            receiver.try_recv()
        else {
            panic!("expected Potion auto.set");
        };
        assert_eq!(automation["potionIds"], serde_json::json!([204, 202]));
        assert_eq!(automation["pokemonTravado"], serde_json::json!([99]));

        runtime.apply_automation(serde_json::json!({
            "potionIds": [204, 202], "ballIds": [4], "pokemonTravado": [99]
        }));
        runtime
            .queue_automation_change("ballIds", serde_json::json!([4, 3, 2]))
            .unwrap();
        let Ok(BrowserControl::SendFrame(ClientFrame::AutoSet { automation })) =
            receiver.try_recv()
        else {
            panic!("expected Ball auto.set");
        };
        assert_eq!(automation["ballIds"], serde_json::json!([4, 3, 2]));
        assert_eq!(automation["pokemonTravado"], serde_json::json!([99]));
    }
    #[test]
    fn potion_use_is_inferred_only_after_a_reconciled_configured_inventory_decrease() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "n".into(), "#fff".into()));
        runtime.apply_state(
            serde_json::from_str(
                r#"{"items":{"204":5,"203":2},"automation":{"potionIds":[204,203]}}"#,
            )
            .unwrap(),
            true,
        );
        assert!(runtime.metrics.potions().is_empty());
        assert!(runtime.state.active_potion_id.is_none());

        runtime.apply_state(
            serde_json::from_str(r#"{"items":{"204":3,"203":2}}"#).unwrap(),
            false,
        );
        assert_eq!(runtime.metrics.potions().get("204"), Some(&2));
        assert_eq!(runtime.state.active_potion_id, Some(204));
        assert_eq!(
            runtime.state.active_potion_source,
            Some(PotionUsageSource::InventoryDelta)
        );
        runtime.apply_state(
            serde_json::from_str(r#"{"items":{"204":0,"203":1}}"#).unwrap(),
            false,
        );
        // A new observed decrement, not configuration order, moves the active
        // presentation to the Potion the game has now actually consumed.
        assert_eq!(runtime.state.active_potion_id, Some(203));
        assert_eq!(runtime.metrics.potions().get("203"), Some(&1));
    }
    #[test]
    fn potion_inference_ignores_reconnect_resync_and_confirmed_local_purchase() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "n".into(), "#fff".into()));
        runtime.apply_state(
            serde_json::from_str(r#"{"items":{"204":5},"automation":{"potionIds":[204]}}"#)
                .unwrap(),
            true,
        );
        runtime.clear_uncertain_game_state();
        runtime.apply_state(
            serde_json::from_str(r#"{"items":{"204":4}}"#).unwrap(),
            false,
        );
        assert!(runtime.metrics.potions().is_empty());

        runtime.auto_buy_in_flight = Some(AutoBuyInFlight {
            kind: AutoBuyKind::Item,
            item_id: 204,
            sent_at_ms: now_ms(),
            confirmed: false,
        });
        runtime.ingest(
            ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"compra","nome":"Ultimate Potion","qtd":1,"gasto":1}]}"#)
                .unwrap(),
        );
        runtime.apply_state(
            serde_json::from_str(r#"{"items":{"204":3}}"#).unwrap(),
            false,
        );
        assert!(runtime.metrics.potions().is_empty());
    }
    #[test]
    fn individual_pokemon_sale_waits_for_confirmed_sale_event_before_reconciling() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"welcome","estado":{"pokemons":[{"id":7,"nome":"Pupitar","level":60,"hp":1,"maxHp":1}]}}"#).unwrap(),
            )
            .unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(sender);
            runtime.browser_transport_ready = true;
        }
        manager.sell_pokemon("a", 7).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::SellPokemon {
                pokemon_id: 7
            }))
        ));
        assert_eq!(manager.snapshots()[0].state.pokemon.len(), 1);
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"venda","nome":"Pupitar","qtd":1,"ganho":20}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert!(manager.snapshots()[0].state.pokemon.is_empty());
    }
    #[test]
    fn rejects_fifth_account_and_duplicate_nick() {
        let manager = AccountManager::default();
        for i in 0..4 {
            manager
                .add(new_record(i.to_string(), format!("nick{i}"), "#fff".into()))
                .unwrap();
        }
        assert!(matches!(
            manager.add(new_record("fifth".into(), "five".into(), "#fff".into())),
            Err(AccountError::LimitReached)
        ));
        let other = AccountManager::default();
        other
            .add(new_record("a".into(), "Same".into(), "#fff".into()))
            .unwrap();
        assert!(matches!(
            other.add(new_record("b".into(), "same".into(), "#fff".into())),
            Err(AccountError::Duplicate(_))
        ));
    }
    #[test]
    fn background_transition_has_one_explicit_runtime_state() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .transition("a", AccountRuntimeState::PreparingHandoff)
            .unwrap();
        assert_eq!(
            manager.snapshots()[0].account.runtime,
            AccountRuntimeState::PreparingHandoff
        );
        manager
            .transition("a", AccountRuntimeState::LoginRequired)
            .unwrap();
        assert_eq!(
            manager.snapshots()[0].account.status,
            ConnectionStatus::LoginRequired
        );
    }
    #[test]
    fn hunt_session_totals_survive_browser_background_transitions() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
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
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":2,"xpTreinador":25,"xpPokemon":25,"ouro":7}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let expected = manager.snapshots()[0].state.hunt_session.clone().unwrap();

        manager
            .transition("a", AccountRuntimeState::BrowserConnected)
            .unwrap();
        manager
            .transition("a", AccountRuntimeState::Background)
            .unwrap();

        assert_eq!(manager.snapshots()[0].state.hunt_session, Some(expected));
    }
    #[test]
    fn hunt_session_accumulates_events_and_survives_reconnect() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager.ingest("a", ServerFrame::parse(r#"{"t":"welcome","estado":{"huntSlug":"ancient_pupitar","noCentro":false,"activeId":1,"vipAte":20,"servidorAgora":10,"pokemons":[{"id":1,"nome":"Venusaur","level":10,"hp":80,"maxHp":100,"xp":150,"xpNivel":100,"xpProximo":200}]}}"#).unwrap()).unwrap();
        manager.ingest("a", ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":2,"xpTreinador":25,"xpPokemon":25,"ouro":7,"ouroVendaAuto":3,"drops":[{"nome":"Earth Ball","ganho":4}]}]}"#).unwrap()).unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        let session = snapshot.state.hunt_session.unwrap();
        assert_eq!(session.kills, 1);
        assert_eq!(session.xp_obtained, 25);
        assert_eq!(session.trainer_xp, 25);
        assert_eq!(session.pokemon_xp, 25);
        assert_eq!(session.gold_combat + session.gold_auto_sale, 10);
        assert_eq!(session.drops["Earth Ball"], 4);
        assert_eq!(snapshot.state.pokemon[0].xp_level, Some(100));
        assert_eq!(snapshot.state.vip_until, Some(20));

        manager
            .websocket_disconnected("a", "closed".into())
            .unwrap();
        manager.reconnect_attempt("a", 1).unwrap();
        manager.websocket_reconnected("a").unwrap();
        assert_eq!(
            manager.snapshots()[0]
                .state
                .hunt_session
                .as_ref()
                .unwrap()
                .kills,
            1
        );

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"noCentro":true}}"#).unwrap(),
            )
            .unwrap();
        assert_eq!(
            manager.snapshots()[0]
                .state
                .hunt_session
                .as_ref()
                .unwrap()
                .kills,
            1
        );
        assert_eq!(manager.snapshots()[0].state.hunt_started_at_ms, None);
        assert_eq!(
            manager.snapshots()[0].state.activity,
            crate::domain::AccountActivity::PokemonCenter
        );
    }
    #[test]
    fn hunt_session_tracks_balls_and_shinies_and_resets_without_changing_hunt_timer() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
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
        let hunt_started_at_ms = manager.snapshots()[0].state.hunt_started_at_ms;
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":2,"xpTreinador":31,"xpPokemon":27,"ouro":0},{"k":"bola","ballId":4,"slot":2,"sucesso":true,"chance":1.0,"shiny":true},{"k":"fugiu","nome":"Pupitar","level":500,"shiny":true}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let before = manager.snapshots().pop().unwrap();
        let session = before.state.hunt_session.unwrap();
        assert_eq!(session.trainer_xp, 31);
        assert_eq!(session.pokemon_xp, 27);
        assert_eq!(session.balls_used.get("4"), Some(&1));
        assert_eq!(session.shinies_seen, 2);
        assert_eq!(session.shinies_captured, 1);

        manager.reset_hunt_session("a").unwrap();

        let after = manager.snapshots().pop().unwrap();
        let reset = after.state.hunt_session.unwrap();
        assert_eq!(reset.hunt_slug, "ancient_pupitar");
        assert!(reset.started_at_ms >= session.started_at_ms);
        assert_eq!(reset.kills, 0);
        assert_eq!(reset.trainer_xp, 0);
        assert!(reset.balls_used.is_empty());
        assert_eq!(after.state.hunt_started_at_ms, hunt_started_at_ms);
    }

    #[test]
    fn hunt_session_is_preserved_across_center_and_hunt_changes_until_manual_reset() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
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
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":2,"xpTreinador":31,"xpPokemon":27,"ouro":12}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let before_center = manager.snapshots()[0].state.hunt_session.clone().unwrap();
        assert_eq!(before_center.kills, 1);
        assert_eq!(before_center.trainer_xp, 31);

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"noCentro":true}}"#).unwrap(),
            )
            .unwrap();
        let centered_session = manager.snapshots()[0].state.hunt_session.clone().unwrap();
        assert_eq!(centered_session.started_at_ms, before_center.started_at_ms);
        assert_eq!(centered_session.kills, before_center.kills);
        assert_eq!(centered_session.trainer_xp, before_center.trainer_xp);
        assert_eq!(centered_session.gold_combat, before_center.gold_combat);
        assert_eq!(manager.snapshots()[0].state.hunt_started_at_ms, None);

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"noCentro":false}}"#).unwrap(),
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Shellder","slug":"shellder"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();

        let snapshots = manager.snapshots();
        let state = &snapshots[0].state;
        let session = state.hunt_session.as_ref().unwrap();
        assert_eq!(session.hunt_slug, "shellder");
        assert_eq!(session.started_at_ms, before_center.started_at_ms);
        assert_eq!(session.kills, 1);
        assert_eq!(session.trainer_xp, 31);
        assert!(state.hunt_started_at_ms.is_some());
        assert_eq!(
            state.activity,
            crate::domain::AccountActivity::Farming {
                hunt_slug: "shellder".into()
            }
        );

        manager.reset_hunt_session("a").unwrap();
        let reset = manager.snapshots()[0].state.hunt_session.clone().unwrap();
        assert_eq!(reset.hunt_slug, "shellder");
        assert!(reset.started_at_ms >= session.started_at_ms);
        assert_eq!(reset.kills, 0);
        assert_eq!(reset.trainer_xp, 0);
    }

    #[test]
    fn hunt_change_and_center_reset_hunt_metrics_and_timer_but_keep_session_totals() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
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
        let first_timer = manager.snapshots()[0].state.hunt_started_at_ms;
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":2,"xpTreinador":25,"xpPokemon":25,"ouro":7}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(manager.snapshots()[0].metrics.kills, 1);

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Shellder","slug":"shellder"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let switched = manager.snapshots()[0].clone();
        assert_eq!(switched.metrics.kills, 0);
        assert_ne!(switched.state.hunt_started_at_ms, first_timer);
        assert_eq!(switched.state.hunt_session.as_ref().unwrap().kills, 1);

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":3,"xpTreinador":31,"xpPokemon":27,"ouro":12}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(manager.snapshots()[0].metrics.kills, 1);
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"centro","motivo":"volta"}]}"#)
                    .unwrap(),
            )
            .unwrap();
        let centered = manager.snapshots()[0].clone();
        assert!(centered.state.no_centro);
        assert_eq!(centered.metrics.kills, 0);
        assert_eq!(centered.metrics.xp_per_hour, 0);
        assert_eq!(centered.state.hunt_started_at_ms, None);
        assert_eq!(centered.state.hunt_session.as_ref().unwrap().kills, 2);
        assert!(
            manager
                .pending_hunt_timer_changes()
                .iter()
                .any(|change| change.timer.hunt_slug.is_empty())
        );
    }

    #[test]
    fn hunt_session_changes_only_after_server_hunt_confirmation() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager.ingest("a", ServerFrame::parse(r#"{"t":"welcome","estado":{"huntSlug":"ancient_pupitar","noCentro":false},"hunts":[{"slug":"ancient_pupitar","nome":"Ancient Pupitar"},{"slug":"shellder","nome":"Shellder"}]}"#).unwrap()).unwrap();
        let initial_hunt_started_at_ms = manager.snapshots()[0].state.hunt_started_at_ms;
        {
            let mut runtimes = manager.runtimes.lock();
            runtimes.get_mut("a").unwrap().state.pending_hunt_slug = Some("shellder".into());
        }
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"huntSlug":"shellder"}}"#).unwrap(),
            )
            .unwrap();
        manager.ingest("a", ServerFrame::parse(r#"{"t":"campo.init","slug":"shellder","mapa":"shellder","ts":42,"unknownFutureField":true}"#).unwrap()).unwrap();
        let before = manager.snapshots().pop().unwrap();
        assert_eq!(before.state.hunt_slug.as_deref(), Some("ancient_pupitar"));
        assert_eq!(before.state.pending_hunt_slug.as_deref(), Some("shellder"));
        assert_eq!(before.state.hunt_started_at_ms, initial_hunt_started_at_ms);
        let hunt_started_at_ms = before.state.hunt_started_at_ms;

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Shellder","slug":"shellder"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let after = manager.snapshots().pop().unwrap();
        assert_eq!(after.state.hunt_slug.as_deref(), Some("shellder"));
        assert_ne!(after.state.hunt_started_at_ms, hunt_started_at_ms);
        assert!(after.state.pending_hunt_slug.is_none());
        assert_eq!(after.state.hunt_session.unwrap().hunt_slug, "shellder");
    }

    #[test]
    fn restored_hunt_timer_survives_matching_welcome_patches_duplicates_and_reconnect() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .restore_hunt_timer(
                "a",
                HuntTimerState {
                    hunt_slug: "ancient_pupitar".into(),
                    started_at_ms: 1_700_000_000_000,
                    revision: 8,
                },
            )
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
        let original_start = manager.snapshots()[0].state.hunt_started_at_ms;
        assert_eq!(original_start, Some(1_700_000_000_000));
        assert!(manager.pending_hunt_timer_changes().is_empty());

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"gold":15}}"#).unwrap(),
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Ancient Pupitar","slug":"ancient_pupitar"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            manager.snapshots()[0].state.hunt_started_at_ms,
            original_start
        );
        assert!(manager.pending_hunt_timer_changes().is_empty());

        manager
            .websocket_disconnected("a", "test reconnect".into())
            .unwrap();
        manager.reconnect_attempt("a", 1).unwrap();
        manager.websocket_reconnected("a").unwrap();
        assert_eq!(
            manager.snapshots()[0].state.hunt_started_at_ms,
            original_start
        );
        assert!(manager.pending_hunt_timer_changes().is_empty());
    }

    #[test]
    fn restored_hunt_timer_resets_only_when_authoritative_welcome_or_hunt_changes_slug() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .restore_hunt_timer(
                "a",
                HuntTimerState {
                    hunt_slug: "ancient_pupitar".into(),
                    started_at_ms: 1_000,
                    revision: 4,
                },
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"huntSlug":"shellder","noCentro":false}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let changed = manager.pending_hunt_timer_changes();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].timer.hunt_slug, "shellder");
        assert_eq!(changed[0].timer.revision, 5);
        assert_ne!(changed[0].timer.started_at_ms, 1_000);
        let new_start = changed[0].timer.started_at_ms;
        manager.acknowledge_hunt_timer_changes(&changed);

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Shellder","slug":"shellder"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            manager.snapshots()[0].state.hunt_started_at_ms,
            Some(new_start)
        );
        assert!(manager.pending_hunt_timer_changes().is_empty());
    }

    #[test]
    fn returning_to_a_previously_used_hunt_starts_a_new_timer_revision() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        runtime.reconcile_hunt_timer("ancient_pupitar", 1_000);
        runtime.hunt_timer_dirty = false;
        runtime.reconcile_hunt_timer("shellder", 2_000);
        runtime.hunt_timer_dirty = false;
        runtime.reconcile_hunt_timer("ancient_pupitar", 3_000);

        let timer = runtime.hunt_timer.unwrap();
        assert_eq!(timer.hunt_slug, "ancient_pupitar");
        assert_eq!(timer.started_at_ms, 3_000);
        assert_eq!(timer.revision, 3);
        assert!(runtime.hunt_timer_dirty);
    }
    #[test]
    fn hunt_confirmation_leaves_pokemon_center_for_farming() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"welcome","estado":{"noCentro":true}}"#).unwrap(),
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Lapras","slug":"lapras"}]}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        assert!(!snapshot.state.no_centro);
        assert_eq!(
            snapshot.state.activity,
            AccountActivity::Farming {
                hunt_slug: "lapras".into()
            }
        );
    }
    #[test]
    fn estado_patch_reconciles_vip_against_the_latest_server_time() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"vipAte":2000,"servidorAgora":1999}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"servidorAgora":2000}}"#).unwrap(),
            )
            .unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        assert_eq!(snapshot.state.vip_until, Some(2000));
        assert_eq!(snapshot.state.server_now, Some(2000));
    }
    #[test]
    fn welcome_marks_vip_store_data_as_available_without_assuming_its_schema() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"welcome","estado":{"loja":{"vipAte":2000,"vip":true,"futureVipField":true}}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        assert!(snapshot.state.vip_data_available);
        assert_eq!(snapshot.state.vip_active, Some(true));
        assert_eq!(snapshot.state.vip_until, Some(2000));
        assert!(manager.runtimes.lock()["a"].state.vip_store.is_some());
    }

    #[test]
    fn hunt_snapshot_preserves_composition_ivs_and_active_xp_bonuses() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager.ingest("a", ServerFrame::parse(r#"{"t":"welcome","estado":{"level":1153,"activeId":7,"servidorAgora":1000,"guildBonusPct":5,"guild":{"boostAte":2000},"evento":{"xpTreinadorPct":10,"xpPokemonPct":10},"twitch":{"assistindo":true,"pctAtual":25},"pokemons":[{"id":7,"nome":"Tyranitar","level":722,"hp":100,"maxHp":200,"quality":1.2,"potencia":1,"speciesId":248,"tipos":["ROCK","DARK"],"ivs":{"hp":17,"atk":23,"def":20,"spAtk":28,"spDef":32,"speed":32}}]},"hunts":[{"slug":"shedinja","nome":"Shedinja","nivel":500,"especies":[{"pokeId":292,"pontos":15}]}]}"#).unwrap()).unwrap();
        let first = manager.snapshots().pop().unwrap();
        assert_eq!(first.state.xp_bonus.guild_boost_active, Some(true));
        assert_eq!(first.state.xp_bonus.event_trainer_pct, Some(10.0));
        assert_eq!(first.state.xp_bonus.twitch_pct, Some(25.0));
        assert_eq!(first.state.pokemon[0].iv_total, Some(152));
        assert_eq!(first.state.hunts[0].species[0].species_id, Some(292));

        manager
            .ingest(
                "a",
                ServerFrame::parse(
                    r#"{"t":"estado","estado":{"pkMud":[{"id":7,"ivs":{"speed":31}}]}}"#,
                )
                .unwrap(),
            )
            .unwrap();
        let patched = manager.snapshots().pop().unwrap();
        let ivs = patched.state.pokemon[0].ivs.as_ref().unwrap();
        assert_eq!(ivs.hp, Some(17));
        assert_eq!(ivs.atk, Some(23));
        assert_eq!(ivs.speed, Some(31));
        assert_eq!(patched.state.pokemon[0].iv_total, Some(151));

        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"servidorAgora":2001}}"#).unwrap(),
            )
            .unwrap();
        assert_eq!(
            manager.snapshots()[0].state.xp_bonus.guild_boost_active,
            Some(false)
        );
    }
    #[test]
    fn pk_mud_merges_by_id_and_updates_the_active_pokemon_immediately() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"welcome","estado":{"activeId":1,"pokemons":[{"id":1,"nome":"Venusaur","level":400,"hp":9000,"maxHp":10000,"xp":10,"xpNivel":0,"xpProximo":20,"heldItemId":7},{"id":2,"nome":"Tyranitar","level":300,"hp":4000,"maxHp":4000}]}}"#).unwrap(),
            )
            .unwrap();
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"pkMud":[{"id":1,"hp":7040,"maxHp":9924,"xp":15,"quality":0.9,"refino":{"level":2}},{"id":2,"hp":3936}]}}"#).unwrap(),
            )
            .unwrap();
        let snapshot = manager.snapshots().pop().unwrap();
        let active = snapshot
            .state
            .pokemon
            .iter()
            .find(|pokemon| pokemon.id == 1)
            .unwrap();
        assert_eq!(
            (active.hp, active.max_hp, active.xp),
            (7040, 9924, Some(15))
        );
        assert_eq!(active.held_item_id, Some(7));
        assert_eq!(active.quality, Some(0.9));
        assert_eq!(active.patch_fields["refino"]["level"], 2);
        let non_active = snapshot
            .state
            .pokemon
            .iter()
            .find(|pokemon| pokemon.id == 2)
            .unwrap();
        assert_eq!((non_active.hp, non_active.max_hp), (3936, 4000));
    }
    #[test]
    fn combat_lock_uses_server_time_and_replaces_the_previous_deadline() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let server_now = now_ms();
        runtime.apply_state(
            serde_json::from_value(serde_json::json!({
                "servidorAgora": server_now,
                "centroLivreEm": server_now + 3_000,
            }))
            .unwrap(),
            false,
        );
        assert!(runtime.state.combat_locked);
        assert_eq!(runtime.state.combat_lock_until, Some(server_now + 3_000));
        runtime.apply_state(
            serde_json::from_value(serde_json::json!({
                "servidorAgora": server_now + 1_000,
                "centroLivreEm": server_now + 4_000,
            }))
            .unwrap(),
            false,
        );
        assert_eq!(runtime.state.combat_lock_until, Some(server_now + 4_000));
        assert!(runtime.state.combat_locked);
        runtime.apply_state(
            serde_json::from_value(serde_json::json!({ "servidorAgora": server_now + 4_000 }))
                .unwrap(),
            false,
        );
        assert!(!runtime.state.combat_locked);
    }
    #[test]
    fn dispatcher_routes_background_browser_and_transition_without_command_specific_rules() {
        assert_eq!(
            command_transport_for(ConnectionOwner::Background, true, false, false).unwrap(),
            CommandTransport::Background
        );
        assert_eq!(
            command_transport_for(ConnectionOwner::Browser, false, true, true).unwrap(),
            CommandTransport::Browser
        );
        assert!(command_transport_for(ConnectionOwner::Transition, true, true, true).is_err());
    }
    #[test]
    fn validation_read_only_guard_blocks_before_both_transport_dispatchers() {
        let guard = Arc::new(AtomicBool::new(true));
        for owner in [ConnectionOwner::Background, ConnectionOwner::Browser] {
            let mut runtime = AccountRuntime::with_validation_guard(
                new_record("a".into(), "nick".into(), "#fff".into()),
                Arc::clone(&guard),
            );
            runtime.account.owner = owner.clone();
            if owner == ConnectionOwner::Browser {
                let (sender, mut receiver) = mpsc::unbounded_channel();
                runtime.browser_control = Some(sender);
                runtime.browser_transport_ready = true;
                assert!(matches!(
                    AccountCommandDispatcher::dispatch(
                        &runtime,
                        ClientFrame::MarketBuy {
                            id: 42,
                            qtd: 1,
                            preco: 999,
                            moeda: crate::protocol::Currency::Gold,
                        },
                        |_, _| panic!("blocked command reached a transport"),
                    ),
                    Err(AccountError::ValidationReadOnly)
                ));
                assert!(receiver.try_recv().is_err());
            } else {
                let route_reached = std::cell::Cell::new(false);
                let transport_reached = std::cell::Cell::new(false);
                assert!(matches!(
                    AccountCommandDispatcher::dispatch_with_route(
                        &runtime,
                        ClientFrame::MarketBuy {
                            id: 42,
                            qtd: 1,
                            preco: 999,
                            moeda: crate::protocol::Currency::Gold,
                        },
                        |_| {
                            route_reached.set(true);
                            Ok(CommandTransport::Background)
                        },
                        |_, _| {
                            transport_reached.set(true);
                            Ok(())
                        },
                    ),
                    Err(AccountError::ValidationReadOnly)
                ));
                assert!(!route_reached.get());
                assert!(!transport_reached.get());
            }
        }
    }
    #[test]
    fn validation_read_only_allows_market_queries_to_background_and_browser() {
        let guard = Arc::new(AtomicBool::new(true));
        for (owner, expected_transport) in [
            (ConnectionOwner::Background, CommandTransport::Background),
            (ConnectionOwner::Browser, CommandTransport::Browser),
        ] {
            let mut runtime = AccountRuntime::with_validation_guard(
                new_record("a".into(), "nick".into(), "#fff".into()),
                Arc::clone(&guard),
            );
            runtime.account.owner = owner;
            runtime.browser_transport_ready = true;
            let routed = std::cell::Cell::new(None);
            AccountCommandDispatcher::dispatch_with_route(
                &runtime,
                ClientFrame::MarketItems {
                    elemento: String::new(),
                    categoria: String::new(),
                },
                |_| Ok(expected_transport),
                |transport, command| {
                    assert!(matches!(command, ClientFrame::MarketItems { .. }));
                    routed.set(Some(transport));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(routed.get(), Some(expected_transport));
        }
    }
    #[cfg(debug_assertions)]
    #[test]
    fn transport_diagnostics_match_dispatcher_readiness_without_processing_actions() {
        let manager = AccountManager::default();
        for id in ["browser", "browser_pending", "transition"] {
            manager
                .add(new_record(id.into(), id.into(), "#fff".into()))
                .unwrap();
        }
        {
            let mut runtimes = manager.runtimes.lock();
            let (browser_sender, _browser_receiver) = mpsc::unbounded_channel();
            let browser = runtimes.get_mut("browser").unwrap();
            browser.account.owner = ConnectionOwner::Browser;
            browser.browser_control = Some(browser_sender);
            browser.browser_transport_ready = true;
            browser.state.pending_navigation = Some(PendingNavigationIntent::Hunt {
                slug: "ancient_pupitar".into(),
            });

            let (pending_sender, _pending_receiver) = mpsc::unbounded_channel();
            let browser_pending = runtimes.get_mut("browser_pending").unwrap();
            browser_pending.account.owner = ConnectionOwner::Browser;
            browser_pending.browser_control = Some(pending_sender);
            browser_pending.browser_transport_ready = false;

            let (transition_sender, _transition_receiver) = mpsc::unbounded_channel();
            let transition = runtimes.get_mut("transition").unwrap();
            transition.account.owner = ConnectionOwner::Transition;
            transition.browser_control = Some(transition_sender);
            transition.browser_transport_ready = true;
        }

        let diagnostics = manager
            .transport_diagnostics()
            .into_iter()
            .map(|diagnostic| (diagnostic.account_id.clone(), diagnostic))
            .collect::<HashMap<_, _>>();

        let browser = diagnostics.get("browser").unwrap();
        assert_eq!(browser.owner, ConnectionOwner::Browser);
        assert!(browser.browser_control_attached);
        assert!(browser.browser_transport_ready);
        assert!(!browser.background_transport_attached);
        assert_eq!(
            browser.selected_transport,
            Some(AccountDiagnosticTransport::Browser)
        );
        {
            let runtimes = manager.runtimes.lock();
            let runtime = runtimes.get("browser").unwrap();
            assert!(matches!(
                runtime.state.pending_navigation,
                Some(PendingNavigationIntent::Hunt { ref slug }) if slug == "ancient_pupitar"
            ));
        }

        let browser_pending = diagnostics.get("browser_pending").unwrap();
        assert_eq!(browser_pending.owner, ConnectionOwner::Browser);
        assert!(browser_pending.browser_control_attached);
        assert!(!browser_pending.browser_transport_ready);
        assert_eq!(browser_pending.selected_transport, None);

        let transition = diagnostics.get("transition").unwrap();
        assert_eq!(transition.owner, ConnectionOwner::Transition);
        assert!(transition.browser_control_attached);
        assert!(transition.browser_transport_ready);
        assert_eq!(transition.selected_transport, None);
    }

    #[test]
    fn validation_read_only_setting_applies_to_existing_and_future_runtimes() {
        let manager = AccountManager::default();
        manager
            .add(new_record("existing".into(), "one".into(), "#fff".into()))
            .unwrap();
        manager.set_validation_read_only(true);
        manager
            .add(new_record("future".into(), "two".into(), "#fff".into()))
            .unwrap();

        let mut runtimes = manager.runtimes.lock();
        for id in ["existing", "future"] {
            let runtime = runtimes.get_mut(id).unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            let (sender, mut receiver) = mpsc::unbounded_channel();
            runtime.browser_control = Some(sender);
            runtime.browser_transport_ready = true;
            assert!(matches!(
                AccountCommandDispatcher::send(runtime, ClientFrame::AutoSaleLoot { ativo: true },),
                Err(AccountError::ValidationReadOnly)
            ));
            assert!(receiver.try_recv().is_err());
        }
        manager.set_validation_read_only(false);
    }

    #[test]
    fn validation_read_only_suspends_due_actions_before_they_mutate_or_dispatch() {
        let mut runtime = AccountRuntime::with_validation_guard(
            new_record("a".into(), "nick".into(), "#fff".into()),
            Arc::new(AtomicBool::new(true)),
        );
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.pending_navigation = Some(PendingNavigationIntent::Hunt {
            slug: "ancient_pupitar".into(),
        });

        runtime.process_due_actions();

        assert!(receiver.try_recv().is_err());
        assert!(matches!(
            runtime.state.pending_navigation,
            Some(PendingNavigationIntent::Hunt { ref slug }) if slug == "ancient_pupitar"
        ));
    }

    #[test]
    fn browser_owner_dispatches_hunt_and_auto_set_through_the_same_bridge() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.no_centro = true;
        runtime.state.hunts = vec![crate::domain::HuntCatalogEntry {
            slug: "shellder".into(),
            name: "Shellder".into(),
            area: None,
            level: None,
            total_spawns: None,
            region: None,
            looktype: None,
            species: vec![],
        }];
        AccountCommandDispatcher::send(
            &runtime,
            ClientFrame::HuntSelect {
                slug: "shellder".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::HuntSelect { slug })) if slug == "shellder"
        ));
        runtime.latest_server_automation =
            Some(Map::from_iter([("autoPotion".into(), Value::Bool(false))]));
        runtime
            .queue_automation_change("autoPotion", Value::Bool(true))
            .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::AutoSet { automation }))
                if automation.get("autoPotion") == Some(&Value::Bool(true))
        ));
    }
    #[test]
    fn browser_owner_can_select_a_hunt_from_the_center_when_not_combat_locked() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(sender);
            runtime.browser_transport_ready = true;
            runtime.state.no_centro = true;
            runtime.state.hunts = vec![crate::domain::HuntCatalogEntry {
                slug: "shellder".into(),
                name: "Shellder".into(),
                area: None,
                level: None,
                total_spawns: None,
                region: None,
                looktype: None,
                species: vec![],
            }];
        }
        manager.select_hunt("a", "shellder".into()).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::HuntSelect { slug })) if slug == "shellder"
        ));
    }
    #[test]
    fn combat_lock_defers_hunt_without_sending_until_the_deadline() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(sender);
            runtime.browser_transport_ready = true;
            runtime.state.hunts = vec![crate::domain::HuntCatalogEntry {
                slug: "shellder".into(),
                name: "Shellder".into(),
                area: None,
                level: None,
                total_spawns: None,
                region: None,
                looktype: None,
                species: vec![],
            }];
            runtime.state.server_offset_ms = Some(0);
            runtime.state.combat_lock_until = Some(now_ms() + 5_000);
            runtime.state.combat_locked = true;
        }
        manager.select_hunt("a", "shellder".into()).unwrap();
        assert!(receiver.try_recv().is_err());
        let snapshot = manager.snapshots().pop().unwrap();
        assert!(matches!(
            snapshot.state.pending_navigation,
            Some(PendingNavigationIntent::Hunt { slug }) if slug == "shellder"
        ));
    }
    #[test]
    fn pending_navigation_releases_once_after_the_deadline_and_center_waits_for_state() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.account.owner = ConnectionOwner::Browser;
            runtime.browser_control = Some(sender);
            runtime.browser_transport_ready = true;
            runtime.state.server_offset_ms = Some(0);
            runtime.state.combat_lock_until = Some(now_ms() + 60_000);
            runtime.state.combat_locked = true;
            runtime.state.hunts = vec![crate::domain::HuntCatalogEntry {
                slug: "lapras".into(),
                name: "Lapras".into(),
                area: None,
                level: None,
                total_spawns: None,
                region: None,
                looktype: None,
                species: vec![],
            }];
        }
        manager.select_hunt("a", "lapras".into()).unwrap();
        assert!(receiver.try_recv().is_err());
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.state.combat_lock_until = Some(now_ms().saturating_sub(1));
        }
        let _ = manager.snapshots();
        assert!(
            matches!(receiver.try_recv(), Ok(BrowserControl::SendFrame(ClientFrame::HuntSelect { slug })) if slug == "lapras")
        );
        manager.go_center("a").unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::CenterGo))
        ));
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"centro","motivo":"visita"}]}"#)
                    .unwrap(),
            )
            .unwrap();
        assert!(
            manager
                .snapshots()
                .pop()
                .unwrap()
                .state
                .pending_navigation
                .is_some()
        );
        manager
            .ingest(
                "a",
                ServerFrame::parse(r#"{"t":"estado","estado":{"noCentro":true}}"#).unwrap(),
            )
            .unwrap();
        let state = manager.snapshots().pop().unwrap().state;
        assert!(state.pending_navigation.is_none());
        assert!(matches!(state.activity, AccountActivity::PokemonCenter));
    }
    #[test]
    fn automation_timeout_releases_the_next_mutation_and_auto_sale_is_dedicated() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.latest_server_automation = Some(Map::from_iter([
            ("autoPotion".into(), Value::Bool(false)),
            ("autoRevive".into(), Value::Bool(false)),
        ]));
        runtime
            .queue_automation_change("autoPotion", Value::Bool(true))
            .unwrap();
        let _ = receiver.try_recv();
        runtime
            .queue_automation_change("autoRevive", Value::Bool(true))
            .unwrap();
        runtime
            .automation_writes
            .in_flight
            .as_mut()
            .unwrap()
            .sent_at_ms = now_ms().saturating_sub(COMMAND_CONFIRMATION_TIMEOUT_MS);
        runtime.process_due_actions();
        assert!(
            matches!(receiver.try_recv(), Ok(BrowserControl::SendFrame(ClientFrame::AutoSet { automation })) if automation.get("autoRevive") == Some(&Value::Bool(true)))
        );
        AccountCommandDispatcher::send(&runtime, ClientFrame::AutoSaleLoot { ativo: true })
            .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::AutoSaleLoot {
                ativo: true
            }))
        ));
    }
    #[test]
    fn auto_buy_and_capture_worker_use_one_in_flight_operation_per_account() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.server_offset_ms = Some(0);
        runtime.state.balls.insert("4".into(), 1);
        runtime.state.automation.ball_ids = vec![4];
        runtime.state.auto_buy_rules.push(AutoBuyRule {
            kind: AutoBuyKind::Ball,
            item_id: 4,
            minimum: 5,
            quantity: 10,
            enabled: true,
            status: None,
        });
        runtime.process_due_actions();
        assert!(
            matches!(receiver.try_recv(), Ok(BrowserControl::SendFrame(ClientFrame::ShopBuy { kind, id: 4, qty: 10 })) if kind == "ball")
        );
        runtime.ingest(ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"compra","nome":"Ultra Ball","qtd":10,"gasto":1300}]}"#).unwrap());
        runtime
            .ingest(ServerFrame::parse(r#"{"t":"estado","estado":{"balls":{"4":11}}}"#).unwrap());
        assert!(runtime.auto_buy_in_flight.is_none());
        runtime.state.capture_mode = CaptureMode::Continuous;
        runtime.ingest(ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":7,"nome":"Pupitar","xpTreinador":1,"ouro":1}]}"#).unwrap());
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::BallThrow {
                ball_id: 4,
                slot: 7
            }))
        ));
        runtime.ingest(ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"bola","ballId":4,"slot":7,"sucesso":false,"chance":0.01,"shiny":false}]}"#).unwrap());
        assert!(runtime.corpses.is_empty());
        assert_eq!(runtime.metrics.balls().get("4"), Some(&1));
    }
    #[test]
    fn confirmed_capture_turns_one_shot_mode_off_and_marks_it_for_persistence() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        runtime.state.capture_mode = CaptureMode::UntilCapture;

        runtime.resolve_capture_result(7, true);

        assert_eq!(runtime.state.capture_mode, CaptureMode::Off);
        assert!(runtime.capture_mode_dirty);
    }
    #[test]
    fn capture_worker_respects_ball_priority_and_falls_back_only_when_stock_is_empty() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.server_offset_ms = Some(0);
        runtime.state.capture_mode = CaptureMode::Continuous;
        runtime.state.automation.ball_ids = vec![4, 3, 2];
        runtime.state.balls.insert("4".into(), 0);
        runtime.state.balls.insert("3".into(), 2);
        runtime.state.balls.insert("2".into(), 5);
        runtime.enqueue_corpse(&crate::protocol::DeathEvent::wild(1, 1));
        runtime.process_due_actions();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::BallThrow {
                ball_id: 3,
                ..
            }))
        ));
    }
    #[test]
    fn auto_buy_category_toggle_disables_stale_rules_and_stock_edits_cannot_reenable_them() {
        let manager = AccountManager::default();
        manager
            .add(new_record("a".into(), "nick".into(), "#fff".into()))
            .unwrap();
        {
            let mut runtimes = manager.runtimes.lock();
            let runtime = runtimes.get_mut("a").unwrap();
            runtime.state.automation.potion_ids = vec![204];
            runtime.state.auto_buy_rules = vec![
                AutoBuyRule {
                    kind: AutoBuyKind::Item,
                    item_id: 204,
                    minimum: 98,
                    quantity: 100,
                    enabled: true,
                    status: None,
                },
                // This simulates an old Potion that was removed from the
                // current selection but still existed in local preferences.
                AutoBuyRule {
                    kind: AutoBuyKind::Item,
                    item_id: 203,
                    minimum: 500,
                    quantity: 1000,
                    enabled: true,
                    status: None,
                },
            ];
        }

        manager
            .set_auto_buy_enabled("a", AutoBuyKind::Item, false)
            .unwrap();
        manager
            .update_auto_buy_rule("a", AutoBuyKind::Item, 204, 99, 100)
            .unwrap();

        let rules = manager.auto_buy_rules("a").unwrap();
        assert!(
            rules
                .iter()
                .filter(|rule| rule.kind == AutoBuyKind::Item)
                .all(|rule| !rule.enabled)
        );
        assert_eq!(
            rules
                .iter()
                .find(|rule| rule.item_id == 204)
                .unwrap()
                .minimum,
            99
        );

        manager
            .set_auto_buy_enabled("a", AutoBuyKind::Item, true)
            .unwrap();
        let rules = manager.auto_buy_rules("a").unwrap();
        assert!(
            rules
                .iter()
                .find(|rule| rule.item_id == 204)
                .unwrap()
                .enabled
        );
        assert!(
            !rules
                .iter()
                .find(|rule| rule.item_id == 203)
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn active_auto_buy_moves_to_the_authoritatively_selected_potion() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.automation.potion_ids = vec![204];
        runtime.state.items.insert("70070".into(), 0);
        runtime.state.auto_buy_rules.push(AutoBuyRule {
            kind: AutoBuyKind::Item,
            item_id: 204,
            minimum: 10,
            quantity: 100,
            enabled: true,
            status: None,
        });

        runtime.apply_automation(serde_json::json!({ "potionIds": [70070] }));

        let old_rule = runtime
            .state
            .auto_buy_rules
            .iter()
            .find(|rule| rule.item_id == 204)
            .unwrap();
        assert!(!old_rule.enabled);
        let golden_rule = runtime
            .state
            .auto_buy_rules
            .iter()
            .find(|rule| rule.item_id == 70070)
            .unwrap();
        assert_eq!((golden_rule.minimum, golden_rule.quantity), (10, 100));
        assert!(golden_rule.enabled);

        runtime.process_due_actions();
        assert!(matches!(
            receiver.try_recv(),
            Ok(BrowserControl::SendFrame(ClientFrame::ShopBuy { kind, id: 70070, qty: 100 }))
                if kind == "item"
        ));
    }

    #[test]
    fn stale_auto_buy_rule_cannot_purchase_an_unselected_item() {
        let mut runtime = AccountRuntime::new(new_record("a".into(), "nick".into(), "#fff".into()));
        let (sender, mut receiver) = mpsc::unbounded_channel();
        runtime.account.owner = ConnectionOwner::Browser;
        runtime.browser_control = Some(sender);
        runtime.browser_transport_ready = true;
        runtime.state.automation.potion_ids = vec![70070];
        runtime.state.items.insert("204".into(), 0);
        runtime.state.auto_buy_rules.push(AutoBuyRule {
            kind: AutoBuyKind::Item,
            item_id: 204,
            minimum: 10,
            quantity: 100,
            enabled: true,
            status: None,
        });

        runtime.process_due_actions();
        assert!(receiver.try_recv().is_err());
    }
}
