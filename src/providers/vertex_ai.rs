use super::{AiProvider, ProviderError};
use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData};
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::path::PathBuf;

super::define_unit_provider!(VertexAiProvider);

const GEMINI_SETTINGS_RELATIVE_PATH: &str = "~/.gemini/settings.json:selectedType=vertex-ai";

impl VertexAiProvider {
    fn settings_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".gemini")
            .join("settings.json")
    }

    fn uses_vertex_ai_auth(content: &str) -> bool {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(content) else {
            return false;
        };

        json.get("security")
            .and_then(|s| s.get("auth"))
            .and_then(|a| a.get("selectedType"))
            .and_then(|t| t.as_str())
            == Some("vertex-ai")
    }

    fn is_vertex_ai_configured() -> bool {
        let path = Self::settings_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            return Self::uses_vertex_ai_auth(&content);
        }
        false
    }
}

#[async_trait]
impl AiProvider for VertexAiProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("vertexai:gcloud"),
            metadata: ProviderMetadata {
                kind: ProviderKind::VertexAi,
                display_name: "Vertex AI".into(),
                brand_name: "Google Cloud".into(),
                icon_asset: "src/icons/provider-vertexai.svg".into(),
                dashboard_url: "https://console.cloud.google.com/vertex-ai".into(),
                account_hint: "Google Cloud account".into(),
                source_label: "vertex ai api".into(),
            },
        }
    }

    async fn check_availability(&self) -> Result<()> {
        if Self::is_vertex_ai_configured() {
            Ok(())
        } else {
            Err(ProviderError::config_missing(GEMINI_SETTINGS_RELATIVE_PATH).into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        Err(ProviderError::unavailable(
            "Vertex AI shares quota with Gemini CLI, please enable Vertex AI auth in Gemini CLI settings",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uses_vertex_ai_auth() {
        let json = r#"{
            "security": {
                "auth": {
                    "selectedType": "vertex-ai"
                }
            }
        }"#;

        assert!(VertexAiProvider::uses_vertex_ai_auth(json));
    }

    #[test]
    fn test_rejects_non_vertex_ai_auth() {
        let json = r#"{
            "security": {
                "auth": {
                    "selectedType": "oauth"
                }
            }
        }"#;

        assert!(!VertexAiProvider::uses_vertex_ai_auth(json));
        assert!(!VertexAiProvider::uses_vertex_ai_auth("not-json"));
    }
}
