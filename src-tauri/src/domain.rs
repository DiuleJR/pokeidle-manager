use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const MAX_ACCOUNTS: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Offline,
    Connecting,
    Online,
    LoginRequired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountMode {
    Background,
    Browser,
    Transitioning,
}

/// The only connection allowed to issue game commands for an account.
/// `Transition` deliberately has no sender, preventing duplicate commands while
/// Browser and Rust establish the next owner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionOwner {
    Browser,
    Background,
    Transition,
    #[default]
    None,
}

/// One explicit runtime state replaces combinations of unrelated booleans.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountRuntimeState {
    #[default]
    Offline,
    Starting,
    BrowserBootstrap,
    WaitingForLogin,
    BrowserConnected,
    PreparingHandoff,
    BackgroundConnecting,
    Background,
    Reconnecting,
    RenewingSession,
    LoginRequired,
    Error,
    Stopping,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountActivity {
    Farming {
        hunt_slug: String,
    },
    PokemonCenter,
    Idle,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingNavigationIntent {
    Hunt { slug: String },
    Center,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    UntilCapture,
    Continuous,
    #[default]
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AutoBuyKind {
    Item,
    Ball,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBuyRule {
    pub kind: AutoBuyKind,
    pub item_id: u64,
    pub minimum: u64,
    pub quantity: u64,
    pub enabled: bool,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub id: String,
    pub nick: String,
    pub local_alias: Option<String>,
    pub card_color: String,
    pub status: ConnectionStatus,
    pub mode: AccountMode,
    pub runtime: AccountRuntimeState,
    #[serde(default)]
    pub owner: ConnectionOwner,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountState {
    pub level: Option<u32>,
    pub xp: Option<u64>,
    pub gold: Option<u64>,
    pub diamonds: Option<u64>,
    pub orbs: Option<u64>,
    pub vip_until: Option<u64>,
    pub vip_active: Option<bool>,
    pub server_now: Option<u64>,
    /// True once the authoritative `welcome.estado.loja` VIP container has
    /// arrived.  The raw store remains runtime-only until its schema is
    /// explicitly confirmed.
    pub vip_data_available: bool,
    /// Whitelisted XP modifiers from authoritative game state. Unknown raw
    /// shop/Twitch fields remain private and are never forwarded to the UI.
    #[serde(default)]
    pub xp_bonus: XpBonusState,
    #[serde(default)]
    pub guild_boost_until: Option<u64>,
    #[serde(skip)]
    pub vip_store: Option<Value>,
    pub center_free_at: Option<u64>,
    /// Current combat-movement deadline, in the game's server clock.
    pub combat_lock_until: Option<u64>,
    /// `servidorAgora - localNow`, refreshed whenever the server sends time.
    pub server_offset_ms: Option<i64>,
    pub combat_locked: bool,
    /// Presentation-only capability derived by AccountCommandDispatcher.
    pub command_transport_available: bool,
    pub hunt_slug: Option<String>,
    /// Local wall-clock timestamp for the current confirmed hunt selection.
    /// This timer is independent of transport ownership and volatile hunt metrics.
    pub hunt_started_at_ms: Option<u64>,
    /// Set only after a request is sent; the confirmed hunt remains unchanged
    /// until the server emits `batalha.ev[].k="hunt"`.
    pub pending_hunt_slug: Option<String>,
    /// A single last-write-wins destination. It is retained until the server
    /// authoritatively confirms the navigation or the user cancels it.
    pub pending_navigation: Option<PendingNavigationIntent>,
    pub navigation_error: Option<String>,
    pub no_centro: bool,
    pub active_id: Option<u64>,
    pub items: BTreeMap<String, u64>,
    pub balls: BTreeMap<String, u64>,
    pub active_ball_id: Option<u64>,
    /// No explicit Potion-use battle event has been captured yet. This is only
    /// set after a reconciled inventory decrease for one configured Potion.
    pub active_potion_id: Option<u64>,
    pub active_potion_source: Option<PotionUsageSource>,
    pub pokemon: Vec<Pokemon>,
    pub wild: Option<WildPokemon>,
    pub automation: AutomationState,
    pub automation_error: Option<String>,
    pub auto_buy_rules: Vec<AutoBuyRule>,
    pub capture_mode: CaptureMode,
    pub capture_queue_len: usize,
    pub capture_error: Option<String>,
    pub hunts: Vec<HuntCatalogEntry>,
    pub hunt_session: Option<HuntSession>,
    pub activity: AccountActivity,
    pub disconnect_reason: Option<String>,
    pub reconnect_attempt: u32,
    pub reconnect_started_at_ms: Option<u64>,
    pub reconnect_duration_ms: Option<u64>,
    pub last_reconnected_at_ms: Option<u64>,
}

/// The confidence/source of the last Potion shown by the UI. Keep this
/// separate from server-confirmed ball events until a Potion frame is captured.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PotionUsageSource {
    InventoryDelta,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutomationState {
    pub auto_potion: Option<bool>,
    pub hp_threshold: Option<f64>,
    pub auto_revive: Option<bool>,
    pub auto_sale_loot: Option<bool>,
    pub auto_lock_shiny: Option<bool>,
    pub auto_lock_nota9: Option<bool>,
    pub auto_lock_nota_min: Option<f64>,
    pub auto_lock_p5: Option<bool>,
    pub potion_ids: Vec<u64>,
    pub ball_ids: Vec<u64>,
    pub revive_ids: Vec<u64>,
    pub auto_return_hunt: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuntCatalogEntry {
    pub slug: String,
    pub name: String,
    pub area: Option<String>,
    pub level: Option<u32>,
    pub total_spawns: Option<u32>,
    pub region: Option<String>,
    pub looktype: Option<u64>,
    #[serde(default)]
    pub species: Vec<HuntSpeciesEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HuntSpeciesEntry {
    pub species_id: Option<u64>,
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PokemonIvs {
    pub hp: Option<u64>,
    pub atk: Option<u64>,
    pub def: Option<u64>,
    pub sp_atk: Option<u64>,
    pub sp_def: Option<u64>,
    pub speed: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XpBonusState {
    pub event_trainer_pct: Option<f64>,
    pub event_pokemon_pct: Option<f64>,
    pub guild_rank_pct: Option<f64>,
    pub guild_boost_active: Option<bool>,
    pub twitch_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pokemon {
    pub id: u64,
    #[serde(rename = "nome")]
    pub name: String,
    pub level: u32,
    pub hp: u64,
    #[serde(rename = "maxHp")]
    pub max_hp: u64,
    pub xp: Option<u64>,
    pub xp_level: Option<u64>,
    pub xp_next: Option<u64>,
    pub quality: Option<f64>,
    pub potencia: Option<u8>,
    pub poder: Option<u64>,
    pub nota: Option<f64>,
    pub shiny: bool,
    pub species_id: Option<u64>,
    pub looktype: Option<u64>,
    pub look_shiny: Option<u64>,
    pub held_item_id: Option<u64>,
    pub types: Vec<String>,
    pub iv_total: Option<u64>,
    #[serde(default)]
    pub ivs: Option<PokemonIvs>,
    /// Runtime-only preservation for partial `pkMud` fields whose UI meaning
    /// has not yet been defined. It is neither persisted nor exposed to React.
    #[serde(skip)]
    pub patch_fields: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WildPokemon {
    pub slot: u64,
    pub species_id: Option<u64>,
    #[serde(rename = "nome")]
    pub name: String,
    pub level: u32,
    pub hp: u64,
    #[serde(rename = "maxHp")]
    pub max_hp: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountMetrics {
    pub xp_per_hour: u64,
    pub gold_per_hour: u64,
    pub kills: u64,
    pub captures: u64,
    pub potions_used: BTreeMap<String, u64>,
    pub potions_per_hour: Option<u64>,
    pub potion_usage_per_hour: BTreeMap<String, u64>,
    pub balls_used: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HuntSession {
    pub hunt_slug: String,
    pub started_at_ms: u64,
    pub kills: u64,
    pub captures: u64,
    pub xp_obtained: u64,
    pub gold_combat: u64,
    pub gold_auto_sale: u64,
    pub drops: BTreeMap<String, u64>,
}

/// Persisted identity and start time for one account's current hunt selection.
/// `revision` prevents delayed writes from replacing a newer A → B → A session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HuntTimerState {
    pub hunt_slug: String,
    pub started_at_ms: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSnapshot {
    pub account: AccountRecord,
    pub state: AccountState,
    pub metrics: AccountMetrics,
}
