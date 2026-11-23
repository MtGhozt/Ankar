//! Local-only agent usage stats (OpenCode SQLite, cursor-agent meta, Ankar busy time).
//! No network, no telemetry. Heavy IO runs on a background thread when the panel opens.

mod ankar;
mod cursor_agent;
mod format;
mod loader;
mod opencode;
mod panel;

pub use ankar::AnkarStatsStore;
pub use loader::StatsRange;
pub use panel::StatsPanel;

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, Default)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.reasoning)
    }

    pub fn add_assign(&mut self, other: &Tokens) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
    }
}

#[derive(Clone, Debug)]
pub struct ModelRow {
    pub provider: String,
    pub model: String,
    pub steps: u64,
    pub tokens: Tokens,
    pub cost: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    OpenCode,
    CursorAgent,
    Ankar,
}

#[derive(Clone, Debug)]
pub struct AgentRow {
    pub source: Source,
    pub name: String,
    pub busy_secs: u64,
    pub waiting_secs: u64,
    pub tasks: u64,
    pub tokens: Option<Tokens>,
}

#[derive(Clone, Debug, Default)]
pub struct DayBucket {
    /// Local calendar day `YYYY-MM-DD`.
    pub day: String,
    pub input: u64,
    pub output: u64,
    pub tasks: u64,
    pub busy_secs: u64,
}

#[derive(Clone, Debug)]
pub struct StatsSnapshot {
    pub range: StatsRange,
    pub tokens: Tokens,
    pub cost: f64,
    pub sessions: u64,
    pub subagents: u64,
    pub prompts: u64,
    pub steps: u64,
    /// Primary agent hours (Ankar busy preferred; else OpenCode durations).
    pub agent_hours: f64,
    pub opencode_hours: f64,
    pub cursor_hours: f64,
    pub ankar_busy_secs: u64,
    pub ankar_waiting_secs: u64,
    pub ankar_starts: u64,
    /// Activity heatmap: `[weekday Mon=0 .. Sun=6][hour 0..23]`.
    pub activity: [[u32; 24]; 7],
    pub models: Vec<ModelRow>,
    pub agents: Vec<AgentRow>,
    pub days: Vec<DayBucket>,
    /// Percent deltas vs previous equal-length window (None if unavailable).
    pub delta_input_pct: Option<f64>,
    pub delta_output_pct: Option<f64>,
    pub delta_tasks_pct: Option<f64>,
    pub delta_hours_pct: Option<f64>,
    pub warnings: Vec<String>,
    pub computed_at: SystemTime,
    pub range_label: String,
}

impl Default for StatsSnapshot {
    fn default() -> Self {
        Self {
            range: StatsRange::Days30,
            tokens: Tokens::default(),
            cost: 0.0,
            sessions: 0,
            subagents: 0,
            prompts: 0,
            steps: 0,
            agent_hours: 0.0,
            opencode_hours: 0.0,
            cursor_hours: 0.0,
            ankar_busy_secs: 0,
            ankar_waiting_secs: 0,
            ankar_starts: 0,
            activity: [[0; 24]; 7],
            models: Vec::new(),
            agents: Vec::new(),
            days: Vec::new(),
            delta_input_pct: None,
            delta_output_pct: None,
            delta_tasks_pct: None,
            delta_hours_pct: None,
            warnings: Vec::new(),
            computed_at: SystemTime::UNIX_EPOCH,
            range_label: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnkarHourCell {
    #[serde(default)]
    pub busy: u64,
    #[serde(default)]
    pub waiting: u64,
    #[serde(default)]
    pub starts: u64,
}
