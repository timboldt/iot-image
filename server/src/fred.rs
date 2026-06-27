use chrono::{Local, NaiveDate, Timelike};
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

#[derive(Debug, Clone)]
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
    pub yield_curve: SeriesData,
    /// Per-point signal classification, parallel to yield_curve.points
    pub yield_curve_signals: Vec<SteepeningType>,
    pub yield_curve_steepening: SteepeningType,
    /// Current raw T10Y3M spread level (percentage points, from FRED)
    pub yield_curve_level: f64,
    /// Scaled velocity change per day over the last 7 days (units: VELOCITY_SCALE / day)
    pub yield_curve_acceleration: f64,
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

struct KalmanFilter {
    // State vector: [position, velocity]
    x: [f64; 2],
    // Covariance matrix: 2x2
    p: [[f64; 2]; 2],
    // Process noise
    q: [[f64; 2]; 2],
    // Measurement noise
    r: f64,
}

impl KalmanFilter {
    fn new(initial_position: f64) -> Self {
        Self {
            x: [initial_position, 0.0],
            p: [[1.0, 0.0], [0.0, 1.0]],
            // Position process noise ~0.07 pp/day; velocity process noise tightened to
            // reduce spurious jumps while staying responsive to real trend changes.
            q: [[0.005, 0.0], [0.0, 0.00005]],
            // Measurement noise: FRED yields are in percentage points (e.g. 0.35).
            // Daily noise std ~0.05 pp → R ≈ 0.0025.  Use 0.01 for slight smoothing.
            r: 0.01,
        }
    }

    fn predict(&mut self, dt_days: f64) {
        // State transition: x = F * x, where F = [[1, dt], [0, 1]]
        let x0 = self.x[0] + self.x[1] * dt_days;
        let x1 = self.x[1];
        self.x = [x0, x1];

        // Covariance: P = F * P * F^T + Q
        let p00 = self.p[0][0]
            + 2.0 * dt_days * self.p[0][1]
            + dt_days * dt_days * self.p[1][1]
            + self.q[0][0];
        let p01 = self.p[0][1] + dt_days * self.p[1][1] + self.q[0][1];
        let p10 = p01;
        let p11 = self.p[1][1] + self.q[1][1];
        self.p = [[p00, p01], [p10, p11]];
    }

    fn update(&mut self, measurement: f64) {
        // Measurement model: H = [1, 0]
        let innovation = measurement - self.x[0];
        let innovation_covariance = self.p[0][0] + self.r;
        let k0 = self.p[0][0] / innovation_covariance;
        let k1 = self.p[1][0] / innovation_covariance;

        self.x[0] += k0 * innovation;
        self.x[1] += k1 * innovation;

        // Covariance update: P = (I - K * H) * P
        let p00 = (1.0 - k0) * self.p[0][0];
        let p01 = (1.0 - k0) * self.p[0][1];
        let p10 = self.p[1][0] - k1 * self.p[0][0];
        let p11 = self.p[1][1] - k1 * self.p[0][1];
        self.p = [[p00, p01], [p10, p11]];
    }
}

fn compute_velocity_series(points: &[DataPoint]) -> Vec<DataPoint> {
    if points.is_empty() {
        return Vec::new();
    }

    let mut filter = KalmanFilter::new(points[0].value);
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

        let velocity = filter.x[1];
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

/// Estimates acceleration as the average rate of velocity change over the last
/// `window_days` observations.  Returns scaled units (VELOCITY_SCALE / day).
fn compute_acceleration(velocity_points: &[DataPoint], window_days: usize) -> f64 {
    let n = velocity_points.len();
    if n < 2 {
        return 0.0;
    }
    let window = window_days.min(n - 1);
    let recent = velocity_points[n - 1].value;
    let past = velocity_points[n - 1 - window].value;
    (recent - past) / window as f64
}

fn build_date_map(points: &[DataPoint]) -> std::collections::HashMap<String, f64> {
    points.iter().map(|p| (p.date.clone(), p.value)).collect()
}

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
    let dgs10_vel = dgs10_vel_windowed
        .last()
        .map(|p| p.value)
        .unwrap_or(0.0);
    let dtb3_vel = dtb3_vel_windowed
        .last()
        .map(|p| p.value)
        .unwrap_or(0.0);

    let steepening = if spread_vel.abs() < YIELD_CURVE_VEL_THRESHOLD {
        SteepeningType::Stable
    } else if spread_vel > YIELD_CURVE_VEL_THRESHOLD {
        // 3M falling faster than 10Y rising → bull steepening (crisis)
        if dtb3_vel < -YIELD_CURVE_VEL_THRESHOLD
            && (-dtb3_vel) > dgs10_vel.max(0.0)
        {
            SteepeningType::BullSteepening
        } else {
            SteepeningType::BearSteepening
        }
    } else {
        // Negative velocity (spread shrinking). Distinguish by the current spread LEVEL:
        // - Spread still positive: "Flattening" (bearish trend but not yet inverted)
        // - Spread negative or near zero: "Inverting" (deepening inversion)
        let current_spread = yield_curve
            .last()
            .map(|p| p.value)
            .unwrap_or(0.0);
        if current_spread > 0.1 {
            SteepeningType::Flattening
        } else {
            SteepeningType::Inverting
        }
    };
    // Warn unused; stored for potential future use in chart annotations.
    let _ = dgs10_vel;

    let acceleration = compute_acceleration(&yield_curve_velocity_windowed, 7);
    let current_spread_level = yield_curve.last().map(|p| p.value).unwrap_or(0.0);

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
        yield_curve: SeriesData {
            symbol: "T10Y3M".to_string(),
            name: "Yield Curve (10Y-3M)".to_string(),
            points: yield_curve_windowed,
        },
        yield_curve_signals,
        yield_curve_steepening: steepening,
        yield_curve_level: current_spread_level,
        yield_curve_acceleration: acceleration,
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
        fred.yield_curve_level,
        fred.yield_curve_acceleration,
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
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.total_cmp(b))
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
    // Gradient goes from top to bottom for inverted Y axis
    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="0%" x2="0%" y2="100%">"#,
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

    // Draw area chart with gradient (inverted Y axis)
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
        true, // invert_y
    );

    // Draw threshold lines (only if within actual data range) - inverted Y axis
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(calm_threshold, "green"), (fear_threshold, "red")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y = chart_y + ((threshold - min_val) / range * chart_h as f64) as i32;
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
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.total_cmp(b))
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
        false, // invert_y
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
    let normal_threshold = 3.0;
    let stress_threshold = 4.0;

    // Calculate data range, ensuring thresholds are always visible
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.total_cmp(b))
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
    // Gradient goes from top to bottom for inverted Y axis

    svg.push_str(&format!(
        r#"<defs><linearGradient id="{}" x1="0%" y1="0%" x2="0%" y2="100%">"#,
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

    // Draw area chart with gradient (inverted Y axis)
    generate_area_chart_internal(
        &mut svg,
        series,
        x,
        y,
        width,
        height,
        &format!("url(#{})", gradient_id),
        None,
        true, // invert_y
    );

    // Draw regime threshold lines (only if within actual data range) - inverted Y axis
    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_h = height - 55;
    let chart_w = width - 50;

    for (threshold, color) in [(normal_threshold, "green"), (stress_threshold, "red")] {
        if threshold >= data_min && threshold <= data_max {
            let line_y = chart_y + ((threshold - min_val) / range * chart_h as f64) as i32;
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2" stroke-dasharray="8,4"/>"#,
                chart_x, line_y, chart_x + chart_w, line_y, color
            ));
        }
    }

    svg
}

fn generate_yield_curve_chart(
    series: &SeriesData,
    signals: &[SteepeningType],
    steepening: &SteepeningType,
    _spread_level: f64,
    acceleration: f64,
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
            r#"<text x="{}" y="{}" text-anchor="end" font-size="14" fill="black">{:+.2}%</text>"#,
            x + width - 5,
            y + 20,
            last.value
        ));
    }

    // Steepening signal label (left) and acceleration (right) on the sub-header row
    let (signal_text, signal_color) = match steepening {
        SteepeningType::BullSteepening => ("BULL STEEP \u{26a0} Crisis Signal", "red"),
        SteepeningType::BearSteepening => ("Bear Steep \u{2014} Expansion", "#cc8800"),
        SteepeningType::Flattening => ("Flattening \u{2014} Caution", "#cc8800"),
        SteepeningType::Inverting => ("Inverting \u{26a0} Warning", "red"),
        SteepeningType::Stable => ("Stable", "#666666"),
    };
    let spread_label = format!("vel:{:+.1}  accel:{:+.1}", acceleration * 7.0, acceleration);
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="start" font-size="11" font-weight="bold" fill="{}">{}</text>"#,
        x + 5,
        y + 32,
        signal_color,
        signal_text
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="11" fill="black">{}</text>"#,
        x + width - 5,
        y + 32,
        spread_label
    ));

    if series.points.is_empty() {
        return svg;
    }

    let chart_x = x + 40;
    let chart_y = y + 35;
    let chart_w = width - 50;
    let chart_h = height - 55;

    // Calculate data range, always spanning zero so the inversion line is meaningful
    let data_min = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(-2.0);
    let data_max = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.total_cmp(b))
        .unwrap_or(3.0);

    let min_val = data_min.min(-0.1);
    let max_val = data_max.max(0.5);
    let range = if max_val > min_val { max_val - min_val } else { 1.0 };

    let num_points = series.points.len();

    // Clip all drawing to the chart area
    let clip_id = format!("ycClip_{}_{}", x, y);
    svg.push_str(&format!(
        r#"<defs><clipPath id="{}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath></defs>"#,
        clip_id, chart_x, chart_y, chart_w, chart_h
    ));

    // ── Vertical signal bands ─────────────────────────────────────────────────
    // Draw one band per data point; merge consecutive identical signals for efficiency.
    let signal_color = |s: &SteepeningType| match s {
        SteepeningType::BullSteepening => "#ff8888",
        SteepeningType::Inverting     => "#ffbbaa",
        SteepeningType::Flattening    => "#ffe8aa",
        SteepeningType::BearSteepening => "#ffffaa",
        SteepeningType::Stable        => "#ccffcc",
    };

    if num_points > 1 {
        let mut band_start = 0usize;
        let mut band_sig = signals.get(0).unwrap_or(&SteepeningType::Stable);

        let px = |i: usize| -> i32 {
            chart_x + (chart_w * i as i32) / (num_points - 1).max(1) as i32
        };

        for i in 1..=num_points {
            let cur_sig = if i < num_points {
                signals.get(i).unwrap_or(&SteepeningType::Stable)
            } else {
                // sentinel: force flush
                &SteepeningType::BullSteepening
            };

            let flush = i == num_points || std::mem::discriminant(cur_sig) != std::mem::discriminant(band_sig);
            if flush {
                let bx = px(band_start);
                let bx2 = px(if i < num_points { i } else { num_points - 1 });
                let bw = (bx2 - bx).max(1);
                svg.push_str(&format!(
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" clip-path="url(#{})"/>"#,
                    bx, chart_y, bw, chart_h, signal_color(band_sig), clip_id
                ));
                band_start = i;
                band_sig = cur_sig;
            }
        }
    }

    // ── Spread level line ─────────────────────────────────────────────────────
    if num_points > 1 {
        let mut path = String::new();
        for (i, point) in series.points.iter().enumerate() {
            let px = chart_x + (chart_w * i as i32) / (num_points - 1).max(1) as i32;
            let py = chart_y + chart_h
                - ((point.value - min_val) / range * chart_h as f64) as i32;
            if i == 0 {
                path.push_str(&format!("M {} {}", px, py));
            } else {
                path.push_str(&format!(" L {} {}", px, py));
            }
        }
        svg.push_str(&format!(
            r#"<path d="{}" fill="none" stroke="black" stroke-width="2" clip-path="url(#{})"/>"#,
            path, clip_id
        ));
    }

    // ── Zero line ─────────────────────────────────────────────────────────────
    let zero_y = chart_y + chart_h - ((0.0_f64 - min_val) / range * chart_h as f64) as i32;
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="1" stroke-dasharray="4,2"/>"#,
        chart_x, zero_y, chart_x + chart_w, zero_y
    ));

    // ── Y-axis labels ─────────────────────────────────────────────────────────
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:+.1}%</text>"#,
        chart_x - 3, chart_y + 5, max_val
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:+.1}%</text>"#,
        chart_x - 3, chart_y + chart_h, min_val
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="9" fill="red">0%</text>"#,
        chart_x - 3, zero_y + 3
    ));

    // ── Chart border (drawn last so it's on top) ──────────────────────────────
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="black" stroke-width="1"/>"#,
        chart_x, chart_y, chart_w, chart_h
    ));

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
    invert_y: bool,
) {
    if series.points.is_empty() {
        return;
    }

    let min_val = series
        .points
        .iter()
        .map(|p| p.value)
        .min_by(|a, b| a.total_cmp(b))
        .unwrap_or(0.0);
    let max_val = series
        .points
        .iter()
        .map(|p| p.value)
        .max_by(|a, b| a.total_cmp(b))
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
            let py = if invert_y {
                chart_y + ((point.value - min_val) / range * chart_h as f64) as i32
            } else {
                chart_y + chart_h - ((point.value - min_val) / range * chart_h as f64) as i32
            };
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
                let threshold_y = if invert_y {
                    chart_y + ((threshold_val - min_val) / range * chart_h as f64) as i32
                } else {
                    chart_y + chart_h - ((threshold_val - min_val) / range * chart_h as f64) as i32
                };
                svg.push_str(&format!(
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="darkred" stroke-width="1" stroke-dasharray="4,2"/>"#,
                    chart_x, threshold_y, chart_x + chart_w, threshold_y
                ));
            }
        }
    }

    // Y-axis labels (min and max) - swap if inverted
    if invert_y {
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.1}</text>"#,
            chart_x - 5,
            chart_y + 5,
            min_val
        ));
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.1}</text>"#,
            chart_x - 5,
            chart_y + chart_h,
            max_val
        ));
    } else {
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
}
