use crate::svg_common;
use chrono::{Local, NaiveDate};
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
    pub name: String,
    pub points: Vec<DataPoint>,
}

#[derive(Debug, Clone)]
pub struct DataPoint {
    pub date: String,
    pub value: f64,
}

#[derive(Debug)]
pub struct FredData {
    pub vix: SeriesData,
    pub sp500: SeriesData,
    pub credit_spread: SeriesData,
    pub yield_curve: SeriesData,
    /// Per-point signal classification, parallel to yield_curve.points
    pub yield_curve_signals: Vec<SteepeningType>,
    pub yield_curve_steepening: SteepeningType,
    pub end_date: String,
    pub duration: usize,
}

const YIELD_CURVE_WARMUP_DAYS: usize = 60;
const YIELD_CURVE_VELOCITY_SCALE: f64 = 1_000.0;
// Scaled velocity threshold for steepening classification (~0.3 bp/day)
const YIELD_CURVE_VEL_THRESHOLD: f64 = 3.0;

/// Classifies the type of yield curve steepening based on which leg is driving it.
/// This is critical for distinguishing crisis signals from benign expansion.
#[derive(Debug, PartialEq)]
pub enum SteepeningType {
    /// 3M rate falling → short-end front-running Fed cuts; systemic risk signal
    BullSteepening,
    /// 10Y rate rising while 3M is flat/rising → expanding term premium; benign
    BearSteepening,
    /// Spread narrowing (moving toward inversion) when already positive, or deepening when negative
    Flattening,
    /// Spread is negative AND deepening (inversion worsening)
    Inverting,
    /// Low velocity; no strong signal
    Stable,
}

use crate::kalman::KalmanFilter;

// Position process noise ~0.07 pp/day; velocity process noise tightened to reduce
// spurious jumps while staying responsive to real trend changes.
const KALMAN_Q_POSITION: f64 = 0.005;
const KALMAN_Q_VELOCITY: f64 = 0.00005;
// Measurement noise: FRED yields are in percentage points (e.g. 0.35).
// Daily noise std ~0.05 pp → R ≈ 0.0025.  Use 0.01 for slight smoothing.
const KALMAN_R: f64 = 0.01;

fn compute_velocity_series(points: &[DataPoint]) -> Vec<DataPoint> {
    if points.is_empty() {
        return Vec::new();
    }

    let mut filter = KalmanFilter::new(
        points[0].value,
        0.0,
        KALMAN_Q_POSITION,
        KALMAN_Q_VELOCITY,
        KALMAN_R,
    );
    let mut velocity_points = Vec::with_capacity(points.len());
    velocity_points.push(DataPoint {
        date: points[0].date.clone(),
        value: 0.0,
    });

    let mut previous_date = NaiveDate::parse_from_str(&points[0].date, "%Y-%m-%d").ok();

    for point in points.iter().skip(1) {
        let current_date = NaiveDate::parse_from_str(&point.date, "%Y-%m-%d").ok();
        let dt_days = if let (Some(prev), Some(curr)) = (previous_date, current_date) {
            let dt = (curr - prev).num_days().max(1);
            dt as f64
        } else {
            1.0
        };

        filter.predict(dt_days);
        filter.update(point.value);

        let velocity = filter.velocity();
        velocity_points.push(DataPoint {
            date: point.date.clone(),
            value: velocity * YIELD_CURVE_VELOCITY_SCALE,
        });

        if current_date.is_some() {
            previous_date = current_date;
        }
    }

    velocity_points
}

fn build_date_map(points: &[DataPoint]) -> std::collections::HashMap<String, f64> {
    points.iter().map(|p| (p.date.clone(), p.value)).collect()
}

/// Classifies one observation from the velocities of the spread and each leg.
///
/// A positive spread velocity covers two very different macro environments:
/// bull steepening (3M falling fast as the market prices emergency Fed cuts —
/// a crisis signal) and bear steepening (10Y rising on an expanding term
/// premium — benign). A negative velocity is split by the current spread
/// *level*: still positive means flattening, at or below zero means a
/// deepening inversion.
fn classify_at_point(
    spread_vel: f64,
    dgs10_vel: f64,
    dtb3_vel: f64,
    spread_level: f64,
) -> SteepeningType {
    if spread_vel.abs() < YIELD_CURVE_VEL_THRESHOLD {
        SteepeningType::Stable
    } else if spread_vel > YIELD_CURVE_VEL_THRESHOLD {
        if dtb3_vel < -YIELD_CURVE_VEL_THRESHOLD && (-dtb3_vel) > dgs10_vel.max(0.0) {
            SteepeningType::BullSteepening
        } else {
            SteepeningType::BearSteepening
        }
    } else if spread_level > 0.1 {
        SteepeningType::Flattening
    } else {
        SteepeningType::Inverting
    }
}

async fn fetch_series(
    api_key: &str,
    series_id: &str,
    end_date: Option<&str>,
    duration: usize,
) -> Result<Vec<DataPoint>, Box<dyn std::error::Error>> {
    let url = if let Some(end) = end_date {
        // Parse end date from YYYYMMDD format
        let end_date = NaiveDate::parse_from_str(end, "%Y%m%d")
            .map_err(|e| format!("Invalid date format. Use YYYYMMDD: {}", e))?;

        // Calculate start date
        let start_date = end_date - chrono::Duration::days(duration as i64);

        format!(
            "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&observation_start={}&observation_end={}&sort_order=desc",
            series_id, api_key, start_date.format("%Y-%m-%d"), end_date.format("%Y-%m-%d")
        )
    } else {
        // Default: fetch last N days
        format!(
            "https://api.stlouisfed.org/fred/series/observations?series_id={}&api_key={}&file_type=json&sort_order=desc&limit={}",
            series_id, api_key, duration
        )
    };

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
/// * `end_date` - Optional end date in YYYYMMDD format (defaults to today)
/// * `duration` - Optional duration in days (defaults to 365)
///
/// # Returns
/// Result containing FredData on success, or error message on failure
pub async fn fetch_fred(
    api_key: &str,
    end_date: Option<&str>,
    duration: Option<usize>,
) -> Result<FredData, Box<dyn std::error::Error>> {
    let duration = duration.unwrap_or(365);

    // Determine the actual end date to display
    let display_end_date = if let Some(date_str) = end_date {
        date_str.to_string()
    } else {
        Local::now().format("%Y%m%d").to_string()
    };

    let chart_end_date = NaiveDate::parse_from_str(&display_end_date, "%Y%m%d")
        .unwrap_or_else(|_| Local::now().date_naive());
    let chart_start_date = chart_end_date - chrono::Duration::days(duration as i64);

    let vix = fetch_series(api_key, "VIXCLS", end_date, duration).await?;
    let sp500 = fetch_series(api_key, "SP500", end_date, duration).await?;
    let credit_spread = fetch_series(api_key, "BAMLH0A0HYM2", end_date, duration).await?;
    let yield_curve = fetch_series(
        api_key,
        "T10Y3M",
        end_date,
        duration + YIELD_CURVE_WARMUP_DAYS,
    )
    .await?;
    // Fetch individual legs to distinguish bull vs. bear steepening.
    let dgs10 = fetch_series(
        api_key,
        "DGS10",
        end_date,
        duration + YIELD_CURVE_WARMUP_DAYS,
    )
    .await?;
    let dtb3 = fetch_series(
        api_key,
        "DTB3",
        end_date,
        duration + YIELD_CURVE_WARMUP_DAYS,
    )
    .await?;

    let yield_curve_velocity = compute_velocity_series(&yield_curve);
    let dgs10_velocity = compute_velocity_series(&dgs10);
    let dtb3_velocity = compute_velocity_series(&dtb3);

    // Build date maps from full velocity series before windowing consumes them
    let spread_vel_map = build_date_map(&yield_curve_velocity);
    let dgs10_vel_map = build_date_map(&dgs10_velocity);
    let dtb3_vel_map = build_date_map(&dtb3_velocity);

    // Window raw spread to the chart period (for plotting)
    let mut yield_curve_windowed: Vec<DataPoint> = yield_curve
        .iter()
        .filter(|point| {
            NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
                .map(|d| d >= chart_start_date)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    if yield_curve_windowed.is_empty() {
        yield_curve_windowed = yield_curve.clone();
    }

    let mut yield_curve_velocity_windowed: Vec<DataPoint> = yield_curve_velocity
        .into_iter()
        .filter(|point| {
            NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
                .map(|d| d >= chart_start_date)
                .unwrap_or(true)
        })
        .collect();
    if yield_curve_velocity_windowed.is_empty() {
        yield_curve_velocity_windowed = compute_velocity_series(&yield_curve);
    }

    // Window individual-leg velocities to the chart period.
    let dgs10_vel_windowed: Vec<DataPoint> = dgs10_velocity
        .into_iter()
        .filter(|p| {
            NaiveDate::parse_from_str(&p.date, "%Y-%m-%d")
                .map(|d| d >= chart_start_date)
                .unwrap_or(true)
        })
        .collect();
    let dtb3_vel_windowed: Vec<DataPoint> = dtb3_velocity
        .into_iter()
        .filter(|p| {
            NaiveDate::parse_from_str(&p.date, "%Y-%m-%d")
                .map(|d| d >= chart_start_date)
                .unwrap_or(true)
        })
        .collect();

    // Classify steepening type from the latest velocities of each leg.
    // Spread velocity > 0 can mean two very different macro environments:
    //   Bull steepening: 3M falling fast (market pricing emergency Fed cuts) → crisis signal
    //   Bear steepening: 10Y rising (expanding term premium, growth optimism) → benign
    let spread_vel = yield_curve_velocity_windowed
        .last()
        .map(|p| p.value)
        .unwrap_or(0.0);
    let dgs10_vel = dgs10_vel_windowed.last().map(|p| p.value).unwrap_or(0.0);
    let dtb3_vel = dtb3_vel_windowed.last().map(|p| p.value).unwrap_or(0.0);

    let current_spread_level = yield_curve.last().map(|p| p.value).unwrap_or(0.0);
    let steepening = classify_at_point(spread_vel, dgs10_vel, dtb3_vel, current_spread_level);

    // Build per-point signal series aligned with the windowed spread for band rendering.
    let yield_curve_signals: Vec<SteepeningType> = yield_curve_windowed
        .iter()
        .map(|p| {
            let sv = spread_vel_map.get(&p.date).copied().unwrap_or(0.0);
            let dv = dgs10_vel_map.get(&p.date).copied().unwrap_or(0.0);
            let tv = dtb3_vel_map.get(&p.date).copied().unwrap_or(0.0);
            classify_at_point(sv, dv, tv, p.value)
        })
        .collect();

    Ok(FredData {
        vix: SeriesData {
            name: "VIX Fear Gauge".to_string(),
            points: vix,
        },
        sp500: SeriesData {
            name: "S&P 500".to_string(),
            points: sp500,
        },
        credit_spread: SeriesData {
            name: "High Yield Spreads".to_string(),
            points: credit_spread,
        },
        yield_curve: SeriesData {
            name: "Yield Curve (10Y-3M)".to_string(),
            points: yield_curve_windowed,
        },
        yield_curve_signals,
        yield_curve_steepening: steepening,
        end_date: display_end_date,
        duration,
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
    svg.push_str(svg_common::BATTERY_GRADIENT_DEF);
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
    let end_date = NaiveDate::parse_from_str(&fred.end_date, "%Y%m%d")
        .unwrap_or_else(|_| Local::now().date_naive());
    let start_date = end_date - chrono::Duration::days(fred.duration as i64);
    let timeframe = format!(
        "{} to {} ({} days)",
        start_date.format("%b %d, %Y"),
        end_date.format("%b %d, %Y"),
        fred.duration
    );
    svg.push_str(&format!(
        r#"<text x="{}" y="20" text-anchor="end" font-size="12" fill="black">{}</text>"#,
        width - 10,
        timeframe
    ));

    // Create 2x2 grid of charts (leaving room for header and footer)
    let chart_width = 380;
    let chart_height = 200;
    let positions = [
        (10, 35),   // Top-left (VIX)
        (410, 35),  // Top-right (S&P 500)
        (10, 245),  // Bottom-left (Credit Spreads)
        (410, 245), // Bottom-right (Yield Curve)
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
    svg.push_str(&generate_yield_curve_chart(
        &fred.yield_curve,
        &fred.yield_curve_signals,
        &fred.yield_curve_steepening,
        positions[3].0,
        positions[3].1,
        chart_width,
        chart_height,
    ));

    // Footer with last updated and battery bar
    let footer_y = height - 10;

    svg.push_str(&svg_common::last_updated_text(10.0, footer_y as f64, "start"));

    // Battery bar (if provided)
    let pct = battery_pct.unwrap_or(50);
    let battery_x = (width - 110) as f64;
    let battery_y = (footer_y - 10) as f64;

    svg.push_str(&svg_common::battery_label_svg(
        battery_x - 5.0,
        footer_y as f64,
        "end",
        12,
    ));
    svg.push_str(&svg_common::battery_bar_svg(
        battery_x,
        battery_y,
        pct,
        2.0,
        "batteryClip",
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
            svg_common::escape_xml(&series.name)
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

    // Pad the range so both regime thresholds stay on screen.
    let (data_min, data_max) = data_bounds(&series.points);
    let frame = ChartFrame::new(
        x,
        y,
        width,
        height,
        data_min.min(calm_threshold),
        data_max.max(fear_threshold),
    );

    // Gradient: green (bottom/low VIX) → orange (mid) → red (top/high VIX).
    // Standard Cartesian: y1="100%"=bottom=frame.min, y2="0%"=top=frame.max
    let gradient_id = format!("vixGradient_{}_{}", x, y);
    svg.push_str(&gradient_def(
        &gradient_id,
        &[
            (0.0, "green"),
            (frame.pct(calm_threshold), "orange"),
            (frame.pct(fear_threshold), "red"),
            (100.0, "red"),
        ],
    ));

    // Standard orientation: high VIX spikes up into the red zone.
    draw_area_series(
        &mut svg,
        &series.points,
        &frame,
        &format!("url(#{})", gradient_id),
    );

    // The frame is padded to contain both thresholds, so both always render.
    for (threshold, color) in [(calm_threshold, "green"), (fear_threshold, "red")] {
        svg.push_str(&frame.threshold_line(threshold, color));
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
            svg_common::escape_xml(&series.name)
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

    let (data_min, data_max) = data_bounds(&series.points);

    // Drawdown thresholds, relative to the highest value in the window.
    let threshold_7 = data_max * 0.93; // -7%
    let threshold_20 = data_max * 0.80; // -20%

    // Pad the range so the -20% threshold stays on screen.
    let frame = ChartFrame::new(x, y, width, height, data_min.min(threshold_20), data_max);

    // Gradient: green above -7%, orange down to -20%, red below.
    let gradient_id = format!("sp500Gradient_{}_{}", x, y);
    svg.push_str(&gradient_def(
        &gradient_id,
        &[
            (0.0, "red"),
            (frame.pct(threshold_20), "orange"),
            (frame.pct(threshold_7), "green"),
            (100.0, "green"),
        ],
    ));

    // Unlike VIX/credit this uses a gradient background rect rather than an
    // area fill: in a sustained uptrend the line sits near the top and the
    // filled body would be almost entirely low-price colours, which says
    // nothing useful.
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#{})" fill-opacity="0.35"/>"#,
        frame.x, frame.y, frame.w, frame.h, gradient_id
    ));

    if series.points.len() > 1 {
        svg.push_str(&format!(
            r#"<path d="{}" fill="none" stroke="black" stroke-width="2"/>"#,
            frame.polyline(&series.points)
        ));
    }

    svg.push_str(&frame.axis_labels(3, |v| format!("{:.0}", v)));

    // The frame is padded to contain both drawdown levels, so both always render.
    for (threshold, color) in [(threshold_7, "green"), (threshold_20, "red")] {
        svg.push_str(&frame.threshold_line(threshold, color));
    }

    // Chart area border (drawn last so it sits on top)
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="black" stroke-width="1"/>"#,
        frame.x, frame.y, frame.w, frame.h
    ));

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
            svg_common::escape_xml(&series.name)
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
    let normal_threshold = 3.0;
    let stress_threshold = 4.0;

    // Pad the range so both regime thresholds stay on screen.
    let (data_min, data_max) = data_bounds(&series.points);
    let frame = ChartFrame::new(
        x,
        y,
        width,
        height,
        data_min.min(normal_threshold),
        data_max.max(stress_threshold),
    );

    // Gradient: green up to the normal threshold, then orange into red.
    let gradient_id = format!("creditGradient_{}_{}", x, y);
    svg.push_str(&gradient_def(
        &gradient_id,
        &[
            (0.0, "green"),
            (frame.pct(normal_threshold), "green"),
            (frame.pct(stress_threshold), "orange"),
            (100.0, "red"),
        ],
    ));

    // Standard orientation: high spreads spike up into the red zone.
    draw_area_series(
        &mut svg,
        &series.points,
        &frame,
        &format!("url(#{})", gradient_id),
    );

    // The frame is padded to contain both thresholds, so both always render.
    for (threshold, color) in [(normal_threshold, "green"), (stress_threshold, "red")] {
        svg.push_str(&frame.threshold_line(threshold, color));
    }

    svg
}

fn generate_yield_curve_chart(
    series: &SeriesData,
    signals: &[SteepeningType],
    steepening: &SteepeningType,
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

    // Title left, signal right — both on the same row, no sub-header needed
    let (signal_text, signal_color) = match steepening {
        SteepeningType::BullSteepening => ("BULL STEEP \u{26a0} Crisis Signal", "red"),
        SteepeningType::BearSteepening => ("Bear Steep \u{2014} Expansion", "#cc8800"),
        SteepeningType::Flattening => ("Flattening \u{2014} Caution", "#cc8800"),
        SteepeningType::Inverting => ("Inverting \u{26a0} Warning", "red"),
        SteepeningType::Stable => ("Stable", "#666666"),
    };
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="start" font-size="16" font-weight="bold" fill="black">{}</text>"#,
        x + 5,
        y + 20,
        svg_common::escape_xml(&series.name)
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="13" font-weight="bold" fill="{}">{}</text>"#,
        x + width - 5, y + 20, signal_color, signal_text
    ));

    if series.points.is_empty() {
        return svg;
    }

    // Pad the range so it always spans zero and the inversion line is meaningful.
    let (data_min, data_max) = data_bounds(&series.points);
    let frame = ChartFrame::new(x, y, width, height, data_min.min(-0.1), data_max.max(0.5));

    let num_points = series.points.len();

    // Clip all drawing to the chart area
    let clip_id = format!("ycClip_{}_{}", x, y);
    svg.push_str(&format!(
        r#"<defs><clipPath id="{}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath></defs>"#,
        clip_id, frame.x, frame.y, frame.w, frame.h
    ));

    // ── Vertical signal bands ─────────────────────────────────────────────────
    // Draw one band per data point; merge consecutive identical signals for efficiency.
    let signal_color = |s: &SteepeningType| match s {
        SteepeningType::BullSteepening => "#ff8888",
        SteepeningType::Inverting => "#ffbbaa",
        SteepeningType::Flattening => "#ffe8aa",
        SteepeningType::BearSteepening => "#ffffaa",
        SteepeningType::Stable => "#ccffcc",
    };

    if num_points > 1 {
        let mut band_start = 0usize;
        let mut band_sig = signals.first().unwrap_or(&SteepeningType::Stable);

        let px = |i: usize| -> i32 { frame.px(i, num_points) };

        for i in 1..=num_points {
            let cur_sig = if i < num_points {
                signals.get(i).unwrap_or(&SteepeningType::Stable)
            } else {
                // sentinel: force flush
                &SteepeningType::BullSteepening
            };

            let flush = i == num_points
                || std::mem::discriminant(cur_sig) != std::mem::discriminant(band_sig);
            if flush {
                let bx = px(band_start);
                let bx2 = px(if i < num_points { i } else { num_points - 1 });
                let bw = (bx2 - bx).max(1);
                svg.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" clip-path="url(#{})"/>"#,
                    bx, frame.y, bw, frame.h, signal_color(band_sig), clip_id
                ));
                band_start = i;
                band_sig = cur_sig;
            }
        }
    }

    // ── Spread level line ─────────────────────────────────────────────────────
    if num_points > 1 {
        svg.push_str(&format!(
            r#"<path d="{}" fill="none" stroke="black" stroke-width="2" clip-path="url(#{})"/>"#,
            frame.polyline(&series.points),
            clip_id
        ));
    }

    // ── Zero line ─────────────────────────────────────────────────────────────
    let zero_y = frame.py(0.0);
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1" stroke-dasharray="4,2"/>"#,
        frame.x, zero_y, frame.x + frame.w, zero_y
    ));

    // ── Y-axis labels ─────────────────────────────────────────────────────────
    svg.push_str(&frame.axis_labels(3, |v| format!("{:+.1}%", v)));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="9" fill="black">0%</text>"#,
        frame.x - 3,
        zero_y + 3
    ));

    // ── Chart border (drawn last so it's on top) ──────────────────────────────
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="black" stroke-width="1"/>"#,
        frame.x, frame.y, frame.w, frame.h
    ));

    svg
}

/// The inner plotting area of a chart panel, together with the value range
/// mapped onto it.
///
/// Every element of a chart — the curve, threshold lines, gradient stops and
/// axis labels — must be positioned through one shared frame. Previously the
/// callers padded the range so their thresholds stayed on screen while the
/// plotting helper re-derived a tighter range from the data alone, so the
/// dashed threshold lines were drawn at a different scale than the curve they
/// were meant to annotate.
struct ChartFrame {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    min: f64,
    max: f64,
}

impl ChartFrame {
    /// `panel_*` is the outer bordered box; the plot area is inset to leave
    /// room for the title row and the y-axis labels.
    fn new(panel_x: i32, panel_y: i32, panel_w: i32, panel_h: i32, min: f64, max: f64) -> Self {
        Self {
            x: panel_x + 40,
            y: panel_y + 35,
            w: panel_w - 50,
            h: panel_h - 55,
            min,
            max: if max > min { max } else { min + 1.0 },
        }
    }

    fn range(&self) -> f64 {
        self.max - self.min
    }

    fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// Horizontal position of sample `i` of `n`.
    fn px(&self, i: usize, n: usize) -> i32 {
        self.x + (self.w * i as i32) / n.saturating_sub(1).max(1) as i32
    }

    /// Vertical position of `value` (higher values sit nearer the top).
    fn py(&self, value: f64) -> i32 {
        self.bottom() - ((value - self.min) / self.range() * self.h as f64) as i32
    }

    /// Height of `value` as a percentage measured from the bottom of the
    /// frame — the coordinate space SVG gradient stops use.
    fn pct(&self, value: f64) -> f64 {
        ((value - self.min) / self.range() * 100.0).clamp(0.0, 100.0)
    }

    fn polyline(&self, points: &[DataPoint]) -> String {
        let mut path = String::new();
        for (i, point) in points.iter().enumerate() {
            let cmd = if i == 0 { "M" } else { " L" };
            path.push_str(&format!(
                "{} {} {}",
                cmd,
                self.px(i, points.len()),
                self.py(point.value)
            ));
        }
        path
    }

    fn axis_labels(&self, offset: i32, format: impl Fn(f64) -> String) -> String {
        svg_common::axis_minmax_labels(
            (self.x - offset) as f64,
            (self.y + 5) as f64,
            self.bottom() as f64,
            &format(self.max),
            &format(self.min),
        )
    }

    fn threshold_line(&self, value: f64, color: &str) -> String {
        format!(
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
            self.x,
            self.py(value),
            self.x + self.w,
            self.py(value),
            color
        )
    }
}

/// Min and max value of a series. Callers early-return on empty input, so the
/// fallbacks are unreachable in practice.
fn data_bounds(points: &[DataPoint]) -> (f64, f64) {
    let min = points
        .iter()
        .map(|p| p.value)
        .min_by(f64::total_cmp)
        .unwrap_or(0.0);
    let max = points
        .iter()
        .map(|p| p.value)
        .max_by(f64::total_cmp)
        .unwrap_or(1.0);
    (min, max)
}

/// A vertical gradient (0% = bottom of the frame, 100% = top) wrapped in its
/// own `<defs>` block. Stop offsets come from `ChartFrame::pct` so the colour
/// bands line up with the values they represent.
fn gradient_def(id: &str, stops: &[(f64, &str)]) -> String {
    let mut out = format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="100%" x2="0%" y2="0%">"#,
        id
    );
    for (offset, color) in stops {
        out.push_str(&format!(
            r#"<stop offset="{}%" style="stop-color:{};stop-opacity:1" />"#,
            offset, color
        ));
    }
    out.push_str(r#"</linearGradient></defs>"#);
    out
}

/// Filled area under the series, plus min/max axis labels.
fn draw_area_series(svg: &mut String, points: &[DataPoint], frame: &ChartFrame, fill: &str) {
    if points.is_empty() {
        return;
    }

    let mut path = format!("M {} {}", frame.x, frame.bottom());
    for (i, point) in points.iter().enumerate() {
        path.push_str(&format!(
            " L {} {}",
            frame.px(i, points.len()),
            frame.py(point.value)
        ));
    }
    path.push_str(&format!(" L {} {} Z", frame.x + frame.w, frame.bottom()));

    svg.push_str(&format!(
        r#"<path d="{}" fill="{}" fill-opacity="0.3" stroke="black" stroke-width="1"/>"#,
        path, fill
    ));
    svg.push_str(&frame.axis_labels(5, |v| format!("{:.1}", v)));
}
