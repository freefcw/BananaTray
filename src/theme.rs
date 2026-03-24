use gpui::*;

/// 全局主题定义
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_subtle: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_accent: Hsla,
    pub border_subtle: Hsla,
    pub element_active: Hsla,
    pub element_inactive: Hsla,
    pub element_selected: Hsla,
    pub status_success: Hsla,
    pub status_error: Hsla,
}

impl Global for Theme {}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg_base: rgb(0xffffff).into(),
            bg_panel: rgb(0xf9f9f9).into(),
            bg_subtle: rgb(0xf3f4f6).into(), // Gray 100
            text_primary: rgb(0x0a0a0a).into(),
            text_secondary: rgb(0x737373).into(),
            text_accent: rgb(0x2563eb).into(), // Blue 600
            border_subtle: rgb(0xe5e5e5).into(),
            element_active: rgb(0x000000).into(),
            element_inactive: rgb(0x737373).into(),
            element_selected: rgb(0xe5e7eb).into(), // Gray 200
            status_success: rgb(0x10b981).into(), // Green 500
            status_error: rgb(0xef4444).into(), // Red 500
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0a0a0a).into(),
            bg_panel: rgb(0x1a1a1a).into(),
            bg_subtle: rgb(0x171717).into(), // Neutral 850
            text_primary: rgb(0xfafafa).into(),
            text_secondary: rgb(0xa3a3a3).into(),
            text_accent: rgb(0x3b82f6).into(), // Blue 500
            border_subtle: rgb(0x262626).into(),
            element_active: rgb(0xffffff).into(),
            element_inactive: rgb(0xa3a3a3).into(),
            element_selected: rgb(0x262626).into(), // Neutral 800
            status_success: rgb(0x10b981).into(),
            status_error: rgb(0xef4444).into(),
        }
    }
}
