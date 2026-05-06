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
