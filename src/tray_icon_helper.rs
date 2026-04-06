//! Tray icon style helper — applies the user-chosen icon style to the system tray.
//!
//! # Why this module exists (macOS `setTemplate` hack)
//!
//! The GPUI framework (`adabraka-gpui`) hard-codes `[NSImage setTemplate:YES]` in
//! its `MacTray::set_icon()` implementation (see `platform/mac/tray.rs:58`).
//!
//! When an NSImage is marked as a **template image**, macOS ignores all color
//! information and only uses the alpha channel to render a monochrome icon that
//! automatically adapts to the menu bar's light/dark appearance. This is great
//! for the default "Monochrome" style, but makes it impossible to show colored
//! icons (Yellow, Colorful) through the normal GPUI API.
//!
//! Since we don't maintain the `adabraka-gpui` crate and can't modify its API,
//! we work around this by directly accessing the `NSStatusItem` button via
//! Objective-C runtime and setting the image with `setTemplate:NO`.
//!
//! ## macOS compatibility notes
//!
//! - `NSStatusBar.statusItems` was deprecated in macOS 13 and throws ObjC
//!   exceptions on macOS 13+.  We use `NSApp.windows` filtered by class name
//!   `NSStatusBarWindow` instead.
//! - To inspect ObjC class names, use `Class::name()` (calls C runtime
//!   `class_getName`).  NEVER use `msg_send![class, UTF8String]` — `Class`
//!   does not respond to `UTF8String` and will throw an unrecognized selector
//!   exception that Rust cannot catch.
//! - Icon PNGs must be RGBA (4-channel with alpha).
//!
//! ## When the framework API is fixed
//!
//! If `adabraka-gpui` ever adds an `is_template` parameter to `set_tray_icon()`,
//! this entire hack can be replaced with a single API call. Search for
//! `HACK(setTemplate)` to find all related code.

use crate::models::TrayIconStyle;
use gpui::App;
use log::info;

/// Return the embedded PNG data for the given icon style.
///
/// This is a pure function, suitable for unit testing without a GUI context.
pub fn icon_png_data(style: TrayIconStyle) -> &'static [u8] {
    match style {
        TrayIconStyle::Monochrome => include_bytes!("tray_icon.png"),
        TrayIconStyle::Yellow => include_bytes!("tray_icon_yellow.png"),
        TrayIconStyle::Colorful => include_bytes!("tray_icon_colorful.png"),
    }
}

/// Apply the given tray icon style.
///
/// For `Monochrome`, delegates to GPUI's `set_tray_icon()` which sets
/// `setTemplate:YES` automatically.
///
/// For `Yellow` and `Colorful`, bypasses GPUI entirely and sets the icon
/// directly via ObjC runtime with `setTemplate:NO` so colors are preserved.
pub fn apply_tray_icon(cx: &mut App, style: TrayIconStyle) {
    let png_data = icon_png_data(style);

    #[cfg(target_os = "macos")]
    {
        if style == TrayIconStyle::Monochrome {
            // Monochrome: use GPUI's built-in path (sets setTemplate:YES).
            cx.set_tray_icon(Some(png_data));
        } else {
            // HACK(setTemplate): Bypass GPUI and set the icon directly with
            // setTemplate:NO so macOS preserves the colors.
            //
            // We call GPUI first with the monochrome icon to ensure the
            // NSStatusItem exists, then immediately replace the image via ObjC.
            // This avoids the wasted work of having GPUI decode the colored PNG
            // only for us to throw it away — the monochrome PNG is smaller and
            // the result is invisible since we overwrite it immediately.
            cx.set_tray_icon(Some(icon_png_data(TrayIconStyle::Monochrome)));
            unsafe {
                set_status_item_image(png_data, false);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On Windows/Linux, GPUI doesn't do setTemplate, colors work as-is.
        cx.set_tray_icon(Some(png_data));
    }

    info!(
        target: "tray",
        "applied tray icon style: {:?}",
        style
    );
}

/// HACK(setTemplate): Directly set the tray icon image with explicit template control.
///
/// Creates a fresh `NSImage` from raw PNG data, sets `setTemplate:` as specified,
/// and assigns it to the status bar button — completely bypassing GPUI's
/// `set_icon()` logic.
///
/// # How it finds the button
///
/// Iterates over `[NSApp windows]`, finds the `NSStatusBarWindow` (private
/// window class backing each `NSStatusItem`), then walks the view hierarchy
/// recursively to find an `NSStatusBarButton` (which responds to `setImage:`).
///
/// # Safety
///
/// Must be called on the main thread after `cx.set_tray_icon()` has created
/// the status item at least once.
#[cfg(target_os = "macos")]
unsafe fn set_status_item_image(png_data: &[u8], is_template: bool) {
    use objc2::AnyThread;
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSData;

    // ── Create NSImage from PNG data ──
    let ns_data = NSData::with_bytes(png_data);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &ns_data) else {
        log::error!(target: "tray", "HACK(setTemplate): failed to create NSImage from PNG data");
        return;
    };
    // Match GPUI's sizing (18x18 points).
    image.setSize(objc2_foundation::NSSize::new(18.0, 18.0));
    image.setTemplate(is_template);

    // ── Find and update the NSStatusBarButton ──
    if let Some(button) = find_status_bar_button_in_app() {
        button.setImage(Some(&image));
        log::debug!(
            target: "tray",
            "HACK(setTemplate): set image (template={}) on status bar button",
            is_template,
        );
    } else {
        log::warn!(
            target: "tray",
            "HACK(setTemplate): could not find NSStatusBarButton, colored icon not applied"
        );
    }
}

/// Find the `NSStatusBarButton` by walking `[NSApp windows]`.
///
/// Looks for a window whose class name contains "StatusBar", then recursively
/// searches its view hierarchy for an `NSStatusBarButton`.
#[cfg(target_os = "macos")]
unsafe fn find_status_bar_button_in_app() -> Option<objc2::rc::Retained<objc2_app_kit::NSButton>> {
    use objc2_app_kit::NSApplication;

    let app = NSApplication::sharedApplication(objc2::MainThreadMarker::new_unchecked());
    let windows = app.windows();

    for window in windows.iter() {
        let window_class_name = window.class().name().to_str().unwrap_or("");
        // On macOS 13+ the class may have been renamed. Accept any name
        // containing "StatusBar" to be future-proof.
        if !window_class_name.contains("StatusBar") {
            continue;
        }

        let Some(content_view) = window.contentView() else {
            continue;
        };

        if let Some(button) = find_status_bar_button(&content_view) {
            return Some(button);
        }
    }

    None
}

/// Recursively search the view hierarchy for the status bar button.
///
/// Looks for views whose class name contains "StatusBarButton" (the canonical
/// `NSStatusBarButton`), then falls back to any `NSButton` subclass.
#[cfg(target_os = "macos")]
unsafe fn find_status_bar_button(
    view: &objc2_app_kit::NSView,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSButton>> {
    use objc2::ClassType;
    use objc2_app_kit::NSButton;
    use objc2_foundation::NSObjectProtocol;

    let view_class_name = view.class().name().to_str().unwrap_or("");
    // Prefer exact NSStatusBarButton match.
    if view_class_name.contains("StatusBarButton") {
        return Some(retain_view_as_button(view));
    }

    // Recurse into subviews.
    let subviews = view.subviews();
    for subview in subviews.iter() {
        if let Some(found) = find_status_bar_button(&subview) {
            return Some(found);
        }
    }

    // Fallback: check if this view is any NSButton subclass.
    if view.isKindOfClass(NSButton::class()) {
        return Some(retain_view_as_button(view));
    }

    None
}

/// SAFETY: Caller must ensure that `view` is actually an `NSButton` (or subclass).
///
/// Retains and casts the `NSView` reference to `Retained<NSButton>`.
#[cfg(target_os = "macos")]
unsafe fn retain_view_as_button(
    view: &objc2_app_kit::NSView,
) -> objc2::rc::Retained<objc2_app_kit::NSButton> {
    use objc2::rc::Retained;
    let retained_view: Retained<objc2_app_kit::NSView> =
        Retained::retain(view as *const _ as *mut objc2_app_kit::NSView)
            .expect("view pointer must be non-null");
    Retained::cast_unchecked(retained_view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_png_data_returns_non_empty_for_all_styles() {
        for style in [
            TrayIconStyle::Monochrome,
            TrayIconStyle::Yellow,
            TrayIconStyle::Colorful,
        ] {
            let data = icon_png_data(style);
            assert!(
                !data.is_empty(),
                "PNG data for {:?} should not be empty",
                style
            );
        }
    }

    #[test]
    fn icon_png_data_starts_with_png_magic() {
        // PNG files start with the 8-byte signature: 137 80 78 71 13 10 26 10
        let png_signature: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        for style in [
            TrayIconStyle::Monochrome,
            TrayIconStyle::Yellow,
            TrayIconStyle::Colorful,
        ] {
            let data = icon_png_data(style);
            assert!(
                data.starts_with(&png_signature),
                "PNG data for {:?} should start with PNG magic bytes",
                style
            );
        }
    }

    #[test]
    fn monochrome_and_colored_icons_differ() {
        let mono = icon_png_data(TrayIconStyle::Monochrome);
        let yellow = icon_png_data(TrayIconStyle::Yellow);
        let colorful = icon_png_data(TrayIconStyle::Colorful);
        assert_ne!(mono, yellow);
        assert_ne!(mono, colorful);
        assert_ne!(yellow, colorful);
    }
}
