#[cfg(feature = "app")]
pub(crate) mod api;
mod auth;
mod availability;
mod descriptor;
pub(crate) mod extractor;
mod fetch;
#[cfg(feature = "app")]
mod file_ops;
#[cfg(any(feature = "app", test))]
pub(in crate::providers::custom) mod generator;
mod json_file;
mod legacy_migrate;
#[cfg(feature = "app")]
mod lifecycle_error;
pub(crate) mod loader;
#[cfg(any(feature = "app", test))]
pub(in crate::providers::custom) mod locator;
mod log_utils;
#[cfg(feature = "app")]
mod newapi_lifecycle;
mod plan;
pub(crate) mod provider;
pub(in crate::providers::custom) mod schema;
#[cfg(feature = "app")]
mod script_provider_lifecycle;
mod url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::providers::custom) enum CustomProviderOrigin {
    NewApi,
    Script,
}

impl CustomProviderOrigin {
    pub(in crate::providers::custom) fn from_id(custom_id: &str) -> Option<Self> {
        if custom_id.ends_with(":newapi") {
            Some(Self::NewApi)
        } else if custom_id.ends_with(":script") {
            Some(Self::Script)
        } else {
            None
        }
    }
}

pub use loader::load_custom_providers;
