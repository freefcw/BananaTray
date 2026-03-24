use super::AiProvider;
use crate::models::{ProviderKind, QuotaInfo};
use anyhow::Result;
use async_trait::async_trait;

pub struct ClaudeProvider {
    // 可以在此处后续添加各种 probe、配置库或 Http Client 等状态
}

impl ClaudeProvider {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Claude
    }

    async fn is_available(&self) -> bool {
        // 先假设有效，后续可检测 CLI (例如 "claude --version")
        true
    }

    async fn refresh(&self) -> Result<Vec<QuotaInfo>> {
        // TODO: 替换为实际通过命令行或 API 抓取
        // 这里只是一个初始的 mock 实现，一旦运行即可证明管理架构走通
        Ok(vec![
            QuotaInfo::new("Session (5h)", 35.0, 50.0),
            QuotaInfo::new("Daily", 120.0, 200.0),
        ])
    }
}
