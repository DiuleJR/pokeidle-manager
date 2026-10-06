use crate::mobile_publisher::PublishedMobileSnapshot;
use crate::{
    accounts::AccountManager,
    assets::GameAssetService,
    market::MarketRuntime,
    mobile::{MobileInventoryCategory, MobileInventoryKind},
};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Response, StatusCode, Uri, header},
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
    routing::get,
};
use futures_util::stream;
use serde::Serialize;
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub const MOBILE_LOOPBACK_HOST: &str = "127.0.0.1";
pub const MOBILE_LOOPBACK_PORT: u16 = 1421;
pub const MOBILE_MAX_CLIENTS: usize = 5;
const VITE_DEV_PORT: u16 = 1420;
const SSE_HEARTBEAT: Duration = Duration::from_secs(25);

pub type MobileAuthorizationCheck = Arc<dyn Fn() -> bool + Send + Sync + 'static>;

#[derive(Clone)]
struct MobileApiState {
    snapshots: watch::Receiver<Arc<PublishedMobileSnapshot>>,
    accounts: AccountManager,
    market: MarketRuntime,
    assets: GameAssetService,
    authorized: MobileAuthorizationCheck,
    cancellation: CancellationToken,
    client_slots: Arc<Semaphore>,
    active_clients: Arc<AtomicUsize>,
    allowed_hosts: Arc<[String]>,
}

pub struct MobileServerHandle {
    #[cfg(test)]
    addr: SocketAddr,
    cancellation: CancellationToken,
    task: JoinHandle<()>,
}

impl MobileServerHandle {
    pub async fn start(
        snapshots: watch::Receiver<Arc<PublishedMobileSnapshot>>,
        accounts: AccountManager,
        market: MarketRuntime,
        assets: GameAssetService,
        authorized: MobileAuthorizationCheck,
        cancellation: CancellationToken,
    ) -> io::Result<Self> {
        let host = MOBILE_LOOPBACK_HOST.parse().map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid loopback host: {error}"),
            )
        })?;
        let addr = SocketAddr::new(host, MOBILE_LOOPBACK_PORT);
        Self::start_at(
            addr,
            snapshots,
            accounts,
            market,
            assets,
            authorized,
            cancellation,
        )
        .await
    }

    async fn start_at(
        addr: SocketAddr,
        snapshots: watch::Receiver<Arc<PublishedMobileSnapshot>>,
        accounts: AccountManager,
        market: MarketRuntime,
        assets: GameAssetService,
        authorized: MobileAuthorizationCheck,
        cancellation: CancellationToken,
    ) -> io::Result<Self> {
        if addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "mobile server accepts IPv4 loopback addresses only",
            ));
        }
        let listener = TcpListener::bind(addr).await?;
        let bound_addr = listener.local_addr()?;
        if bound_addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "mobile server did not bind IPv4 loopback",
            ));
        }

        let allowed_hosts: Arc<[String]> = [
            format!("127.0.0.1:{}", bound_addr.port()),
            format!("localhost:{}", bound_addr.port()),
            format!("127.0.0.1:{VITE_DEV_PORT}"),
            format!("localhost:{VITE_DEV_PORT}"),
        ]
        .into();
        let state = MobileApiState {
            snapshots,
            accounts,
            market,
            assets,
            authorized,
            cancellation: cancellation.clone(),
            client_slots: Arc::new(Semaphore::new(MOBILE_MAX_CLIENTS)),
            active_clients: Arc::new(AtomicUsize::new(0)),
            allowed_hosts,
        };
        let app = Router::new()
            .route("/api/v1/mobile/health", get(health))
            .route("/api/v1/mobile/snapshot", get(snapshot))
            .route("/api/v1/mobile/events", get(events))
            .route("/api/v1/mobile/inventory", get(inventory))
            .route("/api/v1/mobile/pokemon-sprite", get(pokemon_sprite))
            .route("/api/v1/mobile/item-assets", get(item_assets))
            .route("/api/v1/mobile/asset", get(asset))
            .route("/api/v1/mobile/market/summaries", get(market_summaries))
            .with_state(state);
        let server_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            tracing::info!(%bound_addr, "local mobile server started");
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(server_cancellation.cancelled_owned())
                .await;
            if let Err(error) = result {
                tracing::warn!(%error, "local mobile server stopped with an error");
            }
            tracing::info!(%bound_addr, "local mobile server stopped");
        });
        Ok(Self {
            #[cfg(test)]
            addr: bound_addr,
            cancellation,
            task,
        })
    }

    #[cfg(test)]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    pub async fn shutdown(self) {
        self.cancellation.cancel();
        if let Err(error) = self.task.await {
            tracing::warn!(%error, "mobile server task did not join cleanly");
        }
    }
}

#[derive(Serialize)]
struct Health {
    ready: bool,
}

async fn health(headers: HeaderMap, State(state): State<MobileApiState>) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    Json(Health { ready: true }).into_response()
}

async fn snapshot(headers: HeaderMap, State(state): State<MobileApiState>) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let published = state.snapshots.borrow().clone();
    json_response(published.json.as_ref())
}

async fn events(headers: HeaderMap, State(state): State<MobileApiState>) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let permit = match state.client_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let connected = state.active_clients.fetch_add(1, Ordering::AcqRel) + 1;
    tracing::debug!(active_clients = connected, "mobile SSE client connected");

    let client_guard = ActiveClientGuard(state.active_clients.clone());
    let stream_state = EventStreamState {
        snapshots: state.snapshots.clone(),
        cancellation: state.cancellation.clone(),
        authorized: state.authorized.clone(),
        _permit: permit,
        _client_guard: client_guard,
        send_initial: true,
        next_heartbeat: tokio::time::Instant::now() + SSE_HEARTBEAT,
    };
    let event_stream = stream::unfold(stream_state, |mut stream_state| async move {
        if stream_state.send_initial {
            stream_state.send_initial = false;
            if !(stream_state.authorized)() {
                return None;
            }
            let published = stream_state.snapshots.borrow().clone();
            return Some((
                Ok::<Event, std::convert::Infallible>(snapshot_event(&published)),
                stream_state,
            ));
        }

        loop {
            tokio::select! {
                _ = stream_state.cancellation.cancelled() => return None,
                _ = tokio::time::sleep_until(stream_state.next_heartbeat) => {
                    if !(stream_state.authorized)() {
                        return None;
                    }
                    stream_state.next_heartbeat = tokio::time::Instant::now() + SSE_HEARTBEAT;
                    return Some((
                        Ok::<Event, std::convert::Infallible>(Event::default().comment("keep-alive")),
                        stream_state,
                    ));
                }
                changed = stream_state.snapshots.changed() => {
                    if changed.is_err() || !(stream_state.authorized)() {
                        return None;
                    }
                    let published = stream_state.snapshots.borrow_and_update().clone();
                    return Some((
                        Ok::<Event, std::convert::Infallible>(snapshot_event(&published)),
                        stream_state,
                    ));
                }
            }
        }
    });
    Sse::new(event_stream).into_response()
}

#[derive(Debug)]
struct InventoryQuery {
    account_id: String,
    kind: MobileInventoryKind,
    offset: usize,
    limit: Option<usize>,
    q: String,
    category: Option<MobileInventoryCategory>,
    pokemon_type: String,
    minimum_iv: Option<u64>,
    order: String,
}

async fn inventory(
    headers: HeaderMap,
    State(state): State<MobileApiState>,
    uri: Uri,
) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let query = match parse_inventory_query(uri.query().unwrap_or_default()) {
        Ok(query) => query,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let limit = query.limit.unwrap_or(50);
    if query.account_id.trim().is_empty()
        || query.account_id.len() > 128
        || !(1..=100).contains(&limit)
        || query.q.chars().count() > 80
        || query.pokemon_type.chars().count() > 24
        || query.minimum_iv.is_some_and(|value| value > 1000)
        || (query.kind == MobileInventoryKind::Pokemon && query.category.is_some())
        || (query.kind == MobileInventoryKind::Items
            && (!query.pokemon_type.is_empty()
                || query.minimum_iv.is_some()
                || query.order != "level"))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let market_names = if query.kind == MobileInventoryKind::Items {
        state.market.mobile_item_names()
    } else {
        HashMap::new()
    };
    let mut catalog = if query.kind == MobileInventoryKind::Items && query.category.is_some() {
        state.assets.item_catalog().await.ok()
    } else {
        None
    };
    let item_categories: HashMap<u64, String> = catalog
        .as_ref()
        .into_iter()
        .flat_map(|catalog| catalog.items.values())
        .filter_map(|item| item.category.clone().map(|category| (item.id, category)))
        .collect();
    let Some(mut page) = state.accounts.mobile_inventory_page(
        &query.account_id,
        query.kind,
        query.offset,
        limit,
        &query.q,
        query.category,
        &query.pokemon_type,
        query.minimum_iv,
        &query.order,
        &market_names,
        &item_categories,
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if query.kind == MobileInventoryKind::Items {
        if !page.items.is_empty() && catalog.is_none() {
            catalog = state.assets.item_catalog().await.ok();
        }
        for item in &mut page.items {
            let Ok(item_id) = item.id.parse::<u64>() else {
                continue;
            };
            item.asset_path = if item.category == MobileInventoryCategory::Ball {
                confirmed_ball_asset_path(item_id).map(str::to_owned)
            } else {
                catalog
                    .as_ref()
                    .and_then(|catalog| catalog.items.get(&item_id.to_string()))
                    .and_then(|item| item.asset_path.clone())
            };
            if item.category != MobileInventoryCategory::Ball {
                if let Some(catalog_item) = catalog
                    .as_ref()
                    .and_then(|catalog| catalog.items.get(&item_id.to_string()))
                {
                    item.name.clone_from(&catalog_item.name);
                    item.category = match catalog_item.category.as_deref() {
                        Some("heal") => MobileInventoryCategory::Potion,
                        Some("stone") => MobileInventoryCategory::Stone,
                        _ => MobileInventoryCategory::Other,
                    };
                }
            }
        }
    }
    match serde_json::to_string(&page) {
        Ok(json) => json_response(&json),
        Err(error) => {
            tracing::warn!(%error, "could not serialize mobile inventory page");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug)]
struct PokemonSpriteQuery {
    looktypes: Vec<u64>,
}

#[derive(Debug)]
struct ItemAssetsQuery {
    ids: Vec<u64>,
    names: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileResolvedItemAsset {
    id: Option<u64>,
    name: Option<String>,
    asset_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileResolvedItemAssets {
    items: Vec<MobileResolvedItemAsset>,
}

async fn pokemon_sprite(
    headers: HeaderMap,
    State(state): State<MobileApiState>,
    uri: Uri,
) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let query = match parse_pokemon_sprite_query(uri.query().unwrap_or_default()) {
        Ok(query) => query,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    match state.assets.pokemon_sprite(query.looktypes).await {
        Ok(Some(sprite)) => match serde_json::to_string(&sprite) {
            Ok(json) => json_response(&json),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, "mobile pokemon sprite metadata unavailable");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn item_assets(
    headers: HeaderMap,
    State(state): State<MobileApiState>,
    uri: Uri,
) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let query = match parse_item_assets_query(uri.query().unwrap_or_default()) {
        Ok(query) => query,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let catalog = match state.assets.item_catalog().await {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::debug!(%error, "mobile item asset catalog unavailable");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let items = query
        .ids
        .into_iter()
        .map(|id| MobileResolvedItemAsset {
            id: Some(id),
            name: catalog
                .items
                .get(&id.to_string())
                .map(|item| item.name.clone()),
            asset_path: catalog
                .items
                .get(&id.to_string())
                .and_then(|item| item.asset_path.clone())
                .or_else(|| confirmed_ball_asset_path(id).map(str::to_owned)),
        })
        .chain(query.names.into_iter().map(|name| {
            let item = catalog
                .items
                .values()
                .find(|item| item.name.eq_ignore_ascii_case(&name));
            MobileResolvedItemAsset {
                id: item.map(|item| item.id),
                name: Some(name),
                asset_path: item.and_then(|item| item.asset_path.clone()),
            }
        }))
        .collect();
    match serde_json::to_string(&MobileResolvedItemAssets { items }) {
        Ok(json) => json_response(&json),
        Err(error) => {
            tracing::warn!(%error, "could not serialize mobile item assets");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn confirmed_ball_asset_path(id: u64) -> Option<&'static str> {
    match id {
        1 => Some("assets/site/assets/ui/ball-poke.png"),
        2 => Some("assets/site/assets/ui/ball-great.png"),
        3 => Some("img/ball-super.png"),
        4 => Some("assets/site/assets/ui/ball-ultra.png"),
        5 => Some("img/ball-beast.png"),
        _ => None,
    }
}

async fn asset(
    headers: HeaderMap,
    State(state): State<MobileApiState>,
    uri: Uri,
) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let path = match parse_asset_query(uri.query().unwrap_or_default()) {
        Ok(path) => path,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let resolved = match state.assets.resolve_asset(path.clone()).await {
        Ok(asset) => asset,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let bytes = match tokio::task::spawn_blocking(move || std::fs::read(resolved.path)).await {
        Ok(Ok(bytes)) => bytes,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let content_type = match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

#[derive(Debug)]
struct MarketSummariesQuery {
    offset: usize,
    limit: Option<usize>,
    q: String,
    category: Option<String>,
    currency: String,
}

async fn market_summaries(
    headers: HeaderMap,
    State(state): State<MobileApiState>,
    uri: Uri,
) -> Response<Body> {
    if !valid_local_request(&headers, &state) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    if !(state.authorized)() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let query = match parse_market_summaries_query(uri.query().unwrap_or_default()) {
        Ok(query) => query,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) || query.q.chars().count() > 80 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(page) = state.market.try_mobile_summaries_page(
        query.offset,
        limit,
        &query.q,
        query.category.as_deref(),
        &query.currency,
    ) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match serde_json::to_string(&page) {
        Ok(json) => json_response(&json),
        Err(error) => {
            tracing::warn!(%error, "could not serialize mobile market summary page");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn parse_market_summaries_query(raw: &str) -> Result<MarketSummariesQuery, ()> {
    let mut offset = 0;
    let mut limit = None;
    let mut q = String::new();
    let mut category = None;
    let mut currency = "all".to_owned();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_query_component(key)?;
        let value = decode_query_component(value)?;
        match key.as_str() {
            "offset" => offset = value.parse().map_err(|_| ())?,
            "limit" => limit = Some(value.parse().map_err(|_| ())?),
            "q" => q = value,
            "category" => {
                let value = value.to_lowercase();
                if !matches!(value.as_str(), "potion" | "ball" | "stone" | "other") {
                    return Err(());
                }
                category = Some(value);
            }
            "currency" => {
                currency = match value.as_str() {
                    "all" | "gold" | "gems" => value,
                    _ => return Err(()),
                };
            }
            _ => return Err(()),
        }
    }
    Ok(MarketSummariesQuery {
        offset,
        limit,
        q,
        category,
        currency,
    })
}

fn parse_inventory_query(raw: &str) -> Result<InventoryQuery, ()> {
    let mut account_id = None;
    let mut kind = None;
    let mut offset = 0;
    let mut limit = None;
    let mut q = String::new();
    let mut category = None;
    let mut pokemon_type = String::new();
    let mut minimum_iv = None;
    let mut order = "level".to_owned();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_query_component(key)?;
        let value = decode_query_component(value)?;
        match key.as_str() {
            "accountId" => account_id = Some(value),
            "kind" => {
                kind = Some(match value.as_str() {
                    "items" => MobileInventoryKind::Items,
                    "pokemon" => MobileInventoryKind::Pokemon,
                    _ => return Err(()),
                });
            }
            "offset" => offset = value.parse().map_err(|_| ())?,
            "limit" => limit = Some(value.parse().map_err(|_| ())?),
            "q" => q = value,
            "category" => {
                category = Some(match value.as_str() {
                    "potion" => MobileInventoryCategory::Potion,
                    "ball" => MobileInventoryCategory::Ball,
                    "stone" => MobileInventoryCategory::Stone,
                    "other" => MobileInventoryCategory::Other,
                    _ => return Err(()),
                });
            }
            "type" => pokemon_type = value,
            "minIv" => minimum_iv = Some(value.parse().map_err(|_| ())?),
            "order" => {
                order = match value.as_str() {
                    "level" | "quality" | "power" | "note" | "iv" | "type" | "recent" => value,
                    _ => return Err(()),
                };
            }
            _ => return Err(()),
        }
    }
    Ok(InventoryQuery {
        account_id: account_id.ok_or(())?,
        kind: kind.ok_or(())?,
        offset,
        limit,
        q,
        category,
        pokemon_type,
        minimum_iv,
        order,
    })
}

fn parse_pokemon_sprite_query(raw: &str) -> Result<PokemonSpriteQuery, ()> {
    let mut looktypes = None;
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if decode_query_component(key)? != "looktypes" || looktypes.is_some() {
            return Err(());
        }
        let value = decode_query_component(value)?;
        let parsed = value
            .split(',')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        if parsed.is_empty() || parsed.len() > 2 || parsed.iter().any(|value| *value == 0) {
            return Err(());
        }
        looktypes = Some(parsed);
    }
    Ok(PokemonSpriteQuery {
        looktypes: looktypes.ok_or(())?,
    })
}

fn parse_item_assets_query(raw: &str) -> Result<ItemAssetsQuery, ()> {
    let mut ids = None;
    let mut names = Vec::new();
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match decode_query_component(key)?.as_str() {
            "ids" if ids.is_none() => {
                let parsed = decode_query_component(value)?
                    .split(',')
                    .map(str::parse::<u64>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ())?;
                if parsed.iter().any(|id| *id == 0) {
                    return Err(());
                }
                ids = Some(parsed);
            }
            "name" => {
                let name = decode_query_component(value)?.trim().to_owned();
                if name.is_empty()
                    || name.chars().count() > 80
                    || name.chars().any(char::is_control)
                {
                    return Err(());
                }
                names.push(name);
            }
            _ => return Err(()),
        }
    }
    let ids = ids.unwrap_or_default();
    if ids.is_empty() && names.is_empty() || ids.len() + names.len() > 100 {
        return Err(());
    }
    Ok(ItemAssetsQuery { ids, names })
}

fn parse_asset_query(raw: &str) -> Result<String, ()> {
    let mut path = None;
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if decode_query_component(key)? != "path" || path.is_some() {
            return Err(());
        }
        path = Some(decode_query_component(value)?);
    }
    path.filter(|path| !path.is_empty() && path.len() <= 512)
        .ok_or(())
}

fn decode_query_component(raw: &str) -> Result<String, ()> {
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16).ok_or(())? as u8;
                let low = (bytes[index + 2] as char).to_digit(16).ok_or(())? as u8;
                decoded.push((high << 4) | low);
                index += 2;
            }
            b'%' => return Err(()),
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8(decoded).map_err(|_| ())
}

fn snapshot_event(published: &PublishedMobileSnapshot) -> Event {
    Event::default()
        .event("snapshot")
        .id(published.snapshot.revision.to_string())
        .data(published.json.as_ref().to_owned())
}

fn json_response(json: &str) -> Response<Body> {
    let mut response = Response::new(Body::from(json.as_bytes().to_vec()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn valid_local_request(headers: &HeaderMap, state: &MobileApiState) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
    else {
        return false;
    };
    if !state.allowed_hosts.iter().any(|allowed| allowed == host) {
        return false;
    }

    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    matches!(
        origin,
        "http://127.0.0.1:1420"
            | "http://localhost:1420"
            | "http://127.0.0.1:1421"
            | "http://localhost:1421"
    )
}

struct EventStreamState {
    snapshots: watch::Receiver<Arc<PublishedMobileSnapshot>>,
    cancellation: CancellationToken,
    authorized: MobileAuthorizationCheck,
    _permit: tokio::sync::OwnedSemaphorePermit,
    _client_guard: ActiveClientGuard,
    send_initial: bool,
    next_heartbeat: tokio::time::Instant,
}

struct ActiveClientGuard(Arc<AtomicUsize>);

impl Drop for ActiveClientGuard {
    fn drop(&mut self) {
        let active = self.0.fetch_sub(1, Ordering::AcqRel).saturating_sub(1);
        tracing::debug!(active_clients = active, "mobile SSE client disconnected");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobile::{MobileAccount, MobileAutomations, MobileSnapshot, MobileStatus};
    use reqwest::header::{CONTENT_TYPE, HOST, ORIGIN};
    use serde_json::Value;

    fn publication(revision: u64) -> Arc<PublishedMobileSnapshot> {
        Arc::new(
            PublishedMobileSnapshot::new(MobileSnapshot::from_accounts(
                revision,
                revision * 100,
                vec![MobileAccount {
                    id: "id-test".into(),
                    display_name: "Conta Teste".into(),
                    status: MobileStatus::Online,
                    connection_owner: crate::mobile::MobileConnectionOwner::Background,
                    level: Some(42),
                    hunt_name: Some("Hunt".into()),
                    hunt_elapsed_ms: Some(1000),
                    active_pokemon: None,
                    gold: Some(500),
                    orbs: Some(2),
                    xp_per_hour: 10,
                    gold_per_hour: 20,
                    potion: None,
                    ball: None,
                    automations: MobileAutomations::default(),
                }],
            ))
            .expect("known mobile DTO serializes"),
        )
    }

    async fn test_server(
        authorized: bool,
    ) -> (
        MobileServerHandle,
        watch::Sender<Arc<PublishedMobileSnapshot>>,
        reqwest::Client,
    ) {
        let (sender, receiver) = watch::channel(publication(1));
        let cancellation = CancellationToken::new();
        let auth: MobileAuthorizationCheck = Arc::new(move || authorized);
        let accounts = AccountManager::default();
        accounts
            .add(crate::accounts::new_record(
                "test-account-id".into(),
                "Conta Teste".into(),
                "#fff".into(),
            ))
            .unwrap();
        let market = MarketRuntime::new(
            AccountManager::default(),
            Arc::new(parking_lot::Mutex::new(
                rusqlite::Connection::open_in_memory().unwrap(),
            )),
        );
        let server = MobileServerHandle::start_at(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            receiver,
            accounts,
            market,
            GameAssetService::new(std::path::Path::new("target/mobile-test-assets"))
                .expect("test asset service starts"),
            auth,
            cancellation,
        )
        .await
        .expect("loopback test server starts");
        (server, sender, reqwest::Client::new())
    }

    fn url(server: &MobileServerHandle, path: &str) -> String {
        format!("http://{}{}", server.local_addr(), path)
    }

    #[test]
    fn inventory_query_decodes_text_and_rejects_unknown_or_malformed_fields() {
        let parsed = parse_inventory_query(
            "accountId=acct%2Fone&kind=items&offset=5&limit=25&q=stone+ball&category=stone",
        )
        .unwrap();
        assert_eq!(parsed.account_id, "acct/one");
        assert_eq!(parsed.offset, 5);
        assert_eq!(parsed.limit, Some(25));
        assert_eq!(parsed.q, "stone ball");
        assert_eq!(parsed.category, Some(MobileInventoryCategory::Stone));
        assert_eq!(parsed.order, "level");
        let pokemon =
            parse_inventory_query("accountId=a&kind=pokemon&type=Fire&minIv=120&order=iv").unwrap();
        assert_eq!(pokemon.pokemon_type, "Fire");
        assert_eq!(pokemon.minimum_iv, Some(120));
        assert_eq!(pokemon.order, "iv");
        assert!(parse_inventory_query("accountId=x&kind=unknown").is_err());
        assert!(parse_inventory_query("accountId=x&kind=items&limit=%GG").is_err());
        assert!(parse_inventory_query("accountId=x&kind=items&surprise=1").is_err());
        assert!(parse_inventory_query("accountId=x&kind=pokemon&order=unknown").is_err());
        let sprite = parse_pokemon_sprite_query("looktypes=302%2C301").unwrap();
        assert_eq!(sprite.looktypes, vec![302, 301]);
        assert!(parse_pokemon_sprite_query("looktypes=0").is_err());
        let item_assets = parse_item_assets_query("ids=4%2C204&name=Earth+Stone").unwrap();
        assert_eq!(item_assets.ids, vec![4, 204]);
        assert_eq!(item_assets.names, vec!["Earth Stone"]);
        assert!(parse_item_assets_query("ids=0").is_err());
        assert!(parse_item_assets_query("name=").is_err());
        assert_eq!(
            parse_asset_query("path=assets%2Fsite%2Fassets%2Fitems%2Fstone.png").unwrap(),
            "assets/site/assets/items/stone.png"
        );
    }

    #[tokio::test]
    async fn loopback_snapshot_health_and_read_only_routes_work() {
        let (server, _, client) = test_server(true).await;
        assert_eq!(server.local_addr().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

        let snapshot = client
            .get(url(&server, "/api/v1/mobile/snapshot"))
            .send()
            .await
            .expect("snapshot request")
            .error_for_status()
            .expect("snapshot 200");
        assert_eq!(
            snapshot.headers().get(CONTENT_TYPE).unwrap(),
            "application/json; charset=utf-8"
        );
        assert_eq!(
            snapshot.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        let value: Value = snapshot.json().await.expect("JSON snapshot");
        assert_eq!(value["revision"], 1);
        assert_eq!(value["accounts"][0]["displayName"], "Conta Teste");

        let health: Value = client
            .get(url(&server, "/api/v1/mobile/health"))
            .send()
            .await
            .expect("health request")
            .json()
            .await
            .expect("health JSON");
        assert_eq!(health, serde_json::json!({ "ready": true }));

        let inventory: Value = client
            .get(url(
                &server,
                "/api/v1/mobile/inventory?accountId=test-account-id&kind=items",
            ))
            .send()
            .await
            .expect("inventory request")
            .error_for_status()
            .expect("inventory 200")
            .json()
            .await
            .expect("inventory JSON");
        assert_eq!(inventory["accountId"], "test-account-id");
        assert_eq!(inventory["kind"], "items");
        assert_eq!(inventory["limit"], 50);
        assert_eq!(inventory["items"], serde_json::json!([]));
        assert_eq!(inventory["pokemon"], serde_json::json!([]));

        let market_page: Value = client
            .get(url(
                &server,
                "/api/v1/mobile/market/summaries?currency=all&limit=20",
            ))
            .send()
            .await
            .expect("market summaries request")
            .error_for_status()
            .expect("market summaries 200")
            .json()
            .await
            .expect("market summaries JSON");
        assert_eq!(market_page["offset"], 0);
        assert_eq!(market_page["limit"], 20);
        assert_eq!(market_page["total"], 0);
        assert_eq!(market_page["items"], serde_json::json!([]));
        assert_eq!(market_page["categories"], serde_json::json!([]));

        for invalid in [
            "/api/v1/mobile/inventory?accountId=test-account-id&kind=items&limit=101",
            "/api/v1/mobile/inventory?accountId=test-account-id&kind=items&limit=0",
            "/api/v1/mobile/inventory?accountId=test-account-id&kind=items&q=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
            "/api/v1/mobile/inventory?accountId=test-account-id&kind=pokemon&category=ball",
            "/api/v1/mobile/market/summaries?limit=101",
            "/api/v1/mobile/market/summaries?limit=0",
            "/api/v1/mobile/market/summaries?currency=diamonds",
            "/api/v1/mobile/market/summaries?category=not-real",
            "/api/v1/mobile/market/summaries?q=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        ] {
            let response = client
                .get(url(&server, invalid))
                .send()
                .await
                .expect("invalid inventory query response");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
        let missing_account = client
            .get(url(
                &server,
                "/api/v1/mobile/inventory?accountId=missing&kind=pokemon",
            ))
            .send()
            .await
            .expect("missing account response");
        assert_eq!(missing_account.status(), StatusCode::NOT_FOUND);

        for method in [
            reqwest::Method::POST,
            reqwest::Method::PUT,
            reqwest::Method::PATCH,
            reqwest::Method::DELETE,
        ] {
            let response = client
                .request(method, url(&server, "/api/v1/mobile/snapshot"))
                .send()
                .await
                .expect("read-only method response");
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
        let response = client
            .post(url(&server, "/api/v1/mobile/market/summaries"))
            .send()
            .await
            .expect("market summaries mutation method response");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let bad_host = client
            .get(url(&server, "/api/v1/mobile/market/summaries"))
            .header(header::HOST, "not-local.test")
            .send()
            .await
            .expect("market summaries host validation");
        assert_eq!(bad_host.status(), StatusCode::MISDIRECTED_REQUEST);
        let unknown = client
            .get(url(&server, "/api/v1/mobile/command"))
            .send()
            .await
            .expect("unknown endpoint response");
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn inventory_route_requires_authorization() {
        let (server, _, client) = test_server(false).await;
        let response = client
            .get(url(
                &server,
                "/api/v1/mobile/inventory?accountId=test-account-id&kind=items",
            ))
            .send()
            .await
            .expect("inventory request");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let market_response = client
            .get(url(&server, "/api/v1/mobile/market/summaries"))
            .send()
            .await
            .expect("market summaries request");
        assert_eq!(market_response.status(), StatusCode::UNAUTHORIZED);
        let sprite_response = client
            .get(url(&server, "/api/v1/mobile/pokemon-sprite?looktypes=301"))
            .send()
            .await
            .expect("pokemon sprite request");
        assert_eq!(sprite_response.status(), StatusCode::UNAUTHORIZED);
        let item_assets_response = client
            .get(url(&server, "/api/v1/mobile/item-assets?ids=4"))
            .send()
            .await
            .expect("item assets request");
        assert_eq!(item_assets_response.status(), StatusCode::UNAUTHORIZED);
        let asset_response = client
            .get(url(
                &server,
                "/api/v1/mobile/asset?path=img%2Fball-super.png",
            ))
            .send()
            .await
            .expect("asset request");
        assert_eq!(asset_response.status(), StatusCode::UNAUTHORIZED);
        server.shutdown().await;
    }

    #[tokio::test]
    async fn sse_is_latest_only_and_releases_the_port_on_shutdown() {
        let (server, sender, client) = test_server(true).await;
        let addr = server.local_addr();
        let mut response = client
            .get(url(&server, "/api/v1/mobile/events"))
            .header(ORIGIN, "http://127.0.0.1:1420")
            .send()
            .await
            .expect("SSE request");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/event-stream"
        );
        let first = response
            .chunk()
            .await
            .expect("first SSE chunk")
            .expect("initial event");
        assert!(String::from_utf8_lossy(&first).contains("\"revision\":1"));

        for revision in 2..=1000 {
            sender.send_replace(publication(revision));
        }
        let chunk = tokio::time::timeout(Duration::from_secs(2), response.chunk())
            .await
            .expect("latest update arrives")
            .expect("SSE remains connected")
            .expect("snapshot update chunk");
        assert!(String::from_utf8_lossy(&chunk).contains("\"revision\":1000"));

        server.shutdown().await;
        drop(response);
        assert!(
            TcpListener::bind(addr).await.is_ok(),
            "shutdown releases listener port"
        );
    }

    #[tokio::test]
    async fn origin_host_and_client_limit_are_enforced() {
        let (server, _, client) = test_server(false).await;
        let unauthorized = client
            .get(url(&server, "/api/v1/mobile/snapshot"))
            .send()
            .await
            .expect("unauthorized response");
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_events = client
            .get(url(&server, "/api/v1/mobile/events"))
            .send()
            .await
            .expect("unauthorized event response");
        assert_eq!(unauthorized_events.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_inventory = client
            .get(url(
                &server,
                "/api/v1/mobile/inventory?accountId=test-account-id&kind=items",
            ))
            .send()
            .await
            .expect("unauthorized inventory response");
        assert_eq!(unauthorized_inventory.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_asset = client
            .get(url(
                &server,
                "/api/v1/mobile/asset?path=img%2Fball-super.png",
            ))
            .send()
            .await
            .expect("unauthorized asset response");
        assert_eq!(unauthorized_asset.status(), StatusCode::UNAUTHORIZED);

        let wrong_origin = client
            .get(url(&server, "/api/v1/mobile/health"))
            .header(ORIGIN, "http://192.0.2.1:1420")
            .send()
            .await
            .expect("origin response");
        assert_eq!(wrong_origin.status(), StatusCode::MISDIRECTED_REQUEST);

        // Verify no arbitrary Host or wildcard-origin path can access the API.
        let wrong_host = client
            .get(url(&server, "/api/v1/mobile/health"))
            .header(HOST, "attacker.example")
            .send()
            .await
            .expect("host response");
        assert_eq!(wrong_host.status(), StatusCode::MISDIRECTED_REQUEST);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn maximum_of_five_sse_clients_and_disconnect_churn_are_bounded() {
        let (server, sender, client) = test_server(true).await;
        let mut clients = Vec::new();
        for _ in 0..MOBILE_MAX_CLIENTS {
            clients.push(
                client
                    .get(url(&server, "/api/v1/mobile/events"))
                    .send()
                    .await
                    .expect("SSE connect"),
            );
        }
        let rejected = client
            .get(url(&server, "/api/v1/mobile/events"))
            .send()
            .await
            .expect("max-client response");
        assert_eq!(rejected.status(), StatusCode::SERVICE_UNAVAILABLE);
        drop(clients);

        // A slow/unread SSE body cannot queue state: watch replaces old values.
        let slow_response = client
            .get(url(&server, "/api/v1/mobile/events"))
            .send()
            .await
            .expect("slow client connect");
        for revision in 2..=1_001 {
            sender.send_replace(publication(revision));
        }
        let latest: Value = client
            .get(url(&server, "/api/v1/mobile/snapshot"))
            .send()
            .await
            .expect("snapshot while SSE reader is slow")
            .json()
            .await
            .expect("latest snapshot JSON");
        assert_eq!(latest["revision"], 1_001);
        drop(slow_response);

        for _ in 0..1_000 {
            let response = client
                .get(url(&server, "/api/v1/mobile/events"))
                .send()
                .await
                .expect("SSE reconnect");
            assert_eq!(response.status(), StatusCode::OK);
            drop(response);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut clients = Vec::new();
        for _ in 0..MOBILE_MAX_CLIENTS {
            clients.push(
                client
                    .get(url(&server, "/api/v1/mobile/events"))
                    .send()
                    .await
                    .expect("server accepts after disconnect churn"),
            );
        }
        assert!(
            clients
                .iter()
                .all(|response| response.status() == StatusCode::OK)
        );
        drop(clients);
        server.shutdown().await;
    }

    #[tokio::test]
    async fn rejects_non_loopback_addresses_before_bind() {
        let (_sender, receiver) = watch::channel(publication(1));
        let cancellation = CancellationToken::new();
        let auth: MobileAuthorizationCheck = Arc::new(|| true);
        let result = MobileServerHandle::start_at(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            receiver,
            AccountManager::default(),
            MarketRuntime::new(
                AccountManager::default(),
                Arc::new(parking_lot::Mutex::new(
                    rusqlite::Connection::open_in_memory().unwrap(),
                )),
            ),
            GameAssetService::new(std::path::Path::new("target/mobile-test-assets"))
                .expect("test asset service starts"),
            auth,
            cancellation,
        )
        .await;
        assert_eq!(
            result.err().unwrap().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
