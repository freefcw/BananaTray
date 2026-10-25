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
/// 纯读取语义：旧 schema 只做内存迁移，不写回磁盘（迁移落盘只发生在
/// 启动加载路径 `loader::read_provider_def`）。
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
        let Ok(def) = super::loader::parse_provider_def(&content) else {
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

    #[test]
    fn find_custom_provider_yaml_by_id_parses_without_touching_disk() {
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

        let before = std::fs::read_to_string(&yaml_path).unwrap();
        let found = find_custom_provider_yaml_by_id("my-api:newapi", dir.path()).unwrap();
        let after = std::fs::read_to_string(&yaml_path).unwrap();

        assert_eq!(found.path, yaml_path);
        assert_eq!(found.def.id, "my-api:newapi");
        assert_eq!(before, after, "locate must not rewrite the YAML on disk");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "locate must not leave backup or temp files"
        );
    }

    /// 定位 legacy（v1 顶层键）YAML 时同样保持磁盘不变，仅在内存中升到 v2。
    #[test]
    fn find_custom_provider_yaml_by_id_keeps_legacy_yaml_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("legacy-newapi.yaml");
        std::fs::write(
            &yaml_path,
            r#"id: "legacy-router:newapi"
base_url: "https://legacy.example.com"
metadata:
  display_name: "Legacy"
  brand_name: "NewAPI Relay"
availability:
  type: always
source:
  type: http_get
  url: "/api/user/self"
  auth:
    type: cookie
    value: "session=old-token"
parser:
  format: json
  quotas:
    - label: "Balance"
      remaining: "data.quota"
"#,
        )
        .unwrap();

        let found = find_custom_provider_yaml_by_id("legacy-router:newapi", dir.path()).unwrap();

        // 内存中已升到 v2，可正常回填编辑表单
        assert_eq!(found.def.schema_version, 2);
        // 磁盘保持 legacy 原文：迁移写回只发生在启动加载路径
        assert!(std::fs::read_to_string(&yaml_path)
            .unwrap()
            .contains("availability:"));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "locate must not migrate on disk or leave .bak files"
        );
    }
}
