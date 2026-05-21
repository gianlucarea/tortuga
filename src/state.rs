use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

/// Whether an endpoint is currently reachable.
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

/// Per-endpoint runtime + persisted state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointState {
    pub status: Status,
    /// Last HTTP status code received (None if connection failed).
    pub last_code: Option<u16>,
    pub last_checked: Option<DateTime<Utc>>,
    /// Round-trip time of the last successful check in milliseconds.
    pub response_ms: Option<u64>,
    /// Total number of checks performed.
    pub total_checks: u64,
    /// Number of checks that counted as UP.
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

    /// Uptime percentage [0.0, 100.0]. Returns None if no checks yet.
    pub fn uptime_pct(&self) -> Option<f64> {
        if self.total_checks == 0 {
            None
        } else {
            Some(self.up_checks as f64 / self.total_checks as f64 * 100.0)
        }
    }
}

/// A timestamped event pushed to the event log panel.
#[derive(Debug, Clone)]
pub struct LogEvent {
    pub ts: DateTime<Utc>,
    pub message: String,
}

pub type SharedState = Arc<Mutex<HashMap<String, EndpointState>>>;
pub type SharedLog = Arc<Mutex<Vec<LogEvent>>>;

/// Load persisted state from a JSON file.
/// Missing keys are silently ignored (new endpoints start fresh).
pub fn load_state(path: &Path) -> Result<HashMap<String, EndpointState>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let map: HashMap<String, EndpointState> = serde_json::from_str(&raw)?;
    Ok(map)
}

/// Persist current state to a JSON file.
pub fn save_state(path: &Path, state: &HashMap<String, EndpointState>) -> Result<()> {
    let raw = serde_json::to_string_pretty(state)?;
    std::fs::write(path, raw)?;
    Ok(())
}
