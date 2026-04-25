use anyhow::Result;
use async_trait::async_trait;
use log::{debug, info, warn};

use crate::models::{ProviderCapability, ProviderDescriptor, RefreshData};
use crate::providers::{AiProvider, ProviderError};

use super::extractor::{self, CompiledPatterns};
use super::schema::{CustomProviderDef, SourceDef};

/// 基于 YAML 定义的自定义 Provider 运行时。
pub struct CustomProvider {
    def: CustomProviderDef,
    /// 预编译的正则缓存（对 JSON parser 为空）。
    compiled: CompiledPatterns,
}

impl CustomProvider {
    pub fn new(def: CustomProviderDef) -> Result<Self> {
        let compiled = CompiledPatterns::compile(&def.parser)?;
        Ok(Self { def, compiled })
    }

    pub fn id(&self) -> &str {
        &self.def.id
    }
}

#[async_trait]
impl AiProvider for CustomProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        super::descriptor::descriptor(&self.def)
    }

    async fn check_availability(&self) -> Result<()> {
        super::availability::check(&self.def.availability)
    }

    async fn refresh(&self) -> Result<RefreshData> {
        let id = &self.def.id;
        info!(target: "providers::custom", "[{}] refresh started", id);

        let raw = super::fetch::fetch(id, &self.def.base_url, &self.def.source)?;
        debug!(target: "providers::custom", "[{}] raw response ({} bytes): {}", id, raw.len(), super::log_utils::truncate_for_log(&raw, 500));

        let raw = super::fetch::apply_preprocess(&raw, &self.def.preprocess);
        let parser = self.def.parser.as_ref().ok_or_else(|| {
            warn!(target: "providers::custom", "[{}] no parser configured", id);
            ProviderError::unavailable("no parser configured (placeholder provider)")
        })?;

        let result = extractor::extract(parser, &raw, &self.compiled);
        match &result {
            Ok(data) => info!(
                target: "providers::custom",
                "[{}] parsed {} quotas, email={:?}",
                id, data.quotas.len(), data.account_email
            ),
            Err(e) => warn!(
                target: "providers::custom",
                "[{}] parse failed: {}\n  raw response: {}",
                id, e, super::log_utils::truncate_for_log(&raw, 300)
            ),
        }
        result
    }

    fn settings_capability(&self) -> crate::models::SettingsCapability {
        // NewAPI 类型的自定义 Provider（ID 以 ":newapi" 结尾）可编辑配置
        if self.def.id.ends_with(":newapi") {
            crate::models::SettingsCapability::NewApiEditable
        } else {
            crate::models::SettingsCapability::None
        }
    }

    fn provider_capability(&self) -> ProviderCapability {
        match self.def.source {
            SourceDef::Placeholder { .. } => ProviderCapability::Placeholder,
            _ => ProviderCapability::Monitorable,
        }
    }
}
