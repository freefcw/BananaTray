use super::AiProvider;
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::path::PathBuf;

pub struct VertexAiProvider {}

impl VertexAiProvider {
    pub fn new() -> Self {
        Self {}
    }

    fn settings_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".gemini")
            .join("settings.json")
    }

    fn is_vertex_ai_configured() -> bool {
        let path = Self::settings_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                return json
                    .get("security")
                    .and_then(|s| s.get("auth"))
                    .and_then(|a| a.get("selectedType"))
                    .and_then(|t| t.as_str())
                    == Some("vertex-ai");
            }
        }
        false
    }
}

#[async_trait]
impl AiProvider for VertexAiProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::VertexAi,
            display_name: "Vertex AI",
            brand_name: "Google Cloud",
            icon_asset: "src/icons/provider-vertexai.svg",
            dashboard_url: "https://console.cloud.google.com/vertex-ai",
            account_hint: "Google Cloud account",
            source_label: "vertex ai api",
        }
    }

    fn id(&self) -> &'static str {
        "vertexai:gcloud"
    }

    async fn is_available(&self) -> bool {
        Self::is_vertex_ai_configured()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        bail!("Vertex AI quota monitoring uses the same quota system as Gemini CLI. Enable the Gemini provider with Vertex AI auth mode in your Gemini CLI settings.")
    }
}
