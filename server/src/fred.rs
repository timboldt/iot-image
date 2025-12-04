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
        "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&observation_start=2020-02-01&observation_end=2020-06-01&sort_order=desc&limit={}",
        series_id, api_key, limit
    );

    // Alternative test URL to fetch COVID crisis data (Feb-June 2020)
    // let url = format!(
    //     "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&observation_start=2020-02-01&observation_end=2020-06-01&sort_order=desc&limit={}",
    //     series_id, api_key, limit
    // );

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
    svg.push_str(&generate_vix_chart(
        &fred.vix,
        positions[0].0,
        positions[0].1,
        chart_width,
        chart_height,
    ));
    svg.push_str(&generate_sp500_chart(
        &fred.sp500,
        positions[1].0,
        positions[1].1,
        chart_width,
        chart_height,
    ));
    svg.push_str(&generate_credit_spread_chart(
        &fred.credit_spread,
        positions[2].0,
        positions[2].1,
        chart_width,
        chart_height,
    ));
    svg.push_str(&generate_treasury_chart(
        &fred.treasury_10y,
        positions[3].0,
        positions[3].1,
        chart_width,
        chart_height,
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

fn generate_vix_chart(series: &SeriesData, x: i32, y: i32, width: i32, height: i32) -> String {
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
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{:.1}</text>"#,
            x + width - 5,
            y + 20,
            last.value
        ));
    }

    if series.points.is_empty() {
        return svg;
    }

    // VIX regime thresholds
    let calm_threshold = 20.0;
    let fear_threshold = 40.0;

    // Calculate data range, ensuring thresholds are always visible
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(100.0);

    let min_val = data_min.min(calm_threshold);
    let max_val = data_max.max(fear_threshold);
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0
    };

    // Create gradient ID unique to this chart
    let gradient_id = format!("vixGradient_{}_{}", x, y);

    // Create gradient: green (0-20), orange (20-40), red (>40)
    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="100%" x2="0%" y2="0%">"#,
        gradient_id
    ));

    // Calculate gradient stop positions based on data range
    if max_val <= calm_threshold {
        // All calm - just green
        svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
        svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    } else if max_val <= fear_threshold {
        // Calm to elevated - green to yellow (smooth transition)
        let calm_pct = ((calm_threshold - min_val) / range * 100.0).clamp(0.0, 100.0);
        svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
        svg.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
            calm_pct
        ));
        svg.push_str(r#"<stop offset="100%" style="stop-color:orange;stop-opacity:1" />"#);
    } else {
        // Full range - green, yellow, and red (smooth transitions)
        if min_val < calm_threshold {
            let calm_pct = ((calm_threshold - min_val) / range * 100.0).min(100.0);
            let fear_pct = ((fear_threshold - min_val) / range * 100.0).min(100.0);

            svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
                calm_pct
            ));
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:red;stop-opacity:1" />"#,
                fear_pct
            ));
            svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
        } else if min_val < fear_threshold {
            // Starts in yellow zone
            let fear_pct = ((fear_threshold - min_val) / range * 100.0).min(100.0);
            svg.push_str(r#"<stop offset="0%" style="stop-color:orange;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:red;stop-opacity:1" />"#,
                fear_pct
            ));
            svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
        } else {
            // All fear - just red
            svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
            svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
        }
    }

    svg.push_str(r#"</linearGradient></defs>"#);

    // Draw area chart with gradient
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
    );

    // Draw threshold lines (only if within actual data range)
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(calm_threshold, "green"), (fear_threshold, "red")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y =
                chart_y + chart_h - ((threshold - min_val) / range * chart_h as f64) as i32;
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
                chart_x, line_y, chart_x + chart_w, line_y, color
            ));
        }
    }

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
    }

    if series.points.is_empty() {
        return svg;
    }

    // Calculate data range
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(100.0);

    // Calculate drawdown thresholds based on highest value in chart
    let threshold_7 = data_max * 0.93; // -7%
    let threshold_20 = data_max * 0.80; // -20%

    // Ensure both thresholds are always visible
    let min_val = data_min.min(threshold_20);
    let max_val = data_max;
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0
    };

    // Create gradient ID unique to this chart
    let gradient_id = format!("sp500Gradient_{}_{}", x, y);

    // Create gradient: green (0 to -7%), orange (-7% to -20%), red (below -20%)
    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="100%" x2="0%" y2="0%">"#,
        gradient_id
    ));

    // Calculate gradient stop positions
    if min_val >= threshold_7 {
        // All green - no drawdown beyond -7%
        svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
        svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    } else if min_val >= threshold_20 {
        // Green to yellow - drawdown between 0% and -20% (smooth transition)
        let threshold_7_pct = ((threshold_7 - min_val) / range * 100.0).min(100.0);
        svg.push_str(r#"<stop offset="0%" style="stop-color:orange;stop-opacity:1" />"#);
        svg.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:green;stop-opacity:1" />"#,
            threshold_7_pct
        ));
        svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    } else {
        // Full range - green, yellow, and red (smooth transitions)
        let threshold_7_pct = ((threshold_7 - min_val) / range * 100.0).min(100.0);
        let threshold_20_pct = ((threshold_20 - min_val) / range * 100.0).min(100.0);

        svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
        svg.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
            threshold_20_pct
        ));
        svg.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:green;stop-opacity:1" />"#,
            threshold_7_pct
        ));
        svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    }

    svg.push_str(r#"</linearGradient></defs>"#);

    // Draw area chart with gradient
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
    );

    // Draw threshold lines (only if within actual data range)
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(threshold_7, "green"), (threshold_20, "red")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y =
                chart_y + chart_h - ((threshold - min_val) / range * chart_h as f64) as i32;
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
                chart_x, line_y, chart_x + chart_w, line_y, color
            ));
        }
    }

    svg
}

fn generate_credit_spread_chart(
    series: &SeriesData,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
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

        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{:.2}%</text>"#,
            x + width - 5,
            y + 20,
            last.value
        ));
    }

    if series.points.is_empty() {
        return svg;
    }

    // High yield spread regime thresholds
    let normal_threshold = 4.0;
    let stress_threshold = 6.0;

    // Calculate data range, ensuring thresholds are always visible
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(10.0);

    let min_val = data_min.min(normal_threshold);
    let max_val = data_max.max(stress_threshold);
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0
    };

    // Create gradient ID unique to this chart
    let gradient_id = format!("creditGradient_{}_{}", x, y);

    // Create gradient with regime bands: <4% = green, 4-6% = orange, 6%+ = red
    // Calculate percentages for gradient stops (inverted because SVG gradient goes top-to-bottom)

    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="100%" x2="0%" y2="0%">"#,
        gradient_id
    ));

    // Calculate gradient stop positions based on data range
    if max_val <= normal_threshold {
        // All normal - just green
        svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
        svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    } else if min_val >= stress_threshold {
        // All panic - just red
        svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
        svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
    } else {
        // Mixed range - create gradient with stops
        // Bottom is min_val, top is max_val
        // Calculate where thresholds fall as percentages

        if min_val < normal_threshold {
            let normal_pct = ((normal_threshold - min_val) / range * 100.0).min(100.0);
            svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:green;stop-opacity:1" />"#,
                normal_pct
            ));

            if max_val > stress_threshold {
                let stress_pct = ((stress_threshold - min_val) / range * 100.0).min(100.0);
                svg.push_str(&format!(
                    r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
                    stress_pct
                ));
                svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
            } else {
                // Ends in stress zone
                svg.push_str(r#"<stop offset="100%" style="stop-color:orange;stop-opacity:1" />"#);
            }
        } else {
            // Starts in stress zone
            let stress_pct = ((stress_threshold - min_val) / range * 100.0).min(100.0);
            svg.push_str(r#"<stop offset="0%" style="stop-color:orange;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
                stress_pct
            ));
            svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
        }
    }

    svg.push_str(r#"</linearGradient></defs>"#);

    // Draw area chart with gradient
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
    );

    // Draw regime threshold lines (only if within actual data range)
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(normal_threshold, "green"), (stress_threshold, "red")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y =
                chart_y + chart_h - ((threshold - min_val) / range * chart_h as f64) as i32;
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
                chart_x, line_y, chart_x + chart_w, line_y, color
            ));
        }
    }

    svg
}

fn generate_treasury_chart(series: &SeriesData, x: i32, y: i32, width: i32, height: i32) -> String {
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
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{:.1}</text>"#,
            x + width - 5,
            y + 20,
            last.value
        ));
    }

    if series.points.is_empty() {
        return svg;
    }

    // Calculate data range
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(10.0);

    // Treasury regime thresholds: relative to highest value
    // Green when close to peak, orange/red when yields drop significantly
    let normal_threshold = data_max - 0.5; // 0.5% below peak
    let stress_threshold = data_max - 1.0; // 1.0% below peak

    // Ensure thresholds are always visible
    let min_val = data_min.min(stress_threshold);
    let max_val = data_max;
    let range = if max_val > min_val {
        max_val - min_val
    } else {
        1.0
    };

    // Create gradient ID unique to this chart
    let gradient_id = format!("treasuryGradient_{}_{}", x, y);

    // Create gradient: red (>1% below peak), orange (0.5-1% below peak), green (within 0.5% of peak)
    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="100%" x2="0%" y2="0%">"#,
        gradient_id
    ));

    // Calculate gradient stop positions based on data range
    if max_val <= stress_threshold {
        // All stress - just red
        svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
        svg.push_str(r#"<stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
    } else if max_val <= normal_threshold {
        // Stress to elevated - red to yellow (smooth transition)
        let stress_pct = ((stress_threshold - min_val) / range * 100.0).clamp(0.0, 100.0);
        svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
        svg.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
            stress_pct
        ));
        svg.push_str(r#"<stop offset="100%" style="stop-color:orange;stop-opacity:1" />"#);
    } else {
        // Full range - red, yellow, and green (smooth transitions)
        if min_val < stress_threshold {
            let stress_pct = ((stress_threshold - min_val) / range * 100.0).min(100.0);
            let normal_pct = ((normal_threshold - min_val) / range * 100.0).min(100.0);

            svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:orange;stop-opacity:1" />"#,
                stress_pct
            ));
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:green;stop-opacity:1" />"#,
                normal_pct
            ));
            svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
        } else if min_val < normal_threshold {
            // Starts in yellow zone
            let normal_pct = ((normal_threshold - min_val) / range * 100.0).min(100.0);
            svg.push_str(r#"<stop offset="0%" style="stop-color:orange;stop-opacity:1" />"#);
            svg.push_str(&format!(
                r#"<stop offset="{}%" style="stop-color:green;stop-opacity:1" />"#,
                normal_pct
            ));
            svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
        } else {
            // All normal - just green
            svg.push_str(r#"<stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
            svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
        }
    }

    svg.push_str(r#"</linearGradient></defs>"#);

    // Draw area chart with gradient
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
    );

    // Draw threshold lines (only if within actual data range)
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(stress_threshold, "red"), (normal_threshold, "green")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y =
                chart_y + chart_h - ((threshold - min_val) / range * chart_h as f64) as i32;
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
                chart_x, line_y, chart_x + chart_w, line_y, color
            ));
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
            r#"<path d="{}" fill="{}" fill-opacity="0.3" stroke="black" stroke-width="1"/>"#,
            path, color
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
