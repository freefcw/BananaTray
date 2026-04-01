#[allow(dead_code)]
mod cadence_dropdown;
mod card;
mod checkbox;
mod icon;
mod quota_bar;
mod tab;
mod toggle;
mod tooltip;

pub(crate) use card::{render_card, render_detail_section_title};
pub(crate) use checkbox::render_checkbox;
pub(crate) use icon::{render_footer_glyph, render_svg_icon};
pub(crate) use quota_bar::render_quota_bar;
pub(crate) use toggle::render_toggle_switch;
#[allow(unused_imports)]
pub(crate) use tooltip::with_tooltip;

use super::AppView;
use crate::theme::Theme;
use gpui::*;

impl AppView {
    pub(crate) fn render_toggle_switch_small(&self, enabled: bool, theme: &Theme) -> Div {
        render_toggle_switch(enabled, px(36.0), px(20.0), px(14.0), theme)
    }
}
