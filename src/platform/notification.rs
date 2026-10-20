//! Platform system notification delivery.
//!
//! # Platform notification architecture
//!
//! | Platform | Bundled (.app) | Development (cargo run) |
//! |----------|----------------|-------------------------|
//! | macOS    | `UNUserNotificationCenter` (native) | `osascript` (AppleScript) |
//! | Linux    | `notify-rust` (D-Bus) | `notify-rust` (D-Bus) |
//!
//! `QuotaNotificationEvent` / `AlertEngine` live in `application/`.
//! This module only converts domain alerts into OS-specific notifications.
//!
//! ## Why `notify-rust` is excluded on macOS
//!
//! `notify-rust` depends on `mac-notification-sys`, which contains ObjC code that:
//! 1. Calls `LSCopyApplicationURLsForBundleIdentifier()` to resolve bundle IDs
//! 2. Executes AppleScript `get id of application "..."` for app lookup
//! 3. Swizzles `NSBundle.bundleIdentifier` via `method_exchangeImplementations`
//!
//! These operations trigger macOS Launch Services to scan **all** registered app
//! locations, including network volumes (NFS/SMB). If the system has configured
//! network shares, this causes the TCC dialog:
//! **"BananaTray wants to access files on a network volume"**.
//!
//! Since macOS uses its own native notification path (`UNUserNotificationCenter`
//! + `osascript` fallback), `notify-rust` is unnecessary and is excluded via
//!   `cfg(not(target_os = "macos"))` in both `Cargo.toml` and this module.

use crate::application::{format_quota_label, format_quota_remaining_text, QuotaNotificationEvent};
use log::{info, warn};
use rust_i18n::t;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 外部通知调用的上限（macOS `osascript`）。
const NOTIFICATION_CALL_TIMEOUT: Duration = Duration::from_secs(5);
const NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(20);
const NOTIFICATION_QUEUE_CAPACITY: usize = 32;
// ============================================================================
// 系统通知发送
// ============================================================================

/// 发送系统通知
///
/// 在独立线程中发送通知，避免阻塞 GPUI 事件循环。
///
/// - **macOS (App Bundle)**: 通过 `UNUserNotificationCenter` 发送原生通知，
///   支持应用图标显示和系统通知中心管理。
/// - **macOS (cargo run)**: 通过 `osascript`（AppleScript）发送通知作为开发模式 fallback。
/// - **其他平台**: 使用 `notify-rust`（Linux D-Bus / Windows Toast）。
pub fn send_system_notification(alert: &QuotaNotificationEvent, with_sound: bool) {
    let (title, body) = quota_alert_notification(alert);
    spawn_notification(title, body, with_sound);
}

fn quota_alert_notification(alert: &QuotaNotificationEvent) -> (String, String) {
    match alert {
        QuotaNotificationEvent::LowQuota {
            provider_name,
            quota,
        } => (
            t!("notification.low_quota.title", name = provider_name).to_string(),
            t!(
                "notification.low_quota.body",
                label = format_quota_label(quota),
                remaining = format_quota_remaining_text(quota)
            )
            .to_string(),
        ),
        QuotaNotificationEvent::Exhausted {
            provider_name,
            quota,
        } => (
            t!("notification.exhausted.title", name = provider_name).to_string(),
            t!(
                "notification.exhausted.body",
                label = format_quota_label(quota)
            )
            .to_string(),
        ),
        QuotaNotificationEvent::Recovered {
            provider_name,
            quota,
        } => (
            t!("notification.recovered.title", name = provider_name).to_string(),
            t!(
                "notification.recovered.body",
                label = format_quota_label(quota),
                remaining = format_quota_remaining_text(quota)
            )
            .to_string(),
        ),
        QuotaNotificationEvent::UsageProgress {
            provider_name,
            remaining_pct,
        } => (
            t!("notification.usage_progress.title", name = provider_name).to_string(),
            t!(
                "notification.usage_progress.body",
                pct = format!("{:.0}", remaining_pct)
            )
            .to_string(),
        ),
    }
}

/// 发送简单的系统通知（无声音）。
///
/// 在独立线程中发送通知，供不需要 QuotaNotificationEvent 包装的场景使用
/// （如 auto-launch 通知）。
pub fn send_plain_notification(title: &str, body: &str) {
    spawn_notification(title.to_string(), body.to_string(), false);
}

fn spawn_notification(title: String, body: String, with_sound: bool) {
    // 串行化到单条工作线程：通知服务挂住时最多留一条线程，而不是按通知次数
    // 每次 spawn 一条。工作线程在首次通知时惰性启动。
    static SENDER: OnceLock<Mutex<SyncSender<(String, String, bool)>>> = OnceLock::new();

    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<(String, String, bool)>(NOTIFICATION_QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("notification-sender".to_string())
            .spawn(move || {
                info!(target: "notification", "notification worker started");
                while let Ok((title, body, with_sound)) = rx.recv() {
                    platform_send_notification(&title, &body, with_sound);
                }
                info!(target: "notification", "notification worker stopped");
            })
            .expect("failed to spawn notification-sender thread");
        Mutex::new(tx)
    });

    let guard = sender
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match guard.try_send((title, body, with_sound)) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            warn!(
                target: "notification",
                "notification queue is full; dropping notification"
            );
        }
        Err(TrySendError::Disconnected(_)) => {
            warn!(target: "notification", "notification worker is gone; dropping notification");
        }
    }
}

// ---- macOS: 原生通知 (UNUserNotificationCenter) + osascript fallback ----

/// 请求系统通知授权（仅在 App Bundle 模式下生效）。
///
/// 应在应用启动时调用一次。同时设置 delegate 以支持前台通知弹出。
/// 如果不在 Bundle 内（如 `cargo run`），此函数不做任何操作。
#[cfg(target_os = "macos")]
pub fn request_notification_authorization() {
    if !is_running_in_bundle() {
        info!(target: "notification", "not running in app bundle, skipping notification authorization");
        return;
    }

    unsafe {
        use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

        let center = UNUserNotificationCenter::currentNotificationCenter();

        // 设置 delegate，使通知在前台时也能弹出横幅
        install_notification_delegate(&center);

        let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;

        let handler = block2::RcBlock::new(
            |granted: objc2::runtime::Bool, error: *mut objc2_foundation::NSError| {
                if granted.as_bool() {
                    info!(target: "notification", "notification authorization granted");
                } else {
                    warn!(target: "notification", "notification authorization denied");
                    if !error.is_null() {
                        let err = &*error;
                        warn!(target: "notification", "authorization error: {:?}", err);
                    }
                }
            },
        );

        center.requestAuthorizationWithOptions_completionHandler(options, &handler);
    }
}

/// 安装通知 delegate，实现前台横幅弹出。
///
/// macOS 默认行为：当应用在前台时，通知不弹出横幅，只送到通知中心。
/// 通过实现 `UNUserNotificationCenterDelegate` 的
/// `willPresentNotification:withCompletionHandler:` 方法，
/// 指定 `Banner | Sound | List` 来覆盖此默认行为。
#[cfg(target_os = "macos")]
unsafe fn install_notification_delegate(
    center: &objc2_user_notifications::UNUserNotificationCenter,
) {
    use std::cell::OnceCell;
    use std::sync::Once;

    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2_user_notifications::UNNotificationPresentationOptions;

    static REGISTER: Once = Once::new();

    assert!(
        is_main_thread(),
        "install_notification_delegate must be called on the main thread"
    );

    REGISTER.call_once(|| {
        // 注册一个 ObjC 类 BananaTrayNotificationDelegate : NSObject
        let superclass = AnyClass::get(c"NSObject").unwrap();
        let mut builder = ClassBuilder::new(c"BananaTrayNotificationDelegate", superclass).unwrap();

        // 实现 userNotificationCenter:willPresentNotification:withCompletionHandler:
        // 签名: void (id self, SEL _cmd, id center, id notification, id completionHandler)
        unsafe extern "C" fn will_present(
            _this: &AnyObject,
            _cmd: Sel,
            _center: &AnyObject,
            _notification: &AnyObject,
            handler: &block2::Block<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            let options = UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::Sound
                | UNNotificationPresentationOptions::List;
            handler.call((options,));
        }

        builder.add_method(
            objc2::sel!(userNotificationCenter:willPresentNotification:withCompletionHandler:),
            will_present as unsafe extern "C" fn(_, _, _, _, _),
        );

        // 声明遵守 UNUserNotificationCenterDelegate protocol
        let protocol =
            objc2::runtime::AnyProtocol::get(c"UNUserNotificationCenterDelegate").unwrap();
        builder.add_protocol(protocol);

        let _cls = builder.register();
    });

    // 创建 delegate 实例并设置到 center。
    // 注意：UNUserNotificationCenter 仅弱引用 delegate，
    // 所以需要由当前线程长期持有对象。该函数当前只在 UI 启动主线程调用。
    thread_local! {
        static DELEGATE: OnceCell<Retained<AnyObject>> = const { OnceCell::new() };
    }

    DELEGATE.with(|cell| {
        let delegate = cell.get_or_init(|| create_notification_delegate());
        let delegate_obj: &AnyObject = std::ops::Deref::deref(delegate);
        let _: () = objc2::msg_send![center, setDelegate: delegate_obj];
    });

    info!(target: "notification", "notification delegate installed for foreground banner support");
}

#[cfg(target_os = "macos")]
unsafe fn create_notification_delegate() -> objc2::rc::Retained<objc2::runtime::AnyObject> {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};

    let cls = AnyClass::get(c"BananaTrayNotificationDelegate").unwrap();
    let obj: *mut AnyObject = objc2::msg_send![cls, alloc];
    let obj: *mut AnyObject = objc2::msg_send![obj, init];

    Retained::from_raw(obj).expect("delegate pointer must be non-null")
}

#[cfg(target_os = "macos")]
fn is_main_thread() -> bool {
    unsafe extern "C" {
        fn pthread_main_np() -> std::ffi::c_int;
    }

    unsafe { pthread_main_np() == 1 }
}

#[cfg(not(target_os = "macos"))]
pub fn request_notification_authorization() {
    // 非 macOS 平台不需要请求授权
}

/// 检测当前进程是否运行在 macOS App Bundle 内。
///
/// 通过检查 `CFBundleIdentifier` 是否存在来判断：以 `.app` 方式运行时
/// 会有有效的 Bundle ID，而 `cargo run` 直接运行二进制时不会有。
#[cfg(target_os = "macos")]
fn is_running_in_bundle() -> bool {
    use objc2_foundation::NSBundle;
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some()
}

#[cfg(target_os = "macos")]
fn platform_send_notification(title: &str, body: &str, with_sound: bool) {
    if is_running_in_bundle() {
        send_native_notification(title, body, with_sound);
    } else {
        send_osascript_notification(title, body, with_sound);
    }
}

/// 通过 UNUserNotificationCenter 发送原生系统通知。
///
/// 仅在 App Bundle 模式下使用。通知会显示应用图标，
/// 并在系统通知中心归类到 BananaTray。
///
/// 若原生通知发送失败（如未签名），自动 fallback 到 osascript。
#[cfg(target_os = "macos")]
fn send_native_notification(title: &str, body: &str, with_sound: bool) {
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNNotificationSound,
        UNUserNotificationCenter,
    };

    unsafe {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));

        if with_sound {
            let sound = UNNotificationSound::defaultSound();
            content.setSound(Some(&sound));
        }

        // 使用时间戳作为唯一 ID，避免通知覆盖
        let identifier = NSString::from_str(&format!(
            "{}-{}",
            crate::platform::APP_ID_LOWER,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));

        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );

        let center = UNUserNotificationCenter::currentNotificationCenter();

        // 捕获 title/body/with_sound 用于 fallback
        let title_owned = title.to_string();
        let body_owned = body.to_string();
        let handler = block2::RcBlock::new(move |error: *mut objc2_foundation::NSError| {
            if error.is_null() {
                info!(target: "notification", "native notification sent: {}", title_owned);
            } else {
                let err = &*error;
                warn!(target: "notification", "native notification failed: {:?}, falling back to osascript", err);
                send_osascript_notification(&title_owned, &body_owned, with_sound);
            }
        });

        center.addNotificationRequest_withCompletionHandler(&request, Some(&handler));
    }
}

/// 通过 osascript (AppleScript) 发送通知。
///
/// 用于 `cargo run` 开发模式下的 fallback，或任何不在 App Bundle 中运行的场景。
#[cfg(target_os = "macos")]
fn send_osascript_notification(title: &str, body: &str, with_sound: bool) {
    // 转义双引号，防止 AppleScript 注入
    let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");

    let script = if with_sound {
        format!(
            r#"display notification "{}" with title "{}" sound name "Glass""#,
            escaped_body, escaped_title
        )
    } else {
        format!(
            r#"display notification "{}" with title "{}""#,
            escaped_body, escaped_title
        )
    };

    match run_command_with_deadline(
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script),
        NOTIFICATION_CALL_TIMEOUT,
    ) {
        Ok(Some(output)) => {
            if output.status.success() {
                info!(target: "notification", "osascript notification sent: {}", title);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!(target: "notification", "osascript failed: {}", stderr.trim());
            }
        }
        Ok(None) => {
            warn!(
                target: "notification",
                "osascript did not finish within {:?}; killing it",
                NOTIFICATION_CALL_TIMEOUT
            );
        }
        Err(err) => {
            warn!(target: "notification", "failed to run osascript: {}", err);
        }
    }
}

/// 运行命令并等待结果，超过 `timeout` 就杀掉并返回 `Ok(None)`。
#[cfg(target_os = "macos")]
fn run_command_with_deadline(
    command: &mut std::process::Command,
    timeout: Duration,
) -> std::io::Result<Option<std::process::Output>> {
    // 给 osascript 单独建立进程组，避免它退出后派生进程继续持有输出管道，
    // 让后续 `wait_with_output()` 绕过 deadline 一直等 EOF。
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }

    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            #[cfg(unix)]
            kill_process_group(child.id());
            return child.wait_with_output().map(Some);
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            kill_process_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(NOTIFICATION_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn kill_process_group(pid: u32) {
    if let Ok(pgid) = libc::pid_t::try_from(pid) {
        if pgid > 1 {
            unsafe {
                let _ = libc::kill(-pgid, libc::SIGKILL);
            }
        }
    }
}

// ---- non-macOS: notify-rust (Linux D-Bus / Windows Toast) ----
// NOTE: notify-rust is a cfg(not(macos)) dependency in Cargo.toml.
// Do NOT add a macOS code path here; see module-level doc for rationale.

#[cfg(target_os = "linux")]
fn platform_send_notification(title: &str, body: &str, with_sound: bool) {
    let mut notification = notify_rust::Notification::new();
    notification
        .appname(crate::platform::APP_NAME)
        .summary(title)
        .body(body);

    if with_sound {
        notification.sound_name("default");
    }

    let result = smol::block_on(smol::future::race(
        async {
            notification
                .show_async()
                .await
                .map(|_| ())
                .map_err(|err| err.to_string())
        },
        async {
            smol::Timer::after(NOTIFICATION_CALL_TIMEOUT).await;
            Err("notification call timed out".to_string())
        },
    ));

    match result {
        Ok(_) => {
            info!(target: "notification", "system notification sent: {}", title);
        }
        Err(err) => {
            warn!(target: "notification", "failed to send system notification: {}", err);
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
fn platform_send_notification(title: &str, body: &str, with_sound: bool) {
    let mut notification = notify_rust::Notification::new();
    notification
        .appname(crate::platform::APP_NAME)
        .summary(title)
        .body(body);

    if with_sound {
        notification.sound_name("default");
    }

    match notification.show() {
        Ok(_) => {
            info!(target: "notification", "system notification sent: {}", title);
        }
        Err(err) => {
            warn!(target: "notification", "failed to send system notification: {}", err);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    /// `cargo test` 运行时不在 .app Bundle 内，`is_running_in_bundle` 应返回 false
    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_running_in_bundle_returns_false_in_tests() {
        assert!(
            !super::is_running_in_bundle(),
            "cargo test 环境下不应被识别为 App Bundle"
        );
    }

    use crate::application::QuotaNotificationEvent;
    use crate::models::{QuotaInfo, QuotaType};

    fn balance_quota(amount: f64) -> QuotaInfo {
        QuotaInfo::balance_only("credits", amount, None, QuotaType::Credit, None)
    }

    fn assert_no_unreplaced_params(text: &str) {
        assert!(!text.contains("%{"), "unreplaced params in: {text}");
    }

    #[test]
    fn quota_alert_body_uses_native_units_en() {
        let _guard = crate::i18n::test_locale_guard("en");

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::LowQuota {
            provider_name: "Claude".to_string(),
            quota: balance_quota(2.50),
        });
        assert!(body.contains("$2.50"), "body: {body}");
        assert!(!body.contains('%'), "body: {body}");
        assert_no_unreplaced_params(&body);

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::Recovered {
            provider_name: "Claude".to_string(),
            quota: balance_quota(2.50),
        });
        assert!(body.contains("$2.50"), "body: {body}");
        assert!(!body.contains('%'), "body: {body}");

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::LowQuota {
            provider_name: "Claude".to_string(),
            quota: QuotaInfo::with_details("points", 60.0, 100.0, QuotaType::Points, None),
        });
        assert!(body.contains("40.00"), "body: {body}");
        assert!(!body.contains('$'), "body: {body}");
        assert_no_unreplaced_params(&body);

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::LowQuota {
            provider_name: "Claude".to_string(),
            quota: QuotaInfo::new("session", 70.0, 100.0),
        });
        assert!(body.contains("30%"), "body: {body}");
        assert!(body.contains("session"), "body: {body}");
        assert_no_unreplaced_params(&body);

        let (title, body) = super::quota_alert_notification(&QuotaNotificationEvent::Exhausted {
            provider_name: "Claude".to_string(),
            quota: QuotaInfo::new("session", 100.0, 100.0),
        });
        assert!(title.contains("Claude"), "title: {title}");
        assert!(body.contains("session"), "body: {body}");
        assert_no_unreplaced_params(&body);

        let (title, body) =
            super::quota_alert_notification(&QuotaNotificationEvent::UsageProgress {
                provider_name: "Claude".to_string(),
                remaining_pct: 42.0,
            });
        assert!(title.contains("Claude"), "title: {title}");
        assert!(body.contains("42%"), "body: {body}");
        assert_no_unreplaced_params(&body);
    }

    #[test]
    fn quota_alert_body_uses_native_units_zh_cn() {
        let _guard = crate::i18n::test_locale_guard("zh-CN");

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::LowQuota {
            provider_name: "Claude".to_string(),
            quota: balance_quota(2.50),
        });
        assert!(body.contains("$2.50"), "body: {body}");
        assert!(!body.contains('%'), "body: {body}");
        assert_no_unreplaced_params(&body);

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::Recovered {
            provider_name: "Claude".to_string(),
            quota: balance_quota(2.50),
        });
        assert!(body.contains("$2.50"), "body: {body}");

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::LowQuota {
            provider_name: "Claude".to_string(),
            quota: QuotaInfo::with_details("points", 60.0, 100.0, QuotaType::Points, None),
        });
        assert!(body.contains("40.00"), "body: {body}");
        assert!(!body.contains('$'), "body: {body}");

        let (_, body) = super::quota_alert_notification(&QuotaNotificationEvent::Exhausted {
            provider_name: "Claude".to_string(),
            quota: QuotaInfo::new("session", 100.0, 100.0),
        });
        assert!(body.contains("session"), "body: {body}");
        assert_no_unreplaced_params(&body);
    }

    /// 超过上限时杀掉子进程并返回 `Ok(None)`，不无限等待。
    #[cfg(target_os = "macos")]
    #[test]
    fn run_command_with_deadline_kills_slow_child() {
        use std::time::Duration;
        let start = std::time::Instant::now();
        let result = super::run_command_with_deadline(
            std::process::Command::new("sleep").arg("10"),
            Duration::from_millis(100),
        )
        .expect("spawn sleep");

        assert!(result.is_none(), "a slow child must be killed, not awaited");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "run_command_with_deadline must not wait for the whole child"
        );
    }

    /// 正常退出的命令返回输出。
    #[cfg(target_os = "macos")]
    #[test]
    fn run_command_with_deadline_returns_output_for_fast_child() {
        use std::time::Duration;
        let result = super::run_command_with_deadline(
            std::process::Command::new("echo").arg("ok"),
            Duration::from_secs(5),
        )
        .expect("spawn echo");

        let output = result.expect("fast child must finish");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    }
}
