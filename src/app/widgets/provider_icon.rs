use gpui::*;

/// 判断 icon_asset 是否为 SVG 文件路径（内置 Provider）
///
/// 内置 Provider 使用 "src/icons/provider-xxx.svg" 格式；
/// 自定义 Provider 使用首字母文本（如 "N", "月"）。
pub(crate) fn is_svg_icon(icon_asset: &str) -> bool {
    icon_asset.starts_with("src/") && icon_asset.ends_with(".svg")
}

/// 统一的 Provider 图标渲染器
///
/// - SVG 路径 → `svg().path(icon)` 渲染，单色跟随 `text_color`
/// - 文本（首字母等）→ 粗体单色文字渲染，同样跟随 `text_color` 参数
///
/// 两种模式都以主题色单色渲染，视觉风格统一。
pub(crate) fn render_provider_icon(
    icon_asset: impl Into<String>,
    size: Pixels,
    color: Hsla,
) -> AnyElement {
    let icon = icon_asset.into();
    if is_svg_icon(&icon) {
        svg()
            .path(icon)
            .size(size)
            .text_color(color)
            .flex_shrink_0()
            .into_any_element()
    } else {
        // 文本模式：单色粗体，与 SVG 线框图标视觉重量一致
        // 字号约为容器的 65%，配合粗体实现与 SVG stroke-width 相近的视觉密度
        let font_size = size * 0.65;
        div()
            .w(size)
            .h(size)
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .text_size(font_size)
            .font_weight(FontWeight::BOLD)
            .text_color(color)
            .line_height(relative(1.0))
            .child(icon)
            .into_any_element()
    }
}

/// 带容器背景的 Provider 图标（用于 detail header 等需要大图标的场景）
///
/// 内置 SVG 图标：容器背景 + SVG 图标，与现有行为一致
/// 自定义文本图标：粗体单色文字 + 容器背景，视觉统一
pub(crate) fn render_provider_icon_boxed(
    icon_asset: impl Into<String>,
    container_size: Pixels,
    icon_size: Pixels,
    color: Hsla,
    bg: Hsla,
) -> Div {
    let icon = icon_asset.into();
    let base = div()
        .w(container_size)
        .h(container_size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.0))
        .bg(bg)
        .flex_shrink_0();

    if is_svg_icon(&icon) {
        base.child(svg().path(icon).size(icon_size).text_color(color))
    } else {
        // 文本大号显示，粗体单色
        let font_size = icon_size * 0.75;
        base.child(
            div()
                .text_size(font_size)
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .line_height(relative(1.0))
                .child(icon),
        )
    }
}
