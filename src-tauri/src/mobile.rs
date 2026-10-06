use crate::{
    domain::{
        AccountRecord, AccountState, CaptureMode, ConnectionOwner, ConnectionStatus, Pokemon,
    },
    market::{
        MarketCurrency, MarketHistoryEntry, MarketItemMetadata, MarketSummary, MarketTopItemSale,
    },
    metrics::{MetricView, MetricsEngine},
};
use serde::{Deserialize, Serialize};

/// Narrow, versioned mobile read model. Keep this an explicit allowlist rather
/// than serializing domain/runtime structs into a generic JSON value.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileSnapshot {
    pub schema_version: u32,
    /// Monotonic snapshot sequence, not a content-change cursor or HTTP ETag.
    pub revision: u64,
    pub manager_timestamp_ms: u64,
    pub aggregate: MobileAggregate,
    pub accounts: Vec<MobileAccount>,
    pub market: MobileMarket,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileAggregate {
    pub total_accounts: u32,
    pub online_accounts: u32,
    pub xp_per_hour_total: u64,
    pub gold_per_hour_total: u64,
    pub gold_total: u64,
    pub orbs_total: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileAccount {
    pub id: String,
    pub display_name: String,
    /// Deliberately normalized; detailed connection errors are not exposed.
    pub status: MobileStatus,
    /// Ownership is useful to explain Browser/Background mode, but this enum
    /// cannot carry transport handles, URLs, or browser/session details.
    pub connection_owner: MobileConnectionOwner,
    pub level: Option<u32>,
    pub hunt_name: Option<String>,
    /// Elapsed duration at `managerTimestampMs`, used as the base for one UI
    /// ticker. No runtime timer object or task is exposed.
    pub hunt_elapsed_ms: Option<u64>,
    pub active_pokemon: Option<MobilePokemon>,
    pub gold: Option<u64>,
    pub orbs: Option<u64>,
    pub xp_per_hour: u64,
    pub gold_per_hour: u64,
    pub potion: Option<MobileResource>,
    pub ball: Option<MobileResource>,
    pub automations: MobileAutomations,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileAutomations {
    pub auto_potion: bool,
    pub auto_revive: bool,
    pub auto_sale_loot: bool,
    pub auto_return_hunt: bool,
    pub auto_lock_shiny: bool,
    pub auto_lock_nota9: bool,
    pub auto_lock_p5: bool,
    pub capture_mode: CaptureMode,
    pub capture_queue_len: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileMarket {
    pub reader_status: String,
    pub last_updated_at: Option<u64>,
    pub enabled_rule_count: usize,
    pub total_rule_count: usize,
    pub summaries: Vec<MobileMarketSummary>,
    pub top_item_sales: Vec<MobileMarketTopSale>,
    pub recent_transactions: Vec<MobileMarketTransaction>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileMarketSummaryPage {
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
    pub categories: Vec<String>,
    pub items: Vec<MobileMarketSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileMarketSummary {
    pub item_id: u64,
    pub name: String,
    pub category: String,
    pub listings: u64,
    pub units: u64,
    pub min_gold: Option<u64>,
    pub min_orb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileMarketTopSale {
    pub currency_group: MobileMarketRankingGroup,
    pub item_name: String,
    pub quantity: u64,
    pub transactions: u64,
    pub average_unit_price: Option<u64>,
    pub average_gold_unit_price: Option<u64>,
    pub average_orb_unit_price: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileMarketRankingGroup {
    All,
    Gold,
    Gems,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileMarketTransaction {
    pub id: String,
    pub occurred_at: u64,
    pub kind: MobileMarketTransactionKind,
    pub currency: MobileMarketCurrency,
    pub name: String,
    pub looktype: Option<u64>,
    pub look_shiny: Option<u64>,
    pub shiny: bool,
    pub total: u64,
    pub quantity: u64,
    pub unit_price: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileMarketTransactionKind {
    Item,
    Pokemon,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileMarketCurrency {
    Gold,
    Orb,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileInventoryKind {
    Items,
    Pokemon,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileInventoryCategory {
    Potion,
    Ball,
    Stone,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MobileInventoryPage {
    pub account_id: String,
    pub kind: MobileInventoryKind,
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
    pub items: Vec<MobileInventoryItem>,
    pub pokemon: Vec<MobileInventoryPokemon>,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileInventoryItem {
    pub id: String,
    pub name: String,
    pub category: MobileInventoryCategory,
    pub quantity: u64,
    pub asset_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MobileInventoryPokemon {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub shiny: bool,
    pub species_id: Option<u64>,
    pub looktype: Option<u64>,
    pub look_shiny: Option<u64>,
    pub quality: Option<f64>,
    pub note: Option<f64>,
    pub power: Option<u64>,
    pub iv_total: Option<u64>,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileConnectionOwner {
    Browser,
    Background,
    Transition,
    None,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobilePokemon {
    pub name: String,
    pub level: u32,
    pub hp: u64,
    pub max_hp: u64,
    pub shiny: bool,
    pub looktype: Option<u64>,
    pub look_shiny: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MobileResource {
    pub id: u64,
    pub name: String,
    pub quantity: u64,
}

/// Compact values copied while `AccountManager` holds its runtime map lock.
/// Expensive work such as sorting, aggregating, and JSON serialization happens
/// after that lock is released.
pub(crate) struct MobileAccountProjection {
    id: String,
    display_name: String,
    status: MobileStatus,
    connection_owner: MobileConnectionOwner,
    level: Option<u32>,
    hunt_name: Option<String>,
    hunt_elapsed_ms: Option<u64>,
    active_pokemon: Option<MobilePokemon>,
    gold: Option<u64>,
    orbs: Option<u64>,
    xp_per_hour: u64,
    gold_per_hour: u64,
    potion: Option<(u64, u64)>,
    ball: Option<(u64, u64)>,
    automations: MobileAutomations,
}

impl MobileAccountProjection {
    pub(crate) fn into_account(self) -> MobileAccount {
        MobileAccount {
            id: self.id,
            display_name: self.display_name,
            status: self.status,
            connection_owner: self.connection_owner,
            level: self.level,
            hunt_name: self.hunt_name,
            hunt_elapsed_ms: self.hunt_elapsed_ms,
            active_pokemon: self.active_pokemon,
            gold: self.gold,
            orbs: self.orbs,
            xp_per_hour: self.xp_per_hour,
            gold_per_hour: self.gold_per_hour,
            potion: self.potion.map(|(id, quantity)| MobileResource {
                id,
                name: potion_name(id),
                quantity,
            }),
            ball: self.ball.map(|(id, quantity)| MobileResource {
                id,
                name: ball_name(id),
                quantity,
            }),
            automations: self.automations,
        }
    }
}

impl MobileSnapshot {
    pub fn from_accounts(
        revision: u64,
        manager_timestamp_ms: u64,
        mut accounts: Vec<MobileAccount>,
    ) -> Self {
        accounts.sort_by(|left, right| {
            left.display_name
                .to_lowercase()
                .cmp(&right.display_name.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        let aggregate = accounts
            .iter()
            .fold(MobileAggregate::default(), |mut total, account| {
                total.total_accounts = total.total_accounts.saturating_add(1);
                if account.status == MobileStatus::Online {
                    total.online_accounts = total.online_accounts.saturating_add(1);
                }
                total.xp_per_hour_total =
                    total.xp_per_hour_total.saturating_add(account.xp_per_hour);
                total.gold_per_hour_total = total
                    .gold_per_hour_total
                    .saturating_add(account.gold_per_hour);
                total.gold_total = total
                    .gold_total
                    .saturating_add(account.gold.unwrap_or_default());
                total.orbs_total = total
                    .orbs_total
                    .saturating_add(account.orbs.unwrap_or_default());
                total
            });
        Self {
            schema_version: 2,
            revision,
            manager_timestamp_ms,
            aggregate,
            accounts,
            market: MobileMarket::default(),
        }
    }
}

#[cfg(test)]
fn project_account(
    account: &AccountRecord,
    state: &AccountState,
    metrics: &MetricsEngine,
    manager_timestamp_ms: u64,
) -> MobileAccount {
    capture_account_projection(account, state, metrics, manager_timestamp_ms).into_account()
}

pub(crate) fn capture_account_projection(
    account: &AccountRecord,
    state: &AccountState,
    metrics: &MetricsEngine,
    manager_timestamp_ms: u64,
) -> MobileAccountProjection {
    let active_pokemon = state
        .active_id
        .and_then(|active_id| state.pokemon.iter().find(|pokemon| pokemon.id == active_id))
        .map(project_pokemon);
    let hunt_name = state.hunt_slug.as_ref().map(|slug| {
        state
            .hunts
            .iter()
            .find(|hunt| hunt.slug == *slug)
            .map(|hunt| hunt.name.clone())
            .unwrap_or_else(|| slug.clone())
    });
    let hunt_elapsed_ms = state
        .hunt_started_at_ms
        .map(|started_at| manager_timestamp_ms.saturating_sub(started_at));
    let metrics_view: MetricView =
        metrics.view_read_only(!state.no_centro && account.status == ConnectionStatus::Online);

    MobileAccountProjection {
        id: account.id.clone(),
        display_name: account.nick.clone(),
        status: if account.status == ConnectionStatus::Online {
            MobileStatus::Online
        } else {
            MobileStatus::Offline
        },
        connection_owner: project_owner(&account.owner),
        level: state.level,
        hunt_name,
        hunt_elapsed_ms,
        active_pokemon,
        gold: state.gold,
        orbs: state.orbs,
        xp_per_hour: metrics_view.xp_per_hour,
        gold_per_hour: metrics_view.gold_per_hour,
        potion: selected_resource_choice(
            state.active_potion_id,
            &state.automation.potion_ids,
            &state.items,
        ),
        ball: selected_resource_choice(
            state.active_ball_id,
            &state.automation.ball_ids,
            &state.balls,
        ),
        automations: MobileAutomations {
            auto_potion: state.automation.auto_potion.unwrap_or(false),
            auto_revive: state.automation.auto_revive.unwrap_or(false),
            auto_sale_loot: state.automation.auto_sale_loot.unwrap_or(false),
            auto_return_hunt: state.automation.auto_return_hunt.unwrap_or(false),
            auto_lock_shiny: state.automation.auto_lock_shiny.unwrap_or(false),
            auto_lock_nota9: state.automation.auto_lock_nota9.unwrap_or(false),
            auto_lock_p5: state.automation.auto_lock_p5.unwrap_or(false),
            capture_mode: state.capture_mode.clone(),
            capture_queue_len: state.capture_queue_len,
        },
    }
}

pub(crate) fn mobile_market_summary(
    summary: &MarketSummary,
    metadata: Option<&MarketItemMetadata>,
) -> MobileMarketSummary {
    MobileMarketSummary {
        item_id: summary.item_id,
        name: metadata
            .map(|entry| safe_display_text(&entry.name, 80))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Item #{}", summary.item_id)),
        category: normalize_market_category(metadata.and_then(|entry| entry.category.as_deref())),
        listings: summary.listings,
        units: summary.units,
        min_gold: summary.min_gold,
        min_orb: summary.min_orb,
    }
}

pub(crate) fn normalize_market_category(category: Option<&str>) -> String {
    let normalized = category.unwrap_or_default().trim().to_lowercase();
    if normalized.contains("potion") || normalized.contains("poção") || normalized.contains("pocao")
    {
        "potion".into()
    } else if normalized.contains("ball") || normalized.contains("bola") {
        "ball".into()
    } else if normalized.contains("stone") || normalized.contains("pedra") {
        "stone".into()
    } else {
        "other".into()
    }
}

pub(crate) fn mobile_market_top_sale(entry: &MarketTopItemSale) -> MobileMarketTopSale {
    MobileMarketTopSale {
        currency_group: match entry.currency {
            None => MobileMarketRankingGroup::All,
            Some(MarketCurrency::Gold) => MobileMarketRankingGroup::Gold,
            Some(MarketCurrency::Orb) => MobileMarketRankingGroup::Gems,
        },
        item_name: safe_display_text(&entry.item_name, 80),
        quantity: entry.quantity,
        transactions: entry.transactions,
        average_unit_price: entry.average_unit_price,
        average_gold_unit_price: entry.average_gold_unit_price,
        average_orb_unit_price: entry.average_orb_unit_price,
    }
}

pub(crate) fn mobile_market_transaction(entry: &MarketHistoryEntry) -> MobileMarketTransaction {
    let quantity = entry.quantity.unwrap_or(1).max(1);
    let name = entry
        .item_name
        .as_deref()
        .or_else(|| entry.pokemon.as_ref().map(|pokemon| pokemon.name.as_str()))
        .unwrap_or(&entry.description);
    MobileMarketTransaction {
        id: entry.id.to_string(),
        occurred_at: entry.occurred_at,
        kind: match entry.kind.as_str() {
            "item" => MobileMarketTransactionKind::Item,
            "pokemon" => MobileMarketTransactionKind::Pokemon,
            _ => MobileMarketTransactionKind::Unknown,
        },
        currency: match entry.currency {
            MarketCurrency::Gold => MobileMarketCurrency::Gold,
            MarketCurrency::Orb => MobileMarketCurrency::Orb,
        },
        name: safe_display_text(name, 80),
        looktype: entry.pokemon.as_ref().map(|pokemon| pokemon.looktype),
        look_shiny: entry
            .pokemon
            .as_ref()
            .and_then(|pokemon| pokemon.look_shiny),
        shiny: entry.pokemon.as_ref().is_some_and(|pokemon| pokemon.shiny),
        total: entry.total,
        quantity,
        unit_price: (quantity > 0).then_some(entry.total / quantity),
    }
}

fn safe_display_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn project_pokemon(pokemon: &Pokemon) -> MobilePokemon {
    MobilePokemon {
        name: pokemon.name.clone(),
        level: pokemon.level,
        hp: pokemon.hp,
        max_hp: pokemon.max_hp,
        shiny: pokemon.shiny,
        looktype: pokemon.looktype,
        look_shiny: pokemon.look_shiny,
    }
}

fn project_owner(owner: &ConnectionOwner) -> MobileConnectionOwner {
    match owner {
        ConnectionOwner::Browser => MobileConnectionOwner::Browser,
        ConnectionOwner::Background => MobileConnectionOwner::Background,
        ConnectionOwner::Transition => MobileConnectionOwner::Transition,
        ConnectionOwner::None => MobileConnectionOwner::None,
    }
}

fn selected_resource_choice(
    active_id: Option<u64>,
    configured_ids: &[u64],
    inventory: &std::collections::BTreeMap<String, u64>,
) -> Option<(u64, u64)> {
    let id = active_id.or_else(|| {
        configured_ids.iter().copied().find(|id| {
            inventory
                .get(&id.to_string())
                .is_some_and(|quantity| *quantity > 0)
        })
    })?;
    Some((
        id,
        inventory.get(&id.to_string()).copied().unwrap_or_default(),
    ))
}

fn potion_name(id: u64) -> String {
    match id {
        200 => "Small Potion",
        201 => "Great Potion",
        202 => "Ultra Potion",
        203 => "Hyper Potion",
        204 => "Ultimate Potion",
        70070 => "Golden Potion",
        _ => return format!("Potion #{id}"),
    }
    .to_owned()
}

fn ball_name(id: u64) -> String {
    match id {
        1 => "Poké Ball",
        2 => "Great Ball",
        3 => "Super Ball",
        4 => "Ultra Ball",
        5 => "Beast Ball",
        _ => return format!("Ball #{id}"),
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccountActivity, AccountMode};
    use serde_json::Value;

    fn account(id: &str, nick: &str) -> AccountRecord {
        AccountRecord {
            id: id.into(),
            nick: nick.into(),
            local_alias: Some("must-not-serialize".into()),
            card_color: "#fff".into(),
            status: ConnectionStatus::Online,
            mode: AccountMode::Browser,
            runtime: crate::domain::AccountRuntimeState::Background,
            owner: ConnectionOwner::Browser,
        }
    }

    #[test]
    fn empty_mobile_snapshot_has_schema_and_zero_aggregates() {
        let snapshot = MobileSnapshot::from_accounts(1, 123, Vec::new());
        assert_eq!(snapshot.schema_version, 2);
        assert_eq!(snapshot.revision, 1);
        assert_eq!(snapshot.manager_timestamp_ms, 123);
        assert_eq!(snapshot.aggregate, MobileAggregate::default());
        assert!(snapshot.accounts.is_empty());
    }

    #[test]
    fn account_projection_selects_pokemon_hunt_resources_and_elapsed_base() {
        let mut state = AccountState {
            level: Some(88),
            gold: Some(1234),
            orbs: Some(7),
            active_id: Some(42),
            hunt_slug: Some("ancient-pupitar".into()),
            hunt_started_at_ms: Some(900),
            active_potion_id: Some(204),
            active_ball_id: Some(5),
            activity: AccountActivity::Idle,
            ..Default::default()
        };
        state.items.insert("204".into(), 12);
        state.balls.insert("5".into(), 8);
        state.pokemon.push(Pokemon {
            id: 42,
            name: "Venusaur".into(),
            level: 88,
            hp: 80,
            max_hp: 100,
            xp: Some(999),
            xp_level: None,
            xp_next: None,
            quality: None,
            potencia: None,
            poder: None,
            nota: None,
            shiny: false,
            species_id: Some(3),
            looktype: Some(301),
            look_shiny: Some(302),
            held_item_id: None,
            types: vec!["grass".into()],
            iv_total: None,
            ivs: None,
            patch_fields: Default::default(),
        });
        state.hunts.push(crate::domain::HuntCatalogEntry {
            slug: "ancient-pupitar".into(),
            name: "Ancient Pupitar".into(),
            area: Some("Cave".into()),
            level: Some(80),
            total_spawns: Some(3),
            region: None,
            looktype: None,
            species: vec![],
        });

        let view = project_account(
            &account("acct", "DemoTrainerFour"),
            &state,
            &MetricsEngine::default(),
            1_500,
        );
        assert_eq!(view.display_name, "DemoTrainerFour");
        assert_eq!(view.hunt_name.as_deref(), Some("Ancient Pupitar"));
        assert_eq!(view.hunt_elapsed_ms, Some(600));
        assert_eq!(
            view.active_pokemon.as_ref().map(|p| p.name.as_str()),
            Some("Venusaur")
        );
        assert_eq!(
            view.active_pokemon.as_ref().and_then(|p| p.looktype),
            Some(301)
        );
        assert_eq!(
            view.active_pokemon.as_ref().and_then(|p| p.look_shiny),
            Some(302)
        );
        assert_eq!(view.potion.as_ref().map(|r| r.quantity), Some(12));
        assert_eq!(
            view.ball.as_ref().map(|r| r.name.as_str()),
            Some("Beast Ball")
        );
    }

    #[test]
    fn snapshot_sorts_accounts_and_aggregates_only_allowlisted_totals() {
        let mut first = project_account(
            &account("2", "DemoTrainerOne"),
            &AccountState::default(),
            &MetricsEngine::default(),
            100,
        );
        first.gold = Some(200);
        first.orbs = Some(5);
        let mut second = project_account(
            &account("1", "DemoTrainerFour"),
            &AccountState::default(),
            &MetricsEngine::default(),
            100,
        );
        second.status = MobileStatus::Offline;
        second.gold = Some(300);
        second.orbs = Some(7);
        let snapshot = MobileSnapshot::from_accounts(9, 100, vec![first, second]);
        assert_eq!(snapshot.accounts[0].display_name, "DemoTrainerFour");
        assert_eq!(snapshot.accounts[1].display_name, "DemoTrainerOne");
        assert_eq!(snapshot.aggregate.total_accounts, 2);
        assert_eq!(snapshot.aggregate.online_accounts, 1);
        assert_eq!(snapshot.aggregate.gold_total, 500);
        assert_eq!(snapshot.aggregate.orbs_total, 12);
    }

    #[test]
    fn serialized_contract_is_an_explicit_allowlist() {
        let mut record = account("safe-id", "Safe Nick");
        record.local_alias = Some("SECRET_ALIAS".into());
        let state = AccountState {
            automation_error: Some("SECRET_AUTOMATION".into()),
            ..Default::default()
        };
        let view = project_account(&record, &state, &MetricsEngine::default(), 200);
        let value =
            serde_json::to_value(MobileSnapshot::from_accounts(3, 200, vec![view])).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(
            object.keys().cloned().collect::<Vec<_>>(),
            vec![
                "accounts",
                "aggregate",
                "managerTimestampMs",
                "market",
                "revision",
                "schemaVersion"
            ]
        );
        let account = value["accounts"][0].as_object().unwrap();
        assert_eq!(
            account.keys().cloned().collect::<Vec<_>>(),
            vec![
                "activePokemon",
                "automations",
                "ball",
                "connectionOwner",
                "displayName",
                "gold",
                "goldPerHour",
                "huntElapsedMs",
                "huntName",
                "id",
                "level",
                "orbs",
                "potion",
                "status",
                "xpPerHour"
            ]
        );
        assert_eq!(account["connectionOwner"], Value::String("browser".into()));
        let encoded = value.to_string();
        for forbidden in [
            "SECRET_ALIAS",
            "SECRET_AUTOMATION",
            "localAlias",
            "automationError",
            "transport",
            "cookie",
            "token",
            "ownerDetails",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "unexpected field/value in DTO: {forbidden}"
            );
        }
    }
}
