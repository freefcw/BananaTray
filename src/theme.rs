use gpui::*;
use std::sync::LazyLock;

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

// ── YAML 主题加载 ────────────────────────────────────────
//
// 使用 serde_yaml::Value 动态提取颜色值，无需 #[derive(Deserialize)]
// 中间结构体，避免与 GPUI proc-macro 冲突，也消除了结构体重复。

pub(crate) const LIGHT_YAML: &str = include_str!("../themes/light.yaml");
const DARK_YAML: &str = include_str!("../themes/dark.yaml");

/// 从 YAML Value 中按 `section.key` 路径提取颜色并转为 Hsla
pub(crate) fn color(root: &serde_yaml::Value, section: &str, key: &str) -> Hsla {
    let s = root[section][key]
        .as_str()
        .unwrap_or_else(|| panic!("missing theme color: {section}.{key}"));
    parse_color(s)
}

/// 解析 `#RRGGBB` 或 `#RRGGBBAA` 颜色字符串为 GPUI Hsla
///
/// 无效输入时 panic（主题文件是编译时嵌入的静态配置）。
pub(crate) fn parse_color(s: &str) -> Hsla {
    let hex = s.strip_prefix('#').unwrap_or(s);
    match hex.len() {
        6 => {
            let val = u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("invalid color: {s}"));
            rgb(val).into()
        }
        8 => {
            let val = u32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("invalid color: {s}"));
            rgba(val).into()
        }
        _ => panic!("invalid color format (expected #RRGGBB or #RRGGBBAA): {s}"),
    }
}

/// 将 YAML 字符串解析为完整 Theme
fn load_theme(yaml: &str) -> Theme {
    let v: serde_yaml::Value = serde_yaml::from_str(yaml).expect("failed to parse theme YAML file");

    Theme {
        bg: ThemeBg {
            base: color(&v, "bg", "base"),
            panel: color(&v, "bg", "panel"),
            subtle: color(&v, "bg", "subtle"),
            card: color(&v, "bg", "card"),
            card_inner: color(&v, "bg", "card_inner"),
            card_inner_hovered: color(&v, "bg", "card_inner_hovered"),
        },
        text: ThemeText {
            primary: color(&v, "text", "primary"),
            secondary: color(&v, "text", "secondary"),
            muted: color(&v, "text", "muted"),
            accent: color(&v, "text", "accent"),
            accent_soft: color(&v, "text", "accent_soft"),
        },
        border: ThemeBorder {
            subtle: color(&v, "border", "subtle"),
            strong: color(&v, "border", "strong"),
        },
        element: ThemeElement {
            active: color(&v, "element", "active"),
            selected: color(&v, "element", "selected"),
        },
        status: ThemeStatus {
            success: color(&v, "status", "success"),
            error: color(&v, "status", "error"),
            warning: color(&v, "status", "warning"),
            progress_track: color(&v, "status", "progress_track"),
            bar_gradient_start: color(&v, "status", "bar_gradient_start"),
            bar_gradient_mid: color(&v, "status", "bar_gradient_mid"),
        },
        badge: ThemeBadge {
            healthy: color(&v, "badge", "healthy"),
            degraded: color(&v, "badge", "degraded"),
            offline: color(&v, "badge", "offline"),
            text: color(&v, "badge", "text"),
            synced_bg: color(&v, "badge", "synced_bg"),
            syncing_bg: color(&v, "badge", "syncing_bg"),
        },
        button: ThemeButton {
            danger_bg: color(&v, "button", "danger_bg"),
            sync_bg: color(&v, "button", "sync_bg"),
            sync_text: color(&v, "button", "sync_text"),
            action_bg: color(&v, "button", "action_bg"),
            action_text: color(&v, "button", "action_text"),
        },
        nav: ThemeNav {
            pill_active_bg: color(&v, "nav", "pill_active_bg"),
            pill_active_text: color(&v, "nav", "pill_active_text"),
        },
        log: ThemeLog {
            error: color(&v, "log", "error"),
            warn: color(&v, "log", "warn"),
            info: color(&v, "log", "info"),
            debug: color(&v, "log", "debug"),
            trace: color(&v, "log", "trace"),
        },
    }
}

// ── LazyLock 缓存 ────────────────────────────────────────

static LIGHT_THEME: LazyLock<Theme> = LazyLock::new(|| load_theme(LIGHT_YAML));
static DARK_THEME: LazyLock<Theme> = LazyLock::new(|| load_theme(DARK_YAML));

// ── 公开 API（签名不变）──────────────────────────────────

impl Theme {
    pub fn light() -> Self {
        LIGHT_THEME.clone()
    }

    pub fn dark() -> Self {
        DARK_THEME.clone()
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
