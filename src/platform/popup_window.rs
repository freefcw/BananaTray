//! 托盘弹窗窗口尺寸调整。
//!
//! fc-gpui 0.10 起 `Window::resize_anchored` 在 macOS 上同步执行
//! `setFrame:display:animate:NO`：顶边钉住、关掉 UtilityWindow 动画，
//! 且不再因同步回调撞上 `try_borrow_mut` 而丢 resize 通知。
//! 历史上的 AppKit 直调 workaround 见 docs/architecture.md Workaround Register。
//!
//! Overview 展开/折叠不要走这里：原生窗口改尺寸本身就会抖。

use gpui::{size, Pixels, ResizeAnchor, Size, Window, WindowBounds};

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
    // 托盘弹窗钉住顶边（菜单栏下缘），向下生长；非 macOS 平台回退为普通 resize。
    window.resize_anchored(size(width, height), ResizeAnchor::TopLeft);
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
}
