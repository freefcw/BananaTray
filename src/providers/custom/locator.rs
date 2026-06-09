use super::schema::CustomProviderDef;
use std::path::{Path, PathBuf};

/// 自定义 provider YAML 定位结果。
#[derive(Debug)]
pub struct CustomProviderYaml {
    pub path: PathBuf,
    pub filename: String,
    pub def: CustomProviderDef,
}

/// 遍历 providers 目录，按 YAML 内的 `id` 定位自定义 provider。
///
/// 文件名不是身份的一部分；真正的身份来自 YAML `id` 字段。
#[cfg(any(feature = "app", test))]
pub fn find_custom_provider_yaml_by_id(
    provider_custom_id: &str,
    providers_dir: &Path,
) -> Option<CustomProviderYaml> {
    let entries = std::fs::read_dir(providers_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_yaml_path(&path) {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(def) = serde_norway::from_str::<CustomProviderDef>(&content) else {
            continue;
        };
        if def.id != provider_custom_id {
            continue;
        }

        let filename = path.file_name()?.to_str()?.to_string();
        return Some(CustomProviderYaml {
            path,
            filename,
            def,
        });
    }

    None
}

pub fn is_yaml_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext == "yaml" || ext == "yml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_custom_provider_yaml_by_id_matches_yaml_id_not_filename() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("renamed-provider.yaml");
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

        let found = find_custom_provider_yaml_by_id("my-api:newapi", dir.path()).unwrap();
        assert_eq!(found.path, yaml_path);
        assert_eq!(found.filename, "renamed-provider.yaml");
        assert_eq!(found.def.id, "my-api:newapi");
    }

    #[test]
    fn find_custom_provider_yaml_by_id_skips_invalid_yaml_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.yaml"), "not: [valid").unwrap();
        let yaml_path = dir.path().join("good.yaml");
        std::fs::write(
            &yaml_path,
            r#"id: "script:test"
schema_version: 2
metadata:
  display_name: "Script"
  brand_name: "Custom Script"
plan:
  steps:
    - name: "script"
      source:
        type: cli
        command: "python3"
        args:
          - "/tmp/test.py"
      parser:
        format: json
        quotas:
          - label: "Balance"
            remaining: "remaining"
"#,
        )
        .unwrap();

        let found = find_custom_provider_yaml_by_id("script:test", dir.path()).unwrap();
        assert_eq!(found.path, yaml_path);
    }
}
