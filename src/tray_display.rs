//! macOS 多显示器感知的托盘弹窗定位
//!
//! GPUI 的 tray_icon_bounds() 内部用 NSScreen::mainScreen（焦点屏幕）做 Y 翻转，
//! 但 MacWindow::open() 用 primary screen 高度做反向转换。当两者高度不同时产生偏差。
//! 此模块绕过该链路，直接用 CoreGraphics 鼠标坐标计算 display-local 位置。

use gpui::{point, px, App, Bounds, DisplayId, Pixels, Size};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type CGDirectDisplayID = u32;
type CGEventRef = *const std::ffi::c_void;

extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
}

/// 获取鼠标光标的全局位置（CoreGraphics 坐标系：主屏幕左上角为原点，Y 向下）
fn mouse_position() -> Option<CGPoint> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let loc = CGEventGetLocation(event);
        CFRelease(event);
        Some(loc)
    }
}

/// 计算托盘弹窗在多显示器环境中的正确位置。
///
/// 通过 CoreGraphics 获取鼠标位置（位于托盘图标内），找到对应显示器，
/// 然后计算 display-local 坐标，避免 GPUI 内部 mainScreen/primaryScreen 混用导致的偏差。
///
/// 返回 (display-local bounds, target display_id)。
pub(crate) fn compute_tray_popup_bounds(
    cx: &App,
    window_size: Size<Pixels>,
    tray_bounds: Bounds<Pixels>,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    let Some(mouse) = mouse_position() else {
        log::warn!(target: "tray", "无法获取鼠标位置，回退到默认定位");
        return (fallback_bounds(window_size, tray_bounds), None);
    };

    // 在所有显示器中找到鼠标所在的那个
    let displays = cx.displays();
    let target = displays.iter().find_map(|d| {
        let id_u32: u32 = d.id().into();
        let rect = unsafe { CGDisplayBounds(id_u32) };
        let contains = mouse.x >= rect.origin.x
            && mouse.x < rect.origin.x + rect.size.width
            && mouse.y >= rect.origin.y
            && mouse.y < rect.origin.y + rect.size.height;
        if contains {
            Some((d.id(), rect))
        } else {
            None
        }
    });

    let Some((display_id, display_rect)) = target else {
        log::warn!(target: "tray", "未找到鼠标所在显示器，回退到默认定位");
        return (fallback_bounds(window_size, tray_bounds), None);
    };

    // 托盘图标的全局 x 坐标（macOS 和 CG 的 x 轴方向一致）
    let tray_center_x = tray_bounds.origin.x + tray_bounds.size.width * 0.5;
    // 转为 display-local x 并居中窗口
    let local_x = tray_center_x - px(display_rect.origin.x as f32) - window_size.width * 0.5;

    // 鼠标 Y 坐标（CG 坐标，相对于主屏左上角）转为 display-local
    // 用户在菜单栏点击托盘图标时，鼠标 Y ≈ 菜单栏高度（约 25pt）
    let mouse_local_y = px((mouse.y - display_rect.origin.y) as f32);
    // 取鼠标 Y 和托盘图标高度中的较大值，确保窗口在菜单栏下方
    let local_y = mouse_local_y.max(tray_bounds.size.height);

    // 确保窗口不超出屏幕左右边界
    let display_width = px(display_rect.size.width as f32);
    let clamped_x = local_x.max(px(0.0)).min(display_width - window_size.width);

    let bounds = Bounds::new(point(clamped_x, local_y), window_size);

    log::debug!(
        target: "tray",
        "multi-display positioning: mouse=({:.0},{:.0}), display={:?} rect=({:.0},{:.0} {:.0}x{:.0}), result=({:.1},{:.1})",
        mouse.x, mouse.y,
        display_id,
        display_rect.origin.x, display_rect.origin.y,
        display_rect.size.width, display_rect.size.height,
        clamped_x, local_y,
    );

    (bounds, Some(display_id))
}

/// 回退定位：直接使用 tray bounds（仅在无法获取鼠标位置时使用）
fn fallback_bounds(window_size: Size<Pixels>, tray: Bounds<Pixels>) -> Bounds<Pixels> {
    let x = tray.origin.x + (tray.size.width - window_size.width) * 0.5;
    let y = tray.origin.y + tray.size.height;
    Bounds::new(point(x, y), window_size)
}
