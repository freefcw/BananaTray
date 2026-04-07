use crate::models::ProviderKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeiumFamilySpec {
    pub kind: ProviderKind,
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub brand_name: &'static str,
    pub icon_asset: &'static str,
    pub dashboard_url: &'static str,
    pub account_hint: &'static str,
    pub source_label: &'static str,
    pub log_label: &'static str,
    pub ide_name: &'static str,
    pub unavailable_message: &'static str,
    pub cache_db_relative_path: &'static str,
    pub auth_status_key_candidates: &'static [&'static str],
    pub process_markers: &'static [&'static str],
    /// 当 protobuf 解码失败时，尝试从这些 key 读取 JSON 格式的 cachedPlanInfo
    pub cached_plan_info_key_candidates: &'static [&'static str],
}

pub const ANTIGRAVITY_SPEC: CodeiumFamilySpec = CodeiumFamilySpec {
    kind: ProviderKind::Antigravity,
    provider_id: "antigravity:api",
    display_name: "Antigravity",
    brand_name: "Codeium",
    icon_asset: "src/icons/provider-antigravity.svg",
    dashboard_url: "https://codeium.com/account",
    account_hint: "Codeium account",
    source_label: "local api",
    log_label: "Antigravity",
    ide_name: "antigravity",
    unavailable_message: "Antigravity live source and local cache are both unavailable",
    cache_db_relative_path:
        "Library/Application Support/Antigravity/User/globalStorage/state.vscdb",
    auth_status_key_candidates: &["antigravityAuthStatus"],
    process_markers: &[
        "--app_data_dir antigravity",
        "/antigravity/",
        ".antigravity/",
        "/antigravity.app/",
    ],
    cached_plan_info_key_candidates: &[],
};

pub const WINDSURF_SPEC: CodeiumFamilySpec = CodeiumFamilySpec {
    kind: ProviderKind::Windsurf,
    provider_id: "windsurf:api",
    display_name: "Windsurf",
    brand_name: "Codeium",
    icon_asset: "src/icons/provider-windsurf.svg",
    dashboard_url: "https://windsurf.com/",
    account_hint: "Windsurf account",
    source_label: "local api",
    log_label: "Windsurf",
    ide_name: "windsurf",
    unavailable_message: "Windsurf live source and local cache are both unavailable",
    cache_db_relative_path: "Library/Application Support/Windsurf/User/globalStorage/state.vscdb",
    auth_status_key_candidates: &["windsurfAuthStatus", "antigravityAuthStatus"],
    process_markers: &[
        "--ide_name windsurf",
        "/windsurf/",
        ".windsurf/",
        "/windsurf.app/",
    ],
    cached_plan_info_key_candidates: &["windsurf.settings.cachedPlanInfo"],
};
