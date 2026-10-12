use serde::{Deserialize, Serialize};

/// 一条可兑换的限流重置。
///
/// 这不是额度、积分或请求次数。`title` 是上游给用户看的类型名；
/// 没有 title 时再用 `reset_type`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitReset {
    /// 上游类型标识，例如 `codex_rate_limits`。
    #[serde(default)]
    pub reset_type: String,
    /// 上游展示名，例如 `Full reset (Weekly + 5 hr)`。
    #[serde(default)]
    pub title: Option<String>,
    /// 获取时间，Unix 秒。
    #[serde(default)]
    pub granted_at_secs: Option<i64>,
    /// 到期时间，Unix 秒。没有到期时间时为 None。
    #[serde(default)]
    pub expires_at_secs: Option<i64>,
}

/// Codex 一次刷新里读到的限流重置。
///
/// `available_count == Some(0)` 表示上游明确说当前没有可兑换重置。
/// `details_known == false` 时只有数量，类型和到期时间还要另一次请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RateLimitResetSnapshot {
    #[serde(default)]
    pub available_count: Option<u32>,
    #[serde(default)]
    pub resets: Vec<RateLimitReset>,
    #[serde(default)]
    pub details_known: bool,
}

/// 展示层要用的重置明细。`undisclosed` 是有数量但没有类型/时间的剩余条数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitResetPresentation {
    pub resets: Vec<RateLimitReset>,
    pub undisclosed: u32,
}

impl RateLimitResetSnapshot {
    /// usage 只给了数量、还没有逐条明细时，才需要再请求详情。
    pub fn needs_detail_fetch(&self) -> bool {
        !self.details_known && self.available_count.unwrap_or(0) > 0
    }

    /// 没有可展示内容时返回 None。数量为 0 时以上游声明为准，不再展示残留明细。
    pub fn presentation(&self) -> Option<RateLimitResetPresentation> {
        if self.available_count == Some(0) {
            return None;
        }

        if self.resets.is_empty() {
            let count = self.available_count.unwrap_or(0);
            if count == 0 {
                return None;
            }
            return Some(RateLimitResetPresentation {
                resets: Vec::new(),
                undisclosed: count,
            });
        }

        let shown = u32::try_from(self.resets.len()).unwrap_or(u32::MAX);
        let count = self.available_count.unwrap_or(shown);
        Some(RateLimitResetPresentation {
            resets: self.resets.clone(),
            undisclosed: count.saturating_sub(shown),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_reset() -> RateLimitReset {
        RateLimitReset {
            reset_type: "codex_rate_limits".into(),
            title: Some("Full reset".into()),
            granted_at_secs: Some(1_735_689_600),
            expires_at_secs: Some(1_738_281_600),
        }
    }

    #[test]
    fn zero_count_hides_rows() {
        let snapshot = RateLimitResetSnapshot {
            available_count: Some(0),
            details_known: true,
            resets: vec![sample_reset()],
        };
        assert!(snapshot.presentation().is_none());
        assert!(!snapshot.needs_detail_fetch());
    }

    #[test]
    fn count_without_details_asks_for_a_follow_up_fetch() {
        let snapshot = RateLimitResetSnapshot {
            available_count: Some(2),
            details_known: false,
            resets: Vec::new(),
        };
        let presentation = snapshot.presentation().unwrap();
        assert!(presentation.resets.is_empty());
        assert_eq!(presentation.undisclosed, 2);
        assert!(snapshot.needs_detail_fetch());
    }

    #[test]
    fn known_rows_do_not_request_another_fetch() {
        let snapshot = RateLimitResetSnapshot {
            available_count: Some(2),
            details_known: true,
            resets: vec![sample_reset()],
        };
        let presentation = snapshot.presentation().unwrap();
        assert_eq!(presentation.resets.len(), 1);
        assert_eq!(presentation.undisclosed, 1);
        assert!(!snapshot.needs_detail_fetch());
    }
}
