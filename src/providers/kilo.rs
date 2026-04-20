use super::{AiProvider, ProviderError};
use crate::models::{
    ProviderCapability, ProviderDescriptor, ProviderKind, ProviderMetadata, RefreshData,
};
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use std::path::PathBuf;

super::define_unit_provider!(KiloProvider);

const KILO_EXTENSION_PREFIX: &str = "kilocode.kilo-code";

impl KiloProvider {
    fn extensions_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".vscode")
            .join("extensions")
    }

    fn is_kilo_extension_name(name: &str) -> bool {
        name.starts_with(KILO_EXTENSION_PREFIX)
    }

    fn has_kilo_extension() -> bool {
        let dir = Self::extensions_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if Self::is_kilo_extension_name(name) {
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
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: Cow::Borrowed("kilo:ext"),
            metadata: ProviderMetadata {
                kind: ProviderKind::Kilo,
                display_name: "Kilo".into(),
                brand_name: "Kilo".into(),
                icon_asset: "src/icons/provider-kilo.svg".into(),
                dashboard_url: "https://app.kilo.ai/usage".into(),
                account_hint: "Kilo account".into(),
                source_label: "kilo api".into(),
            },
        }
    }

    fn provider_capability(&self) -> ProviderCapability {
        ProviderCapability::Placeholder
    }

    async fn check_availability(&self) -> Result<()> {
        if Self::has_kilo_extension() {
            Ok(())
        } else {
            Err(ProviderError::unavailable("Kilo extension not detected").into())
        }
    }

    async fn refresh(&self) -> Result<RefreshData> {
        Err(ProviderError::unavailable(
            "Kilo Code does not support usage monitoring, it runs as a VS Code extension with no public API",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_kilo_extension_name() {
        assert!(KiloProvider::is_kilo_extension_name(
            "kilocode.kilo-code-1.2.3"
        ));
        assert!(!KiloProvider::is_kilo_extension_name("other.publisher"));
    }
}
