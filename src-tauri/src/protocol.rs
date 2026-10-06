//! Protocol types derived only from documented captures. Unknown input remains non-fatal.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("invalid JSON frame: {0}")]
    Json(#[from] serde_json::Error),
    #[error("frame lacks string field t")]
    MissingType,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Hello {
    pub nick: String,
    #[serde(skip_serializing)]
    pub token: String,
    pub delta: u8,
    pub dispositivo: String,
}
impl std::fmt::Debug for Hello {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hello")
            .field("nick", &self.nick)
            .field("token", &"[REDACTED]")
            .field("delta", &self.delta)
            .field("dispositivo", &"[REDACTED]")
            .finish()
    }
}
impl Hello {
    /// This is the only serialization path for a captured, in-memory hello.
    /// Keeping `token` skipped on the derived serializer prevents accidental
    /// persistence/logging while still allowing the confirmed game handshake.
    pub fn to_wire(&self) -> String {
        serde_json::json!({
            "t": "hello",
            "nick": self.nick,
            "token": self.token,
            "delta": self.delta,
            "dispositivo": self.dispositivo,
        })
        .to_string()
    }
}

#[derive(Debug, Clone)]
pub enum ServerFrame {
    Welcome(WelcomeFrame),
    State(StateFrame),
    FieldInit(FieldInitFrame),
    Battle(BattleFrame),
    Market(Value),
    Friends(Value),
    Profile(Value),
    Unknown { frame_type: String, payload: Value },
}
#[derive(Debug, Clone, Deserialize)]
pub struct WelcomeFrame {
    #[serde(default)]
    pub admin: bool,
    #[serde(rename = "chatMod", default)]
    pub chat_mod: bool,
    #[serde(rename = "chatCmd", default)]
    pub chat_cmd: bool,
    pub estado: RemoteState,
    #[serde(default)]
    pub hunts: Vec<RemoteHunt>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct StateFrame {
    pub estado: RemoteState,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteState {
    pub nick: Option<String>,
    pub level: Option<u32>,
    pub xp: Option<u64>,
    pub gold: Option<u64>,
    pub diamonds: Option<u64>,
    pub orbs: Option<u64>,
    #[serde(rename = "vipAte")]
    pub vip_until: Option<u64>,
    #[serde(rename = "servidorAgora")]
    pub server_now: Option<u64>,
    #[serde(rename = "centroLivreEm")]
    pub center_free_at: Option<u64>,
    /// `welcome.estado.loja` is the authoritative container for the account's
    /// VIP data.  Its internal schema has not yet been captured completely,
    /// so retain it without guessing field names.
    pub loja: Option<Value>,
    #[serde(rename = "evento")]
    pub xp_event: Option<RemoteXpEvent>,
    #[serde(rename = "guildBonusPct")]
    pub guild_bonus_pct: Option<f64>,
    #[serde(rename = "guild")]
    pub guild: Option<RemoteGuildState>,
    #[serde(rename = "twitch")]
    pub twitch: Option<RemoteTwitchState>,
    #[serde(rename = "huntSlug")]
    pub hunt_slug: Option<String>,
    #[serde(rename = "noCentro")]
    pub no_centro: Option<bool>,
    #[serde(rename = "activeId")]
    pub active_id: Option<u64>,
    pub items: Option<std::collections::BTreeMap<String, u64>>,
    pub balls: Option<std::collections::BTreeMap<String, u64>>,
    pub pokemons: Option<Vec<RemotePokemon>>,
    /// Incremental patches for known Pokémon instances. Each entry is keyed
    /// by `id` and may contain only the fields that changed.
    #[serde(rename = "pkMud")]
    pub pokemon_patches: Option<Vec<RemotePokemonPatch>>,
    pub selvagem: Option<RemoteWild>,
    /// Kept raw so an `auto.set` update never drops fields the Manager does
    /// not yet understand. Typed UI fields are derived at the account boundary.
    pub automation: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteXpEvent {
    #[serde(rename = "xpTreinadorPct")]
    pub trainer_pct: Option<f64>,
    #[serde(rename = "xpPokemonPct")]
    pub pokemon_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteGuildState {
    #[serde(rename = "boostAte")]
    pub boost_until: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteTwitchState {
    #[serde(rename = "assistindo")]
    pub watching: Option<bool>,
    #[serde(rename = "pctAtual")]
    pub current_pct: Option<f64>,
    pub pct: Option<f64>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteHunt {
    pub slug: String,
    #[serde(rename = "nome")]
    pub name: String,
    pub area: Option<String>,
    pub nivel: Option<u32>,
    #[serde(default)]
    pub especies: Vec<Value>,
    #[serde(rename = "totalSpawns")]
    pub total_spawns: Option<u32>,
    pub regiao: Option<String>,
    pub pixel: Option<Vec<u32>>,
    pub looktype: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RemotePokemon {
    pub id: u64,
    #[serde(rename = "nome")]
    pub name: String,
    pub level: u32,
    pub hp: u64,
    #[serde(rename = "maxHp")]
    pub max_hp: u64,
    pub xp: Option<u64>,
    #[serde(rename = "xpNivel")]
    pub xp_level: Option<u64>,
    #[serde(rename = "xpProximo")]
    pub xp_next: Option<u64>,
    pub quality: Option<f64>,
    pub potencia: Option<u8>,
    pub nota: Option<f64>,
    pub shiny: Option<bool>,
    #[serde(rename = "speciesId")]
    pub species_id: Option<u64>,
    pub looktype: Option<u64>,
    #[serde(rename = "lookShiny")]
    pub look_shiny: Option<u64>,
    pub tipos: Option<Vec<String>>,
    pub ivs: Option<RemoteIvs>,
    pub poder: Option<u64>,
    #[serde(rename = "heldItemId")]
    pub held_item_id: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RemotePokemonPatch {
    pub id: u64,
    /// Preserve every supplied property, including fields not yet displayed by
    /// the Manager. Known properties are merged at the account boundary.
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteIvs {
    pub hp: Option<u64>,
    pub atk: Option<u64>,
    pub def: Option<u64>,
    #[serde(rename = "spAtk")]
    pub sp_atk: Option<u64>,
    #[serde(rename = "spDef")]
    pub sp_def: Option<u64>,
    pub speed: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteWild {
    pub slot: u64,
    #[serde(rename = "speciesId")]
    pub species_id: Option<u64>,
    #[serde(rename = "nome")]
    pub name: String,
    pub level: u32,
    pub hp: u64,
    #[serde(rename = "maxHp")]
    pub max_hp: u64,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteAutomation {
    #[serde(rename = "autoPotion")]
    pub auto_potion: Option<bool>,
    #[serde(rename = "hpLimiar")]
    pub hp_threshold: Option<f64>,
    #[serde(rename = "autoRevive")]
    pub auto_revive: Option<bool>,
    #[serde(rename = "autoVendaLoot")]
    pub auto_sale_loot: Option<bool>,
    #[serde(rename = "autoLockShiny")]
    pub auto_lock_shiny: Option<bool>,
    #[serde(rename = "autoLockNota9")]
    pub auto_lock_nota9: Option<bool>,
    #[serde(rename = "autoLockNotaMin")]
    pub auto_lock_nota_min: Option<f64>,
    #[serde(rename = "autoLockP5")]
    pub auto_lock_p5: Option<bool>,
    #[serde(rename = "potionIds")]
    pub potion_ids: Option<Vec<u64>>,
    #[serde(rename = "ballIds")]
    pub ball_ids: Option<Vec<u64>>,
    #[serde(rename = "reviveIds")]
    pub revive_ids: Option<Vec<u64>>,
    #[serde(rename = "autoVoltarHunt")]
    pub auto_return_hunt: Option<bool>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct BattleFrame {
    #[serde(rename = "ev")]
    pub events: Vec<BattleEvent>,
}
/// Kept intentionally shallow: only map identity and its timestamp are known
/// to be relevant to the confirmed hunt-selection flow.
#[derive(Debug, Clone, Deserialize)]
pub struct FieldInitFrame {
    pub slug: Option<String>,
    pub mapa: Option<String>,
    pub ts: Option<u64>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "k")]
pub enum BattleEvent {
    #[serde(rename = "ataque")]
    Attack {
        slot: u64,
        #[serde(rename = "por")]
        source: Option<String>,
        #[serde(rename = "hpAlvo")]
        target_hp: u64,
    },
    #[serde(rename = "hunt")]
    HuntSelected { nome: String, slug: String },
    #[serde(rename = "centro")]
    Center { motivo: Option<String> },
    #[serde(rename = "morte")]
    Death(DeathEvent),
    #[serde(rename = "bola")]
    Ball {
        #[serde(rename = "ballId")]
        ball_id: u64,
        slot: u64,
        sucesso: bool,
        chance: f64,
        shiny: bool,
    },
    #[serde(rename = "fugiu")]
    Fled {
        nome: String,
        level: u32,
        shiny: bool,
    },
    #[serde(rename = "marketComprado")]
    MarketPurchased {
        descricao: String,
        total: u64,
        moeda: String,
        sobra: u64,
        trancado: Value,
    },
    #[serde(rename = "aviso")]
    Notice { msg: String },
    #[serde(rename = "amigoPedidoEnviado")]
    FriendRequestSent { nick: String },
    #[serde(rename = "amigoAceito")]
    FriendAccepted { nick: String },
    #[serde(rename = "compra")]
    Purchase { nome: String, qtd: u64, gasto: u64 },
    #[serde(rename = "venda")]
    Sale { nome: String, qtd: u64, ganho: u64 },
    #[serde(other)]
    Unknown,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DeathEvent {
    pub quem: String,
    pub slot: u64,
    #[serde(rename = "nome")]
    pub name: Option<String>,
    #[serde(rename = "speciesId")]
    pub species_id: Option<u64>,
    #[serde(rename = "xpTreinador")]
    pub trainer_xp: u64,
    pub ouro: u64,
    #[serde(rename = "ouroVendaAuto", default)]
    pub auto_sale_gold: u64,
    #[serde(rename = "xpPokemon", default)]
    pub pokemon_xp: u64,
    #[serde(default)]
    pub drops: Vec<DropEvent>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DropEvent {
    pub nome: String,
    #[serde(rename = "qtd", default)]
    pub quantity: u64,
    #[serde(rename = "ganho", default)]
    pub gained: u64,
}
impl DeathEvent {
    pub fn wild(trainer_xp: u64, ouro: u64) -> Self {
        Self {
            quem: "selvagem".to_owned(),
            slot: 1,
            name: None,
            species_id: None,
            trainer_xp,
            ouro,
            auto_sale_gold: 0,
            pokemon_xp: trainer_xp,
            drops: Vec::new(),
        }
    }
}
impl ServerFrame {
    pub fn parse(raw: &str) -> Result<Self, ProtocolError> {
        let payload: Value = serde_json::from_str(raw)?;
        let t = payload
            .get("t")
            .and_then(Value::as_str)
            .ok_or(ProtocolError::MissingType)?;
        let frame = match t {
            "welcome" => Self::Welcome(serde_json::from_value(payload)?),
            "estado" => Self::State(serde_json::from_value(payload)?),
            "campo.init" => Self::FieldInit(serde_json::from_value(payload)?),
            "batalha" => Self::Battle(serde_json::from_value(payload)?),
            "market" => Self::Market(payload),
            "amigos" => Self::Friends(payload),
            "perfil" => Self::Profile(payload),
            other => Self::Unknown {
                frame_type: other.to_owned(),
                payload,
            },
        };
        Ok(frame)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t")]
pub enum ClientFrame {
    #[serde(rename = "hunt.select")]
    HuntSelect { slug: String },
    #[serde(rename = "centro.ir")]
    CenterGo,
    #[serde(rename = "auto.set")]
    AutoSet {
        #[serde(flatten)]
        automation: Map<String, Value>,
    },
    #[serde(rename = "ball.throw")]
    BallThrow {
        #[serde(rename = "ballId")]
        ball_id: u64,
        slot: u64,
    },
    #[serde(rename = "shop.autoVendaLoot")]
    AutoSaleLoot { ativo: bool },
    #[serde(rename = "shop.buy")]
    ShopBuy { kind: String, id: u64, qty: u64 },
    #[serde(rename = "shop.sellPokemon")]
    SellPokemon {
        #[serde(rename = "pokemonId")]
        pokemon_id: u64,
    },
    #[serde(rename = "shop.sellAllPokemons")]
    SellAllPokemons,
    #[serde(rename = "market.itens")]
    MarketItems { elemento: String, categoria: String },
    #[serde(rename = "market.item")]
    MarketItem {
        #[serde(rename = "itemId")]
        item_id: u64,
        moeda: Currency,
    },
    #[serde(rename = "market.historicoGlobal")]
    MarketHistoryGlobal { pagina: u64 },
    #[serde(rename = "market.comprar")]
    MarketBuy {
        id: u64,
        qtd: u64,
        preco: u64,
        moeda: Currency,
    },
    #[serde(rename = "ranking.perfil")]
    RankingProfile { nick: String },
    #[serde(rename = "amigo.pedir")]
    FriendRequest { nick: String },
}
impl ClientFrame {
    /// Validation mode uses a fail-closed allowlist: only frames that query
    /// data are permitted. Any new frame remains blocked until explicitly
    /// classified here.
    #[cfg(debug_assertions)]
    pub fn is_read_only_validation_query(&self) -> bool {
        matches!(
            self,
            Self::MarketItems { .. }
                | Self::MarketItem { .. }
                | Self::MarketHistoryGlobal { .. }
                | Self::RankingProfile { .. }
        )
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Gold,
    Orb,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_frame_is_tolerated() {
        assert!(matches!(
            ServerFrame::parse(r#"{"t":"future.event","x":1}"#).unwrap(),
            ServerFrame::Unknown { .. }
        ));
    }
    #[test]
    fn known_battle_events_and_unknown_event_parse() {
        let frame = ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"morte","quem":"selvagem","slot":3,"xpTreinador":10,"ouro":5},{"k":"new"}]}"#).unwrap();
        let ServerFrame::Battle(frame) = frame else {
            panic!("expected battle")
        };
        assert!(matches!(frame.events[0], BattleEvent::Death(_)));
        assert!(matches!(frame.events[1], BattleEvent::Unknown));
    }
    #[test]
    fn welcome_keeps_orbs_separate_from_diamonds() {
        let frame =
            ServerFrame::parse(r#"{"t":"welcome","estado":{"nick":"ash","diamonds":91,"orbs":7}}"#)
                .unwrap();
        let ServerFrame::Welcome(welcome) = frame else {
            panic!("expected welcome")
        };
        assert_eq!(welcome.estado.orbs, Some(7));
        assert_eq!(welcome.estado.diamonds, Some(91));
    }
    #[test]
    fn hello_never_exposes_token_in_debug() {
        assert!(
            !format!(
                "{:?}",
                Hello {
                    nick: "n".into(),
                    token: "secret".into(),
                    delta: 1,
                    dispositivo: "d".into()
                }
            )
            .contains("secret")
        );
    }
    #[test]
    fn auto_set_serializes_the_complete_preserved_snapshot() {
        let mut automation = Map::new();
        automation.insert("autoPotion".into(), Value::Bool(false));
        automation.insert("pokemonTravado".into(), serde_json::json!([123]));
        automation.insert("bossKills".into(), serde_json::json!({"boss": 4}));
        automation.insert("hpLimiar".into(), serde_json::json!(0.4));
        let wire = serde_json::to_value(ClientFrame::AutoSet { automation }).unwrap();
        assert_eq!(wire["t"], "auto.set");
        assert_eq!(wire["pokemonTravado"], serde_json::json!([123]));
        assert_eq!(wire["bossKills"], serde_json::json!({"boss": 4}));
        assert_eq!(wire["hpLimiar"], serde_json::json!(0.4));
    }
    #[test]
    fn hunt_select_serializes_the_confirmed_wire_shape() {
        let wire = serde_json::to_value(ClientFrame::HuntSelect {
            slug: "shellder".into(),
        })
        .unwrap();
        assert_eq!(
            wire,
            serde_json::json!({"t":"hunt.select","slug":"shellder"})
        );
    }
    #[test]
    fn market_commands_keep_the_confirmed_wire_shapes() {
        assert_eq!(
            serde_json::to_value(ClientFrame::MarketItem {
                item_id: 133,
                moeda: Currency::Gold,
            })
            .unwrap(),
            serde_json::json!({"t":"market.item","itemId":133,"moeda":"gold"})
        );
        assert_eq!(
            serde_json::to_value(ClientFrame::MarketBuy {
                id: 24825,
                qtd: 1,
                preco: 179999,
                moeda: Currency::Gold,
            })
            .unwrap(),
            serde_json::json!({"t":"market.comprar","id":24825,"qtd":1,"preco":179999,"moeda":"gold"})
        );
        assert_eq!(
            serde_json::to_value(ClientFrame::MarketHistoryGlobal { pagina: 0 }).unwrap(),
            serde_json::json!({"t":"market.historicoGlobal","pagina":0})
        );
    }
    #[test]
    fn validation_read_only_allowlist_contains_only_queries() {
        let allowed = [
            ClientFrame::MarketItems {
                elemento: String::new(),
                categoria: String::new(),
            },
            ClientFrame::MarketItem {
                item_id: 1,
                moeda: Currency::Gold,
            },
            ClientFrame::MarketHistoryGlobal { pagina: 0 },
            ClientFrame::RankingProfile {
                nick: "trainer".into(),
            },
        ];
        assert!(
            allowed
                .iter()
                .all(ClientFrame::is_read_only_validation_query)
        );

        let blocked = [
            ClientFrame::HuntSelect {
                slug: "route".into(),
            },
            ClientFrame::CenterGo,
            ClientFrame::AutoSet {
                automation: Map::new(),
            },
            ClientFrame::BallThrow {
                ball_id: 1,
                slot: 1,
            },
            ClientFrame::AutoSaleLoot { ativo: true },
            ClientFrame::ShopBuy {
                kind: "item".into(),
                id: 1,
                qty: 1,
            },
            ClientFrame::SellPokemon { pokemon_id: 1 },
            ClientFrame::SellAllPokemons,
            ClientFrame::MarketBuy {
                id: 1,
                qtd: 1,
                preco: 1,
                moeda: Currency::Gold,
            },
            ClientFrame::FriendRequest {
                nick: "trainer".into(),
            },
        ];
        assert!(
            blocked
                .iter()
                .all(|frame| !frame.is_read_only_validation_query())
        );
    }
    #[test]
    fn center_shop_and_pokemon_sale_frames_use_the_confirmed_wire_shapes() {
        assert_eq!(
            serde_json::to_value(ClientFrame::CenterGo).unwrap(),
            serde_json::json!({"t":"centro.ir"})
        );
        assert_eq!(
            serde_json::to_value(ClientFrame::SellPokemon {
                pokemon_id: 4583985
            })
            .unwrap(),
            serde_json::json!({"t":"shop.sellPokemon","pokemonId":4583985})
        );
        assert_eq!(
            serde_json::to_value(ClientFrame::SellAllPokemons).unwrap(),
            serde_json::json!({"t":"shop.sellAllPokemons"})
        );
        let frame = ServerFrame::parse(r#"{"t":"batalha","ev":[{"k":"centro","motivo":"visita"},{"k":"venda","nome":"Pupitar","qtd":1,"ganho":20}]}"#).unwrap();
        let ServerFrame::Battle(battle) = frame else {
            panic!("expected battle")
        };
        assert!(matches!(battle.events[0], BattleEvent::Center { .. }));
        assert!(matches!(
            battle.events[1],
            BattleEvent::Sale {
                qtd: 1,
                ganho: 20,
                ..
            }
        ));
    }
    #[test]
    fn campo_init_and_hunt_confirmation_parse_tolerantly() {
        let field = ServerFrame::parse(r#"{"t":"campo.init","slug":"shellder","mapa":"shellder","ts":42,"mobs":[],"future":true}"#).unwrap();
        let ServerFrame::FieldInit(field) = field else {
            panic!("expected campo.init")
        };
        assert_eq!(field.slug.as_deref(), Some("shellder"));
        assert_eq!(field.mapa.as_deref(), Some("shellder"));
        assert_eq!(field.ts, Some(42));
        let battle = ServerFrame::parse(
            r#"{"t":"batalha","ev":[{"k":"hunt","nome":"Shellder","slug":"shellder"}]}"#,
        )
        .unwrap();
        let ServerFrame::Battle(battle) = battle else {
            panic!("expected battle")
        };
        assert!(
            matches!(&battle.events[0], BattleEvent::HuntSelected { slug, .. } if slug == "shellder")
        );
    }
    #[test]
    fn welcome_fixture_tolerates_unknown_fields_and_keeps_catalog() {
        let frame = ServerFrame::parse(r#"{"t":"welcome","admin":false,"chatMod":false,"chatCmd":false,"estado":{"gold":10,"orbs":7,"diamonds":2,"vipAte":20,"servidorAgora":10,"activeId":9,"noCentro":false,"loja":{"futureVipField":true},"automation":{"autoPotion":true,"potionIds":[204],"unknownFutureField":true},"items":{"204":3},"balls":{"4":9},"selvagem":{"slot":1,"nome":"Pupitar","level":60,"hp":3,"maxHp":5}},"hunts":[{"slug":"pupitar","nome":"Pupitar","area":"kanto","nivel":60,"especies":[],"totalSpawns":15}]}"#).unwrap();
        let ServerFrame::Welcome(welcome) = frame else {
            panic!("expected welcome")
        };
        assert_eq!(welcome.estado.orbs, Some(7));
        assert_eq!(welcome.estado.active_id, Some(9));
        assert_eq!(welcome.estado.loja.unwrap()["futureVipField"], true);
        assert_eq!(welcome.hunts[0].slug, "pupitar");
        assert_eq!(
            welcome.estado.automation.unwrap()["unknownFutureField"],
            true
        );
    }
}
