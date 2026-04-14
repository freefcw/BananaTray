//! NewAPI 中转站纯数据类型和 ID 计算逻辑。
//!
//! 从 `providers/custom/generator.rs` 迁入，消除 `application/` → `providers/` 的反向依赖。
//! 本模块不包含磁盘 I/O 或 YAML 模板生成，仅包含纯数据结构和纯函数。

/// NewAPI 配置输入（用户通过表单提交的数据）
#[derive(Debug, Clone)]
pub struct NewApiConfig {
    /// 显示名称，如 "我的 NewAPI 站"
    pub display_name: String,
    /// 站点 URL，如 "https://your-site.com"（不含末尾斜杠）
    pub base_url: String,
    /// 完整的 Cookie 字符串（从浏览器 DevTools 复制）
    /// 如 "session=eyJ...; cf_clearance=abc123"
    pub cookie: String,
    /// 用户 ID（部分站点需要，可选）
    pub user_id: Option<String>,
    /// 积分换算比例（默认 500000 积分 = $1 USD）
    pub divisor: Option<f64>,
}

/// 从 YAML 配置中解析出的编辑数据（GPUI-free，可用于回填表单）
#[derive(Debug, Clone)]
pub struct NewApiEditData {
    /// 显示名称
    pub display_name: String,
    /// 站点 URL（身份标识，编辑时只读）
    pub base_url: String,
    /// Cookie 字符串
    pub cookie: String,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 积分换算比例
    pub divisor: Option<f64>,
    /// 原始 YAML 文件名（编辑保存时复用，避免身份变更导致文件残留）
    pub original_filename: String,
}

/// 从 base_url 中提取域名部分，用于生成 id 和文件名
///
/// 例如：
/// - `https://my-api.example.com` → `my-api-example-com`
/// - `http://localhost:3000` → `localhost-3000`
pub fn extract_domain_slug(base_url: &str) -> String {
    let url = base_url
        .trim_end_matches('/')
        .replace("https://", "")
        .replace("http://", "");

    // 替换非字母数字字符为连字符，去除多余连字符
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// 根据 base_url 计算 NewAPI Provider 的 ID（`{slug}:newapi`）。
///
/// 用于在保存 YAML 之前提前将 ID 注册到 settings（sidebar/enabled），
/// 避免热重载后 Provider 已存在但未启用的问题。
pub fn newapi_provider_id(base_url: &str) -> String {
    let slug = extract_domain_slug(base_url);
    format!("{}:newapi", slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_domain_slug_basic() {
        assert_eq!(
            extract_domain_slug("https://my-api.example.com"),
            "my-api-example-com"
        );
        assert_eq!(
            extract_domain_slug("http://localhost:3000"),
            "localhost-3000"
        );
        assert_eq!(extract_domain_slug("https://api.site.io/"), "api-site-io");
    }

    #[test]
    fn newapi_provider_id_basic() {
        assert_eq!(
            newapi_provider_id("https://my-api.example.com"),
            "my-api-example-com:newapi"
        );
    }
}
