use super::*;
use crate::models::AppSettings;
use crate::providers::ProviderManager;

#[derive(Default)]
struct FakeCaps {
    rendered: bool,
}

impl ContextCapabilities for FakeCaps {
    fn render(&mut self, _state: &Rc<RefCell<AppState>>) {
        self.rendered = true;
    }
}

fn make_state() -> Rc<RefCell<AppState>> {
    let (tx, _rx) = smol::channel::bounded(1);
    let manager = std::sync::Arc::new(ProviderManager::new());
    Rc::new(RefCell::new(AppState::new(
        tx,
        manager,
        AppSettings::default(),
        None,
    )))
}

#[test]
fn run_context_effect_routes_render_to_capability() {
    let state = make_state();
    let mut caps = FakeCaps::default();

    run_context_effect(&state, ContextEffect::Render, &mut caps);

    assert!(caps.rendered);
}
