use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;

pub struct GeminiProvider {}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini:mock"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Gemini
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        Ok(vec![
            QuotaInfo::new("Pro", 0.0, 100.0),
            QuotaInfo::new("Flash", 0.0, 100.0),
            QuotaInfo::new("Flash Lite", 0.0, 100.0),
        ])
    }
}
