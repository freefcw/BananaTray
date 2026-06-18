//! Tray entry command policy.

use crate::application::AppSession;
use crate::models::NavTab;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProviderToggleTarget {
    Show(NavTab),
    OpenSettings,
}

pub(super) fn provider_toggle_target(session: &mut AppSession) -> ProviderToggleTarget {
    let provider_tab = session.default_provider_tab();

    // Overview 启用时优先展示 Overview tab
    if session.settings.display.show_overview {
        ProviderToggleTarget::Show(NavTab::Overview)
    } else if let Some(tab) = provider_tab {
        ProviderToggleTarget::Show(tab)
    } else {
        ProviderToggleTarget::OpenSettings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_helpers::make_test_provider;
    use crate::models::{ConnectionStatus, ProviderId, ProviderKind};

    fn make_session(kinds: &[ProviderKind], enabled: &[ProviderKind]) -> AppSession {
        let providers = kinds
            .iter()
            .map(|k| make_test_provider(*k, ConnectionStatus::Disconnected))
            .collect::<Vec<_>>();
        let mut settings = crate::models::AppSettings::default();
        for k in enabled {
            settings
                .provider
                .set_enabled(&ProviderId::BuiltIn(*k), true);
        }
        AppSession::new(settings, providers)
    }

    #[test]
    fn overview_enabled_shows_overview_tab() {
        let mut session = make_session(&[ProviderKind::Claude], &[ProviderKind::Claude]);
        session.settings.display.show_overview = true;

        assert_eq!(
            provider_toggle_target(&mut session),
            ProviderToggleTarget::Show(NavTab::Overview)
        );
    }

    #[test]
    fn overview_disabled_shows_provider_tab_when_enabled() {
        let mut session = make_session(&[ProviderKind::Claude], &[ProviderKind::Claude]);
        session.settings.display.show_overview = false;

        let result = provider_toggle_target(&mut session);
        assert!(matches!(
            result,
            ProviderToggleTarget::Show(NavTab::Provider(_))
        ));
    }

    #[test]
    fn overview_disabled_no_enabled_provider_opens_settings() {
        let mut session = make_session(&[ProviderKind::Claude], &[]);
        session.settings.display.show_overview = false;

        assert_eq!(
            provider_toggle_target(&mut session),
            ProviderToggleTarget::OpenSettings
        );
    }

    #[test]
    fn overview_takes_priority_over_provider() {
        // 即使有 enabled provider，overview 开启时也优先
        let mut session = make_session(
            &[ProviderKind::Claude, ProviderKind::Gemini],
            &[ProviderKind::Claude, ProviderKind::Gemini],
        );
        session.settings.display.show_overview = true;

        assert_eq!(
            provider_toggle_target(&mut session),
            ProviderToggleTarget::Show(NavTab::Overview)
        );
    }
}
