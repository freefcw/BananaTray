mod failure;
mod info;
mod label;
mod provider_status;
mod refresh_data;
mod types;

pub use failure::{FailureAdvice, FailureReason, ProviderFailure};
pub use info::QuotaInfo;
pub use label::{QuotaDetailSpec, QuotaLabelSpec};
pub use provider_status::{ConnectionStatus, ErrorKind, ProviderStatus, UpdateStatus};
pub use refresh_data::RefreshData;
pub use types::{QuotaType, StatusLevel};

pub(super) use label::slugify_key;

#[cfg(test)]
mod tests;
