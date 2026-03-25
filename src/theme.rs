use gpui::*;

/// 全局主题定义
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_subtle: Hsla,
    pub bg_card: Hsla,
    pub bg_card_active: Hsla,
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
}

impl Global for Theme {}

impl Theme {
    pub fn light() -> Self {
        Self {
            bg_base: rgb(0xf6f1df).into(),
            bg_panel: rgb(0xfffcf2).into(),
            bg_subtle: rgb(0xf0e7c9).into(),
            bg_card: rgb(0xfff9e8).into(),
            bg_card_active: rgb(0x2d2416).into(),
            text_primary: rgb(0x211a10).into(),
            text_secondary: rgb(0x64553e).into(),
            text_muted: rgb(0x9f8e72).into(),
            text_accent: rgb(0xb87910).into(),
            text_accent_soft: rgb(0x5a4522).into(),
            border_subtle: rgb(0xe6d6ab).into(),
            border_strong: rgb(0xd0bb83).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0xb87910).into(),
            status_success: rgb(0x3d7a3f).into(),
            status_error: rgb(0xbf513b).into(),
            status_warning: rgb(0xcf8a12).into(),
            progress_track: rgb(0xd6c48f).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0f0d09).into(),
            bg_panel: rgb(0x17130e).into(),
            bg_subtle: rgb(0x241d13).into(),
            bg_card: rgb(0x1d1710).into(),
            bg_card_active: rgb(0x2b2113).into(),
            text_primary: rgb(0xfff6dc).into(),
            text_secondary: rgb(0xd8c39a).into(),
            text_muted: rgb(0x9f8a63).into(),
            text_accent: rgb(0xffc44d).into(),
            text_accent_soft: rgb(0x5c4317).into(),
            border_subtle: rgb(0x3a2b17).into(),
            border_strong: rgb(0x57401f).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0xb87910).into(),
            status_success: rgb(0x6bd06c).into(),
            status_error: rgb(0xff8b72).into(),
            status_warning: rgb(0xffc44d).into(),
            progress_track: rgb(0x3a2c18).into(),
        }
    }
}
