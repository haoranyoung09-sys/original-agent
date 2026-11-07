//! Runtime session: login gate, model, effort cycling, usage stats.

use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use claudius::Effort;

use crate::provider::{ApiStyle, ProviderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn cycle(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Max,
            Self::Max => Self::Low,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    /// Map UI levels to claudius Effort (Anthropic-compatible adaptive thinking).
    pub fn to_api_effort(self) -> Effort {
        match self {
            Self::Low => Effort::Low,
            Self::Medium => Effort::Medium,
            Self::High => Effort::High,
            Self::Max => Effort::Max,
        }
    }
}

impl fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for EffortLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" | "l" => Ok(Self::Low),
            "medium" | "med" | "m" => Ok(Self::Medium),
            "high" | "h" => Ok(Self::High),
            "max" => Ok(Self::Max),
            other => Err(format!("未知 effort: {other}（可用 low|medium|high|max）")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Last turn's approximate context size (input + cache tokens of that turn).
    pub last_context_tokens: u64,
    pub turns: u64,
}

impl UsageStats {
    pub fn record_turn(
        &mut self,
        input: i32,
        output: i32,
        cache_read: Option<i32>,
        cache_creation: Option<i32>,
    ) {
        let input = input.max(0) as u64;
        let output = output.max(0) as u64;
        let cr = cache_read.unwrap_or(0).max(0) as u64;
        let cc = cache_creation.unwrap_or(0).max(0) as u64;
        self.input_tokens += input;
        self.output_tokens += output;
        self.cache_read_tokens += cr;
        self.cache_creation_tokens += cc;
        self.last_context_tokens = input + cr + cc;
        self.turns += 1;
    }

    /// Cache hit rate over the session, if any cache activity was reported.
    pub fn cache_hit_rate(&self) -> Option<f64> {
        let denom = self.input_tokens + self.cache_read_tokens + self.cache_creation_tokens;
        if denom == 0 || (self.cache_read_tokens == 0 && self.cache_creation_tokens == 0) {
            return None;
        }
        Some(self.cache_read_tokens as f64 / denom as f64)
    }
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub logged_in: bool,
    pub model: String,
    pub effort: EffortLevel,
    pub base_url: String,
    pub provider: ProviderId,
    pub api_style: ApiStyle,
    pub context_window: u32,
    pub usage: UsageStats,
}

impl SessionState {
    pub fn from_config(cfg: &crate::config::Config, logged_in: bool) -> Self {
        Self {
            logged_in,
            model: cfg.model.clone(),
            effort: cfg.effort,
            base_url: cfg.base_url.clone(),
            provider: cfg.provider,
            api_style: cfg.api_style,
            context_window: cfg.context_window,
            usage: UsageStats::default(),
        }
    }

    #[allow(dead_code)]
    pub fn prompt(&self) -> String {
        "❯ ".to_string()
    }
}

pub type SharedSession = Arc<Mutex<SessionState>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn effort_parse_and_cycle() {
        assert_eq!("low".parse::<EffortLevel>().unwrap(), EffortLevel::Low);
        assert_eq!("MED".parse::<EffortLevel>().unwrap(), EffortLevel::Medium);
        assert_eq!("max".parse::<EffortLevel>().unwrap(), EffortLevel::Max);
        assert!("nope".parse::<EffortLevel>().is_err());
        assert_eq!(
            EffortLevel::Low.cycle().cycle().cycle().cycle(),
            EffortLevel::Low
        );
        assert_eq!(EffortLevel::High.to_api_effort(), claudius::Effort::High);
    }

    #[test]
    fn usage_records_and_cache_rate() {
        let mut u = UsageStats::default();
        u.record_turn(100, 20, Some(50), Some(10));
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 20);
        assert_eq!(u.cache_read_tokens, 50);
        assert_eq!(u.last_context_tokens, 160);
        assert_eq!(u.turns, 1);
        let rate = u.cache_hit_rate().unwrap();
        assert!((rate - 50.0 / 160.0).abs() < 1e-9);
    }

    #[test]
    fn session_from_config_login_flag() {
        let mut cfg = Config::default();
        cfg.model = "m".into();
        cfg.base_url = "https://example.com".into();
        let s = SessionState::from_config(&cfg, true);
        assert!(s.logged_in);
        assert_eq!(s.model, "m");
        assert_eq!(s.prompt(), "❯ ");
    }
}
