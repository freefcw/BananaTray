mod custom_provider_lifecycle;
mod layout;
pub mod newapi;
mod provider;
mod quota;
mod script_provider;
mod settings;
#[cfg(test)]
pub(crate) mod test_helpers;

// 统一 re-export，保持外部 `use crate::models::Xxx` 路径不变
pub use custom_provider_lifecycle::{
    CustomProviderLifecycleFailure, NewApiSaveSuccess, ScriptProviderDeleteSuccess,
    ScriptProviderSaveSuccess,
};
pub use layout::{
    compute_popup_height_detailed, compute_popup_height_for_overview,
    compute_popup_height_for_quotas, PopupLayout,
};
pub use newapi::{
    format_divisor_value, format_optional_divisor_value, newapi_provider_id, parse_divisor_input,
    NewApiConfig, NewApiDivisorError, NewApiEditData,
};
pub use provider::{
    NavTab, ProviderCapability, ProviderDescriptor, ProviderId, ProviderKind, ProviderMetadata,
    SettingsCapability, TokenEditMode, TokenInputCapability, TokenInputState,
};
pub use quota::{
    ConnectionStatus, ErrorKind, FailureAdvice, FailureReason, ProviderFailure, ProviderStatus,
    QuotaDetailSpec, QuotaInfo, QuotaLabelSpec, QuotaType, RefreshData, StatusLevel, UpdateStatus,
};
pub use script_provider::{
    parse_script_stdout, script_provider_id, script_provider_id_from_slug, script_provider_slug,
    unique_script_provider_id, ScriptProviderConfig, ScriptProviderEditData,
    ScriptProviderQuotaPreview, ScriptProviderTestResult, DEFAULT_SCRIPT_INTERPRETER,
    DEFAULT_SCRIPT_TIMEOUT_MS,
};
pub use settings::{
    AppSettings, AppTheme, DisplaySettings, NotificationSettings, ProviderConfig, ProviderSettings,
    QuotaDisplayMode, SavedWindowPosition, SystemSettings, TrayIconStyle, TrayPopupSettings,
};
