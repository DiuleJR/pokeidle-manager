//! Opt-in development-only protocol metadata/projection tap.
//!
//! This module is compiled only in debug builds. It never persists raw frames:
//! metadata is reconstructed from safe identifiers, and payload output is a
//! strict Twitch/Kick allowlist. A bounded channel keeps disk I/O off the socket
//! path and drops diagnostic events instead of applying backpressure.
use crate::{
    accounts::AccountManager,
    inspector::{ProtocolDirection, ProtocolTransport},
};
use serde_json::{Map, Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

const ENV_ENABLE: &str = "POKEIDLE_DEV_PROTOCOL_TAP";
const ENV_ACCOUNT: &str = "POKEIDLE_DEV_PROTOCOL_ACCOUNT";
const ENV_FILTER: &str = "POKEIDLE_DEV_PROTOCOL_FILTER";
const ENV_MODE: &str = "POKEIDLE_DEV_PROTOCOL_MODE";
const ENV_RECORD: &str = "POKEIDLE_DEV_PROTOCOL_RECORD";
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_SESSION_FILES: usize = 3;
const MAX_EVENT_BYTES: usize = 32 * 1024;
const MAX_RAW_FRAME_BYTES: usize = 512 * 1024;
// At most ~4 MiB of copied raw frame data can wait for the dedicated writer.
const CHANNEL_CAPACITY: usize = 8;
const MAX_TREE_NODES: usize = 4096;
const MAX_TREE_DEPTH: usize = 12;
const MAX_MATCHES: usize = 64;
const MAX_TOP_LEVEL_KEYS: usize = 32;
const MAX_ARRAY_ROWS: usize = 32;
const MAX_STRING_BYTES: usize = 96;
const DEFAULT_FILTER: &str = "twitch,kick,live,stream,channel,viewer,online,offline,bonus";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadMode {
    MetadataOnly,
    FilteredPayload,
}

#[derive(Debug, Clone)]
struct TapConfig {
    account_id: Option<String>,
    account_label: String,
    terms: Vec<String>,
    mode: PayloadMode,
    record_all_metadata: bool,
}

enum TapMessage {
    #[cfg(test)]
    Line(Vec<u8>),
    RawFrame {
        raw: Vec<u8>,
        direction: ProtocolDirection,
        transport: ProtocolTransport,
    },
    Shutdown,
}

pub struct DevProtocolTap {
    config: TapConfig,
    sender: SyncSender<TapMessage>,
    writer: Option<JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
    file_limit_reached: Arc<AtomicBool>,
    path: PathBuf,
}

impl DevProtocolTap {
    pub fn is_requested() -> bool {
        std::env::var(ENV_ENABLE).ok().as_deref() == Some("1")
    }

    /// Returns `Ok(None)` unless explicitly enabled and uniquely bound to one saved account.
    pub fn from_env(
        app_data_dir: &Path,
        accounts: &AccountManager,
    ) -> Result<Option<Self>, String> {
        if !Self::is_requested() {
            return Ok(None);
        }
        ensure_smoke_app_data_dir(app_data_dir)?;

        let account = std::env::var(ENV_ACCOUNT)
            .map_err(|_| format!("{ENV_ACCOUNT} precisa selecionar uma única conta"))?;
        normalize_account(&account).ok_or_else(|| format!("valor inválido em {ENV_ACCOUNT}"))?;
        let account_label = account
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
            })
            .take(32)
            .collect::<String>();
        if account_label.is_empty() {
            return Err(format!("valor inválido em {ENV_ACCOUNT}"));
        }
        let matching_account_ids = accounts
            .runtime_metadata()
            .into_iter()
            .filter(|(saved, _)| saved.nick.eq_ignore_ascii_case(&account))
            .map(|(saved, _)| saved.id)
            .collect::<Vec<_>>();
        let account_id = match matching_account_ids.as_slice() {
            [account_id] => account_id.clone(),
            [] => return Err(format!("nenhuma conta salva corresponde a {ENV_ACCOUNT}")),
            _ => {
                return Err(format!(
                    "mais de uma conta salva corresponde a {ENV_ACCOUNT}"
                ));
            }
        };

        let configured_terms = std::env::var(ENV_FILTER).unwrap_or_else(|_| DEFAULT_FILTER.into());
        let terms = parse_terms(&configured_terms);
        if terms.is_empty() {
            return Err(format!("nenhum termo válido em {ENV_FILTER}"));
        }
        let mode = match std::env::var(ENV_MODE).as_deref() {
            Ok("metadata_only") => PayloadMode::MetadataOnly,
            Ok("filtered_payload") | Err(_) => PayloadMode::FilteredPayload,
            Ok(_) => {
                return Err(format!(
                    "{ENV_MODE} deve ser metadata_only ou filtered_payload"
                ));
            }
        };
        let record_all_metadata = match std::env::var(ENV_RECORD).as_deref() {
            Ok("matching_only") => false,
            Ok("all_metadata") => true,
            Err(_) => false,
            Ok(_) => {
                return Err(format!(
                    "{ENV_RECORD} deve ser all_metadata ou matching_only"
                ));
            }
        };

        let directory = app_data_dir.join("dev").join("protocol");
        fs::create_dir_all(&directory)
            .map_err(|error| format!("não foi possível criar a pasta do Protocol Tap: {error}"))?;
        prune_old_session_files(&directory)?;
        let path = create_session_file(&directory)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!("não foi possível criar o arquivo do Protocol Tap: {error}")
            })?;
        let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let file_limit_reached = Arc::new(AtomicBool::new(false));
        let writer_limit = file_limit_reached.clone();
        let writer_dropped = dropped.clone();
        let writer_config = TapConfig {
            account_id: Some(account_id.clone()),
            account_label: account_label.clone(),
            terms: terms.clone(),
            mode,
            record_all_metadata,
        };
        let writer = thread::Builder::new()
            .name("dev-protocol-tap-writer".into())
            .spawn(move || {
                writer_loop(
                    file,
                    receiver,
                    writer_limit,
                    writer_dropped,
                    writer_config,
                    MAX_FILE_BYTES,
                )
            })
            .map_err(|error| {
                format!("não foi possível iniciar o escritor do Protocol Tap: {error}")
            })?;

        Ok(Some(Self {
            config: TapConfig {
                account_id: Some(account_id),
                account_label,
                terms,
                mode,
                record_all_metadata,
            },
            sender,
            writer: Some(writer),
            dropped,
            file_limit_reached,
            path,
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn account_id(&self) -> Option<&str> {
        self.config.account_id.as_deref()
    }

    fn matches_account_id(&self, account_id: &str) -> bool {
        self.config.account_id.as_deref() == Some(account_id)
    }

    pub fn record(
        &self,
        account_id: &str,
        direction: ProtocolDirection,
        transport: ProtocolTransport,
        raw: &str,
    ) {
        if self.file_limit_reached.load(Ordering::Relaxed) || !self.matches_account_id(account_id) {
            return;
        }
        if raw.len() > MAX_RAW_FRAME_BYTES {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        match self.sender.try_send(TapMessage::RawFrame {
            raw: raw.as_bytes().to_vec(),
            direction,
            transport,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.file_limit_reached.store(true, Ordering::Release);
            }
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.sender.send(TapMessage::Shutdown);
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn ensure_smoke_app_data_dir(app_data_dir: &Path) -> Result<(), String> {
    if app_data_dir.file_name().and_then(|name| name.to_str()) == Some("com.pokeidle.manager.smoke")
    {
        Ok(())
    } else {
        Err("DEV protocol tap só pode ser usado no app-data Smoke".into())
    }
}

impl Drop for DevProtocolTap {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn writer_loop(
    file: File,
    receiver: mpsc::Receiver<TapMessage>,
    limit_reached: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    config: TapConfig,
    max_bytes: u64,
) {
    let mut writer = BufWriter::new(file);
    let mut bytes_written = 0_u64;
    while let Ok(message) = receiver.recv() {
        match message {
            #[cfg(test)]
            TapMessage::Line(line) => {
                if bytes_written.saturating_add(line.len() as u64) > max_bytes {
                    limit_reached.store(true, Ordering::Release);
                    break;
                }
                if writer
                    .write_all(&line)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    limit_reached.store(true, Ordering::Release);
                    break;
                }
                bytes_written += line.len() as u64;
            }
            TapMessage::RawFrame {
                raw,
                direction,
                transport,
            } => {
                let Ok(raw) = std::str::from_utf8(&raw) else {
                    dropped.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                let Some(mut event) = build_event(&config, direction, transport, raw) else {
                    continue;
                };
                let dropped_before = dropped.swap(0, Ordering::AcqRel);
                if dropped_before > 0 {
                    event["droppedBefore"] = json!(dropped_before);
                }
                let Ok(mut line) = serde_json::to_vec(&event) else {
                    dropped.fetch_add(dropped_before.saturating_add(1), Ordering::Relaxed);
                    continue;
                };
                if line.len() > MAX_EVENT_BYTES {
                    dropped.fetch_add(dropped_before.saturating_add(1), Ordering::Relaxed);
                    continue;
                }
                line.push(b'\n');
                if bytes_written.saturating_add(line.len() as u64) > max_bytes {
                    limit_reached.store(true, Ordering::Release);
                    break;
                }
                if writer
                    .write_all(&line)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    limit_reached.store(true, Ordering::Release);
                    break;
                }
                bytes_written += line.len() as u64;
            }
            TapMessage::Shutdown => break,
        }
    }
    let _ = writer.flush();
    limit_reached.store(true, Ordering::Release);
}

fn create_session_file(directory: &Path) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let base = format!("protocol-tap-{timestamp}-{}", std::process::id());
    for suffix in 0..100_u8 {
        let name = if suffix == 0 {
            format!("{base}.jsonl")
        } else {
            format!("{base}-{suffix}.jsonl")
        };
        let path = directory.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("não foi possível reservar um nome único para o Protocol Tap".into())
}

/// Keep two prior tap sessions plus the file being created now.
fn prune_old_session_files(directory: &Path) -> Result<(), String> {
    let mut sessions = fs::read_dir(directory)
        .map_err(|error| format!("não foi possível listar arquivos antigos do tap: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if !name.starts_with("protocol-tap-") || !name.ends_with(".jsonl") {
                return None;
            }
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect::<Vec<_>>();
    sessions.sort_by_key(|(modified, _)| *modified);
    let remove_count = sessions
        .len()
        .saturating_sub(MAX_SESSION_FILES.saturating_sub(1));
    for (_, path) in sessions.into_iter().take(remove_count) {
        fs::remove_file(path).map_err(|error| {
            format!("não foi possível limitar arquivos antigos do tap: {error}")
        })?;
    }
    Ok(())
}

fn normalize_account(value: &str) -> Option<String> {
    if value.is_empty()
        || value.len() > 32
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
    {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn parse_terms(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|term| {
            !term.is_empty()
                && term.len() <= 32
                && term
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .map(str::to_ascii_lowercase)
        .take(24)
        .collect()
}

fn build_event(
    config: &TapConfig,
    direction: ProtocolDirection,
    transport: ProtocolTransport,
    raw: &str,
) -> Option<Value> {
    let parsed = (raw.len() <= MAX_RAW_FRAME_BYTES)
        .then(|| serde_json::from_str::<Value>(raw).ok())
        .flatten();
    let mut matches = Vec::new();
    let mut visited = 0;
    if let Some(value) = parsed.as_ref() {
        find_matches(value, "", 0, &mut visited, &config.terms, &mut matches);
    }
    matches.sort();
    matches.dedup();
    matches.truncate(MAX_MATCHES);

    if !config.record_all_metadata && matches.is_empty() {
        return None;
    }

    let mut event = json!({
        "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "account": config.account_label,
        "direction": direction_name(direction),
        "transport": transport_name(transport),
        "frameType": parsed.as_ref().and_then(|value| value.get("t")).and_then(Value::as_str).map(safe_identifier),
        "sizeBytes": raw.len(),
        "topLevelKeys": parsed.as_ref().and_then(Value::as_object).map(top_level_keys).unwrap_or_default(),
        "matchedPaths": matches,
    });

    if config.mode == PayloadMode::FilteredPayload
        && direction == ProtocolDirection::ServerToClient
        && let Some(value) = parsed.as_ref()
    {
        let projections = project_live_state(value);
        if !projections.is_empty() {
            event["payload"] = Value::Array(projections);
        }
    }

    serde_json::to_vec(&event)
        .ok()
        .filter(|line| line.len() <= MAX_EVENT_BYTES)
        .map(|_| event)
}

fn find_matches(
    value: &Value,
    path: &str,
    depth: usize,
    visited: &mut usize,
    terms: &[String],
    matches: &mut Vec<String>,
) {
    if depth > MAX_TREE_DEPTH || *visited >= MAX_TREE_NODES || matches.len() >= MAX_MATCHES {
        return;
    }
    *visited += 1;
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if *visited >= MAX_TREE_NODES || matches.len() >= MAX_MATCHES {
                    break;
                }
                let child_path = join_path(path, &key_segment(key));
                if contains_term(key, terms)
                    || child
                        .as_str()
                        .is_some_and(|text| contains_term(text, terms))
                {
                    matches.push(child_path.clone());
                }
                find_matches(child, &child_path, depth + 1, visited, terms, matches);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().take(MAX_ARRAY_ROWS).enumerate() {
                let child_path = join_path(path, &format!("[{index}]"));
                find_matches(child, &child_path, depth + 1, visited, terms, matches);
                if *visited >= MAX_TREE_NODES || matches.len() >= MAX_MATCHES {
                    break;
                }
            }
        }
        _ => {}
    }
}

fn contains_term(value: &str, terms: &[String]) -> bool {
    let value = value
        .get(..value.len().min(512))
        .unwrap_or(value)
        .to_ascii_lowercase();
    terms.iter().any(|term| value.contains(term))
}

fn project_live_state(value: &Value) -> Vec<Value> {
    let mut projections = Vec::new();
    if let Some(state) = value.get("estado").and_then(Value::as_object) {
        if let Some(twitch) = state.get("twitch").and_then(project_twitch) {
            projections.push(json!({"path": "estado.twitch", "value": twitch}));
        }
        if let Some(kick) = state.get("kick").and_then(project_kick) {
            projections.push(json!({"path": "estado.kick", "value": kick}));
        }
    } else if value.get("t").and_then(Value::as_str) == Some("estado") {
        if let Some(twitch) = value.get("twitch").and_then(project_twitch) {
            projections.push(json!({"path": "twitch", "value": twitch}));
        }
        if let Some(kick) = value.get("kick").and_then(project_kick) {
            projections.push(json!({"path": "kick", "value": kick}));
        }
    }
    projections
}

fn project_twitch(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mut projected = Map::new();
    if let Some(lives) = object.get("lives") {
        projected.insert(
            "lives".into(),
            project_rows(
                lives,
                &[
                    "login",
                    "nome",
                    "espectadores",
                    "aoVivo",
                    "online",
                    "viewers",
                ],
            ),
        );
    }
    if let Some(official) = object.get("oficiais") {
        projected.insert(
            "oficiais".into(),
            project_rows(
                official,
                &[
                    "login",
                    "nome",
                    "bonus",
                    "aoVivo",
                    "espectadores",
                    "online",
                    "viewers",
                ],
            ),
        );
    }
    (!projected.is_empty()).then_some(Value::Object(projected))
}

fn project_kick(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mut projected = Map::new();
    if let Some(official) = object.get("oficiais") {
        projected.insert(
            "oficiais".into(),
            project_rows(
                official,
                &[
                    "slug",
                    "nome",
                    "aoVivo",
                    "espectadores",
                    "ativo",
                    "online",
                    "viewers",
                ],
            ),
        );
    }
    (!projected.is_empty()).then_some(Value::Object(projected))
}

fn project_rows(value: &Value, allowed: &[&str]) -> Value {
    let Some(rows) = value.as_array() else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(MAX_ARRAY_ROWS)
            .map(|row| {
                let Some(object) = row.as_object() else {
                    return Value::Null;
                };
                let mut projection = Map::new();
                for key in allowed {
                    if let Some(value) = object.get(*key).and_then(safe_scalar) {
                        projection.insert((*key).into(), value);
                    }
                }
                Value::Object(projection)
            })
            .collect(),
    )
}

fn safe_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(text)
            if text.len() <= MAX_STRING_BYTES && text.chars().all(|ch| !ch.is_control()) =>
        {
            Some(value.clone())
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(value.clone()),
        _ => None,
    }
}

fn top_level_keys(object: &Map<String, Value>) -> Vec<String> {
    object
        .keys()
        .filter_map(|key| safe_schema_key(key).map(|key| safe_identifier(key)))
        .take(MAX_TOP_LEVEL_KEYS)
        .collect()
}

fn safe_identifier(value: &str) -> String {
    let value = value
        .chars()
        .take(80)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "[unknown]".into()
    } else {
        value
    }
}

fn key_segment(value: &str) -> String {
    safe_schema_key(value)
        .map(safe_identifier)
        .unwrap_or_else(|| "[field]".into())
}

fn safe_schema_key(value: &str) -> Option<&str> {
    const SAFE_KEYS: &[&str] = &[
        "t",
        "estado",
        "twitch",
        "kick",
        "lives",
        "oficiais",
        "login",
        "nome",
        "espectadores",
        "aoVivo",
        "online",
        "viewers",
        "bonus",
        "slug",
        "ativo",
        "assistindo",
        "pct",
        "pctAtual",
        "canal",
        "emLives",
        "assistindoEm",
        "extra",
        "recompensas",
        "streamer",
        "vinculado",
        "minutos",
        "tipo",
        "titulo",
        "custo",
        "data",
        "payload",
        "request",
        "response",
        "id",
        "status",
        "count",
        "total",
    ];
    SAFE_KEYS
        .iter()
        .copied()
        .find(|safe_key| safe_key.eq_ignore_ascii_case(value))
}

fn join_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.into()
    } else {
        format!("{parent}.{child}")
    }
}

fn direction_name(direction: ProtocolDirection) -> &'static str {
    match direction {
        ProtocolDirection::ClientToServer => "client_to_server",
        ProtocolDirection::ServerToClient => "server_to_client",
    }
}

fn transport_name(transport: ProtocolTransport) -> &'static str {
    match transport {
        ProtocolTransport::Browser => "browser",
        ProtocolTransport::Background => "background",
    }
}

#[cfg(test)]
fn create_writer_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::mpsc,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn config(mode: PayloadMode) -> TapConfig {
        TapConfig {
            account_id: Some("account-test-one".into()),
            account_label: "ContaTeste".into(),
            terms: parse_terms(DEFAULT_FILTER),
            mode,
            record_all_metadata: true,
        }
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pokeidle-protocol-tap-test-{label}-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn account_filter_is_case_insensitive_and_rejects_others() {
        let tap_config = config(PayloadMode::MetadataOnly);
        assert_eq!(normalize_account("contateste"), Some("contateste".into()));
        assert_eq!(normalize_account("ContaTeste"), Some("contateste".into()));
        assert_eq!(normalize_account("Conta Teste"), None);
        assert_eq!(tap_config.account_id.as_deref(), Some("account-test-one"));
        let tap = DevProtocolTap {
            config: tap_config,
            sender: mpsc::sync_channel(1).0,
            writer: None,
            dropped: Arc::new(AtomicU64::new(0)),
            file_limit_reached: Arc::new(AtomicBool::new(false)),
            path: PathBuf::new(),
        };
        assert!(tap.matches_account_id("account-test-one"));
        assert!(!tap.matches_account_id("account-other"));
    }

    #[test]
    fn tap_requires_the_smoke_app_data_directory() {
        assert!(
            ensure_smoke_app_data_dir(Path::new(
                r"C:\Users\tester\AppData\Roaming\com.pokeidle.manager.smoke"
            ))
            .is_ok()
        );
        assert!(
            ensure_smoke_app_data_dir(Path::new(
                r"C:\Users\tester\AppData\Roaming\com.pokeidle.manager"
            ))
            .is_err()
        );
    }

    #[test]
    fn records_both_directions_with_metadata_and_structural_matches() {
        let event = build_event(
            &config(PayloadMode::MetadataOnly),
            ProtocolDirection::ClientToServer,
            ProtocolTransport::Browser,
            r#"{"t":"bonus.refresh","estado":{"kick":{"oficiais":[]}}}"#,
        )
        .unwrap();
        assert_eq!(event["direction"], "client_to_server");
        assert_eq!(event["transport"], "browser");
        assert_eq!(event["frameType"], "bonus.refresh");
        assert!(
            event["matchedPaths"]
                .as_array()
                .unwrap()
                .iter()
                .any(|path| path == "estado.kick")
        );
        assert!(event.get("payload").is_none());
    }

    #[test]
    fn filtered_payload_is_allowlisted_and_does_not_copy_unknown_secrets() {
        let event = build_event(
            &config(PayloadMode::FilteredPayload),
            ProtocolDirection::ServerToClient,
            ProtocolTransport::Background,
            r#"{"t":"estado","estado":{"twitch":{"login":"account-secret","lives":[{"login":"streamer","nome":"Streamer","espectadores":44,"access_token":"secret"}],"private":"do-not-copy"},"kick":{"bonus":{"xp":1791907686493},"oficiais":[{"slug":"streamer","nome":"Streamer","aoVivo":true,"espectadores":9,"ativo":true,"token":"secret"}]}}}"#,
        )
        .unwrap();
        let text = event.to_string();
        assert!(text.contains("streamer"));
        assert!(!text.contains("account-secret"));
        assert!(!text.contains("1791907686493"));
        assert!(!text.contains("do-not-copy"));
        assert!(!text.contains("secret"));
        let projections = event["payload"].as_array().unwrap();
        let twitch = projections
            .iter()
            .find(|projection| {
                projection["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(".twitch"))
            })
            .unwrap();
        let kick = projections
            .iter()
            .find(|projection| {
                projection["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(".kick"))
            })
            .unwrap();
        assert_eq!(twitch["value"]["lives"][0]["espectadores"], 44);
        assert_eq!(kick["value"]["oficiais"][0]["aoVivo"], true);
    }

    #[test]
    fn metadata_only_never_emits_payload_values_or_raw_frames() {
        let event = build_event(
            &config(PayloadMode::MetadataOnly),
            ProtocolDirection::ServerToClient,
            ProtocolTransport::Browser,
            r#"{"t":"welcome","estado":{"twitch":{"lives":[{"nome":"PublicChannel"}]},"gold":123456}}"#,
        )
        .unwrap();
        assert!(event.get("payload").is_none());
        assert!(!event.to_string().contains("PublicChannel"));
        assert!(!event.to_string().contains("123456"));
    }

    #[test]
    fn arbitrary_strings_and_unknown_branches_are_never_exported() {
        let event = build_event(
            &config(PayloadMode::FilteredPayload),
            ProtocolDirection::ServerToClient,
            ProtocolTransport::Background,
            r#"{"t":"update","private":{"text":"twitch password hunter2"},"estado":{"twitch":{"extra":"access_token=abc","oficiais":[{"login":"safe-channel","bonus":true,"credential":"secret"}]}}}"#,
        )
        .unwrap();
        let text = event.to_string();
        assert!(!text.contains("private.text"));
        assert!(!text.contains("hunter2"));
        assert!(!text.contains("abc"));
        assert!(!text.contains("secret"));
        assert!(text.contains("safe-channel"));
    }

    #[test]
    fn live_projection_only_reads_confirmed_state_paths() {
        let event = build_event(
            &config(PayloadMode::FilteredPayload),
            ProtocolDirection::ServerToClient,
            ProtocolTransport::Background,
            r#"{"t":"update","unrelated":{"twitch":{"lives":[{"login":"not-a-live-path"}]}},"estado":{"twitch":{"lives":[{"login":"confirmed"}]}}}"#,
        )
        .unwrap();
        let projections = event["payload"].as_array().unwrap();
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0]["path"], "estado.twitch");
        assert!(event.to_string().contains("confirmed"));
        assert!(!event.to_string().contains("not-a-live-path"));
    }

    #[test]
    fn malformed_json_is_metadata_only() {
        let event = build_event(
            &config(PayloadMode::FilteredPayload),
            ProtocolDirection::ServerToClient,
            ProtocolTransport::Browser,
            "not a json frame twitch token=secret",
        )
        .unwrap();
        assert_eq!(event["sizeBytes"], 36);
        assert!(event.get("payload").is_none());
        assert!(!event.to_string().contains("secret"));
    }

    #[test]
    fn writer_stops_at_size_limit_and_flushes_lines() {
        let path = temp_path("rotation-limit");
        let file = create_writer_file(&path).unwrap();
        let (sender, receiver) = mpsc::sync_channel(4);
        let reached = Arc::new(AtomicBool::new(false));
        let writer_flag = reached.clone();
        let dropped = Arc::new(AtomicU64::new(0));
        let writer_config = config(PayloadMode::MetadataOnly);
        let writer = thread::spawn(move || {
            writer_loop(file, receiver, writer_flag, dropped, writer_config, 5)
        });
        sender
            .try_send(TapMessage::Line(b"1234\n".to_vec()))
            .unwrap();
        sender
            .try_send(TapMessage::Line(b"5678\n".to_vec()))
            .unwrap();
        let _ = sender.send(TapMessage::Shutdown);
        writer.join().unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "1234\n");
        assert!(reached.load(Ordering::Acquire));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn dedicated_writer_sanitizes_and_flushes_projected_frames() {
        let path = temp_path("async-sanitize");
        let file = create_writer_file(&path).unwrap();
        let (sender, receiver) = mpsc::sync_channel(2);
        let reached = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let writer = thread::spawn({
            let reached = reached.clone();
            let dropped = dropped.clone();
            let config = config(PayloadMode::FilteredPayload);
            move || writer_loop(file, receiver, reached, dropped, config, MAX_FILE_BYTES)
        });
        sender
            .try_send(TapMessage::RawFrame {
                raw: br#"{"t":"welcome","estado":{"twitch":{"lives":[{"login":"streamer","access_token":"do-not-write"}]},"secret":"not-projected"}}"#.to_vec(),
                direction: ProtocolDirection::ServerToClient,
                transport: ProtocolTransport::Background,
            })
            .unwrap();
        sender.try_send(TapMessage::Shutdown).unwrap();
        writer.join().unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("streamer"));
        assert!(!contents.contains("do-not-write"));
        assert!(!contents.contains("not-projected"));
        assert!(contents.ends_with('\n'));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn safe_projection_rejects_objects_and_oversized_strings() {
        let projected = project_rows(
            &json!([{"login":"ok","nome":"n".repeat(100),"espectadores":{"secret":1}}]),
            &["login", "nome", "espectadores"],
        );
        assert_eq!(projected[0].get("login").unwrap(), "ok");
        assert!(projected[0].get("nome").is_none());
        assert!(projected[0].get("espectadores").is_none());
    }
}
