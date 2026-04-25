use anyhow::Result;

use crate::providers::ProviderError;

use super::url::expand_tilde;

/// 读取本地 JSON 文件并解析，供可用性检查和认证读取共用。
pub(super) fn read_json_file(path: &str) -> Result<serde_json::Value> {
    let expanded = expand_tilde(path);
    let content = std::fs::read_to_string(&expanded)
        .map_err(|_| ProviderError::unavailable(&format!("file not found: {}", path)))?;
    serde_json::from_str(&content)
        .map_err(|_| ProviderError::parse_failed(&format!("invalid JSON in: {}", path)).into())
}
