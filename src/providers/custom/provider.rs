use anyhow::Result;
use async_trait::async_trait;
use log::info;

use crate::models::{ProviderCapability, ProviderDescriptor, RefreshData};
use crate::providers::{
    AiProvider, ProviderCapabilities, ProviderExecutionContext, ProviderResult,
};

use super::plan::CompiledPlan;
use super::schema::CustomProviderDef;
use super::CustomProviderOrigin;

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

    async fn check_availability(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<()> {
        self.plan.check_availability()
    }

    async fn refresh(&self, _ctx: &ProviderExecutionContext<'_>) -> ProviderResult<RefreshData> {
        let id = &self.def.id;
        info!(
            target: "providers::custom",
            "[{}] refresh started ({} step(s), mode={:?})",
            id,
            self.step_count(),
            self.plan.mode()
        );

        self.plan.execute(&self.def.id, &self.def.base_url)
    }
}

impl ProviderCapabilities for CustomProvider {
    fn settings_capability(&self) -> crate::models::SettingsCapability {
        match CustomProviderOrigin::from_id(&self.def.id) {
            Some(CustomProviderOrigin::NewApi) => crate::models::SettingsCapability::NewApiEditable,
            Some(CustomProviderOrigin::Script) => crate::models::SettingsCapability::ScriptEditable,
            None => crate::models::SettingsCapability::None,
        }
    }

    fn provider_capability(&self) -> ProviderCapability {
        if self.plan.is_placeholder_only() {
            ProviderCapability::Placeholder
        } else {
            ProviderCapability::Monitorable
        }
    }
}
