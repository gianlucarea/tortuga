// TODO Phase 4: Webhook alert dispatch.
// On status change fire a non-blocking tokio::spawn that POSTs a Slack-compatible
// JSON payload { "text": "..." } to the configured webhook URL.
