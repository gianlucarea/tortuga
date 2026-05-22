use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::time::{sleep, Duration};

use crate::alert;
use crate::config::{EndpointConfig, GlobalConfig};
use crate::state::{EndpointState, LogEvent, SharedLog, SharedState, Status};

/// Determine `Up`/`Down` from an HTTP response code.
/// `None` for `expected` → any 2xx is `Up`; otherwise an exact match is required.
pub fn determine_status(code: u16, expected: Option<u16>) -> Status {
    match expected {
        Some(exp) => {
            if code == exp {
                Status::Up
            } else {
                Status::Down
            }
        }
        None => {
            if (200..300).contains(&code) {
                Status::Up
            } else {
                Status::Down
            }
        }
    }
}

/// Apply one HTTP check result to the shared state.
/// Returns `true` when the endpoint's status changed (caller should fire an alert).
pub fn apply_check_result(
    name: &str,
    new_status: Status,
    code: Option<u16>,
    response_ms: u64,
    state: &SharedState,
    log: &SharedLog,
) -> bool {
    let mut map = state.lock().unwrap();
    let entry = map.entry(name.to_string()).or_insert_with(EndpointState::new);

    let prev = entry.status.clone();
    entry.total_checks += 1;
    if new_status == Status::Up {
        entry.up_checks += 1;
    }
    entry.status = new_status.clone();
    entry.last_code = code;
    entry.last_checked = Some(Utc::now());
    entry.response_ms = Some(response_ms);

    let changed = prev != new_status;
    if changed {
        let msg = format!(
            "[{}] {} → {}{}",
            name,
            prev.label(),
            new_status.label(),
            code.map_or(String::new(), |c| format!(" (HTTP {})", c)),
        );
        let mut l = log.lock().unwrap();
        if l.len() >= 200 {
            l.remove(0);
        }
        l.push(LogEvent { ts: Utc::now(), message: msg });
    }
    changed
}

/// Execute one HTTP poll and update shared state / log.
/// Extracted from the task loop so tests can call it directly.
pub async fn poll_once(
    ep: &EndpointConfig,
    global: &GlobalConfig,
    state: &SharedState,
    log: &SharedLog,
    client: &reqwest::Client,
) {
    let start = Instant::now();
    let result = client
        .get(&ep.url)
        .timeout(Duration::from_secs(ep.timeout_secs))
        .send()
        .await;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let (new_status, code) = match result {
        Ok(resp) => {
            let code = resp.status().as_u16();
            (determine_status(code, ep.expected_status), Some(code))
        }
        Err(_) => (Status::Down, None),
    };

    let changed = apply_check_result(&ep.name, new_status.clone(), code, elapsed_ms, state, log);
    if changed {
        alert::fire_alert(ep, global, &new_status, client).await;
    }
}

/// Spawn a monitoring task for one endpoint.
/// Returns a `JoinHandle` so the caller can abort it on shutdown.
pub fn spawn_monitor(
    ep: EndpointConfig,
    global: Arc<GlobalConfig>,
    state: SharedState,
    log: SharedLog,
    client: reqwest::Client,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(ep.interval_secs)).await;
            poll_once(&ep, &global, &state, &log, &client).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn make_state() -> SharedState {
        Arc::new(Mutex::new(HashMap::new()))
    }

    fn make_log() -> SharedLog {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[test]
    fn status_exact_match_up() {
        assert_eq!(determine_status(200, Some(200)), Status::Up);
    }

    #[test]
    fn status_exact_match_wrong_code_down() {
        assert_eq!(determine_status(404, Some(200)), Status::Down);
    }

    #[test]
    fn status_exact_other_2xx_is_down() {
        assert_eq!(determine_status(201, Some(200)), Status::Down);
    }

    #[test]
    fn status_any_2xx_variants_up() {
        assert_eq!(determine_status(200, None), Status::Up);
        assert_eq!(determine_status(201, None), Status::Up);
        assert_eq!(determine_status(204, None), Status::Up);
        assert_eq!(determine_status(299, None), Status::Up);
    }

    #[test]
    fn status_non_2xx_down() {
        assert_eq!(determine_status(301, None), Status::Down);
        assert_eq!(determine_status(404, None), Status::Down);
        assert_eq!(determine_status(500, None), Status::Down);
    }

    #[test]
    fn first_check_unknown_to_up_signals_change() {
        let state = make_state();
        let log = make_log();
        let changed = apply_check_result("svc", Status::Up, Some(200), 50, &state, &log);
        assert!(changed, "Unknown→Up must signal a status change");
        let map = state.lock().unwrap();
        let e = map.get("svc").unwrap();
        assert_eq!(e.status, Status::Up);
        assert_eq!(e.last_code, Some(200));
        assert_eq!(e.total_checks, 1);
        assert_eq!(e.up_checks, 1);
    }

    #[test]
    fn same_status_repeated_no_change() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Up, Some(200), 10, &state, &log);
        let changed = apply_check_result("svc", Status::Up, Some(200), 12, &state, &log);
        assert!(!changed, "Up→Up must not signal a change");
        let map = state.lock().unwrap();
        let e = map.get("svc").unwrap();
        assert_eq!(e.total_checks, 2);
        assert_eq!(e.up_checks, 2);
    }

    #[test]
    fn status_change_pushes_log_entry() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Up, Some(200), 10, &state, &log);
        apply_check_result("svc", Status::Down, None, 5000, &state, &log);
        let l = log.lock().unwrap();
        assert_eq!(l.len(), 2);
        assert!(l[1].message.contains("DOWN"), "second entry should mention DOWN");
    }

    #[test]
    fn no_change_does_not_push_log_entry() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Up, Some(200), 10, &state, &log);
        apply_check_result("svc", Status::Up, Some(200), 12, &state, &log);
        let l = log.lock().unwrap();
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn ring_buffer_caps_at_200() {
        let state = make_state();
        let log = make_log();
        for i in 0u64..=200 {
            let status = if i % 2 == 0 { Status::Up } else { Status::Down };
            apply_check_result("svc", status, Some(200), 10, &state, &log);
        }
        let l = log.lock().unwrap();
        assert!(l.len() <= 200, "log must not exceed 200 entries");
    }

    #[test]
    fn counters_accumulate_correctly() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Up, Some(200), 10, &state, &log);
        apply_check_result("svc", Status::Down, None, 10, &state, &log);
        apply_check_result("svc", Status::Up, Some(200), 10, &state, &log);
        let map = state.lock().unwrap();
        let e = map.get("svc").unwrap();
        assert_eq!(e.total_checks, 3);
        assert_eq!(e.up_checks, 2);
    }

    #[test]
    fn down_check_does_not_increment_up_checks() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Down, None, 10, &state, &log);
        apply_check_result("svc", Status::Down, None, 10, &state, &log);
        let map = state.lock().unwrap();
        let e = map.get("svc").unwrap();
        assert_eq!(e.total_checks, 2);
        assert_eq!(e.up_checks, 0);
    }

    #[test]
    fn response_ms_updated_on_each_check() {
        let state = make_state();
        let log = make_log();
        apply_check_result("svc", Status::Up, Some(200), 42, &state, &log);
        {
            let map = state.lock().unwrap();
            assert_eq!(map.get("svc").unwrap().response_ms, Some(42));
        }
        apply_check_result("svc", Status::Up, Some(200), 99, &state, &log);
        let map = state.lock().unwrap();
        assert_eq!(map.get("svc").unwrap().response_ms, Some(99));
    }
}
