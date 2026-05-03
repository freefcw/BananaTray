mod layout;
pub mod newapi;
mod provider;
mod quota;
mod settings;
#[cfg(test)]
pub(crate) mod test_helpers;

// 统一 re-export，保持外部 `use crate::models::Xxx` 路径不变
pub use layout::{
    compute_popup_height_detailed, compute_popup_height_for_overview,
    compute_popup_height_for_quotas, PopupLayout,
};
pub use newapi::{newapi_provider_id, NewApiConfig, NewApiEditData};
pub use provider::{
    NavTab, ProviderCapability, ProviderDescriptor, ProviderId, ProviderKind, ProviderMetadata,
    SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
};
pub use quota::{
    ConnectionStatus, ErrorKind, FailureAdvice, FailureReason, ProviderFailure, ProviderStatus,
    QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, RefreshData, StatusLevel, UpdateStatus,
};
pub use settings::{
    AppSettings, AppTheme, DisplaySettings, NotificationSettings, ProviderConfig, ProviderSettings,
    QuotaDisplayMode, SavedWindowPosition, SystemSettings, TrayIconStyle, TrayPopupSettings,
};
