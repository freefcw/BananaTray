mod layout;
mod provider;
mod quota;
mod settings;

// 统一 re-export，保持外部 `use crate::models::Xxx` 路径不变
pub use layout::{compute_popup_height_for_quotas, PopupLayout};
pub use provider::{NavTab, ProviderKind, ProviderMetadata};
pub use quota::{ConnectionStatus, ProviderStatus, QuotaInfo, QuotaType, RefreshData, StatusLevel};
pub use settings::{AppSettings, AppTheme};
