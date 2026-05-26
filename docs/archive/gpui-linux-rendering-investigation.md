# GPUI Linux Rendering Investigation

## Summary

BananaTray exposes a set of rendering issues on Linux/Wayland that appear to be related to GPUI's Linux windowing and WGPU rendering backend rather than application-level state or layout logic.

The same UI state renders correctly after user interaction such as scrolling, while the initial frame or frames after window movement can miss non-text visual primitives. The missing pieces are standard GPUI-styled elements such as separators, rounded backgrounds, animated nav pill backgrounds, and progress bar fills.

This suggests a systemic issue in one or more of these areas:

- Linux Wayland/X11 window surface configuration
- Transparent surface / client-side decoration handling
- WGPU alpha mode and premultiplied alpha blending
- Frame callback / expose handling that presents a cached scene without forcing a full render
- Quad / gradient / clip primitive rendering under transparent compositing

## Affected environment

Observed environment from BananaTray logs:

- OS/session: Linux, Wayland
- GPU: NVIDIA GeForce GTX 1660 SUPER
- GPUI backend: WGPU
- Selected adapter: Vulkan
- Other adapters visible: OpenGL and llvmpipe
- Global hotkeys unavailable because Wayland session is active

Representative log lines:

```text
Global hotkeys not supported on Wayland
Selected GPU adapter: "NVIDIA GeForce GTX 1660 SUPER" (Vulkan)
gpui::platform::linux::platform activate is not implemented on Linux, ignoring the call
```

There is also an EGL warning during startup, although GPUI ultimately selects the Vulkan adapter:

```text
wgpu_hal::gles::egl EGL 'eglCreateSyncKHR' code 0x3004: EGL_BAD_ATTRIBUTE
```

## User-visible symptoms

### 1. Separator line disappears after window movement

A 1px horizontal separator between the navigation/tab area and the content area is initially visible, but disappears after moving the window.

The separator is rendered as a normal GPUI `div` with fixed height and solid background color, for example:

```rust
div().w_full().h(px(1.0)).bg(theme.border.subtle)
```

or as a GPUI border:

```rust
.border_b_1().border_color(theme.border.subtle)
```

Application-level experiments such as reserving 1px layout height or adding `flex_none()` did not change the behavior, which makes a pure flex/layout explanation unlikely.

### 2. Nav pill visual state is incomplete on Linux

The active navigation pill should have an animated rounded background. On Linux, the background/animation can be missing or incomplete.

The pill background is an absolutely positioned rounded `Div` with a solid background and animation:

```rust
div()
    .absolute()
    .top(px(0.0))
    .h(to_height)
    .rounded(px(8.0))
    .bg(bg)
    .with_animation(...)
```

### 3. Progress bar fill is missing until scroll/interact

The quota progress bar track may be visible while the colored fill/gradient is missing. After scrolling, the fill appears correctly.

The fill is a rounded clipped gradient quad with animated relative width:

```rust
div()
    .h_full()
    .rounded_full()
    .bg(multi_stop_linear_gradient(...))
    .with_animation(..., move |el, delta| {
        el.w(relative(delta * target_ratio))
    })
```

The data is correct. Logs show quota data arriving correctly and the UI becomes correct after a repaint-triggering interaction.

### 4. Other non-text backgrounds can be missing

Some cards, list backgrounds, row backgrounds, and border-like elements can appear only after scrolling or interaction.

Text and icons are usually visible, while backgrounds, gradients, borders, and fills are affected. This points toward non-text GPUI primitives such as quads/gradients/clips rather than application data.

### 5. Toggle switch renders as a white square — track background never appears

A standard pill-shaped toggle switch (44×24 rounded track with an 18×18 round knob) renders as a small ~18–24px white square on Linux. The track quad — both background fill and border — is completely invisible regardless of color.

What the user sees:

- **Disabled state**: an ~18×18 square (the knob), no surrounding pill at all.
- **Enabled state**: a slightly wider area is visible, but the colored track behind/around the knob is still missing. The width difference between disabled and enabled visually corresponds to the knob's left margin, suggesting only the knob primitive lands on screen.

What the implementation looks like:

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

Diagnostic steps already taken in BananaTray, none of which changed the symptom on Linux:

1. **Theme color contrast** — switched disabled track from `theme.bg.subtle` (#1c1c20, near-panel) to the much brighter `theme.border.strong` (#3f3f46). No visible change.
2. **Hardcoded extreme colors** — set track bg to `rgb(0x9333ea)` purple, border to `rgb(0xfbbf24)` yellow, knob to `rgb(0x000000)` black. The knob correctly turned black, but the **purple track and yellow border never appeared** — the visible area just became a black square. This is the strongest evidence that the track quad/border quad simply does not render.
3. **`flex_none()` on knob** — to prevent flex stretching of the cross axis. No visible change.
4. **`flex_none()` on outer track div** — to prevent the parent flex container (`trash button + toggle + refresh button` row) from shrinking the toggle. No visible change.
5. **Replaced `ml(travel)` with `justify_end()` / `justify_start()`** — to remove margin-based knob positioning. No visible change.
6. **Confirmed function is invoked correctly** — `eprintln!` debug logging shows `render_toggle_switch enabled=true width=44px height=24px knob=18px travel=22px` is emitted on every relevant render, with correct parameters and correct enabled/disabled toggling driven by user clicks.

So the layout primitives are running, the function is called with the right arguments, the knob quad renders, but the track's `bg(...)` and `border_1() / border_color(...)` quads never become visible.

This is consistent with the broader Linux symptom: text/icons render, simple solid quads render, but a rounded solid quad + 1px border quad combination (especially when used as a flex container holding other primitives) can drop on Linux/Wayland transparent CSD surfaces. The toggle is essentially a small-scale stress test of the same primitive set affected in symptoms 1–4 (rounded solid backgrounds, 1px borders, child quads positioned inside a rounded container).

Worth noting: this widget is also used in many other places in the settings UI (general tab cards, display tab toggles, provider quota visibility section, custom segmented controls). All of them exhibit the same "white square" pattern on Linux while looking correct on macOS — strong evidence the issue is platform-level, not widget-level.

## Why this is likely not application state or layout

The application state is already correct when the frame is visually wrong:

- Provider quota data is parsed and logged correctly.
- The Amp quota values are correct.
- The same UI becomes correct after scrolling without changing the underlying data.
- Toggling providers or refreshing data is not required for the missing visuals to appear.
- A pure `1px` flex/layout tweak did not fix the separator disappearance.

The decisive observation is that scrolling or interaction makes missing visuals appear. That points to frame invalidation, repaint, scene replay, surface presentation, alpha compositing, or primitive rendering rather than business logic.

## BananaTray window setup

BananaTray opens both settings and popup windows using GPUI `WindowOptions` with `titlebar: None` and otherwise mostly default options.

Settings window:

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

Tray popup:

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

`WindowOptions::default()` uses:

```rust
window_background: WindowBackgroundAppearance::Opaque,
window_decorations: None,
```

Despite the logical opaque background, the Wayland backend appears to initialize and often operate as a transparent surface when client-side decorations are used.

## Relevant GPUI backend observations

These observations are from a local fork of `adabraka-gpui`, but the paths and logic should map closely to the upstream GPUI Linux/WGPU backend.

### 1. Wayland initializes WGPU surface as transparent

File:

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

Relevant code:

```rust
let config = WgpuSurfaceConfig {
    size: options.bounds.to_device_pixels(1.0).size,
    transparent: true,
    preferred_present_mode: None,
};
WgpuRenderer::new(gpu_context, &raw_window, config, None)?
```

This means the Wayland window starts as a transparent WGPU surface regardless of the application window background option.

### 2. Client-side decorations force transparent behavior

File:

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

Relevant code:

```rust
pub fn is_transparent(&self) -> bool {
    self.decorations == WindowDecorations::Client
        || self.background_appearance != WindowBackgroundAppearance::Opaque
}
```

This is important because BananaTray uses `titlebar: None`, which typically leads to client-side decoration behavior. Even if `WindowBackgroundAppearance::Opaque` is requested, `decorations == Client` makes the surface transparent.

### 3. Opaque region is disabled for client-side decorations

File:

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

Relevant code:

```rust
if state.background_appearance == WindowBackgroundAppearance::Opaque
    && state.decorations == WindowDecorations::Server
{
    state.surface.set_opaque_region(Some(&region));
} else {
    state.surface.set_opaque_region(None);
}
```

For client-side decorations, no opaque region is promised to the compositor. Combined with transparent surface clearing, this makes the entire window dependent on correct full-scene redraw and alpha compositing.

### 4. WGPU renderer clears the surface to transparent every frame

File:

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
```

Relevant code:

```rust
ops: wgpu::Operations {
    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
    store: wgpu::StoreOp::Store,
},
```

This is fragile for a UI that visually expects an opaque application window. If some quads are missed, clipped, alpha-blended incorrectly, or not included in the presented scene, transparent background shows through.

### 5. Alpha mode changes pipeline blending

File:

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
```

Relevant code:

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

and:

```rust
let blend_mode = match alpha_mode {
    wgpu::CompositeAlphaMode::PreMultiplied => {
        wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
    }
    _ => wgpu::BlendState::ALPHA_BLENDING,
};
```

The quad shader also branches on `premultiplied_alpha`:

```wgsl
fn blend_color(color: vec4<f32>, alpha_factor: f32) -> vec4<f32> {
    let alpha = color.a * alpha_factor;
    let multiplier = select(1.0, alpha, globals.premultiplied_alpha != 0u);
    return vec4<f32>(color.rgb * multiplier, alpha);
}
```

A mismatch between surface alpha mode, pipeline blending, shader output, and compositor expectations could affect low-alpha quads, thin separators, gradients, and rounded backgrounds.

### 6. Frame/expose can present without full scene rebuild

File:

```text
crates/gpui/src/window.rs
```

Relevant scheduling logic:

```rust
if invalidator.is_dirty() || request_frame_options.force_render {
    window.draw(cx);
    window.present();
} else if needs_present {
    window.present();
}
```

`present()` draws the already rendered scene:

```rust
fn present(&self) {
    self.platform_window.draw(&self.rendered_frame.scene);
    self.needs_present.set(false);
}
```

Wayland frame callback normally does not force render:

```rust
fun(RequestFrameOptions {
    force_render,
    ..Default::default()
});
```

X11 expose also presents without forcing render:

```rust
window.refresh(RequestFrameOptions {
    require_presentation: expose_event_received,
    force_render: false,
});
```

This may be correct in principle, but it is a key area to inspect when cached scene presentation interacts with transparent surfaces, animations, relative widths, clip masks, or compositor expose/move events.

## Primitive categories affected

The affected visuals are mostly GPUI non-text primitives:

| Visual | Likely GPUI primitive path |
|--------|----------------------------|
| 1px separator | Solid background quad or border quad |
| Nav pill background | Rounded solid quad, absolute positioned, animated |
| Progress fill | Rounded clipped gradient quad with animated width |
| Card backgrounds | Rounded solid quads |
| List/row backgrounds | Solid quads |
| Borders | Quad border shader path |
| Toggle switch track | Rounded solid quad + 1px border quad with child content (acts as flex container) |

Text and icons tend to be more stable, suggesting the issue is not general frame failure but specific primitive/surface/compositing behavior.

## Proposed upstream investigation plan

### Experiment 1: Force opaque/server-side path

Goal: determine whether transparent/CSD surface handling is the primary trigger.

In the application or in a GPUI test app, create a window with:

```rust
WindowOptions {
    window_background: WindowBackgroundAppearance::Opaque,
    window_decorations: Some(WindowDecorations::Server),
    titlebar: None,
    ..Default::default()
}
```

Expected outcomes:

- If visuals stabilize, the root cause is likely transparent surface / CSD / opaque region / alpha compositing.
- If visuals still fail, investigate WGPU quad rendering or frame invalidation.

Important: on Wayland, server-side decorations may not be honored depending on compositor support. Log the actual runtime decoration mode.

### Experiment 2: Add runtime logging for surface state

Add logs around Wayland `update_window`, `draw`, and WGPU renderer alpha mode changes.

Suggested fields:

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

This should answer:

- Is the window actually running as transparent even when the app requested opaque?
- Does moving the window switch decoration or alpha mode?
- Are frames after movement just presenting old scenes?
- Does scrolling mark the window dirty and force a full render?

### Experiment 3: Temporarily clear WGPU surface to opaque color

Change the main render pass clear from transparent:

```rust
wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
```

to an opaque dark color:

```rust
wgpu::LoadOp::Clear(wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.025,
    a: 1.0,
})
```

This is not a proposed final fix. It is a diagnostic test.

Expected outcomes:

- If missing backgrounds/separators/progress fills become stable, the issue is strongly tied to transparent surface compositing.
- If nothing changes, focus on scene contents, quad shader, or frame scheduling.

### Experiment 4: Force render on Wayland frame callback / X11 expose

Temporarily change Wayland frame callback to always force render:

```rust
fun(RequestFrameOptions {
    force_render: true,
    ..Default::default()
});
```

Temporarily change X11 expose refresh to:

```rust
window.refresh(RequestFrameOptions {
    require_presentation: expose_event_received,
    force_render: true,
});
```

Expected outcomes:

- If the problem disappears, cached scene presentation is unsafe for these Linux expose/frame paths.
- If the problem persists, the root is more likely WGPU surface/alpha/quad rendering.

### Experiment 5: Build a minimal GPUI repro window

Create a standalone GPUI example with no BananaTray app state:

- `WindowOptions { titlebar: None, window_background: Opaque, ... }`
- Root rounded container with `overflow_hidden`
- Header
- 1px separator
- Scroll container
- Rounded card
- Gradient progress bar with `relative()` width
- Animated absolute rounded nav pill

Then test on Linux/Wayland:

1. Open the window.
2. Move it.
3. Scroll the content.
4. Switch tabs or trigger animation.
5. Compare first frame vs post-scroll frame.

This will isolate GPUI from BananaTray selectors, provider state, and application logic.

## Suggested minimal repro UI structure

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

Test with both:

```rust
window_decorations: Some(WindowDecorations::Client)
```

and:

```rust
window_decorations: Some(WindowDecorations::Server)
```

Also test transparent vs opaque background.

## Recommended areas for upstream to inspect

### Wayland window transparency lifecycle

File:

```text
crates/gpui/src/platform/linux/wayland/window.rs
```

Questions:

- Should Wayland windows start with `transparent: true` unconditionally?
- Should `WindowBackgroundAppearance::Opaque` override transparent mode even with CSD if the app content is fully opaque?
- Is `update_window()` called at all required times after decoration mode changes?
- Should opaque region be enabled for the content area even with CSD, excluding only rounded/inset decoration regions?

### WGPU alpha mode and blend mode

Files:

```text
crates/gpui/src/platform/wgpu/wgpu_renderer.rs
crates/gpui/src/platform/wgpu/shaders.wgsl
```

Questions:

- Are shader outputs and pipeline blending correct for `PreMultiplied`, `Inherit`, and `Opaque` alpha modes?
- Is `CompositeAlphaMode::Inherit` handled correctly, or does treating it like `ALPHA_BLENDING` mismatch compositor expectations?
- Is transparent clear correct for windows that logically requested opaque background?
- Should opaque windows clear to opaque black or an explicit background color instead of transparent?

### Frame scheduling and cached scene presentation

Files:

```text
crates/gpui/src/window.rs
crates/gpui/src/platform/linux/wayland/window.rs
crates/gpui/src/platform/linux/x11/client.rs
```

Questions:

- Are move/expose/frame events allowed to present cached scenes safely on Linux?
- Should Wayland configure/frame/expose paths force render after window move, surface reconfiguration, or decoration changes?
- Does scrolling force a full redraw while movement only presents cached content?
- Are animations and relative layout values captured correctly when a cached scene is presented later?

### Quad / rounded / gradient shader

Files:

```text
crates/gpui/src/platform/wgpu/shaders.wgsl
crates/gpui/src/scene.rs
crates/gpui/src/window.rs
```

Questions:

- Are 1px quads and borders robust under fractional scaling and transparent compositing?
- Do rounded quads with `overflow_hidden` generate masks that can incorrectly clip thin children?
- Are gradient quads with very small heights and relative widths handled consistently?
- Do sorted quad batches preserve correct order across scene replay?

## Why application-level fixes are unlikely to be sufficient

Application-level changes such as adding `flex_none`, reserving 1px, or triggering delayed repaint can reduce symptoms but do not address the root cause.

The affected UI elements are ordinary GPUI components:

- solid `Div` backgrounds
- `border_1` / `border_b_1`
- rounded backgrounds
- gradient backgrounds
- scroll containers
- animations

These are framework-level primitives. A Linux backend should render them reliably without each app adding repaint workarounds or avoiding standard composition patterns.

## Desired upstream outcome

A robust fix should make ordinary GPUI UI primitives reliable on Linux/Wayland/X11 under these conditions:

- opaque application window with custom titlebar or `titlebar: None`
- rounded root containers
- scroll containers
- 1px separators and borders
- rounded gradient progress bars
- animated absolute-positioned backgrounds
- window movement, expose, resize, and scrolling

The final fix may involve one or more of:

- honoring opaque window background more strictly on Linux
- avoiding transparent WGPU surfaces unless explicitly requested
- improving Wayland opaque region handling for CSD windows
- fixing alpha mode / premultiplied alpha behavior
- forcing full render on specific Linux surface lifecycle events
- fixing quad/gradient/clip shader edge cases

## Short issue title suggestion

```text
Linux/Wayland WGPU backend intermittently drops non-text quad/gradient primitives on transparent CSD windows
```

## Short issue summary suggestion

```text
On Linux/Wayland, GPUI windows using titlebar: None / CSD and WGPU rendering can intermittently lose non-text primitives such as 1px separators, rounded backgrounds, animated pill backgrounds, and gradient progress fills. The same UI state becomes correct after scroll/interact, suggesting a backend issue involving transparent surfaces, alpha compositing, frame callback/expose presentation, scene replay, or quad/gradient rendering rather than application state.
```
