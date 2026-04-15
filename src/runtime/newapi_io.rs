//! NewAPI Provider YAML 文件的磁盘 I/O 操作。
//!
//! 封装 YAML 生成、目录创建、文件写入等 I/O 步骤，
//! 供 `run_common_effect` 中的 `SaveNewApiProvider` handler 调用。

use crate::models::NewApiConfig;
use crate::providers::custom::generator;
use std::path::PathBuf;

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
