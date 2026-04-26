use super::codeium_family::{self, ANTIGRAVITY_SPEC};
use super::ProviderError;
use super::{AiProvider, ProviderResult};
use crate::models::RefreshData;
use anyhow::Result;
use async_trait::async_trait;
use log::warn;

super::define_unit_provider!(AntigravityProvider);

#[async_trait]
impl AiProvider for AntigravityProvider {
    fn descriptor(&self) -> crate::models::ProviderDescriptor {
        codeium_family::descriptor(&ANTIGRAVITY_SPEC)
    }

    async fn check_availability(&self) -> ProviderResult<()> {
        Ok(codeium_family::classify_unavailable(&ANTIGRAVITY_SPEC)?)
    }

    async fn refresh(&self) -> ProviderResult<RefreshData> {
        Ok(refresh_antigravity()?)
    }
}

fn refresh_antigravity() -> Result<RefreshData> {
    match codeium_family::refresh_live(&ANTIGRAVITY_SPEC) {
        Ok(data) => Ok(data),
        Err(live_err) => {
            warn!(
                target: "providers",
                "{} local API failed: {}, falling back to local cache",
                ANTIGRAVITY_SPEC.log_label,
                live_err
            );

            match codeium_family::refresh_cache(&ANTIGRAVITY_SPEC) {
                Ok(data) => Ok(data),
                Err(cache_err) => Err(ProviderError::fetch_failed(&format!(
                    "all sources failed: local API error: {}; cache error: {}",
                    live_err, cache_err
                ))
                .into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_classify_unavailable_maps_both_sources_missing() {
        let err = ProviderError::unavailable(ANTIGRAVITY_SPEC.unavailable_message);
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }

    #[test]
    fn test_matches_antigravity_process_with_app_data_dir() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc --extension_server_port 57048 --app_data_dir antigravity";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_matches_antigravity_process_with_path() {
        let line = "53319 /Applications/Antigravity.app/Contents/Resources/app/extensions/antigravity/bin/language_server_macos_arm --enable_lsp --csrf_token abc";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_is_antigravity_process_rejects_windsurf() {
        let line = "3483 /Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm --api_server_url https://server.codeium.com --run_child --enable_lsp --extension_server_port 55114 --ide_name windsurf";
        assert!(!codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }

    #[test]
    fn test_is_antigravity_process_with_dot_antigravity_path() {
        let line = "12345 /Users/test/.antigravity/bin/language_server_macos_arm --csrf_token abc";
        assert!(codeium_family::matches_process_line(
            line,
            &ANTIGRAVITY_SPEC
        ));
    }
}
