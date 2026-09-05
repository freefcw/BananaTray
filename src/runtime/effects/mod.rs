use std::cell::RefCell;
use std::rc::Rc;

use crate::application::{AppAction, CommonEffect};

use super::AppState;

mod debug;
pub(super) mod newapi;
mod notification;
mod refresh;
pub(super) mod script_provider;
mod settings;

pub(super) fn run_common_effect(
    state: &Rc<RefCell<AppState>>,
    effect: CommonEffect,
) -> Vec<AppAction> {
    match effect {
        CommonEffect::Settings(effect) => {
            settings::run(state, effect);
            Vec::new()
        }
        CommonEffect::Notification(effect) => {
            notification::run(effect);
            Vec::new()
        }
        CommonEffect::Refresh(effect) => refresh::run(state, effect),
        CommonEffect::Debug(effect) => debug::run(state, effect),
        CommonEffect::NewApi(effect) => newapi::run(state, effect),
        CommonEffect::ScriptProvider(effect) => script_provider::run(state, effect),
    }
}
