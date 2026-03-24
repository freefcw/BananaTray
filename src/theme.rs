use gpui::*;

/// 全局主题定义
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub border_subtle: Hsla,
    pub element_active: Hsla,
    pub element_inactive: Hsla,
}

impl Global for Theme {}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg_base: rgb(0xffffff).into(),
            bg_panel: rgb(0xf9f9f9).into(), // neutral-50
            text_primary: rgb(0x0a0a0a).into(),
            text_secondary: rgb(0x737373).into(),
            border_subtle: rgb(0xe5e5e5).into(),
            element_active: rgb(0x000000).into(),
            element_inactive: rgb(0x737373).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0a0a0a).into(),
            bg_panel: rgb(0x1a1a1a).into(), // neutral-900
            text_primary: rgb(0xfafafa).into(),
            text_secondary: rgb(0xa3a3a3).into(),
            border_subtle: rgb(0x262626).into(),
            element_active: rgb(0xffffff).into(),
            element_inactive: rgb(0xa3a3a3).into(),
        }
    }
}
