use std::borrow::Cow;

use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata};

use super::schema::CustomProviderDef;
use super::url::resolve_url;

pub(super) fn descriptor(def: &CustomProviderDef) -> ProviderDescriptor {
    let base = &def.base_url;
    let icon_asset = if def.metadata.icon.is_empty() {
        first_letter_icon(&def.metadata.display_name)
    } else {
        def.metadata.icon.clone()
    };
    ProviderDescriptor {
        id: Cow::Owned(def.id.clone()),
        metadata: ProviderMetadata {
            kind: ProviderKind::Custom,
            display_name: def.metadata.display_name.clone(),
            brand_name: def.metadata.brand_name.clone(),
            icon_asset,
            dashboard_url: resolve_url(base, &def.metadata.dashboard_url),
            account_hint: def.metadata.account_hint.clone(),
            source_label: def.metadata.source_label.clone(),
        },
    }
}

/// 从 display_name 提取首字母（大写）作为单色图标文本。
///
/// 中文取第一个汉字，英文取首字母大写。
/// 例："NewAPI" → "N"，"月之暗面" → "月"。
fn first_letter_icon(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_provider_descriptor() {
        let yaml = r#"
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  dashboard_url: "https://test.com"
  source_label: "test cli"
availability:
  type: cli_exists
  value: "echo"
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
        let desc = descriptor(&def);

        assert_eq!(desc.id.as_ref(), "test:cli");
        assert_eq!(desc.metadata.display_name, "Test Provider");
        assert_eq!(desc.metadata.brand_name, "TestBrand");
        assert_eq!(desc.metadata.kind, ProviderKind::Custom);
        assert_eq!(desc.metadata.icon_asset, "T");
    }

    #[test]
    fn test_first_letter_icon_english() {
        assert_eq!(first_letter_icon("NewAPI"), "N");
    }

    #[test]
    fn test_first_letter_icon_lowercase() {
        assert_eq!(first_letter_icon("myProvider"), "M");
    }

    #[test]
    fn test_first_letter_icon_chinese() {
        assert_eq!(first_letter_icon("月之暗面"), "月");
    }

    #[test]
    fn test_first_letter_icon_empty() {
        assert_eq!(first_letter_icon(""), "?");
    }

    #[test]
    fn test_descriptor_explicit_icon_preserved() {
        let yaml = r#"
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  icon: "X"
availability:
  type: cli_exists
  value: "echo"
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(desc.metadata.icon_asset, "X");
    }

    #[test]
    fn test_dashboard_url_env_expansion() {
        std::env::set_var("TEST_CUSTOM_BASE_URL", "https://my-newapi.com");
        let yaml = r#"
id: "test:api"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "${TEST_CUSTOM_BASE_URL}/dashboard"
availability:
  type: cli_exists
  value: "echo"
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(
            desc.metadata.dashboard_url,
            "https://my-newapi.com/dashboard"
        );
        std::env::remove_var("TEST_CUSTOM_BASE_URL");
    }

    #[test]
    fn test_descriptor_with_base_url() {
        let yaml = r#"
id: "test:api"
base_url: "https://my-site.com"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "/dashboard"
availability:
  type: always
source:
  type: cli
  command: "echo"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;
        let def: CustomProviderDef = serde_yml::from_str(yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(desc.metadata.dashboard_url, "https://my-site.com/dashboard");
    }
}
