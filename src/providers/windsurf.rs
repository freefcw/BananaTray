use super::codeium_family::{self, WINDSURF_SPEC};
use super::AiProvider;
use crate::models::RefreshData;
use anyhow::Result;
use async_trait::async_trait;

super::define_unit_provider!(WindsurfProvider);

#[async_trait]
impl AiProvider for WindsurfProvider {
    fn descriptor(&self) -> crate::models::ProviderDescriptor {
        codeium_family::descriptor(&WINDSURF_SPEC)
    }

    async fn check_availability(&self) -> Result<()> {
        codeium_family::classify_unavailable(&WINDSURF_SPEC)
    }

    async fn refresh(&self) -> Result<RefreshData> {
        codeium_family::refresh_with_fallback(&WINDSURF_SPEC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderError;

    #[test]
    fn test_classify_unavailable_maps_both_sources_missing() {
        let err = ProviderError::unavailable(WINDSURF_SPEC.unavailable_message);
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_matches_windsurf_process() {
        let line = "3483 /Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.codeium.com --run_child --enable_lsp --csrf_token abc --extension_server_port 55114 --ide_name windsurf";
        assert!(codeium_family::matches_process_line(line, &WINDSURF_SPEC));
    }

    #[test]
    fn test_rejects_antigravity_process() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc --extension_server_port 57048 --app_data_dir antigravity";
        assert!(!codeium_family::matches_process_line(line, &WINDSURF_SPEC));
    }
}
