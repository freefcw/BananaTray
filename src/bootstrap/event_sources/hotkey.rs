use crate::application::GlobalHotkeyError;
use crate::models::SystemSettings;
use crate::runtime::AppState;
use crate::tray::TrayController;
use gpui::App;
use log::{info, warn};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupHotkeyRegistration {
    Registered {
        persisted: String,
        canonicalized: bool,
    },
    RecoverWithDefault,
    KeepConfiguredError(GlobalHotkeyError),
}

/// 注册全局热键（从 settings 读取，可在运行时重新绑定）。
pub(crate) fn register_global_hotkey(
    state: &Rc<RefCell<AppState>>,
    controller: &Rc<RefCell<TrayController>>,
    cx: &mut App,
) {
    let configured_hotkey = state.borrow().session.settings.system.global_hotkey.clone();

    match classify_startup_hotkey_registration(
        &configured_hotkey,
        crate::runtime::global_hotkey::register_hotkey_string(&configured_hotkey, None, cx),
    ) {
        StartupHotkeyRegistration::Registered {
            persisted,
            canonicalized,
        } => {
            clear_global_hotkey_error(state);

            if canonicalized {
                persist_hotkey_value(state, persisted, "startup canonicalization");
            }
        }
        StartupHotkeyRegistration::RecoverWithDefault => {
            warn!(
                target: "settings",
                "configured global hotkey {} is invalid; falling back to default {}",
                configured_hotkey,
                SystemSettings::DEFAULT_GLOBAL_HOTKEY
            );

            let fallback_hotkey = SystemSettings::DEFAULT_GLOBAL_HOTKEY.to_string();
            persist_hotkey_value(state, fallback_hotkey.clone(), "startup recovery");

            match crate::runtime::global_hotkey::register_hotkey_string(
                SystemSettings::DEFAULT_GLOBAL_HOTKEY,
                None,
                cx,
            ) {
                Ok(_) => {
                    clear_global_hotkey_error(state);
                }
                Err(fallback_err) => {
                    warn!(
                        target: "settings",
                        "failed to register fallback global hotkey {}: {:?}",
                        SystemSettings::DEFAULT_GLOBAL_HOTKEY,
                        fallback_err
                    );
                    set_global_hotkey_error(state, fallback_hotkey, fallback_err);
                }
            }
        }
        StartupHotkeyRegistration::KeepConfiguredError(err) => {
            let error_hotkey = normalize_hotkey_error_candidate(&configured_hotkey)
                .unwrap_or(configured_hotkey.clone());
            warn!(
                target: "settings",
                "failed to register configured global hotkey {}: {:?}; keeping saved value",
                configured_hotkey,
                err
            );
            set_global_hotkey_error(state, error_hotkey, err);
        }
    }

    let async_cx = cx.to_async();
    let ctrl = controller.clone();
    cx.on_global_hotkey(move |id| {
        if id == crate::runtime::global_hotkey::GLOBAL_HOTKEY_ID {
            info!(target: "app", "received global hotkey {}", id);
            let _ = async_cx.update(|cx| {
                ctrl.borrow_mut().toggle_popup(cx);
            });
        }
    });
}

fn classify_startup_hotkey_registration(
    configured_hotkey: &str,
    result: Result<String, GlobalHotkeyError>,
) -> StartupHotkeyRegistration {
    match result {
        Ok(persisted) => StartupHotkeyRegistration::Registered {
            canonicalized: persisted != configured_hotkey,
            persisted,
        },
        Err(err) if err.is_invalid_configuration() => StartupHotkeyRegistration::RecoverWithDefault,
        Err(err) => StartupHotkeyRegistration::KeepConfiguredError(err),
    }
}

fn normalize_hotkey_error_candidate(hotkey: &str) -> Option<String> {
    crate::runtime::global_hotkey::parse_hotkey_string(hotkey)
        .map(|keystroke| crate::runtime::global_hotkey::format_hotkey_for_settings(&keystroke))
        .ok()
}

// BYPASS: bootstrap-only direct state mutation, too simple for full Action-Reducer-Effect cycle.
// If bootstrap state operations grow beyond 5 call sites, migrate to dispatch pipeline.
fn clear_global_hotkey_error(state: &Rc<RefCell<AppState>>) {
    let mut s = state.borrow_mut();
    s.session.settings_ui.clear_global_hotkey_error();
}

// BYPASS: bootstrap-only direct state mutation (see clear_global_hotkey_error).
fn set_global_hotkey_error(
    state: &Rc<RefCell<AppState>>,
    hotkey: String,
    error: GlobalHotkeyError,
) {
    let mut s = state.borrow_mut();
    s.session
        .settings_ui
        .record_global_hotkey_error(hotkey, error);
}

// BYPASS: bootstrap-only direct state mutation (see clear_global_hotkey_error).
fn persist_hotkey_value(state: &Rc<RefCell<AppState>>, hotkey: String, reason: &str) {
    {
        let mut s = state.borrow_mut();
        s.session.settings.system.global_hotkey = hotkey;
    }

    let settings_saved = {
        let s = state.borrow();
        s.settings_writer.flush(s.session.settings.clone())
    };
    if !settings_saved {
        warn!(
            target: "settings",
            "failed to persist global hotkey after {}",
            reason
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registration_keeps_valid_registered_hotkey() {
        assert_eq!(
            classify_startup_hotkey_registration("cmd-shift-s", Ok("cmd-shift-s".to_string())),
            StartupHotkeyRegistration::Registered {
                persisted: "cmd-shift-s".to_string(),
                canonicalized: false,
            }
        );
    }

    #[test]
    fn startup_registration_marks_legacy_display_format_for_canonicalization() {
        assert_eq!(
            classify_startup_hotkey_registration("Cmd+S", Ok("cmd-s".to_string())),
            StartupHotkeyRegistration::Registered {
                persisted: "cmd-s".to_string(),
                canonicalized: true,
            }
        );
    }

    #[test]
    fn startup_registration_recovers_only_for_invalid_configuration() {
        assert_eq!(
            classify_startup_hotkey_registration(
                "bad-hotkey",
                Err(GlobalHotkeyError::InvalidFormat)
            ),
            StartupHotkeyRegistration::RecoverWithDefault
        );
    }

    #[test]
    fn startup_registration_preserves_saved_hotkey_on_transient_failure() {
        let conflict = GlobalHotkeyError::Conflict("already in use".to_string());

        assert_eq!(
            classify_startup_hotkey_registration("cmd-s", Err(conflict.clone())),
            StartupHotkeyRegistration::KeepConfiguredError(conflict)
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn startup_error_candidate_normalizes_legacy_display_format() {
        assert_eq!(
            normalize_hotkey_error_candidate("Cmd+S"),
            Some("cmd-s".to_string())
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn startup_error_candidate_normalizes_legacy_display_format_linux() {
        // On Linux the underlying GPUI representation may canonicalize the modifier
        // as `super` rather than `cmd`. Accept the platform-specific output here.
        let got = normalize_hotkey_error_candidate("Cmd+S");
        assert!(got == Some("cmd-s".to_string()) || got == Some("super-s".to_string()));
    }
}
