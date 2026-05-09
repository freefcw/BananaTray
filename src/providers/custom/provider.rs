use anyhow::Result;
use async_trait::async_trait;
use log::info;

use crate::models::{ProviderCapability, ProviderDescriptor, RefreshData};
use crate::providers::{AiProvider, ProviderResult};

use super::plan::CompiledPlan;
use super::schema::CustomProviderDef;

/// 基于 YAML 定义的自定义 Provider 运行时。
pub struct CustomProvider {
    def: CustomProviderDef,
    plan: CompiledPlan,
}

impl CustomProvider {
    pub fn new(def: CustomProviderDef) -> Result<Self> {
        let plan = CompiledPlan::compile(&def.plan)?;
        Ok(Self { def, plan })
    }

    pub fn id(&self) -> &str {
        &self.def.id
    }

    fn step_count(&self) -> usize {
        self.plan.step_count()
    }
}

#[async_trait]
impl AiProvider for CustomProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        super::descriptor::descriptor(&self.def)
    }

    async fn check_availability(&self) -> ProviderResult<()> {
        self.plan.check_availability(&self.def.plan)
    }

    async fn refresh(&self) -> ProviderResult<RefreshData> {
        let id = &self.def.id;
        info!(
            target: "providers::custom",
            "[{}] refresh started ({} step(s), mode={:?})",
            id,
            self.step_count(),
            self.def.plan.mode
        );

        self.plan
            .execute(&self.def.id, &self.def.base_url, &self.def.plan)
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
        if super::plan::is_placeholder_only(&self.def.plan) {
            ProviderCapability::Placeholder
        } else {
            ProviderCapability::Monitorable
        }
    }
}
