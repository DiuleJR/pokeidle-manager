//! The background game connection. Bootstrap material exists only in memory.
use crate::{
    accounts::AccountManager,
    domain::AccountRuntimeState,
    inspector::ProtocolDirection,
    protocol::{ClientFrame, Hello, ServerFrame},
};
use futures_util::{SinkExt, StreamExt};
use std::{fmt, time::Duration};
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Error as WebSocketError, Message,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
    },
};
use tokio_util::sync::CancellationToken;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(90);
const HEARTBEAT_SEND_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_SEND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct SessionHeaders(Vec<(String, String)>);
impl SessionHeaders {
    pub fn from_cdp(value: &serde_json::Value) -> Self {
        // CDP exposes the exact request headers. WebSocket transport headers are
        // deliberately left to tungstenite, which must generate them itself.
        const TRANSPORT_HEADERS: &[&str] = &[
            "host",
            "connection",
            "upgrade",
            "sec-websocket-key",
            "sec-websocket-version",
            "sec-websocket-extensions",
            "sec-websocket-protocol",
            "content-length",
        ];
        let headers = value
            .as_object()
            .into_iter()
            .flat_map(|object| object.iter())
            .filter_map(|(name, value)| {
                let normalized = name.to_ascii_lowercase();
                (!TRANSPORT_HEADERS.contains(&normalized.as_str()))
                    .then(|| value.as_str().map(|value| (name.clone(), value.to_owned())))
                    .flatten()
            })
            .collect();
        Self(headers)
    }
    fn apply(&self, request: &mut tokio_tungstenite::tungstenite::handshake::client::Request) {
        for (name, value) in &self.0 {
            let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(value),
            ) else {
                continue;
            };
            request.headers_mut().insert(name, value);
        }
    }
}
impl fmt::Debug for SessionHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SessionHeaders")
            .field(&"[REDACTED]")
            .finish()
    }
}

/// Never serialize or log this. It is discarded at application shutdown.
#[derive(Clone)]
pub struct ConnectionBootstrap {
    pub account_id: String,
    pub ws_url: String,
    pub headers: SessionHeaders,
    pub hello: Hello,
}
impl fmt::Debug for ConnectionBootstrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionBootstrap")
            .field("account_id", &self.account_id)
            .field("ws_url", &"[REDACTED]")
            .field("headers", &self.headers)
            .field("hello", &self.hello)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("URL WebSocket inválida: {0}")]
    Request(String),
    #[error("não foi possível conectar ao WebSocket Rust: {0}")]
    Connect(String),
    #[error("servidor rejeitou a sessão da conta (HTTP {0})")]
    AuthenticationRejected(u16),
    #[error("não foi possível enviar hello pelo WebSocket Rust: {0}")]
    SendHello(String),
    #[error("tempo esgotado aguardando welcome do WebSocket Rust")]
    WelcomeTimeout,
    #[error("WebSocket Rust encerrou antes do welcome")]
    ClosedBeforeWelcome,
    #[error("não foi possível ler o WebSocket Rust: {0}")]
    Receive(String),
    #[error("fila de comandos da conta indisponível: {0}")]
    CommandQueue(String),
}

pub struct BackgroundConnection {
    cancellation: CancellationToken,
    receiver: JoinHandle<()>,
    processor: JoinHandle<()>,
    commands: mpsc::Sender<ClientFrame>,
}
impl BackgroundConnection {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    /// Gracefully stop both socket processing tasks before another transport
    /// is allowed to become active for this account.
    pub async fn shutdown(mut self) {
        self.cancellation.cancel();
        let _ = (&mut self.receiver).await;
        let _ = (&mut self.processor).await;
    }

    #[cfg(test)]
    pub(crate) fn test_connection(
        completion_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        use std::sync::atomic::Ordering;

        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let receiver_completion = completion_count.clone();
        let receiver = tokio::spawn(async move {
            receiver_cancellation.cancelled().await;
            receiver_completion.fetch_add(1, Ordering::AcqRel);
        });
        let processor_cancellation = cancellation.clone();
        let processor = tokio::spawn(async move {
            processor_cancellation.cancelled().await;
            completion_count.fetch_add(1, Ordering::AcqRel);
        });
        let (commands, _command_receiver) = mpsc::channel(1);

        Self {
            cancellation,
            receiver,
            processor,
            commands,
        }
    }

    pub fn dispatch(&self, command: ClientFrame) -> Result<(), ConnectionError> {
        self.commands
            .try_send(command)
            .map_err(|error| ConnectionError::CommandQueue(error.to_string()))
    }
}
impl Drop for BackgroundConnection {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.receiver.abort();
        self.processor.abort();
    }
}

pub struct ConnectionManager;
impl ConnectionManager {
    async fn open_authenticated(
        bootstrap: &ConnectionBootstrap,
        accounts: &AccountManager,
    ) -> Result<
        (
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            ServerFrame,
        ),
        ConnectionError,
    > {
        let mut request = bootstrap
            .ws_url
            .clone()
            .into_client_request()
            .map_err(|error| ConnectionError::Request(error.to_string()))?;
        bootstrap.headers.apply(&mut request);
        let (mut socket, _) = connect_async(request).await.map_err(|error| match error {
            WebSocketError::Http(response) if matches!(response.status().as_u16(), 401 | 403) => {
                ConnectionError::AuthenticationRejected(response.status().as_u16())
            }
            error => ConnectionError::Connect(error.to_string()),
        })?;
        let hello = bootstrap.hello.to_wire();
        socket
            .send(Message::Text(hello.clone().into()))
            .await
            .map_err(|error| ConnectionError::SendHello(error.to_string()))?;
        accounts.record_protocol_frame(
            &bootstrap.account_id,
            ProtocolDirection::ClientToServer,
            &hello,
        );
        let welcome = tokio::time::timeout(Duration::from_secs(15), async {
            while let Some(message) = socket.next().await {
                let message =
                    message.map_err(|error| ConnectionError::Receive(error.to_string()))?;
                if let Message::Text(payload) = message {
                    accounts.record_protocol_frame(
                        &bootstrap.account_id,
                        ProtocolDirection::ServerToClient,
                        &payload,
                    );
                    let frame = ServerFrame::parse(&payload)
                        .map_err(|error| ConnectionError::Receive(error.to_string()))?;
                    if matches!(frame, ServerFrame::Welcome(_)) {
                        return Ok(frame);
                    }
                }
            }
            Err(ConnectionError::ClosedBeforeWelcome)
        })
        .await
        .map_err(|_| ConnectionError::WelcomeTimeout)??;
        Ok((socket, welcome))
    }

    /// Handoff only succeeds after this method has received a real `welcome`.
    pub async fn connect(
        bootstrap: ConnectionBootstrap,
        accounts: AccountManager,
    ) -> Result<BackgroundConnection, ConnectionError> {
        let (mut socket, first_frame) = Self::open_authenticated(&bootstrap, &accounts).await?;

        // The first welcome establishes ownership before the browser is closed.
        let _ = accounts.ingest(&bootstrap.account_id, first_frame);
        let cancellation = CancellationToken::new();
        let (frame_tx, mut frame_rx) = mpsc::channel::<ServerFrame>(256);
        let (command_tx, mut command_rx) = mpsc::channel::<ClientFrame>(32);
        let receiver_cancel = cancellation.clone();
        let receiver_accounts = accounts.clone();
        let receiver_bootstrap = bootstrap.clone();
        let receiver = tokio::spawn(async move {
            loop {
                let mut disconnected = false;
                let mut disconnect_reason = "background websocket disconnected".to_owned();
                let mut last_pong_at = tokio::time::Instant::now();
                let mut next_heartbeat = tokio::time::Instant::now() + HEARTBEAT_INTERVAL;
                loop {
                    tokio::select! {
                        _ = receiver_cancel.cancelled() => break,
                        _ = tokio::time::sleep_until(next_heartbeat) => {
                            let now = tokio::time::Instant::now();
                            if now.duration_since(last_pong_at) >= HEARTBEAT_TIMEOUT {
                                tracing::warn!(account_id = %receiver_bootstrap.account_id, "background websocket heartbeat timed out");
                                disconnect_reason = "heartbeat timeout".into();
                                disconnected = true;
                                break;
                            }
                            let heartbeat_sent = tokio::time::timeout(
                                HEARTBEAT_SEND_TIMEOUT,
                                socket.send(Message::Ping(Vec::new().into())),
                            )
                            .await
                            .is_ok_and(|result| result.is_ok());
                            if !heartbeat_sent {
                                tracing::warn!(account_id = %receiver_bootstrap.account_id, "background websocket heartbeat send failed");
                                disconnect_reason = "heartbeat send failed".into();
                                disconnected = true;
                                break;
                            }
                            next_heartbeat = now + HEARTBEAT_INTERVAL;
                        }
                        command = command_rx.recv() => match command {
                            Some(command) => match serde_json::to_string(&command) {
                                Ok(payload) => {
                                    match tokio::time::timeout(
                                        COMMAND_SEND_TIMEOUT,
                                        socket.send(Message::Text(payload.clone().into())),
                                    )
                                    .await
                                    {
                                        Ok(Ok(())) => {}
                                        Ok(Err(error)) => {
                                            tracing::warn!(account_id = %receiver_bootstrap.account_id, %error, "background websocket command send failed");
                                            disconnect_reason = format!("command send failed: {error}");
                                            disconnected = true;
                                            break;
                                        }
                                        Err(_) => {
                                            tracing::warn!(account_id = %receiver_bootstrap.account_id, "background websocket command send timed out");
                                            disconnect_reason = "command send timeout".into();
                                            disconnected = true;
                                            break;
                                        }
                                    }
                                    receiver_accounts.record_protocol_frame(
                                        &receiver_bootstrap.account_id,
                                        ProtocolDirection::ClientToServer,
                                        &payload,
                                    );
                                }
                                Err(error) => tracing::warn!(%error, "could not serialize game command"),
                            },
                            None => break,
                        },
                        message = socket.next() => match message {
                            Some(Ok(Message::Text(payload))) => {
                                receiver_accounts.record_protocol_frame(
                                    &receiver_bootstrap.account_id,
                                    ProtocolDirection::ServerToClient,
                                    &payload,
                                );
                                if let Ok(frame) = ServerFrame::parse(&payload) {
                                    // The socket reader never waits on UI/SQLite work.
                                    if let Err(error) = frame_tx.try_send(frame) {
                                        tracing::warn!(account_id = %receiver_bootstrap.account_id, %error, "background websocket frame queue is full or closed; reconnecting to resynchronize");
                                        disconnect_reason = format!("frame processing queue unavailable: {error}");
                                        disconnected = true;
                                        break;
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                if let Err(error) = socket.send(Message::Pong(payload)).await {
                                    disconnect_reason = format!("pong send failed: {error}");
                                    disconnected = true;
                                    break;
                                }
                            }
                            Some(Ok(Message::Pong(_))) => {
                                last_pong_at = tokio::time::Instant::now();
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                disconnect_reason = error.to_string();
                                tracing::info!(reason = %disconnect_reason, "[account] websocket disconnected");
                                disconnected = true; break;
                            }
                            None => {
                                tracing::info!("[account] websocket disconnected: remote closed");
                                disconnect_reason = "remote closed".into();
                                disconnected = true; break;
                            }
                        }
                    }
                }
                if receiver_cancel.is_cancelled() {
                    break;
                }
                if !disconnected {
                    continue;
                }
                let _ = receiver_accounts
                    .websocket_disconnected(&receiver_bootstrap.account_id, disconnect_reason);
                let _ = receiver_accounts.transition(
                    &receiver_bootstrap.account_id,
                    AccountRuntimeState::Reconnecting,
                );
                while command_rx.try_recv().is_ok() {}
                let mut reconnected = false;
                let mut attempt = 0_u32;
                let mut authentication_rejected = false;
                while !receiver_cancel.is_cancelled() {
                    let attempt_number = attempt.saturating_add(1);
                    let _ = receiver_accounts
                        .reconnect_attempt(&receiver_bootstrap.account_id, attempt_number);
                    tracing::info!(attempt = attempt_number, "[account] reconnect attempt");
                    tokio::select! {
                        _ = receiver_cancel.cancelled() => break,
                        _ = tokio::time::sleep(Self::reconnect_delay(attempt)) => {}
                    }
                    if receiver_cancel.is_cancelled() {
                        break;
                    }
                    let result = tokio::select! {
                        _ = receiver_cancel.cancelled() => break,
                        result = tokio::time::timeout(
                            Duration::from_secs(20),
                            Self::open_authenticated(&receiver_bootstrap, &receiver_accounts),
                        ) => result,
                    };
                    match result {
                        Ok(Ok((new_socket, welcome))) => {
                            if let Err(error) = frame_tx.try_send(welcome) {
                                tracing::warn!(account_id = %receiver_bootstrap.account_id, %error, "background websocket could not queue reconnect welcome");
                                attempt = attempt.saturating_add(1);
                                continue;
                            }
                            socket = new_socket;
                            let _ = receiver_accounts.transition(
                                &receiver_bootstrap.account_id,
                                AccountRuntimeState::Background,
                            );
                            let _ = receiver_accounts
                                .websocket_reconnected(&receiver_bootstrap.account_id);
                            tracing::info!(
                                attempt = attempt_number,
                                "[account] reconnect successful"
                            );
                            reconnected = true;
                            break;
                        }
                        Ok(Err(error @ ConnectionError::AuthenticationRejected(_))) => {
                            tracing::warn!(%error, "background websocket session rejected; attempting silent profile renewal");
                            authentication_rejected = true;
                            break;
                        }
                        Ok(Err(error)) => {
                            tracing::warn!(%error, attempt = attempt_number, "background websocket reconnect failed; retrying");
                        }
                        Err(_) => {
                            tracing::warn!(
                                attempt = attempt_number,
                                "background websocket reconnect timed out; retrying"
                            );
                        }
                    }
                    attempt = attempt.saturating_add(1);
                }
                if !reconnected && authentication_rejected {
                    let _ = receiver_accounts.transition(
                        &receiver_bootstrap.account_id,
                        AccountRuntimeState::RenewingSession,
                    );
                    break;
                }
                if receiver_cancel.is_cancelled() {
                    break;
                }
            }
        });
        let processor_cancel = cancellation.clone();
        let processor_accounts = accounts;
        let processor_account_id = bootstrap.account_id;
        let processor = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = processor_cancel.cancelled() => break,
                    frame = frame_rx.recv() => match frame {
                        Some(frame) => { let _ = processor_accounts.ingest(&processor_account_id, frame); }
                        None => break,
                    }
                }
            }
        });
        Ok(BackgroundConnection {
            cancellation,
            receiver,
            processor,
            commands: command_tx,
        })
    }

    pub fn reconnect_delay(attempt: u32) -> Duration {
        Duration::from_secs((1_u64 << attempt.min(6)).min(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_debug_redacts_session_material() {
        let bootstrap = ConnectionBootstrap {
            account_id: "account".into(),
            ws_url: "wss://secret.example/socket?token=x".into(),
            headers: SessionHeaders(vec![("Cookie".into(), "secret".into())]),
            hello: Hello {
                nick: "n".into(),
                token: "secret".into(),
                delta: 1,
                dispositivo: "d".into(),
            },
        };
        let printed = format!("{bootstrap:?}");
        assert!(!printed.contains("secret"));
        assert!(!printed.contains("token=x"));
    }
    #[test]
    fn reconnect_backoff_is_bounded() {
        assert_eq!(
            ConnectionManager::reconnect_delay(0),
            Duration::from_secs(1)
        );
        assert_eq!(
            ConnectionManager::reconnect_delay(3),
            Duration::from_secs(8)
        );
        assert_eq!(
            ConnectionManager::reconnect_delay(9),
            Duration::from_secs(60)
        );
    }
}
