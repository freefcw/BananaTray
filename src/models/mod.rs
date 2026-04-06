mod layout;
mod provider;
mod quota;
mod settings;
#[cfg(test)]
pub(crate) mod test_helpers;

// 统一 re-export，保持外部 `use crate::models::Xxx` 路径不变
pub use layout::{compute_popup_height_detailed, compute_popup_height_for_quotas, PopupLayout};
pub use provider::{NavTab, ProviderDescriptor, ProviderId, ProviderKind, ProviderMetadata};
pub use quota::{
    ConnectionStatus, ErrorKind, ProviderStatus, QuotaInfo, QuotaType, RefreshData, StatusLevel,
    UpdateStatus,
};
pub use settings::{
    AppSettings, AppTheme, DisplaySettings, NotificationSettings, ProviderConfig, ProviderSettings,
    QuotaDisplayMode, SystemSettings, TrayIconStyle,
};
