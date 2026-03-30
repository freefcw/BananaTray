use super::{AiProvider, ProviderError};
use crate::models::{ProviderKind, ProviderMetadata, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

super::define_unit_provider!(KiloProvider);

impl KiloProvider {
    fn extensions_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".vscode")
            .join("extensions")
    }

    fn has_kilo_extension() -> bool {
        let dir = Self::extensions_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("kilocode.kilo-code") {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[async_trait]
impl AiProvider for KiloProvider {
    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            kind: ProviderKind::Kilo,
            display_name: "Kilo".into(),
            brand_name: "Kilo".into(),
            icon_asset: "src/icons/provider-kilo.svg".into(),
            dashboard_url: "https://app.kilo.ai/usage".into(),
            account_hint: "Kilo account".into(),
            source_label: "kilo api".into(),
        }
    }

    fn id(&self) -> &'static str {
        "kilo:ext"
    }

    async fn is_available(&self) -> bool {
        Self::has_kilo_extension()
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        Err(ProviderError::Unavailable(
            "Kilo Code usage monitoring is not yet supported. Kilo Code runs as a VS Code extension without a public usage API."
                .to_string(),
        )
        .into())
    }
}
