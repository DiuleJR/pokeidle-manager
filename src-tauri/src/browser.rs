//! Visible Brave + CDP observation plus a narrowly scoped bridge for the single
//! game-owned WebSocket. The bridge never creates a socket or retains secrets.
use crate::{
    accounts::{AccountManager, BrowserControl},
    connection::{ConnectionBootstrap, ConnectionManager, SessionHeaders},
    domain::AccountRuntimeState,
    inspector::ProtocolDirection,
    logging::sanitize_frame,
    protocol::{Hello, ServerFrame},
};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Read,
    net::TcpListener,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio_tungstenite::{WebSocketStream, connect_async, tungstenite::Message};

struct CdpSocket {
    stream: WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    pending_events: VecDeque<Value>,
}
impl CdpSocket {
    async fn next(&mut self) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
        if let Some(event) = self.pending_events.pop_front() {
            return Some(Ok(Message::Text(event.to_string().into())));
        }
        self.stream.next().await
    }
}
const BROWSER_RECONNECT_GRACE: Duration = Duration::from_secs(8);

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error(
        "BROWSER_NOT_FOUND: Brave não encontrado. Instale o Brave pelo site oficial e tente novamente."
    )]
    NotFound,
    #[error(
        "BROWSER_INVALID_INSTALLATION: a instalação encontrada não contém um executável Brave válido."
    )]
    InvalidInstallation(PathBuf),
    #[error("não foi possível iniciar a operação do navegador: {0}")]
    Launch(String),
    #[error("BROWSER_FAILED_TO_START: não foi possível iniciar o Brave: {0}")]
    BrowserFailedToStart(String),
    #[error("CDP_STARTUP_FAILED: falha ao acessar http://127.0.0.1:{port}/json/version: {details}")]
    CdpEndpoint { port: u16, details: String },
    #[error("CDP_STARTUP_FAILED: não foi possível configurar o cliente HTTP local do CDP: {0}")]
    CdpClient(String),
    #[error("não há uma aba do jogo disponível para inspeção")]
    GameTabUnavailable,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserSessionMode {
    Interactive,
    /// A visible page that remains the connection owner until the user asks to
    /// return it to Rust Background.
    InteractiveOwner,
    /// Reattaches to the existing visible Brave/CDP instance for an errored
    /// Browser account. This mode must never launch a replacement process.
    ReconnectExistingOwner,
    BackgroundBootstrap,
    /// Identifies a copied profile through the normal Browser/CDP observer,
    /// without ingesting frames into account state or starting Background.
    RecoveryProbe,
}

pub fn is_interactive_owner_mode(mode: BrowserSessionMode) -> bool {
    matches!(
        mode,
        BrowserSessionMode::InteractiveOwner | BrowserSessionMode::ReconnectExistingOwner
    )
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserLifecycle {
    Starting,
    ConnectingCdp,
    LoadingGame,
    WaitingForLogin,
    WaitingForHello,
    WaitingForWelcome,
    Authenticated,
    ReadyForHandoff,
    Closing,
    #[default]
    Closed,
    Error,
}

/// Explicit snapshot reason when a requested reload is refused to preserve
/// the single game-WebSocket owner invariant. This is intentionally separate
/// from the free-form user-facing diagnostic message so operational tooling can
/// classify a guarded action without treating it as a connection failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReloadGuardReason {
    ActiveGameSocket,
    ReconnectGrace,
    UnobservedTarget,
}

/// Outcome for the validation-only observed reload path. Kept separate from
/// `ReloadGuardReason`: the generic reload guard remains unchanged.
#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlledReloadStatus {
    #[default]
    NotRequested,
    PreconditionsBlocked,
    Reloading,
    Pass,
    DualSocket,
    Blocked,
}

impl BrowserLifecycle {
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "Iniciando",
            Self::ConnectingCdp => "Conectando CDP",
            Self::LoadingGame => "Carregando jogo",
            Self::WaitingForLogin => "Aguardando login",
            Self::WaitingForHello => "Aguardando hello",
            Self::WaitingForWelcome => "Aguardando welcome",
            Self::Authenticated => "Autenticada",
            Self::ReadyForHandoff => "Pronta para handoff",
            Self::Closing => "Fechando",
            Self::Closed => "Encerrada",
            Self::Error => "Erro",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrowserProfile {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDiagnostic {
    pub lifecycle: BrowserLifecycle,
    pub session_mode: Option<BrowserSessionMode>,
    pub brave_found: bool,
    pub profile_created: bool,
    pub account_persisted: bool,
    pub persistent_profile: bool,
    pub brave_started: bool,
    pub cdp_port: Option<u16>,
    pub cdp_endpoint_available: bool,
    pub cdp_endpoint_attempts: u32,
    pub cdp_endpoint_last_error: Option<String>,
    pub browser_product: Option<String>,
    pub browser_ws_url_obtained: bool,
    pub browser_ws_connected: bool,
    pub cdp_connected: bool,
    pub target_found: bool,
    pub target_id_found: bool,
    pub session_created: bool,
    pub managed_game_page: bool,
    pub page_enabled: bool,
    pub page_navigate_sent: bool,
    pub game_url_navigated: bool,
    pub final_url: Option<String>,
    pub network_enabled: bool,
    pub websocket_count: u32,
    /// Counts only game WebSocket lifecycle events observed by CDP; used to
    /// prove the validation reload disconnected and reconnected the Browser.
    pub game_ws_created_count: u32,
    pub game_ws_closed_count: u32,
    pub game_ws_open_count: u32,
    pub pokeidle_socket_detected: bool,
    pub websocket_detected: bool,
    pub hello_detected: bool,
    pub welcome_received: bool,
    pub ws_url_captured: bool,
    pub session_material_captured: bool,
    pub rust_ws_connected: bool,
    pub rust_hello_sent: bool,
    pub rust_welcome_received: bool,
    pub browser_closed: bool,
    pub background_active: bool,
    pub controlled_brave_pid: Option<u32>,
    pub automatic_reload_used: bool,
    pub reload_guard: Option<ReloadGuardReason>,
    #[cfg(debug_assertions)]
    pub controlled_reload_status: ControlledReloadStatus,
    #[cfg(debug_assertions)]
    pub controlled_reload_old_socket_observed: bool,
    #[cfg(debug_assertions)]
    pub controlled_reload_old_socket_closed: bool,
    #[cfg(debug_assertions)]
    pub controlled_reload_new_socket_observed: bool,
    #[cfg(debug_assertions)]
    pub controlled_reload_document_generation_before: Option<u64>,
    #[cfg(debug_assertions)]
    pub controlled_reload_document_generation_after: Option<u64>,
    #[cfg(debug_assertions)]
    pub controlled_reload_rust_ws_ever_connected: bool,
    pub nick: Option<String>,
    pub state: String,
    pub message: String,
    pub account_id: Option<String>,
}
impl IntegrationDiagnostic {
    pub fn waiting() -> Self {
        Self {
            state: "Aguardando".into(),
            message: "Nenhuma integração em andamento.".into(),
            ..Self::default()
        }
    }
}

#[derive(Clone)]
pub struct WindowsBraveManager {
    base: PathBuf,
}

/// The process handle for the one Brave instance opened by a recovery probe.
/// The CLI keeps ownership so it can wait/reap it even if CDP setup fails.
pub type RecoveryProcessSlot = Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>;

/// Forcefully terminates only the owned Brave process tree and reaps its root.
/// The normal cleanup path remains CDP `Browser.close`; this is a bounded
/// fallback for cancellation or failed CDP shutdown.
pub(crate) async fn terminate_recovery_process_tree(child: &mut tokio::process::Child) -> bool {
    #[cfg(windows)]
    {
        let Some(pid) = child.id() else {
            return false;
        };
        // Resolve the OS utility through SystemRoot instead of PATH, so a
        // workspace-local or user-provided executable cannot intercept cleanup.
        let Some(system_root) = std::env::var_os("SystemRoot") else {
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            return false;
        };
        let taskkill_path = PathBuf::from(system_root)
            .join("System32")
            .join("taskkill.exe");
        let taskkill = tokio::process::Command::new(taskkill_path)
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !tokio::time::timeout(Duration::from_secs(5), taskkill)
            .await
            .is_ok_and(|status| status.is_ok_and(|status| status.success()))
        {
            let _ = child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            return false;
        }
    }
    #[cfg(not(windows))]
    if child.kill().await.is_err() {
        return false;
    }

    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_ok_and(|status| status.is_ok())
}
impl WindowsBraveManager {
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }
    pub fn profile_for(&self, account_id: &str) -> BrowserProfile {
        BrowserProfile {
            path: self.base.join("profiles").join(account_id),
        }
    }
    pub fn detect_brave(&self) -> Result<PathBuf, BrowserError> {
        let paths = [
            std::env::var_os("PROGRAMFILES")
                .map(PathBuf::from)
                .map(|p| p.join("BraveSoftware/Brave-Browser/Application/brave.exe")),
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|p| p.join("BraveSoftware/Brave-Browser/Application/brave.exe")),
            std::env::var_os("PROGRAMFILES(X86)")
                .map(PathBuf::from)
                .map(|p| p.join("BraveSoftware/Brave-Browser/Application/brave.exe")),
        ];
        resolve_brave_candidates(paths.into_iter().flatten())
    }
    fn saved_cdp_port(&self, profile: &BrowserProfile) -> Option<u16> {
        std::fs::read_to_string(profile.path.join("cdp-port"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    fn save_cdp_port(&self, profile: &BrowserProfile, port: u16) -> Result<(), BrowserError> {
        std::fs::write(profile.path.join("cdp-port"), port.to_string())
            .map_err(|error| BrowserError::Launch(error.to_string()))
    }
    fn saved_cdp_pid(&self, profile: &BrowserProfile) -> Option<u32> {
        std::fs::read_to_string(profile.path.join("cdp-pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
    fn save_cdp_pid(&self, profile: &BrowserProfile, pid: u32) -> Result<(), BrowserError> {
        std::fs::write(profile.path.join("cdp-pid"), pid.to_string())
            .map_err(|error| BrowserError::Launch(error.to_string()))
    }
    pub fn launch_controlled(
        &self,
        executable: &PathBuf,
        profile: &BrowserProfile,
        port: u16,
        initial_url: Option<&str>,
        mode: BrowserSessionMode,
    ) -> Result<tokio::process::Child, BrowserError> {
        std::fs::create_dir_all(&profile.path)
            .map_err(|error| BrowserError::Launch(error.to_string()))?;
        let mut command = tokio::process::Command::new(executable);
        // The controlled browser is a GUI child. Never inherit a development
        // console: Chromium writes its CDP address to stderr and Windows can
        // otherwise surface a terminal alongside the Manager.
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(matches!(mode, BrowserSessionMode::RecoveryProbe));
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.args([
            format!("--user-data-dir={}", profile.path.display()),
            "--remote-debugging-address=127.0.0.1".into(),
            format!("--remote-debugging-port={port}"),
            "--remote-allow-origins=http://127.0.0.1".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
        ]);
        match mode {
            BrowserSessionMode::Interactive
            | BrowserSessionMode::InteractiveOwner
            | BrowserSessionMode::ReconnectExistingOwner
            | BrowserSessionMode::RecoveryProbe => {
                if initial_url.is_some() {
                    command.arg("--new-window");
                } else {
                    // Gated startup must not ask Chromium to create a second
                    // page while it restores this profile. If no usable page
                    // exists, the observer creates one CDP target and tracks
                    // that exact target ID before navigating it.
                    command.arg("--no-startup-window");
                }
            }
            BrowserSessionMode::BackgroundBootstrap => {
                command.arg("--headless=new");
                if initial_url.is_none() {
                    command.arg("--no-startup-window");
                }
            }
        }
        if let Some(initial_url) = initial_url {
            command.arg(initial_url);
        }
        command
            .spawn()
            .map_err(|error| BrowserError::BrowserFailedToStart(error.to_string()))
    }
}

fn resolve_brave_candidates(
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<PathBuf, BrowserError> {
    let mut invalid_candidate = None;
    for path in paths {
        if path.is_file() {
            let is_windows_executable = std::fs::File::open(&path)
                .and_then(|mut file| {
                    let mut signature = [0; 2];
                    file.read_exact(&mut signature)?;
                    Ok(signature == *b"MZ")
                })
                .unwrap_or(false);
            if is_windows_executable {
                return Ok(path);
            }
            if invalid_candidate.is_none() {
                invalid_candidate = Some(path);
            }
            continue;
        }
        if path.exists() && invalid_candidate.is_none() {
            invalid_candidate = Some(path);
        }
    }
    match invalid_candidate {
        Some(path) => Err(BrowserError::InvalidInstallation(path)),
        None => Err(BrowserError::NotFound),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CdpTargetInfo {
    #[serde(rename = "targetId")]
    id: String,
    #[serde(rename = "type")]
    target_type: String,
    url: String,
}

fn update(
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    apply: impl FnOnce(&mut IntegrationDiagnostic),
) {
    apply(&mut diagnostic.lock());
}

fn transition(
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    lifecycle: BrowserLifecycle,
    message: impl Into<String>,
) {
    let message = message.into();
    update(diagnostic, |status| {
        status.lifecycle = lifecycle;
        status.state = lifecycle.label().into();
        status.message = message;
    });
}

fn free_local_port() -> Result<u16, BrowserError> {
    TcpListener::bind("127.0.0.1:0")
        .map_err(|error| BrowserError::Launch(error.to_string()))?
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| BrowserError::Launch(error.to_string()))
}

#[derive(Debug, Clone)]
struct CdpVersion {
    browser: String,
    web_socket_debugger_url: String,
}

#[derive(Debug)]
struct CdpProbeError {
    kind: &'static str,
    source: String,
    url: String,
}

fn cdp_http_client() -> Result<reqwest::Client, BrowserError> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|error| BrowserError::CdpClient(error.to_string()))
}

fn request_error(url: String, error: reqwest::Error) -> CdpProbeError {
    let kind = if error.is_connect() {
        "connection refused/conexão"
    } else if error.is_timeout() {
        "timeout"
    } else if error.is_request() {
        "erro de requisição"
    } else {
        "outro erro HTTP"
    };
    let nested = std::error::Error::source(&error)
        .map(ToString::to_string)
        .unwrap_or_else(|| "sem causa interna adicional".into());
    CdpProbeError {
        kind,
        source: format!("{error}; source: {nested}"),
        url,
    }
}

async fn fetch_cdp_version(
    client: &reqwest::Client,
    port: u16,
) -> Result<CdpVersion, CdpProbeError> {
    let url = format!("http://127.0.0.1:{port}/json/version");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| request_error(url.clone(), error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CdpProbeError {
            kind: "HTTP status inesperado",
            source: status.to_string(),
            url,
        });
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|error| CdpProbeError {
            kind: "erro de parsing JSON",
            source: error.to_string(),
            url: url.clone(),
        })?;
    let browser = value
        .get("Browser")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CdpProbeError {
            kind: "erro de parsing JSON",
            source: "campo Browser ausente ou inválido".into(),
            url: url.clone(),
        })?;
    let web_socket_debugger_url = value
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CdpProbeError {
            kind: "erro de parsing JSON",
            source: "campo webSocketDebuggerUrl ausente ou inválido".into(),
            url,
        })?;
    Ok(CdpVersion {
        browser,
        web_socket_debugger_url,
    })
}

async fn wait_for_cdp_endpoint(
    client: &reqwest::Client,
    port: u16,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
) -> Result<CdpVersion, BrowserError> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let started_at = Instant::now();
    let mut attempt = 0;
    loop {
        attempt += 1;
        match fetch_cdp_version(client, port).await {
            Ok(version) => {
                update(diagnostic, |status| {
                    status.cdp_endpoint_available = true;
                    status.cdp_endpoint_attempts = attempt;
                    status.cdp_endpoint_last_error = None;
                    status.browser_product = Some(version.browser.clone());
                    status.browser_ws_url_obtained = true;
                    status.message = "Endpoint CDP confirmado pelo backend Rust.".into();
                });
                return Ok(version);
            }
            Err(error) => {
                let elapsed = started_at.elapsed().as_millis();
                let attempt_error = format!(
                    "tentativa {attempt}: kind={}; source={}; url={}; elapsed={elapsed} ms",
                    error.kind, error.source, error.url
                );
                update(diagnostic, |status| {
                    status.cdp_endpoint_attempts = attempt;
                    status.cdp_endpoint_last_error = Some(attempt_error.clone());
                    status.message = format!("Aguardando endpoint CDP ({attempt_error})");
                });
                if Instant::now() >= deadline {
                    return Err(BrowserError::CdpEndpoint {
                        port,
                        details: attempt_error,
                    });
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn game_origin(game_url: &str) -> String {
    game_url
        .split_once("://")
        .map(|(scheme, remainder)| {
            format!(
                "{scheme}://{}",
                remainder.split('/').next().unwrap_or_default()
            )
        })
        .unwrap_or_else(|| game_url.trim_end_matches('/').to_owned())
}

fn is_game_page(target: &CdpTargetInfo, game_origin: &str) -> bool {
    target.target_type == "page"
        && (target.url == game_origin || target.url.starts_with(&format!("{game_origin}/")))
}

async fn cdp_command(
    socket: &mut CdpSocket,
    id: i64,
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> Result<(Value, Vec<Value>), BrowserError> {
    tokio::time::timeout(
        Duration::from_secs(10),
        cdp_command_inner(socket, id, method, params, session_id),
    )
    .await
    .map_err(|_| BrowserError::Launch(format!("CDP {method}: tempo esgotado")))?
}

async fn cdp_command_inner(
    socket: &mut CdpSocket,
    id: i64,
    method: &str,
    params: Value,
    session_id: Option<&str>,
) -> Result<(Value, Vec<Value>), BrowserError> {
    let mut command = json!({"id": id, "method": method, "params": params});
    if let Some(session_id) = session_id {
        command["sessionId"] = Value::String(session_id.to_owned());
    }
    socket
        .stream
        .send(Message::Text(command.to_string().into()))
        .await
        .map_err(|error| BrowserError::Launch(format!("CDP {method}: {error}")))?;
    while let Some(message) = socket.stream.next().await {
        let message = message.map_err(|error| BrowserError::Launch(error.to_string()))?;
        let Message::Text(raw) = message else {
            continue;
        };
        let response: Value =
            serde_json::from_str(&raw).map_err(|error| BrowserError::Launch(error.to_string()))?;
        if response.get("id").and_then(Value::as_i64) == Some(id) {
            if let Some(error) = response.get("error") {
                return Err(BrowserError::Launch(format!("CDP {method}: {error}")));
            }
            return Ok((
                response.get("result").cloned().unwrap_or(Value::Null),
                Vec::new(),
            ));
        }
        if response.get("method").is_some() {
            socket.pending_events.push_back(response);
        }
    }
    Err(BrowserError::Launch(format!(
        "CDP {method}: sessão encerrada"
    )))
}

const VALIDATION_READ_ONLY_FRAME_TYPES: &[&str] = &[
    "market.itens",
    "market.item",
    "market.historicoGlobal",
    "ranking.perfil",
];

const BROWSER_WS_BRIDGE_SOURCE: &str = r#"(() => {
  const key = '__pokeidleManagerBridge';
  if (window[key]) return;
  const validationReadOnly = __VALIDATION_READ_ONLY__;
  const validationReadOnlyTypes = new Set(__VALIDATION_READ_ONLY_TYPES__);
  const nativeSend = WebSocket.prototype.send;
  let candidate = null;
  let confirmed = null;
  const parse = (value) => {
    if (typeof value !== 'string') return null;
    try { return JSON.parse(value); } catch { return null; }
  };
  const allowsFrame = (frame) => !validationReadOnly || Boolean(
    frame && (frame.t === 'hello' || validationReadOnlyTypes.has(frame.t))
  );
  WebSocket.prototype.send = function(data) {
    const frame = parse(data);
    if (!allowsFrame(frame)) return;
    if (frame && frame.t === 'hello') {
      candidate = this;
      confirmed = null;
      this.addEventListener('message', (event) => {
        const received = parse(event.data);
        if (candidate === this && received && received.t === 'welcome') confirmed = this;
      });
    }
    return nativeSend.apply(this, arguments);
  };
  Object.defineProperty(window, key, {
    configurable: true,
    value: Object.freeze({
      get ready() { return Boolean(confirmed && confirmed.readyState === WebSocket.OPEN); },
      send(payload) {
        if (!confirmed || confirmed.readyState !== WebSocket.OPEN) return false;
        if (!allowsFrame(parse(payload))) return false;
        nativeSend.call(confirmed, payload);
        return true;
      }
    })
  });
})()"#;

fn browser_ws_bridge_source(validation_read_only: bool) -> String {
    BROWSER_WS_BRIDGE_SOURCE
        .replace(
            "__VALIDATION_READ_ONLY__",
            if validation_read_only {
                "true"
            } else {
                "false"
            },
        )
        .replace(
            "__VALIDATION_READ_ONLY_TYPES__",
            &json!(VALIDATION_READ_ONLY_FRAME_TYPES).to_string(),
        )
}

#[cfg(debug_assertions)]
fn validation_bridge_allows_frame_type(frame_type: Option<&str>) -> bool {
    matches!(frame_type, Some("hello"))
        || frame_type
            .is_some_and(|frame_type| VALIDATION_READ_ONLY_FRAME_TYPES.contains(&frame_type))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BrowserBridgeStartupPlan {
    start_blank: bool,
    install_bridge: bool,
    read_only: bool,
    navigate_after_gate: bool,
}

fn browser_bridge_startup_plan(
    mode: BrowserSessionMode,
    validation_mode_active: bool,
) -> BrowserBridgeStartupPlan {
    // Operational validation must keep the game's normal WebSocket protocol
    // intact. Its mutating-command guard lives in AccountCommandDispatcher;
    // only the isolated RecoveryProbe filters the page's own frames.
    let read_only = matches!(mode, BrowserSessionMode::RecoveryProbe);
    let gate_before_navigation = validation_mode_active
        || read_only
        || matches!(
            mode,
            BrowserSessionMode::InteractiveOwner
                | BrowserSessionMode::ReconnectExistingOwner
                | BrowserSessionMode::BackgroundBootstrap
        );
    BrowserBridgeStartupPlan {
        start_blank: gate_before_navigation,
        install_bridge: gate_before_navigation || is_interactive_owner_mode(mode),
        read_only,
        navigate_after_gate: gate_before_navigation,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatedStartupTargetDecision {
    UseBlankTarget,
    AdoptRestoredGameTarget,
    BlockMultipleGameTargets,
    WaitForBlankTarget,
}

enum GatedBlankWaitResult {
    Ready(CdpTargetInfo),
    GameTargetAppeared(CdpTargetInfo),
    MultipleGameTargets,
    NoPageAppeared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockedBrowserOwnershipAction {
    RetainStartedProcess,
    PreserveReusedProcess,
}

fn blocked_browser_ownership_action(owns_child: bool) -> BlockedBrowserOwnershipAction {
    if owns_child {
        BlockedBrowserOwnershipAction::RetainStartedProcess
    } else {
        BlockedBrowserOwnershipAction::PreserveReusedProcess
    }
}

fn gated_startup_target_decision(
    blank_target_found: bool,
    game_target_count: usize,
) -> GatedStartupTargetDecision {
    if game_target_count > 1 {
        GatedStartupTargetDecision::BlockMultipleGameTargets
    } else if game_target_count == 1 {
        GatedStartupTargetDecision::AdoptRestoredGameTarget
    } else if blank_target_found {
        GatedStartupTargetDecision::UseBlankTarget
    } else {
        GatedStartupTargetDecision::WaitForBlankTarget
    }
}

fn initial_browser_url(game_url: &str, gated_startup: bool) -> Option<&str> {
    if gated_startup { None } else { Some(game_url) }
}

async fn install_browser_ws_bridge(
    socket: &mut CdpSocket,
    id: i64,
    session_id: &str,
    validation_read_only: bool,
) -> Result<Vec<Value>, BrowserError> {
    let (_, events) = cdp_command(
        socket,
        id,
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": browser_ws_bridge_source(validation_read_only)}),
        Some(session_id),
    )
    .await?;
    Ok(events)
}

async fn send_via_browser_ws_bridge(
    socket: &mut CdpSocket,
    id: i64,
    session_id: &str,
    frame: &crate::protocol::ClientFrame,
) -> Result<Vec<Value>, BrowserError> {
    let payload = serde_json::to_string(frame).map_err(|error| {
        BrowserError::Launch(format!("Não foi possível serializar comando: {error}"))
    })?;
    let expression = format!(
        "Boolean(window.__pokeidleManagerBridge && window.__pokeidleManagerBridge.send({}))",
        serde_json::to_string(&payload).unwrap_or_default(),
    );
    let (result, events) = cdp_command(
        socket,
        id,
        "Runtime.evaluate",
        json!({"expression": expression, "returnByValue": true, "awaitPromise": true}),
        Some(session_id),
    )
    .await?;
    if result.pointer("/result/value").and_then(Value::as_bool) != Some(true) {
        return Err(BrowserError::Launch(
            "WebSocket do jogo ainda não foi confirmado por hello → welcome.".into(),
        ));
    }
    Ok(events)
}

async fn browser_ws_bridge_ready(
    socket: &mut CdpSocket,
    id: i64,
    session_id: &str,
) -> Result<(bool, Vec<Value>), BrowserError> {
    let (result, events) = cdp_command(
        socket,
        id,
        "Runtime.evaluate",
        json!({"expression": "Boolean(window.__pokeidleManagerBridge && window.__pokeidleManagerBridge.ready)", "returnByValue": true}),
        Some(session_id),
    )
    .await?;
    Ok((
        result.pointer("/result/value").and_then(Value::as_bool) == Some(true),
        events,
    ))
}

fn targets_from(result: &Value) -> Result<Vec<CdpTargetInfo>, BrowserError> {
    serde_json::from_value(
        result
            .get("targetInfos")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    )
    .map_err(|error| BrowserError::Launch(format!("Target.getTargets: {error}")))
}

/// Returns a pre-existing game page only when the target is unambiguous.
/// OAuth/CAPTCHA and duplicate game targets are never candidates for cleanup.
async fn existing_game_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    game_url: &str,
) -> Result<Option<CdpTargetInfo>, BrowserError> {
    let (result, _) = cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
    *next_id += 1;
    let origin = game_origin(game_url);
    let targets = targets_from(&result)?;
    let game_pages: Vec<CdpTargetInfo> = targets
        .iter()
        .filter(|target| is_game_page(target, &origin))
        .cloned()
        .collect();
    if game_pages.len() > 1 {
        return Err(BrowserError::Launch(
            "Inicialização bloqueada: existem múltiplas páginas top-level do Pokeidle; nenhuma foi escolhida ou fechada.".into(),
        ));
    }
    Ok(game_pages.into_iter().next())
}

async fn wait_for_game_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    game_url: &str,
    create_if_missing: bool,
) -> Result<CdpTargetInfo, BrowserError> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(target) = existing_game_target(socket, next_id, game_url).await? {
            return Ok(target);
        }
        if Instant::now() >= deadline {
            if create_if_missing {
                let (created, _) = cdp_command(
                    socket,
                    *next_id,
                    "Target.createTarget",
                    json!({"url": game_url}),
                    None,
                )
                .await?;
                *next_id += 1;
                let target_id =
                    created
                        .get("targetId")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            BrowserError::Launch(
                                "Target.createTarget não retornou targetId para a aba do jogo."
                                    .into(),
                            )
                        })?;
                let (targets, _) =
                    cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
                *next_id += 1;
                return targets_from(&targets)?
                    .into_iter()
                    .find(|target| {
                        target.id == target_id && is_game_page(target, &game_origin(game_url))
                    })
                    .ok_or_else(|| {
                        BrowserError::Launch(
                            "A nova aba do jogo não apareceu entre os targets CDP.".into(),
                        )
                    });
            }
            return Err(BrowserError::Launch(
                "A página do jogo não apareceu entre os targets CDP dentro de 10 segundos.".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_blank_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    game_origin: &str,
) -> Result<GatedBlankWaitResult, BrowserError> {
    // Give profile restoration a short chance to expose a restored game page,
    // but do not make a fresh profile wait through the old 10-second timeout.
    // If nothing appears, create and track one blank target explicitly below.
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let (result, _events) =
            cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
        *next_id += 1;
        let targets = targets_from(&result)?;
        let game_targets: Vec<_> = targets
            .iter()
            .filter(|target| is_game_page(target, game_origin))
            .cloned()
            .collect();
        if game_targets.len() > 1 {
            return Ok(GatedBlankWaitResult::MultipleGameTargets);
        }
        if let Some(target) = game_targets.into_iter().next() {
            return Ok(GatedBlankWaitResult::GameTargetAppeared(target));
        }
        if let Some(target) = targets
            .into_iter()
            .find(|target| target.target_type == "page" && target.url == "about:blank")
        {
            return Ok(GatedBlankWaitResult::Ready(target));
        }
        if Instant::now() >= deadline {
            return Ok(GatedBlankWaitResult::NoPageAppeared);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn create_manager_blank_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
) -> Result<CdpTargetInfo, BrowserError> {
    let (result, _) = cdp_command(
        socket,
        *next_id,
        "Target.createTarget",
        json!({"url":"about:blank"}),
        None,
    )
    .await?;
    *next_id += 1;
    let id = result
        .get("targetId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            BrowserError::Launch(
                "Target.createTarget não retornou targetId para a página neutra do Manager.".into(),
            )
        })?;
    Ok(CdpTargetInfo {
        id: id.to_owned(),
        target_type: "page".into(),
        url: "about:blank".into(),
    })
}

/// Closes only a temporary target returned by `Target.createTarget` in this
/// startup attempt. It is never used for discovered/restored targets or to
/// close the Browser process.
async fn close_manager_created_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    target_id: &str,
) -> bool {
    let result = cdp_command(
        socket,
        *next_id,
        "Target.closeTarget",
        json!({"targetId": target_id}),
        None,
    )
    .await;
    *next_id += 1;
    result.is_ok_and(|(result, _)| result.get("success").and_then(Value::as_bool) == Some(true))
}

async fn find_unobserved_game_target_before_navigation(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    controlled_target_id: &str,
    game_origin: &str,
) -> Result<Vec<CdpTargetInfo>, BrowserError> {
    // A target can be restored asynchronously just after the first snapshot.
    // Require a short quiet window with repeated snapshots immediately before
    // navigating; any second game page/event fails closed.
    const DISCOVERY_QUIET_WINDOW: Duration = Duration::from_millis(250);
    const DISCOVERY_POLL_INTERVAL: Duration = Duration::from_millis(25);
    let deadline = Instant::now() + DISCOVERY_QUIET_WINDOW;
    let mut other_game_pages = HashMap::new();
    loop {
        let (result, events) =
            cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
        *next_id += 1;
        let snapshot = targets_from(&result)?;
        let controlled = snapshot
            .iter()
            .find(|target| target.id == controlled_target_id && target.target_type == "page")
            .ok_or_else(|| {
                BrowserError::Launch(
                    "Baseline bloqueado: target controlada desapareceu antes da navegação.".into(),
                )
            })?;
        if controlled.url != "about:blank" {
            return Err(BrowserError::Launch(
                "Baseline bloqueado: target controlada deixou de estar em about:blank antes da navegação."
                    .into(),
            ));
        }
        for target in snapshot
            .into_iter()
            .filter(|target| is_game_page(target, game_origin))
            .chain(
                events
                    .into_iter()
                    .filter_map(|event| game_target_from_event(&event, game_origin)),
            )
            .chain(take_game_target_events(
                &mut socket.pending_events,
                game_origin,
            ))
        {
            // Ignore old URL events for this adopted target; its current URL
            // was just verified. Other page IDs arriving late must block.
            if target.id != controlled_target_id {
                other_game_pages.entry(target.id.clone()).or_insert(target);
            }
        }
        if !other_game_pages.is_empty() {
            return Ok(other_game_pages.into_values().collect());
        }
        if Instant::now() >= deadline {
            return Ok(Vec::new());
        }
        tokio::time::sleep(
            DISCOVERY_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        )
        .await;
    }
}

fn unique_game_target_after_navigation(
    targets: &[CdpTargetInfo],
    expected_target_id: &str,
    game_origin: &str,
) -> Option<CdpTargetInfo> {
    let game_pages: Vec<_> = targets
        .iter()
        .filter(|target| is_game_page(target, game_origin))
        .collect();
    (game_pages.len() == 1 && game_pages[0].id == expected_target_id).then(|| game_pages[0].clone())
}

async fn verify_unique_game_target_after_navigation(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    expected_target_id: &str,
    game_origin: &str,
) -> Result<CdpTargetInfo, BrowserError> {
    const RECONCILIATION_WINDOW: Duration = Duration::from_millis(250);
    const RECONCILIATION_POLL_INTERVAL: Duration = Duration::from_millis(25);
    let deadline = Instant::now() + RECONCILIATION_WINDOW;
    loop {
        let (result, events) =
            cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
        *next_id += 1;
        let snapshot = targets_from(&result)?;
        let current = unique_game_target_after_navigation(
            &snapshot,
            expected_target_id,
            game_origin,
        )
        .ok_or_else(|| {
            BrowserError::Launch(
                "Navegação bloqueada: durante a reconciliação não permaneceu exatamente uma página Pokeidle na target controlada."
                    .into(),
            )
        })?;
        let other_target_event = events
            .into_iter()
            .filter_map(|event| game_target_from_event(&event, game_origin))
            .chain(take_game_target_events(
                &mut socket.pending_events,
                game_origin,
            ))
            .any(|target| target.id != expected_target_id);
        if other_target_event {
            return Err(BrowserError::Launch(
                "Navegação bloqueada: uma segunda página top-level do Pokeidle apareceu durante a reconciliação."
                    .into(),
            ));
        }
        if Instant::now() >= deadline {
            return Ok(current);
        }
        tokio::time::sleep(
            RECONCILIATION_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        )
        .await;
    }
}

async fn neutralize_restored_game_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    session_id: &str,
) -> Result<Vec<Value>, BrowserError> {
    // Discard only navigation-completion events from before our navigation;
    // retain every other CDP event for the normal observer.
    socket.pending_events.retain(|event| {
        event.get("sessionId").and_then(Value::as_str) != Some(session_id)
            || !matches!(
                event.get("method").and_then(Value::as_str),
                Some("Page.loadEventFired" | "Page.frameNavigated")
            )
    });

    let (_, _) = cdp_command(
        socket,
        *next_id,
        "Page.navigate",
        json!({"url": "about:blank"}),
        Some(session_id),
    )
    .await?;
    *next_id += 1;

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut main_frame_committed = false;
    let mut load_event_fired = false;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Some(message) = tokio::time::timeout(remaining, socket.next())
            .await
            .map_err(|_| {
                BrowserError::Launch(
                    "Neutralização bloqueada: about:blank não concluiu a navegação no prazo."
                        .into(),
                )
            })?
        else {
            return Err(BrowserError::Launch(
                "Neutralização bloqueada: sessão CDP encerrou antes de confirmar about:blank."
                    .into(),
            ));
        };
        let message = message.map_err(|error| {
            BrowserError::Launch(format!("Neutralização bloqueada: CDP: {error}"))
        })?;
        let Message::Text(raw) = message else {
            continue;
        };
        let event: Value = serde_json::from_str(&raw).map_err(|error| {
            BrowserError::Launch(format!(
                "Neutralização bloqueada: evento CDP inválido: {error}"
            ))
        })?;
        if event.get("sessionId").and_then(Value::as_str) == Some(session_id) {
            match event.get("method").and_then(Value::as_str) {
                Some("Page.frameNavigated") => {
                    let frame = event.get("params").and_then(|params| params.get("frame"));
                    let is_main_frame = frame
                        .and_then(|frame| frame.get("parentId"))
                        .map_or(true, Value::is_null);
                    let is_blank = frame
                        .and_then(|frame| frame.get("url"))
                        .and_then(Value::as_str)
                        == Some("about:blank");
                    main_frame_committed |= is_main_frame && is_blank;
                }
                Some("Page.loadEventFired") => load_event_fired = true,
                _ => {}
            }
        }
        events.push(event);
        if main_frame_committed && load_event_fired {
            return Ok(events);
        }
    }
    Err(BrowserError::Launch(
        "Neutralização bloqueada: não foi possível confirmar commit e load de about:blank.".into(),
    ))
}

async fn verify_neutralized_target_baseline(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    target_id: &str,
    game_origin: &str,
) -> Result<CdpTargetInfo, BrowserError> {
    let (result, _) = cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
    *next_id += 1;
    let targets = targets_from(&result)?;
    let adopted = targets
        .iter()
        .find(|target| target.id == target_id && target.target_type == "page")
        .cloned()
        .ok_or_else(|| {
            BrowserError::Launch(
                "Neutralização bloqueada: a target adotada não está mais disponível.".into(),
            )
        })?;
    if adopted.url != "about:blank" {
        return Err(BrowserError::Launch(
            "Neutralização bloqueada: a mesma target não confirmou URL about:blank.".into(),
        ));
    }
    if targets
        .iter()
        .any(|target| is_game_page(target, game_origin))
    {
        return Err(BrowserError::Launch(
            "Neutralização bloqueada: outra página top-level do Pokeidle permaneceu aberta.".into(),
        ));
    }
    Ok(adopted)
}

async fn retain_blocked_browser_process(
    account_id: &str,
    controlled_process: &mut Option<tokio::process::Child>,
) -> bool {
    static GUARDED_BROWSER_PROCESSES: OnceLock<
        tokio::sync::Mutex<Vec<(String, tokio::task::JoinHandle<()>)>>,
    > = OnceLock::new();
    if blocked_browser_ownership_action(controlled_process.is_some())
        == BlockedBrowserOwnershipAction::PreserveReusedProcess
    {
        // Reused Browser process is preserved in place; no target or process
        // is closed when startup is blocked.
        return false;
    }
    let Some(mut child) = controlled_process.take() else {
        return false;
    };
    let account_id = account_id.to_owned();
    let monitor_account = account_id.clone();
    let monitor = tokio::spawn(async move {
        if let Err(error) = child.wait().await {
            tracing::warn!(account_id = %monitor_account, %error, "blocked Browser child wait failed");
        } else {
            tracing::debug!(account_id = %monitor_account, "blocked Browser process exited and was reaped");
        }
    });
    let monitors = GUARDED_BROWSER_PROCESSES.get_or_init(|| tokio::sync::Mutex::new(Vec::new()));
    let mut monitors = monitors.lock().await;
    monitors.retain(|(_, task)| !task.is_finished());
    monitors.push((account_id, monitor));
    true
}

fn game_target_from_event(event: &Value, game_origin: &str) -> Option<CdpTargetInfo> {
    if !matches!(
        event.get("method").and_then(Value::as_str),
        Some("Target.targetCreated" | "Target.targetInfoChanged")
    ) {
        return None;
    }
    let target: CdpTargetInfo =
        serde_json::from_value(event.pointer("/params/targetInfo")?.clone()).ok()?;
    is_game_page(&target, game_origin).then_some(target)
}

fn take_game_target_events(
    pending_events: &mut VecDeque<Value>,
    game_origin: &str,
) -> Vec<CdpTargetInfo> {
    let mut retained = VecDeque::new();
    let mut game_targets = HashMap::new();
    while let Some(event) = pending_events.pop_front() {
        if let Some(target) = game_target_from_event(&event, game_origin) {
            game_targets.entry(target.id.clone()).or_insert(target);
        } else {
            retained.push_back(event);
        }
    }
    *pending_events = retained;
    game_targets.into_values().collect()
}

async fn wait_for_navigated_target(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    target_id: &str,
    game_url: &str,
) -> Result<(CdpTargetInfo, Vec<Value>), BrowserError> {
    let origin = game_origin(game_url);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    loop {
        let (result, mut observed_events) =
            cdp_command(socket, *next_id, "Target.getTargets", json!({}), None).await?;
        events.append(&mut observed_events);
        *next_id += 1;
        if let Some(target) = targets_from(&result)?
            .into_iter()
            .find(|target| target.id == target_id && is_game_page(target, &origin))
        {
            return Ok((target, events));
        }
        if Instant::now() >= deadline {
            return Err(BrowserError::Launch(
                "A página não chegou ao GAME_URL após a instalação do guard.".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn is_pokeidle_socket(url: &str) -> bool {
    url.contains("://pokeidle.io") || url.contains("://www.pokeidle.io")
}

#[derive(Clone)]
struct ObservedSocket {
    ws_url: String,
    headers: Option<SessionHeaders>,
    hello: Option<Hello>,
    document_generation: u64,
    handshake_response_received: bool,
}

/// Emitted exactly once by an InteractiveOwner observer after its live browser
/// transport has been proven lost. Session material stays in memory only long
/// enough for the runtime to try Rust Background recovery.
#[derive(Debug)]
pub struct BrowserOwnerLost {
    pub reason: String,
    pub bootstrap: Option<ConnectionBootstrap>,
}

enum HandoffFailure {
    BrowserStillActive(String),
    BrowserTargetClosed(String),
}

fn browser_owner_loss_reason(event: &Value, target_id: &str) -> Option<&'static str> {
    let method = event.get("method").and_then(Value::as_str)?;
    match method {
        "Target.targetDestroyed"
            if event.pointer("/params/targetId").and_then(Value::as_str) == Some(target_id) =>
        {
            Some("A aba gerenciada do jogo foi fechada")
        }
        _ => None,
    }
}

fn browser_control_loss_reason(event: &Value, session_id: &str) -> Option<&'static str> {
    let method = event.get("method").and_then(Value::as_str)?;
    match method {
        "Target.detachedFromTarget"
            if event.pointer("/params/sessionId").and_then(Value::as_str) == Some(session_id) =>
        {
            Some(
                "A sessão CDP foi desanexada; a conexão do jogo ainda não foi comprovada como encerrada",
            )
        }
        "Inspector.detached"
            if event.get("sessionId").and_then(Value::as_str) == Some(session_id) =>
        {
            Some(
                "O Inspector foi desanexado; a conexão do jogo ainda não foi comprovada como encerrada",
            )
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetachedTargetDecision {
    RecoverAfterTargetDestroyed,
    CloseTargetBeforeFallback,
    StopWithoutFallback,
}

fn detached_target_decision(target_exists: Option<bool>) -> DetachedTargetDecision {
    match target_exists {
        Some(false) => DetachedTargetDecision::RecoverAfterTargetDestroyed,
        Some(true) => DetachedTargetDecision::CloseTargetBeforeFallback,
        None => DetachedTargetDecision::StopWithoutFallback,
    }
}

fn candidate_game_socket_closed(event: &Value, candidate_socket_id: Option<&str>) -> bool {
    event.get("method").and_then(Value::as_str) == Some("Network.webSocketClosed")
        && candidate_socket_id == event.pointer("/params/requestId").and_then(Value::as_str)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageReloadSafetyDecision {
    ReloadNow,
    RefuseUnobservedTarget,
    RefuseActiveSocket,
    RefuseReconnectGrace,
}

fn page_reload_safety_decision(
    candidate_socket_id: Option<&str>,
    sockets: &HashMap<String, ObservedSocket>,
    reconnect_grace_active: bool,
    observation_covered_from_navigation: bool,
) -> PageReloadSafetyDecision {
    if !observation_covered_from_navigation {
        PageReloadSafetyDecision::RefuseUnobservedTarget
    } else if reconnect_grace_active {
        PageReloadSafetyDecision::RefuseReconnectGrace
    } else if candidate_socket_id.is_none() && sockets.is_empty() {
        PageReloadSafetyDecision::ReloadNow
    } else {
        PageReloadSafetyDecision::RefuseActiveSocket
    }
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlledReloadBlockReason {
    ValidationModeInactive,
    WrongSessionMode,
    UnmanagedTarget,
    NetworkCoverageMissing,
    UnobservedTargets,
    NonUniqueGamePage,
    SocketCountNotOne,
    CandidateSocketMissing,
    SocketNotCurrentDocument,
    HandshakeNotConfirmed,
    BrowserOwnerNotOnline,
    RustTransportConnected,
    MutationGuardInactive,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlledReloadSocketSnapshot {
    socket_id: String,
    document_generation: u64,
    handshake_response_received: bool,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlledReloadPreconditions {
    validation_mode_active: bool,
    interactive_owner_mode: bool,
    managed_target: bool,
    network_covered_from_navigation: bool,
    unobserved_game_targets: usize,
    game_page_count: usize,
    candidate_socket_id: Option<String>,
    current_document_generation: u64,
    observed_game_socket_count: usize,
    candidate_socket: Option<ControlledReloadSocketSnapshot>,
    browser_owner: bool,
    account_online: bool,
    rust_ws_connected: bool,
    mutation_guard_active: bool,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlledReloadBaseline {
    old_socket_id: String,
    document_generation: u64,
}

#[cfg(debug_assertions)]
fn controlled_reload_precondition_decision(
    input: ControlledReloadPreconditions,
) -> Result<ControlledReloadBaseline, ControlledReloadBlockReason> {
    if !input.validation_mode_active {
        return Err(ControlledReloadBlockReason::ValidationModeInactive);
    }
    if !input.interactive_owner_mode {
        return Err(ControlledReloadBlockReason::WrongSessionMode);
    }
    if !input.managed_target {
        return Err(ControlledReloadBlockReason::UnmanagedTarget);
    }
    if !input.network_covered_from_navigation {
        return Err(ControlledReloadBlockReason::NetworkCoverageMissing);
    }
    if input.unobserved_game_targets != 0 {
        return Err(ControlledReloadBlockReason::UnobservedTargets);
    }
    if input.game_page_count != 1 {
        return Err(ControlledReloadBlockReason::NonUniqueGamePage);
    }
    if input.observed_game_socket_count != 1 {
        return Err(ControlledReloadBlockReason::SocketCountNotOne);
    }
    let Some(socket_id) = input.candidate_socket_id else {
        return Err(ControlledReloadBlockReason::CandidateSocketMissing);
    };
    let Some(socket) = input.candidate_socket else {
        return Err(ControlledReloadBlockReason::CandidateSocketMissing);
    };
    if socket.socket_id != socket_id
        || socket.document_generation != input.current_document_generation
    {
        return Err(ControlledReloadBlockReason::SocketNotCurrentDocument);
    }
    if !socket.handshake_response_received {
        return Err(ControlledReloadBlockReason::HandshakeNotConfirmed);
    }
    if !input.browser_owner || !input.account_online {
        return Err(ControlledReloadBlockReason::BrowserOwnerNotOnline);
    }
    if input.rust_ws_connected {
        return Err(ControlledReloadBlockReason::RustTransportConnected);
    }
    if !input.mutation_guard_active {
        return Err(ControlledReloadBlockReason::MutationGuardInactive);
    }
    Ok(ControlledReloadBaseline {
        old_socket_id: socket_id,
        document_generation: input.current_document_generation,
    })
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlledReloadOutcome {
    Pending,
    Pass,
    DualSocket,
    Blocked,
}

#[cfg(debug_assertions)]
struct ControlledReloadTracker {
    baseline: ControlledReloadBaseline,
    session_id: String,
    document_generation: u64,
    old_socket_closed: bool,
    new_socket_ids: HashSet<String>,
    handshaken_new_socket_ids: HashSet<String>,
    open_game_socket_ids: HashSet<String>,
    new_socket_handshake_confirmed: bool,
    new_socket_created_before_old_close: bool,
    new_handshake_before_old_close: bool,
    dual_socket_proven: bool,
}

#[cfg(debug_assertions)]
impl ControlledReloadTracker {
    fn new(baseline: ControlledReloadBaseline, session_id: &str) -> Self {
        Self {
            document_generation: baseline.document_generation,
            old_socket_closed: false,
            new_socket_ids: HashSet::new(),
            handshaken_new_socket_ids: HashSet::new(),
            open_game_socket_ids: HashSet::from([baseline.old_socket_id.clone()]),
            new_socket_handshake_confirmed: false,
            new_socket_created_before_old_close: false,
            new_handshake_before_old_close: false,
            dual_socket_proven: false,
            baseline,
            session_id: session_id.to_owned(),
        }
    }

    fn observe(&mut self, event: &Value, game_origin: &str) {
        if event.get("sessionId").and_then(Value::as_str) != Some(self.session_id.as_str()) {
            return;
        }
        let method = event.get("method").and_then(Value::as_str);
        match method {
            Some("Page.frameNavigated") => {
                let frame = event.pointer("/params/frame");
                let is_main_frame = frame
                    .and_then(|frame| frame.get("parentId"))
                    .map_or(true, Value::is_null);
                let is_game_page = frame
                    .and_then(|frame| frame.get("url"))
                    .and_then(Value::as_str)
                    .is_some_and(|url| {
                        url == game_origin || url.starts_with(&format!("{game_origin}/"))
                    });
                if is_main_frame && is_game_page {
                    self.document_generation = self.document_generation.saturating_add(1);
                }
            }
            Some("Network.webSocketCreated") => {
                let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
                let url = event
                    .pointer("/params/url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if self.document_generation > self.baseline.document_generation
                    && is_pokeidle_socket(url)
                    && let Some(request_id) = request_id
                    && request_id != self.baseline.old_socket_id
                {
                    if !self.old_socket_closed {
                        self.new_socket_created_before_old_close = true;
                    }
                    self.new_socket_ids.insert(request_id.to_owned());
                }
            }
            Some("Network.webSocketHandshakeResponseReceived") => {
                let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
                let successful_handshake = event
                    .pointer("/params/response/status")
                    .and_then(Value::as_u64)
                    == Some(101);
                if successful_handshake
                    && request_id.is_some_and(|id| self.new_socket_ids.contains(id))
                {
                    if !self.old_socket_closed {
                        self.new_handshake_before_old_close = true;
                    }
                    if self.open_game_socket_ids.iter().any(|open_id| {
                        open_id != &self.baseline.old_socket_id
                            && Some(open_id.as_str()) != request_id
                    }) {
                        self.dual_socket_proven = true;
                    }
                    self.new_socket_handshake_confirmed = true;
                    if let Some(request_id) = request_id {
                        self.handshaken_new_socket_ids.insert(request_id.to_owned());
                        self.open_game_socket_ids.insert(request_id.to_owned());
                    }
                }
            }
            Some("Network.webSocketClosed") => {
                let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
                if request_id == Some(self.baseline.old_socket_id.as_str()) {
                    self.old_socket_closed = true;
                }
                if let Some(request_id) = request_id {
                    self.open_game_socket_ids.remove(request_id);
                }
            }
            Some("Network.webSocketFrameReceived" | "Network.webSocketFrameSent") => {
                let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
                if self.new_socket_handshake_confirmed
                    && request_id == Some(self.baseline.old_socket_id.as_str())
                    && self
                        .open_game_socket_ids
                        .contains(&self.baseline.old_socket_id)
                {
                    // A real frame on the old socket after a successful new
                    // handshake proves both connections were simultaneously live.
                    self.dual_socket_proven = true;
                }
            }
            _ => {}
        }
    }

    fn outcome(&self) -> ControlledReloadOutcome {
        if self.dual_socket_proven {
            ControlledReloadOutcome::DualSocket
        } else if self.old_socket_closed
            && self.document_generation > self.baseline.document_generation
            && !self.new_socket_created_before_old_close
            && !self.new_handshake_before_old_close
            && self.new_socket_handshake_confirmed
            && self.new_socket_ids.len() == 1
            && self.handshaken_new_socket_ids.len() == 1
            && self.open_game_socket_ids.len() == 1
            && self
                .handshaken_new_socket_ids
                .iter()
                .next()
                .is_some_and(|id| self.open_game_socket_ids.contains(id))
        {
            ControlledReloadOutcome::Pass
        } else {
            ControlledReloadOutcome::Pending
        }
    }

    fn finish(&self) -> ControlledReloadOutcome {
        match self.outcome() {
            ControlledReloadOutcome::Pending => ControlledReloadOutcome::Blocked,
            terminal => terminal,
        }
    }
}

#[cfg(debug_assertions)]
fn validation_rust_transport_connected(
    accounts: &AccountManager,
    account_id: &str,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
) -> bool {
    diagnostic.lock().rust_ws_connected
        || accounts.snapshots().into_iter().any(|snapshot| {
            snapshot.account.id == account_id
                && snapshot.account.owner != crate::domain::ConnectionOwner::Browser
        })
        || accounts.transport_diagnostics().into_iter().any(|row| {
            row.account_id == account_id
                && (row.background_transport_attached
                    || row.selected_transport
                        == Some(crate::accounts::AccountDiagnosticTransport::Background))
        })
}

#[cfg(debug_assertions)]
async fn controlled_observed_reload(
    socket: &mut CdpSocket,
    next_id: &mut i64,
    target: &CdpTargetInfo,
    session_id: &str,
    game_origin: &str,
    accounts: &AccountManager,
    account_id: &str,
    mode: BrowserSessionMode,
    network_covered_from_navigation: bool,
    document_generation: u64,
    candidate_socket_id: Option<&str>,
    observed_sockets: &HashMap<String, ObservedSocket>,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    lifecycle_cancellation: &tokio_util::sync::CancellationToken,
) -> ControlledReloadOutcome {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(14);
    update(diagnostic, |status| {
        status.controlled_reload_status = ControlledReloadStatus::NotRequested;
        status.controlled_reload_old_socket_observed = false;
        status.controlled_reload_old_socket_closed = false;
        status.controlled_reload_new_socket_observed = false;
        status.controlled_reload_document_generation_before = None;
        status.controlled_reload_document_generation_after = None;
        status.controlled_reload_rust_ws_ever_connected = false;
    });
    let current_diagnostic = diagnostic.lock().clone();
    let target_is_unique = tokio::time::timeout(
        Duration::from_secs(1).min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        verify_unique_game_target_after_navigation(socket, next_id, &target.id, game_origin),
    )
    .await
    .is_ok_and(|result| result.is_ok());
    *next_id += 1;
    let game_page_count = if target_is_unique { 1 } else { 0 };
    let account = accounts
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.account.id == account_id);
    let transport = accounts
        .transport_diagnostics()
        .into_iter()
        .find(|row| row.account_id == account_id);
    let browser_owner_ready = account.as_ref().is_some_and(|snapshot| {
        snapshot.account.owner == crate::domain::ConnectionOwner::Browser
            && snapshot.account.status == crate::domain::ConnectionStatus::Online
    }) && transport.as_ref().is_some_and(|row| {
        row.owner == crate::domain::ConnectionOwner::Browser
            && row.selected_transport == Some(crate::accounts::AccountDiagnosticTransport::Browser)
            && row.browser_control_attached
            && row.browser_transport_ready
            && !row.background_transport_attached
    });
    let rust_ws_connected = validation_rust_transport_connected(accounts, account_id, diagnostic);
    let candidate_socket = candidate_socket_id.and_then(|id| {
        observed_sockets
            .get(id)
            .map(|observed| ControlledReloadSocketSnapshot {
                socket_id: id.to_owned(),
                document_generation: observed.document_generation,
                handshake_response_received: observed.handshake_response_received,
            })
    });
    let preconditions = ControlledReloadPreconditions {
        validation_mode_active: crate::recovery_validation_mode_active(),
        interactive_owner_mode: is_interactive_owner_mode(mode),
        managed_target: target.target_type == "page" && !target.id.is_empty(),
        network_covered_from_navigation: network_covered_from_navigation
            && current_diagnostic.network_enabled
            && current_diagnostic.game_url_navigated
            && document_generation > 0,
        unobserved_game_targets: if current_diagnostic.reload_guard
            == Some(ReloadGuardReason::UnobservedTarget)
            || !target_is_unique
        {
            1
        } else {
            0
        },
        game_page_count,
        candidate_socket_id: candidate_socket_id.map(str::to_owned),
        current_document_generation: document_generation,
        observed_game_socket_count: observed_sockets.len(),
        candidate_socket,
        browser_owner: browser_owner_ready,
        account_online: account.as_ref().is_some_and(|snapshot| {
            snapshot.account.status == crate::domain::ConnectionStatus::Online
        }),
        rust_ws_connected,
        // request_browser_reload is the only producer of this control event;
        // it checks the account's read-only mutation guard before sending.
        mutation_guard_active: crate::recovery_validation_mode_active(),
    };
    let baseline = match controlled_reload_precondition_decision(preconditions) {
        Ok(baseline) => baseline,
        Err(reason) => {
            update(diagnostic, |status| {
                status.controlled_reload_status = ControlledReloadStatus::PreconditionsBlocked;
                status.controlled_reload_old_socket_observed = false;
                status.controlled_reload_old_socket_closed = false;
                status.controlled_reload_new_socket_observed = false;
                status.controlled_reload_document_generation_before = Some(document_generation);
                status.controlled_reload_document_generation_after = Some(document_generation);
                status.controlled_reload_rust_ws_ever_connected = rust_ws_connected;
                status.reload_guard = if reason == ControlledReloadBlockReason::UnobservedTargets
                    || reason == ControlledReloadBlockReason::NonUniqueGamePage
                {
                    Some(ReloadGuardReason::UnobservedTarget)
                } else {
                    Some(ReloadGuardReason::ActiveGameSocket)
                };
                status.message = format!("Reload controlado bloqueado antes do envio: {reason:?}.");
            });
            return ControlledReloadOutcome::Blocked;
        }
    };

    // Only this validation-only operation clears a stale generic guard for
    // reporting. The global `page_reload_safety_decision` remains untouched.
    update(diagnostic, |status| {
        status.reload_guard = None;
        status.controlled_reload_status = ControlledReloadStatus::Reloading;
        status.controlled_reload_old_socket_observed = true;
        status.controlled_reload_old_socket_closed = false;
        status.controlled_reload_new_socket_observed = false;
        status.controlled_reload_document_generation_before = Some(baseline.document_generation);
        status.controlled_reload_document_generation_after = Some(baseline.document_generation);
        status.controlled_reload_rust_ws_ever_connected = rust_ws_connected;
        status.message = "Reload controlado iniciado na mesma target; aguardando close, novo documento e novo handshake CDP.".into();
    });

    let reload_command = tokio::time::timeout_at(
        deadline,
        cdp_command(
            socket,
            *next_id,
            "Page.reload",
            json!({"ignoreCache": false}),
            Some(session_id),
        ),
    )
    .await;
    *next_id += 1;
    let (_, mut command_events) = match reload_command {
        Ok(Ok(reply)) => reply,
        _ => {
            update(diagnostic, |status| {
                status.controlled_reload_status = ControlledReloadStatus::Blocked;
                status.message =
                    "Reload controlado bloqueado: CDP não confirmou Page.reload.".into();
            });
            return ControlledReloadOutcome::Blocked;
        }
    };
    let mut tracker = ControlledReloadTracker::new(baseline, session_id);
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut events = Vec::new();
    let mut rust_ws_ever_connected = rust_ws_connected;
    for event in &command_events {
        tracker.observe(event, game_origin);
        rust_ws_ever_connected |=
            validation_rust_transport_connected(accounts, account_id, diagnostic);
        if rust_ws_ever_connected {
            update(diagnostic, |status| {
                status.controlled_reload_rust_ws_ever_connected = true
            });
        }
    }
    let outcome = loop {
        if rust_ws_ever_connected {
            update(diagnostic, |status| {
                status.controlled_reload_rust_ws_ever_connected = true
            });
            break ControlledReloadOutcome::Blocked;
        }
        match tracker.outcome() {
            ControlledReloadOutcome::Pending => {}
            terminal => break terminal,
        }
        if tokio::time::Instant::now() >= deadline {
            break tracker.finish();
        }
        tokio::select! {
            _ = lifecycle_cancellation.cancelled() => break tracker.finish(),
            _ = interval.tick() => {
                rust_ws_ever_connected |= validation_rust_transport_connected(accounts, account_id, diagnostic);
                if rust_ws_ever_connected {
                    update(diagnostic, |status| status.controlled_reload_rust_ws_ever_connected = true);
                }
            }
            message = socket.next() => {
                let Some(message) = message else {
                    break tracker.finish();
                };
                let Ok(message) = message else {
                    break tracker.finish();
                };
                let Message::Text(raw) = message else {
                    continue;
                };
                let Ok(event) = serde_json::from_str::<Value>(&raw) else {
                    break tracker.finish();
                };
                tracker.observe(&event, game_origin);
                events.push(event);
                rust_ws_ever_connected |= validation_rust_transport_connected(accounts, account_id, diagnostic);
                if rust_ws_ever_connected {
                    update(diagnostic, |status| status.controlled_reload_rust_ws_ever_connected = true);
                }
                match tracker.outcome() {
                    ControlledReloadOutcome::Pending => {}
                    terminal => break terminal,
                }
            }
            _ = tokio::time::sleep_until(deadline) => break tracker.finish(),
        }
    };
    command_events.append(&mut events);
    for event in command_events.into_iter().rev() {
        socket.pending_events.push_front(event);
    }
    let old_closed = tracker.old_socket_closed;
    let new_open = tracker.new_socket_handshake_confirmed;
    let generation_after = tracker.document_generation;
    let outcome = if rust_ws_ever_connected {
        ControlledReloadOutcome::Blocked
    } else {
        outcome
    };
    update(diagnostic, |status| {
        status.controlled_reload_old_socket_closed = old_closed;
        status.controlled_reload_new_socket_observed = new_open;
        status.controlled_reload_document_generation_after = Some(generation_after);
        status.controlled_reload_rust_ws_ever_connected = rust_ws_ever_connected;
        status.controlled_reload_status = match outcome {
            ControlledReloadOutcome::Pending | ControlledReloadOutcome::Blocked => {
                ControlledReloadStatus::Blocked
            }
            ControlledReloadOutcome::Pass => ControlledReloadStatus::Pass,
            ControlledReloadOutcome::DualSocket => ControlledReloadStatus::DualSocket,
        };
        status.message = match outcome {
            ControlledReloadOutcome::Pass => "Reload controlado comprovado: socket antigo fechou, novo documento carregou e novo handshake abriu exatamente um socket.".into(),
            ControlledReloadOutcome::DualSocket => "Falha real: frame no socket antigo foi observado após o handshake bem-sucedido do socket novo.".into(),
            ControlledReloadOutcome::Pending | ControlledReloadOutcome::Blocked if rust_ws_ever_connected => "Reload bloqueado: foi observado transporte Rust/Background durante a janela controlada.".into(),
            ControlledReloadOutcome::Pending | ControlledReloadOutcome::Blocked => "Reload bloqueado/indeterminado: a sequência de close, novo documento e handshake único não foi comprovada em 12 segundos.".into(),
        };
    });
    outcome
}

fn main_frame_game_navigation_generation(
    event: &Value,
    session_id: &str,
    game_origin: &str,
) -> bool {
    if event.get("sessionId").and_then(Value::as_str) != Some(session_id)
        || event.get("method").and_then(Value::as_str) != Some("Page.frameNavigated")
    {
        return false;
    }
    let frame = event.pointer("/params/frame");
    let is_main_frame = frame
        .and_then(|frame| frame.get("parentId"))
        .map_or(true, Value::is_null);
    let is_game_page = frame
        .and_then(|frame| frame.get("url"))
        .and_then(Value::as_str)
        .is_some_and(|url| url == game_origin || url.starts_with(&format!("{game_origin}/")));
    is_main_frame && is_game_page
}

fn tracked_game_socket_closed(
    request_id: Option<&str>,
    sockets: &HashMap<String, ObservedSocket>,
    recovery_socket_ids: &HashSet<String>,
) -> bool {
    request_id.is_some_and(|request_id| {
        sockets.contains_key(request_id) || recovery_socket_ids.contains(request_id)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconnectGraceDecision {
    BrowserBridgeReadyAwaitRustWelcome,
    CloseTargetThenFallback,
    FallbackAfterTargetDestroyed,
    StopWithoutFallback,
}

fn reconnect_grace_decision(
    bridge_ready: bool,
    target_exists: Option<bool>,
) -> ReconnectGraceDecision {
    // This query runs in the exact managed target/session. The bridge only
    // reports ready after observing the game's hello -> welcome exchange.
    // Requiring the Rust-side candidate cache as well creates a false negative
    // when corresponding CDP events are queued behind this command reply.
    if bridge_ready {
        ReconnectGraceDecision::BrowserBridgeReadyAwaitRustWelcome
    } else {
        match target_exists {
            Some(true) => ReconnectGraceDecision::CloseTargetThenFallback,
            Some(false) => ReconnectGraceDecision::FallbackAfterTargetDestroyed,
            None => ReconnectGraceDecision::StopWithoutFallback,
        }
    }
}

async fn close_managed_target(
    socket: &mut CdpSocket,
    next_id: i64,
    target_id: &str,
) -> Result<(), BrowserError> {
    let (result, _) = cdp_command(
        socket,
        next_id,
        "Target.closeTarget",
        json!({"targetId": target_id}),
        None,
    )
    .await?;
    if result.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(BrowserError::Launch(
            "CDP não confirmou o encerramento da aba gerenciada.".into(),
        ));
    }
    Ok(())
}

fn notify_browser_owner_lost(
    accounts: &AccountManager,
    account_id: &str,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    owner_loss_sender: &Option<tokio::sync::mpsc::UnboundedSender<BrowserOwnerLost>>,
    reason: impl Into<String>,
    bootstrap: Option<ConnectionBootstrap>,
) -> bool {
    let Some(sender) = owner_loss_sender.as_ref() else {
        return false;
    };
    let reason = reason.into();
    let Ok(mut started) = accounts.browser_owner_lost(account_id, reason.clone()) else {
        return false;
    };
    if !started {
        started = accounts
            .begin_browser_recovery(account_id, reason.clone())
            .unwrap_or(false);
    }
    if !started {
        return false;
    }
    update(diagnostic, |status| {
        status.browser_ws_connected = false;
        status.background_active = false;
        status.state = "Reconectando".into();
        status.message = format!("{reason}; retomando a conta em Background.");
    });
    let _ = sender.send(BrowserOwnerLost { reason, bootstrap });
    true
}

fn bootstrap_for(
    account_id: &str,
    request_id: Option<&str>,
    sockets: &HashMap<String, ObservedSocket>,
) -> Option<ConnectionBootstrap> {
    let socket = sockets.get(request_id?)?;
    Some(ConnectionBootstrap {
        account_id: account_id.to_owned(),
        ws_url: socket.ws_url.clone(),
        headers: socket.headers.clone()?,
        hello: socket.hello.clone()?,
    })
}

fn observe_page_event(
    accounts: &AccountManager,
    account_id: &str,
    mode: BrowserSessionMode,
    event: &Value,
    session_id: &str,
    game_origin: &str,
    document_generation: u64,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    candidate_socket_id: &mut Option<String>,
    sockets: &mut HashMap<String, ObservedSocket>,
    recovery_socket_ids: &mut HashSet<String>,
) -> Option<ServerFrame> {
    if event.get("sessionId").and_then(Value::as_str) != Some(session_id) {
        return None;
    }
    match event.get("method").and_then(Value::as_str) {
        Some("Page.frameNavigated") => {
            let url = event.pointer("/params/frame/url").and_then(Value::as_str);
            if let Some(url) = url.filter(|url| url.starts_with(game_origin)) {
                update(diagnostic, |status| {
                    status.final_url = Some(url.to_owned());
                    status.game_url_navigated = true;
                    status.lifecycle = BrowserLifecycle::LoadingGame;
                    status.state = BrowserLifecycle::LoadingGame.label().into();
                    status.message = "GAME_URL carregada na aba gerenciada.".into();
                });
            }
        }
        Some("Network.webSocketCreated") => {
            let socket_url = event
                .pointer("/params/url")
                .and_then(Value::as_str)
                .unwrap_or_default();
            update(diagnostic, |status| {
                status.websocket_count += 1;
                if is_pokeidle_socket(socket_url) {
                    status.game_ws_created_count = status.game_ws_created_count.saturating_add(1);
                    status.game_ws_open_count = status.game_ws_open_count.saturating_add(1);
                    status.pokeidle_socket_detected = true;
                    status.lifecycle = BrowserLifecycle::WaitingForHello;
                    status.state = BrowserLifecycle::WaitingForHello.label().into();
                }
                status.message = "WebSocket observado; verificando o protocolo do jogo.".into();
            });
            if let Some(request_id) = event.pointer("/params/requestId").and_then(Value::as_str) {
                if is_pokeidle_socket(socket_url) {
                    if matches!(mode, BrowserSessionMode::RecoveryProbe) {
                        recovery_socket_ids.insert(request_id.to_owned());
                    } else {
                        sockets.insert(
                            request_id.to_owned(),
                            ObservedSocket {
                                ws_url: socket_url.to_owned(),
                                headers: None,
                                hello: None,
                                document_generation,
                                handshake_response_received: false,
                            },
                        );
                    }
                }
            }
        }
        Some("Network.webSocketWillSendHandshakeRequest") => {
            if !matches!(mode, BrowserSessionMode::RecoveryProbe)
                && let Some(request_id) = event.pointer("/params/requestId").and_then(Value::as_str)
                && let Some(socket) = sockets.get_mut(request_id)
            {
                socket.headers = Some(SessionHeaders::from_cdp(
                    event
                        .pointer("/params/request/headers")
                        .unwrap_or(&Value::Null),
                ));
                update(diagnostic, |status| {
                    status.ws_url_captured = true;
                    status.session_material_captured = true;
                });
            }
        }
        Some("Network.webSocketHandshakeResponseReceived") => {
            let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
            let successful_handshake = event
                .pointer("/params/response/status")
                .and_then(Value::as_u64)
                == Some(101);
            if successful_handshake
                && let Some(request_id) = request_id
                && let Some(socket) = sockets.get_mut(request_id)
            {
                socket.handshake_response_received = true;
            }
        }
        Some("Network.webSocketClosed") => {
            let request_id = event.pointer("/params/requestId").and_then(Value::as_str);
            let tracked_game_socket =
                tracked_game_socket_closed(request_id, &sockets, &recovery_socket_ids);
            if tracked_game_socket {
                update(diagnostic, |status| {
                    status.game_ws_closed_count = status.game_ws_closed_count.saturating_add(1);
                    status.game_ws_open_count = status.game_ws_open_count.saturating_sub(1);
                });
            }
            if candidate_socket_id.as_deref() == request_id {
                *candidate_socket_id = None;
                if !matches!(mode, BrowserSessionMode::RecoveryProbe) {
                    let _ = accounts.set_browser_transport_ready(account_id, false);
                    let _ = accounts.clear_uncertain_game_state(account_id);
                }
                update(diagnostic, |status| {
                    status.message = "WebSocket do jogo foi fechado; aguardando reconexão.".into();
                });
            }
            if let Some(request_id) = request_id {
                sockets.remove(request_id);
                recovery_socket_ids.remove(request_id);
            }
        }
        Some("Network.webSocketFrameSent") => {
            let Some(payload) = event
                .pointer("/params/response/payloadData")
                .and_then(Value::as_str)
            else {
                return None;
            };
            if !matches!(mode, BrowserSessionMode::RecoveryProbe) {
                accounts.record_protocol_frame(
                    account_id,
                    ProtocolDirection::ClientToServer,
                    payload,
                );
            }
            let Ok(frame) = serde_json::from_str::<Value>(payload) else {
                return None;
            };
            if frame.get("t").and_then(Value::as_str) != Some("hello") {
                return None;
            }
            let Some(request_id) = event
                .pointer("/params/requestId")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                return None;
            };
            if matches!(mode, BrowserSessionMode::RecoveryProbe)
                && !recovery_socket_ids.contains(&request_id)
            {
                return None;
            }
            *candidate_socket_id = Some(request_id);
            if !matches!(mode, BrowserSessionMode::RecoveryProbe)
                && let (Some(request_id), Ok(hello)) = (
                    candidate_socket_id.as_deref(),
                    serde_json::from_value::<Hello>(frame.clone()),
                )
                && let Some(socket) = sockets.get_mut(request_id)
            {
                socket.hello = Some(hello);
            }
            let nick = (!matches!(mode, BrowserSessionMode::RecoveryProbe))
                .then(|| frame.get("nick").and_then(Value::as_str).map(str::to_owned))
                .flatten();
            update(diagnostic, |status| {
                status.websocket_detected = true;
                status.hello_detected = true;
                status.nick = nick;
                status.lifecycle = BrowserLifecycle::WaitingForWelcome;
                status.state = BrowserLifecycle::WaitingForWelcome.label().into();
                status.message = "hello detectado; aguardando welcome.".into();
            });
            if !matches!(mode, BrowserSessionMode::RecoveryProbe) {
                tracing::info!(frame = %sanitize_frame(payload), "CDP hello observed");
            }
        }
        Some("Network.webSocketFrameReceived") => {
            if candidate_socket_id.as_deref()
                == event.pointer("/params/requestId").and_then(Value::as_str)
                && let Some(payload) = event
                    .pointer("/params/response/payloadData")
                    .and_then(Value::as_str)
            {
                if !matches!(mode, BrowserSessionMode::RecoveryProbe) {
                    accounts.record_protocol_frame(
                        account_id,
                        ProtocolDirection::ServerToClient,
                        payload,
                    );
                }
                return ServerFrame::parse(payload).ok();
            }
        }
        _ => {}
    }
    None
}

fn ingest_browser_frame(
    accounts: &AccountManager,
    database: &Arc<Mutex<rusqlite::Connection>>,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    account_id: &str,
    mode: BrowserSessionMode,
    frame: ServerFrame,
) -> bool {
    if let ServerFrame::Welcome(welcome) = &frame {
        if matches!(mode, BrowserSessionMode::ReconnectExistingOwner) {
            let expected_nick = accounts
                .snapshots()
                .into_iter()
                .find(|snapshot| snapshot.account.id == account_id)
                .map(|snapshot| snapshot.account.nick);
            let received_nick = welcome.estado.nick.as_deref().map(str::trim);
            if !expected_nick
                .as_deref()
                .zip(received_nick)
                .is_some_and(|(expected, received)| {
                    !received.is_empty() && expected.eq_ignore_ascii_case(received)
                })
            {
                let reason = "A identidade recebida do Brave não corresponde à conta selecionada; o owner não foi transferido.";
                accounts.browser_control_unavailable(account_id, reason.into());
                update(diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = reason.into();
                });
                return false;
            }
        }
        if let Some(nick) = welcome.estado.nick.as_deref()
            && let Err(error) = accounts.apply_authoritative_nick(account_id, nick)
        {
            let _ = accounts.transition(account_id, AccountRuntimeState::Error);
            update(diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Error;
                status.state = BrowserLifecycle::Error.label().into();
                status.message = format!(
                    "Esta identidade já está cadastrada no Manager ({error}). O perfil novo não foi registrado."
                );
            });
            return false;
        }
    }
    let welcome = matches!(frame, ServerFrame::Welcome(_));
    if accounts.ingest(account_id, frame).is_err() {
        return false;
    }
    if !welcome {
        return true;
    }
    let account = accounts
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.account.id == account_id)
        .map(|snapshot| snapshot.account);
    if let Some(account) = &account {
        // This is local display metadata only. A temporary account is inserted
        // only after welcome gives us its authoritative identity; session
        // material never reaches SQLite.
        let _ = database.lock().execute(
            "INSERT INTO accounts(id, nick, local_alias, card_color, created_at, last_active_at) VALUES (?1, ?2, ?3, ?4, unixepoch(), unixepoch()) ON CONFLICT(id) DO UPDATE SET nick = excluded.nick, local_alias = excluded.local_alias, card_color = excluded.card_color, last_active_at = unixepoch()",
            (&account.id, &account.nick, &account.local_alias, &account.card_color),
        );
    }
    let _ = accounts.transition(account_id, AccountRuntimeState::BrowserConnected);
    update(diagnostic, |status| {
        status.welcome_received = true;
        status.nick = account.as_ref().map(|account| account.nick.clone());
        status.account_persisted = account.is_some();
        status.lifecycle = BrowserLifecycle::Authenticated;
        status.state = BrowserLifecycle::Authenticated.label().into();
        status.message = "welcome recebido pelo navegador; dados reais estão no Dashboard.".into();
    });
    tracing::info!(account_id, "startup first welcome");
    true
}

fn capture_recovery_welcome(
    frame: &ServerFrame,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
) -> bool {
    let ServerFrame::Welcome(welcome) = frame else {
        return false;
    };
    update(diagnostic, |status| {
        status.welcome_received = true;
        status.nick = welcome
            .estado
            .nick
            .as_deref()
            .map(str::trim)
            .filter(|nick| !nick.is_empty())
            .map(str::to_owned);
        status.account_persisted = false;
        status.background_active = false;
        status.lifecycle = BrowserLifecycle::Authenticated;
        status.state = "Welcome autoritativo recebido".into();
        status.message = "Identidade capturada do welcome do servidor.".into();
    });
    true
}

fn cdp_endpoint_contains_owned_pid(process_info: &Value, expected_pid: u32) -> bool {
    process_info
        .get("processInfo")
        .and_then(Value::as_array)
        .is_some_and(|processes| {
            processes.iter().any(|process| {
                process.get("id").and_then(Value::as_u64) == Some(u64::from(expected_pid))
            })
        })
}

async fn close_recovery_browser(
    socket: &mut CdpSocket,
    next_id: i64,
    process_slot: &RecoveryProcessSlot,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
) -> Result<(), BrowserError> {
    transition(
        diagnostic,
        BrowserLifecycle::Closing,
        "Encerrando a instância Brave do probe.",
    );
    let close_result = cdp_command(socket, next_id, "Browser.close", json!({}), None).await;
    let Some(mut child) = process_slot.lock().await.take() else {
        if close_result.is_ok() {
            update(diagnostic, |status| {
                status.browser_closed = true;
                status.lifecycle = BrowserLifecycle::Closed;
                status.state = BrowserLifecycle::Closed.label().into();
                status.message = "Brave do probe encerrado.".into();
            });
            return Ok(());
        }
        return close_result.map(|_| ());
    };
    let graceful_exit = match close_result {
        Ok(_) => tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_ok_and(|result| result.is_ok()),
        Err(_) => false,
    };
    if graceful_exit {
        update(diagnostic, |status| {
            status.browser_closed = true;
            status.controlled_brave_pid = None;
            status.lifecycle = BrowserLifecycle::Closed;
            status.state = BrowserLifecycle::Closed.label().into();
            status.message = "Brave do probe encerrado e processo confirmado.".into();
        });
        return Ok(());
    }

    // Only this probe's owned Child handle is eligible for an emergency stop.
    // Never kill by process name: the user's personal Brave stays untouched.
    let reaped = terminate_recovery_process_tree(&mut child).await;
    update(diagnostic, |status| {
        status.browser_closed = false;
        status.controlled_brave_pid = None;
        status.lifecycle = BrowserLifecycle::Error;
        status.state = BrowserLifecycle::Error.label().into();
        status.message =
            "O fechamento CDP não foi confirmado; processo do probe encerrado pelo handle próprio."
                .into();
    });
    if reaped {
        Err(BrowserError::Launch(
            "controlled Brave required owned-process termination".into(),
        ))
    } else {
        Err(BrowserError::Launch(
            "controlled Brave process could not be reaped".into(),
        ))
    }
}

async fn wait_managed_browser_exit(
    child: &mut tokio::process::Child,
    timeout: Duration,
) -> Result<(), BrowserError> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(BrowserError::Launch(format!(
            "Brave controlado encerrou com status {status}"
        ))),
        Ok(Err(error)) => Err(BrowserError::Launch(format!(
            "não foi possível aguardar o Brave controlado: {error}"
        ))),
        Err(_) => Err(BrowserError::Launch(
            "timeout aguardando o Brave controlado encerrar normalmente".into(),
        )),
    }
}

/// The browser remains open until this returns success: `ConnectionManager`
/// only returns after Rust has sent hello and received a real welcome.
async fn handoff_to_background(
    socket: &mut CdpSocket,
    next_id: i64,
    target_id: &str,
    bootstrap: ConnectionBootstrap,
    accounts: &AccountManager,
    account_id: &str,
    diagnostic: &Arc<Mutex<IntegrationDiagnostic>>,
    lifecycle_cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(), HandoffFailure> {
    let _ = accounts.transition(account_id, AccountRuntimeState::PreparingHandoff);
    update(diagnostic, |status| {
        status.lifecycle = BrowserLifecycle::ReadyForHandoff;
        status.state = BrowserLifecycle::ReadyForHandoff.label().into();
        status.message =
            "Fechando a aba do jogo antes de transferir o WebSocket para o Rust.".into();
    });
    close_managed_target(socket, next_id, target_id)
        .await
        .map_err(|error| HandoffFailure::BrowserStillActive(error.to_string()))?;
    let _ = accounts.transition(account_id, AccountRuntimeState::BackgroundConnecting);
    let connection = tokio::select! {
        _ = lifecycle_cancellation.cancelled() => {
            return Err(HandoffFailure::BrowserTargetClosed("handoff cancelado pelo lifecycle da conta".into()));
        }
        result = tokio::time::timeout(
            Duration::from_secs(25),
            ConnectionManager::connect(bootstrap, accounts.clone()),
        ) => match result {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => return Err(HandoffFailure::BrowserTargetClosed(error.to_string())),
            Err(_) => return Err(HandoffFailure::BrowserTargetClosed("timeout aguardando welcome do Rust".into())),
        }
    };
    update(diagnostic, |status| {
        status.rust_ws_connected = true;
        status.rust_hello_sent = true;
        status.rust_welcome_received = true;
        status.lifecycle = BrowserLifecycle::Closing;
        status.state = BrowserLifecycle::Closing.label().into();
        status.message = "welcome do Rust confirmado; encerrando Brave controlado.".into();
    });
    accounts
        .attach_background(account_id, connection)
        .map_err(|error| HandoffFailure::BrowserTargetClosed(error.to_string()))?;
    match cdp_command(socket, next_id, "Browser.close", json!({}), None).await {
        Ok(_) => update(diagnostic, |status| {
            status.browser_closed = true;
            status.background_active = true;
            status.lifecycle = BrowserLifecycle::Closed;
            status.state = "Background".into();
            status.message = "Brave controlado encerrado; conta ativa em background Rust.".into();
        }),
        Err(error) => update(diagnostic, |status| {
            status.background_active = true;
            status.message =
                format!("Rust está em background, mas o encerramento do Brave falhou: {error}");
        }),
    }
    Ok(())
}

/// Starts one dedicated Brave session and observes the game page that Brave opens directly.
pub async fn start_observer(
    base: PathBuf,
    account_id: String,
    game_url: String,
    mode: BrowserSessionMode,
    accounts: AccountManager,
    database: Arc<Mutex<rusqlite::Connection>>,
    diagnostic: Arc<Mutex<IntegrationDiagnostic>>,
    owner_loss_sender: Option<tokio::sync::mpsc::UnboundedSender<BrowserOwnerLost>>,
    lifecycle_cancellation: tokio_util::sync::CancellationToken,
    recovery_process: Option<RecoveryProcessSlot>,
    resolved_executable: Option<PathBuf>,
) {
    if lifecycle_cancellation.is_cancelled() {
        return;
    }
    let validation_mode_active = crate::recovery_validation_mode_active();
    let bridge_plan = browser_bridge_startup_plan(mode, validation_mode_active);
    if matches!(mode, BrowserSessionMode::RecoveryProbe) && recovery_process.is_none() {
        transition(
            &diagnostic,
            BrowserLifecycle::Error,
            "Recovery process ownership missing.",
        );
        return;
    }
    macro_rules! recovery_phase {
        ($future:expr) => {{
            if matches!(mode, BrowserSessionMode::RecoveryProbe) {
                tokio::select! {
                    _ = lifecycle_cancellation.cancelled() => {
                        transition(
                            &diagnostic,
                            BrowserLifecycle::Closing,
                            "Probe cancelado; encerrando somente o Brave controlado.",
                        );
                        return;
                    }
                    value = $future => value,
                }
            } else {
                $future.await
            }
        }};
    }
    let manager = WindowsBraveManager::new(base);
    let recovery_process_slot = recovery_process.clone();
    let profile = manager.profile_for(&account_id);
    update(&diagnostic, |status| {
        *status = IntegrationDiagnostic {
            account_id: Some(account_id.clone()),
            account_persisted: false,
            lifecycle: BrowserLifecycle::Starting,
            session_mode: Some(mode),
            state: BrowserLifecycle::Starting.label().into(),
            message: "Procurando Brave...".into(),
            ..Default::default()
        };
    });
    let executable = match resolved_executable
        .map(Ok)
        .unwrap_or_else(|| manager.detect_brave())
    {
        Ok(path) => {
            update(&diagnostic, |status| {
                status.brave_found = true;
                status.message = "Brave encontrado.".into();
            });
            path
        }
        Err(error) => {
            update(&diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Error;
                status.state = BrowserLifecycle::Error.label().into();
                status.message = error.to_string();
            });
            return;
        }
    };
    if let Err(error) = std::fs::create_dir_all(&profile.path) {
        update(&diagnostic, |status| {
            status.lifecycle = BrowserLifecycle::Error;
            status.state = BrowserLifecycle::Error.label().into();
            status.message = format!("Não foi possível criar o perfil isolado: {error}");
        });
        return;
    }
    update(&diagnostic, |status| {
        status.profile_created = true;
        status.persistent_profile = true;
    });

    let cdp_client = match cdp_http_client() {
        Ok(client) => client,
        Err(error) => {
            update(&diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Error;
                status.state = BrowserLifecycle::Error.label().into();
                status.message = error.to_string();
            });
            return;
        }
    };

    // This decision is local to this account. The diagnostic object is shared
    // for display, so it must never decide whether another account's Brave
    // process can be reused during concurrent startup.
    let (port, reused_controlled_browser) = if matches!(
        mode,
        BrowserSessionMode::ReconnectExistingOwner
    ) {
        let Some(port) = manager.saved_cdp_port(&profile) else {
            transition(
                &diagnostic,
                BrowserLifecycle::Error,
                "Não foi encontrada uma instância Brave previamente controlada para esta conta. Nenhum navegador novo foi iniciado.",
            );
            return;
        };
        match fetch_cdp_version(&cdp_client, port).await {
            // Brave currently reports a `Chrome/...` product string from
            // `/json/version`. Do not use that branding as ownership proof;
            // the exact controlled PID is verified through CDP below before
            // any target is changed.
            Ok(_version) => {
                update(&diagnostic, |status| {
                    status.brave_started = true;
                    status.cdp_port = Some(port);
                    status.message = "Endpoint CDP da sessão existente respondeu; confirmando a identidade do processo antes de reconectar.".into();
                });
                (port, true)
            }
            Err(error) => {
                transition(
                    &diagnostic,
                    BrowserLifecycle::Error,
                    format!(
                        "A instância Brave anterior não respondeu na porta CDP salva ({:?}). Feche a aba antiga e reinicie o Manager para uma recuperação segura; nenhum navegador novo foi iniciado.",
                        error.kind
                    ),
                );
                return;
            }
        }
    } else if matches!(mode, BrowserSessionMode::RecoveryProbe) {
        match free_local_port() {
            Ok(port) => (port, false),
            Err(error) => {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = error.to_string();
                });
                return;
            }
        }
    } else if validation_mode_active {
        if let Some(saved_port) = manager.saved_cdp_port(&profile) {
            if fetch_cdp_version(&cdp_client, saved_port).await.is_ok() {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = "Validação segura recusada: já existe uma instância Brave ativa para este perfil. Feche-a antes de validar.".into();
                });
                return;
            }
        }
        match free_local_port() {
            Ok(port) => (port, false),
            Err(error) => {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = error.to_string();
                });
                return;
            }
        }
    } else if let Some(port) = manager.saved_cdp_port(&profile) {
        if fetch_cdp_version(&cdp_client, port).await.is_ok() {
            update(&diagnostic, |status| {
                status.brave_started = true;
                status.cdp_port = Some(port);
                status.message = "Instância controlada do Brave reutilizada.".into();
            });
            (port, true)
        } else {
            match free_local_port() {
                Ok(port) => (port, false),
                Err(error) => {
                    update(&diagnostic, |status| {
                        status.lifecycle = BrowserLifecycle::Error;
                        status.state = BrowserLifecycle::Error.label().into();
                        status.message = error.to_string();
                    });
                    return;
                }
            }
        }
    } else {
        match free_local_port() {
            Ok(port) => (port, false),
            Err(error) => {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = error.to_string();
                });
                return;
            }
        }
    };

    let mut controlled_process: Option<tokio::process::Child> = None;
    if !reused_controlled_browser {
        let launch_url = initial_browser_url(&game_url, bridge_plan.start_blank);
        match manager.launch_controlled(&executable, &profile, port, launch_url, mode) {
            Ok(child) => {
                let pid = child.id();
                if let Some(pid) = pid
                    && let Err(error) = manager.save_cdp_pid(&profile, pid)
                {
                    tracing::warn!(account_id, %error, "could not persist controlled Brave process identity");
                }
                if let Some(process_slot) = recovery_process.as_ref() {
                    *process_slot.lock().await = Some(child);
                } else {
                    controlled_process = Some(child);
                }
                update(&diagnostic, |status| {
                    status.brave_started = true;
                    status.controlled_brave_pid = pid;
                    status.cdp_port = Some(port);
                    status.lifecycle = BrowserLifecycle::ConnectingCdp;
                    status.state = BrowserLifecycle::ConnectingCdp.label().into();
                    status.message = if launch_url.is_none() {
                        format!("Brave iniciado sem target de startup na porta CDP {port}.")
                    } else {
                        format!("Brave iniciado diretamente em GAME_URL na porta CDP {port}.")
                    };
                });
            }
            Err(error) => {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = error.to_string();
                });
                return;
            }
        }
    }

    let browser_version =
        match recovery_phase!(wait_for_cdp_endpoint(&cdp_client, port, &diagnostic)) {
            Ok(version) => version,
            Err(error) => {
                update(&diagnostic, |status| {
                    status.lifecycle = BrowserLifecycle::Error;
                    status.state = BrowserLifecycle::Error.label().into();
                    status.message = error.to_string();
                });
                return;
            }
        };
    if !matches!(mode, BrowserSessionMode::RecoveryProbe) {
        if let Err(error) = manager.save_cdp_port(&profile, port) {
            update(&diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Error;
                status.state = BrowserLifecycle::Error.label().into();
                status.message = error.to_string();
            });
            return;
        }
    }
    update(&diagnostic, |status| {
        status.lifecycle = BrowserLifecycle::LoadingGame;
        status.state = BrowserLifecycle::LoadingGame.label().into();
        status.message = if bridge_plan.start_blank {
            "Endpoint CDP disponível; associando about:blank antes de carregar o jogo.".into()
        } else {
            "Endpoint CDP disponível; localizando a página aberta diretamente pelo Brave.".into()
        };
    });

    let (stream, _) = match recovery_phase!(connect_async(browser_version.web_socket_debugger_url))
    {
        Ok(connection) => connection,
        Err(error) => {
            update(&diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Error;
                status.state = BrowserLifecycle::Error.label().into();
                status.message = format!("Falha ao conectar ao Browser WebSocket CDP: {error}");
            });
            return;
        }
    };
    let mut socket = CdpSocket {
        stream,
        pending_events: VecDeque::new(),
    };
    update(&diagnostic, |status| {
        status.browser_ws_connected = true;
        status.message = "Browser WebSocket CDP conectado; procurando a página do jogo.".into();
    });

    let mut next_id = 1;
    if bridge_plan.read_only || matches!(mode, BrowserSessionMode::ReconnectExistingOwner) {
        let owned_pid = if matches!(mode, BrowserSessionMode::ReconnectExistingOwner) {
            manager.saved_cdp_pid(&profile)
        } else if let Some(process_slot) = recovery_process_slot.as_ref() {
            process_slot
                .lock()
                .await
                .as_ref()
                .and_then(|child| child.id())
        } else {
            controlled_process.as_ref().and_then(|child| child.id())
        };
        let Some(owned_pid) = owned_pid else {
            transition(
                &diagnostic,
                BrowserLifecycle::Error,
                "A identidade do processo Brave não pôde ser confirmada; nenhuma aba foi alterada.",
            );
            return;
        };
        let process_info = match recovery_phase!(cdp_command(
            &mut socket,
            next_id,
            "SystemInfo.getProcessInfo",
            json!({}),
            None,
        )) {
            Ok((result, _)) => result,
            Err(_) => {
                transition(
                    &diagnostic,
                    BrowserLifecycle::Error,
                    "Could not verify ownership of the local CDP endpoint.",
                );
                return;
            }
        };
        if !cdp_endpoint_contains_owned_pid(&process_info, owned_pid) {
            transition(
                &diagnostic,
                BrowserLifecycle::Error,
                "A porta CDP salva não pertence ao processo Brave desta conta; nenhuma aba foi alterada.",
            );
            return;
        }
        next_id += 1;
    }
    if let Err(error) = recovery_phase!(cdp_command(
        &mut socket,
        next_id,
        "Target.setDiscoverTargets",
        json!({"discover": true}),
        None,
    )) {
        transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
        return;
    }
    next_id += 1;
    let gate_before_navigation = bridge_plan.navigate_after_gate;
    let mut restored_target_adopted = false;
    let mut manager_created_target_id: Option<String> = None;
    macro_rules! close_manager_target_on_failure {
        () => {
            if let Some(target_id) = manager_created_target_id.take() {
                let _ = close_manager_created_target(&mut socket, &mut next_id, &target_id).await;
            }
        };
    }
    let mut target = if gate_before_navigation {
        let (targets_result, _) = match recovery_phase!(cdp_command(
            &mut socket,
            next_id,
            "Target.getTargets",
            json!({}),
            None,
        )) {
            Ok(result) => result,
            Err(error) => {
                transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                return;
            }
        };
        next_id += 1;
        let targets = match targets_from(&targets_result) {
            Ok(targets) => targets,
            Err(error) => {
                transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                return;
            }
        };
        let blank_target = targets
            .iter()
            .find(|target| target.target_type == "page" && target.url == "about:blank")
            .cloned();
        let mut game_targets: HashMap<String, CdpTargetInfo> = targets
            .iter()
            .filter(|target| is_game_page(target, &game_origin(&game_url)))
            .cloned()
            .map(|target| (target.id.clone(), target))
            .collect();
        for target in take_game_target_events(&mut socket.pending_events, &game_origin(&game_url)) {
            game_targets.entry(target.id.clone()).or_insert(target);
        }
        let game_targets: Vec<_> = game_targets.into_values().collect();
        match gated_startup_target_decision(blank_target.is_some(), game_targets.len()) {
            GatedStartupTargetDecision::UseBlankTarget => blank_target.unwrap(),
            GatedStartupTargetDecision::AdoptRestoredGameTarget => {
                restored_target_adopted = true;
                game_targets.into_iter().next().unwrap()
            }
            GatedStartupTargetDecision::BlockMultipleGameTargets => {
                update(&diagnostic, |status| {
                    status.target_found = true;
                    status.target_id_found = false;
                    status.managed_game_page = false;
                    status.final_url = None;
                    status.game_url_navigated = true;
                    status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                    status.lifecycle = BrowserLifecycle::WaitingForHello;
                    status.state = BrowserLifecycle::WaitingForHello.label().into();
                    status.message = "Inicialização bloqueada: foram encontradas múltiplas páginas top-level do Pokeidle; nenhuma target foi escolhida ou navegada.".into();
                });
                let process_retained =
                    retain_blocked_browser_process(&account_id, &mut controlled_process).await;
                update(&diagnostic, |status| {
                    status.message = if process_retained {
                        "Inicialização bloqueada; Browser/target preservados e processo mantido sob monitoramento.".into()
                    } else {
                        "Inicialização bloqueada; Browser/target reutilizados foram preservados sem reconexão.".into()
                    };
                });
                return;
            }
            GatedStartupTargetDecision::WaitForBlankTarget => {
                let origin = game_origin(&game_url);
                match recovery_phase!(wait_for_blank_target(&mut socket, &mut next_id, &origin,)) {
                    Ok(GatedBlankWaitResult::Ready(target)) => target,
                    Ok(GatedBlankWaitResult::GameTargetAppeared(target)) => {
                        restored_target_adopted = true;
                        target
                    }
                    Ok(GatedBlankWaitResult::MultipleGameTargets) => {
                        update(&diagnostic, |status| {
                            status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                            status.lifecycle = BrowserLifecycle::WaitingForHello;
                            status.state = BrowserLifecycle::WaitingForHello.label().into();
                            status.message = "Inicialização bloqueada: múltiplas páginas top-level do Pokeidle apareceram durante a detecção de about:blank.".into();
                        });
                        let process_retained =
                            retain_blocked_browser_process(&account_id, &mut controlled_process)
                                .await;
                        update(&diagnostic, |status| {
                            status.message = if process_retained {
                                "Inicialização bloqueada; Browser preservado e processo mantido sob monitoramento.".into()
                            } else {
                                "Inicialização bloqueada; Browser reutilizado preservado sem reconexão.".into()
                            };
                        });
                        return;
                    }
                    Ok(GatedBlankWaitResult::NoPageAppeared) => {
                        match recovery_phase!(create_manager_blank_target(
                            &mut socket,
                            &mut next_id,
                        )) {
                            Ok(target) => {
                                manager_created_target_id = Some(target.id.clone());
                                target
                            }
                            Err(error) => {
                                transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                        return;
                    }
                }
            }
        }
    } else {
        match recovery_phase!(wait_for_game_target(
            &mut socket,
            &mut next_id,
            &game_url,
            reused_controlled_browser && matches!(mode, BrowserSessionMode::BackgroundBootstrap),
        )) {
            Ok(target) => target,
            Err(error) => {
                transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                return;
            }
        }
    };
    update(&diagnostic, |status| {
        status.target_found = target.target_type == "page";
        status.target_id_found = !target.id.is_empty();
        status.managed_game_page = !gate_before_navigation;
        status.page_navigate_sent = !gate_before_navigation;
        status.final_url = Some(target.url.clone());
        status.game_url_navigated = !gate_before_navigation;
        status.message = if restored_target_adopted {
            "Página do jogo restaurada adotada; será neutralizada na mesma target antes de reconectar.".into()
        } else if gate_before_navigation {
            "Página about:blank associada; o GAME_URL ainda não foi carregado.".into()
        } else {
            format!(
                "Página GAME_URL associada como managed_game_page ({})",
                target.url
            )
        };
    });
    let (attached, pending_navigation_events) = match recovery_phase!(cdp_command(
        &mut socket,
        next_id,
        "Target.attachToTarget",
        json!({"targetId": target.id, "flatten": true}),
        None,
    )) {
        Ok(value) => value,
        Err(error) => {
            close_manager_target_on_failure!();
            transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
            return;
        }
    };
    next_id += 1;
    let Some(session_id) = attached
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        close_manager_target_on_failure!();
        transition(
            &diagnostic,
            BrowserLifecycle::Error,
            "Target.attachToTarget não retornou sessionId.",
        );
        return;
    };
    update(&diagnostic, |status| {
        status.cdp_connected = true;
        status.session_created = true;
        status.message = "Sessão CDP da aba gerenciada criada.".into();
    });
    if let Err(error) = recovery_phase!(cdp_command(
        &mut socket,
        next_id,
        "Page.enable",
        json!({}),
        Some(&session_id),
    )) {
        close_manager_target_on_failure!();
        transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
        return;
    }
    next_id += 1;
    update(&diagnostic, |status| {
        status.page_enabled = true;
        status.message = "Page.enable confirmado na sessão da aba.".into();
    });
    if let Err(error) = recovery_phase!(cdp_command(
        &mut socket,
        next_id,
        "Network.enable",
        json!({}),
        Some(&session_id),
    )) {
        close_manager_target_on_failure!();
        transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
        return;
    }
    next_id += 1;
    update(&diagnostic, |status| {
        status.network_enabled = true;
        status.message = "Network.enable confirmado na sessão da aba.".into();
    });

    // A restored game document predates Network.enable, so do not infer its
    // old socket state. Reuse that exact target, destroy only its old document
    // via about:blank, and establish a clean baseline before reconnecting.
    let mut bridge_reload_events = Vec::new();
    if restored_target_adopted {
        let neutralization_origin = game_origin(&game_url);
        match recovery_phase!(neutralize_restored_game_target(
            &mut socket,
            &mut next_id,
            &session_id,
        )) {
            Ok(mut events) => bridge_reload_events.append(&mut events),
            Err(error) => {
                update(&diagnostic, |status| {
                    status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                    status.lifecycle = BrowserLifecycle::WaitingForHello;
                    status.state = BrowserLifecycle::WaitingForHello.label().into();
                    status.message = format!(
                        "Adoção bloqueada: não foi possível neutralizar a página restaurada com confirmação CDP: {error}"
                    );
                });
                let _ = retain_blocked_browser_process(&account_id, &mut controlled_process).await;
                return;
            }
        }
        match recovery_phase!(verify_neutralized_target_baseline(
            &mut socket,
            &mut next_id,
            &target.id,
            &neutralization_origin,
        )) {
            Ok(neutralized_target) => target = neutralized_target,
            Err(error) => {
                update(&diagnostic, |status| {
                    status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                    status.lifecycle = BrowserLifecycle::WaitingForHello;
                    status.state = BrowserLifecycle::WaitingForHello.label().into();
                    status.message = format!(
                        "Adoção bloqueada: baseline about:blank não pôde ser comprovado: {error}"
                    );
                });
                let _ = retain_blocked_browser_process(&account_id, &mut controlled_process).await;
                return;
            }
        }
        update(&diagnostic, |status| {
            status.managed_game_page = false;
            status.page_navigate_sent = false;
            status.game_url_navigated = false;
            status.final_url = Some("about:blank".into());
            status.reload_guard = None;
            status.message =
                "Target restaurada neutralizada; baseline about:blank confirmado na mesma aba."
                    .into();
        });
    }
    if bridge_plan.install_bridge {
        if let Err(error) = recovery_phase!(install_browser_ws_bridge(
            &mut socket,
            next_id,
            &session_id,
            bridge_plan.read_only,
        )) {
            close_manager_target_on_failure!();
            transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
            return;
        }
        next_id += 1;
        if gate_before_navigation {
            let origin = game_origin(&game_url);
            match recovery_phase!(find_unobserved_game_target_before_navigation(
                &mut socket,
                &mut next_id,
                &target.id,
                &origin,
            )) {
                Ok(game_targets) if !game_targets.is_empty() => {
                    close_manager_target_on_failure!();
                    let unobserved_target = game_targets.first().cloned();
                    update(&diagnostic, |status| {
                        status.target_found = true;
                        status.target_id_found = unobserved_target
                            .as_ref()
                            .is_some_and(|target| !target.id.is_empty());
                        status.final_url = unobserved_target.map(|target| target.url);
                        status.game_url_navigated = true;
                        status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                        status.lifecycle = BrowserLifecycle::WaitingForHello;
                        status.state = BrowserLifecycle::WaitingForHello.label().into();
                        status.message = format!(
                            "Navegação bloqueada: {} página(s) top-level do Pokeidle apareceu(ram) antes da navegação coberta; nenhuma target foi selecionada ou fechada.",
                            game_targets.len()
                        );
                    });
                    let process_retained =
                        retain_blocked_browser_process(&account_id, &mut controlled_process).await;
                    update(&diagnostic, |status| {
                        status.message = if process_retained {
                            "Navegação bloqueada; Browser/target preservados e processo mantido sob monitoramento.".into()
                        } else {
                            "Navegação bloqueada; Browser/target reutilizados foram preservados sem reconexão.".into()
                        };
                    });
                    return;
                }
                Ok(_) => {}
                Err(error) => {
                    close_manager_target_on_failure!();
                    transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                    return;
                }
            }
            match recovery_phase!(cdp_command(
                &mut socket,
                next_id,
                "Page.navigate",
                json!({"url": game_url}),
                Some(&session_id),
            )) {
                Ok((_, mut events)) => {
                    bridge_reload_events.append(&mut events);
                }
                Err(error) => {
                    close_manager_target_on_failure!();
                    transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                    return;
                }
            }
            next_id += 1;
            match recovery_phase!(wait_for_navigated_target(
                &mut socket,
                &mut next_id,
                &target.id,
                &game_url,
            )) {
                Ok((navigated_target, mut events)) => {
                    target = navigated_target;
                    bridge_reload_events.append(&mut events);
                }
                Err(error) => {
                    close_manager_target_on_failure!();
                    transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                    return;
                }
            }
            match recovery_phase!(verify_unique_game_target_after_navigation(
                &mut socket,
                &mut next_id,
                &target.id,
                &origin,
            )) {
                Ok(verified_target) => target = verified_target,
                Err(error) => {
                    close_manager_target_on_failure!();
                    update(&diagnostic, |status| {
                        status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                        status.lifecycle = BrowserLifecycle::WaitingForHello;
                        status.state = BrowserLifecycle::WaitingForHello.label().into();
                        status.message = format!(
                            "Navegação bloqueada: unicidade da target pós-navegação não confirmada: {error}"
                        );
                    });
                    let _ =
                        retain_blocked_browser_process(&account_id, &mut controlled_process).await;
                    return;
                }
            }
            // The exact temporary target is now the managed game page, so it
            // must no longer be eligible for failure cleanup.
            let _ = manager_created_target_id.take();
            update(&diagnostic, |status| {
                status.target_found = target.target_type == "page";
                status.target_id_found = !target.id.is_empty();
                status.managed_game_page = true;
                status.page_navigate_sent = true;
                status.final_url = Some(target.url.clone());
                status.game_url_navigated = true;
                status.message = format!(
                    "Guard de validação instalado antes da navegação para GAME_URL ({})",
                    target.url
                );
            });
        } else {
            update(&diagnostic, |status| {
                status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                status.lifecycle = BrowserLifecycle::WaitingForHello;
                status.state = BrowserLifecycle::WaitingForHello.label().into();
                status.message = "Inicialização bloqueada: a bridge não pode ser habilitada com segurança após GAME_URL já estar carregada; a página não será recarregada sem cobertura CDP anterior.".into();
            });
            return;
        }
        next_id += 1;
        update(&diagnostic, |status| {
            status.automatic_reload_used = true;
            status.lifecycle = BrowserLifecycle::WaitingForHello;
            status.state = BrowserLifecycle::WaitingForHello.label().into();
            status.message = "Bridge do navegador instalado; aguardando hello → welcome.".into();
        });
    }

    // This channel exists only for a visible Browser-owner session. It gives the
    // UI a direct handoff request without polling or a second browser process.
    let mut browser_control = if matches!(
        mode,
        BrowserSessionMode::Interactive
            | BrowserSessionMode::InteractiveOwner
            | BrowserSessionMode::ReconnectExistingOwner
    ) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        if let Err(error) = accounts.attach_browser_control(&account_id, sender) {
            transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
            return;
        }
        Some(receiver)
    } else {
        None
    };

    let game_origin = game_origin(&game_url);
    let mut document_generation = 0_u64;
    let mut candidate_socket_id: Option<String> = None;
    let mut observed_sockets = HashMap::<String, ObservedSocket>::new();
    let mut recovery_socket_ids = HashSet::<String>::new();
    let mut handoff_attempted = false;
    let mut pending_reconnect: Option<(tokio::time::Instant, Option<ConnectionBootstrap>)> = None;
    let mut initial_events = pending_navigation_events;
    initial_events.append(&mut bridge_reload_events);
    let mut recovery_welcome_received = false;
    for event in &initial_events {
        if main_frame_game_navigation_generation(event, &session_id, &game_origin) {
            document_generation = document_generation.saturating_add(1);
        }
        if let Some(frame) = observe_page_event(
            &accounts,
            &account_id,
            mode,
            event,
            &session_id,
            &game_origin,
            document_generation,
            &diagnostic,
            &mut candidate_socket_id,
            &mut observed_sockets,
            &mut recovery_socket_ids,
        ) {
            if matches!(mode, BrowserSessionMode::RecoveryProbe) {
                recovery_welcome_received |= capture_recovery_welcome(&frame, &diagnostic);
                continue;
            }
            let browser_welcome = matches!(frame, ServerFrame::Welcome(_));
            let accepted =
                ingest_browser_frame(&accounts, &database, &diagnostic, &account_id, mode, frame);
            if browser_welcome && !accepted {
                return;
            }
            if browser_welcome && is_interactive_owner_mode(mode) {
                let (ready, _) =
                    match browser_ws_bridge_ready(&mut socket, next_id, &session_id).await {
                        Ok(value) => value,
                        Err(error) => {
                            transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                            return;
                        }
                    };
                next_id += 1;
                if !ready {
                    transition(
                        &diagnostic,
                        BrowserLifecycle::Error,
                        "Bridge do navegador não confirmou o WebSocket hello → welcome.",
                    );
                    return;
                }
                let _ = accounts.set_browser_transport_ready(&account_id, true);
                let _ = accounts.complete_browser_owner(&account_id);
            }
        }
    }
    if recovery_welcome_received {
        let Some(process_slot) = recovery_process_slot.as_ref() else {
            transition(
                &diagnostic,
                BrowserLifecycle::Error,
                "Recovery process ownership missing.",
            );
            return;
        };
        let _ = close_recovery_browser(&mut socket, next_id, process_slot, &diagnostic).await;
        return;
    }
    if matches!(mode, BrowserSessionMode::BackgroundBootstrap) && candidate_socket_id.is_none() {
        update(&diagnostic, |status| status.reload_guard = None);
        match page_reload_safety_decision(None, &observed_sockets, false, true) {
            PageReloadSafetyDecision::ReloadNow => {
                let (_, reload_events) = match cdp_command(
                    &mut socket,
                    next_id,
                    "Page.reload",
                    json!({}),
                    Some(&session_id),
                )
                .await
                {
                    Ok(value) => value,
                    Err(error) => {
                        transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                        return;
                    }
                };
                next_id += 1;
                update(&diagnostic, |status| {
                    status.automatic_reload_used = true;
                    status.lifecycle = BrowserLifecycle::WaitingForHello;
                    status.state = BrowserLifecycle::WaitingForHello.label().into();
                    status.message = "Reload único da página gerenciada; aguardando hello.".into();
                });
                for event in &reload_events {
                    if main_frame_game_navigation_generation(event, &session_id, &game_origin) {
                        document_generation = document_generation.saturating_add(1);
                    }
                    if let Some(frame) = observe_page_event(
                        &accounts,
                        &account_id,
                        mode,
                        event,
                        &session_id,
                        &game_origin,
                        document_generation,
                        &diagnostic,
                        &mut candidate_socket_id,
                        &mut observed_sockets,
                        &mut recovery_socket_ids,
                    ) {
                        let accepted = ingest_browser_frame(
                            &accounts,
                            &database,
                            &diagnostic,
                            &account_id,
                            mode,
                            frame,
                        );
                        if matches!(mode, BrowserSessionMode::ReconnectExistingOwner) && !accepted {
                            return;
                        }
                    }
                }
            }
            PageReloadSafetyDecision::RefuseUnobservedTarget => {
                transition(
                    &diagnostic,
                    BrowserLifecycle::WaitingForHello,
                    "Reload automático bloqueado: não há cobertura CDP comprovada desde o carregamento do target.",
                );
                update(&diagnostic, |status| {
                    status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                });
            }
            PageReloadSafetyDecision::RefuseActiveSocket => {
                transition(
                    &diagnostic,
                    BrowserLifecycle::WaitingForHello,
                    "WebSocket do jogo já observado sem hello; reload automático bloqueado para evitar conexão concorrente.",
                );
                update(&diagnostic, |status| {
                    status.reload_guard = Some(ReloadGuardReason::ActiveGameSocket);
                });
            }
            PageReloadSafetyDecision::RefuseReconnectGrace => {
                transition(
                    &diagnostic,
                    BrowserLifecycle::WaitingForHello,
                    "Reload automático bloqueado durante a janela de reconexão.",
                );
                update(&diagnostic, |status| {
                    status.reload_guard = Some(ReloadGuardReason::ReconnectGrace);
                });
            }
        }
    } else if candidate_socket_id.is_none() {
        let lifecycle = if matches!(mode, BrowserSessionMode::Interactive) {
            BrowserLifecycle::WaitingForLogin
        } else {
            BrowserLifecycle::WaitingForHello
        };
        transition(
            &diagnostic,
            lifecycle,
            "Listeners ativos; aguardando autenticação e WebSocket do jogo.",
        );
    }
    loop {
        let mut handoff_requested = false;
        let mut close_requested = false;
        let mut reconnect_grace_expired = false;
        let mut reload_requested = false;
        let mut browser_command = None;
        let next_message = if let Some(control) = browser_control.as_mut() {
            tokio::select! {
                _ = lifecycle_cancellation.cancelled() => {
                    close_requested = true;
                    None
                },
                _ = async {
                    match controlled_process.as_mut() {
                        Some(child) => {
                            let _ = child.wait().await;
                        }
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    if notify_browser_owner_lost(
                        &accounts,
                        &account_id,
                        &diagnostic,
                        &owner_loss_sender,
                        "O processo Brave controlado foi encerrado",
                        bootstrap_for(&account_id, candidate_socket_id.as_deref(), &observed_sockets),
                    ) {
                        return;
                    }
                    None
                },
                command = control.recv() => match command {
                    Some(BrowserControl::HandoffToBackground) => {
                        handoff_requested = true;
                        None
                    }
                    Some(BrowserControl::SendFrame(frame)) => {
                        browser_command = Some(frame);
                        None
                    }
                    Some(BrowserControl::ReloadPage) => {
                        reload_requested = true;
                        None
                    }
                    Some(BrowserControl::CloseManagedBrowser) => {
                        close_requested = true;
                        None
                    }
                    None => break,
                },
                _ = async {
                    match pending_reconnect.as_ref() {
                        Some((deadline, _)) => tokio::time::sleep_until(*deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    reconnect_grace_expired = true;
                    None
                },
                message = socket.next() => message,
            }
        } else if matches!(mode, BrowserSessionMode::BackgroundBootstrap)
            && candidate_socket_id.is_none()
        {
            match tokio::select! {
                _ = lifecycle_cancellation.cancelled() => {
                    close_requested = true;
                    Ok(None)
                }
                result = tokio::time::timeout(Duration::from_secs(30), socket.next()) => result,
            } {
                Ok(message) => message,
                Err(_) => {
                    let _ = accounts.transition(&account_id, AccountRuntimeState::LoginRequired);
                    let _ =
                        cdp_command(&mut socket, next_id, "Browser.close", json!({}), None).await;
                    update(&diagnostic, |status| {
                        status.browser_closed = true;
                        status.lifecycle = BrowserLifecycle::Closed;
                        status.state = "Login necessário".into();
                        status.message =
                            "O perfil não autenticou no bootstrap em background; nenhuma sessão foi conectada."
                                .into();
                    });
                    return;
                }
            }
        } else if matches!(mode, BrowserSessionMode::RecoveryProbe) {
            tokio::select! {
                _ = lifecycle_cancellation.cancelled() => {
                    close_requested = true;
                    None
                }
                message = socket.next() => message,
            }
        } else {
            tokio::select! {
                _ = lifecycle_cancellation.cancelled() => {
                    close_requested = true;
                    None
                }
                message = socket.next() => message,
            }
        };
        if close_requested {
            if matches!(mode, BrowserSessionMode::RecoveryProbe) {
                if let Some(process_slot) = recovery_process_slot.as_ref() {
                    let _ = close_recovery_browser(&mut socket, next_id, process_slot, &diagnostic)
                        .await;
                } else {
                    transition(
                        &diagnostic,
                        BrowserLifecycle::Error,
                        "Recovery process ownership missing.",
                    );
                }
            } else {
                let close_result =
                    cdp_command(&mut socket, next_id, "Browser.close", json!({}), None).await;
                if let Some(child) = controlled_process.as_mut() {
                    match close_result {
                        Ok(_) => {
                            match wait_managed_browser_exit(child, Duration::from_secs(8)).await {
                                Ok(()) => update(&diagnostic, |status| {
                                    status.browser_closed = true;
                                    status.controlled_brave_pid = None;
                                }),
                                Err(error) => {
                                    update(&diagnostic, |status| {
                                        status.browser_closed = false;
                                        status.message = error.to_string();
                                    });
                                    tracing::error!(account_id, %error, "controlled Brave did not exit gracefully");
                                }
                            }
                        }
                        Err(error) => {
                            update(&diagnostic, |status| {
                                status.browser_closed = false;
                                status.message =
                                    format!("Falha ao solicitar fechamento CDP: {error}");
                            });
                        }
                    }
                }
            }
            return;
        }
        // Recovery only observes the game WebSocket and authoritative welcome.
        // Keep it out of normal owner recovery, reconnect, handoff, and command
        // paths below, all of which can mutate account runtime state.
        if matches!(mode, BrowserSessionMode::RecoveryProbe) {
            let Some(message) = next_message else {
                transition(
                    &diagnostic,
                    BrowserLifecycle::Error,
                    "CDP connection ended before a recovery result.",
                );
                return;
            };
            let Ok(Message::Text(raw)) = message else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            if main_frame_game_navigation_generation(&event, &session_id, &game_origin) {
                document_generation = document_generation.saturating_add(1);
            }
            let owner_loss = browser_owner_loss_reason(&event, &target.id);
            let received = observe_page_event(
                &accounts,
                &account_id,
                mode,
                &event,
                &session_id,
                &game_origin,
                document_generation,
                &diagnostic,
                &mut candidate_socket_id,
                &mut observed_sockets,
                &mut recovery_socket_ids,
            );
            if received
                .as_ref()
                .is_some_and(|frame| capture_recovery_welcome(frame, &diagnostic))
                || owner_loss.is_some()
            {
                if owner_loss.is_some() {
                    transition(
                        &diagnostic,
                        BrowserLifecycle::Error,
                        "A página gerenciada fechou antes do welcome.",
                    );
                }
                if let Some(process_slot) = recovery_process_slot.as_ref() {
                    let _ = close_recovery_browser(&mut socket, next_id, process_slot, &diagnostic)
                        .await;
                }
                return;
            }
            continue;
        }
        if reconnect_grace_expired {
            let (ready, _) = match browser_ws_bridge_ready(&mut socket, next_id, &session_id).await
            {
                Ok(value) => value,
                Err(error) => {
                    accounts.browser_control_unavailable(
                        &account_id,
                        format!("Falha ao revalidar o bridge após o reload: {error}"),
                    );
                    update(&diagnostic, |status| {
                        status.browser_ws_connected = false;
                        status.state = "Controle do navegador indisponível".into();
                        status.message = format!(
                            "Não foi possível revalidar a aba após o reload; nenhum fallback será aberto: {error}"
                        );
                    });
                    return;
                }
            };
            next_id += 1;
            let target_exists =
                match cdp_command(&mut socket, next_id, "Target.getTargets", json!({}), None).await
                {
                    Ok((result, _)) => targets_from(&result)
                        .ok()
                        .map(|targets| targets.iter().any(|entry| entry.id == target.id)),
                    Err(_) => None,
                };
            next_id += 1;
            match reconnect_grace_decision(ready, target_exists) {
                ReconnectGraceDecision::BrowserBridgeReadyAwaitRustWelcome => {
                    // Keep command routing disabled until Rust drains and
                    // validates the actual welcome frame, including identity.
                    update(&diagnostic, |status| {
                        status.browser_ws_connected = false;
                        status.state = "Validando reconexão".into();
                        status.message = "A aba reconectou; aguardando o Rust validar o welcome antes de liberar comandos.".into();
                    });
                    pending_reconnect = None;
                    continue;
                }
                ReconnectGraceDecision::FallbackAfterTargetDestroyed => {
                    let bootstrap = pending_reconnect
                        .take()
                        .and_then(|(_, bootstrap)| bootstrap);
                    if notify_browser_owner_lost(
                        &accounts,
                        &account_id,
                        &diagnostic,
                        &owner_loss_sender,
                        "A aba gerenciada desapareceu durante a reconexão do WebSocket",
                        bootstrap,
                    ) {
                        return;
                    }
                    continue;
                }
                ReconnectGraceDecision::StopWithoutFallback => {
                    accounts.browser_control_unavailable(
                        &account_id,
                        "Não foi possível revalidar a aba CDP durante o fallback.".into(),
                    );
                    update(&diagnostic, |status| {
                        status.browser_ws_connected = false;
                        status.state = "Controle do navegador indisponível".into();
                        status.message = "Não foi possível confirmar se a aba ainda existe; fallback suspenso para evitar dois WebSockets.".into();
                    });
                    return;
                }
                ReconnectGraceDecision::CloseTargetThenFallback => {}
            }
            if let Err(error) = close_managed_target(&mut socket, next_id, &target.id).await {
                accounts.browser_control_unavailable(
                    &account_id,
                    format!("Falha ao fechar a aba gerenciada antes do fallback: {error}"),
                );
                update(&diagnostic, |status| {
                    status.browser_ws_connected = false;
                    status.state = "Controle do navegador indisponível".into();
                    status.message = format!(
                        "Fallback suspenso para evitar WebSockets concorrentes; não foi possível fechar a aba com segurança: {error}"
                    );
                });
                return;
            }
            let bootstrap = pending_reconnect
                .take()
                .and_then(|(_, bootstrap)| bootstrap);
            if notify_browser_owner_lost(
                &accounts,
                &account_id,
                &diagnostic,
                &owner_loss_sender,
                "O WebSocket do jogo não reconectou em 8 segundos; aba encerrada antes do fallback",
                bootstrap,
            ) {
                return;
            }
            return;
        }
        if handoff_requested {
            let bootstrap = bootstrap_for(
                &account_id,
                candidate_socket_id.as_deref(),
                &observed_sockets,
            )
            .or_else(|| {
                pending_reconnect
                    .as_ref()
                    .and_then(|(_, bootstrap)| bootstrap.clone())
            });
            let Some(bootstrap) = bootstrap else {
                accounts.restore_browser_owner(&account_id);
                transition(
                    &diagnostic,
                    BrowserLifecycle::Authenticated,
                    "Não há material efêmero suficiente para voltar ao background; Brave permanece como owner.",
                );
                continue;
            };
            let recovery_bootstrap = bootstrap.clone();
            match handoff_to_background(
                &mut socket,
                next_id,
                &target.id,
                bootstrap,
                &accounts,
                &account_id,
                &diagnostic,
                &lifecycle_cancellation,
            )
            .await
            {
                Ok(()) => {
                    if let Some(child) = controlled_process.as_mut() {
                        if let Err(error) =
                            wait_managed_browser_exit(child, Duration::from_secs(8)).await
                        {
                            update(&diagnostic, |status| {
                                status.browser_closed = false;
                                status.lifecycle = BrowserLifecycle::Error;
                                status.state = BrowserLifecycle::Error.label().into();
                                status.message = error.to_string();
                            });
                            tracing::error!(account_id, %error, "controlled Brave handoff did not exit gracefully");
                        } else {
                            update(&diagnostic, |status| {
                                status.browser_closed = true;
                                status.controlled_brave_pid = None;
                            });
                        }
                    }
                    return;
                }
                Err(HandoffFailure::BrowserStillActive(error)) => {
                    accounts.restore_browser_owner(&account_id);
                    update(&diagnostic, |status| {
                        status.lifecycle = BrowserLifecycle::Authenticated;
                        status.state = BrowserLifecycle::Authenticated.label().into();
                        status.message = format!(
                            "Handoff Rust não confirmado ({error}); Brave permanece conectado."
                        );
                    });
                    continue;
                }
                Err(HandoffFailure::BrowserTargetClosed(error)) => {
                    accounts.browser_handoff_failed_after_close(
                        &account_id,
                        format!("Rust handoff failed after closing the game target: {error}"),
                    );
                    update(&diagnostic, |status| {
                        status.browser_ws_connected = false;
                        status.background_active = false;
                        status.state = "Reconectando".into();
                        status.message = format!(
                            "A aba já foi encerrada para impedir sockets simultâneos, mas o handoff Rust falhou: {error}; tentando recuperação segura."
                        );
                    });
                    if let Some(sender) = owner_loss_sender.as_ref() {
                        let _ = sender.send(BrowserOwnerLost {
                            reason: format!("Handoff falhou após encerrar a aba: {error}"),
                            bootstrap: Some(recovery_bootstrap),
                        });
                    }
                    return;
                }
            }
        }
        if reload_requested {
            #[cfg(debug_assertions)]
            if crate::recovery_validation_mode_active() && is_interactive_owner_mode(mode) {
                let reload_diagnostic = diagnostic.lock().clone();
                let observed_coverage = bridge_plan.navigate_after_gate
                    && reload_diagnostic.network_enabled
                    && reload_diagnostic.game_url_navigated;
                let outcome = controlled_observed_reload(
                    &mut socket,
                    &mut next_id,
                    &target,
                    &session_id,
                    &game_origin,
                    &accounts,
                    &account_id,
                    mode,
                    observed_coverage,
                    document_generation,
                    candidate_socket_id.as_deref(),
                    &observed_sockets,
                    &diagnostic,
                    &lifecycle_cancellation,
                )
                .await;
                if outcome == ControlledReloadOutcome::Pass {
                    update(&diagnostic, |status| {
                        status.automatic_reload_used = true;
                        status.lifecycle = BrowserLifecycle::WaitingForHello;
                        status.state = BrowserLifecycle::WaitingForHello.label().into();
                    });
                }
                // In validation this explicit request never falls through to
                // the generic reload decision; a failed proof stays blocked.
                continue;
            }
            update(&diagnostic, |status| status.reload_guard = None);
            match page_reload_safety_decision(
                candidate_socket_id.as_deref(),
                &observed_sockets,
                pending_reconnect.is_some(),
                bridge_plan.navigate_after_gate,
            ) {
                PageReloadSafetyDecision::ReloadNow => {
                    match cdp_command(
                        &mut socket,
                        next_id,
                        "Page.reload",
                        json!({"ignoreCache": false}),
                        Some(&session_id),
                    )
                    .await
                    {
                        Ok(_) => {
                            next_id += 1;
                            continue;
                        }
                        Err(error) => {
                            tracing::warn!(account_id, %error, "managed page reload failed");
                            accounts.browser_control_unavailable(
                                &account_id,
                                format!("Não foi possível recarregar a aba controlada: {error}"),
                            );
                            update(&diagnostic, |status| {
                                status.browser_ws_connected = false;
                                status.state = "Controle do navegador indisponível".into();
                                status.message = "Recarga CDP não confirmada; fallback suspenso para evitar um segundo WebSocket.".into();
                            });
                            return;
                        }
                    }
                }
                PageReloadSafetyDecision::RefuseUnobservedTarget => {
                    update(&diagnostic, |status| {
                        status.reload_guard = Some(ReloadGuardReason::UnobservedTarget);
                        status.message = "Recarga bloqueada: o target iniciou GAME_URL antes da cobertura Network/bridge; sockets anteriores não são enumerados retroativamente.".into();
                    });
                    continue;
                }
                PageReloadSafetyDecision::RefuseActiveSocket => {
                    update(&diagnostic, |status| {
                        status.reload_guard = Some(ReloadGuardReason::ActiveGameSocket);
                        status.message = "Recarga bloqueada: há WebSocket de jogo ativo; a CDP não oferece fechamento direcionado seguro, então a aba permanece como owner.".into();
                    });
                    continue;
                }
                PageReloadSafetyDecision::RefuseReconnectGrace => {
                    update(&diagnostic, |status| {
                        status.reload_guard = Some(ReloadGuardReason::ReconnectGrace);
                        status.message = "Recarga bloqueada durante a janela de reconexão; aguarde a confirmação do socket antes de tentar novamente.".into();
                    });
                    continue;
                }
            }
        }
        if let Some(command) = browser_command {
            match send_via_browser_ws_bridge(&mut socket, next_id, &session_id, &command).await {
                Ok(events) => {
                    next_id += 1;
                    for event in &events {
                        if main_frame_game_navigation_generation(event, &session_id, &game_origin) {
                            document_generation = document_generation.saturating_add(1);
                        }
                        if let Some(frame) = observe_page_event(
                            &accounts,
                            &account_id,
                            mode,
                            event,
                            &session_id,
                            &game_origin,
                            document_generation,
                            &diagnostic,
                            &mut candidate_socket_id,
                            &mut observed_sockets,
                            &mut recovery_socket_ids,
                        ) {
                            let accepted = ingest_browser_frame(
                                &accounts,
                                &database,
                                &diagnostic,
                                &account_id,
                                mode,
                                frame,
                            );
                            if matches!(mode, BrowserSessionMode::ReconnectExistingOwner)
                                && !accepted
                            {
                                return;
                            }
                        }
                    }
                }
                Err(error) => {
                    let _ = accounts.set_browser_transport_ready(&account_id, false);
                    tracing::warn!(account_id, %error, "browser command bridge unavailable");
                    accounts.browser_control_unavailable(
                        &account_id,
                        format!("Bridge WebSocket do navegador indisponível: {error}"),
                    );
                    update(&diagnostic, |status| {
                        status.browser_ws_connected = false;
                        status.state = "Controle do navegador indisponível".into();
                        status.message = format!(
                            "Bridge de comandos falhou; fallback suspenso até comprovar o encerramento do WebSocket atual: {error}"
                        );
                    });
                    return;
                }
            }
            continue;
        }
        let Some(message) = next_message else {
            break;
        };
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                accounts.browser_control_unavailable(
                    &account_id,
                    format!("A conexão CDP do navegador falhou: {error}"),
                );
                update(&diagnostic, |status| {
                    status.browser_ws_connected = false;
                    status.state = "Controle do navegador indisponível".into();
                    status.message = format!(
                        "A conexão CDP caiu sem comprovar o encerramento do socket do jogo; fallback automático suspenso por segurança: {error}"
                    );
                });
                break;
            }
        };
        let Message::Text(raw) = message else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if main_frame_game_navigation_generation(&event, &session_id, &game_origin) {
            document_generation = document_generation.saturating_add(1);
        }
        let matching_socket_closed =
            candidate_game_socket_closed(&event, candidate_socket_id.as_deref());
        let closed_socket_bootstrap = matching_socket_closed.then(|| {
            bootstrap_for(
                &account_id,
                candidate_socket_id.as_deref(),
                &observed_sockets,
            )
        });
        let owner_loss = browser_owner_loss_reason(&event, &target.id).map(str::to_owned);
        let recovery_bootstrap = owner_loss.as_ref().and_then(|_| {
            bootstrap_for(
                &account_id,
                candidate_socket_id.as_deref(),
                &observed_sockets,
            )
        });
        let received = observe_page_event(
            &accounts,
            &account_id,
            mode,
            &event,
            &session_id,
            &game_origin,
            document_generation,
            &diagnostic,
            &mut candidate_socket_id,
            &mut observed_sockets,
            &mut recovery_socket_ids,
        );
        if let Some(reason) = owner_loss
            && notify_browser_owner_lost(
                &accounts,
                &account_id,
                &diagnostic,
                &owner_loss_sender,
                reason,
                recovery_bootstrap,
            )
        {
            return;
        }
        if let Some(reason) = browser_control_loss_reason(&event, &session_id) {
            // Detach can race ahead of targetDestroyed. Reconcile against the
            // browser's authoritative target list before deciding whether the
            // Browser owner is gone; a live target is ambiguous, not a reason
            // to start a competing Rust WebSocket.
            next_id += 1;
            let target_exists =
                cdp_command(&mut socket, next_id, "Target.getTargets", json!({}), None)
                    .await
                    .ok()
                    .and_then(|(result, _)| targets_from(&result).ok())
                    .map(|targets| targets.iter().any(|entry| entry.id == target.id));
            let bootstrap = bootstrap_for(
                &account_id,
                candidate_socket_id.as_deref(),
                &observed_sockets,
            );
            match detached_target_decision(target_exists) {
                DetachedTargetDecision::RecoverAfterTargetDestroyed => {
                    if notify_browser_owner_lost(
                        &accounts,
                        &account_id,
                        &diagnostic,
                        &owner_loss_sender,
                        "A aba gerenciada deixou de existir após o detach da sessão CDP",
                        bootstrap,
                    ) {
                        return;
                    }
                }
                DetachedTargetDecision::CloseTargetBeforeFallback => {
                    if let Err(close_error) =
                        close_managed_target(&mut socket, next_id, &target.id).await
                    {
                        accounts.browser_control_unavailable(
                            &account_id,
                            format!(
                                "{reason}; falha ao fechar a aba antes do fallback: {close_error}"
                            ),
                        );
                        update(&diagnostic, |status| {
                            status.browser_ws_connected = false;
                            status.state = "Controle do navegador indisponível".into();
                            status.message = format!(
                                "{reason}; o target ainda existe e não foi possível fechá-lo. Fallback bloqueado para evitar WebSockets concorrentes: {close_error}"
                            );
                        });
                        return;
                    }
                    if notify_browser_owner_lost(
                        &accounts,
                        &account_id,
                        &diagnostic,
                        &owner_loss_sender,
                        "A sessão CDP foi desanexada; a aba gerenciada foi fechada antes do fallback",
                        bootstrap,
                    ) {
                        return;
                    }
                }
                DetachedTargetDecision::StopWithoutFallback => {}
            }
            accounts.browser_control_unavailable(&account_id, reason.into());
            update(&diagnostic, |status| {
                status.browser_ws_connected = false;
                status.state = "Controle do navegador indisponível".into();
                status.message = format!(
                    "{reason}; target inconclusivo ou recovery não iniciado, portanto nenhum outro WebSocket será aberto automaticamente."
                );
            });
            return;
        }
        if matching_socket_closed {
            pending_reconnect = Some((
                tokio::time::Instant::now() + BROWSER_RECONNECT_GRACE,
                closed_socket_bootstrap.flatten(),
            ));
            update(&diagnostic, |status| {
                status.browser_ws_connected = false;
                status.state = "Aguardando reconexão".into();
                status.message = "O socket antigo fechou; aguardando até 8 segundos pelo hello → welcome da mesma aba.".into();
            });
        }
        let Some(frame) = received else {
            continue;
        };
        let browser_welcome = matches!(frame, ServerFrame::Welcome(_));
        let accepted =
            ingest_browser_frame(&accounts, &database, &diagnostic, &account_id, mode, frame);
        if browser_welcome
            && !accepted
            && matches!(mode, BrowserSessionMode::ReconnectExistingOwner)
        {
            return;
        }
        if !browser_welcome || !accepted {
            continue;
        }
        if is_interactive_owner_mode(mode) {
            let (ready, _) = match browser_ws_bridge_ready(&mut socket, next_id, &session_id).await
            {
                Ok(value) => value,
                Err(error) => {
                    transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                    return;
                }
            };
            next_id += 1;
            if !ready {
                transition(
                    &diagnostic,
                    BrowserLifecycle::Error,
                    "Bridge do navegador não confirmou o WebSocket hello → welcome.",
                );
                return;
            }
            let _ = accounts.set_browser_transport_ready(&account_id, true);
            pending_reconnect = None;
            if let Err(error) = accounts.complete_browser_owner(&account_id) {
                transition(&diagnostic, BrowserLifecycle::Error, error.to_string());
                return;
            }
            update(&diagnostic, |status| {
                status.lifecycle = BrowserLifecycle::Authenticated;
                status.state = BrowserLifecycle::Authenticated.label().into();
                status.message =
                    "Navegador pronto e conectado; Rust Background foi encerrado.".into();
            });
            continue;
        }
        if handoff_attempted {
            continue;
        }
        let Some(bootstrap) = bootstrap_for(
            &account_id,
            candidate_socket_id.as_deref(),
            &observed_sockets,
        ) else {
            transition(
                &diagnostic,
                BrowserLifecycle::Authenticated,
                "welcome do navegador recebido; aguardando material efêmero da conexão para handoff.",
            );
            continue;
        };
        handoff_attempted = true;
        let recovery_bootstrap = bootstrap.clone();
        if let Err(error) = handoff_to_background(
            &mut socket,
            next_id,
            &target.id,
            bootstrap,
            &accounts,
            &account_id,
            &diagnostic,
            &lifecycle_cancellation,
        )
        .await
        {
            match error {
                HandoffFailure::BrowserStillActive(error) => {
                    let _ = accounts.transition(&account_id, AccountRuntimeState::BrowserConnected);
                    update(&diagnostic, |status| {
                        status.lifecycle = BrowserLifecycle::Authenticated;
                        status.state = BrowserLifecycle::Authenticated.label().into();
                        status.message = format!(
                            "Handoff Rust não confirmado ({error}); Brave permanece conectado."
                        );
                    });
                }
                HandoffFailure::BrowserTargetClosed(error) => {
                    if lifecycle_cancellation.is_cancelled() {
                        return;
                    }
                    accounts.browser_control_unavailable(
                        &account_id,
                        format!("Background bootstrap failed after closing the game tab: {error}"),
                    );
                    update(&diagnostic, |status| {
                        status.lifecycle = BrowserLifecycle::Error;
                        status.state = BrowserLifecycle::Error.label().into();
                        status.message = format!(
                            "A aba foi encerrada antes da transferência; Background não conectou: {error}. Inicie uma nova reconexão da conta."
                        );
                    });
                    let _ = recovery_bootstrap;
                    return;
                }
            }
        } else {
            return;
        }
    }
    accounts.browser_control_unavailable(
        &account_id,
        "A conexão CDP do navegador foi encerrada.".into(),
    );
    update(&diagnostic, |status| {
        status.browser_ws_connected = false;
        status.lifecycle = BrowserLifecycle::Error;
        status.state = BrowserLifecycle::Error.label().into();
        status.message = "A sessão CDP foi encerrada; nenhum WebSocket de fallback foi aberto sem prova de que o socket do jogo terminou.".into();
    });
}

#[cfg(test)]
#[path = "browser_target_tests.rs"]
mod browser_target_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "pokeidle-brave-resolver-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn brave_resolver_returns_existing_executable_candidate() {
        let root = unique_test_root();
        let executable = root.join("Application").join("brave.exe");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"MZ test executable placeholder").unwrap();

        assert_eq!(
            resolve_brave_candidates([executable.clone()]).unwrap(),
            executable
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn brave_resolver_distinguishes_missing_and_invalid_installation() {
        let root = unique_test_root();
        let missing = root.join("missing").join("brave.exe");
        assert!(matches!(
            resolve_brave_candidates([missing]),
            Err(BrowserError::NotFound)
        ));

        let invalid = root.join("invalid").join("brave.exe");
        std::fs::create_dir_all(&invalid).unwrap();
        assert!(matches!(
            resolve_brave_candidates([invalid.clone()]),
            Err(BrowserError::InvalidInstallation(path)) if path == invalid
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn brave_resolver_retries_filesystem_detection_without_caching_not_found() {
        let root = unique_test_root();
        let executable = root.join("Application").join("brave.exe");
        assert!(matches!(
            resolve_brave_candidates([executable.clone()]),
            Err(BrowserError::NotFound)
        ));

        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"MZ test executable placeholder").unwrap();
        assert_eq!(
            resolve_brave_candidates([executable.clone()]).unwrap(),
            executable
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn brave_resolver_rejects_a_corrupt_executable_file() {
        let root = unique_test_root();
        let executable = root.join("Application").join("brave.exe");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"not a windows executable").unwrap();

        assert!(matches!(
            resolve_brave_candidates([executable.clone()]),
            Err(BrowserError::InvalidInstallation(path)) if path == executable
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_bridge_policy_is_fail_closed_and_preserves_handshake_queries() {
        for frame_type in [
            Some("hello"),
            Some("market.itens"),
            Some("market.item"),
            Some("market.historicoGlobal"),
            Some("ranking.perfil"),
        ] {
            assert!(validation_bridge_allows_frame_type(frame_type));
        }
        for frame_type in [
            None,
            Some("shop.buy"),
            Some("market.comprar"),
            Some("ball.throw"),
            Some("auto.set"),
            Some("future.unknown"),
        ] {
            assert!(!validation_bridge_allows_frame_type(frame_type));
        }
    }

    #[test]
    fn operational_validation_preserves_game_protocol_and_probe_remains_read_only() {
        let normal_source = browser_ws_bridge_source(false);
        assert!(normal_source.contains("const validationReadOnly = false;"));
        let probe_source = browser_ws_bridge_source(true);
        assert!(probe_source.contains("const validationReadOnly = true;"));
        assert!(!probe_source.contains("__VALIDATION_READ_ONLY__"));
        assert!(!probe_source.contains("__VALIDATION_READ_ONLY_TYPES__"));
        assert!(probe_source.contains("WebSocket.prototype.send = function(data)"));
        assert!(probe_source.contains("if (!allowsFrame(frame)) return;"));
        assert!(probe_source.contains("if (!allowsFrame(parse(payload))) return false;"));
        for frame_type in VALIDATION_READ_ONLY_FRAME_TYPES {
            assert!(probe_source.contains(&format!("\"{frame_type}\"")));
        }
        let operational_plan =
            browser_bridge_startup_plan(BrowserSessionMode::InteractiveOwner, true);
        assert!(operational_plan.start_blank && operational_plan.install_bridge);
        assert!(!operational_plan.read_only);
        assert!(operational_plan.navigate_after_gate);
    }

    #[test]
    fn operational_validation_activates_bridge_for_every_live_browser_mode() {
        let live_modes = [
            BrowserSessionMode::Interactive,
            BrowserSessionMode::InteractiveOwner,
            BrowserSessionMode::ReconnectExistingOwner,
            BrowserSessionMode::BackgroundBootstrap,
        ];
        for mode in live_modes {
            let plan = browser_bridge_startup_plan(mode, true);
            assert_eq!(
                plan,
                BrowserBridgeStartupPlan {
                    start_blank: true,
                    install_bridge: true,
                    read_only: false,
                    navigate_after_gate: true,
                }
            );
            assert_eq!(
                initial_browser_url("https://pokeidle.io/app", plan.start_blank),
                None
            );
        }

        // Probe remains read-only. Owner/bootstrap modes are CDP-gated and
        // launch without a positional URL; plain Interactive remains direct-
        // load and cannot safely reload because its socket may predate Network.enable.
        assert_eq!(
            browser_bridge_startup_plan(BrowserSessionMode::RecoveryProbe, false),
            BrowserBridgeStartupPlan {
                start_blank: true,
                install_bridge: true,
                read_only: true,
                navigate_after_gate: true,
            }
        );
        assert_eq!(
            browser_bridge_startup_plan(BrowserSessionMode::BackgroundBootstrap, false),
            BrowserBridgeStartupPlan {
                start_blank: true,
                install_bridge: true,
                read_only: false,
                navigate_after_gate: true,
            }
        );
        assert_eq!(
            browser_bridge_startup_plan(BrowserSessionMode::InteractiveOwner, false),
            BrowserBridgeStartupPlan {
                start_blank: true,
                install_bridge: true,
                read_only: false,
                navigate_after_gate: true,
            }
        );
        assert_eq!(
            browser_bridge_startup_plan(BrowserSessionMode::ReconnectExistingOwner, false),
            BrowserBridgeStartupPlan {
                start_blank: true,
                install_bridge: true,
                read_only: false,
                navigate_after_gate: true,
            }
        );
        assert_eq!(
            browser_bridge_startup_plan(BrowserSessionMode::Interactive, false),
            BrowserBridgeStartupPlan {
                start_blank: false,
                install_bridge: false,
                read_only: false,
                navigate_after_gate: false,
            }
        );
        assert_eq!(
            initial_browser_url("https://pokeidle.io/app", false),
            Some("https://pokeidle.io/app")
        );
        assert_eq!(
            initial_browser_url("https://pokeidle.io/app", true),
            None,
            "gated startup must not ask Brave's launcher to create a second blank page"
        );
    }

    #[test]
    fn reconnect_existing_owner_never_falls_back_to_a_new_browser_launch() {
        assert!(is_interactive_owner_mode(
            BrowserSessionMode::ReconnectExistingOwner
        ));
        assert!(!matches!(
            BrowserSessionMode::ReconnectExistingOwner,
            BrowserSessionMode::BackgroundBootstrap
        ));
        let source = include_str!("browser.rs").replace("\r\n", "\n");
        let reconnect_branch = source
            .find("BrowserSessionMode::ReconnectExistingOwner\n    ) {")
            .expect("the existing-owner path is present");
        let fallback_branch = source[reconnect_branch..]
            .find("Nenhum navegador novo foi iniciado.")
            .expect("a missing existing browser fails closed");
        let launch = source[reconnect_branch..]
            .find("manager.launch_controlled")
            .expect("normal session launch remains available after reconnect checks");
        assert!(fallback_branch < launch);
    }

    #[test]
    fn validation_browser_installs_bridge_before_loading_game_url_without_filtering_protocol() {
        let plan = browser_bridge_startup_plan(BrowserSessionMode::Interactive, true);
        assert!(plan.start_blank && plan.install_bridge && plan.navigate_after_gate);
        assert!(!plan.read_only);
        assert_eq!(
            initial_browser_url("https://pokeidle.io/app", plan.start_blank),
            None
        );

        // Validation sessions attach the normal bridge to a blank target, then
        // navigate. The protocol is not filtered; mutating Manager commands
        // are guarded centrally by AccountCommandDispatcher.
        let source = include_str!("browser.rs");
        let start = source
            .find("let mut target = if gate_before_navigation")
            .unwrap();
        let end = source[start..]
            .find("let mut bridge_reload_events = Vec::new();")
            .map(|offset| start + offset)
            .unwrap();
        let guarded_startup = &source[start..end];
        let blank_target = guarded_startup.find("wait_for_blank_target(").unwrap();
        let attach = guarded_startup.find("Target.attachToTarget").unwrap();
        let inject = source[end..]
            .find("install_browser_ws_bridge(")
            .map(|offset| end + offset)
            .unwrap();
        let navigate = source[inject..]
            .find("\"Page.navigate\"")
            .map(|offset| inject + offset)
            .unwrap();
        assert!(start + blank_target < start + attach);
        assert!(end < inject && inject < navigate);
        assert!(!guarded_startup.contains("Target.createTarget"));
    }

    #[test]
    fn recovery_cdp_endpoint_must_report_the_owned_browser_pid() {
        let owned = json!({"processInfo": [
            {"id": 1234, "type": "browser"},
            {"id": 5678, "type": "renderer"}
        ]});
        assert!(cdp_endpoint_contains_owned_pid(&owned, 1234));
        assert!(!cdp_endpoint_contains_owned_pid(&owned, 9999));
        assert!(!cdp_endpoint_contains_owned_pid(
            &json!({"processInfo": []}),
            1234
        ));
        assert!(!cdp_endpoint_contains_owned_pid(&json!({}), 1234));
    }

    #[test]
    fn creates_profiles_below_the_application_data_directory() {
        let manager = WindowsBraveManager::new(PathBuf::from("app-data"));
        assert_eq!(
            manager.profile_for("acc-1").path,
            PathBuf::from("app-data/profiles/acc-1")
        );
    }

    #[test]
    fn recognizes_only_pages_at_the_configured_game_origin() {
        let origin = game_origin("https://pokeidle.io/app");
        let game = CdpTargetInfo {
            id: "game".into(),
            target_type: "page".into(),
            url: "https://pokeidle.io/app?restored=true".into(),
        };
        let oauth = CdpTargetInfo {
            id: "oauth".into(),
            target_type: "page".into(),
            url: "https://accounts.example.test/login".into(),
        };
        assert!(is_game_page(&game, &origin));
        assert!(!is_game_page(&oauth, &origin));
    }

    #[test]
    fn uses_the_game_host_only_as_a_clue_not_as_authentication() {
        assert!(is_pokeidle_socket("wss://pokeidle.io/socket"));
        assert!(!is_pokeidle_socket("wss://example.test/socket"));
    }

    #[test]
    fn only_proven_managed_target_destruction_is_an_immediate_owner_loss() {
        let target_destroyed = json!({
            "method": "Target.targetDestroyed",
            "params": {"targetId": "managed-page"}
        });
        assert_eq!(
            browser_owner_loss_reason(&target_destroyed, "managed-page",),
            Some("A aba gerenciada do jogo foi fechada")
        );

        let websocket_closed = json!({
            "method": "Network.webSocketClosed",
            "params": {"requestId": "socket-1"}
        });
        assert_eq!(
            browser_owner_loss_reason(&websocket_closed, "managed-page"),
            None,
            "a WebSocket close is not proof that the managed page/owner died"
        );
        let session_detached = json!({
            "method": "Target.detachedFromTarget",
            "params": {"sessionId": "session-1", "targetId": "managed-page"}
        });
        assert_eq!(
            browser_owner_loss_reason(&session_detached, "managed-page"),
            None,
            "a CDP detach requires target reconciliation before fallback"
        );
        assert!(browser_control_loss_reason(&session_detached, "session-1").is_some());
        assert!(candidate_game_socket_closed(
            &websocket_closed,
            Some("socket-1")
        ));
        assert!(!candidate_game_socket_closed(
            &websocket_closed,
            Some("another-socket")
        ));
    }

    #[test]
    fn reload_close_counter_recognizes_the_old_game_socket_after_new_hello() {
        let observed = HashMap::from([(
            "old-socket".to_owned(),
            ObservedSocket {
                ws_url: "wss://pokeidle.io/socket".into(),
                headers: None,
                hello: None,
                document_generation: 1,
                handshake_response_received: true,
            },
        )]);
        let recovery_sockets = HashSet::new();

        assert!(tracked_game_socket_closed(
            Some("old-socket"),
            &observed,
            &recovery_sockets
        ));
        assert!(!candidate_game_socket_closed(
            &json!({
                "method": "Network.webSocketClosed",
                "params": {"requestId": "old-socket"}
            }),
            Some("new-socket")
        ));
        assert!(!tracked_game_socket_closed(
            Some("unrelated-socket"),
            &observed,
            &recovery_sockets
        ));
    }

    #[test]
    fn reconnect_grace_decisions_never_fallback_while_the_browser_target_may_be_live() {
        assert_eq!(
            reconnect_grace_decision(true, Some(true)),
            ReconnectGraceDecision::BrowserBridgeReadyAwaitRustWelcome
        );
        assert_eq!(
            reconnect_grace_decision(false, Some(true)),
            ReconnectGraceDecision::CloseTargetThenFallback
        );
        assert_eq!(
            reconnect_grace_decision(false, Some(false)),
            ReconnectGraceDecision::FallbackAfterTargetDestroyed
        );
        assert_eq!(
            reconnect_grace_decision(false, None),
            ReconnectGraceDecision::StopWithoutFallback
        );

        // Long deterministic soak over all timing/target combinations: whenever
        // a Browser WebSocket is ready it retains ownership; when target state
        // is unknown, fallback is refused rather than risking a second socket.
        for cycle in 0..10_000 {
            let bridge_ready = cycle % 4 == 0;
            let target_exists = match cycle % 3 {
                0 => Some(true),
                1 => Some(false),
                _ => None,
            };
            let decision = reconnect_grace_decision(bridge_ready, target_exists);
            if bridge_ready {
                assert_eq!(
                    decision,
                    ReconnectGraceDecision::BrowserBridgeReadyAwaitRustWelcome
                );
            } else if target_exists == Some(true) {
                assert_eq!(decision, ReconnectGraceDecision::CloseTargetThenFallback);
            } else if target_exists == Some(false) {
                assert_eq!(
                    decision,
                    ReconnectGraceDecision::FallbackAfterTargetDestroyed
                );
            } else {
                assert_eq!(decision, ReconnectGraceDecision::StopWithoutFallback);
            }
        }
    }

    #[test]
    fn reload_waits_for_observed_socket_close_and_reconnect_grace() {
        let accounts = AccountManager::default();
        accounts
            .add(crate::accounts::new_record(
                "account-1".into(),
                "test-account".into(),
                "#fff".into(),
            ))
            .unwrap();
        let diagnostic = Arc::new(Mutex::new(IntegrationDiagnostic {
            game_ws_created_count: 1,
            game_ws_open_count: 1,
            ..IntegrationDiagnostic::default()
        }));
        let mut candidate_socket_id = Some("socket-1".to_owned());
        let mut observed_sockets = HashMap::from([(
            "socket-1".to_owned(),
            ObservedSocket {
                ws_url: "wss://pokeidle.io/socket".into(),
                headers: None,
                hello: None,
                document_generation: 1,
                handshake_response_received: true,
            },
        )]);
        let mut recovery_socket_ids = HashSet::new();

        assert_eq!(
            page_reload_safety_decision(
                candidate_socket_id.as_deref(),
                &observed_sockets,
                false,
                true,
            ),
            PageReloadSafetyDecision::RefuseActiveSocket,
            "never reload while the observed owner socket remains open"
        );

        let closed = json!({
            "sessionId": "session-1",
            "method": "Network.webSocketClosed",
            "params": {"requestId": "socket-1"}
        });
        observe_page_event(
            &accounts,
            "account-1",
            BrowserSessionMode::InteractiveOwner,
            &closed,
            "session-1",
            "https://pokeidle.io",
            1,
            &diagnostic,
            &mut candidate_socket_id,
            &mut observed_sockets,
            &mut recovery_socket_ids,
        );
        assert!(candidate_socket_id.is_none());
        assert!(observed_sockets.is_empty());
        assert_eq!(diagnostic.lock().game_ws_open_count, 0);

        assert_eq!(
            page_reload_safety_decision(None, &observed_sockets, true, true),
            PageReloadSafetyDecision::RefuseReconnectGrace,
            "a just-closed socket is not enough while reconnect grace is active"
        );
        assert_eq!(
            page_reload_safety_decision(None, &observed_sockets, false, true),
            PageReloadSafetyDecision::ReloadNow,
            "reload becomes eligible only after close is observed and grace has ended"
        );
    }

    #[test]
    fn reload_is_blocked_for_direct_loaded_target_and_preexisting_game_page() {
        let no_observed_sockets = HashMap::new();
        assert_eq!(
            page_reload_safety_decision(None, &no_observed_sockets, false, false),
            PageReloadSafetyDecision::RefuseUnobservedTarget,
            "empty Network event state is not proof that an already-loaded page has no socket"
        );
        assert_eq!(
            gated_startup_target_decision(false, 1),
            GatedStartupTargetDecision::AdoptRestoredGameTarget,
            "one preexisting GAME_URL page is adopted for controlled neutralization"
        );
        assert_eq!(
            gated_startup_target_decision(false, 2),
            GatedStartupTargetDecision::BlockMultipleGameTargets,
            "multiple preexisting GAME_URL pages must never be chosen arbitrarily"
        );
        assert_eq!(
            gated_startup_target_decision(true, 0),
            GatedStartupTargetDecision::UseBlankTarget
        );
        assert_eq!(
            gated_startup_target_decision(false, 0),
            GatedStartupTargetDecision::WaitForBlankTarget
        );
        let restored_target = json!({
            "method": "Target.targetCreated",
            "params": {"targetInfo": {
                "targetId": "restored-game",
                "type": "page",
                "url": "https://pokeidle.io/app"
            }}
        });
        assert_eq!(
            game_target_from_event(&restored_target, "https://pokeidle.io").map(|target| target.id),
            Some("restored-game".into()),
            "a restored GAME_URL arriving after the snapshot must still trip the guard"
        );
        assert_eq!(
            blocked_browser_ownership_action(true),
            BlockedBrowserOwnershipAction::RetainStartedProcess,
            "an observer-started Browser stays owned by its monitor until process exit"
        );
        assert_eq!(
            blocked_browser_ownership_action(false),
            BlockedBrowserOwnershipAction::PreserveReusedProcess,
            "a reused Browser must never be closed or killed by this guard"
        );
    }

    #[test]
    fn reload_guard_snapshot_has_stable_machine_readable_reasons() {
        let mut diagnostic = IntegrationDiagnostic::default();
        diagnostic.reload_guard = Some(ReloadGuardReason::ActiveGameSocket);
        let snapshot = serde_json::to_value(diagnostic).unwrap();

        assert_eq!(snapshot["reloadGuard"], "activeGameSocket");
        assert_eq!(
            serde_json::to_value(ReloadGuardReason::ReconnectGrace).unwrap(),
            "reconnectGrace"
        );
        assert_eq!(
            serde_json::to_value(ReloadGuardReason::UnobservedTarget).unwrap(),
            "unobservedTarget"
        );
    }

    #[test]
    fn detached_session_recovery_closes_a_live_target_before_fallback() {
        assert_eq!(
            detached_target_decision(Some(false)),
            DetachedTargetDecision::RecoverAfterTargetDestroyed
        );
        assert_eq!(
            detached_target_decision(Some(true)),
            DetachedTargetDecision::CloseTargetBeforeFallback
        );
        assert_eq!(
            detached_target_decision(None),
            DetachedTargetDecision::StopWithoutFallback
        );
    }

    #[tokio::test]
    #[ignore = "requires a real CDP endpoint; set POKEIDLE_CDP_PORT"]
    async fn probes_a_real_cdp_version_endpoint_from_rust() {
        let port = std::env::var("POKEIDLE_CDP_PORT")
            .expect("set POKEIDLE_CDP_PORT to the real managed Brave CDP port")
            .parse::<u16>()
            .expect("POKEIDLE_CDP_PORT must be a valid port");
        let version = fetch_cdp_version(&cdp_http_client().expect("build local CDP client"), port)
            .await
            .expect("Rust must read /json/version from the real Brave instance");
        assert!(!version.browser.is_empty());
        assert!(
            version
                .web_socket_debugger_url
                .starts_with("ws://127.0.0.1:")
        );
        println!("Browser: {}", version.browser);
        println!("Browser WS URL obtida: ✓");
    }
}
