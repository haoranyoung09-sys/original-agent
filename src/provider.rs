//! Provider presets: Anthropic / OpenAI compatible + common hosted defaults.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiStyle {
    #[default]
    Anthropic,
    OpenAI,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    #[default]
    Custom,
    Anthropic,
    OpenAI,
    DeepSeekAnthropic,
    DeepSeekOpenAI,
}

#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub id: ProviderId,
    pub label: &'static str,
    pub api_style: ApiStyle,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    /// Suggested list price USD per 1M tokens (input, output); None = unknown.
    pub suggest_input_per_mtok: Option<f64>,
    pub suggest_output_per_mtok: Option<f64>,
    pub suggest_context_window: u32,
}

pub fn presets() -> &'static [ProviderPreset] {
    &[
        ProviderPreset {
            id: ProviderId::Anthropic,
            label: "Anthropic (官方 Messages API)",
            api_style: ApiStyle::Anthropic,
            default_base_url: "https://api.anthropic.com",
            default_model: "claude-sonnet-4-20250514",
            suggest_input_per_mtok: Some(3.0),
            suggest_output_per_mtok: Some(15.0),
            suggest_context_window: 200_000,
        },
        ProviderPreset {
            id: ProviderId::OpenAI,
            label: "OpenAI (Chat Completions)",
            api_style: ApiStyle::OpenAI,
            default_base_url: "https://api.openai.com/v1",
            default_model: "gpt-4.1",
            suggest_input_per_mtok: Some(2.0),
            suggest_output_per_mtok: Some(8.0),
            suggest_context_window: 1_047_576,
        },
        ProviderPreset {
            id: ProviderId::DeepSeekAnthropic,
            label: "DeepSeek (Anthropic 兼容)",
            api_style: ApiStyle::Anthropic,
            default_base_url: "https://api.deepseek.com/anthropic",
            default_model: "deepseek-flash",
            suggest_input_per_mtok: Some(0.3),
            suggest_output_per_mtok: Some(1.2),
            suggest_context_window: 1_000_000,
        },
        ProviderPreset {
            id: ProviderId::DeepSeekOpenAI,
            label: "DeepSeek (OpenAI 兼容)",
            api_style: ApiStyle::OpenAI,
            default_base_url: "https://api.deepseek.com",
            default_model: "deepseek-flash",
            suggest_input_per_mtok: Some(0.3),
            suggest_output_per_mtok: Some(1.2),
            suggest_context_window: 1_000_000,
        },
        ProviderPreset {
            id: ProviderId::Custom,
            label: "自定义 Anthropic 兼容",
            api_style: ApiStyle::Anthropic,
            default_base_url: "https://api.example.com",
            default_model: "my-model",
            suggest_input_per_mtok: None,
            suggest_output_per_mtok: None,
            suggest_context_window: 128_000,
        },
        ProviderPreset {
            id: ProviderId::Custom,
            label: "自定义 OpenAI 兼容",
            api_style: ApiStyle::OpenAI,
            default_base_url: "https://api.example.com/v1",
            default_model: "my-model",
            suggest_input_per_mtok: None,
            suggest_output_per_mtok: None,
            suggest_context_window: 128_000,
        },
    ]
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::DeepSeekAnthropic => "deepseek_anthropic",
            Self::DeepSeekOpenAI => "deepseek_openai",
        }
    }
}

impl ApiStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
        }
    }
}

impl std::fmt::Display for ApiStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
