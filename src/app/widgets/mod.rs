mod action_button;
mod cadence_dropdown;
mod card;
mod checkbox;
mod colored_icon;
mod global_actions;
mod icon;
mod icon_row;
mod info_row;
mod input_actions;
mod provider_icon;
mod quota_bar;
mod segmented_control;
mod simple_input;
mod tab;
mod toggle;
mod tooltip;

pub(crate) use action_button::{render_action_button, ButtonVariant};
pub(crate) use cadence_dropdown::render_cadence_trigger;
pub(crate) use card::render_detail_section_title;
pub(crate) use checkbox::render_checkbox;
#[allow(unused_imports)]
pub(crate) use colored_icon::{render_colored_icon, render_colored_icon_sized};
pub(crate) use icon::{render_footer_glyph, render_svg_icon};
pub(crate) use icon_row::render_icon_row;
pub(crate) use info_row::{render_info_cell, render_kv_info_row};
pub(crate) use input_actions::register_input_actions;
pub(crate) use provider_icon::{render_provider_icon, render_provider_icon_boxed};
pub(crate) use quota_bar::render_quota_bar;
pub(crate) use segmented_control::{render_segmented_control, SegmentedSize};
pub(crate) use simple_input::{render_simple_input, render_simple_textarea, SimpleInputState};
pub(crate) use toggle::render_toggle_switch;
#[allow(unused_imports)]
pub(crate) use tooltip::{with_multiline_tooltip, with_tooltip};

use super::AppView;
use crate::theme::Theme;
use gpui::*;

impl AppView {
    pub(crate) fn render_toggle_switch_small(&self, enabled: bool, theme: &Theme) -> Div {
        render_toggle_switch(enabled, px(36.0), px(20.0), px(14.0), theme)
    }
}
