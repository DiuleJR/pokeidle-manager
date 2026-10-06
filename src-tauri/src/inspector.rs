//! Development-only, in-memory protocol capture.
//! Frames are sanitized before entering the bounded per-account buffer and are
//! never written to the database.
use crate::logging::sanitize_frame;
use serde::Serialize;

pub const FRAME_CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolDirection {
    ClientToServer,
    ServerToClient,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolFrame {
    pub timestamp_ms: u64,
    pub direction: ProtocolDirection,
    pub payload: String,
}

pub fn sanitized_frame(direction: ProtocolDirection, raw: &str) -> ProtocolFrame {
    ProtocolFrame {
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        direction,
        payload: sanitize_frame(raw),
    }
}
