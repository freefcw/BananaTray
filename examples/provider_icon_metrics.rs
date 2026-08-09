use resvg::{tiny_skia, usvg};
use std::path::{Path, PathBuf};

const ICON_SIZE: f32 = 24.0;
const SCALE: u32 = 16;
const PADDING: u32 = 4 * SCALE;
const CANVAS_SIZE: u32 = 32 * SCALE;
const ALPHA_THRESHOLD: u8 = 8;
const MIN_RECOMMENDED_MARGIN: f32 = 2.0;
const MIN_RECOMMENDED_EDGE: f32 = 13.0;
const MAX_RECOMMENDED_EDGE: f32 = 20.0;
const MAX_CENTER_OFFSET: f32 = 1.5;

#[derive(Debug)]
struct OpticalMetrics {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    ink_area: f32,
}

impl OpticalMetrics {
    fn width(&self) -> f32 {
        self.max_x - self.min_x
    }

    fn height(&self) -> f32 {
        self.max_y - self.min_y
    }

    fn longest_edge(&self) -> f32 {
        self.width().max(self.height())
    }

    fn minimum_margin(&self) -> f32 {
        self.min_x
            .min(self.min_y)
            .min(ICON_SIZE - self.max_x)
            .min(ICON_SIZE - self.max_y)
    }

    fn center_offset(&self) -> (f32, f32) {
        (
            (self.min_x + self.max_x) / 2.0 - ICON_SIZE / 2.0,
            (self.min_y + self.max_y) / 2.0 - ICON_SIZE / 2.0,
        )
    }

    fn is_clipped(&self) -> bool {
        self.min_x < -0.05
            || self.min_y < -0.05
            || self.max_x > ICON_SIZE + 0.05
            || self.max_y > ICON_SIZE + 0.05
    }
}

fn measure_svg(data: &[u8]) -> Result<OpticalMetrics, String> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default())
        .map_err(|error| format!("cannot parse SVG: {error}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(CANVAS_SIZE, CANVAS_SIZE)
        .ok_or_else(|| "cannot allocate audit canvas".to_string())?;
    let transform = tiny_skia::Transform::from_row(
        SCALE as f32,
        0.0,
        0.0,
        SCALE as f32,
        PADDING as f32,
        PADDING as f32,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let mut min_x = CANVAS_SIZE;
    let mut min_y = CANVAS_SIZE;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut visible_pixels = 0_u32;
    let mut alpha_sum = 0.0_f32;

    for (index, pixel) in pixmap.data().chunks_exact(4).enumerate() {
        let alpha = pixel[3];
        alpha_sum += f32::from(alpha) / 255.0;
        if alpha < ALPHA_THRESHOLD {
            continue;
        }

        let x = index as u32 % CANVAS_SIZE;
        let y = index as u32 / CANVAS_SIZE;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        visible_pixels += 1;
    }

    if visible_pixels == 0 {
        return Err("renders no visible pixels".to_string());
    }

    let to_icon_units = |pixel: u32| (pixel as f32 - PADDING as f32) / SCALE as f32;
    Ok(OpticalMetrics {
        min_x: to_icon_units(min_x),
        min_y: to_icon_units(min_y),
        max_x: to_icon_units(max_x + 1),
        max_y: to_icon_units(max_y + 1),
        ink_area: alpha_sum / (SCALE * SCALE) as f32,
    })
}

fn measure_file(path: &Path) -> Result<OpticalMetrics, String> {
    let data = std::fs::read(path).map_err(|error| format!("cannot read file: {error}"))?;
    measure_svg(&data)
}

fn warning_reasons(metrics: &OpticalMetrics) -> Vec<String> {
    let mut warnings = Vec::new();
    let margin = metrics.minimum_margin();
    let longest_edge = metrics.longest_edge();
    let (center_x, center_y) = metrics.center_offset();

    if margin < MIN_RECOMMENDED_MARGIN {
        warnings.push(format!("margin {margin:.2} < {MIN_RECOMMENDED_MARGIN:.1}"));
    }
    if !(MIN_RECOMMENDED_EDGE..=MAX_RECOMMENDED_EDGE).contains(&longest_edge) {
        warnings.push(format!(
            "longest edge {longest_edge:.2} outside {MIN_RECOMMENDED_EDGE:.1}-{MAX_RECOMMENDED_EDGE:.1}"
        ));
    }
    if center_x.abs() > MAX_CENTER_OFFSET || center_y.abs() > MAX_CENTER_OFFSET {
        warnings.push(format!(
            "center offset ({center_x:+.2}, {center_y:+.2}) exceeds {MAX_CENTER_OFFSET:.1}"
        ));
    }

    warnings
}

fn audit(paths: &[PathBuf]) -> Result<(), usize> {
    let mut failures = 0;
    let mut warnings = 0;

    println!(
        "{:<28} {:>13} {:>13} {:>15} {:>9}",
        "icon", "bounds", "size", "center offset", "ink"
    );

    for path in paths {
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("<invalid>");
        match measure_file(path) {
            Ok(metrics) => {
                let (center_x, center_y) = metrics.center_offset();
                println!(
                    "{name:<28} {:>5.2},{:>5.2} {:>5.2}x{:>5.2} ({center_x:+.2},{center_y:+.2}) {:>7.2}",
                    metrics.min_x,
                    metrics.min_y,
                    metrics.width(),
                    metrics.height(),
                    metrics.ink_area,
                );

                if metrics.is_clipped() {
                    eprintln!("ERROR: {} paints outside the 24x24 canvas", path.display());
                    failures += 1;
                    continue;
                }

                let reasons = warning_reasons(&metrics);
                if !reasons.is_empty() {
                    eprintln!("WARNING: {name}: {}", reasons.join("; "));
                    warnings += 1;
                }
            }
            Err(error) => {
                eprintln!("ERROR: {}: {error}", path.display());
                failures += 1;
            }
        }
    }

    println!(
        "Optical audit completed: {} icon(s), {warnings} warning(s), {failures} failure(s)",
        paths.len()
    );

    if failures == 0 {
        Ok(())
    } else {
        Err(failures)
    }
}

fn main() {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("usage: cargo run --example provider_icon_metrics -- <provider SVGs...>");
        std::process::exit(2);
    }

    if audit(&paths).is_err() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SVG: &str = r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><rect x="4" y="4" width="16" height="16" fill="black"/></svg>"#;

    #[test]
    fn measures_visible_bounds_in_icon_units() {
        let metrics = measure_svg(VALID_SVG.as_bytes()).expect("valid SVG");
        assert!((metrics.min_x - 4.0).abs() < 0.1);
        assert!((metrics.min_y - 4.0).abs() < 0.1);
        assert!((metrics.width() - 16.0).abs() < 0.1);
        assert!((metrics.height() - 16.0).abs() < 0.1);
    }

    #[test]
    fn rejects_empty_rendering() {
        let svg = r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"/>"#;
        assert!(measure_svg(svg.as_bytes()).is_err());
    }

    #[test]
    fn detects_geometry_outside_the_canvas() {
        let svg = r#"<svg width="24" height="24" viewBox="0 0 24 24" overflow="visible" xmlns="http://www.w3.org/2000/svg"><rect x="-1" y="4" width="8" height="8" fill="black"/></svg>"#;
        let metrics = measure_svg(svg.as_bytes()).expect("visible SVG");
        assert!(metrics.is_clipped());
    }
}
