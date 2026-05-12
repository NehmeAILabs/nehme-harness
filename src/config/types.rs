use std::collections::HashMap;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickModelConfig {
    pub provider: CompactString,
    pub model: CompactString,
    #[serde(default)]
    pub input_token_cost: f64,
    #[serde(default)]
    pub output_token_cost: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_token_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiStyle {
    Responses,
    Completions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    pub provider_type: CompactString,
    pub base_url: String,
    pub api_key_env: Option<CompactString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub danger_accept_invalid_certs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_style: Option<ApiStyle>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<CompactString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditSystem {
    #[default]
    Similarity,
    Hashedit,
}

impl std::fmt::Display for EditSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditSystem::Similarity => write!(f, "similarity"),
            EditSystem::Hashedit => write!(f, "hashedit"),
        }
    }
}

impl std::str::FromStr for EditSystem {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "similarity" => Ok(EditSystem::Similarity),
            "hashedit" => Ok(EditSystem::Hashedit),
            _ => Err(format!(
                "unknown edit system '{}' (valid: similarity, hashedit)",
                s
            )),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    pub chat_background: Option<CompactString>,
    pub input_background: Option<CompactString>,
    pub status_background: Option<CompactString>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainConfig {
    #[serde(rename = "brainstorm-to-plan")]
    pub brainstorm_to_plan: bool,
    #[serde(rename = "plan-to-code")]
    pub plan_to_code: bool,
    #[serde(rename = "code-to-review")]
    pub code_to_review: bool,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            brainstorm_to_plan: true,
            plan_to_code: true,
            code_to_review: false,
        }
    }
}

#[cfg(feature = "advisor")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvisorConfig {
    pub enabled: bool,
    pub model: Option<CompactString>,
    pub max_uses: Option<usize>,
    pub human_handoff: bool,
    pub advisor_kilobytes_limit: u32,
}

#[cfg(feature = "advisor")]
impl Default for AdvisorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: Some(CompactString::new("deepseek-v4-pro")),
            max_uses: Some(3),
            human_handoff: true,
            advisor_kilobytes_limit: 256,
        }
    }
}
