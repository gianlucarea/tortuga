use anyhow::Result;

use crate::config::{EndpointConfig, GlobalConfig};
use crate::state::Status;

fn alert_text(ep: &EndpointConfig, new_status: &Status) -> String {
    format!("[{}] status is now {}", ep.name, new_status.label())
}

async fn fire_webhook(url: &str, text: &str, client: &reqwest::Client) -> Result<()> {
    let body = serde_json::json!({ "text": text });
    client.post(url).json(&body).send().await?.error_for_status()?;
    Ok(())
}

async fn fire_telegram(
    base: &str,
    token: &str,
    chat_id: &str,
    text: &str,
    client: &reqwest::Client,
) -> Result<()> {
    let url = format!("{}/bot{}/sendMessage", base, token);
    let body = serde_json::json!({ "chat_id": chat_id, "text": text });
    client.post(&url).json(&body).send().await?.error_for_status()?;
    Ok(())
}

async fn fire_alert_inner(
    ep: &EndpointConfig,
    global: &GlobalConfig,
    new_status: &Status,
    client: &reqwest::Client,
    telegram_base: &str,
) {
    let text = alert_text(ep, new_status);
    let webhook_url = ep.webhook_url.as_deref().or(global.webhook_url.as_deref());
    let tg_token = global.telegram_bot_token.as_deref();
    let tg_chat_id = ep.telegram_chat_id.as_deref().or(global.telegram_chat_id.as_deref());

    let wh = async {
        if let Some(url) = webhook_url {
            if let Err(e) = fire_webhook(url, &text, client).await {
                eprintln!("webhook alert error: {e}");
            }
        }
    };

    let tg = async {
        if let (Some(token), Some(chat_id)) = (tg_token, tg_chat_id) {
            if let Err(e) = fire_telegram(telegram_base, token, chat_id, &text, client).await {
                eprintln!("telegram alert error: {e}");
            }
        }
    };

    tokio::join!(wh, tg);
}

pub async fn fire_alert(
    ep: &EndpointConfig,
    global: &GlobalConfig,
    new_status: &Status,
    client: &reqwest::Client,
) {
    fire_alert_inner(ep, global, new_status, client, "https://api.telegram.org").await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EndpointConfig, GlobalConfig};
    use crate::state::Status;

    fn make_ep(webhook_url: Option<String>, telegram_chat_id: Option<String>) -> EndpointConfig {
        EndpointConfig {
            name: "test-svc".to_string(),
            url: "https://example.com/health".to_string(),
            interval_secs: 30,
            expected_status: None,
            timeout_secs: 10,
            webhook_url,
            telegram_chat_id,
        }
    }

    fn make_global(
        webhook_url: Option<String>,
        telegram_bot_token: Option<String>,
        telegram_chat_id: Option<String>,
    ) -> GlobalConfig {
        GlobalConfig {
            state_file: "state.json".to_string(),
            webhook_url,
            telegram_bot_token,
            telegram_chat_id,
        }
    }

    #[tokio::test]
    async fn webhook_posts_slack_compatible_json() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/hook")
            .match_header("content-type", mockito::Matcher::Regex("application/json".to_string()))
            .match_body(mockito::Matcher::PartialJson(
                serde_json::json!({ "text": "[test-svc] status is now DOWN" }),
            ))
            .with_status(200)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(Some(format!("{}/hook", server.url())), None);
        let global = make_global(None, None, None);
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Down, &client, "").await;

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn endpoint_webhook_overrides_global() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/ep-hook")
            .with_status(200)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(Some(format!("{}/ep-hook", server.url())), None);
        let global = make_global(Some("http://127.0.0.1:1/not-called".to_string()), None, None);
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Up, &client, "").await;

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn global_webhook_used_as_fallback() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/global-hook")
            .with_status(200)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(None, None);
        let global = make_global(Some(format!("{}/global-hook", server.url())), None, None);
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Down, &client, "").await;

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn telegram_posts_to_bot_api() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/botmy-token/sendMessage")
            .match_body(mockito::Matcher::PartialJson(
                serde_json::json!({ "chat_id": "chat123" }),
            ))
            .with_status(200)
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(None, None);
        let global =
            make_global(None, Some("my-token".to_string()), Some("chat123".to_string()));
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Up, &client, &server.url()).await;

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn endpoint_telegram_chat_id_overrides_global() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/botshared-token/sendMessage")
            .match_body(mockito::Matcher::PartialJson(
                serde_json::json!({ "chat_id": "ep-chat" }),
            ))
            .with_status(200)
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(None, Some("ep-chat".to_string()));
        let global = make_global(
            None,
            Some("shared-token".to_string()),
            Some("global-chat".to_string()),
        );
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Down, &client, &server.url()).await;

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn no_alert_config_completes_silently() {
        let ep = make_ep(None, None);
        let global = make_global(None, None, None);
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Down, &client, "").await;
    }

    #[tokio::test]
    async fn webhook_http_error_is_non_fatal() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/fail")
            .with_status(500)
            .expect(1)
            .create_async()
            .await;

        let ep = make_ep(Some(format!("{}/fail", server.url())), None);
        let global = make_global(None, None, None);
        let client = reqwest::Client::new();
        fire_alert_inner(&ep, &global, &Status::Down, &client, "").await;
    }
}

