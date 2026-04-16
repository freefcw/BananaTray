//! macOS CoreGraphics 安全包装。
//!
//! 集中管理少量鼠标/显示器几何查询，避免在业务模块重复声明 FFI。

#[derive(Clone, Copy, Debug)]
pub(crate) struct MousePosition {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayBounds {
    pub(crate) origin_x: f64,
    pub(crate) origin_y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl DisplayBounds {
    pub(crate) fn contains(self, position: MousePosition) -> bool {
        position.x >= self.origin_x
            && position.x < self.origin_x + self.width
            && position.y >= self.origin_y
            && position.y < self.origin_y + self.height
    }
}

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

unsafe extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
}

/// 获取鼠标光标的全局位置（CoreGraphics 坐标系：主屏幕左上角为原点，Y 向下）。
pub(crate) fn mouse_position() -> Option<MousePosition> {
    // SAFETY: CGEventCreate(NULL) is documented to create a blank event — NULL source is valid.
    // event.is_null() guards all subsequent dereferences. CFRelease is called after loc fields
    // are copied into MousePosition (no use-after-free). All FFI symbols are system CoreGraphics
    // functions available on macOS.
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let loc = CGEventGetLocation(event);
        CFRelease(event);
        Some(MousePosition { x: loc.x, y: loc.y })
    }
}

/// 查询指定显示器在 CoreGraphics 全局坐标系中的边界。
pub(crate) fn display_bounds(display_id: CGDirectDisplayID) -> DisplayBounds {
    // SAFETY: CGDisplayBounds returns an empty CGRect for invalid display IDs rather than
    // causing undefined behavior. Callers pass GPUI's DisplayId which is always valid.
    unsafe {
        let rect = CGDisplayBounds(display_id);
        DisplayBounds {
            origin_x: rect.origin.x,
            origin_y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        }
    }
}
