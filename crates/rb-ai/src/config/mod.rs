pub mod keyring;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

fn default_temperature() -> f32 {
    0.2
}

impl AiConfig {
    pub fn example_openai() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "openai-compat".into(),
            ProviderConfig {
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini".into(),
                temperature: 0.2,
                thinking_enabled: None,
                reasoning_effort: None,
            },
        );
        Self {
            default_provider: Some("openai-compat".into()),
            providers,
        }
    }

    pub async fn load_or_default(path: &Path) -> Result<Self, AiError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Provide a usable default provider from environment variables without
    /// persisting secrets or config. This keeps fresh dev environments usable
    /// when the user exports DEEPSEEK_API_KEY but has not opened Settings yet.
    pub fn apply_env_defaults(&mut self) {
        if self.default_provider.is_some() {
            return;
        }
        let has_deepseek_key = std::env::var("DEEPSEEK_API_KEY")
            .ok()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !has_deepseek_key {
            return;
        }
        self.providers.insert(
            "openai-compat".into(),
            ProviderConfig {
                base_url: "https://api.deepseek.com/v1".into(),
                model: "deepseek-v4-flash".into(),
                temperature: 0.2,
                thinking_enabled: Some(true),
                reasoning_effort: Some("high".into()),
            },
        );
        self.default_provider = Some("openai-compat".into());
    }

    pub async fn save(&self, path: &Path) -> Result<(), AiError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?).await?;
        fs::rename(&tmp, path).await?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("rustbrain").join("ai.json"))
            .unwrap_or_else(|| PathBuf::from("./rustbrain-ai.json"))
    }
}

impl ProviderConfig {
    pub fn is_deepseek_endpoint(&self) -> bool {
        self.base_url
            .trim()
            .to_ascii_lowercase()
            .contains("api.deepseek.com")
    }

    pub fn effective_thinking_enabled(&self) -> bool {
        self.thinking_enabled.unwrap_or_else(|| {
            self.is_deepseek_endpoint() && self.model.trim().starts_with("deepseek")
        })
    }

    pub fn effective_reasoning_effort(&self) -> Option<String> {
        if !self.effective_thinking_enabled() {
            return None;
        }
        self.reasoning_effort
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| Some("high".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn roundtrip_defaults_and_custom() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("ai.json");
        let cfg = AiConfig::example_openai();
        cfg.save(&p).await.unwrap();
        let back = AiConfig::load_or_default(&p).await.unwrap();
        assert_eq!(back.default_provider.as_deref(), Some("openai-compat"));
        assert_eq!(back.providers["openai-compat"].model, "gpt-4o-mini");
    }

    #[tokio::test]
    async fn missing_file_yields_default() {
        let tmp = tempdir().unwrap();
        let cfg = AiConfig::load_or_default(&tmp.path().join("missing.json"))
            .await
            .unwrap();
        assert!(cfg.default_provider.is_none());
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn env_defaults_configure_deepseek_when_key_exists() {
        std::env::set_var("DEEPSEEK_API_KEY", "test-key");
        let mut cfg = AiConfig::default();
        cfg.apply_env_defaults();
        std::env::remove_var("DEEPSEEK_API_KEY");

        assert_eq!(cfg.default_provider.as_deref(), Some("openai-compat"));
        let pc = &cfg.providers["openai-compat"];
        assert_eq!(pc.base_url, "https://api.deepseek.com/v1");
        assert_eq!(pc.model, "deepseek-v4-flash");
        assert_eq!(pc.thinking_enabled, Some(true));
    }

    #[tokio::test]
    async fn save_creates_parent_dirs() {
        let tmp = tempdir().unwrap();
        let nested = tmp.path().join("a").join("b").join("ai.json");
        let cfg = AiConfig::example_openai();
        cfg.save(&nested).await.unwrap();
        assert!(nested.exists());
    }
}
