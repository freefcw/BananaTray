//! theme 模块的单元测试 — GPUI-free
//!
//! 测试 `parse_color`、YAML 加载、`color()` 提取等纯逻辑。

#[cfg(test)]
mod tests {
    use crate::theme::{color, parse_color, LIGHT_YAML};

    #[test]
    fn parse_color_rgb6() {
        let c = parse_color("#ff8800");
        assert!(c.a > 0.99, "alpha should be ~1.0 for #RRGGBB");
    }

    #[test]
    fn parse_color_rgba8() {
        let c = parse_color("#ff880080");
        // alpha = 0x80 / 255 ≈ 0.502
        assert!(
            (c.a - 0.502).abs() < 0.01,
            "alpha should be ~0.5 for #..80, got {}",
            c.a
        );
    }

    #[test]
    fn parse_color_without_hash() {
        let c = parse_color("2563eb");
        assert!(c.a > 0.99);
    }

    #[test]
    #[should_panic(expected = "invalid color")]
    fn parse_color_invalid_hex() {
        parse_color("#gggggg");
    }

    #[test]
    #[should_panic(expected = "invalid color format")]
    fn parse_color_wrong_length() {
        parse_color("#fff");
    }

    #[test]
    fn light_theme_loads() {
        let theme = crate::theme::Theme::light();
        assert!(
            theme.bg.panel.l > 0.9,
            "light panel lightness should be > 0.9, got {}",
            theme.bg.panel.l
        );
    }

    #[test]
    fn dark_theme_loads() {
        let theme = crate::theme::Theme::dark();
        assert!(
            theme.bg.base.l < 0.1,
            "dark base lightness should be < 0.1, got {}",
            theme.bg.base.l
        );
    }

    #[test]
    fn light_and_dark_differ() {
        let light = crate::theme::Theme::light();
        let dark = crate::theme::Theme::dark();
        assert_ne!(
            light.bg.base.l, dark.bg.base.l,
            "light and dark should have different base lightness"
        );
    }

    #[test]
    fn color_extractor_valid_key() {
        let v: serde_yaml::Value = serde_yaml::from_str(LIGHT_YAML).unwrap();
        let c = color(&v, "bg", "base");
        // #ffffff → lightness = 1.0
        assert!((c.l - 1.0).abs() < 0.01, "bg.base in light should be white");
    }

    #[test]
    #[should_panic(expected = "missing theme color")]
    fn color_extractor_missing_key() {
        let v: serde_yaml::Value = serde_yaml::from_str(LIGHT_YAML).unwrap();
        color(&v, "bg", "nonexistent_key");
    }
}
