//! 托盘弹窗窗口尺寸调整。
//!
//! GPUI 的 `Window::resize()` 在 macOS 上会异步调用 `setContentSize:`，并保持
//! AppKit 左下角原点不变。菜单栏弹窗因此会整窗上顶/下缩，叠加 PopUp 的
//! UtilityWindow 动画。这里改为 `setFrame:display:animate:`：顶边钉住、关掉动画。
//! 调用必须 `dispatch_async` 回主队列——`render()` 期间 App 已被借走，同步
//! setFrame 会让 GPUI 的 resize callback `try_borrow_mut` 失败。
//!
//! Overview 展开/折叠不要走这里：原生窗口改尺寸本身就会抖。

use gpui::{size, Pixels, Size, Window, WindowBounds};

const SIZE_EPSILON: f32 = 2.0;

fn exceeds_epsilon(width_delta: f64, height_delta: f64) -> bool {
    width_delta.abs() > f64::from(SIZE_EPSILON) || height_delta.abs() > f64::from(SIZE_EPSILON)
}

pub(crate) fn size_differs(current: Size<Pixels>, width: Pixels, height: Pixels) -> bool {
    exceeds_epsilon(
        f64::from(current.width - width),
        f64::from(current.height - height),
    )
}

/// 将弹窗内容区调整到 `width × height`。高度对不齐时才动手。
pub(crate) fn resize_popup_window(window: &mut Window, width: Pixels, height: Pixels) {
    let WindowBounds::Windowed(current) = window.window_bounds() else {
        return;
    };
    if !size_differs(current.size, width, height) {
        return;
    }

    #[cfg(target_os = "macos")]
    if schedule_mac_popup_resize(window, width, height) {
        return;
    }

    window.resize(size(width, height));
}

#[cfg(target_os = "macos")]
fn schedule_mac_popup_resize(window: &Window, width: Pixels, height: Pixels) -> bool {
    use dispatch2::DispatchQueue;
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };

    // SAFETY: GPUI 的 AppKit handle 指向当前窗口仍然活着的 NSView。
    let ns_view = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let Some(ns_window) = ns_view.window() else {
        return false;
    };

    let frame = ns_window.frame();
    let content = ns_window.contentRectForFrameRect(frame);
    let new_width = f64::from(width);
    let new_height = f64::from(height);
    let Some(pinned) =
        resized_content_preserving_top(content.origin.y, content.size, new_width, new_height)
    else {
        return true;
    };

    let mut new_content = content;
    new_content.origin.y = pinned.origin_y;
    new_content.size.width = pinned.width;
    new_content.size.height = pinned.height;
    let new_frame = ns_window.frameRectForContentRect(new_content);
    let main_frame = MainThreadFrame(new_frame);

    // window() 已经是 Retained；所有权交给主队列闭包。
    let main_window = MainThreadWindow::from_retained(ns_window);
    DispatchQueue::main().exec_async(move || {
        let MainThreadFrame(new_frame) = main_frame;
        let Some(ns_window) = main_window.into_retained() else {
            return;
        };
        ns_window.setFrame_display_animate(new_frame, false, false);
    });
    true
}

/// NSWindow 只能在主线程用。GCD 要求闭包 Send；用 usize 过边界，不把 `*mut NSWindow` 放进闭包。
#[cfg(target_os = "macos")]
struct MainThreadWindow(usize);

#[cfg(target_os = "macos")]
impl MainThreadWindow {
    fn from_retained(window: objc2::rc::Retained<objc2_app_kit::NSWindow>) -> Self {
        Self(objc2::rc::Retained::into_raw(window) as usize)
    }

    fn into_retained(self) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
        // SAFETY: 只从 from_retained 构造，闭包只跑一次，且始终在主线程。
        unsafe { objc2::rc::Retained::from_raw(self.0 as *mut objc2_app_kit::NSWindow) }
    }
}

/// NSRect 只是几何值，但含裸指针字段没有 Send；主线程 → 主队列闭包，用 wrapper 满足约束。
#[cfg(target_os = "macos")]
struct MainThreadFrame(objc2_foundation::NSRect);

// SAFETY: 不跨线程使用，只为满足 DispatchQueue::exec_async 的 Send 约束。
#[cfg(target_os = "macos")]
unsafe impl Send for MainThreadFrame {}

#[cfg(target_os = "macos")]
struct TopPinnedContent {
    origin_y: f64,
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
fn resized_content_preserving_top(
    origin_y: f64,
    old_size: objc2_foundation::NSSize,
    new_width: f64,
    new_height: f64,
) -> Option<TopPinnedContent> {
    let delta_h = new_height - old_size.height;
    let delta_w = new_width - old_size.width;
    if !exceeds_epsilon(delta_w, delta_h) {
        return None;
    }
    // AppKit 原点在左下：增高时 origin.y 下移，顶边钉在菜单栏下方。
    Some(TopPinnedContent {
        origin_y: origin_y - delta_h,
        width: new_width,
        height: new_height,
    })
}

#[cfg(test)]
mod tests {
    use super::size_differs;
    use gpui::{px, size};

    #[test]
    fn size_differs_ignores_sub_epsilon_noise() {
        let current = size(px(380.0), px(300.0));
        assert!(!size_differs(current, px(380.0), px(301.0)));
        assert!(size_differs(current, px(380.0), px(360.0)));
    }

    #[cfg(target_os = "macos")]
    use super::resized_content_preserving_top;
    #[cfg(target_os = "macos")]
    use objc2_foundation::NSSize;

    #[test]
    #[cfg(target_os = "macos")]
    fn expanding_moves_origin_down() {
        let pinned =
            resized_content_preserving_top(200.0, NSSize::new(380.0, 300.0), 380.0, 360.0).unwrap();
        assert_eq!(pinned.origin_y, 140.0);
        assert_eq!(pinned.width, 380.0);
        assert_eq!(pinned.height, 360.0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn collapsing_moves_origin_up() {
        let pinned =
            resized_content_preserving_top(140.0, NSSize::new(380.0, 360.0), 380.0, 300.0).unwrap();
        assert_eq!(pinned.origin_y, 200.0);
        assert_eq!(pinned.height, 300.0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn tiny_delta_is_ignored() {
        assert!(
            resized_content_preserving_top(200.0, NSSize::new(380.0, 300.0), 380.0, 301.0)
                .is_none()
        );
    }
}
