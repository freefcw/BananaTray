// 测试仍覆盖已废弃的 set_provider_enabled API，确保其行为正确
#![allow(deprecated)]
use super::*;

fn builtin(kind: ProviderKind) -> ProviderId {
    ProviderId::BuiltIn(kind)
}

// ── ProviderConfig 核心逻辑测试 ──────────────────────

#[test]
fn provider_config_is_enabled_default_false() {
    let config = ProviderConfig::default();
    assert!(!config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
}

#[test]
fn provider_config_set_and_check_enabled() {
    let mut config = ProviderConfig::default();
    config.set_provider_enabled(ProviderKind::Claude, true);
    assert!(config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
    assert!(!config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Gemini)));
}

#[test]
fn provider_config_set_disabled_preserves_layout_item() {
    let mut config = ProviderConfig::default();
    let custom = ProviderId::Custom("retry:api".to_string());

    config.set_enabled(&custom, false);

    assert!(config.has_layout_item(&custom));
    assert!(!config.is_enabled(&custom));
}

#[test]
fn register_discovered_custom_providers_auto_enables_missing_customs() {
    let mut config = ProviderConfig::default();
    let fresh = ProviderId::Custom("fresh:api".to_string());

    let registered = config.register_discovered_custom_providers(&[
        ProviderId::BuiltIn(ProviderKind::Claude),
        fresh.clone(),
    ]);

    assert_eq!(registered, vec![fresh.clone()]);
    assert!(config.is_enabled(&fresh));
    assert!(config.is_in_sidebar(&fresh));
}

#[test]
fn register_discovered_custom_providers_preserves_explicit_state_and_sidebar() {
    let mut config = ProviderConfig::default();
    let fresh = ProviderId::Custom("fresh:api".to_string());
    let disabled = ProviderId::Custom("disabled:api".to_string());
    config.add_to_sidebar(&fresh);
    config.set_enabled(&disabled, false);

    let registered =
        config.register_discovered_custom_providers(&[fresh.clone(), disabled.clone()]);

    assert_eq!(registered, vec![]);
    assert!(!config.is_enabled(&fresh));
    assert!(!config.is_enabled(&disabled));
    assert!(config.is_in_sidebar(&fresh));
}

#[test]
fn provider_config_ordered_providers_ignores_invalid() {
    let config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("gemini", true, false),
            ProviderLayoutItem::new("invalid", false, false),
            ProviderLayoutItem::new("claude", true, false),
            ProviderLayoutItem::new("gemini", true, false),
        ],
        ..Default::default()
    };

    let ordered = config.ordered_providers();
    assert_eq!(ordered[0], ProviderKind::Gemini);
    assert_eq!(ordered[1], ProviderKind::Claude);
    assert_eq!(ordered.len(), ProviderKind::all().len());
}

#[test]
fn provider_layout_item_disables_hidden_provider() {
    let hidden_enabled = ProviderLayoutItem::new("claude", false, true);
    let visible_enabled = ProviderLayoutItem::new("claude", true, true);

    assert!(!hidden_enabled.is_enabled());
    assert!(!hidden_enabled.is_in_sidebar());
    assert!(visible_enabled.is_enabled());
    assert!(visible_enabled.is_in_sidebar());
}

#[test]
fn normalize_layout_deduplicates_and_repairs_invalid_state() {
    let mut config = ProviderConfig {
        provider_layout: serde_json::from_value(serde_json::json!([
            {"id": "gemini", "in_sidebar": false, "enabled": true},
            {"id": "gemini", "in_sidebar": true, "enabled": true},
            {"id": "", "in_sidebar": true, "enabled": true}
        ]))
        .unwrap(),
        ..Default::default()
    };

    assert!(config.normalize_layout());
    assert_eq!(config.provider_layout.len(), 1);
    assert_eq!(config.provider_layout[0].id(), "gemini");
    assert!(!config.provider_layout[0].is_enabled());
    assert!(!config.provider_layout[0].is_in_sidebar());
}

#[test]
fn provider_config_quota_visibility() {
    let mut config = ProviderConfig::default();
    assert!(config.is_quota_visible(&builtin(ProviderKind::Claude), "session"));

    config.toggle_quota_visibility(&builtin(ProviderKind::Claude), "session".to_string());
    assert!(!config.is_quota_visible(&builtin(ProviderKind::Claude), "session"));
    // 其他 provider 不受影响
    assert!(config.is_quota_visible(&builtin(ProviderKind::Gemini), "session"));

    config.toggle_quota_visibility(&builtin(ProviderKind::Claude), "session".to_string());
    assert!(config.is_quota_visible(&builtin(ProviderKind::Claude), "session"));
}

#[test]
fn provider_config_move_to_index_normalizes_layout() {
    let mut config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("gemini", true, false),
            ProviderLayoutItem::new("gemini", true, false),
            ProviderLayoutItem::new("claude", true, false),
        ],
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(config.move_provider_to_index(&claude, 0, &[]));
    assert_eq!(
        config.provider_layout[0].id(),
        ProviderKind::Claude.id_key()
    );
    assert_eq!(
        config
            .provider_layout
            .iter()
            .filter(|item| item.id() == ProviderKind::Gemini.id_key())
            .count(),
        1
    );
}

// ── TrayIconStyle ────────────────────────────────────

#[test]
fn tray_icon_style_default_is_platform_aware() {
    let expected = if cfg!(target_os = "linux") {
        TrayIconStyle::Yellow
    } else {
        TrayIconStyle::Monochrome
    };
    assert_eq!(TrayIconStyle::default(), expected);
}

#[test]
fn tray_icon_style_serde_round_trip() {
    for style in [
        TrayIconStyle::Monochrome,
        TrayIconStyle::Yellow,
        TrayIconStyle::Colorful,
    ] {
        let json = serde_json::to_string(&style).unwrap();
        let deserialized: TrayIconStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(style, deserialized);
    }
}

#[test]
fn display_settings_missing_tray_popup_uses_default() {
    let json = serde_json::json!({
        "theme": AppTheme::Dark,
        "language": "system",
        "tray_icon_style": TrayIconStyle::default(),
        "quota_display_mode": QuotaDisplayMode::default(),
        "show_dashboard_button": true,
        "show_refresh_button": true,
        "show_debug_tab": false,
        "show_account_info": true,
        "show_overview": true
    });

    let restored: DisplaySettings = serde_json::from_value(json).unwrap();
    assert_eq!(restored.tray_popup, TrayPopupSettings::default());
}

#[test]
fn tray_popup_linux_position_round_trip() {
    let settings = DisplaySettings {
        tray_popup: TrayPopupSettings {
            linux_last_position: Some(SavedWindowPosition { x: 42.0, y: 84.0 }),
        },
        ..Default::default()
    };

    let json = serde_json::to_value(&settings).unwrap();
    let restored: DisplaySettings = serde_json::from_value(json).unwrap();
    assert_eq!(
        restored.tray_popup.linux_last_position,
        Some(SavedWindowPosition { x: 42.0, y: 84.0 })
    );
}

// ── hidden_quotas ────────────────────────────────────

#[test]
fn hidden_quotas_default_all_visible() {
    let settings = AppSettings::default();
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "session"));
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "model:Opus"));
}

#[test]
fn toggle_quota_visibility_hides_then_shows() {
    let mut settings = AppSettings::default();
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "model:Opus"));

    settings
        .provider
        .toggle_quota_visibility(&builtin(ProviderKind::Claude), "model:Opus".to_string());
    assert!(!settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "model:Opus"));
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "model:Sonnet"));

    settings
        .provider
        .toggle_quota_visibility(&builtin(ProviderKind::Claude), "model:Opus".to_string());
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "model:Opus"));
}

#[test]
fn hidden_quotas_isolated_per_provider() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .toggle_quota_visibility(&builtin(ProviderKind::Claude), "session".to_string());

    assert!(!settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Claude), "session"));
    assert!(settings
        .provider
        .is_quota_visible(&builtin(ProviderKind::Gemini), "session"));
}

#[test]
fn hidden_quotas_isolated_between_custom_providers() {
    let mut settings = AppSettings::default();
    let first = ProviderId::Custom("first:newapi".to_string());
    let second = ProviderId::Custom("second:newapi".to_string());

    settings
        .provider
        .toggle_quota_visibility(&first, "session".to_string());

    assert!(!settings.provider.is_quota_visible(&first, "session"));
    assert!(settings.provider.is_quota_visible(&second, "session"));
}

// ── ordered_provider_ids ──────────────────────────────

#[test]
fn ordered_provider_ids_respects_saved_order() {
    let settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("gemini", true, false),
                ProviderLayoutItem::new("claude", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let ids = settings.provider.ordered_provider_ids(&[]);
    assert_eq!(ids[0], ProviderId::BuiltIn(ProviderKind::Gemini));
    assert_eq!(ids[1], ProviderId::BuiltIn(ProviderKind::Claude));
    assert!(ids.len() >= ProviderKind::all().len());
}

#[test]
fn ordered_provider_ids_includes_custom() {
    let settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("gemini", true, false),
                ProviderLayoutItem::new("myai:cli", true, false),
                ProviderLayoutItem::new("claude", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };
    let custom = vec![ProviderId::Custom("myai:cli".to_string())];

    let ids = settings.provider.ordered_provider_ids(&custom);
    let pos_gemini = ids
        .iter()
        .position(|id| *id == ProviderId::BuiltIn(ProviderKind::Gemini))
        .unwrap();
    let pos_custom = ids
        .iter()
        .position(|id| *id == ProviderId::Custom("myai:cli".to_string()))
        .unwrap();
    let pos_claude = ids
        .iter()
        .position(|id| *id == ProviderId::BuiltIn(ProviderKind::Claude))
        .unwrap();
    assert!(pos_gemini < pos_custom);
    assert!(pos_custom < pos_claude);
}

#[test]
fn ordered_provider_ids_appends_unseen_custom() {
    let settings = AppSettings::default();
    let custom = vec![ProviderId::Custom("new:provider".to_string())];

    let ids = settings.provider.ordered_provider_ids(&custom);
    assert!(ids.contains(&ProviderId::Custom("new:provider".to_string())));
    assert_eq!(ids.len(), ProviderKind::all().len() + 1);
}

#[test]
fn ordered_provider_ids_deduplicates() {
    let settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("gemini", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let ids = settings.provider.ordered_provider_ids(&[]);
    let claude_count = ids
        .iter()
        .filter(|id| **id == ProviderId::BuiltIn(ProviderKind::Claude))
        .count();
    assert_eq!(claude_count, 1);
}

// ── prune_stale_custom_ids ──────────────────────────────

#[test]
fn prune_removes_stale_custom_from_enabled() {
    let mut config = ProviderConfig::default();
    config.set_enabled(&ProviderId::Custom("old:api".to_string()), true);
    config.set_enabled(&ProviderId::Custom("keep:api".to_string()), true);
    config.set_provider_enabled(ProviderKind::Claude, true);

    let existing = vec![ProviderId::Custom("keep:api".to_string())];
    let changed = config.prune_stale_custom_ids(&existing);

    assert!(changed);
    assert!(!config.is_enabled(&ProviderId::Custom("old:api".to_string())));
    assert!(config.is_enabled(&ProviderId::Custom("keep:api".to_string())));
    assert!(config.is_enabled(&ProviderId::BuiltIn(ProviderKind::Claude)));
}

#[test]
fn prune_removes_stale_custom_from_provider_layout() {
    let config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new(ProviderKind::Claude.id_key(), true, false),
            ProviderLayoutItem::new("old:api", true, false),
            ProviderLayoutItem::new("keep:api", true, false),
        ],
        ..Default::default()
    };
    // prune 需要 &mut，但 clippy 建议初始化时赋值，所以这里重新绑定
    let mut config = config;

    let existing = vec![ProviderId::Custom("keep:api".to_string())];
    let changed = config.prune_stale_custom_ids(&existing);

    assert!(changed);
    assert_eq!(config.provider_layout.len(), 2);
    assert!(!config.has_layout_item(&ProviderId::Custom("old:api".to_string())));
}

#[test]
fn prune_returns_false_when_nothing_to_prune() {
    let mut config = ProviderConfig::default();
    config.set_provider_enabled(ProviderKind::Claude, true);

    let existing: Vec<ProviderId> = vec![];
    let changed = config.prune_stale_custom_ids(&existing);

    assert!(!changed);
}

#[test]
fn prune_preserves_all_builtin_keys() {
    let mut config = ProviderConfig::default();
    for kind in ProviderKind::all() {
        config.set_provider_enabled(*kind, true);
    }

    let existing: Vec<ProviderId> = vec![];
    let changed = config.prune_stale_custom_ids(&existing);

    assert!(!changed);
    for kind in ProviderKind::all() {
        assert!(config.is_enabled(&ProviderId::BuiltIn(*kind)));
    }
}

// ── move_provider_to_index（拖拽排序）──────────────────

#[test]
fn move_provider_to_index_forward() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("gemini", true, false),
                ProviderLayoutItem::new("copilot", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    // claude 从 index 0 → index 2
    assert!(settings.provider.move_provider_to_index(&claude, 2, &[]));
    // ensure_order 展开后 claude 应在第三个位置
    let pos = settings
        .provider
        .provider_layout
        .iter()
        .position(|item| item.id() == "claude")
        .unwrap();
    assert_eq!(pos, 2);
}

#[test]
fn move_provider_to_index_backward() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("gemini", true, false),
                ProviderLayoutItem::new("copilot", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let copilot = ProviderId::BuiltIn(ProviderKind::Copilot);
    // copilot 从 index 2 → index 0
    assert!(settings.provider.move_provider_to_index(&copilot, 0, &[]));
    let pos = settings
        .provider
        .provider_layout
        .iter()
        .position(|item| item.id() == "copilot")
        .unwrap();
    assert_eq!(pos, 0);
}

#[test]
fn move_provider_to_index_same_position_returns_false() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("gemini", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(!settings.provider.move_provider_to_index(&claude, 0, &[]));
}

#[test]
fn move_provider_to_index_clamps_out_of_bounds() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("gemini", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    // target=999 应被 clamp 到末尾
    assert!(settings.provider.move_provider_to_index(&claude, 999, &[]));
    let visible = settings.provider.sidebar_provider_ids(&[]);
    assert_eq!(visible.last(), Some(&claude));
}

#[test]
fn move_custom_provider_to_index() {
    let custom = ProviderId::Custom("myai:cli".to_string());
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_layout: vec![
                ProviderLayoutItem::new("claude", true, false),
                ProviderLayoutItem::new("myai:cli", true, false),
                ProviderLayoutItem::new("gemini", true, false),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    // myai:cli 从 index 1 → index 0
    assert!(settings
        .provider
        .move_provider_to_index(&custom, 0, std::slice::from_ref(&custom)));
    assert_eq!(settings.provider.provider_layout[0].id(), "myai:cli");
    assert_eq!(settings.provider.provider_layout[1].id(), "claude");
}

// ── provider_layout / sidebar ─────────────────────────

#[test]
fn sidebar_provider_ids_returns_subset_in_layout_order() {
    let config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("gemini", true, false),
            ProviderLayoutItem::new("claude", true, false),
        ],
        ..Default::default()
    };
    let ids = config.sidebar_provider_ids(&[]);
    assert_eq!(
        ids,
        vec![builtin(ProviderKind::Gemini), builtin(ProviderKind::Claude)]
    );
}

#[test]
fn sidebar_provider_ids_excludes_hidden_items() {
    let config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("claude", true, false),
            ProviderLayoutItem::new("gemini", false, false),
        ],
        ..Default::default()
    };
    assert_eq!(
        config.sidebar_provider_ids(&[]),
        vec![builtin(ProviderKind::Claude)]
    );
}

#[test]
fn sidebar_provider_ids_includes_custom() {
    let config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("claude", true, false),
            ProviderLayoutItem::new("myai:newapi", true, false),
        ],
        ..Default::default()
    };
    let custom = vec![ProviderId::Custom("myai:newapi".to_string())];
    let ids = config.sidebar_provider_ids(&custom);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1], ProviderId::Custom("myai:newapi".to_string()));
}

#[test]
fn hidden_layout_item_keeps_its_position_when_readded() {
    let mut config = ProviderConfig {
        provider_layout: vec![
            ProviderLayoutItem::new("claude", true, false),
            ProviderLayoutItem::new("gemini", true, false),
        ],
        ..Default::default()
    };
    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(config.remove_from_sidebar(&claude));
    assert!(config.add_to_sidebar(&claude));
    assert_eq!(config.provider_layout[0].id(), "claude");
}

// ── addable_provider_kinds ────────────────────────────

#[test]
fn addable_provider_kinds_excludes_existing() {
    let config = ProviderConfig::default();
    let addable = config.addable_provider_kinds();
    assert!(!addable.contains(&ProviderKind::Claude));
    assert!(!addable.contains(&ProviderKind::Codex));
    assert!(addable.contains(&ProviderKind::Gemini));
    assert_eq!(addable.len(), ProviderKind::all().len() - 2);
}

#[test]
fn addable_provider_kinds_all_when_layout_empty() {
    let config = ProviderConfig {
        provider_layout: Vec::new(),
        ..Default::default()
    };
    assert_eq!(
        config.addable_provider_kinds().len(),
        ProviderKind::all().len()
    );
}

// ── add_to_sidebar ───────────────────────────────────

#[test]
fn add_to_sidebar_success() {
    let mut config = ProviderConfig {
        provider_layout: Vec::new(),
        ..Default::default()
    };
    let id = ProviderId::BuiltIn(ProviderKind::Gemini);
    assert!(config.add_to_sidebar(&id));
    assert!(config.is_in_sidebar(&id));
    assert_eq!(config.provider_layout[0].id(), "gemini");
}

#[test]
fn add_to_sidebar_duplicate_builtin_rejected() {
    let mut config = ProviderConfig::default();
    let id = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(!config.add_to_sidebar(&id));
    assert_eq!(
        config
            .provider_layout
            .iter()
            .filter(|item| item.id() == "claude" && item.is_in_sidebar())
            .count(),
        1
    );
}

#[test]
fn add_to_sidebar_duplicate_custom_rejected() {
    let mut config = ProviderConfig::default();
    let id = ProviderId::Custom("myai:newapi".to_string());
    assert!(config.add_to_sidebar(&id));
    assert!(!config.add_to_sidebar(&id));
    assert_eq!(
        config
            .provider_layout
            .iter()
            .filter(|item| item.id() == "myai:newapi" && item.is_in_sidebar())
            .count(),
        1
    );
}

// ── remove_from_sidebar ──────────────────────────────

#[test]
fn remove_from_sidebar_success() {
    let mut config = ProviderConfig::default();
    let id = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(config.remove_from_sidebar(&id));
    assert!(!config.is_in_sidebar(&id));
    assert!(!config.is_enabled(&id));
}

#[test]
fn remove_from_sidebar_nonexistent_noop() {
    let mut config = ProviderConfig::default();
    let id = ProviderId::BuiltIn(ProviderKind::Gemini);
    assert!(!config.remove_from_sidebar(&id));
}

// ── ProviderSettings credential accessors ──────────────

#[test]
fn get_credential_existing_key() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("github_token", "ghp_abc123".to_string());
    assert_eq!(settings.get_credential("github_token"), Some("ghp_abc123"));
}

#[test]
fn get_credential_missing_value() {
    let settings = ProviderSettings::default();
    assert_eq!(settings.get_credential("github_token"), None);
}

#[test]
fn get_credential_unknown_key() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("github_token", "ghp_abc123".to_string());
    assert_eq!(settings.get_credential("nonexistent_key"), None);
}

#[test]
fn set_credential_known_key() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("github_token", "ghp_new".to_string());
    assert_eq!(settings.get_credential("github_token"), Some("ghp_new"));
}

#[test]
fn set_credential_supports_arbitrary_key() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("custom_token", "value".to_string());
    assert_eq!(settings.get_credential("custom_token"), Some("value"));
}

#[test]
fn remove_credential_clears_value() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("github_token", "ghp_new".to_string());
    assert!(settings.remove_credential("github_token"));
    assert_eq!(settings.get_credential("github_token"), None);
}

#[test]
fn provider_settings_serializes_flattened_credentials() {
    let mut settings = ProviderSettings::default();
    settings.set_credential("github_token", "ghp_abc123".to_string());
    settings.set_credential("custom_token", "custom_value".to_string());

    let json = serde_json::to_value(&settings).unwrap();
    assert_eq!(json["github_token"], "ghp_abc123");
    assert_eq!(json["custom_token"], "custom_value");

    let restored: ProviderSettings = serde_json::from_value(json).unwrap();
    assert_eq!(restored.get_credential("github_token"), Some("ghp_abc123"));
    assert_eq!(
        restored.get_credential("custom_token"),
        Some("custom_value")
    );
}
