use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::path::Path;

// ============================================================================
// Layer 1: Data Structures
// ============================================================================

#[derive(Debug, Clone)]
pub struct WeightReading {
    pub timestamp: DateTime<Utc>,
    pub weight_lbs: f64,
}

#[derive(Debug, Clone)]
pub struct KalmanState {
    pub timestamp: DateTime<Utc>,
    pub weight_lbs: f64,
    pub velocity_lbs_per_day: f64,
}

#[derive(Debug, Clone)]
pub struct ProjectionPoint {
    pub timestamp: DateTime<Utc>,
    pub weight_lbs: f64,
}

#[derive(Debug, Clone)]
pub struct WeightData {
    pub raw_readings: Vec<WeightReading>,
    pub kalman_states: Vec<KalmanState>,
    pub linear_projection: Vec<ProjectionPoint>,
    pub decay_projection: Vec<ProjectionPoint>,
    pub stall_point: Option<ProjectionPoint>,
    #[allow(dead_code)]
    pub start_date: DateTime<Utc>,
    pub today: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CsvRecord {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "Actual Weight")]
    actual_weight: f64,
}

// ============================================================================
// Layer 2: Data Processing - CSV Reading
// ============================================================================

pub fn read_weight_csv(path: &Path) -> Result<Vec<WeightReading>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut rdr = csv::Reader::from_reader(file);

    let mut readings = Vec::new();

    for result in rdr.deserialize() {
        let record: CsvRecord = result?;

        // Parse date as NaiveDate (YYYY-MM-DD)
        let naive_date = NaiveDate::parse_from_str(&record.date, "%Y-%m-%d")?;

        // Convert to DateTime<Utc> at noon
        let timestamp = naive_date
            .and_hms_opt(12, 0, 0)
            .ok_or("Invalid time")?
            .and_utc();

        readings.push(WeightReading {
            timestamp,
            weight_lbs: record.actual_weight,
        });
    }

    // Sort chronologically (oldest first)
    readings.sort_by_key(|r| r.timestamp);

    Ok(readings)
}

// ============================================================================
// Layer 2: Data Processing - Kalman Filter
// ============================================================================

struct KalmanFilter {
    // State vector: [weight, velocity]
    x: [f64; 2],
    // Covariance matrix: 2x2
    p: [[f64; 2]; 2],
    // Process noise
    q: [[f64; 2]; 2],
    // Measurement noise
    r: f64,
}

impl KalmanFilter {
    fn new(initial_weight: f64) -> Self {
        Self {
            x: [initial_weight, -0.5], // Initial velocity estimate: -0.5 lbs/day
            p: [[1.0, 0.0], [0.0, 1.0]], // Initial covariance
            q: [[0.005, 0.0], [0.0, 0.0005]], // Process noise (reduced for more smoothing)
            r: 1.5, // Measurement noise (increased for more smoothing)
        }
    }

    fn predict(&mut self, dt_days: f64) {
        // State transition: x = F * x
        // F = [[1, dt], [0, 1]]
        let x0 = self.x[0] + self.x[1] * dt_days;
        let x1 = self.x[1];
        self.x = [x0, x1];

        // Covariance: P = F * P * F^T + Q
        let p00 = self.p[0][0] + 2.0 * dt_days * self.p[0][1] + dt_days * dt_days * self.p[1][1] + self.q[0][0];
        let p01 = self.p[0][1] + dt_days * self.p[1][1] + self.q[0][1];
        let p10 = p01; // Symmetric
        let p11 = self.p[1][1] + self.q[1][1];

        self.p = [[p00, p01], [p10, p11]];
    }

    fn update(&mut self, measurement: f64) {
        // Measurement model: H = [1, 0]
        // Innovation: y = z - H * x
        let y = measurement - self.x[0];

        // Innovation covariance: S = H * P * H^T + R
        let s = self.p[0][0] + self.r;

        // Kalman gain: K = P * H^T / S
        let k0 = self.p[0][0] / s;
        let k1 = self.p[1][0] / s;

        // Update state: x = x + K * y
        self.x[0] += k0 * y;
        self.x[1] += k1 * y;

        // Update covariance: P = (I - K * H) * P
        let p00 = (1.0 - k0) * self.p[0][0];
        let p01 = (1.0 - k0) * self.p[0][1];
        let p10 = self.p[1][0] - k1 * self.p[0][0];
        let p11 = self.p[1][1] - k1 * self.p[0][1];

        self.p = [[p00, p01], [p10, p11]];
    }
}

pub fn process_weight_data(readings: &[WeightReading]) -> Vec<KalmanState> {
    if readings.is_empty() {
        return Vec::new();
    }

    let mut filter = KalmanFilter::new(readings[0].weight_lbs);
    let mut states = Vec::new();

    // Add initial state
    states.push(KalmanState {
        timestamp: readings[0].timestamp,
        weight_lbs: filter.x[0],
        velocity_lbs_per_day: filter.x[1],
    });

    // Process subsequent readings
    for i in 1..readings.len() {
        let dt = (readings[i].timestamp - readings[i-1].timestamp).num_seconds() as f64 / 86400.0;

        filter.predict(dt);
        filter.update(readings[i].weight_lbs);

        states.push(KalmanState {
            timestamp: readings[i].timestamp,
            weight_lbs: filter.x[0],
            velocity_lbs_per_day: filter.x[1],
        });
    }

    states
}

// ============================================================================
// Layer 2: Data Processing - Projections
// ============================================================================

pub fn calculate_linear_projection(
    last_state: &KalmanState,
    days_ahead: i64,
) -> Vec<ProjectionPoint> {
    let mut projection = Vec::new();

    for day in 0..=days_ahead {
        let timestamp = last_state.timestamp + Duration::days(day);
        let weight_lbs = last_state.weight_lbs + last_state.velocity_lbs_per_day * day as f64;

        projection.push(ProjectionPoint {
            timestamp,
            weight_lbs,
        });
    }

    projection
}

pub fn calculate_decay_projection(
    last_state: &KalmanState,
    days_ahead: i64,
) -> (Vec<ProjectionPoint>, Option<ProjectionPoint>) {
    let mut projection = Vec::new();
    let mut stall_point = None;

    let mut weight = last_state.weight_lbs;
    let mut velocity = last_state.velocity_lbs_per_day;

    // Metabolic resistance acceleration: +0.0177 lbs/week/day = +0.0177/7 lbs/day/day
    let acceleration = 0.0177 / 7.0;

    for day in 0..=days_ahead {
        let timestamp = last_state.timestamp + Duration::days(day);

        projection.push(ProjectionPoint {
            timestamp,
            weight_lbs: weight,
        });

        // Check if we've reached stall point (velocity >= 0)
        if velocity >= 0.0 && stall_point.is_none() {
            stall_point = Some(ProjectionPoint {
                timestamp,
                weight_lbs: weight,
            });
            break;
        }

        // Update for next iteration
        weight += velocity;
        velocity += acceleration;
    }

    (projection, stall_point)
}

// ============================================================================
// Layer 2: Data Processing - Main Fetch Function
// ============================================================================

pub async fn fetch_weight_data(
    csv_path: &Path,
) -> Result<WeightData, Box<dyn Error>> {
    // Read CSV
    let raw_readings = read_weight_csv(csv_path)?;

    if raw_readings.is_empty() {
        return Err("No weight data found in CSV".into());
    }

    // Process through Kalman filter
    let kalman_states = process_weight_data(&raw_readings);

    if kalman_states.is_empty() {
        return Err("Failed to process weight data".into());
    }

    let last_state = kalman_states.last().unwrap();
    let start_date = raw_readings.first().unwrap().timestamp;
    let today = Utc::now();

    // Calculate projections
    let linear_projection = calculate_linear_projection(last_state, 90);
    let (decay_projection, stall_point) = calculate_decay_projection(last_state, 90);

    Ok(WeightData {
        raw_readings,
        kalman_states,
        linear_projection,
        decay_projection,
        stall_point,
        start_date,
        today,
    })
}

// ============================================================================
// Layer 3: SVG Generation - Forecast Chart
// ============================================================================

pub fn generate_forecast_svg(data: &WeightData, battery_pct: Option<u8>) -> String {
    // Chart dimensions
    let width = 800;
    let height = 480;
    let margin_left = 60;
    let margin_right = 40;
    let margin_top = 50;
    let margin_bottom = 50;
    let chart_width = width - margin_left - margin_right;
    let chart_height = height - margin_top - margin_bottom;

    // Date range: -60 to +90 days from today
    let days_back = 60;
    let days_forward = 90;
    let x_min = -days_back as f64;
    let x_max = days_forward as f64;

    // Calculate y-axis range
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;

    for reading in &data.raw_readings {
        y_min = y_min.min(reading.weight_lbs);
        y_max = y_max.max(reading.weight_lbs);
    }

    for point in &data.linear_projection {
        y_min = y_min.min(point.weight_lbs);
        y_max = y_max.max(point.weight_lbs);
    }

    for point in &data.decay_projection {
        y_min = y_min.min(point.weight_lbs);
        y_max = y_max.max(point.weight_lbs);
    }

    y_min -= 2.0;
    y_max += 2.0;

    // Helper functions for coordinate conversion
    let days_from_today = |timestamp: DateTime<Utc>| -> f64 {
        (timestamp - data.today).num_seconds() as f64 / 86400.0
    };

    let x_to_pixel = |days: f64| -> f64 {
        margin_left as f64 + (days - x_min) / (x_max - x_min) * chart_width as f64
    };

    let y_to_pixel = |weight: f64| -> f64 {
        margin_top as f64 + (y_max - weight) / (y_max - y_min) * chart_height as f64
    };

    // Start building SVG
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        width, height, width, height
    );

    // Define gradient for battery bar
    svg.push_str(r#"<defs>"#);
    svg.push_str(r#"<linearGradient id="batteryGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push_str(r#"</linearGradient>"#);
    svg.push_str(r#"</defs>"#);

    // White background
    svg.push_str(&format!(
        r#"<rect width="{}" height="{}" fill="white"/>"#,
        width, height
    ));

    // Title
    svg.push_str(&format!(
        r#"<text x="{}" y="30" text-anchor="middle" font-size="24" font-weight="bold" fill="black">90-Day Weight Forecast</text>"#,
        width / 2
    ));

    // Battery bar in top right
    if let Some(pct) = battery_pct {
        let battery_bar_width = 100;
        let battery_bar_height = 12;
        let battery_x = width - margin_right - battery_bar_width - 10;
        let battery_y = 10;
        let battery_inset = 2;
        let battery_fill_width = (battery_bar_width - battery_inset * 2) * pct as i32 / 100;

        // Label
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="11" fill="black">Battery:</text>"#,
            battery_x - 5,
            battery_y + 10
        ));

        // Background (container) rectangle
        svg.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="1.5" rx="2"/>"#,
            battery_x, battery_y, battery_bar_width, battery_bar_height
        ));

        // ClipPath for battery bar
        svg.push_str(&format!(r#"<clipPath id="batteryClipForecast{}"><rect x="{}" y="{}" width="{}" height="{}" rx="1"/></clipPath>"#,
            pct,
            battery_x + battery_inset,
            battery_y + battery_inset,
            battery_fill_width,
            battery_bar_height - battery_inset * 2
        ));

        // Full-width gradient rect, clipped
        svg.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#batteryGradient)" clip-path="url(#batteryClipForecast{})" rx="1"/>"#,
            battery_x + battery_inset,
            battery_y + battery_inset,
            battery_bar_width - battery_inset * 2,
            battery_bar_height - battery_inset * 2,
            pct
        ));
    }

    // Draw grid lines
    for days in [-60, -30, 0, 30, 60, 90].iter() {
        let x = x_to_pixel(*days as f64);
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="1"/>"##,
            x, margin_top, x, height - margin_bottom
        ));
    }

    // Draw horizontal grid lines
    let y_step = ((y_max - y_min) / 5.0).ceil();
    let mut y_val = (y_min / y_step).ceil() * y_step;
    while y_val <= y_max {
        let y = y_to_pixel(y_val);
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="1"/>"##,
            margin_left, y, width - margin_right, y
        ));
        y_val += y_step;
    }

    // Draw axes
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>"#,
        margin_left, height - margin_bottom, width - margin_right, height - margin_bottom
    ));
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>"#,
        margin_left, margin_top, margin_left, height - margin_bottom
    ));

    // X-axis labels
    for days in [-60, -30, 0, 30, 60, 90].iter() {
        let x = x_to_pixel(*days as f64);
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="middle" font-size="12" fill="black">{}</text>"#,
            x, height - margin_bottom + 20, days
        ));
    }

    // Y-axis labels
    y_val = (y_min / y_step).ceil() * y_step;
    while y_val <= y_max {
        let y = y_to_pixel(y_val);
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="12" fill="black">{:.0}</text>"#,
            margin_left - 10, y + 4.0, y_val
        ));
        y_val += y_step;
    }

    // Raw readings scatter (light gray circles)
    for reading in &data.raw_readings {
        let days = days_from_today(reading.timestamp);
        if days >= x_min && days <= x_max {
            let x = x_to_pixel(days);
            let y = y_to_pixel(reading.weight_lbs);
            svg.push_str(&format!(
                r##"<circle cx="{}" cy="{}" r="2" fill="#aaaaaa" opacity="0.6"/>"##,
                x, y
            ));
        }
    }

    // Decay projection (orange solid)
    if !data.decay_projection.is_empty() {
        let mut path = String::from("M");
        for (i, point) in data.decay_projection.iter().enumerate() {
            let days = days_from_today(point.timestamp);
            let x = x_to_pixel(days);
            let y = y_to_pixel(point.weight_lbs);

            if i == 0 {
                path.push_str(&format!("{},{}", x, y));
            } else {
                path.push_str(&format!(" L{},{}", x, y));
            }
        }
        svg.push_str(&format!(
            r#"<path d="{}" stroke="orange" stroke-width="1.5" fill="none"/>"#,
            path
        ));
    }

    // Linear projection (blue dashed)
    if !data.linear_projection.is_empty() {
        let mut path = String::from("M");
        for (i, point) in data.linear_projection.iter().enumerate() {
            let days = days_from_today(point.timestamp);
            let x = x_to_pixel(days);
            let y = y_to_pixel(point.weight_lbs);

            if i == 0 {
                path.push_str(&format!("{},{}", x, y));
            } else {
                path.push_str(&format!(" L{},{}", x, y));
            }
        }
        svg.push_str(&format!(
            r#"<path d="{}" stroke="blue" stroke-width="2" stroke-dasharray="5,3" fill="none"/>"#,
            path
        ));
    }

    // Kalman filtered line (black bold)
    if !data.kalman_states.is_empty() {
        let mut path = String::from("M");
        for (i, state) in data.kalman_states.iter().enumerate() {
            let days = days_from_today(state.timestamp);
            if days >= x_min && days <= x_max {
                let x = x_to_pixel(days);
                let y = y_to_pixel(state.weight_lbs);

                if i == 0 {
                    path.push_str(&format!("{},{}", x, y));
                } else {
                    path.push_str(&format!(" L{},{}", x, y));
                }
            }
        }
        svg.push_str(&format!(
            r#"<path d="{}" stroke="black" stroke-width="2" fill="none"/>"#,
            path
        ));
    }

    // Stall point annotation
    if let Some(stall) = &data.stall_point {
        let days = days_from_today(stall.timestamp);
        let x = x_to_pixel(days);
        let y = y_to_pixel(stall.weight_lbs);

        svg.push_str(&format!(
            r#"<circle cx="{}" cy="{}" r="4" fill="red"/>"#,
            x, y
        ));
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" font-size="12" fill="red">Stall Point</text>"#,
            x + 10.0, y - 5.0
        ));
    }

    // Footer
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");

    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="middle" font-size="12" fill="black">{}</text>"#,
        width / 2, height - 10, timestamp
    ));

    svg.push_str("</svg>");
    svg
}

// ============================================================================
// Layer 3: SVG Generation - Velocity Chart
// ============================================================================

pub fn generate_velocity_svg(data: &WeightData, battery_pct: Option<u8>) -> String {
    // Chart dimensions
    let width = 800;
    let height = 480;
    let margin_left = 60;
    let margin_right = 40;
    let margin_top = 40;
    let margin_bottom = 60;
    let gap = 10;

    // Split into two panels
    let total_chart_height = height - margin_top - margin_bottom;
    let top_height = (total_chart_height * 3) / 5; // 60% for weight chart
    let bottom_height = total_chart_height - top_height - gap; // 40% for velocity
    let chart_width = width - margin_left - margin_right;

    // Date range: past 90 days
    let x_min = data.today - Duration::days(90);
    let total_days = 90.0;

    // Top panel Y-axis: weight range (only for past 90 days)
    let mut weight_min = f64::MAX;
    let mut weight_max = f64::MIN;
    for reading in &data.raw_readings {
        if reading.timestamp >= x_min {
            weight_min = weight_min.min(reading.weight_lbs);
            weight_max = weight_max.max(reading.weight_lbs);
        }
    }
    for state in &data.kalman_states {
        if state.timestamp >= x_min {
            weight_min = weight_min.min(state.weight_lbs);
            weight_max = weight_max.max(state.weight_lbs);
        }
    }
    weight_min -= 1.0;
    weight_max += 1.0;

    // Bottom panel Y-axis range: -2.0 to +2.0 lbs/week
    let vel_min = -2.0;
    let vel_max = 2.0;

    // Helper functions for coordinate conversion
    let x_to_pixel = |timestamp: DateTime<Utc>| -> f64 {
        let days = (timestamp - x_min).num_seconds() as f64 / 86400.0;
        margin_left as f64 + (days / total_days) * chart_width as f64
    };

    let weight_to_pixel = |weight: f64| -> f64 {
        margin_top as f64 + (weight_max - weight) / (weight_max - weight_min) * top_height as f64
    };

    let velocity_to_pixel = |velocity_lbs_per_week: f64| -> f64 {
        let bottom_top = margin_top + top_height + gap;
        bottom_top as f64 + (vel_max - velocity_lbs_per_week) / (vel_max - vel_min) * bottom_height as f64
    };

    // Start building SVG
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
        width, height, width, height
    );

    // Define gradient for battery bar
    svg.push_str(r#"<defs>"#);
    svg.push_str(r#"<linearGradient id="batteryGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push_str(r#"<stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push_str(r#"<stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push_str(r#"</linearGradient>"#);
    svg.push_str(r#"</defs>"#);

    // White background
    svg.push_str(&format!(
        r#"<rect width="{}" height="{}" fill="white"/>"#,
        width, height
    ));

    // Title
    svg.push_str(&format!(
        r#"<text x="{}" y="25" text-anchor="middle" font-size="20" font-weight="bold" fill="black">Weight Analysis: Actual vs. Kalman Trend</text>"#,
        width / 2
    ));

    // Battery bar in top right
    if let Some(pct) = battery_pct {
        let battery_bar_width = 100;
        let battery_bar_height = 12;
        let battery_x = width - margin_right - battery_bar_width - 10;
        let battery_y = 5;
        let battery_inset = 2;
        let battery_fill_width = (battery_bar_width - battery_inset * 2) * pct as i32 / 100;

        // Label
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="11" fill="black">Battery:</text>"#,
            battery_x - 5,
            battery_y + 10
        ));

        // Background (container) rectangle
        svg.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="1.5" rx="2"/>"#,
            battery_x, battery_y, battery_bar_width, battery_bar_height
        ));

        // ClipPath for battery bar
        svg.push_str(&format!(r#"<clipPath id="batteryClip{}"><rect x="{}" y="{}" width="{}" height="{}" rx="1"/></clipPath>"#,
            pct, // unique ID
            battery_x + battery_inset,
            battery_y + battery_inset,
            battery_fill_width,
            battery_bar_height - battery_inset * 2
        ));

        // Full-width gradient rect, clipped
        svg.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#batteryGradient)" clip-path="url(#batteryClip{})" rx="1"/>"#,
            battery_x + battery_inset,
            battery_y + battery_inset,
            battery_bar_width - battery_inset * 2,
            battery_bar_height - battery_inset * 2,
            pct
        ));
    }

    // Define panel boundaries
    let top_panel_bottom = margin_top + top_height;
    let bottom_panel_top = top_panel_bottom + gap;
    let bottom_panel_bottom = height - margin_bottom;

    // Draw vertical grid lines for both panels
    let label_interval = ((total_days / 5.0).ceil() as i64).max(15);
    let mut day = 0;
    while day <= total_days as i64 {
        let timestamp = x_min + Duration::days(day);
        let x = x_to_pixel(timestamp);
        // Top panel grid
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="0.5"/>"##,
            x, margin_top, x, top_panel_bottom
        ));
        // Bottom panel grid
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="0.5"/>"##,
            x, bottom_panel_top, x, bottom_panel_bottom
        ));
        day += label_interval;
    }

    // Top panel horizontal grid lines (weight)
    let weight_step = ((weight_max - weight_min) / 4.0).ceil();
    let mut w = (weight_min / weight_step).ceil() * weight_step;
    while w <= weight_max {
        let y = weight_to_pixel(w);
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="0.5"/>"##,
            margin_left, y, width - margin_right, y
        ));
        w += weight_step;
    }

    // Bottom panel horizontal grid lines (velocity)
    for vel in [-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0].iter() {
        let y = velocity_to_pixel(*vel);
        svg.push_str(&format!(
            r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e0e0e0" stroke-width="0.5"/>"##,
            margin_left, y, width - margin_right, y
        ));
    }

    // Draw axes for top panel
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
        margin_left, top_panel_bottom, width - margin_right, top_panel_bottom
    ));
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
        margin_left, margin_top, margin_left, top_panel_bottom
    ));

    // Draw axes for bottom panel
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
        margin_left, bottom_panel_bottom, width - margin_right, bottom_panel_bottom
    ));
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
        margin_left, bottom_panel_top, margin_left, bottom_panel_bottom
    ));

    // X-axis labels (at bottom only)
    day = 0;
    while day <= total_days as i64 {
        let timestamp = x_min + Duration::days(day);
        let x = x_to_pixel(timestamp);
        let label = timestamp.format("%Y-%m-%d");
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="middle" font-size="10" fill="black">{}</text>"#,
            x, bottom_panel_bottom + 20, label
        ));
        day += label_interval;
    }

    // Y-axis labels for top panel (weight)
    w = (weight_min / weight_step).ceil() * weight_step;
    while w <= weight_max {
        let y = weight_to_pixel(w);
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.0}</text>"#,
            margin_left - 5, y + 3.0, w
        ));
        w += weight_step;
    }

    // Y-axis labels for bottom panel (velocity)
    for vel in [-2.0, -1.0, 0.0, 1.0, 2.0].iter() {
        let y = velocity_to_pixel(*vel);
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">{:.1}</text>"#,
            margin_left - 5, y + 3.0, vel
        ));
    }

    // Y-axis label for top panel
    svg.push_str(&format!(
        r#"<text x="15" y="{}" text-anchor="middle" font-size="11" fill="black" transform="rotate(-90 15 {})">Weight (lbs)</text>"#,
        margin_top + top_height / 2, margin_top + top_height / 2
    ));

    // Y-axis label for bottom panel
    svg.push_str(&format!(
        r#"<text x="15" y="{}" text-anchor="middle" font-size="11" fill="black" transform="rotate(-90 15 {})">Velocity (lbs/week)</text>"#,
        bottom_panel_top + bottom_height / 2, bottom_panel_top + bottom_height / 2
    ));

    // ===== TOP PANEL: Weight Chart =====

    // Draw actual weight readings as gray dots (past 90 days only)
    for reading in &data.raw_readings {
        if reading.timestamp >= x_min {
            let x = x_to_pixel(reading.timestamp);
            let y = weight_to_pixel(reading.weight_lbs);
            svg.push_str(&format!(
                r##"<circle cx="{}" cy="{}" r="2.5" fill="#999999" opacity="0.6"/>"##,
                x, y
            ));
        }
    }

    // Draw Kalman filtered line (blue solid, past 90 days only)
    if !data.kalman_states.is_empty() {
        let mut path = String::new();
        let mut first = true;
        for state in data.kalman_states.iter() {
            if state.timestamp >= x_min {
                let x = x_to_pixel(state.timestamp);
                let y = weight_to_pixel(state.weight_lbs);

                if first {
                    path.push_str(&format!("M{},{}", x, y));
                    first = false;
                } else {
                    path.push_str(&format!(" L{},{}", x, y));
                }
            }
        }
        if !path.is_empty() {
            svg.push_str(&format!(
                r#"<path d="{}" stroke="blue" stroke-width="2.5" fill="none"/>"#,
                path
            ));
        }
    }

    // Legend for top panel
    svg.push_str(&format!(
        r##"<circle cx="{}" cy="{}" r="3" fill="#999999" opacity="0.6"/>"##,
        width - margin_right - 200, margin_top + 15
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" font-size="11" fill="black">Actual Scale Weight</text>"#,
        width - margin_right - 192, margin_top + 19
    ));
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="blue" stroke-width="2.5"/>"#,
        width - margin_right - 200, margin_top + 30, width - margin_right - 180, margin_top + 30
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" font-size="11" fill="black">Kalman Trend (Denoised)</text>"#,
        width - margin_right - 172, margin_top + 34
    ));

    // ===== BOTTOM PANEL: Velocity Chart =====

    // Zero baseline (thick black)
    let zero_y = velocity_to_pixel(0.0);
    svg.push_str(&format!(
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>"#,
        margin_left, zero_y, width - margin_right, zero_y
    ));

    // Area chart (velocity curve with red fill, past 90 days only, clipped at +/-2)
    if !data.kalman_states.is_empty() {
        let mut area_path = String::new();
        let mut line_path = String::new();
        let mut area_first = true;
        let mut line_first = true;

        for state in data.kalman_states.iter() {
            if state.timestamp >= x_min {
                let x = x_to_pixel(state.timestamp);
                let velocity_lbs_week = (state.velocity_lbs_per_day * 7.0).clamp(vel_min, vel_max);
                let y = velocity_to_pixel(velocity_lbs_week);

                if area_first {
                    area_path.push_str(&format!("M{},{}", x, zero_y));
                    area_first = false;
                }
                area_path.push_str(&format!(" L{},{}", x, y));

                if line_first {
                    line_path.push_str(&format!("M{},{}", x, y));
                    line_first = false;
                } else {
                    line_path.push_str(&format!(" L{},{}", x, y));
                }
            }
        }

        if !area_path.is_empty() {
            area_path.push_str(&format!(" L{},{} Z", width - margin_right, zero_y));
            svg.push_str(&format!(
                r#"<path d="{}" fill="red" opacity="0.3"/>"#,
                area_path
            ));
        }

        if !line_path.is_empty() {
            svg.push_str(&format!(
                r#"<path d="{}" stroke="red" stroke-width="2" fill="none"/>"#,
                line_path
            ));
        }
    }

    svg.push_str("</svg>");
    svg
}
