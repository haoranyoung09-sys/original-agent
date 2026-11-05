//! Persist settings under `%USERPROFILE%\.original-agent` (or `$HOME/.original-agent`).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::provider::{ApiStyle, ProviderId};
use crate::session::EffortLevel;

pub const CONFIG_DIR_NAME: &str = ".original-agent";
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MAX_TOKENS: u32 = 8192;
pub const DEFAULT_BUDGET_TOKENS: u32 = 200_000;
pub const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pricing {
    /// USD per 1M input tokens.
    #[serde(default)]
    pub input_per_mtok: Option<f64>,
    /// USD per 1M output tokens.
    #[serde(default)]
    pub output_per_mtok: Option<f64>,
    /// USD per 1M cache-read tokens (optional).
    #[serde(default)]
    pub cache_read_per_mtok: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub provider: ProviderId,
    #[serde(default)]
    pub api_style: ApiStyle,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub effort: EffortLevel,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_budget_tokens")]
    pub budget_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default)]
    pub pricing: Pricing,
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}
fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}
fn default_max_tokens() -> u32 {
    DEFAULT_MAX_TOKENS
}
fn default_budget_tokens() -> u32 {
    DEFAULT_BUDGET_TOKENS
}
fn default_context_window() -> u32 {
    DEFAULT_CONTEXT_WINDOW
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            provider: ProviderId::Custom,
            api_style: ApiStyle::Anthropic,
            model: String::new(),
            base_url: String::new(),
            effort: EffortLevel::Medium,
            max_tokens: DEFAULT_MAX_TOKENS,
            budget_tokens: DEFAULT_BUDGET_TOKENS,
            context_window: DEFAULT_CONTEXT_WINDOW,
            pricing: Pricing::default(),
        }
    }
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("无法定位用户主目录")?;
        Ok(home.join(CONFIG_DIR_NAME))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    pub fn history_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("history"))
    }

    #[allow(dead_code)]
    pub fn bin_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("bin"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("读取配置失败: {}", path.display()))?;
        let cfg: Config = serde_json::from_str(&raw)
            .with_context(|| format!("解析配置失败: {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.max_tokens > 0 && self.budget_tokens > 0 && self.context_window > 0,
            "Token limits must be positive"
        );
        for price in [
            self.pricing.input_per_mtok,
            self.pricing.output_per_mtok,
            self.pricing.cache_read_per_mtok,
        ]
        .into_iter()
        .flatten()
        {
            anyhow::ensure!(
                price.is_finite() && price >= 0.0,
                "Prices must be finite and nonnegative"
            );
        }
        if !self.base_url.is_empty() {
            let url = reqwest::Url::parse(&self.base_url)?;
            let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
            anyhow::ensure!(
                url.scheme() == "https" || (url.scheme() == "http" && local),
                "Use HTTPS, or HTTP on localhost"
            );
            anyhow::ensure!(
                url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "URL must not contain credentials, query, or fragment"
            );
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir).with_context(|| format!("创建配置目录失败: {}", dir.display()))?;
        let path = Self::config_path()?;
        let raw = serde_json::to_string_pretty(self).context("序列化配置失败")?;
        fs::write(&path, raw).with_context(|| format!("写入配置失败: {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key
            .as_ref()
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false)
    }

    pub fn resolve_api_key_from_env() -> Option<String> {
        for key in [
            "ORIGINAL_API_KEY",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "DEEPSEEK_API_KEY",
            "CLAUDIUS_API_KEY",
        ] {
            if let Ok(v) = std::env::var(key) {
                let trimmed = v.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
        None
    }

    /// Env key wins over saved config.
    pub fn effective_api_key(&self) -> Option<String> {
        Self::resolve_api_key_from_env().or_else(|| {
            self.api_key
                .as_ref()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
        })
    }

    pub fn estimate_cost_usd(&self, stats: &crate::session::UsageStats) -> Option<f64> {
        let in_p = self.pricing.input_per_mtok?;
        let out_p = self.pricing.output_per_mtok?;
        let mut cost = (stats.input_tokens as f64 / 1_000_000.0) * in_p
            + (stats.output_tokens as f64 / 1_000_000.0) * out_p;
        if let Some(cr) = self.pricing.cache_read_per_mtok {
            cost += (stats.cache_read_tokens as f64 / 1_000_000.0) * cr;
        }
        Some(cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_limits_and_prices() {
        let mut c = Config {
            context_window: 0,
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.context_window = 100;
        c.pricing.input_per_mtok = Some(f64::NAN);
        assert!(c.validate().is_err());
    }
    #[test]
    fn validates_endpoint_transport() {
        let mut c = Config {
            base_url: "http://example.com".into(),
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.base_url = "http://127.0.0.1:8080/v1".into();
        assert!(c.validate().is_ok());
        c.base_url = "https://user:secret@example.com".into();
        assert!(c.validate().is_err());
    }
}
