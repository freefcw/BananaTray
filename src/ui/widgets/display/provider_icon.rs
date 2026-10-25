use gpui::{
    div, px, relative, svg, AnyElement, Div, FontWeight, Hsla, IntoElement, ParentElement, Pixels,
    Styled,
};

/// 判断 icon_asset 是否为 SVG 文件路径
///
/// 内置 Provider 使用打包路径 "src/icons/provider-xxx.svg"；自定义 Provider
/// 既可用首字母文本（如 "N", "月"），也可指向任意本地 .svg 文件
/// （见 `providers::custom::descriptor::resolve_icon_asset`）。
pub(crate) fn is_svg_icon(icon_asset: &str) -> bool {
    icon_asset.ends_with(".svg")
}

/// 字母徽章比例（相对块尺寸）：镂空字号 / 圆角 / 基线下移补偿
const BADGE_FONT_RATIO: f32 = 0.85;
const BADGE_CORNER_RATIO: f32 = 0.28;
/// 文本按基线排版，em 框居中时大写字形偏上（descender 空间在基线下方），
/// margin-top 下移补偿让 cap-height 在块内光学居中
const BADGE_BASELINE_OFFSET_RATIO: f32 = 0.18;

/// 镂空字母徽章：实色圆角块 + 以 `surface` 底色绘制的字母模拟挖空。
/// `surface` 必须等于徽章所在的实际背景色，镂空效果才成立。
fn letter_badge(icon: String, size: Pixels, color: Hsla, surface: Hsla) -> Div {
    div()
        .w(size)
        .h(size)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(size * BADGE_CORNER_RATIO)
        .bg(color)
        .text_size(size * BADGE_FONT_RATIO)
        .font_weight(FontWeight::BOLD)
        .text_color(surface)
        .line_height(relative(1.0))
        .child(div().mt(size * BADGE_BASELINE_OFFSET_RATIO).child(icon))
}

/// 统一的 Provider 图标渲染器
///
/// - SVG 路径 → `svg().path(icon)` 渲染，单色跟随 `text_color`
/// - 文本（首字母等）→ 镂空字母徽章（见 `letter_badge`），
///   与实心品牌剪影图标的块面重量对齐；`surface` 传图标所在的实际背景色
///
/// SVG 模式以主题色单色渲染，徽章模式以主题色实块呈现，视觉风格统一。
pub(crate) fn render_provider_icon(
    icon_asset: impl Into<String>,
    size: Pixels,
    color: Hsla,
    surface: Hsla,
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
        letter_badge(icon, size, color, surface).into_any_element()
    }
}

/// 带容器背景的 Provider 图标（用于 detail header 等需要大图标的场景）
///
/// 内置 SVG 图标：容器背景 + SVG 图标，与现有行为一致
/// 自定义文本图标：容器内嵌实色 chip + 镂空字母（`bg` 色），
/// 与 `render_provider_icon` 的徽章样式一致
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
        base.child(letter_badge(icon, icon_size, color, bg))
    }
}
