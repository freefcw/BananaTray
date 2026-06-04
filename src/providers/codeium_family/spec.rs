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
    pub cache_db_config_relative_path: &'static str,
    /// 当主路径 (`cache_db_config_relative_path`) 不存在时，按序尝试的备选 DB 路径。
    /// 典型场景：品牌重命名后 data dir 迁移（Windsurf → Devin），旧路径仍需兜底。
    pub cache_db_fallback_paths: &'static [&'static str],
    pub auth_status_key_candidates: &'static [&'static str],
    pub process_markers: &'static [&'static str],
    /// 当 protobuf 解码失败时，尝试从这些 key 读取 JSON 格式的 cachedPlanInfo
    pub cached_plan_info_key_candidates: &'static [&'static str],
    /// 缓存 SQLite 文件 mtime 超过该秒数即视为陈旧不可信，read_refresh_data 直接返回 unavailable。
    /// 0 表示不做 mtime 校验（兼容测试用）。
    pub cache_max_age_secs: u64,
}

pub const ANTIGRAVITY_SPEC: CodeiumFamilySpec = CodeiumFamilySpec {
    kind: ProviderKind::Antigravity,
    provider_id: "antigravity:api",
    display_name: "Antigravity",
    brand_name: "Codeium",
    icon_asset: "src/icons/provider-antigravity.svg",
    dashboard_url: "",
    account_hint: "Codeium account",
    source_label: "local api",
    log_label: "Antigravity",
    ide_name: "antigravity",
    unavailable_message: "Antigravity live source and local cache are both unavailable",
    cache_db_config_relative_path: "Antigravity/User/globalStorage/state.vscdb",
    cache_db_fallback_paths: &[],
    auth_status_key_candidates: &["antigravityAuthStatus"],
    process_markers: &[
        "--app_data_dir antigravity",
        "/antigravity/",
        ".antigravity/",
        "/antigravity.app/",
    ],
    cached_plan_info_key_candidates: &[],
    // 3 小时：language server 长期未运行 → 缓存 quota 快照不再可信
    cache_max_age_secs: 3 * 60 * 60,
};

/// Devin Desktop（原 Windsurf）的 provider spec。
///
/// 2026-06 品牌重命名后：app bundle、data dir、CLI 已改为 Devin，
/// 但内部协议（`--ide_name windsurf`、DB auth key、seat API endpoint）仍用 windsurf。
/// `id_key` 保持 `"windsurf"` 以兼容已有用户设置。
pub const WINDSURF_SPEC: CodeiumFamilySpec = CodeiumFamilySpec {
    kind: ProviderKind::Windsurf,
    provider_id: "windsurf:api",
    display_name: "Devin Desktop",
    brand_name: "Cognition",
    icon_asset: "src/icons/provider-devin-desktop.svg",
    dashboard_url: "https://app.devin.ai",
    account_hint: "Devin account",
    source_label: "local/cloud fallback",
    log_label: "Devin Desktop",
    ide_name: "windsurf", // 进程仍用 --ide_name windsurf，不可改
    unavailable_message: "Devin Desktop live source and local cache are both unavailable",
    cache_db_config_relative_path: "Devin/User/globalStorage/state.vscdb",
    cache_db_fallback_paths: &["Windsurf/User/globalStorage/state.vscdb"],
    auth_status_key_candidates: &["windsurfAuthStatus", "antigravityAuthStatus"],
    process_markers: &[
        "--ide_name windsurf",
        "/devin.app/",
        "/devin/",
        ".devin/",
        // legacy backward compat：尚未升级到 Devin 品牌的 Windsurf 安装
        "/windsurf/",
        ".windsurf/",
    ],
    cached_plan_info_key_candidates: &["windsurf.settings.cachedPlanInfo"],
    // 3 小时：与 Antigravity 一致；仍有 seat_source 云端兜底
    cache_max_age_secs: 3 * 60 * 60,
};
