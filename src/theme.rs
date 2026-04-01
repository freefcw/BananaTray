use gpui::*;

#[derive(Clone)]
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_subtle: Hsla,
    pub bg_card: Hsla,
    /// 卡片内层背景（更深的黑色，用于 quota 卡片）
    pub bg_card_inner: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_accent: Hsla,
    pub text_accent_soft: Hsla,
    pub border_subtle: Hsla,
    pub border_strong: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub status_success: Hsla,
    pub status_error: Hsla,
    pub status_warning: Hsla,
    pub progress_track: Hsla,

    // ── Lumina Bar 新增 token ──
    /// 状态徽章：HEALTHY 绿色
    pub badge_healthy: Hsla,
    /// 状态徽章：DEGRADED 黄/橙色
    pub badge_degraded: Hsla,
    /// 状态徽章：OFFLINE 红色
    pub badge_offline: Hsla,
    /// 状态徽章文字颜色
    #[allow(dead_code)]
    pub badge_text: Hsla,
    /// 头部 Synced 徽章背景
    pub badge_synced_bg: Hsla,
    /// 底部危险按钮（关闭）背景
    pub btn_danger_bg: Hsla,
    /// Sync Data 按钮背景
    pub btn_sync_bg: Hsla,
    /// Sync Data 按钮文字
    pub btn_sync_text: Hsla,
    /// 导航选中 pill 背景
    pub nav_pill_active_bg: Hsla,
    /// 导航选中 pill 文字
    pub nav_pill_active_text: Hsla,
}

impl Global for Theme {}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg_base: rgb(0xffffff).into(),
            bg_panel: rgb(0xf6f6f8).into(),  // 面板底色：浅灰
            bg_subtle: rgb(0xececee).into(), // 更深的灰底
            bg_card: rgb(0xf0f0f2).into(),
            bg_card_inner: rgb(0xffffff).into(), // quota 卡片：纯白
            text_primary: rgb(0x18181b).into(),
            text_secondary: rgb(0x71717a).into(),
            text_muted: rgb(0xa1a1aa).into(),
            text_accent: rgb(0x2563eb).into(),
            text_accent_soft: rgb(0xdbeafe).into(),
            border_subtle: rgb(0xe4e4e7).into(),
            border_strong: rgb(0xd0d0d5).into(), // 卡片边框：更深
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x2563eb).into(),
            status_success: rgb(0x22c55e).into(),
            status_error: rgb(0xef4444).into(),
            status_warning: rgb(0xf59e0b).into(),
            progress_track: rgba(0x00000012).into(), // 进度条轨道
            // Lumina 新增
            badge_healthy: rgb(0x22c55e).into(),
            badge_degraded: rgb(0xf59e0b).into(),
            badge_offline: rgb(0xef4444).into(),
            badge_text: rgb(0xffffff).into(),
            badge_synced_bg: rgba(0x22c55e1a).into(),
            btn_danger_bg: rgba(0xef44441a).into(),
            btn_sync_bg: rgb(0x27272a).into(),
            btn_sync_text: rgb(0xf4f4f5).into(),
            nav_pill_active_bg: rgb(0x18181b).into(),
            nav_pill_active_text: rgb(0xffffff).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0a0a0c).into(),
            bg_panel: rgb(0x111114).into(),
            bg_subtle: rgb(0x1c1c20).into(),
            bg_card: rgb(0x1c1c20).into(),
            bg_card_inner: rgb(0x151518).into(),
            text_primary: rgb(0xf4f4f5).into(),
            text_secondary: rgb(0xa1a1aa).into(),
            text_muted: rgb(0x71717a).into(),
            text_accent: rgb(0x3b82f6).into(),
            text_accent_soft: rgb(0x1e3a8a).into(),
            border_subtle: rgb(0x2a2a2e).into(),
            border_strong: rgb(0x3f3f46).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x3b82f6).into(),
            status_success: rgb(0x22c55e).into(),
            status_error: rgb(0xef4444).into(),
            status_warning: rgb(0xf59e0b).into(),
            progress_track: rgba(0xffffff1a).into(),
            // Lumina 新增
            badge_healthy: rgb(0x22c55e).into(),
            badge_degraded: rgb(0xf59e0b).into(),
            badge_offline: rgb(0xef4444).into(),
            badge_text: rgb(0x0a0a0c).into(),
            badge_synced_bg: rgba(0x22c55e1a).into(),
            btn_danger_bg: rgba(0xef44442e).into(),
            btn_sync_bg: rgb(0x1c1c20).into(),
            btn_sync_text: rgb(0xf4f4f5).into(),
            nav_pill_active_bg: rgb(0x2c2c30).into(),
            nav_pill_active_text: rgb(0xf4f4f5).into(),
        }
    }
}
