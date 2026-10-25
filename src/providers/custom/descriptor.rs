use std::borrow::Cow;

use crate::models::{ProviderDescriptor, ProviderKind, ProviderMetadata};

use super::schema::CustomProviderDef;
use super::url::resolve_url;

pub(super) fn descriptor(def: &CustomProviderDef) -> ProviderDescriptor {
    let base = &def.base_url;
    let icon_asset = if def.metadata.icon.is_empty() {
        first_letter_icon(&def.metadata.display_name)
    } else {
        resolve_icon_asset(&def.metadata.icon)
            .unwrap_or_else(|| first_letter_icon(&def.metadata.display_name))
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

/// 解析 metadata.icon 的 SVG 路径语义；非 .svg 值原样保留（字母/emoji 文本模式）。
///
/// - `src/…​.svg`：打包资源路径，原样保留
/// - `/abs/…​.svg`：绝对路径，原样保留
/// - `~/…​.svg`：展开 HOME
/// - 其他相对 `.svg`：相对 custom providers 目录解析为绝对路径
///
/// AssetSource 用 `fs::read(base.join(path))` 加载——绝对路径下 join 直接替换 base，
/// 所以解析成绝对路径后即可穿过资源层读到任意本地 SVG。
/// 返回 `None` 表示路径语义可识别但无法解析（如 `~` 展开缺少 HOME、裸 `~x.svg`），
/// 调用方应回退到首字母文本，避免留下无法加载的空路径。
fn resolve_icon_asset(icon: &str) -> Option<String> {
    if !icon.ends_with(".svg") || icon.starts_with("src/") || icon.starts_with('/') {
        return Some(icon.to_string());
    }
    if icon.starts_with('~') {
        return icon
            .strip_prefix("~/")
            .zip(std::env::var("HOME").ok())
            .map(|(rest, home)| format!("{home}/{rest}"));
    }
    Some(
        crate::platform::paths::custom_providers_dir()
            .join(icon)
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::env_lock;

    fn minimal_plan_yaml() -> &'static str {
        r#"
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
"#
    }

    #[test]
    fn test_custom_provider_descriptor() {
        let yaml = format!(
            r#"
schema_version: 2
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  dashboard_url: "https://test.com"
  source_label: "test cli"
{}
"#,
            minimal_plan_yaml()
        );
        let def: CustomProviderDef = serde_norway::from_str(&yaml).unwrap();
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
        let yaml = format!(
            r#"
schema_version: 2
id: "test:cli"
metadata:
  display_name: "Test Provider"
  brand_name: "TestBrand"
  icon: "X"
{}
"#,
            minimal_plan_yaml()
        );
        let def: CustomProviderDef = serde_norway::from_str(&yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(desc.metadata.icon_asset, "X");
    }

    #[test]
    fn test_resolve_icon_asset_passthrough() {
        assert_eq!(
            resolve_icon_asset("src/icons/provider-custom.svg"),
            Some("src/icons/provider-custom.svg".to_string())
        );
        assert_eq!(
            resolve_icon_asset("/abs/icon.svg"),
            Some("/abs/icon.svg".to_string())
        );
        assert_eq!(resolve_icon_asset("M"), Some("M".to_string()));
        assert_eq!(resolve_icon_asset("月"), Some("月".to_string()));
    }

    #[test]
    fn test_resolve_icon_asset_tilde() {
        let _guard = env_lock().lock().unwrap();
        // SAFETY: 通过共享 env_lock 串行化进程级环境变量修改；断言前恢复原值，避免污染后续测试。
        let original = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", "/fake-home") };
        let resolved = resolve_icon_asset("~/icons/mine.svg");
        unsafe {
            match &original {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }
        assert_eq!(resolved, Some("/fake-home/icons/mine.svg".to_string()));
    }

    #[test]
    fn test_resolve_icon_asset_bare_tilde_falls_back() {
        // "~icon.svg" 不是合法的 ~ 展开形式，应返回 None 由调用方回退首字母
        assert_eq!(resolve_icon_asset("~icon.svg"), None);
    }

    #[test]
    fn test_resolve_icon_asset_relative_to_providers_dir() {
        let resolved = resolve_icon_asset("ciii.svg").unwrap();
        assert!(
            resolved.ends_with("/providers/ciii.svg"),
            "相对路径应落到 providers 目录，得到: {resolved}"
        );
    }

    #[test]
    fn test_dashboard_url_env_expansion() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::set_var("TEST_CUSTOM_BASE_URL", "https://my-newapi.com") };
        let yaml = format!(
            r#"
schema_version: 2
id: "test:api"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "${{TEST_CUSTOM_BASE_URL}}/dashboard"
{}
"#,
            minimal_plan_yaml()
        );
        let def: CustomProviderDef = serde_norway::from_str(&yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(
            desc.metadata.dashboard_url,
            "https://my-newapi.com/dashboard"
        );
        unsafe { std::env::remove_var("TEST_CUSTOM_BASE_URL") };
    }

    #[test]
    fn test_descriptor_with_base_url() {
        let yaml = format!(
            r#"
schema_version: 2
id: "test:api"
base_url: "https://my-site.com"
metadata:
  display_name: "Test"
  brand_name: "Test"
  dashboard_url: "/dashboard"
{}
"#,
            minimal_plan_yaml()
        );
        let def: CustomProviderDef = serde_norway::from_str(&yaml).unwrap();
        let desc = descriptor(&def);
        assert_eq!(desc.metadata.dashboard_url, "https://my-site.com/dashboard");
    }
}
