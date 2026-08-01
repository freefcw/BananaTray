//! NewAPI custom provider lifecycle operations.
//!
//! 本模块只持有 NewAPI provider 的身份、文件名、编辑态加载、保存和删除语义。
//! 低层文件替换与回滚由 `file_ops.rs` 负责。

use crate::models::newapi::{newapi_provider_id, NewApiConfig, NewApiEditData};
use crate::models::ProviderId;
use crate::providers::custom::file_ops;
use crate::providers::custom::generator;
use crate::providers::custom::lifecycle_error::{
    CustomProviderLifecycleError, CustomProviderLifecycleResult,
};
use crate::providers::custom::locator::find_custom_provider_yaml_by_id;
use std::path::{Path, PathBuf};

pub(crate) fn generate_filename(config: &NewApiConfig) -> String {
    // 与 filename_for_id 保持一致：文件名由身份 ID 推导（含 user_id 维度）
    let id = newapi_provider_id(&config.base_url, config.user_id.as_deref());
    let stem = id.strip_suffix(":newapi").unwrap_or(id.as_str());
    format!("newapi-{stem}.yaml")
}

pub(crate) fn read_config(provider_custom_id: &str) -> Option<NewApiEditData> {
    read_config_in_dir(
        provider_custom_id,
        &crate::platform::paths::custom_providers_dir(),
    )
}

/// 将 NewAPI 配置写入磁盘 YAML 文件。
///
/// `id_override` 用于编辑保存时保持 Provider 身份（原始 YAML 的 `id`）不变；
/// 新增时传 `None`，按 base_url + user_id 计算身份。
pub(crate) fn save_yaml(
    config: &NewApiConfig,
    filename: &str,
    id_override: Option<&str>,
) -> CustomProviderLifecycleResult<PathBuf> {
    let yaml_content = generator::generate_newapi_yaml(config, id_override);
    let path = crate::platform::paths::custom_provider_path(filename);

    file_ops::write_newapi_yaml(&path, &yaml_content)
        .map_err(|err| CustomProviderLifecycleError::file_operation("save NewAPI provider", err))
}

/// 删除 NewAPI 配置对应的 YAML 文件。
pub(crate) fn delete_yaml(provider_id: &ProviderId) -> CustomProviderLifecycleResult<PathBuf> {
    let custom_id = match provider_id {
        ProviderId::Custom(custom_id) => custom_id,
        _ => {
            return Err(CustomProviderLifecycleError::invalid_provider_id(
                "delete NewAPI provider",
                "custom",
                provider_id.to_string(),
            ))
        }
    };

    let yaml_path = find_yaml_path(custom_id)?;
    file_ops::delete_yaml_file(&yaml_path)
        .map_err(|err| CustomProviderLifecycleError::file_operation("delete NewAPI provider", err))
}

fn filename_for_id(custom_id: &str) -> Option<String> {
    let slug = custom_id.strip_suffix(":newapi")?;
    Some(format!("newapi-{}.yaml", slug))
}

fn read_config_in_dir(provider_custom_id: &str, providers_dir: &Path) -> Option<NewApiEditData> {
    let yaml = find_custom_provider_yaml_by_id(provider_custom_id, providers_dir)?;
    generator::parse_newapi_edit_data(&yaml.def, yaml.filename)
}

fn find_yaml_path(custom_id: &str) -> CustomProviderLifecycleResult<PathBuf> {
    if !custom_id.ends_with(":newapi") {
        return Err(CustomProviderLifecycleError::invalid_provider_id(
            "delete NewAPI provider",
            "newapi",
            custom_id,
        ));
    }

    let providers_dir = crate::platform::paths::custom_providers_dir();
    if let Ok(path) = find_yaml_path_in_dir(custom_id, &providers_dir) {
        return Ok(path);
    }

    let fallback_yaml = filename_for_id(custom_id)
        .map(|filename| crate::platform::paths::custom_provider_path(&filename));
    Err(CustomProviderLifecycleError::yaml_not_found(
        "delete NewAPI provider",
        custom_id,
        fallback_yaml,
    ))
}

fn find_yaml_path_in_dir(
    custom_id: &str,
    providers_dir: &Path,
) -> CustomProviderLifecycleResult<PathBuf> {
    find_custom_provider_yaml_by_id(custom_id, providers_dir)
        .map(|yaml| yaml.path)
        .ok_or_else(|| {
            CustomProviderLifecycleError::yaml_not_found("delete NewAPI provider", custom_id, None)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewApiConfig, ProviderKind};

    fn make_newapi_config() -> NewApiConfig {
        NewApiConfig {
            display_name: "Example".to_string(),
            base_url: "https://example.com".to_string(),
            cookie: "session=abc".to_string(),
            user_id: Some("42".to_string()),
            divisor: Some(500000.0),
        }
    }

    #[test]
    fn delete_yaml_rejects_builtin_provider() {
        let err = delete_yaml(&ProviderId::BuiltIn(ProviderKind::Claude)).unwrap_err();
        assert!(err.to_string().contains("expected custom provider id"));
    }

    #[test]
    fn delete_yaml_rejects_non_newapi_custom_provider() {
        let err = delete_yaml(&ProviderId::Custom("custom:cli".to_string())).unwrap_err();
        assert!(err.to_string().contains("expected newapi provider id"));
    }

    #[test]
    fn find_yaml_path_matches_yaml_id_not_filename() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        std::fs::create_dir_all(&providers_dir).unwrap();
        let yaml_path = providers_dir.join("renamed-provider.yaml");
        std::fs::write(
            &yaml_path,
            r#"id: "my-api:newapi"
schema_version: 2
base_url: "https://example.com"
metadata:
  display_name: "Example"
  brand_name: "NewAPI Relay"
plan:
  steps:
    - name: "api"
      source:
        type: http
        method: get
        url: "/api/user/self"
      parser:
        format: json
        quotas:
          - label: "Balance"
            remaining: "data.quota"
"#,
        )
        .unwrap();

        let found = find_yaml_path_in_dir("my-api:newapi", &providers_dir).unwrap();
        assert_eq!(found, yaml_path);
    }

    #[test]
    fn generate_filename_includes_user_id_dimension() {
        let filename = generate_filename(&make_newapi_config());
        assert_eq!(filename, "newapi-example-com-42.yaml");

        let no_user = NewApiConfig {
            user_id: None,
            ..make_newapi_config()
        };
        assert_eq!(generate_filename(&no_user), "newapi-example-com.yaml");
    }

    #[test]
    fn read_config_in_dir_uses_yaml_id_and_actual_filename() {
        let dir = tempfile::tempdir().unwrap();
        let providers_dir = dir.path().join("providers");
        std::fs::create_dir_all(&providers_dir).unwrap();
        let config = make_newapi_config();
        let yaml = generator::generate_newapi_yaml(&config, None);
        let yaml_path = providers_dir.join("renamed-provider.yaml");
        std::fs::write(&yaml_path, yaml).unwrap();

        let edit = read_config_in_dir("example-com-42:newapi", &providers_dir).unwrap();

        assert_eq!(edit.display_name, "Example");
        assert_eq!(edit.base_url, "https://example.com");
        assert_eq!(edit.cookie, "session=abc");
        assert_eq!(edit.user_id.as_deref(), Some("42"));
        assert_eq!(edit.original_filename, "renamed-provider.yaml");
        // 身份来自 YAML 的 id 字段，编辑保存时保持不变
        assert_eq!(edit.original_id, "example-com-42:newapi");
    }
}
