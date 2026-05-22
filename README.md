# Tortuga 🏴‍☠️

> *"Make way! Make way for Tortuga!"*
> — Captain Barbossa

**A terminal-based HTTP endpoint monitor written in Rust**

Tortuga watches your HTTP services, renders a live dashboard in the terminal, and fires instant alerts via **Telegram** and Slack/Discord webhooks whenever a service goes up or down.

## Features

- **Live TUI dashboard** — real-time ratatui table refreshed every second, showing status, HTTP code, uptime %, response time, and last-checked time
- **Concurrent monitoring** — each endpoint runs in its own async task with an independent polling interval and timeout
- **Telegram alerts** — instant push notifications via the Telegram Bot API to a channel or chat of your choice
- **Webhook alerts** — Slack-compatible JSON POST to any webhook URL (Slack, Discord, etc.)
- **State persistence** — uptime counters and last status survive restarts via a local `state.json` file
- **Graceful shutdown** — press `q` or `Ctrl-C` to exit cleanly; state is flushed to disk


## Dashboard

```
┌─────────────────────────────── Tortuga ────────────────────────────────┐
│ Name            URL                            Status  HTTP  Uptime  ms │
│ httpbin OK      https://httpbin.org/status/200 UP      200   100%    87 │
│ httpbin 404     https://httpbin.org/status/404 DOWN    404    0%    102 │
│ Example.com     https://example.com            UP      200    99%   210 │
├─────────────────────────────── Events ─────────────────────────────────┤
│ 16:34:01  [httpbin OK] Unknown → UP (HTTP 200)                          │
│ 16:34:02  [httpbin 404] Unknown → DOWN (HTTP 404)                       │
└────────────────────────────────────────────────────────────────────────┘
```

- Green = UP · Red = DOWN · Yellow = Unknown / timeout


## Telegram Alerts

![Telegram alerts preview](images/telegram.png)

Alerts are pushed to a Telegram channel or chat the moment an endpoint changes status.

### Setting up Telegram

1. **Create a bot** — open [@BotFather](https://t.me/BotFather) on Telegram, send `/newbot`, and copy the token it gives you.
2. **Find your chat ID**
   - For a **public channel**: use `@yourchannel` as the chat ID.
   - For a **private channel / group**: add the bot as an admin, then call `https://api.telegram.org/bot<token>/getUpdates` — the `"chat"."id"` field in the response is your chat ID (negative number for channels, e.g. `-1001234567890`).
3. Add the values to `config.toml` (see configuration below).


## Installation

**Prerequisites:** Rust 1.80+ and Cargo ([rustup.rs](https://rustup.rs))

```bash
git clone https://github.com/youruser/tortuga
cd tortuga
cargo build --release
# Binary is at target/release/tortuga
```


## Configuration

Create (or edit) `config.toml` next to the binary:

```toml
[global]
state_file = "state.json"           # path to persist uptime state

# --- Telegram (optional) ---
telegram_bot_token = "123456:ABC-DEF..."   # from @BotFather
telegram_chat_id   = "@mychannel"          # public: "@chan" | private: "-1001234567890"

# --- Webhook / Slack / Discord (optional) ---
# webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

[[endpoints]]
name            = "My API"
url             = "https://api.example.com/health"
interval_secs   = 30
expected_status = 200      # optional — omit to treat any 2xx as UP
timeout_secs    = 10       # optional, default 10

# Per-endpoint overrides (both optional):
# webhook_url      = "https://..."
# telegram_chat_id = "-1009876543210"

[[endpoints]]
name          = "Example.com"
url           = "https://example.com"
interval_secs = 60
timeout_secs  = 15
```

### Configuration reference

| Key | Scope | Description |
|-----|-------|-------------|
| `state_file` | global | Path to the JSON file used for state persistence |
| `telegram_bot_token` | global | Telegram bot token from @BotFather |
| `telegram_chat_id` | global / endpoint | Target channel or chat (endpoint value overrides global) |
| `webhook_url` | global / endpoint | Slack-compatible webhook URL (endpoint value overrides global) |
| `name` | endpoint | Display name in the dashboard |
| `url` | endpoint | URL to poll |
| `interval_secs` | endpoint | Polling interval in seconds |
| `expected_status` | endpoint | Exact HTTP status for UP; omit for any 2xx |
| `timeout_secs` | endpoint | Request timeout in seconds (default: 10) |


## Usage

```bash
# Use the default config.toml in the current directory
./tortuga

# Specify a custom config file
./tortuga --config /etc/tortuga/prod.toml
```

Press **`q`** or **`Ctrl-C`** to exit. The current state is saved to `state.json` on quit and reloaded on the next startup, preserving uptime percentages.


## Project structure

```
src/
  main.rs      — CLI args, task orchestration, graceful shutdown
  config.rs    — TOML deserialization
  state.rs     — EndpointState, SharedState, load/save JSON
  monitor.rs   — async polling tasks
  alert.rs     — Telegram + webhook delivery
  ui.rs        — ratatui TUI render loop
```


## License

[MIT](LICENSE)