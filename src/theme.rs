use gpui::*;

/// 判断 WindowAppearance 是否为深色系
pub fn is_dark_appearance(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

// ── 子结构体 ──────────────────────────────────────────────

/// 背景色 token
#[derive(Clone)]
pub struct ThemeBg {
    pub base: Hsla,
    pub panel: Hsla,
    pub subtle: Hsla,
    pub card: Hsla,
    /// 卡片内层背景（更深的黑色，用于 quota 卡片）
    pub card_inner: Hsla,
    /// 卡片内层背景 hover 态（亮度稍高）
    pub card_inner_hovered: Hsla,
}

/// 文字色 token
#[derive(Clone)]
pub struct ThemeText {
    pub primary: Hsla,
    pub secondary: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
}

/// 边框色 token
#[derive(Clone)]
pub struct ThemeBorder {
    pub subtle: Hsla,
    pub strong: Hsla,
}

/// 交互元素色 token
#[derive(Clone)]
pub struct ThemeElement {
    pub active: Hsla,
    pub selected: Hsla,
}

/// 状态色 token
#[derive(Clone)]
pub struct ThemeStatus {
    pub success: Hsla,
    pub error: Hsla,
    pub warning: Hsla,
    pub progress_track: Hsla,
    /// 进度条渐变起始色（靛蓝）
    pub bar_gradient_start: Hsla,
    /// 进度条渐变中间色（青色）
    pub bar_gradient_mid: Hsla,
}

/// 状态徽章色 token
#[derive(Clone)]
pub struct ThemeBadge {
    pub healthy: Hsla,
    pub degraded: Hsla,
    pub offline: Hsla,
    #[allow(dead_code)]
    pub text: Hsla,
    pub synced_bg: Hsla,
    /// "正在同步" 状态的徽章背景色
    pub syncing_bg: Hsla,
}

/// 按钮色 token
#[derive(Clone)]
pub struct ThemeButton {
    pub danger_bg: Hsla,
    pub sync_bg: Hsla,
    pub sync_text: Hsla,
    /// 操作型按钮背景色（如 Force Refresh）
    pub action_bg: Hsla,
    /// 操作型按钮文字色
    pub action_text: Hsla,
}

/// 日志级别颜色 token
///
/// 浅色模式使用更深/更饱和的色值，确保在白色背景上对比度足够；
/// 深色模式使用更明亮的色值，确保在暗色背景上清晰可读。
#[derive(Clone)]
pub struct ThemeLog {
    pub error: Hsla,
    pub warn: Hsla,
    pub info: Hsla,
    pub debug: Hsla,
    pub trace: Hsla,
}

/// 导航色 token
#[derive(Clone)]
pub struct ThemeNav {
    pub pill_active_bg: Hsla,
    pub pill_active_text: Hsla,
}

// ── 主结构体 ──────────────────────────────────────────────

#[derive(Clone)]
pub struct Theme {
    pub bg: ThemeBg,
    pub text: ThemeText,
    pub border: ThemeBorder,
    pub element: ThemeElement,
    pub status: ThemeStatus,
    pub badge: ThemeBadge,
    pub button: ThemeButton,
    pub nav: ThemeNav,
    pub log: ThemeLog,
}

impl Global for Theme {}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg: ThemeBg {
                base: rgb(0xffffff).into(),
                panel: rgb(0xf6f6f8).into(),
                subtle: rgb(0xececee).into(),
                card: rgb(0xf0f0f2).into(),
                card_inner: rgb(0xffffff).into(),
                card_inner_hovered: rgb(0xededef).into(),
            },
            text: ThemeText {
                primary: rgb(0x18181b).into(),
                secondary: rgb(0x71717a).into(),
                muted: rgb(0xa1a1aa).into(),
                accent: rgb(0x2563eb).into(),
                accent_soft: rgb(0xdbeafe).into(),
            },
            border: ThemeBorder {
                subtle: rgb(0xe4e4e7).into(),
                strong: rgb(0xd0d0d5).into(),
            },
            element: ThemeElement {
                active: rgb(0xffffff).into(),
                selected: rgb(0x2563eb).into(),
            },
            status: ThemeStatus {
                success: rgb(0x22c55e).into(),
                error: rgb(0xef4444).into(),
                warning: rgb(0xf59e0b).into(),
                progress_track: rgba(0x00000012).into(),
                bar_gradient_start: rgb(0x6366f1).into(),
                bar_gradient_mid: rgb(0x06b6d4).into(),
            },
            badge: ThemeBadge {
                healthy: rgb(0x22c55e).into(),
                degraded: rgb(0xf59e0b).into(),
                offline: rgb(0xef4444).into(),
                text: rgb(0xffffff).into(),
                synced_bg: rgba(0x22c55e1a).into(),
                syncing_bg: rgba(0x2563eb1a).into(),
            },
            button: ThemeButton {
                danger_bg: rgba(0xef44441a).into(),
                sync_bg: rgb(0x27272a).into(),
                sync_text: rgb(0xf4f4f5).into(),
                action_bg: rgb(0x2d6a4f).into(),
                action_text: rgb(0xffffff).into(),
            },
            nav: ThemeNav {
                pill_active_bg: rgb(0x18181b).into(),
                pill_active_text: rgb(0xffffff).into(),
            },
            log: ThemeLog {
                error: rgb(0xdc2626).into(),
                warn: rgb(0xd97706).into(),
                info: rgb(0x16a34a).into(),
                debug: rgb(0x2563eb).into(),
                trace: rgb(0x78716c).into(),
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: ThemeBg {
                base: rgb(0x0a0a0c).into(),
                panel: rgb(0x111114).into(),
                subtle: rgb(0x1c1c20).into(),
                card: rgb(0x1c1c20).into(),
                card_inner: rgb(0x151518).into(),
                card_inner_hovered: rgb(0x272329).into(),
            },
            text: ThemeText {
                primary: rgb(0xf4f4f5).into(),
                secondary: rgb(0xa1a1aa).into(),
                muted: rgb(0x71717a).into(),
                accent: rgb(0x3b82f6).into(),
                accent_soft: rgb(0x1e3a8a).into(),
            },
            border: ThemeBorder {
                subtle: rgb(0x2a2a2e).into(),
                strong: rgb(0x3f3f46).into(),
            },
            element: ThemeElement {
                active: rgb(0xffffff).into(),
                selected: rgb(0x3b82f6).into(),
            },
            status: ThemeStatus {
                success: rgb(0x22c55e).into(),
                error: rgb(0xef4444).into(),
                warning: rgb(0xf59e0b).into(),
                progress_track: rgba(0xffffff1a).into(),
                bar_gradient_start: rgb(0x6366f1).into(),
                bar_gradient_mid: rgb(0x06b6d4).into(),
            },
            badge: ThemeBadge {
                healthy: rgb(0x22c55e).into(),
                degraded: rgb(0xf59e0b).into(),
                offline: rgb(0xef4444).into(),
                text: rgb(0x0a0a0c).into(),
                synced_bg: rgba(0x22c55e1a).into(),
                syncing_bg: rgba(0x3b82f61a).into(),
            },
            button: ThemeButton {
                danger_bg: rgba(0xef44442e).into(),
                sync_bg: rgb(0x1c1c20).into(),
                sync_text: rgb(0xf4f4f5).into(),
                action_bg: rgb(0x2d6a4f).into(),
                action_text: rgb(0xffffff).into(),
            },
            nav: ThemeNav {
                pill_active_bg: rgb(0x2c2c30).into(),
                pill_active_text: rgb(0xf4f4f5).into(),
            },
            log: ThemeLog {
                error: rgb(0xe74c3c).into(),
                warn: rgb(0xf39c12).into(),
                info: rgb(0x27ae60).into(),
                debug: rgb(0x3498db).into(),
                trace: rgb(0x95a5a6).into(),
            },
        }
    }

    /// 根据 WindowAppearance 和用户主题设置解析为具体 Theme
    ///
    /// 当用户选择 System 时使用 `appearance` 检测深色/浅色；
    /// 用户明确选择 Light/Dark 时忽略 `appearance`。
    pub fn resolve_for_settings(
        user_theme: crate::models::AppTheme,
        appearance: WindowAppearance,
    ) -> Self {
        let resolved = user_theme.resolve(is_dark_appearance(appearance));
        match resolved {
            crate::models::AppTheme::Light => Self::light(),
            crate::models::AppTheme::Dark => Self::dark(),
            crate::models::AppTheme::System => unreachable!("resolve() never returns System"),
        }
    }
}
