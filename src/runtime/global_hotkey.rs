use crate::application::GlobalHotkeyError;
use gpui::{App, Keystroke, Modifiers};
use log::{info, warn};
use std::cell::RefCell;
use std::rc::Rc;

use super::AppState;

pub(crate) const GLOBAL_HOTKEY_ID: u32 = 1;
const HOTKEY_PROBE_ID: u32 = 2;

#[derive(Debug)]
struct ParsedGlobalHotkey {
    persisted: String,
    keystroke: Keystroke,
}

/// 解析并验证热键字符串。
///
/// 兼容两类输入：
/// - GPUI 可直接解析的持久化格式，如 `cmd-shift-s`
/// - 旧版/用户手填的展示格式，如 `Cmd+Shift+S`
pub(crate) fn parse_hotkey_string(input: &str) -> Result<Keystroke, GlobalHotkeyError> {
    let compact: String = input.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(GlobalHotkeyError::Empty);
    }

    let looks_like_display =
        compact.contains('+') || compact.chars().any(|ch| ch.is_ascii_uppercase());

    let keystroke = if looks_like_display {
        let delimiter = if compact.contains('+') { '+' } else { '-' };
        parse_display_hotkey(&compact, delimiter)?
    } else {
        Keystroke::parse(&compact).map_err(|_| GlobalHotkeyError::InvalidFormat)?
    };

    validate_hotkey(&keystroke)?;
    Ok(keystroke)
}

/// 生成可安全回读的持久化格式，避免单字符 key 被误解析成额外的 `Shift`。
pub(crate) fn format_hotkey_for_settings(keystroke: &Keystroke) -> String {
    keystroke.unparse()
}

/// 解析用户请求，并在真正替换当前热键前先做一次 probe 检查。
///
/// - probe 阶段失败：保留现有热键不动，返回 `Conflict`
/// - 正式替换失败：尽力恢复旧热键，返回 `RegistrationFailed`
pub(crate) fn register_hotkey_string(
    requested_hotkey: &str,
    previous_hotkey: Option<&str>,
    cx: &mut App,
) -> Result<String, GlobalHotkeyError> {
    let parsed = parse_hotkey_input(requested_hotkey)?;
    let requested_matches_previous = previous_hotkey
        .map(|previous| hotkeys_match(&parsed.keystroke, previous))
        .unwrap_or(false);

    if !requested_matches_previous {
        probe_hotkey_registration(&parsed.keystroke, cx)?;
    }

    cx.unregister_global_hotkey(GLOBAL_HOTKEY_ID);
    match cx.register_global_hotkey(GLOBAL_HOTKEY_ID, &parsed.keystroke) {
        Ok(()) => {
            info!(
                target: "settings",
                "registered global hotkey {}",
                parsed.persisted
            );
            Ok(parsed.persisted)
        }
        Err(err) => {
            if let Some(previous) = previous_hotkey {
                restore_previous_hotkey(previous, cx);
            }
            Err(GlobalHotkeyError::RegistrationFailed(err.to_string()))
        }
    }
}

/// 处理设置页触发的热键变更。
pub(crate) fn rebind_global_hotkey(
    state: &Rc<RefCell<AppState>>,
    requested_hotkey: &str,
    cx: &mut App,
) {
    let previous_hotkey = state.borrow().session.settings.system.global_hotkey.clone();

    match register_hotkey_string(requested_hotkey, Some(&previous_hotkey), cx) {
        Ok(persisted) => {
            {
                let mut s = state.borrow_mut();
                s.session.settings.system.global_hotkey = persisted;
                s.session.settings_ui.global_hotkey_error = None;
                s.session.settings_ui.global_hotkey_error_candidate = None;
            }

            let settings_saved = {
                let s = state.borrow();
                s.settings_writer.flush(s.session.settings.clone())
            };
            if !settings_saved {
                warn!(
                    target: "settings",
                    "global hotkey updated in memory but failed to persist settings"
                );
            }
        }
        Err(error) => {
            warn!(target: "settings", "failed to update global hotkey: {:?}", error);
            let mut s = state.borrow_mut();
            s.session.settings_ui.global_hotkey_error = Some(error);
            s.session.settings_ui.global_hotkey_error_candidate =
                Some(requested_hotkey.to_string());
        }
    }
}

fn parse_hotkey_input(input: &str) -> Result<ParsedGlobalHotkey, GlobalHotkeyError> {
    let keystroke = parse_hotkey_string(input)?;
    Ok(ParsedGlobalHotkey {
        persisted: format_hotkey_for_settings(&keystroke),
        keystroke,
    })
}

fn parse_display_hotkey(source: &str, delimiter: char) -> Result<Keystroke, GlobalHotkeyError> {
    let mut modifiers = Modifiers::default();
    let mut key = None;

    for component in source.split(delimiter) {
        if component.is_empty() {
            return Err(GlobalHotkeyError::InvalidFormat);
        }

        match component.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.control = true,
            "alt" | "option" => modifiers.alt = true,
            "shift" => modifiers.shift = true,
            "fn" | "function" => modifiers.function = true,
            "secondary" => {
                if cfg!(target_os = "macos") {
                    modifiers.platform = true;
                } else {
                    modifiers.control = true;
                }
            }
            "cmd" | "command" | "platform" | "super" | "win" => {
                modifiers.platform = true;
            }
            _ => {
                if key.is_some() {
                    return Err(GlobalHotkeyError::InvalidFormat);
                }
                key = Some(normalize_display_key(component)?);
            }
        }
    }

    let key = key.ok_or(GlobalHotkeyError::InvalidFormat)?;
    Ok(Keystroke {
        modifiers,
        key,
        key_char: None,
    })
}

fn normalize_display_key(component: &str) -> Result<String, GlobalHotkeyError> {
    let lower = component.to_ascii_lowercase();
    let normalized = match lower.as_str() {
        "space" => "space".to_string(),
        "enter" | "return" => "enter".to_string(),
        "esc" | "escape" => "escape".to_string(),
        "tab" => "tab".to_string(),
        "backspace" => "backspace".to_string(),
        "delete" | "del" => "delete".to_string(),
        "insert" | "ins" => "insert".to_string(),
        "home" => "home".to_string(),
        "end" => "end".to_string(),
        "pageup" | "pgup" => "pageup".to_string(),
        "pagedown" | "pgdn" => "pagedown".to_string(),
        "left" => "left".to_string(),
        "right" => "right".to_string(),
        "up" => "up".to_string(),
        "down" => "down".to_string(),
        _ if is_function_key(&lower) => lower,
        _ if component.chars().count() == 1 => lower,
        _ => return Err(GlobalHotkeyError::InvalidFormat),
    };

    Ok(normalized)
}

fn is_function_key(component: &str) -> bool {
    component.starts_with('f')
        && component.len() > 1
        && component[1..].chars().all(|ch| ch.is_ascii_digit())
}

fn validate_hotkey(keystroke: &Keystroke) -> Result<(), GlobalHotkeyError> {
    if matches!(
        keystroke.key.as_str(),
        "shift" | "control" | "alt" | "platform" | "function"
    ) {
        return Err(GlobalHotkeyError::ModifierOnly);
    }

    if !keystroke.modifiers.modified() {
        return Err(GlobalHotkeyError::MissingModifier);
    }

    Ok(())
}

fn hotkeys_match(requested: &Keystroke, previous_hotkey: &str) -> bool {
    parse_hotkey_string(previous_hotkey)
        .map(|previous| previous == *requested)
        .unwrap_or(false)
}

fn probe_hotkey_registration(keystroke: &Keystroke, cx: &mut App) -> Result<(), GlobalHotkeyError> {
    cx.register_global_hotkey(HOTKEY_PROBE_ID, keystroke)
        .map_err(|err| GlobalHotkeyError::Conflict(err.to_string()))?;
    cx.unregister_global_hotkey(HOTKEY_PROBE_ID);
    Ok(())
}

fn restore_previous_hotkey(previous_hotkey: &str, cx: &mut App) {
    match parse_hotkey_input(previous_hotkey) {
        Ok(previous) => {
            if let Err(err) = cx.register_global_hotkey(GLOBAL_HOTKEY_ID, &previous.keystroke) {
                warn!(
                    target: "settings",
                    "failed to restore previous global hotkey {}: {}",
                    previous.persisted,
                    err
                );
            }
        }
        Err(err) => {
            warn!(
                target: "settings",
                "failed to parse previous global hotkey during rollback: {:?}",
                err
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hotkey_accepts_plus_separated_input() {
        let parsed = parse_hotkey_input(" Cmd + Shift + s ").unwrap();
        let expected = Keystroke::parse("cmd-shift-s").unwrap().unparse();

        assert_eq!(parsed.persisted, expected);
        assert_eq!(parsed.keystroke.key, "s");
        assert!(parsed.keystroke.modifiers.platform);
        assert!(parsed.keystroke.modifiers.shift);
    }

    #[test]
    fn parse_hotkey_preserves_plain_letter_without_implicit_shift() {
        let keystroke = parse_hotkey_string("Cmd+S").unwrap();
        let expected = Keystroke::parse("cmd-s").unwrap().unparse();

        assert_eq!(keystroke.key, "s");
        assert!(keystroke.modifiers.platform);
        assert!(!keystroke.modifiers.shift);
        assert_eq!(format_hotkey_for_settings(&keystroke), expected);
    }

    #[test]
    fn parse_hotkey_accepts_legacy_hyphen_display_input() {
        let keystroke = parse_hotkey_string("Cmd-S").unwrap();

        assert_eq!(keystroke.key, "s");
        assert!(keystroke.modifiers.platform);
        assert!(!keystroke.modifiers.shift);
    }

    #[test]
    fn parse_hotkey_rejects_empty_input() {
        assert_eq!(
            parse_hotkey_input("   ").unwrap_err(),
            GlobalHotkeyError::Empty
        );
    }

    #[test]
    fn parse_hotkey_rejects_missing_modifier() {
        assert_eq!(
            parse_hotkey_input("s").unwrap_err(),
            GlobalHotkeyError::MissingModifier
        );
    }

    #[test]
    fn parse_hotkey_rejects_modifier_only_binding() {
        assert_eq!(
            parse_hotkey_input("cmd").unwrap_err(),
            GlobalHotkeyError::ModifierOnly
        );
    }

    #[test]
    fn parse_hotkey_rejects_invalid_sequence() {
        assert_eq!(
            parse_hotkey_input("cmd+shift+s+t").unwrap_err(),
            GlobalHotkeyError::InvalidFormat
        );
    }
}
