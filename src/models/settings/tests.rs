use super::*;

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
fn provider_config_remove_enabled_record_clears_explicit_state() {
    let mut config = ProviderConfig::default();
    let custom = ProviderId::Custom("retry:newapi".to_string());

    config.set_enabled(&custom, false);
    assert_eq!(config.remove_enabled_record(&custom), Some(false));
    assert!(!config.enabled_providers.contains_key(&custom.id_key()));
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
    assert!(config.sidebar_providers.contains(&fresh.id_key()));
}

#[test]
fn register_discovered_custom_providers_preserves_explicit_state_and_sidebar() {
    let mut config = ProviderConfig {
        sidebar_providers: vec!["fresh:api".into()],
        ..Default::default()
    };
    let fresh = ProviderId::Custom("fresh:api".to_string());
    let disabled = ProviderId::Custom("disabled:api".to_string());
    config.set_enabled(&disabled, false);

    let registered =
        config.register_discovered_custom_providers(&[fresh.clone(), disabled.clone()]);

    assert_eq!(registered, vec![fresh.clone()]);
    assert!(config.is_enabled(&fresh));
    assert!(!config.is_enabled(&disabled));
    assert_eq!(
        config
            .sidebar_providers
            .iter()
            .filter(|key| **key == "fresh:api")
            .count(),
        1
    );
}

#[test]
fn provider_config_ordered_providers_ignores_invalid() {
    let config = ProviderConfig {
        provider_order: vec![
            "gemini".into(),
            "invalid".into(),
            "claude".into(),
            "gemini".into(), // duplicate
        ],
        ..Default::default()
    };

    let ordered = config.ordered_providers();
    assert_eq!(ordered[0], ProviderKind::Gemini);
    assert_eq!(ordered[1], ProviderKind::Claude);
    assert_eq!(ordered.len(), ProviderKind::all().len());
}

#[test]
fn provider_config_quota_visibility() {
    let mut config = ProviderConfig::default();
    assert!(config.is_quota_visible(ProviderKind::Claude, "session"));

    config.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());
    assert!(!config.is_quota_visible(ProviderKind::Claude, "session"));
    // 其他 provider 不受影响
    assert!(config.is_quota_visible(ProviderKind::Gemini, "session"));

    config.toggle_quota_visibility(ProviderKind::Claude, "session".to_string());
    assert!(config.is_quota_visible(ProviderKind::Claude, "session"));
}

#[test]
fn provider_config_move_to_index_normalizes_order() {
    let mut config = ProviderConfig {
        provider_order: vec!["gemini".into(), "gemini".into(), "claude".into()],
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    // 带重复 key 的 provider_order 应被 ensure_order 正规化，然后正确移动
    assert!(config.move_provider_to_index(&claude, 0, &[]));
    assert_eq!(config.provider_order[0], ProviderKind::Claude.id_key());
    assert_eq!(config.provider_order.len(), ProviderKind::all().len());
}

// ── TrayIconStyle ────────────────────────────────────

#[test]
fn tray_icon_style_default_is_monochrome() {
    assert_eq!(TrayIconStyle::default(), TrayIconStyle::Monochrome);
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

// ── 新/旧格式序列化 ──────────────────────────────────

#[test]
fn app_settings_new_format_round_trip() {
    let settings = AppSettings::default();
    let json = serde_json::to_value(&settings).unwrap();
    let restored: AppSettings = serde_json::from_value(json).unwrap();
    assert_eq!(restored.display.tray_icon_style, TrayIconStyle::Monochrome);
    assert_eq!(restored.system.refresh_interval_mins, 5);
}

// ── hidden_quotas ────────────────────────────────────

#[test]
fn hidden_quotas_default_all_visible() {
    let settings = AppSettings::default();
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "session"));
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "model:Opus"));
}

#[test]
fn toggle_quota_visibility_hides_then_shows() {
    let mut settings = AppSettings::default();
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "model:Opus"));

    settings
        .provider
        .toggle_quota_visibility(ProviderKind::Claude, "model:Opus".to_string());
    assert!(!settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "model:Opus"));
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "model:Sonnet"));

    settings
        .provider
        .toggle_quota_visibility(ProviderKind::Claude, "model:Opus".to_string());
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "model:Opus"));
}

#[test]
fn hidden_quotas_isolated_per_provider() {
    let mut settings = AppSettings::default();
    settings
        .provider
        .toggle_quota_visibility(ProviderKind::Claude, "session".to_string());

    assert!(!settings
        .provider
        .is_quota_visible(ProviderKind::Claude, "session"));
    assert!(settings
        .provider
        .is_quota_visible(ProviderKind::Gemini, "session"));
}

// ── ordered_provider_ids ──────────────────────────────

#[test]
fn ordered_provider_ids_respects_saved_order() {
    let settings = AppSettings {
        provider: ProviderConfig {
            provider_order: vec!["gemini".into(), "claude".into()],
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
            provider_order: vec!["gemini".into(), "myai:cli".into(), "claude".into()],
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
            provider_order: vec!["claude".into(), "claude".into(), "gemini".into()],
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
fn prune_removes_stale_custom_from_provider_order() {
    let config = ProviderConfig {
        provider_order: vec![
            ProviderKind::Claude.id_key().to_string(),
            "old:api".to_string(),
            "keep:api".to_string(),
        ],
        ..Default::default()
    };
    // prune 需要 &mut，但 clippy 建议初始化时赋值，所以这里重新绑定
    let mut config = config;

    let existing = vec![ProviderId::Custom("keep:api".to_string())];
    let changed = config.prune_stale_custom_ids(&existing);

    assert!(changed);
    assert_eq!(config.provider_order.len(), 2);
    assert!(!config.provider_order.contains(&"old:api".to_string()));
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
            provider_order: vec!["claude".into(), "gemini".into(), "copilot".into()],
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
        .provider_order
        .iter()
        .position(|k| k == "claude")
        .unwrap();
    assert_eq!(pos, 2);
}

#[test]
fn move_provider_to_index_backward() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_order: vec!["claude".into(), "gemini".into(), "copilot".into()],
            ..Default::default()
        },
        ..Default::default()
    };

    let copilot = ProviderId::BuiltIn(ProviderKind::Copilot);
    // copilot 从 index 2 → index 0
    assert!(settings.provider.move_provider_to_index(&copilot, 0, &[]));
    let pos = settings
        .provider
        .provider_order
        .iter()
        .position(|k| k == "copilot")
        .unwrap();
    assert_eq!(pos, 0);
}

#[test]
fn move_provider_to_index_same_position_returns_false() {
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_order: vec!["claude".into(), "gemini".into()],
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
            provider_order: vec!["claude".into(), "gemini".into()],
            ..Default::default()
        },
        ..Default::default()
    };

    let claude = ProviderId::BuiltIn(ProviderKind::Claude);
    // target=999 应被 clamp 到末尾
    assert!(settings.provider.move_provider_to_index(&claude, 999, &[]));
    let pos = settings
        .provider
        .provider_order
        .iter()
        .position(|k| k == "claude")
        .unwrap();
    assert_eq!(pos, settings.provider.provider_order.len() - 1);
}

#[test]
fn move_custom_provider_to_index() {
    let custom = ProviderId::Custom("myai:cli".to_string());
    let mut settings = AppSettings {
        provider: ProviderConfig {
            provider_order: vec!["claude".into(), "myai:cli".into(), "gemini".into()],
            ..Default::default()
        },
        ..Default::default()
    };

    // myai:cli 从 index 1 → index 0
    assert!(settings
        .provider
        .move_provider_to_index(&custom, 0, std::slice::from_ref(&custom)));
    assert_eq!(settings.provider.provider_order[0], "myai:cli");
    assert_eq!(settings.provider.provider_order[1], "claude");
}

// ── sidebar_provider_ids ──────────────────────────────

#[test]
fn sidebar_provider_ids_returns_subset() {
    let config = ProviderConfig {
        sidebar_providers: vec!["claude".into(), "gemini".into()],
        provider_order: vec!["gemini".into(), "claude".into()],
        ..Default::default()
    };
    let ids = config.sidebar_provider_ids(&[]);
    assert_eq!(ids.len(), 2);
    // 按 provider_order 排序
    assert_eq!(ids[0], ProviderId::BuiltIn(ProviderKind::Gemini));
    assert_eq!(ids[1], ProviderId::BuiltIn(ProviderKind::Claude));
}

#[test]
fn sidebar_provider_ids_excludes_non_sidebar() {
    let config = ProviderConfig {
        sidebar_providers: vec!["claude".into()],
        ..Default::default()
    };
    let ids = config.sidebar_provider_ids(&[]);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], ProviderId::BuiltIn(ProviderKind::Claude));
}

#[test]
fn sidebar_provider_ids_includes_custom() {
    let config = ProviderConfig {
        sidebar_providers: vec!["claude".into(), "myai:newapi".into()],
        provider_order: vec!["claude".into(), "myai:newapi".into()],
        ..Default::default()
    };
    let custom = vec![ProviderId::Custom("myai:newapi".to_string())];
    let ids = config.sidebar_provider_ids(&custom);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1], ProviderId::Custom("myai:newapi".to_string()));
}

// ── addable_provider_kinds ────────────────────────────

#[test]
fn addable_provider_kinds_excludes_existing() {
    let config = ProviderConfig {
        sidebar_providers: vec!["claude".into(), "codex".into()],
        ..Default::default()
    };
    let addable = config.addable_provider_kinds();
    assert!(!addable.contains(&ProviderKind::Claude));
    assert!(!addable.contains(&ProviderKind::Codex));
    assert!(addable.contains(&ProviderKind::Gemini));
    assert_eq!(addable.len(), ProviderKind::all().len() - 2);
}

#[test]
fn addable_provider_kinds_all_when_sidebar_empty() {
    let config = ProviderConfig::default();
    let addable = config.addable_provider_kinds();
    assert_eq!(addable.len(), ProviderKind::all().len());
}

// ── add_to_sidebar ───────────────────────────────────

#[test]
fn add_to_sidebar_success() {
    let mut config = ProviderConfig::default();
    let id = ProviderId::BuiltIn(ProviderKind::Gemini);
    assert!(config.add_to_sidebar(&id));
    assert!(config.sidebar_providers.contains(&"gemini".to_string()));
    assert!(config.provider_order.contains(&"gemini".to_string()));
}

#[test]
fn add_to_sidebar_duplicate_builtin_rejected() {
    let mut config = ProviderConfig {
        sidebar_providers: vec!["claude".into()],
        ..Default::default()
    };
    let id = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(!config.add_to_sidebar(&id));
    // sidebar 中仍只有一个 claude
    assert_eq!(
        config
            .sidebar_providers
            .iter()
            .filter(|k| *k == "claude")
            .count(),
        1
    );
}

#[test]
fn add_to_sidebar_custom_allows_duplicate() {
    let mut config = ProviderConfig {
        sidebar_providers: vec!["myai:newapi".into()],
        ..Default::default()
    };
    let id = ProviderId::Custom("myai:newapi".to_string());
    assert!(config.add_to_sidebar(&id));
    assert_eq!(
        config
            .sidebar_providers
            .iter()
            .filter(|k| *k == "myai:newapi")
            .count(),
        2
    );
}

// ── remove_from_sidebar ──────────────────────────────

#[test]
fn remove_from_sidebar_success() {
    let mut config = ProviderConfig {
        sidebar_providers: vec!["claude".into(), "gemini".into()],
        ..Default::default()
    };
    let id = ProviderId::BuiltIn(ProviderKind::Claude);
    assert!(config.remove_from_sidebar(&id));
    assert!(!config.sidebar_providers.contains(&"claude".to_string()));
    assert_eq!(config.sidebar_providers.len(), 1);
}

#[test]
fn remove_from_sidebar_nonexistent_noop() {
    let mut config = ProviderConfig {
        sidebar_providers: vec!["claude".into()],
        ..Default::default()
    };
    let id = ProviderId::BuiltIn(ProviderKind::Gemini);
    assert!(!config.remove_from_sidebar(&id));
    assert_eq!(config.sidebar_providers.len(), 1);
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
