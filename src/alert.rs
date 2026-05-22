use crate::config::{EndpointConfig, GlobalConfig};
use crate::state::Status;

/// Fire alerts for a status change (webhook + Telegram).
/// Phase 4 will implement the actual dispatch; this stub is a no-op.
pub async fn fire_alert(
    _ep: &EndpointConfig,
    _global: &GlobalConfig,
    _new_status: &Status,
    _client: &reqwest::Client,
) {
    // TODO Phase 4: POST Slack-compatible JSON to webhook_url (per-endpoint → global fallback)
    // TODO Phase 4: POST to Telegram Bot API if token + chat_id are configured
}
