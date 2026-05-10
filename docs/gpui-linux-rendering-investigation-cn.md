# GPUI Linux 渲染问题调查

## 摘要

BananaTray 在 Linux/Wayland 上暴露出一组渲染问题。这些问题看起来与 GPUI 的 Linux 窗口系统和 WGPU 渲染后端有关，而不是应用层状态或布局逻辑导致的。

同一份 UI 状态在用户滚动等交互之后可以正确渲染，但初始帧，或窗口移动后的若干帧，可能会缺失非文本视觉图元。缺失的部分都是标准的 GPUI 样式元素，例如分隔线、圆角背景、带动画的导航 pill 背景，以及进度条填充。

这表明问题很可能系统性地出现在以下一个或多个领域：

- Linux Wayland/X11 窗口 surface 配置
- 透明 surface / 客户端装饰处理
- WGPU alpha mode 与预乘 alpha 混合
- frame callback / expose 处理：在没有强制完整渲染的情况下呈现了缓存 scene
- 透明合成下的 quad / gradient / clip 图元渲染

## 受影响环境

从 BananaTray 日志观察到的环境：

- OS/session：Linux，Wayland
- GPU：NVIDIA GeForce GTX 1660 SUPER
- GPUI backend：WGPU
- 选中的 adapter：Vulkan
- 可见的其他 adapter：OpenGL 和 llvmpipe
- 由于处于 Wayland session，全局快捷键不可用

代表性日志：

```text
Global hotkeys not supported on Wayland
Selected GPU adapter: "NVIDIA GeForce GTX 1660 SUPER" (Vulkan)
gpui::platform::linux::platform activate is not implemented on Linux, ignoring the call
```

启动期间还会出现一条 EGL 警告，虽然 GPUI 最终选择的是 Vulkan adapter：

```text
wgpu_hal::gles::egl EGL 'eglCreateSyncKHR' code 0x3004: EGL_BAD_ATTRIBUTE
```

## 用户可见症状

### 1. 窗口移动后分隔线消失

导航/tab 区域和内容区域之间的 1px 水平分隔线最初可见，但移动窗口之后会消失。

分隔线是用普通 GPUI `div` 渲染的，带固定高度和纯色背景，例如：

```rust
div().w_full().h(px(1.0)).bg(theme.border.subtle)
```

或者使用 GPUI border：

```rust
.border_b_1().border_color(theme.border.subtle)
```

应用层实验，例如预留 1px 布局高度或添加 `flex_none()`，没有改变这个行为。因此，纯 flex/layout 解释不太可能成立。

### 2. Linux 上导航 pill 的视觉状态不完整

激活的导航 pill 应该带有动画圆角背景。在 Linux 上，这个背景/动画可能缺失或不完整。

pill 背景是一个绝对定位的圆角 `Div`，带纯色背景和动画：

```rust
div()
    .absolute()
    .top(px(0.0))
    .h(to_height)
    .rounded(px(8.0))
    .bg(bg)
    .with_animation(...)
```

### 3. 进度条填充在滚动/交互前缺失

配额进度条的 track 可能可见，但彩色填充/渐变缺失。滚动之后，填充会正确出现。

填充是一个圆角裁剪的渐变 quad，带动画相对宽度：

```rust
div()
    .h_full()
    .rounded_full()
    .bg(multi_stop_linear_gradient(...))
    .with_animation(..., move |el, delta| {
        el.w(relative(delta * target_ratio))
    })
```

数据是正确的。日志显示配额数据正确到达，并且触发 repaint 的交互之后 UI 会变得正确。

### 4. 其他非文本背景也可能缺失

一些卡片、列表背景、行背景和类似 border 的元素，可能只有在滚动或交互之后才出现。

文本和图标通常可见，而背景、渐变、border 和填充会受到影响。这说明问题更可能出在非文本 GPUI 图元，例如 quads/gradients/clips，而不是应用数据。

### 5. Toggle switch 渲染成白色方块：track 背景始终不出现

标准 pill 形 toggle switch（44×24 圆角 track，18×18 圆形 knob）在 Linux 上渲染成一个约 18–24px 的白色小方块。track quad，包括背景填充和 border，无论使用什么颜色都完全不可见。

用户看到的现象：

- **禁用状态**：一个约 18×18 的方块（knob），完全没有外围 pill。
- **启用状态**：可见区域略宽，但 knob 后面/周围的彩色 track 仍然缺失。禁用和启用状态之间的宽度差异，在视觉上对应 knob 的左边距，说明只有 knob 图元落到了屏幕上。

实现大致如下：

```rust
div()
    .flex_none()
    .w(px(44.0))
    .h(px(24.0))
    .flex()
    .items_center()
    .px(px(2.0))
    .rounded_full()
    .bg(track_bg)            // e.g. theme.element.selected (#3b82f6) or theme.bg.subtle
    .border_1()
    .border_color(track_border)
    .child(
        div()
            .flex_none()
            .w(px(18.0))
            .h(px(18.0))
            .rounded_full()
            .bg(theme.element.active) // white knob
            // either: .ml(travel)            (margin-based positioning)
            // or:     parent .justify_end()  (flex justify-based positioning)
    )
```

BananaTray 已经做过以下诊断步骤，但没有任何一步改变 Linux 上的症状：

1. **主题颜色对比度**：将禁用 track 从 `theme.bg.subtle`（#1c1c20，接近 panel 颜色）切换为亮很多的 `theme.border.strong`（#3f3f46）。没有可见变化。
2. **硬编码极端颜色**：将 track bg 设为 `rgb(0x9333ea)` 紫色，border 设为 `rgb(0xfbbf24)` 黄色，knob 设为 `rgb(0x000000)` 黑色。knob 正确变黑，但 **紫色 track 和黄色 border 从未出现**，可见区域只是变成了黑色方块。这是最强证据，说明 track quad/border quad 根本没有渲染出来。
3. **在 knob 上使用 `flex_none()`**：防止 flex 在交叉轴上拉伸。没有可见变化。
4. **在外层 track div 上使用 `flex_none()`**：防止父 flex 容器（`trash button + toggle + refresh button` 行）收缩 toggle。没有可见变化。
5. **将 `ml(travel)` 替换为 `justify_end()` / `justify_start()`**：移除基于 margin 的 knob 定位。没有可见变化。
6. **确认函数被正确调用**：`eprintln!` 调试日志显示，在每次相关 render 中都会输出 `render_toggle_switch enabled=true width=44px height=24px knob=18px travel=22px`，参数正确，并且启用/禁用状态会由用户点击正确驱动。

因此，布局图元确实在运行，函数调用参数正确，knob quad 可以渲染，但 track 的 `bg(...)` 和 `border_1() / border_color(...)` quad 始终不可见。

这与更广泛的 Linux 症状一致：文本/图标可以渲染，简单纯色 quad 可以渲染，但圆角纯色 quad + 1px border quad 组合，尤其当它作为 flex 容器持有其他图元时，可能会在 Linux/Wayland 透明 CSD surface 上丢失。toggle 本质上是症状 1–4 中受影响图元集合的小型压力测试：圆角纯色背景、1px border、圆角容器内部定位的子 quad。

值得注意的是，这个 widget 也在设置 UI 的很多其他位置使用，例如 general tab cards、display tab toggles、provider quota visibility section、自定义 segmented controls。它们在 Linux 上都表现出相同的“白色方块”模式，而在 macOS 上看起来正确。这强烈说明问题在平台层，而不是 widget 层。

## 为什么这很可能不是应用状态或布局问题

当帧在视觉上出错时，应用状态已经是正确的：

- Provider 配额数据被正确解析并记录到日志。
- Amp 配额值是正确的。
- 不改变底层数据，只要滚动，同一个 UI 就会变正确。
- 不需要切换 provider 或刷新数据，缺失的视觉元素也会出现。
- 纯 `1px` flex/layout 调整没有修复分隔线消失问题。

决定性的观察是：滚动或交互会让缺失的视觉元素出现。这指向 frame invalidation、repaint、scene replay、surface presentation、alpha compositing 或图元渲染，而不是业务逻辑。

## BananaTray 窗口设置

BananaTray 使用 GPUI `WindowOptions` 打开设置窗口和 popup 窗口，两者都设置了 `titlebar: None`，其余大多使用默认选项。

设置窗口：

```rust
WindowOptions {
    window_bounds: Some(window_bounds),
    window_min_size: Some(size(px(460.0), px(520.0))),
    titlebar: None,
    kind: WindowKind::Normal,
    display_id: target_display_id,
    ..Default::default()
}
```

托盘 popup：

```rust
WindowOptions {
    titlebar: None,
    kind,
    focus: true,
    show: true,
    window_bounds: Some(WindowBounds::Windowed(bounds)),
    display_id,
    ..Default::default()
}
```

`WindowOptions::default()` 使用：

```rust
window_background: WindowBackgroundAppearance::Opaque,
window_decorations: None,
```

尽管逻辑上是 opaque background，但当使用客户端装饰时，Wayland backend 看起来会将窗口初始化为透明 surface，并且很多时候也以透明 surface 方式运行。

## 相关 GPUI backend 观察

这些观察来自本地 `adabraka-gpui` fork，但路径和逻辑应该与上游 GPUI Linux/WGPU backend 高度对应。

### 1. Wayland 将 WGPU surface 初始化为透明

文件：

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

相关代码：

```rust
let config = WgpuSurfaceConfig {
    size: options.bounds.to_device_pixels(1.0).size,
    transparent: true,
    preferred_present_mode: None,
};
WgpuRenderer::new(gpu_context, &raw_window, config, None)?
```

这意味着无论应用的窗口背景选项如何，Wayland 窗口都会以透明 WGPU surface 启动。

### 2. 客户端装饰会强制透明行为

文件：

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

相关代码：

```rust
pub fn is_transparent(&self) -> bool {
    self.decorations == WindowDecorations::Client
        || self.background_appearance != WindowBackgroundAppearance::Opaque
}
```

这一点很重要，因为 BananaTray 使用 `titlebar: None`，这通常会导致客户端装饰行为。即使请求了 `WindowBackgroundAppearance::Opaque`，只要 `decorations == Client`，surface 就会被视为透明。

### 3. 客户端装饰会禁用 opaque region

文件：

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

相关代码：

```rust
if state.background_appearance == WindowBackgroundAppearance::Opaque
    && state.decorations == WindowDecorations::Server
{
    state.surface.set_opaque_region(Some(&region));
} else {
    state.surface.set_opaque_region(None);
}
```

对于客户端装饰，程序不会向 compositor 承诺 opaque region。再结合透明 surface clear，整个窗口都依赖正确的完整 scene redraw 和 alpha compositing。

### 4. WGPU renderer 每帧都会将 surface clear 为透明

文件：

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
```

相关代码：

```rust
ops: wgpu::Operations {
    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    store: wgpu::StoreOp::Store,
},
```

对于一个视觉上期望 opaque 应用窗口的 UI 来说，这很脆弱。如果某些 quad 被遗漏、裁剪错误、alpha 混合错误，或没有包含在被呈现的 scene 中，透明背景就会透出来。

### 5. Alpha mode 会改变 pipeline blending

文件：

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
```

相关代码：

```rust
let transparent_alpha_mode = pick_alpha_mode(&[
    wgpu::CompositeAlphaMode::PreMultiplied,
    wgpu::CompositeAlphaMode::Inherit,
])?;

let opaque_alpha_mode = pick_alpha_mode(&[
    wgpu::CompositeAlphaMode::Opaque,
    wgpu::CompositeAlphaMode::Inherit,
])?;
```

以及：

```rust
let blend_mode = match alpha_mode {
    wgpu::CompositeAlphaMode::PreMultiplied => {
        wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
    _ => wgpu::BlendState::ALPHA_BLENDING,
};
```

quad shader 也会根据 `premultiplied_alpha` 分支：

```wgsl
fn blend_color(color: vec4<f32>, alpha_factor: f32) -> vec4<f32> {
    let alpha = color.a * alpha_factor;
    let multiplier = select(1.0, alpha, globals.premultiplied_alpha != 0u);
    return vec4<f32>(color.rgb * multiplier, alpha);
}
```

surface alpha mode、pipeline blending、shader output 和 compositor 预期之间如果不匹配，可能会影响低 alpha quad、细分隔线、渐变和圆角背景。

### 6. Frame/expose 可能在不完整重建 scene 的情况下 present

文件：

```text
crates/gpui/src/window.rs
```

相关调度逻辑：

```rust
if invalidator.is_dirty() || request_frame_options.force_render {
    window.draw(cx);
    window.present();
} else if needs_present {
    window.present();
}
```

`present()` 会绘制已经渲染好的 scene：

```rust
fn present(&self) {
    self.platform_window.draw(&self.rendered_frame.scene);
    self.needs_present.set(false);
}
```

Wayland frame callback 通常不会强制 render：

```rust
fun(RequestFrameOptions {
    force_render,
    ..Default::default()
});
```

X11 expose 也会在不强制 render 的情况下 present：

```rust
window.refresh(RequestFrameOptions {
    require_presentation: expose_event_received,
    force_render: false,
});
```

原则上这可能是正确的，但当缓存 scene presentation 与透明 surface、动画、相对宽度、clip mask 或 compositor expose/move 事件交互时，这是一个需要重点检查的区域。

## 受影响的图元类别

受影响的视觉元素大多是 GPUI 非文本图元：

| 视觉元素 | 可能的 GPUI primitive path |
|--------|----------------------------|
| 1px 分隔线 | 纯色背景 quad 或 border quad |
| 导航 pill 背景 | 圆角纯色 quad、绝对定位、动画 |
| 进度条填充 | 带动画宽度的圆角裁剪渐变 quad |
| 卡片背景 | 圆角纯色 quad |
| 列表/行背景 | 纯色 quad |
| Borders | Quad border shader path |
| Toggle switch track | 圆角纯色 quad + 带子内容的 1px border quad（作为 flex 容器） |

文本和图标往往更稳定，说明问题不是通用的帧失败，而是特定的图元/surface/合成行为。

## 建议的上游调查计划

### 实验 1：强制 opaque/server-side 路径

目标：判断 transparent/CSD surface 处理是否是主要触发因素。

在应用中或 GPUI 测试应用中，创建一个带以下配置的窗口：

```rust
WindowOptions {
    window_background: WindowBackgroundAppearance::Opaque,
    window_decorations: Some(WindowDecorations::Server),
    titlebar: None,
    ..Default::default()
}
```

预期结果：

- 如果视觉元素稳定，根因很可能是 transparent surface / CSD / opaque region / alpha compositing。
- 如果视觉元素仍然失败，继续调查 WGPU quad 渲染或 frame invalidation。

重要：在 Wayland 上，server-side decorations 是否生效取决于 compositor 支持。需要记录实际运行时 decoration mode。

### 实验 2：为 surface 状态添加运行时日志

在 Wayland `update_window`、`draw` 和 WGPU renderer alpha mode 变化附近添加日志。

建议字段：

```text
decorations
background_appearance
is_transparent()
surface_config.alpha_mode
surface_config.format
present_mode
opaque_region enabled/disabled
window size / surface size / scale factor
force_render
require_presentation
invalidator.is_dirty()
```

这应该能够回答：

- 即使应用请求 opaque，窗口实际是否仍以 transparent 运行？
- 移动窗口是否会切换 decoration 或 alpha mode？
- 移动之后的帧是否只是 present 旧 scene？
- 滚动是否会把窗口标记为 dirty 并强制完整 render？

### 实验 3：临时将 WGPU surface clear 为 opaque 颜色

将主 render pass clear 从透明：

```rust
wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
```

改成 opaque 深色：

```rust
wgpu::LoadOp::Clear(wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.025,
    a: 1.0,
})
```

这不是建议的最终修复，只是诊断测试。

预期结果：

- 如果缺失的背景/分隔线/进度条填充变稳定，说明问题与 transparent surface compositing 强相关。
- 如果没有变化，则重点关注 scene contents、quad shader 或 frame scheduling。

### 实验 4：在 Wayland frame callback / X11 expose 上强制 render

临时将 Wayland frame callback 改为始终强制 render：

```rust
fun(RequestFrameOptions {
    force_render: true,
    ..Default::default()
});
```

临时将 X11 expose refresh 改为：

```rust
window.refresh(RequestFrameOptions {
    require_presentation: expose_event_received,
    force_render: true,
});
```

预期结果：

- 如果问题消失，说明 cached scene presentation 对这些 Linux expose/frame 路径来说不安全。
- 如果问题仍然存在，根因更可能是 WGPU surface/alpha/quad rendering。

### 实验 5：构建一个最小 GPUI 复现窗口

创建一个独立 GPUI 示例，不包含 BananaTray 应用状态：

- `WindowOptions { titlebar: None, window_background: Opaque, ... }`
- 带 `overflow_hidden` 的根圆角容器
- Header
- 1px 分隔线
- 滚动容器
- 圆角卡片
- 使用 `relative()` 宽度的渐变进度条
- 带动画的绝对定位圆角导航 pill

然后在 Linux/Wayland 上测试：

1. 打开窗口。
2. 移动窗口。
3. 滚动内容。
4. 切换 tab 或触发动画。
5. 对比首帧和滚动后的帧。

这会将 GPUI 与 BananaTray selectors、provider state 和应用逻辑隔离开。

## 建议的最小复现 UI 结构

```rust
div()
    .flex()
    .flex_col()
    .size_full()
    .bg(theme_bg)
    .rounded(px(14.0))
    .overflow_hidden()
    .child(header)
    .child(tab_bar)
    .child(div().w_full().h(px(1.0)).bg(border_color))
    .child(
        div()
            .flex_col()
            .h_full()
            .overflow_y_scroll()
            .child(
                div()
                    .rounded(px(12.0))
                    .bg(card_bg)
                    .border_1()
                    .border_color(border_color)
                    .child(
                        div()
                            .w_full()
                            .h(px(5.0))
                            .bg(track_color)
                            .rounded_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(multi_stop_linear_gradient(...))
                                    .w(relative(0.7))
                            )
                    )
            )
    )
```

同时测试：

```rust
window_decorations: Some(WindowDecorations::Client)
```

和：

```rust
window_decorations: Some(WindowDecorations::Server)
```

也要测试 transparent 与 opaque background。

## 建议上游重点检查的区域

### Wayland 窗口透明度生命周期

文件：

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

问题：

- Wayland 窗口是否应该无条件以 `transparent: true` 启动？
- 如果应用内容完全 opaque，即使使用 CSD，`WindowBackgroundAppearance::Opaque` 是否也应该覆盖 transparent mode？
- decoration mode 变化之后，`update_window()` 是否在所有必要时机被调用？
- 即使使用 CSD，是否也应该为 content area 启用 opaque region，只排除圆角/内缩的 decoration 区域？

### WGPU alpha mode 与 blend mode

文件：

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
crates/gpui/src/platform/wgpu/shaders.wgsl
```

问题：

- 对 `PreMultiplied`、`Inherit` 和 `Opaque` alpha mode 来说，shader output 和 pipeline blending 是否正确？
- `CompositeAlphaMode::Inherit` 是否被正确处理，还是把它当作 `ALPHA_BLENDING` 会与 compositor 预期不匹配？
- 对逻辑上请求 opaque background 的窗口来说，transparent clear 是否正确？
- Opaque 窗口是否应该 clear 为 opaque black 或显式背景色，而不是 transparent？

### Frame scheduling 与 cached scene presentation

文件：

```text
crates/gpui/src/window.rs
crates/gpui/src/platform/linux/wayland/window.rs
crates/gpui/src/platform/linux/x11/client.rs
```

问题：

- 在 Linux 上，move/expose/frame 事件是否可以安全地 present cached scene？
- Wayland configure/frame/expose 路径是否应该在窗口移动、surface reconfiguration 或 decoration 变化之后强制 render？
- 滚动是否会强制完整 redraw，而移动窗口只会 present cached content？
- 当 cached scene 稍后被 present 时，动画和相对布局值是否被正确捕获？

### Quad / rounded / gradient shader

文件：

```text
crates/gpui/src/platform/wgpu/shaders.wgsl
crates/gpui/src/scene.rs
crates/gpui/src/window.rs
```

问题：

- 1px quads 和 borders 在 fractional scaling 与 transparent compositing 下是否足够健壮？
- 带 `overflow_hidden` 的圆角 quads 是否会生成可能错误裁剪细子元素的 mask？
- 高度很小、使用相对宽度的 gradient quads 是否处理一致？
- 排序后的 quad batches 在 scene replay 时是否保持正确顺序？

## 为什么应用层修复不太可能足够

应用层改动，例如添加 `flex_none`、预留 1px 或触发延迟 repaint，可能缓解症状，但不能解决根因。

受影响的 UI 元素都是普通 GPUI 组件：

- 纯色 `Div` backgrounds
- `border_1` / `border_b_1`
- 圆角背景
- 渐变背景
- 滚动容器
- 动画

这些都是框架级 primitive。Linux backend 应该可靠地渲染它们，而不需要每个应用添加 repaint workaround 或避开标准组合模式。

## 期望的上游结果

稳健的修复应该让普通 GPUI UI primitive 在 Linux/Wayland/X11 的以下条件下可靠工作：

- 带自定义 titlebar 或 `titlebar: None` 的 opaque 应用窗口
- 圆角根容器
- 滚动容器
- 1px 分隔线和 border
- 圆角渐变进度条
- 带动画的绝对定位背景
- 窗口移动、expose、resize 和 scrolling

最终修复可能涉及以下一项或多项：

- 在 Linux 上更严格地遵守 opaque window background
- 除非显式请求，否则避免使用 transparent WGPU surfaces
- 改进 CSD 窗口的 Wayland opaque region 处理
- 修复 alpha mode / premultiplied alpha 行为
- 在特定 Linux surface 生命周期事件上强制完整 render
- 修复 quad/gradient/clip shader 边界场景

## 简短 issue 标题建议

```text
Linux/Wayland WGPU backend intermittently drops non-text quad/gradient primitives on transparent CSD windows
```

## 简短 issue 摘要建议

```text
On Linux/Wayland, GPUI windows using titlebar: None / CSD and WGPU rendering can intermittently lose non-text primitives such as 1px separators, rounded backgrounds, animated pill backgrounds, and gradient progress fills. The same UI state becomes correct after scroll/interact, suggesting a backend issue involving transparent surfaces, alpha compositing, frame callback/expose presentation, scene replay, or quad/gradient rendering rather than application state.
```
