use std::cell::RefCell;
use std::rc::Rc;

use crate::application::CommonEffect;

use super::AppState;

mod debug;
mod newapi;
mod notification;
mod refresh;
mod settings;

pub(super) fn run_common_effect(state: &Rc<RefCell<AppState>>, effect: CommonEffect) {
    match effect {
        CommonEffect::Settings(effect) => settings::run(state, effect),
        CommonEffect::Notification(effect) => notification::run(effect),
        CommonEffect::Refresh(effect) => refresh::run(state, effect),
        CommonEffect::Debug(effect) => debug::run(state, effect),
        CommonEffect::NewApi(effect) => newapi::run(state, effect),
    }
}
