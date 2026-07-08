use std::path::PathBuf;

pub(crate) type CustomProviderLifecycleResult<T> = Result<T, CustomProviderLifecycleError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CustomProviderLifecycleError {
    InvalidProviderId {
        operation: &'static str,
        expected: &'static str,
        actual: String,
    },
    ProviderYamlNotFound {
        operation: &'static str,
        provider_id: String,
        expected_path: Option<PathBuf>,
    },
    InvalidScriptProvider {
        operation: &'static str,
        provider_id: String,
        reason: String,
    },
    FileOperation {
        operation: &'static str,
        detail: String,
    },
}

impl CustomProviderLifecycleError {
    pub(crate) fn invalid_provider_id(
        operation: &'static str,
        expected: &'static str,
        actual: impl Into<String>,
    ) -> Self {
        Self::InvalidProviderId {
            operation,
            expected,
            actual: actual.into(),
        }
    }

    pub(crate) fn yaml_not_found(
        operation: &'static str,
        provider_id: impl Into<String>,
        expected_path: Option<PathBuf>,
    ) -> Self {
        Self::ProviderYamlNotFound {
            operation,
            provider_id: provider_id.into(),
            expected_path,
        }
    }

    pub(crate) fn invalid_script_provider(
        operation: &'static str,
        provider_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::InvalidScriptProvider {
            operation,
            provider_id: provider_id.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn file_operation(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::FileOperation {
            operation,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for CustomProviderLifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProviderId {
                operation,
                expected,
                actual,
            } => write!(
                f,
                "{operation}: expected {expected} provider id, got {actual}"
            ),
            Self::ProviderYamlNotFound {
                operation,
                provider_id,
                expected_path,
            } => {
                write!(f, "{operation}: provider YAML not found for {provider_id}")?;
                if let Some(path) = expected_path {
                    write!(f, " (expected {} or matching .yml)", path.display())?;
                }
                Ok(())
            }
            Self::InvalidScriptProvider {
                operation,
                provider_id,
                reason,
            } => write!(
                f,
                "{operation}: invalid script provider {provider_id}: {reason}"
            ),
            Self::FileOperation { operation, detail } => write!(f, "{operation}: {detail}"),
        }
    }
}

impl std::error::Error for CustomProviderLifecycleError {}
