mod ai_provider;
mod error;

pub(crate) mod codeium_family;
pub(crate) mod common;
pub(crate) mod custom;
pub(crate) mod manager;

use crate::models::{AppSettings, TokenEditMode, TokenInputState};
use std::sync::Arc;

pub use ai_provider::AiProvider;
#[cfg(test)]
pub(crate) use copilot::copilot_settings_capability;
pub use error::{ProviderError, ProviderResult};
pub use manager::{ProviderManager, ProviderManagerHandle};

pub(crate) fn default_token_input_state(
    settings: &AppSettings,
    credential_key: &'static str,
) -> TokenInputState {
    let value = settings.provider.credentials.get_credential(credential_key);
    let has_token = value.is_some();
    TokenInputState {
        has_token,
        masked: value.map(mask_token),
        source_i18n_key: None,
        edit_mode: if has_token {
            TokenEditMode::EditStored
        } else {
            TokenEditMode::SetNew
        },
    }
}

fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        "•".repeat(chars.len())
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{}•••{}", prefix, suffix)
    }
}

/// 消除零字段 Provider 的重复样板代码（struct + Default + new）
macro_rules! define_unit_provider {
    ($name:ident) => {
        pub struct $name;

        impl Default for $name {
            fn default() -> Self {
                Self
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self
            }
        }
    };
}
pub(crate) use define_unit_provider;

macro_rules! register_providers {
    ($($variant:ident => $id:literal => $mod_name:ident::$struct_name:ident),* $(,)?) => {
        $(mod $mod_name;)*

        /// 注册所有可用的 Provider 实现
        pub(crate) fn register_all(manager: &mut ProviderManager) {
            $(
                manager.register(Arc::new($mod_name::$struct_name::new()));
            )*
        }
    };
}

crate::builtin_provider_manifest::builtin_provider_manifest!(register_providers);
