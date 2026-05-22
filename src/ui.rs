use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Terminal,
};

use crate::config::Config;
use crate::state::{SharedLog, SharedState, Status};

pub fn run_ui(config: &Config, state: SharedState, log: SharedLog) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, config, &state, &log);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &Config,
    state: &SharedState,
    log: &SharedLog,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, config, state, log))?;

        if event::poll(Duration::from_millis(1000))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _)
                    | (KeyCode::Char('Q'), _)
                    | (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

fn status_color(status: Option<&Status>) -> Color {
    match status {
        Some(Status::Up) => Color::Green,
        Some(Status::Down) => Color::Red,
        _ => Color::Yellow,
    }
}

fn draw(f: &mut ratatui::Frame, config: &Config, state: &SharedState, log: &SharedLog) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(12),
        ])
        .split(area);

    // ── Title bar ────────────────────────────────────────────────────────────
    let title = Paragraph::new(Span::styled(
        " tortuga  HTTP endpoint monitor  (press q to quit) ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    f.render_widget(title, chunks[0]);

    // ── Endpoints table ───────────────────────────────────────────────────────
    let header = Row::new(
        ["Name", "URL", "Status", "HTTP", "Uptime %", "Resp ms", "Last Checked"]
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::White),
                )
            }),
    )
    .style(Style::default().bg(Color::DarkGray))
    .height(1);

    let state_map = state.lock().unwrap();

    let rows: Vec<Row> = config
        .endpoints
        .iter()
        .map(|ep| {
            let es = state_map.get(&ep.name);

            let status = es.map(|e| &e.status);
            let color = status_color(status);
            let status_label = match status {
                Some(Status::Up) => "UP",
                Some(Status::Down) => "DOWN",
                _ => "---",
            };

            let http = es
                .and_then(|e| e.last_code)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "---".into());

            let uptime = es
                .and_then(|e| e.uptime_pct())
                .map(|p| format!("{:.1}%", p))
                .unwrap_or_else(|| "---".into());

            let resp = es
                .and_then(|e| e.response_ms)
                .map(|ms| ms.to_string())
                .unwrap_or_else(|| "---".into());

            let checked = es
                .and_then(|e| e.last_checked)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "---".into());

            Row::new(vec![
                Cell::from(ep.name.clone()),
                Cell::from(ep.url.clone()),
                Cell::from(status_label)
                    .style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Cell::from(http),
                Cell::from(uptime),
                Cell::from(resp),
                Cell::from(checked),
            ])
        })
        .collect();

    drop(state_map);

    let widths = [
        Constraint::Percentage(15),
        Constraint::Percentage(30),
        Constraint::Percentage(8),
        Constraint::Percentage(7),
        Constraint::Percentage(10),
        Constraint::Percentage(10),
        Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(" Endpoints "));

    f.render_widget(table, chunks[1]);

    // ── Event log panel ───────────────────────────────────────────────────────
    let log_entries = log.lock().unwrap();
    let visible = chunks[2].height.saturating_sub(2) as usize;
    let log_lines: Vec<Line> = log_entries
        .iter()
        .rev()
        .take(visible)
        .map(|ev| {
            Line::from(format!(
                "{} {}",
                ev.ts.format("%H:%M:%S"),
                ev.message
            ))
        })
        .collect();
    drop(log_entries);

    let log_widget = Paragraph::new(log_lines)
        .block(Block::default().borders(Borders::ALL).title(" Events "))
        .wrap(Wrap { trim: true });

    f.render_widget(log_widget, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::config::{EndpointConfig, GlobalConfig};
    use crate::state::{EndpointState, LogEvent, Status};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_ep(name: &str, url: &str) -> EndpointConfig {
        EndpointConfig {
            name: name.to_string(),
            url: url.to_string(),
            interval_secs: 30,
            expected_status: None,
            timeout_secs: 10,
            webhook_url: None,
            telegram_chat_id: None,
        }
    }

    fn make_config(endpoints: Vec<(&str, &str)>) -> Config {
        Config {
            global: GlobalConfig::default(),
            endpoints: endpoints.into_iter().map(|(n, u)| make_ep(n, u)).collect(),
        }
    }

    fn empty_state() -> SharedState {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn empty_log() -> SharedLog {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// Render `draw` into a `TestBackend` and return every symbol concatenated.
    fn render(config: &Config, state: SharedState, log: SharedLog) -> String {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw(f, config, &state, &log))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    // ── status_color ─────────────────────────────────────────────────────────

    #[test]
    fn status_color_up_is_green() {
        assert_eq!(status_color(Some(&Status::Up)), Color::Green);
    }

    #[test]
    fn status_color_down_is_red() {
        assert_eq!(status_color(Some(&Status::Down)), Color::Red);
    }

    #[test]
    fn status_color_unknown_is_yellow() {
        assert_eq!(status_color(Some(&Status::Unknown)), Color::Yellow);
    }

    #[test]
    fn status_color_none_is_yellow() {
        assert_eq!(status_color(None), Color::Yellow);
    }

    // ── draw — title bar ─────────────────────────────────────────────────────

    #[test]
    fn draw_contains_title() {
        let cfg = make_config(vec![("svc", "https://example.com")]);
        let out = render(&cfg, empty_state(), empty_log());
        assert!(out.contains("tortuga"), "title bar should contain 'tortuga'");
    }

    // ── draw — endpoints table ────────────────────────────────────────────────

    #[test]
    fn draw_shows_endpoint_name_and_url() {
        // Keep the URL short enough to fit within the 30% column at 120 cols.
        let cfg = make_config(vec![("my-api", "https://example.com")]);
        let out = render(&cfg, empty_state(), empty_log());
        assert!(out.contains("my-api"), "should render endpoint name");
        assert!(out.contains("https://example.com"), "should render endpoint URL");
    }

    #[test]
    fn draw_no_state_shows_unknown_placeholder() {
        let cfg = make_config(vec![("svc", "https://x.com")]);
        // No entry in state → all columns show "---"
        let out = render(&cfg, empty_state(), empty_log());
        assert!(out.contains("---"), "missing state should render '---' placeholder");
    }

    #[test]
    fn draw_up_status_renders_up_label() {
        let cfg = make_config(vec![("svc", "https://x.com")]);

        let state = empty_state();
        state.lock().unwrap().insert(
            "svc".to_string(),
            EndpointState {
                status: Status::Up,
                last_code: Some(200),
                last_checked: Some(Utc::now()),
                response_ms: Some(42),
                total_checks: 5,
                up_checks: 5,
            },
        );

        let out = render(&cfg, state, empty_log());
        assert!(out.contains("UP"), "UP endpoint should render 'UP'");
        assert!(out.contains("200"), "should render HTTP status code");
        assert!(out.contains("100.0%"), "should render uptime percentage");
        assert!(out.contains("42"), "should render response time");
    }

    #[test]
    fn draw_down_status_renders_down_label() {
        let cfg = make_config(vec![("svc", "https://x.com")]);

        let state = empty_state();
        state.lock().unwrap().insert(
            "svc".to_string(),
            EndpointState {
                status: Status::Down,
                last_code: None,
                last_checked: None,
                response_ms: None,
                total_checks: 3,
                up_checks: 0,
            },
        );

        let out = render(&cfg, state, empty_log());
        assert!(out.contains("DOWN"), "DOWN endpoint should render 'DOWN'");
        assert!(out.contains("0.0%"), "should render 0% uptime");
    }

    #[test]
    fn draw_multiple_endpoints_all_rendered() {
        let cfg = make_config(vec![
            ("alpha", "https://alpha.example.com"),
            ("beta", "https://beta.example.com"),
        ]);
        let out = render(&cfg, empty_state(), empty_log());
        assert!(out.contains("alpha"), "first endpoint name should appear");
        assert!(out.contains("beta"), "second endpoint name should appear");
    }

    // ── draw — event log panel ────────────────────────────────────────────────

    #[test]
    fn draw_shows_log_entries() {
        let cfg = make_config(vec![("svc", "https://x.com")]);
        let log = empty_log();
        log.lock().unwrap().push(LogEvent {
            ts: Utc::now(),
            message: "[svc] --- → UP (HTTP 200)".to_string(),
        });

        let out = render(&cfg, empty_state(), log);
        assert!(
            out.contains("UP (HTTP 200)"),
            "log entry should appear in the events panel"
        );
    }

    #[test]
    fn draw_empty_log_shows_events_border() {
        let cfg = make_config(vec![("svc", "https://x.com")]);
        let out = render(&cfg, empty_state(), empty_log());
        assert!(out.contains("Events"), "events panel border title should be rendered");
    }
}

