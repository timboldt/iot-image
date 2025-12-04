use chrono::{Local, Timelike};
use serde::Deserialize;

// FRED API response structures
#[derive(Debug, Deserialize)]
pub struct FredResponse {
    pub observations: Vec<FredObservation>,
}

#[derive(Debug, Deserialize)]
pub struct FredObservation {
    pub date: String,
    pub value: String,
}

#[derive(Debug)]
pub struct SeriesData {
    #[allow(dead_code)]
    pub symbol: String,
    pub name: String,
    pub points: Vec<DataPoint>,
}

#[derive(Debug)]
pub struct DataPoint {
    #[allow(dead_code)]
    pub date: String,
    pub value: f64,
}

#[derive(Debug)]
pub struct FredData {
    pub vix: SeriesData,
    pub sp500: SeriesData,
    pub credit_spread: SeriesData,
    pub treasury_10y: SeriesData,
}

async fn fetch_series(
    api_key: &str,
    series_id: &str,
    limit: usize,
) -> Result<Vec<DataPoint>, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&sort_order=desc&limit={}",
        series_id, api_key, limit
    );

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;
    let fred_response: FredResponse = response.json().await?;

    let mut points: Vec<DataPoint> = fred_response
        .observations
        .iter()
        .filter_map(|obs| {
            // Skip missing values (marked as "." in FRED)
            if obs.value == "." {
                return None;
            }
            let value = obs.value.parse::<f64>().ok()?;
            Some(DataPoint {
                date: obs.date.clone(),
                value,
            })
        })
        .collect();

    // FRED returns newest first when using sort_order=desc, so reverse for chronological order
    points.reverse();
    Ok(points)
}

/// Fetches economic data from FRED API
///
/// # Arguments
/// * `api_key` - FRED API key (get one free at https://fred.stlouisfed.org/docs/api/api_key.html)
///
/// # Returns
/// Result containing FredData on success, or error message on failure
pub async fn fetch_fred(api_key: &str) -> Result<FredData, Box<dyn std::error::Error>> {
    // Fetch last 120 days of data for each series
    let vix = fetch_series(api_key, "VIXCLS", 120).await?;
    let sp500 = fetch_series(api_key, "SP500", 120).await?;
    let credit_spread = fetch_series(api_key, "BAMLH0A0HYM2", 120).await?;
    let treasury_10y = fetch_series(api_key, "DGS10", 120).await?;

    Ok(FredData {
        vix: SeriesData {
            symbol: "VIX".to_string(),
            name: "VIX Fear Gauge".to_string(),
            points: vix,
        },
        sp500: SeriesData {
            symbol: "SPX".to_string(),
            name: "S&amp;P 500".to_string(),
            points: sp500,
        },
        credit_spread: SeriesData {
            symbol: "HY-OAS".to_string(),
            name: "High Yield Spreads".to_string(),
            points: credit_spread,
        },
        treasury_10y: SeriesData {
            symbol: "10Y".to_string(),
            name: "10-Year Treasury".to_string(),
            points: treasury_10y,
        },
    })
}

/// Generates an SVG display of economic data
///
/// # Arguments
/// * `fred` - The FRED economic data to display
/// * `battery_pct` - Optional battery percentage to display
///
/// # Returns
/// A String containing the SVG markup
pub fn generate_fred_svg(fred: &FredData, battery_pct: Option<u8>) -> String {
    let width = 800;
    let height = 480;
    let mut svg = String::new();

    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg">"#,
        width, height
    ));

    // Define gradient for battery bar
    svg.push_str(r#"<defs>"#);
    svg.push_str(r#"<linearGradient id="batteryGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push_str(r#"</linearGradient>"#);
    svg.push_str(r#"</defs>"#);

    // Background
    svg.push_str(&format!(
        r#"<rect width="{}" height="{}" fill="white"/>"#,
        width, height
    ));

    // Title
    svg.push_str(&format!(
        r#"<text x="{}" y="20" text-anchor="middle" font-size="22" font-weight="bold" fill="black">Market Crash Monitor</text>"#,
        width / 2
    ));

    // Timeframe indicator (top right)
    svg.push_str(&format!(
        r#"<text x="{}" y="20" text-anchor="end" font-size="14" fill="black">120 days</text>"#,
        width - 10
    ));

    // Create 2x2 grid of charts (leaving room for header and footer)
    let chart_width = 380;
    let chart_height = 200;
    let positions = [
        (10, 35),   // Top-left (VIX)
        (410, 35),  // Top-right (S&P 500)
        (10, 245),  // Bottom-left (Credit Spreads)
        (410, 245), // Bottom-right (10Y Treasury)
    ];

    // Generate charts
    svg.push_str(&generate_area_chart(
        &fred.vix,
        positions[0].0,
        positions[0].1,
        chart_width,
        chart_height,
        "red",
        Some(15.0),
    ));
    svg.push_str(&generate_sp500_chart(
        &fred.sp500,
        positions[1].0,
        positions[1].1,
        chart_width,
        chart_height,
    ));
    svg.push_str(&generate_area_chart(
        &fred.credit_spread,
        positions[2].0,
        positions[2].1,
        chart_width,
        chart_height,
        "orange",
        None,
    ));
    svg.push_str(&generate_area_chart(
        &fred.treasury_10y,
        positions[3].0,
        positions[3].1,
        chart_width,
        chart_height,
        "blue",
        None,
    ));

    // Footer with last updated and battery bar
    let footer_y = height - 10;

    // Last updated timestamp
    let now = Local::now();
    let timestamp = format!(
        "Last updated: {:02}:{:02}:{:02}",
        now.hour(),
        now.minute(),
        now.second()
    );
    svg.push_str(&format!(
        r#"<text x="10" y="{}" font-size="12" fill="black">{}</text>"#,
        footer_y, timestamp
    ));

    // Battery bar (if provided)
    let pct = battery_pct.unwrap_or(50);
    let battery_bar_width = 100;
    let battery_bar_height = 12;
    let battery_x = width - 110;
    let battery_y = footer_y - 10;
    let battery_inset = 2;
    let battery_fill_width = (battery_bar_width - battery_inset * 2) * pct as i32 / 100;

    // Label
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="12" fill="black">Battery:</text>"#,
        battery_x - 5,
        footer_y
    ));

    // Background (container) rectangle
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="2"/>"#,
        battery_x, battery_y, battery_bar_width, battery_bar_height
    ));

    // ClipPath for battery bar
    svg.push_str(r#"<clipPath id="batteryClip">"#);
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="1"/>"#,
        battery_x + battery_inset,
        battery_y + battery_inset,
        battery_fill_width,
        battery_bar_height - battery_inset * 2
    ));
    svg.push_str(r#"</clipPath>"#);

    // Full-width gradient rect, clipped
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#batteryGradient)" clip-path="url(#batteryClip)" rx="1"/>"#,
        battery_x + battery_inset,
        battery_y + battery_inset,
        battery_bar_width - battery_inset * 2,
        battery_bar_height - battery_inset * 2
    ));

    svg.push_str("</svg>");
    svg
}

fn generate_area_chart(
    series: &SeriesData,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: &str,
    threshold: Option<f64>,
) -> String {
    let mut svg = String::new();

    // Chart border
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>"#,
        x, y, width, height
    ));

    // Title and current value
    if let Some(last) = series.points.last() {
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="start" font-size="16" font-weight="bold" fill="black">{}</text>"#,
            x + 5,
            y + 20,
            series.name
        ));

        // Format value based on magnitude
        let value_str = if series.name.contains("%") {
            format!("{:.2}%", last.value)
        } else if last.value > 100.0 {
            format!("{:.0}", last.value)
        } else {
            format!("{:.1}", last.value)
        };

        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{}</text>"#,
            x + width - 5,
            y + 20,
            value_str
        ));
    }

    if series.points.is_empty() {
        return svg;
    }

    // Draw area chart
    generate_area_chart_internal(&mut svg, series, x, y, width, height, color, threshold);

    svg
}

fn generate_sp500_chart(series: &SeriesData, x: i32, y: i32, width: i32, height: i32) -> String {
    let mut svg = String::new();

    // Chart border
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>"#,
        x, y, width, height
    ));

    // Title and current value
    if let Some(last) = series.points.last() {
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="start" font-size="16" font-weight="bold" fill="black">{}</text>"#,
            x + 5,
            y + 20,
            series.name
        ));

        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{:.0}</text>"#,
            x + width - 5,
            y + 20,
            last.value
        ));

        // Calculate circuit breaker levels based on most recent close
        let cb_7 = last.value * 0.93; // -7%
        let cb_13 = last.value * 0.87; // -13%
        let cb_20 = last.value * 0.80; // -20%

        // Draw area chart
        generate_area_chart_internal(&mut svg, series, x, y, width, height, "black", None);

        // Draw circuit breaker lines
        if !series.points.is_empty() {
            let min_val = series
                .points
                .iter()
                .map(|p| p.value)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            let max_val = series
                .points
                .iter()
                .map(|p| p.value)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            let range = if max_val > min_val {
                max_val - min_val
            } else {
                1.0
            };

            let chart_x = x + 40;
            let chart_y = y + 35;
            let chart_h = height - 55;

            // Only draw circuit breakers if they're in the visible range
            for (level, color, label) in [
                (cb_7, "orange", "-7%"),
                (cb_13, "darkorange", "-13%"),
                (cb_20, "red", "-20%"),
            ] {
                if level >= min_val && level <= max_val {
                    let line_y =
                        chart_y + chart_h - ((level - min_val) / range * chart_h as f64) as i32;
                    svg.push_str(&format!(
                        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="1" stroke-dasharray="4,2"/>"#,
                        chart_x, line_y, chart_x + width - 50, line_y, color
                    ));
                    svg.push_str(&format!(
                        r#"<text x="{}" y="{}" font-size="9" fill="{}">{}</text>"#,
                        chart_x + 2,
                        line_y - 2,
                        color,
                        label
                    ));
                }
            }
        }
    }

    svg
}

#[allow(clippy::too_many_arguments)]
fn generate_area_chart_internal(
    svg: &mut String,
    series: &SeriesData,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: &str,
    threshold: Option<f64>,
) {
    if series.points.is_empty() {
        return;
    }

    let min_val = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let max_val = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(100.0);
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0
    };

    // Chart area
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_w = width - 50;
    let chart_h = height - 55;

    // Build area path (line + filled area below)
    let num_points = series.points.len();
    if num_points > 0 {
        let mut path = String::new();

        // Start at bottom-left
        let first_x = chart_x;
        path.push_str(&format!("M {} {}", first_x, chart_y + chart_h));

        // Draw line along data points
        for (i, point) in series.points.iter().enumerate() {
            let px = chart_x + (chart_w * i as i32) / (num_points - 1).max(1) as i32;
            let py = chart_y + chart_h - ((point.value - min_val) / range * chart_h as f64) as i32;
            path.push_str(&format!(" L {} {}", px, py));
        }

        // Close path at bottom-right
        let last_x = chart_x + chart_w;
        path.push_str(&format!(" L {} {} Z", last_x, chart_y + chart_h));

        // Draw filled area with transparency
        svg.push_str(&format!(
            r#"<path d="{}" fill="{}" fill-opacity="0.3" stroke="{}" stroke-width="2"/>"#,
            path, color, color
        ));

        // Draw threshold line if provided (e.g., VIX panic level at 15)
        if let Some(threshold_val) = threshold {
            if threshold_val >= min_val && threshold_val <= max_val {
                let threshold_y =
                    chart_y + chart_h - ((threshold_val - min_val) / range * chart_h as f64) as i32;
                svg.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="darkred" stroke-width="1" stroke-dasharray="4,2"/>"#,
                    chart_x, threshold_y, chart_x + chart_w, threshold_y
                ));
            }
        }
    }

    // Y-axis labels (min and max)
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.1}</text>"#,
        chart_x - 5,
        chart_y + 5,
        max_val
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.1}</text>"#,
        chart_x - 5,
        chart_y + chart_h,
        min_val
    ));
}
