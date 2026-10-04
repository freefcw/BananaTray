use crate::models::{AppSettings, ProviderId, ProviderKind, ProviderLayoutItem};

use super::job::ProviderRetentionCutoff;
use super::series::HistoryRange;

pub const DAY_MS: i64 = 86_400_000;

pub fn cutoff_ms(now_ms: i64, days: u16) -> i64 {
    now_ms.saturating_sub(i64::from(days) * DAY_MS)
}

/// 读图窗口的起点不能早于这个 provider 的保留期限。查询窗口仍是选中的范围；画出来的横轴跟样本走。
pub fn query_window(range: HistoryRange, now_ms: i64, retention_days: u16) -> (i64, i64, i64, i64) {
    let (axis_start, axis_end) = range.window(now_ms);
    let from = axis_start.max(cutoff_ms(now_ms, retention_days));
    (axis_start, axis_end, from, axis_end)
}

/// 启动和改天数共用这一份名单。
///
/// 内置 id 只要还在 layout 里就裁。自定义 id 必须这次已经在 `loaded_custom_ids` 里。
pub fn retention_cutoffs(
    layout: &[ProviderLayoutItem],
    loaded_custom_ids: &[ProviderId],
    settings: &AppSettings,
    now_ms: i64,
) -> Vec<ProviderRetentionCutoff> {
    let mut targets = Vec::new();
    for item in layout {
        if ProviderKind::from_id_key(item.id()).is_none() {
            continue;
        }
        targets.push(cutoff_for(item.id(), settings, now_ms));
    }
    for id in loaded_custom_ids {
        if !id.is_custom() {
            continue;
        }
        let key = id.id_key();
        if targets.iter().any(|target| target.provider_id == key) {
            continue;
        }
        targets.push(cutoff_for(&key, settings, now_ms));
    }
    targets
}

fn cutoff_for(provider_id: &str, settings: &AppSettings, now_ms: i64) -> ProviderRetentionCutoff {
    let id = ProviderId::from_id_key(provider_id);
    let days = settings.effective_history_retention_days(&id);
    ProviderRetentionCutoff {
        provider_id: provider_id.to_string(),
        cutoff_ms: cutoff_ms(now_ms, days),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryRange;
    use crate::models::{AppSettings, ProviderId, ProviderKind, ProviderLayoutItem};

    #[test]
    fn unloaded_custom_id_stays_out_while_disabled_builtin_is_included() {
        let mut settings = AppSettings::default();
        settings
            .provider
            .provider_layout
            .push(ProviderLayoutItem::new("relay:script", true, false));
        settings
            .provider
            .set_enabled(&ProviderId::BuiltIn(ProviderKind::Codex), false);
        settings
            .provider
            .set_history_retention_days(&ProviderId::BuiltIn(ProviderKind::Codex), Some(7));

        let targets = retention_cutoffs(
            &settings.provider.provider_layout,
            &[],
            &settings,
            10 * DAY_MS,
        );
        assert!(targets
            .iter()
            .any(|target| target.provider_id == "codex"
                && target.cutoff_ms == cutoff_ms(10 * DAY_MS, 7)));
        assert!(targets
            .iter()
            .all(|target| target.provider_id != "relay:script"));
    }

    #[test]
    fn loaded_disabled_custom_id_is_included() {
        let settings = AppSettings::default();
        let loaded = [ProviderId::Custom("relay:script".to_string())];
        let targets = retention_cutoffs(
            &settings.provider.provider_layout,
            &loaded,
            &settings,
            DAY_MS,
        );
        assert!(targets
            .iter()
            .any(|target| target.provider_id == "relay:script"));
    }

    #[test]
    fn query_window_does_not_start_before_retention() {
        let now = 40 * DAY_MS;
        let (axis_start, _, from, _) = query_window(HistoryRange::Last30Days, now, 7);
        assert!(from > axis_start);
        assert_eq!(from, cutoff_ms(now, 7));
    }
}
