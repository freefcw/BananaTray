//! Tray popup positioning strategy.
//!
//! This module keeps GPUI anchor/display probing out of `TrayController`.

use gpui::{
    px, App, Bounds, DisplayId, Pixels, PlatformDisplay, Point, Size, TrayAnchor, WindowPosition,
};
use log::debug;
use std::rc::Rc;

#[derive(Debug, Clone, Copy)]
pub(super) struct PopupPositionInputs {
    pub(super) window_size: Size<Pixels>,
    pub(super) last_click_position: Option<Point<Pixels>>,
    pub(super) saved_position: Option<crate::models::SavedWindowPosition>,
}

pub(super) trait PopupPositionContext {
    fn tray_icon_anchor(&self) -> Option<TrayAnchor>;
    fn tray_anchor_for_position(&self, position: Point<Pixels>) -> Option<TrayAnchor>;
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>>;
    fn compute_window_bounds(
        &self,
        size: Size<Pixels>,
        position: &WindowPosition,
    ) -> Bounds<Pixels>;
}

impl PopupPositionContext for App {
    fn tray_icon_anchor(&self) -> Option<TrayAnchor> {
        App::tray_icon_anchor(self)
    }

    fn tray_anchor_for_position(&self, position: Point<Pixels>) -> Option<TrayAnchor> {
        App::tray_anchor_for_position(self, position)
    }

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        App::displays(self)
    }

    fn compute_window_bounds(
        &self,
        size: Size<Pixels>,
        position: &WindowPosition,
    ) -> Bounds<Pixels> {
        App::compute_window_bounds(self, size, position)
    }
}

pub(super) fn preferred_window_bounds(
    cx: &impl PopupPositionContext,
    inputs: PopupPositionInputs,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    #[cfg(target_os = "linux")]
    if let Some(saved) = saved_popup_bounds(cx, inputs.window_size, inputs.saved_position) {
        return saved;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = inputs.saved_position;

    // 优先使用系统原生锚点（macOS 始终可用）
    if let Some(anchor) = cx
        .tray_icon_anchor()
        .filter(|a| a.bounds.size.width > px(0.0) && a.bounds.size.height > px(0.0))
    {
        debug!(
            target: "tray",
            "tray_icon_anchor: display={:?} origin=({:.1},{:.1}) size=({:.1}x{:.1})",
            anchor.display_id,
            anchor.bounds.origin.x, anchor.bounds.origin.y,
            anchor.bounds.size.width, anchor.bounds.size.height,
        );

        let display_id = anchor.display_id;
        let position = WindowPosition::TrayAnchored(anchor);
        return (
            cx.compute_window_bounds(inputs.window_size, &position),
            Some(display_id),
        );
    }

    // Linux: 用 SNI 点击坐标构造近似锚点
    if let Some(anchor) = inputs
        .last_click_position
        .and_then(|pos| cx.tray_anchor_for_position(pos))
    {
        debug!(
            target: "tray",
            "tray_anchor_for_position: display={:?} bounds=({:.1},{:.1} {:.1}x{:.1})",
            anchor.display_id,
            anchor.bounds.origin.x, anchor.bounds.origin.y,
            anchor.bounds.size.width, anchor.bounds.size.height,
        );

        let display_id = anchor.display_id;
        let position = WindowPosition::TrayAnchored(anchor);
        return (
            cx.compute_window_bounds(inputs.window_size, &position),
            Some(display_id),
        );
    }

    debug!(target: "tray", "tray anchor unavailable and no click position, using fallback");

    fallback_window_bounds(cx, inputs.window_size)
}

#[cfg(target_os = "linux")]
fn saved_popup_bounds(
    cx: &impl PopupPositionContext,
    window_size: Size<Pixels>,
    saved_position: Option<crate::models::SavedWindowPosition>,
) -> Option<(Bounds<Pixels>, Option<DisplayId>)> {
    let saved = saved_position?;
    if !saved.x.is_finite() || !saved.y.is_finite() {
        return None;
    }

    let origin = gpui::point(px(saved.x), px(saved.y));
    let bounds = Bounds::new(origin, window_size);
    let display = display_id_for_bounds(cx, bounds)?;

    debug!(
        target: "tray",
        "using saved linux popup position on display {:?}: origin=({:.1},{:.1})",
        display,
        bounds.origin.x,
        bounds.origin.y,
    );
    Some((bounds, Some(display)))
}

#[cfg(target_os = "linux")]
pub(super) fn saved_position_from_bounds(
    bounds: Bounds<Pixels>,
    cx: &impl PopupPositionContext,
) -> Option<crate::models::SavedWindowPosition> {
    let x = f32::from(bounds.origin.x);
    let y = f32::from(bounds.origin.y);
    if !x.is_finite() || !y.is_finite() || display_id_for_bounds(cx, bounds).is_none() {
        return None;
    }

    Some(crate::models::SavedWindowPosition { x, y })
}

#[cfg(target_os = "linux")]
fn display_id_for_bounds(
    cx: &impl PopupPositionContext,
    bounds: Bounds<Pixels>,
) -> Option<DisplayId> {
    let center = gpui::point(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    );
    cx.displays().into_iter().find_map(|display| {
        let display_bounds = display.bounds();
        (display_bounds.contains(&bounds.origin) || display_bounds.contains(&center))
            .then(|| display.id())
    })
}

fn fallback_window_bounds(
    cx: &impl PopupPositionContext,
    window_size: Size<Pixels>,
) -> (Bounds<Pixels>, Option<DisplayId>) {
    if cfg!(target_os = "linux") {
        // Wayland 的 primary_display() 返回 None，compute_window_bounds 的
        // TopRight 路径会退化到 (0,0)。直接取第一个显示器手动计算。
        if let Some(display) = cx.displays().into_iter().next() {
            let db = display.bounds();
            let margin = px(16.0);
            let origin = gpui::point(
                db.origin.x + db.size.width - window_size.width - margin,
                db.origin.y + margin,
            );
            let bounds = Bounds::new(origin, window_size);
            debug!(
                target: "tray",
                "fallback TopRight on display {:?}: origin=({:.1},{:.1})",
                display.id(), bounds.origin.x, bounds.origin.y,
            );
            return (bounds, Some(display.id()));
        }
        // 连 displays() 都为空（不太可能），最终 fallback
        (
            Bounds::new(gpui::point(px(0.0), px(0.0)), window_size),
            None,
        )
    } else {
        let position = WindowPosition::Center;
        (cx.compute_window_bounds(window_size, &position), None)
    }
}
