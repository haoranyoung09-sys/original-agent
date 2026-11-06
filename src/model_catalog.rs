//! Built-in public list prices / context windows for known model IDs.
//! Sources are vendor docs; values are snapshots and may lag official changes.
//! Last reviewed: 2026-11-06.
//!
//! - Anthropic: https://platform.claude.com/docs/en/about-claude/pricing
//! - OpenAI: https://developers.openai.com/api/docs/models
//! - DeepSeek: https://api-docs.deepseek.com/quick_start/pricing/ (peak rates)

/// USD per 1M tokens + context window for a known model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelDefaults {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: Option<f64>,
    pub context_window: u32,
    /// Short note shown after /login (e.g. "peak rate").
    pub note: &'static str,
}

/// Exact IDs first; longer aliases help dated snapshots (claude-sonnet-4-20250514).
const CATALOG: &[(&str, ModelDefaults)] = &[
    // Anthropic Claude (list price ≤200K prompts where long-context tiers exist)
    (
        "claude-sonnet-4-20250514",
        ModelDefaults {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.30),
            context_window: 200_000,
            note: "Anthropic list ≤200K",
        },
    ),
    (
        "claude-sonnet-4",
        ModelDefaults {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.30),
            context_window: 200_000,
            note: "Anthropic list ≤200K",
        },
    ),
    (
        "claude-sonnet-4-5",
        ModelDefaults {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.30),
            context_window: 200_000,
            note: "Anthropic list",
        },
    ),
    (
        "claude-sonnet-4-6",
        ModelDefaults {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.30),
            context_window: 1_000_000,
            note: "Anthropic list",
        },
    ),
    (
        "claude-sonnet-5",
        ModelDefaults {
            input_per_mtok: 2.0,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(0.20),
            context_window: 200_000,
            note: "Anthropic list",
        },
    ),
    (
        "claude-sonnet-5-5",
        ModelDefaults {
            input_per_mtok: 2.0,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(0.10),
            context_window: 200_000,
            note: "Anthropic list",
        },
    ),
    (
        "claude-opus-4",
        ModelDefaults {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.50),
            context_window: 200_000,
            note: "Anthropic list (retired tier)",
        },
    ),
    (
        "claude-opus-4-1",
        ModelDefaults {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: Some(1.50),
            context_window: 200_000,
            note: "Anthropic list (retired tier)",
        },
    ),
    (
        "claude-opus-4-5",
        ModelDefaults {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
            cache_read_per_mtok: Some(0.50),
            context_window: 200_000,
            note: "Anthropic list",
        },
    ),
    (
        "claude-haiku-4-5",
        ModelDefaults {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cache_read_per_mtok: Some(0.10),
            context_window: 200_000,
            note: "Anthropic list",
        },
    ),
    // OpenAI
    (
        "gpt-4.1",
        ModelDefaults {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
            cache_read_per_mtok: Some(0.50),
            context_window: 1_047_576,
            note: "OpenAI list",
        },
    ),
    (
        "gpt-4.1-2025-04-14",
        ModelDefaults {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
            cache_read_per_mtok: Some(0.50),
            context_window: 1_047_576,
            note: "OpenAI list",
        },
    ),
    (
        "gpt-4.1-mini",
        ModelDefaults {
            input_per_mtok: 0.40,
            output_per_mtok: 1.60,
            cache_read_per_mtok: Some(0.10),
            context_window: 1_047_576,
            note: "OpenAI list (approx)",
        },
    ),
    (
        "gpt-4.1-nano",
        ModelDefaults {
            input_per_mtok: 0.10,
            output_per_mtok: 0.40,
            cache_read_per_mtok: Some(0.025),
            context_window: 1_047_576,
            note: "OpenAI list (approx)",
        },
    ),
    (
        "gpt-4o",
        ModelDefaults {
            input_per_mtok: 2.50,
            output_per_mtok: 10.0,
            cache_read_per_mtok: Some(1.25),
            context_window: 128_000,
            note: "OpenAI list",
        },
    ),
    (
        "gpt-4o-mini",
        ModelDefaults {
            input_per_mtok: 0.15,
            output_per_mtok: 0.60,
            cache_read_per_mtok: Some(0.075),
            context_window: 128_000,
            note: "OpenAI list",
        },
    ),
    (
        "o3",
        ModelDefaults {
            input_per_mtok: 2.0,
            output_per_mtok: 8.0,
            cache_read_per_mtok: Some(0.50),
            context_window: 200_000,
            note: "OpenAI list (approx)",
        },
    ),
    (
        "o3-mini",
        ModelDefaults {
            input_per_mtok: 1.10,
            output_per_mtok: 4.40,
            cache_read_per_mtok: Some(0.55),
            context_window: 200_000,
            note: "OpenAI list",
        },
    ),
    (
        "o4-mini",
        ModelDefaults {
            input_per_mtok: 1.10,
            output_per_mtok: 4.40,
            cache_read_per_mtok: Some(0.275),
            context_window: 200_000,
            note: "OpenAI list (approx)",
        },
    ),
    // DeepSeek — store peak rates (matches prior presets / README); off-peak is ~half
    (
        "deepseek-flash",
        ModelDefaults {
            input_per_mtok: 0.30,
            output_per_mtok: 1.20,
            cache_read_per_mtok: Some(0.006),
            context_window: 1_000_000,
            note: "DeepSeek peak cache-miss; off-peak ~half",
        },
    ),
    (
        "deepseek-v4-flash",
        ModelDefaults {
            input_per_mtok: 0.30,
            output_per_mtok: 1.20,
            cache_read_per_mtok: Some(0.006),
            context_window: 1_000_000,
            note: "alias → flash peak",
        },
    ),
    (
        "deepseek-v4-pro",
        ModelDefaults {
            input_per_mtok: 1.32,
            output_per_mtok: 3.96,
            cache_read_per_mtok: Some(0.044),
            context_window: 1_000_000,
            note: "DeepSeek peak cache-miss; off-peak ~half",
        },
    ),
    (
        "deepseek-chat",
        ModelDefaults {
            input_per_mtok: 0.30,
            output_per_mtok: 1.20,
            cache_read_per_mtok: Some(0.006),
            context_window: 1_000_000,
            note: "legacy id; billed as flash peak",
        },
    ),
    (
        "deepseek-reasoner",
        ModelDefaults {
            input_per_mtok: 0.30,
            output_per_mtok: 1.20,
            cache_read_per_mtok: Some(0.006),
            context_window: 1_000_000,
            note: "legacy id; billed as flash peak",
        },
    ),
];

/// Look up defaults by model id (case-insensitive).
/// Also matches dated Anthropic snapshots when the catalog has a shorter prefix
/// (e.g. `claude-sonnet-4-20250514` already exact; `claude-opus-4-20250514` → `claude-opus-4`).
pub fn lookup(model: &str) -> Option<&'static ModelDefaults> {
    let key = model.trim();
    if key.is_empty() {
        return None;
    }
    let lower = key.to_ascii_lowercase();
    for (name, def) in CATALOG {
        if name.eq_ignore_ascii_case(&lower) {
            return Some(def);
        }
    }
    // Longest-prefix match for dated / variant ids.
    let mut best: Option<(&str, &ModelDefaults)> = None;
    for (name, def) in CATALOG {
        let n = name.to_ascii_lowercase();
        if lower.starts_with(&n)
            && (lower.len() == n.len()
                || lower.as_bytes().get(n.len()) == Some(&b'-')
                || lower.as_bytes().get(n.len()) == Some(&b'_'))
        {
            match best {
                Some((prev, _)) if prev.len() >= n.len() => {}
                _ => best = Some((name, def)),
            }
        }
    }
    best.map(|(_, d)| d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_preset_defaults() {
        let s = lookup("claude-sonnet-4-20250514").unwrap();
        assert_eq!(s.input_per_mtok, 3.0);
        assert_eq!(s.output_per_mtok, 15.0);
        assert_eq!(s.context_window, 200_000);

        let g = lookup("GPT-4.1").unwrap();
        assert_eq!(g.input_per_mtok, 2.0);
        assert_eq!(g.context_window, 1_047_576);

        let d = lookup("deepseek-flash").unwrap();
        assert_eq!(d.input_per_mtok, 0.30);
        assert_eq!(d.context_window, 1_000_000);
    }

    #[test]
    fn prefix_match_dated_ids() {
        let o = lookup("claude-opus-4-20250514").unwrap();
        assert_eq!(o.input_per_mtok, 15.0);
        assert!(lookup("my-custom-model").is_none());
        assert!(lookup("").is_none());
    }
}
