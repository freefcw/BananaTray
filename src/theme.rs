use gpui::*;

#[derive(Clone)]
pub struct Theme {
    pub bg_base: Hsla,
    pub bg_panel: Hsla,
    pub bg_subtle: Hsla,
    pub bg_card: Hsla,
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
            bg_base: rgb(0xffffff).into(),
            bg_panel: rgb(0xffffff).into(),
            bg_subtle: rgb(0xf4f4f5).into(),
            bg_card: rgb(0xf4f4f5).into(),
            text_primary: rgb(0x18181b).into(),
            text_secondary: rgb(0x71717a).into(),
            text_muted: rgb(0xa1a1aa).into(),
            text_accent: rgb(0x2563eb).into(),
            text_accent_soft: rgb(0xdbeafe).into(),
            border_subtle: rgb(0xe4e4e7).into(),
            border_strong: rgb(0xd4d4d8).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x2563eb).into(),
            status_success: rgb(0x22c55e).into(),
            status_error: rgb(0xef4444).into(),
            status_warning: rgb(0xf59e0b).into(),
            progress_track: rgba(0xffffff4c).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_base: rgb(0x0f0f11).into(),
            bg_panel: rgb(0x18181b).into(),
            bg_subtle: rgb(0x27272a).into(),
            bg_card: rgb(0x27272a).into(),
            text_primary: rgb(0xf4f4f5).into(),
            text_secondary: rgb(0xa1a1aa).into(),
            text_muted: rgb(0x71717a).into(),
            text_accent: rgb(0x3b82f6).into(),
            text_accent_soft: rgb(0x1e3a8a).into(),
            border_subtle: rgb(0x3f3f46).into(),
            border_strong: rgb(0x52525b).into(),
            element_active: rgb(0xffffff).into(),
            element_selected: rgb(0x3b82f6).into(),
            status_success: rgb(0x22c55e).into(),
            status_error: rgb(0xef4444).into(),
            status_warning: rgb(0xf59e0b).into(),
            progress_track: rgba(0xffffff33).into(),
        }
    }
}
