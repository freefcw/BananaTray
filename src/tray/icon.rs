//! Tray icon style helper — applies the user-chosen icon style to the system tray.
//!
//! Uses GPUI's native `set_tray_icon_rendering_mode` API to control whether the icon
//! renders as a template (Adaptive, for Monochrome) or with original colors (Original,
//! for Yellow/Colorful).

use crate::application::TrayIconRequest;
use crate::models::{StatusLevel, TrayIconStyle};
use gpui::{App, TrayIconRenderingMode};
use log::info;

/// 单色图标的 PNG 数据（平台相关）。
///
/// Linux 没有 template rendering，深色面板上黑色图标不可见，使用白色变体。
/// 使用 `#[cfg]` 而非 `cfg!()` 确保只有目标平台的图标被嵌入二进制。
#[cfg(target_os = "linux")]
fn monochrome_png() -> &'static [u8] {
    include_bytes!("tray_icon_light.png")
}

/// 单色图标的 PNG 数据（平台相关）。
///
/// macOS / Windows 有 template rendering，使用黑色原版。
#[cfg(not(target_os = "linux"))]
fn monochrome_png() -> &'static [u8] {
    include_bytes!("tray_icon.png")
}

/// Return the embedded PNG data for the given icon request.
///
/// This is a pure function, suitable for unit testing without a GUI context.
pub fn icon_png_data(request: TrayIconRequest) -> &'static [u8] {
    match request {
        TrayIconRequest::Static(TrayIconStyle::Monochrome) => monochrome_png(),
        TrayIconRequest::Static(TrayIconStyle::Yellow) => include_bytes!("tray_icon_yellow.png"),
        TrayIconRequest::Static(TrayIconStyle::Colorful) => {
            include_bytes!("tray_icon_colorful.png")
        }
        // Dynamic 选项直接传入时回退 Monochrome（正常流程不会到这，reducer 会 resolve）
        TrayIconRequest::Static(TrayIconStyle::Dynamic) => monochrome_png(),
        // Dynamic 模式：Green 状态用 Monochrome，减少视觉干扰
        TrayIconRequest::DynamicStatus(StatusLevel::Green) => monochrome_png(),
        TrayIconRequest::DynamicStatus(StatusLevel::Yellow) => {
            include_bytes!("tray_icon_yellow.png")
        }
        TrayIconRequest::DynamicStatus(StatusLevel::Red) => include_bytes!("tray_icon_red.png"),
    }
}

/// 返回托盘图标的渲染模式
/// - Monochrome / Dynamic Green 使用 Adaptive（模板渲染，自动适应深色/浅色菜单栏）
/// - Yellow / Colorful / Red 使用 Original（保留原始颜色）
fn icon_rendering_mode(request: TrayIconRequest) -> TrayIconRenderingMode {
    match request {
        TrayIconRequest::Static(TrayIconStyle::Monochrome)
        | TrayIconRequest::Static(TrayIconStyle::Dynamic)
        | TrayIconRequest::DynamicStatus(StatusLevel::Green) => TrayIconRenderingMode::Adaptive,
        _ => TrayIconRenderingMode::Original,
    }
}

/// Apply the given tray icon.
///
/// 使用 GPUI 原生 API：先设置渲染模式，再设置图标数据（确保一次到位）。
pub fn apply_tray_icon(cx: &mut App, request: TrayIconRequest) {
    let png_data = icon_png_data(request);
    let rendering_mode = icon_rendering_mode(request);

    cx.set_tray_icon_rendering_mode(rendering_mode);
    cx.set_tray_icon(Some(png_data));

    info!(
        target: "tray",
        "applied tray icon: {:?}, rendering_mode: {:?}",
        request, rendering_mode
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有可能的 TrayIconRequest 变体
    fn all_requests() -> Vec<TrayIconRequest> {
        vec![
            TrayIconRequest::Static(TrayIconStyle::Monochrome),
            TrayIconRequest::Static(TrayIconStyle::Yellow),
            TrayIconRequest::Static(TrayIconStyle::Colorful),
            TrayIconRequest::Static(TrayIconStyle::Dynamic),
            TrayIconRequest::DynamicStatus(StatusLevel::Green),
            TrayIconRequest::DynamicStatus(StatusLevel::Yellow),
            TrayIconRequest::DynamicStatus(StatusLevel::Red),
        ]
    }

    #[test]
    fn icon_png_data_returns_non_empty_for_all_requests() {
        for request in all_requests() {
            let data = icon_png_data(request);
            assert!(
                !data.is_empty(),
                "PNG data for {:?} should not be empty",
                request
            );
        }
    }

    #[test]
    fn icon_png_data_starts_with_png_magic() {
        let png_signature: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        for request in all_requests() {
            let data = icon_png_data(request);
            assert!(
                data.starts_with(&png_signature),
                "PNG data for {:?} should start with PNG magic bytes",
                request
            );
        }
    }

    #[test]
    fn static_icons_differ_from_each_other() {
        let mono = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Monochrome));
        let yellow = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Yellow));
        let colorful = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Colorful));
        assert_ne!(mono, yellow);
        assert_ne!(mono, colorful);
        assert_ne!(yellow, colorful);
    }

    #[test]
    fn dynamic_green_uses_monochrome() {
        let mono = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Monochrome));
        let green = icon_png_data(TrayIconRequest::DynamicStatus(StatusLevel::Green));
        assert_eq!(mono, green, "Dynamic Green should use Monochrome icon");
    }

    #[test]
    fn dynamic_yellow_uses_yellow_icon() {
        let yellow = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Yellow));
        let dyn_yellow = icon_png_data(TrayIconRequest::DynamicStatus(StatusLevel::Yellow));
        assert_eq!(yellow, dyn_yellow, "Dynamic Yellow should use Yellow icon");
    }

    #[test]
    fn dynamic_red_is_unique() {
        let mono = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Monochrome));
        let yellow = icon_png_data(TrayIconRequest::Static(TrayIconStyle::Yellow));
        let red = icon_png_data(TrayIconRequest::DynamicStatus(StatusLevel::Red));
        assert_ne!(red, mono, "Red icon should differ from Monochrome");
        assert_ne!(red, yellow, "Red icon should differ from Yellow");
    }

    #[test]
    fn icon_rendering_mode_monochrome_is_adaptive() {
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::Static(TrayIconStyle::Monochrome)),
            TrayIconRenderingMode::Adaptive
        );
    }

    #[test]
    fn icon_rendering_mode_dynamic_green_is_adaptive() {
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::DynamicStatus(StatusLevel::Green)),
            TrayIconRenderingMode::Adaptive
        );
    }

    #[test]
    fn icon_rendering_mode_colored_is_original() {
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::Static(TrayIconStyle::Yellow)),
            TrayIconRenderingMode::Original
        );
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::Static(TrayIconStyle::Colorful)),
            TrayIconRenderingMode::Original
        );
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::DynamicStatus(StatusLevel::Yellow)),
            TrayIconRenderingMode::Original
        );
        assert_eq!(
            icon_rendering_mode(TrayIconRequest::DynamicStatus(StatusLevel::Red)),
            TrayIconRenderingMode::Original
        );
    }
}
