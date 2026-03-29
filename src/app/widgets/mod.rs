mod cadence_dropdown;
mod card;
mod checkbox;
mod icon;
mod quota_bar;
mod tab;
mod toggle;

pub(crate) use cadence_dropdown::render_cadence_dropdown;
pub(crate) use card::{
    render_card, render_card_separator, render_checkbox_row, render_detail_section_title,
    render_info_row, render_section_label,
};
pub(crate) use checkbox::render_checkbox;
pub(crate) use icon::{render_footer_glyph, render_svg_icon};
pub(crate) use quota_bar::render_quota_bar;
pub(crate) use tab::render_icon_tab;
pub(crate) use toggle::render_toggle_switch;

use super::AppView;
use crate::theme::Theme;
use gpui::*;

impl AppView {
    pub(crate) fn render_toggle_switch_small(&self, enabled: bool, theme: &Theme) -> Div {
        render_toggle_switch(enabled, px(36.0), px(20.0), px(14.0), theme)
    }
}
