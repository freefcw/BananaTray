use anyhow::Result;
use async_trait::async_trait;
use log::info;

use crate::models::{ProviderCapability, ProviderDescriptor, RefreshData};
use crate::providers::{
    AiProvider, ProviderCapabilities, ProviderExecutionContext, ProviderResult,
};

use super::plan::CompiledPlan;
use super::schema::{CustomProviderDef, SourceDef};
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

    /// 脚本 Provider 的解释器命令，取自首个 CLI 步骤（生成的 YAML 只有这一步）。
    fn script_interpreter(&self) -> String {
        match self.def.plan.steps.first().map(|step| &step.source) {
            Some(SourceDef::Cli { command, .. }) => command.clone(),
            _ => String::new(),
        }
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
            Some(CustomProviderOrigin::NewApi) => {
                crate::models::SettingsCapability::NewApiEditable {
                    base_url: self.def.base_url.clone().unwrap_or_default(),
                }
            }
            Some(CustomProviderOrigin::Script) => {
                crate::models::SettingsCapability::ScriptEditable {
                    interpreter: self.script_interpreter(),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::newapi::NewApiConfig;
    use crate::models::{ScriptProviderConfig, SettingsCapability};
    use std::path::Path;

    fn provider_from_yaml(yaml: &str) -> CustomProvider {
        let def: CustomProviderDef = serde_norway::from_str(yaml).unwrap();
        CustomProvider::new(def).unwrap()
    }

    /// 设置卡片展示的站点地址必须来自向导实际写入的 YAML，而不是另一套推导。
    #[test]
    fn newapi_capability_carries_generated_base_url() {
        let config = NewApiConfig {
            display_name: "My Relay".to_string(),
            // 末尾斜杠由生成器统一去掉，卡片显示的应是归一化后的地址
            base_url: "https://my-site.com/".to_string(),
            cookie: "session=abc".to_string(),
            user_id: None,
            divisor: None,
        };
        let provider = provider_from_yaml(&super::super::generator::generate_newapi_yaml(
            &config, None,
        ));

        assert_eq!(
            provider.settings_capability(),
            SettingsCapability::NewApiEditable {
                base_url: "https://my-site.com".to_string()
            }
        );
    }

    #[test]
    fn script_capability_carries_generated_interpreter() {
        let config = ScriptProviderConfig {
            display_name: "My Script".to_string(),
            provider_id: "my-script:script".to_string(),
            interpreter: "python3".to_string(),
            timeout_ms: 20_000,
            script: "print('{}')".to_string(),
        };
        let provider = provider_from_yaml(&super::super::generator::generate_script_provider_yaml(
            &config,
            Path::new("/tmp/script-my-script.py"),
        ));

        assert_eq!(
            provider.settings_capability(),
            SettingsCapability::ScriptEditable {
                interpreter: "python3".to_string()
            }
        );
    }

    /// 非向导来源的自定义 provider 没有编辑入口，设置区走占位卡片。
    #[test]
    fn plain_custom_provider_has_no_settings_capability() {
        let provider = provider_from_yaml(
            r#"
schema_version: 2
id: "test:cli"
metadata:
  display_name: "Test"
  brand_name: "Test"
plan:
  steps:
    - name: cli
      source:
        type: cli
        command: "echo"
      parser:
        format: regex
        quotas:
          - label: "Usage"
            pattern: '(\d+)/(\d+)'
"#,
        );

        assert_eq!(provider.settings_capability(), SettingsCapability::None);
    }
}
