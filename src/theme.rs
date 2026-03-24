use gpui::*;

/// 全局主题定义
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_subtle: Hsla,
    pub bg_card: Hsla,
    pub bg_card_active: Hsla,
    pub bg_hover: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_accent: Hsla,
    pub text_accent_soft: Hsla,
    pub border_subtle: Hsla,
    pub border_strong: Hsla,
    pub element_active: Hsla,
    pub element_inactive: Hsla,
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
            bg_base: rgb(0xf4f7fb).into(),
            bg_panel: rgb(0xffffff).into(),
            bg_subtle: rgb(0xeef2f7).into(),
            bg_card: rgb(0xffffff).into(),
            bg_card_active: rgb(0x2e5cff).into(),
            bg_hover: rgb(0xe8eefb).into(),
            text_primary: rgb(0x0f172a).into(),
            text_secondary: rgb(0x475569).into(),
            text_muted: rgb(0x94a3b8).into(),
            text_accent: rgb(0x2e5cff).into(),
            text_accent_soft: rgb(0xdbe6ff).into(),
            border_subtle: rgb(0xe2e8f0).into(),
            border_strong: rgb(0xcbd5e1).into(),
            element_active: rgb(0xffffff).into(),
            element_inactive: rgb(0x64748b).into(),
            element_selected: rgb(0x2e5cff).into(),
            status_success: rgb(0x16a34a).into(),
            status_error: rgb(0xdc2626).into(),
            status_warning: rgb(0xea580c).into(),
            progress_track: rgb(0xdbe4f4).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0c1220).into(),
            bg_panel: rgb(0x121a2a).into(),
            bg_subtle: rgb(0x192235).into(),
            bg_card: rgb(0x101827).into(),
            bg_card_active: rgb(0x2f5bff).into(),
            bg_hover: rgb(0x1d2940).into(),
            text_primary: rgb(0xf8fafc).into(),
            text_secondary: rgb(0xcbd5e1).into(),
            text_muted: rgb(0x7f8ba3).into(),
            text_accent: rgb(0x79a8ff).into(),
            text_accent_soft: rgb(0x223a75).into(),
            border_subtle: rgb(0x22304a).into(),
            border_strong: rgb(0x31415d).into(),
            element_active: rgb(0xffffff).into(),
            element_inactive: rgb(0xa6b1c5).into(),
            element_selected: rgb(0x2f5bff).into(),
            status_success: rgb(0x34d399).into(),
            status_error: rgb(0xff6b6b).into(),
            status_warning: rgb(0xf59e0b).into(),
            progress_track: rgb(0x24324b).into(),
        }
    }
}
