pub fn init() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "pokeidle_manager_lib=info".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
pub fn sanitize_frame(raw: &str) -> String {
    let mut value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return "[non-json frame]".into(),
    };
    redact(&mut value);
    value.to_string()
}
fn redact(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if [
                    "token",
                    "access_token",
                    "refresh_token",
                    "cookie",
                    "cookies",
                    "set-cookie",
                    "authorization",
                    "dispositivo",
                    "deviceid",
                    "device_id",
                ]
                .iter()
                .any(|s| key.eq_ignore_ascii_case(s))
                {
                    *value = serde_json::Value::String("[REDACTED]".into());
                } else {
                    redact(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact(value);
            }
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_secrets() {
        assert!(
            !sanitize_frame(r#"{"token":"secret","nested":{"authorization":"bearer"}}"#)
                .contains("secret")
        );
    }
}
