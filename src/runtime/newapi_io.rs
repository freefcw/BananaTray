//! NewAPI Provider YAML 文件的磁盘 I/O 操作。
//!
//! 封装 YAML 生成、目录创建、文件写入 / 删除等 I/O 步骤，
//! 供 `run_common_effect` 中的 NewAPI effect handlers 调用。

use crate::models::{NewApiConfig, ProviderId};
use crate::providers::custom::generator;
use std::path::{Path, PathBuf};

/// 将 NewAPI 配置写入磁盘 YAML 文件。
///
/// 步骤：
/// 1. 生成 YAML 内容（`generator::generate_newapi_yaml`）
/// 2. 计算文件路径（`custom_provider_path`）
/// 3. 确保目录存在（`create_dir_all`）
/// 4. 写入文件（`fs::write`）
///
/// 成功返回文件路径，失败返回错误描述。
pub fn save_newapi_yaml(config: &NewApiConfig, filename: &str) -> Result<PathBuf, String> {
    let yaml_content = generator::generate_newapi_yaml(config);
    let path = crate::platform::paths::custom_provider_path(filename);

    let providers_dir = path
        .parent()
        .ok_or_else(|| format!("failed to resolve providers dir for {}", filename))?;

    std::fs::create_dir_all(providers_dir)
        .map_err(|e| format!("failed to create providers dir: {}", e))?;

    std::fs::write(&path, &yaml_content)
        .map_err(|e| format!("failed to write YAML to {}: {}", path.display(), e))?;

    Ok(path)
}

/// 删除 NewAPI 配置对应的 YAML 文件。
///
/// 步骤：
/// 1. 校验 provider id 是 NewAPI custom provider
/// 2. 计算文件路径
/// 3. 删除 YAML 文件
///
/// 成功返回被删除的文件路径，失败返回错误描述。
pub fn delete_newapi_yaml(provider_id: &ProviderId) -> Result<PathBuf, String> {
    let custom_id = match provider_id {
        ProviderId::Custom(custom_id) => custom_id,
        _ => {
            return Err(format!(
                "DeleteNewApiProvider: not a custom provider id: {provider_id}"
            ))
        }
    };

    let filename = generator::filename_for_id(custom_id)
        .ok_or_else(|| format!("DeleteNewApiProvider: not a newapi provider id: {custom_id}"))?;
    let path = crate::platform::paths::custom_provider_path(&filename);

    delete_yaml_file(&path)
}

fn delete_yaml_file(path: &Path) -> Result<PathBuf, String> {
    std::fs::remove_file(path)
        .map(|()| path.to_path_buf())
        .map_err(|e| format!("failed to delete YAML {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderKind;

    #[test]
    fn delete_yaml_file_removes_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay.yaml");
        std::fs::write(&path, "id: test:newapi\n").unwrap();

        let deleted_path = delete_yaml_file(&path).unwrap();

        assert_eq!(deleted_path, path);
        assert!(!path.exists());
    }

    #[test]
    fn delete_newapi_yaml_rejects_builtin_provider() {
        let err = delete_newapi_yaml(&ProviderId::BuiltIn(ProviderKind::Claude)).unwrap_err();
        assert!(err.contains("not a custom provider id"));
    }

    #[test]
    fn delete_newapi_yaml_rejects_non_newapi_custom_provider() {
        let err = delete_newapi_yaml(&ProviderId::Custom("custom:cli".to_string())).unwrap_err();
        assert!(err.contains("not a newapi provider id"));
    }
}
