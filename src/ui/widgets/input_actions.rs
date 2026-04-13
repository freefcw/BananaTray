use adabraka_ui::components::input_state::InputState;
use gpui::{Div, Entity, InteractiveElement, Stateful, Window};

/// 注册所有键盘事件处理器到 InputState entity
///
/// GPUI 的 InputState 需要手动注册键盘 action（backspace/delete/方向键等），
/// 此函数将所有标准键盘操作统一注册到一个 Stateful<Div> 上。
///
/// 被 Copilot Token 输入框和 NewAPI 表单共用。
pub(crate) fn register_input_actions(
    div: Stateful<Div>,
    input_entity: &Entity<InputState>,
    window: &mut Window,
) -> Stateful<Div> {
    div.on_action(window.listener_for(input_entity, InputState::backspace))
        .on_action(window.listener_for(input_entity, InputState::delete))
        .on_action(window.listener_for(input_entity, InputState::left))
        .on_action(window.listener_for(input_entity, InputState::right))
        .on_action(window.listener_for(input_entity, InputState::select_left))
        .on_action(window.listener_for(input_entity, InputState::select_right))
        .on_action(window.listener_for(input_entity, InputState::select_all))
        .on_action(window.listener_for(input_entity, InputState::home))
        .on_action(window.listener_for(input_entity, InputState::end))
        .on_action(window.listener_for(input_entity, InputState::copy))
        .on_action(window.listener_for(input_entity, InputState::cut))
        .on_action(window.listener_for(input_entity, InputState::paste))
        .on_action(window.listener_for(input_entity, InputState::word_left))
        .on_action(window.listener_for(input_entity, InputState::word_right))
        .on_action(window.listener_for(input_entity, InputState::select_word_left))
        .on_action(window.listener_for(input_entity, InputState::select_word_right))
}
