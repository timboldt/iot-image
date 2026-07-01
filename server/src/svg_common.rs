//! Small SVG snippets shared by the weather/stocks/fred/weight chart generators.
//! These modules otherwise duplicated this markup (battery indicator, axis labels)
//! nearly verbatim in half a dozen places.

/// `<linearGradient>` definition for the battery bar fill (red at 0% -> green at 100%).
/// Callers must emit this once inside their `<defs>` block before calling `battery_bar_svg`.
pub const BATTERY_GRADIENT_DEF: &str = r#"<linearGradient id="batteryGradient" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" style="stop-color:red;stop-opacity:1" /><stop offset="100%" style="stop-color:green;stop-opacity:1" /></linearGradient>"#;

/// Renders the "Battery:" label. `anchor` is an SVG `text-anchor` value ("start" or "end").
pub fn battery_label_svg(x: f64, y: f64, anchor: &str, font_size: u32) -> String {
    format!(
        r#"<text x="{}" y="{}" text-anchor="{}" font-size="{}" fill="black">Battery:</text>"#,
        x, y, anchor, font_size
    )
}

/// Renders the battery bar body: background rect, clip-path, and gradient fill.
/// `(x, y)` is the top-left corner of a 100x12 bar. `clip_id` must be unique within
/// the containing SVG document. Requires `BATTERY_GRADIENT_DEF` to already be in `<defs>`.
pub fn battery_bar_svg(x: f64, y: f64, pct: u8, stroke_width: f64, clip_id: &str) -> String {
    const WIDTH: f64 = 100.0;
    const HEIGHT: f64 = 12.0;
    const INSET: f64 = 2.0;
    let fill_width = (WIDTH - INSET * 2.0) * (pct as f64 / 100.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="{}" rx="2"/>"#,
        x, y, WIDTH, HEIGHT, stroke_width
    ));
    svg.push_str(&format!(r#"<clipPath id="{}">"#, clip_id));
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="1"/>"#,
        x + INSET,
        y + INSET,
        fill_width,
        HEIGHT - INSET * 2.0
    ));
    svg.push_str("</clipPath>");
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#batteryGradient)" clip-path="url(#{})" rx="1"/>"#,
        x + INSET,
        y + INSET,
        WIDTH - INSET * 2.0,
        HEIGHT - INSET * 2.0,
        clip_id
    ));
    svg
}

/// Renders the paired min/max axis labels used at the top and bottom of a chart's Y axis.
/// `x` should already include the caller's offset from the chart's left edge (e.g. `chart_x - 5`).
pub fn axis_minmax_labels(
    x: f64,
    y_top: f64,
    y_bottom: f64,
    max_label: &str,
    min_label: &str,
) -> String {
    format!(
        r#"<text x="{x}" y="{y_top}" text-anchor="end" font-size="10" fill="black">{max_label}</text><text x="{x}" y="{y_bottom}" text-anchor="end" font-size="10" fill="black">{min_label}</text>"#,
        x = x,
        y_top = y_top,
        y_bottom = y_bottom,
        max_label = max_label,
        min_label = min_label,
    )
}
