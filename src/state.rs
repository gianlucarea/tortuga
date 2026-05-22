use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Unknown,
    Up,
    Down,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Up => "UP",
            Status::Down => "DOWN",
            Status::Unknown => "---",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointState {
    pub status: Status,
    pub last_code: Option<u16>,
    pub last_checked: Option<DateTime<Utc>>,
    pub response_ms: Option<u64>,
    pub total_checks: u64,
    pub up_checks: u64,
}

impl EndpointState {
    pub fn new() -> Self {
        Self {
            status: Status::Unknown,
            last_code: None,
            last_checked: None,
            response_ms: None,
            total_checks: 0,
            up_checks: 0,
        }
    }

    pub fn uptime_pct(&self) -> Option<f64> {
        if self.total_checks == 0 {
            None
        } else {
            Some(self.up_checks as f64 / self.total_checks as f64 * 100.0)
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEvent {
    pub ts: DateTime<Utc>,
    pub message: String,
}

pub type SharedState = Arc<Mutex<HashMap<String, EndpointState>>>;
pub type SharedLog = Arc<Mutex<Vec<LogEvent>>>;

pub fn load_state(path: &Path) -> Result<HashMap<String, EndpointState>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let map: HashMap<String, EndpointState> = serde_json::from_str(&raw)?;
    Ok(map)
}

pub fn save_state(path: &Path, state: &HashMap<String, EndpointState>) -> Result<()> {
    let raw = serde_json::to_string_pretty(state)?;
    std::fs::write(path, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_state() -> HashMap<String, EndpointState> {
        let mut map = HashMap::new();
        map.insert(
            "api".to_string(),
            EndpointState {
                status: Status::Up,
                last_code: Some(200),
                last_checked: Some(Utc.with_ymd_and_hms(2026, 5, 21, 12, 0, 0).unwrap()),
                response_ms: Some(142),
                total_checks: 10,
                up_checks: 9,
            },
        );
        map.insert(
            "broken".to_string(),
            EndpointState {
                status: Status::Down,
                last_code: None,
                last_checked: None,
                response_ms: None,
                total_checks: 3,
                up_checks: 0,
            },
        );
        map
    }

    #[test]
    fn uptime_pct_no_checks() {
        assert_eq!(EndpointState::new().uptime_pct(), None);
    }

    #[test]
    fn uptime_pct_calculation() {
        let s = EndpointState { total_checks: 4, up_checks: 3, ..EndpointState::new() };
        assert!((s.uptime_pct().unwrap() - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn uptime_pct_full() {
        let s = EndpointState { total_checks: 5, up_checks: 5, ..EndpointState::new() };
        assert!((s.uptime_pct().unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        let original = sample_state();
        save_state(&path, &original).expect("save failed");

        let loaded = load_state(&path).expect("load failed");

        assert_eq!(loaded.len(), original.len());

        let api = loaded.get("api").unwrap();
        assert_eq!(api.status, Status::Up);
        assert_eq!(api.last_code, Some(200));
        assert_eq!(api.response_ms, Some(142));
        assert_eq!(api.total_checks, 10);
        assert_eq!(api.up_checks, 9);

        let broken = loaded.get("broken").unwrap();
        assert_eq!(broken.status, Status::Down);
        assert_eq!(broken.last_code, None);
        assert_eq!(broken.total_checks, 3);
        assert_eq!(broken.up_checks, 0);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = load_state(&path).expect("should not error");
        assert!(result.is_empty());
    }

    #[test]
    fn load_corrupt_json_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, b"not json at all {{{{").unwrap();
        assert!(load_state(&path).is_err());
    }

    #[test]
    fn status_labels() {
        assert_eq!(Status::Up.label(), "UP");
        assert_eq!(Status::Down.label(), "DOWN");
        assert_eq!(Status::Unknown.label(), "---");
    }
}
