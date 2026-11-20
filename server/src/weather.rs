use crate::svg_common;
use base64::{engine::general_purpose, Engine as _};
use chrono::prelude::*;
use chrono::Timelike;
use reverse_geocoder::ReverseGeocoder;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Debug)]
pub struct WeatherData {
    pub lat: f32,
    pub lon: f32,
    pub timezone_offset: i32,
    pub current: CurrentWeather,
    pub daily: Vec<DailyWeather>,
    #[serde(default)]
    pub hourly: Vec<HourlyWeather>,
    #[serde(default)]
    pub alerts: Vec<WeatherAlert>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WeatherOverviewData {
    pub lat: f32,
    pub lon: f32,
    pub tz: String,
    pub date: String,
    pub units: String,
    pub weather_overview: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WeatherAlert {
    pub event: String,
    pub description: String,
    pub start: i64,
    pub end: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CurrentWeather {
    pub dt: i64,
    pub temp: f32,
    pub humidity: i32,
    pub weather: Vec<Weather>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DailyWeather {
    pub dt: i64,
    pub temp: TempRange,
    pub feels_like: FeelsLike,
    pub humidity: i32,
    pub wind_speed: f32,
    pub wind_gust: Option<f32>,
    pub sunrise: i64,
    pub sunset: i64,
    pub weather: Vec<Weather>,
    #[serde(default)]
    pub pop: f32,
    #[serde(default)]
    pub rain: f32,
    pub uvi: Option<f32>,
    pub clouds: Option<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HourlyWeather {
    pub dt: i64,
    pub feels_like: f32,
    pub wind_speed: f32,
    pub wind_gust: Option<f32>,
    #[serde(default)]
    pub uvi: f32,
    #[serde(default)]
    pub pop: f32,
    pub rain: Option<HourlyPrecip>,
}

/// Unlike the daily `rain` (a bare number), hourly rain is `{"1h": mm}` and is
/// omitted entirely when no rain is forecast.
#[derive(Serialize, Deserialize, Debug)]
pub struct HourlyPrecip {
    #[serde(rename = "1h", default)]
    pub one_hour: f32,
}

impl HourlyWeather {
    fn rain_mm(&self) -> f32 {
        self.rain.as_ref().map_or(0.0, |r| r.one_hour)
    }

    fn gust(&self) -> f32 {
        self.wind_gust.unwrap_or(self.wind_speed)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TempRange {
    pub day: f32,
    pub min: f32,
    pub max: f32,
    pub morn: f32,
    pub night: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FeelsLike {
    pub day: f32,
    pub night: f32,
    pub eve: f32,
    pub morn: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Weather {
    pub description: String,
    pub main: String,
    pub icon: String,
}

/// Maps OpenWeatherMap icon codes to local SVG icon filenames
fn map_weather_icon(icon_code: &str) -> &'static str {
    match icon_code {
        "01d" => "clear-day.svg",
        "01n" => "clear-night.svg",
        "02d" => "partly-cloudy-day.svg",
        "02n" => "partly-cloudy-night.svg",
        "03d" | "03n" => "cloudy.svg",
        "04d" | "04n" => "overcast-day.svg",
        "09d" | "09n" => "rain.svg",
        "10d" => "overcast-day-rain.svg",
        "10n" => "overcast-night-rain.svg",
        "11d" => "thunderstorms-day.svg",
        "11n" => "thunderstorms-night.svg",
        "13d" => "snow.svg",
        "13n" => "snow.svg",
        "50d" | "50n" => "fog.svg",
        _ => "cloudy.svg", // default fallback
    }
}

const RAIN_ICON_POP_CUTOFF: f32 = 0.25; // 25% chance of rain
const MM_PER_INCH: f32 = 25.4;
const RAIN_ICON_AMOUNT_CUTOFF: f32 = 0.1 * MM_PER_INCH;

/// Whether the forecast rain is likely and heavy enough to be worth showing.
fn is_significant_rain(pop: f32, rain: f32) -> bool {
    pop >= RAIN_ICON_POP_CUTOFF && rain >= RAIN_ICON_AMOUNT_CUTOFF
}

/// Rain amount at which the forecast rain bar is full.
const RAIN_BAR_MAX_INCHES: f32 = 1.0;

/// Rain bar fill percentage, shown only when the rain icon is.
/// OpenWeatherMap reports `rain` in mm even in imperial units.
fn rain_fill(pop: f32, rain: f32) -> Option<f32> {
    is_significant_rain(pop, rain)
        .then(|| (rain / MM_PER_INCH / RAIN_BAR_MAX_INCHES * 100.0).clamp(0.0, 100.0))
}

fn display_icon_for_daily_weather(icon_code: &str, pop: f32, rain: f32) -> &str {
    if !is_significant_rain(pop, rain) {
        match icon_code {
            "09d" => "03d",
            "09n" => "03n",
            "10d" => "04d",
            "10n" => "04n",
            _ => icon_code,
        }
    } else {
        icon_code
    }
}

/// Loads an SVG icon and returns its content as a base64-encoded data URI
fn load_weather_icon_as_data_uri(icon_code: &str) -> Result<String, std::io::Error> {
    let icon_filename = map_weather_icon(icon_code);
    let icon_path = format!("assets/static/fill-svg-static/{}", icon_filename);
    let svg_content = fs::read(&icon_path)?;

    // Encode as base64 data URI for embedding in SVG
    let encoded = general_purpose::STANDARD.encode(&svg_content);
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}

/// Distance from the bar's outer edge to its fill, so the fill never covers
/// the border.
const BAR_INSET: f32 = 2.0;

/// A rounded meter: white background with a black border, overlaid with a
/// gradient that is clipped to the filled portion.
///
/// This markup — background rect, `clipPath`, gradient rect — was previously
/// written out longhand at seven call sites in this file.
struct GradientBar<'a> {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gradient_id: &'a str,
    /// Must be unique within the containing SVG document.
    clip_id: &'a str,
    inner_radius: u32,
}

impl GradientBar<'_> {
    /// `fill_offset` and `fill_width` are pixels measured from the inside edge
    /// of the border, which lets the forecast rows fill a min..max span rather
    /// than always starting at the left.
    fn render(&self, fill_offset: f32, fill_width: f32) -> String {
        let inner_x = self.x + BAR_INSET;
        let inner_y = self.y + BAR_INSET;
        let inner_h = self.height - BAR_INSET * 2.0;

        format!(
            concat!(
                r#"  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
                "\n",
                r#"  <clipPath id="{clip}">"#,
                "\n",
                r#"    <rect x="{fill_x}" y="{inner_y}" width="{fill_w}" height="{inner_h}" rx="{radius}"/>"#,
                "\n",
                r#"  </clipPath>"#,
                "\n",
                r#"  <rect x="{inner_x}" y="{inner_y}" width="{inner_w}" height="{inner_h}" fill="url(#{gradient})" clip-path="url(#{clip})" rx="{radius}"/>"#,
            ),
            x = self.x,
            y = self.y,
            w = self.width,
            h = self.height,
            clip = self.clip_id,
            fill_x = inner_x + fill_offset,
            fill_w = fill_width,
            inner_x = inner_x,
            inner_y = inner_y,
            inner_w = self.width - BAR_INSET * 2.0,
            inner_h = inner_h,
            radius = self.inner_radius,
            gradient = self.gradient_id,
        )
    }

    /// Fills from the left edge out to `pct` of the bar's inner width.
    fn render_pct(&self, pct: f32) -> String {
        let inner_w = self.width - BAR_INSET * 2.0;
        self.render(0.0, inner_w * (pct / 100.0))
    }
}

/// Each of these maps a reading onto a 0-100% bar fill.
fn temperature_fill(temp: f32) -> f32 {
    // 32°F to 100°F spans the bar.
    ((temp - 32.0) / 68.0 * 100.0).clamp(0.0, 100.0)
}

/// Weather alerts listed under the today panel; more would reach the footer.
const MAX_ALERTS: usize = 3;

/// Hours of hourly forecast covered by the today panel's sparklines.
const SPARK_HOURS: usize = 24;

/// Hourly rain rate at which the rain column is full, unless the peak is higher.
const SPARK_RAIN_MAX_MM: f32 = 0.1 * MM_PER_INCH;

/// Maps hourly samples onto one sparkline row. Every row is built over the
/// same `start..end` span so the rows line up; each has its own value range.
struct SparkFrame {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    start: i64,
    end: i64,
    min: f32,
    max: f32,
}

impl SparkFrame {
    fn tx(&self, t: i64) -> f32 {
        let span = (self.end - self.start).max(1) as f32;
        self.x + self.w * ((t - self.start) as f32 / span).clamp(0.0, 1.0)
    }

    fn vy(&self, v: f32) -> f32 {
        self.bottom() - self.h * self.frac(v)
    }

    fn frac(&self, v: f32) -> f32 {
        let range = (self.max - self.min).max(f32::EPSILON);
        ((v - self.min) / range).clamp(0.0, 1.0)
    }

    fn bottom(&self) -> f32 {
        self.y + self.h
    }

    fn line_path(&self, points: &[(i64, f32)]) -> String {
        let mut path = String::new();
        for (i, &(t, v)) in points.iter().enumerate() {
            let cmd = if i == 0 { "M" } else { " L" };
            path.push_str(&format!("{} {:.1} {:.1}", cmd, self.tx(t), self.vy(v)));
        }
        path
    }

    fn area_path(&self, points: &[(i64, f32)]) -> String {
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return String::new();
        };
        format!(
            "M {:.1} {:.1} {} L {:.1} {:.1} Z",
            self.tx(first.0),
            self.bottom(),
            self.line_path(points).replacen('M', "L", 1),
            self.tx(last.0),
            self.bottom()
        )
    }

    /// A vertical gradient that blends smoothly between `stops`, ascending
    /// `(value, colour)` pairs, so a line or area is coloured by its value
    /// exactly as the matching meter bar would be. The gradient is anchored at
    /// the stop values themselves, which may lie outside this frame's range,
    /// so an autoscaled row still gives a value the same colour as the bar did.
    fn smooth_gradient(&self, id: &str, stops: &[(f32, &str)]) -> String {
        let (Some(&(lo, _)), Some(&(hi, _))) = (stops.first(), stops.last()) else {
            return String::new();
        };
        let range = (self.max - self.min).max(f32::EPSILON);
        let y_at = |v: f32| self.bottom() - self.h * (v - self.min) / range;
        let span = (hi - lo).max(f32::EPSILON);
        let stops: String = stops
            .iter()
            .map(|&(v, color)| {
                format!(
                    r#"<stop offset="{:.1}%" style="stop-color:{};stop-opacity:1"/>"#,
                    (v - lo) / span * 100.0,
                    color
                )
            })
            .collect();
        format!(
            r#"  <defs><linearGradient id="{}" gradientUnits="userSpaceOnUse" x1="0" y1="{:.1}" x2="0" y2="{:.1}">{}</linearGradient></defs>"#,
            id,
            y_at(lo),
            y_at(hi),
            stops
        )
    }

    fn baseline(&self) -> String {
        format!(
            r#"  <line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="1"/>"#,
            self.x,
            self.bottom(),
            self.x + self.w,
            self.bottom()
        )
    }
}

/// The parts of `start..end` outside every `(sunrise, sunset)` interval.
fn night_spans(start: i64, end: i64, days: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut days = days.to_vec();
    days.sort_unstable();
    let mut spans = Vec::new();
    let mut cursor = start;
    for (rise, set) in days {
        if rise > cursor {
            let night_end = rise.min(end);
            if night_end > cursor {
                spans.push((cursor, night_end));
            }
        }
        cursor = cursor.max(set);
    }
    if cursor < end {
        spans.push((cursor, end));
    }
    spans
}

/// Value range for the feels-like row: the data's range, widened to at least
/// 20°F so an ordinary day doesn't look dramatic, plus a little headroom.
fn temperature_spark_range(lo: f32, hi: f32) -> (f32, f32) {
    let mid = (lo + hi) / 2.0;
    let half = ((hi - lo) / 2.0).max(10.0) + 2.0;
    (mid - half, mid + half)
}

/// "6a", "12p", ... for the sparkline time axis.
fn short_hour(hour: u32) -> String {
    let (h, suffix) = match hour {
        0 => (12, "a"),
        1..=11 => (hour, "a"),
        12 => (12, "p"),
        _ => (hour - 12, "p"),
    };
    format!("{}{}", h, suffix)
}

/// The next 24 hours as four stacked sparklines (feels-like, rain, wind, UV)
/// on a shared time axis, with night shaded. Occupies the left panel between
/// `top` (the time-axis labels) and `bottom`; the rows share the height in
/// fixed proportions.
fn render_hourly_sparklines(
    svg: &mut String,
    hourly: &[HourlyWeather],
    daily: &[DailyWeather],
    tz: &chrono::FixedOffset,
    top: f32,
    bottom: f32,
) {
    let hours = &hourly[..hourly.len().min(SPARK_HOURS + 1)];
    let (Some(first), Some(last)) = (hours.first(), hours.last()) else {
        svg.push_str(&format!(
            r#"  <text x="40" y="{}" font-family="Arial" font-size="18" fill="black">Hourly forecast unavailable</text>"#,
            top + 40.0
        ));
        svg.push('\n');
        return;
    };
    if hours.len() < 2 {
        return;
    }
    let (start, end) = (first.dt, last.dt);

    let label_x = 20.0;
    let chart_x = 110.0;
    let chart_w = 260.0;
    let value_x = 465.0;
    let frame = |y: f32, h: f32, min: f32, max: f32| SparkFrame {
        x: chart_x,
        y,
        w: chart_w,
        h,
        start,
        end,
        min,
        max,
    };

    // Rows: (top, height). Temperature gets the most room because it is the
    // one most people look at; rain is next because it has two series.
    let axis_y = top;
    let gap = 14.0;
    let weights = [80.0, 60.0, 50.0, 45.0];
    let scale = (bottom - top - 10.0 - gap * 3.0) / weights.iter().sum::<f32>();
    let temp_row = (top + 10.0, weights[0] * scale);
    let rain_row = (temp_row.0 + temp_row.1 + gap, weights[1] * scale);
    let wind_row = (rain_row.0 + rain_row.1 + gap, weights[2] * scale);
    let uv_row = (wind_row.0 + wind_row.1 + gap, weights[3] * scale);
    let rows_top = temp_row.0;
    let rows_bottom = uv_row.0 + uv_row.1;

    // Night shading: a sparse dot pattern, which stays on-palette when dithered.
    let axis = frame(rows_top, rows_bottom - rows_top, 0.0, 1.0);
    svg.push_str(r#"  <defs><pattern id="nightDots" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="1" height="1" fill="black"/></pattern></defs>"#);
    svg.push('\n');
    let days: Vec<(i64, i64)> = daily.iter().map(|d| (d.sunrise, d.sunset)).collect();
    for (a, b) in night_spans(start, end, &days) {
        svg.push_str(&format!(
            r#"  <rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#nightDots)"/>"#,
            axis.tx(a),
            rows_top,
            axis.tx(b) - axis.tx(a),
            rows_bottom - rows_top
        ));
        svg.push('\n');
    }

    // Time axis: "Now" at the left, then every 6 hours (which lines up with
    // the device's wake schedule).
    svg.push_str(&format!(
        r#"  <text x="{}" y="{}" font-family="Arial" font-size="13" fill="black">Now</text>"#,
        chart_x, axis_y
    ));
    svg.push('\n');
    for hour in hours.iter().skip(1) {
        let local = Utc
            .timestamp_opt(hour.dt, 0)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(tz);
        if local.hour() % 6 != 0 {
            continue;
        }
        let x = axis.tx(hour.dt);
        // Too close to the left edge its label would run into "Now", and an
        // unlabelled gridline reads as a "Now" marker, so skip both.
        if x - chart_x <= 45.0 {
            continue;
        }
        svg.push_str(&format!(
            r#"  <line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="black" stroke-width="1" stroke-dasharray="2,3"/>"#,
            x, rows_top, x, rows_bottom
        ));
        svg.push('\n');
        svg.push_str(&format!(
            r#"  <text x="{:.1}" y="{}" text-anchor="middle" font-family="Arial" font-size="13" fill="black">{}</text>"#,
            x,
            axis_y,
            short_hour(local.hour())
        ));
        svg.push('\n');
    }

    let row_text = |svg: &mut String, (y, h): (f32, f32), label: &str, value: &str| {
        svg.push_str(&format!(
            r#"  <text x="{}" y="{:.1}" font-family="Arial" font-size="18" fill="black">{}</text>"#,
            label_x,
            y + h / 2.0 + 6.0,
            label
        ));
        svg.push('\n');
        // A value may span several lines (split on '\n'), centred on the row,
        // so that long values stay clear of the chart.
        let lines: Vec<&str> = value.lines().collect();
        let first_y = y + h / 2.0 + 6.0 - 9.0 * lines.len().saturating_sub(1) as f32;
        for (i, line) in lines.iter().enumerate() {
            svg.push_str(&format!(
                r#"  <text x="{}" y="{:.1}" text-anchor="end" font-family="Arial" font-size="16" font-weight="bold" fill="black">{}</text>"#,
                value_x,
                first_y + 18.0 * i as f32,
                svg_common::escape_xml(line)
            ));
            svg.push('\n');
        }
    };

    // Feels-like temperature: autoscaled (see temperature_spark_range), with
    // the line coloured on an absolute scale so the colour still reads as
    // cold / mild / hot.
    let temps: Vec<(i64, f32)> = hours.iter().map(|h| (h.dt, h.feels_like)).collect();
    let lo = temps.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
    let hi = temps.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    let (min, max) = temperature_spark_range(lo, hi);
    let f = frame(temp_row.0, temp_row.1, min, max);
    svg.push_str(&f.smooth_gradient("sparkTemp", &[(32.0, "blue"), (100.0, "red")]));
    svg.push('\n');
    svg.push_str(&format!(
        r#"  <path d="{}" fill="none" stroke="url(#sparkTemp)" stroke-width="4" stroke-linejoin="round" stroke-linecap="round"/>"#,
        f.line_path(&temps)
    ));
    svg.push('\n');
    row_text(
        svg,
        temp_row,
        "Feels like",
        &format!("{:.0}°/{:.0}°", hi, lo),
    );

    // Rain: one column per hour for the amount, and a thin line for the
    // chance of rain (0-100% of the row height).
    let peak_mm = hours.iter().map(|h| h.rain_mm()).fold(0.0, f32::max);
    let f = frame(rain_row.0, rain_row.1, 0.0, SPARK_RAIN_MAX_MM.max(peak_mm));
    for pair in hours.windows(2) {
        let mm = pair[0].rain_mm();
        if mm <= 0.0 {
            continue;
        }
        let x0 = f.tx(pair[0].dt) + 1.0;
        let x1 = f.tx(pair[1].dt) - 1.0;
        let height = (f.bottom() - f.vy(mm)).max(2.0);
        svg.push_str(&format!(
            r#"  <rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="blue"/>"#,
            x0,
            f.bottom() - height,
            (x1 - x0).max(1.0),
            height
        ));
        svg.push('\n');
    }
    let pops: Vec<(i64, f32)> = hours.iter().map(|h| (h.dt, h.pop)).collect();
    let pop_frame = frame(rain_row.0, rain_row.1, 0.0, 1.0);
    svg.push_str(&format!(
        r#"  <path d="{}" fill="none" stroke="black" stroke-width="1.5" stroke-linejoin="round"/>"#,
        pop_frame.line_path(&pops)
    ));
    svg.push('\n');
    let total_in = hours[..hours.len() - 1]
        .iter()
        .map(|h| h.rain_mm())
        .sum::<f32>()
        / MM_PER_INCH;
    let max_pop = hours.iter().map(|h| h.pop).fold(0.0, f32::max);
    let rain_text = if total_in < 0.005 && max_pop < 0.1 {
        "Dry".to_string()
    } else {
        format!("{:.2}\"\n{:.0}%", total_in, max_pop * 100.0)
    };
    row_text(svg, rain_row, "Rain", &rain_text);

    // Wind gusts (falling back to sustained speed), 0-30 mph unless gustier.
    let gusts: Vec<(i64, f32)> = hours.iter().map(|h| (h.dt, h.gust())).collect();
    let peak_gust = gusts.iter().map(|p| p.1).fold(0.0, f32::max);
    let f = frame(wind_row.0, wind_row.1, 0.0, peak_gust.max(30.0));
    svg.push_str(&f.smooth_gradient(
        "sparkWind",
        &[(0.0, "green"), (30.0, "orange"), (60.0, "red")],
    ));
    svg.push('\n');
    svg.push_str(&format!(
        r#"  <path d="{}" fill="none" stroke="url(#sparkWind)" stroke-width="4" stroke-linejoin="round" stroke-linecap="round"/>"#,
        f.line_path(&gusts)
    ));
    svg.push('\n');
    row_text(svg, wind_row, "Wind", &format!("{:.0} mph", peak_gust));

    // UV index as a filled area, which carries colour better than a thin line.
    // 0-8 unless higher, so a winter peak of 2-3 is still visible.
    let uvs: Vec<(i64, f32)> = hours.iter().map(|h| (h.dt, h.uvi)).collect();
    let peak_uv = uvs.iter().map(|p| p.1).fold(0.0, f32::max);
    let f = frame(uv_row.0, uv_row.1, 0.0, peak_uv.max(8.0));
    svg.push_str(&f.smooth_gradient(
        "sparkUv",
        &[
            (0.0, "green"),
            (2.97, "yellow"),
            (5.94, "orange"),
            (7.92, "red"),
            (11.0, "purple"),
        ],
    ));
    svg.push('\n');
    svg.push_str(&format!(
        r#"  <path d="{}" fill="url(#sparkUv)"/>"#,
        f.area_path(&uvs)
    ));
    svg.push('\n');
    row_text(svg, uv_row, "UV", &format!("{:.0}", peak_uv));

    for (y, h) in [temp_row, rain_row, wind_row, uv_row] {
        svg.push_str(&frame(y, h, 0.0, 1.0).baseline());
        svg.push('\n');
    }
}

/// Converts wind speed in mph to Beaufort scale (0-12)
fn wind_speed_to_beaufort(wind_speed: f32) -> u8 {
    match wind_speed as i32 {
        0 => 0,
        1..=3 => 1,
        4..=7 => 2,
        8..=12 => 3,
        13..=18 => 4,
        19..=24 => 5,
        25..=31 => 6,
        32..=38 => 7,
        39..=46 => 8,
        47..=54 => 9,
        55..=63 => 10,
        64..=72 => 11,
        _ => 12,
    }
}

/// Loads a Beaufort icon and returns its content as a base64-encoded data URI
fn load_beaufort_icon_as_data_uri(beaufort: u8) -> Result<String, std::io::Error> {
    let icon_filename = format!("wind-beaufort-{}.svg", beaufort);
    let icon_path = format!("assets/static/fill-svg-static/{}", icon_filename);
    let svg_content = fs::read(&icon_path)?;
    let encoded = general_purpose::STANDARD.encode(&svg_content);
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}

/// Fetches weather data from OpenWeatherMap One Call API (3.0)
///
/// # Arguments
/// * `lat` - Latitude coordinate
/// * `lon` - Longitude coordinate
/// * `key` - OpenWeatherMap API key
///
/// # Returns
/// Result containing WeatherData on success, or error message on failure
pub async fn fetch_weather(
    lat: &str,
    lon: &str,
    key: &str,
) -> Result<WeatherData, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.openweathermap.org/data/3.0/onecall?lat={}&lon={}&units=imperial&exclude=minutely&appid={}",
        lat, lon, key
    );

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;
    let weather_data = response.json::<WeatherData>().await?;

    Ok(weather_data)
}

pub async fn fetch_weather_overview(
    lat: &str,
    lon: &str,
    key: &str,
) -> Result<WeatherOverviewData, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.openweathermap.org/data/3.0/onecall/overview?lat={}&lon={}&units=imperial&appid={}",
        lat, lon, key
    );

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;
    let weather_overview_data = response.json::<WeatherOverviewData>().await?;

    Ok(weather_overview_data)
}

fn wrap_text_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if candidate_len > max_chars && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

pub fn generate_weather_overview_svg(
    weather: &WeatherOverviewData,
    battery_pct: Option<u8>,
    geocoder: &ReverseGeocoder,
) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480">"#,
    );
    svg.push('\n');

    svg.push_str(r#"  <defs>"#);
    svg.push('\n');
    svg.push_str(svg_common::BATTERY_GRADIENT_DEF);
    svg.push('\n');
    svg.push_str(r#"  </defs>"#);
    svg.push('\n');

    svg.push_str(r#"  <rect width="800" height="480" fill="white"/>"#);
    svg.push('\n');

    let coords = (weather.lat as f64, weather.lon as f64);
    let search_result = geocoder.search(coords);
    svg.push_str(r#"  <text x="20" y="38" font-family="Arial" font-size="30" font-weight="bold" fill="black">Weather Overview</text>"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"  <text x="20" y="62" font-family="Arial" font-size="16" fill="black">{}, {} ({})</text>"#,
        svg_common::escape_xml(&search_result.record.name),
        svg_common::escape_xml(&weather.date),
        svg_common::escape_xml(&weather.tz)
    ));
    svg.push('\n');
    svg.push_str(r#"  <line x1="20" y1="76" x2="780" y2="76" stroke="black" stroke-width="1"/>"#);
    svg.push('\n');

    let lines = wrap_text_lines(&weather.weather_overview, 60);
    let line_height = 26;
    let mut y = 108;
    for line in lines.iter().take(14) {
        svg.push_str(&format!(
            r#"  <text x="30" y="{}" font-family="Arial" font-size="24" fill="black">{}</text>"#,
            y,
            svg_common::escape_xml(line)
        ));
        svg.push('\n');
        y += line_height;
    }

    let footer_y = 470;
    let pct = battery_pct.unwrap_or(50);
    let battery_x = 75.0;
    let battery_y = footer_y as f64 - 10.0;

    svg.push_str(&svg_common::battery_label_svg(
        10.0,
        footer_y as f64,
        "start",
        12,
    ));
    svg.push('\n');
    svg.push_str(&svg_common::battery_bar_svg(
        battery_x,
        battery_y,
        pct,
        2.0,
        "batteryClip",
    ));
    svg.push('\n');

    svg.push_str("  ");
    svg.push_str(&svg_common::last_updated_text(
        790.0,
        footer_y as f64,
        "end",
    ));
    svg.push('\n');

    svg.push_str("</svg>");
    svg
}

/// Generates an SVG weather display from weather data
///
/// # Arguments
/// * `weather` - The weather data to display
/// * `battery_pct` - Optional battery percentage to display
///
/// # Returns
/// A String containing the SVG markup
/// "(11/18 6pm - 11/19 6am)": an alert's span in local time, with the start
/// rounded down and the end rounded up to the hour.
fn alert_time_range(alert: &WeatherAlert, tz: &chrono::FixedOffset) -> Option<String> {
    let start_time = chrono::DateTime::from_timestamp(alert.start, 0)?.with_timezone(tz);
    let mut end_time = chrono::DateTime::from_timestamp(alert.end, 0)?.with_timezone(tz);
    if end_time.minute() > 0 || end_time.second() > 0 {
        end_time += chrono::Duration::hours(1);
    }
    let twelve_hour = |hour: u32| match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    let (start_12h, start_ampm) = twelve_hour(start_time.hour());
    let (end_12h, end_ampm) = twelve_hour(end_time.hour());
    Some(format!(
        "({}/{} {}{} - {}/{} {}{})",
        start_time.month(),
        start_time.day(),
        start_12h,
        start_ampm,
        end_time.month(),
        end_time.day(),
        end_12h,
        end_ampm
    ))
}

pub fn generate_weather_svg(
    weather: &WeatherData,
    battery_pct: Option<u8>,
    geocoder: &ReverseGeocoder,
) -> String {
    if weather.daily.is_empty() {
        return r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
    <text x="400" y="240" text-anchor="middle" font-size="20">Error: weather.daily is empty</text>
</svg>"#
            .to_string();
    }

    // Create timezone offset from the weather data
    let tz_offset =
        chrono::FixedOffset::east_opt(weather.timezone_offset).unwrap_or_else(|| chrono::Utc.fix());

    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480">"#,
    );
    svg.push('\n');

    // Define gradients for bars
    svg.push_str(r#"  <defs>"#);
    svg.push('\n');

    // Temperature gradient: blue (cold) -> red (hot)
    svg.push_str(r#"    <linearGradient id="tempGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // Battery gradient: red (low) -> green (full)
    svg.push_str(svg_common::BATTERY_GRADIENT_DEF);
    svg.push('\n');

    // Rain gradient: solid blue, so the forecast rain bar can reuse GradientBar
    svg.push_str(r#"    <linearGradient id="rainGradient">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    svg.push_str(r#"  </defs>"#);
    svg.push('\n');

    // Background
    svg.push_str(r#"  <rect width="800" height="480" fill="white"/>"#);
    svg.push('\n');

    // Left section: Today's detailed forecast (takes ~60% of width)
    let left_width = 480.0;
    let today = &weather.daily[0];

    // Date header
    let today_time = Utc
        .timestamp_opt(today.dt, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .with_timezone(&tz_offset);
    svg.push_str(&format!(
        r#"  <text x="20" y="35" font-family="Arial" font-size="28" font-weight="bold" fill="black">{}</text>"#,
        today_time.format("%A, %b %e")
    ));
    svg.push('\n');

    // City name from coordinates
    let coords = (weather.lat as f64, weather.lon as f64);
    let search_result = geocoder.search(coords);
    svg.push_str(&format!(
        r#"  <text x="20" y="58" font-family="Arial" font-size="16" fill="black">{}</text>"#,
        svg_common::escape_xml(&search_result.record.name)
    ));
    svg.push('\n');

    // Weather icon (large, centered in left section)
    if let Some(w) = today.weather.first() {
        // Embed weather icon as a data URI
        let icon_code = display_icon_for_daily_weather(&w.icon, today.pop, today.rain);
        if let Ok(data_uri) = load_weather_icon_as_data_uri(icon_code) {
            svg.push_str(&format!(
                r#"  <image x="350" y="2" width="80" height="80" href="{}"/>"#,
                data_uri
            ));
            svg.push('\n');
        }
    }

    // Next 24 hours as sparklines, replacing the old Morning/Day/Evening
    // temperatures and the humidity/wind/cloudiness/UV meters.
    // The charts grow into whatever room the alerts leave free; alerts past
    // MAX_ALERTS would run into the footer, so they are not shown.
    let shown_alerts = weather.alerts.len().min(MAX_ALERTS);
    let charts_bottom = 400.0 - 18.0 * shown_alerts.saturating_sub(1) as f32;
    render_hourly_sparklines(
        &mut svg,
        &weather.hourly,
        &weather.daily,
        &tz_offset,
        100.0,
        charts_bottom,
    );

    // Sunrise/sunset times (calculated once, used conditionally below)
    let sunrise_time = Utc
        .timestamp_opt(today.sunrise, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .with_timezone(&tz_offset);
    let sunset_time = Utc
        .timestamp_opt(today.sunset, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .with_timezone(&tz_offset);

    // Sunrise text (moved down)
    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Sunrise: {}</text>"#,
        charts_bottom + 25.0, sunrise_time.format("%l:%M %P")
    ));
    svg.push('\n');

    // Sunset text (moved down)
    svg.push_str(&format!(
        r#"  <text x="280" y="{}" font-family="Arial" font-size="20" fill="black">Sunset: {}</text>"#,
        charts_bottom + 25.0, sunset_time.format("%l:%M %P")
    ));
    svg.push('\n');

    // Weather warning titles (if any)
    if !weather.alerts.is_empty() {
        let mut alert_y = charts_bottom + 50.0;
        for alert in weather.alerts.iter().take(MAX_ALERTS) {
            let Some(time_range) = alert_time_range(alert, &tz_offset) else {
                continue;
            };

            // Event name in bold red
            svg.push_str(&format!(
                r#"  <text x="40" y="{}" font-family="Arial" font-size="16" font-weight="bold" fill="red">{}</text>"#,
                alert_y, svg_common::escape_xml(&alert.event)
            ));
            svg.push('\n');

            // Time range in regular black
            svg.push_str(&format!(
                r#"  <text x="260" y="{}" font-family="Arial" font-size="16" fill="black">{}</text>"#,
                alert_y, time_range
            ));
            svg.push('\n');
            alert_y += 18.0;
        }
    }

    // Vertical divider
    svg.push_str(&format!(
        r#"  <line x1="{}" y1="20" x2="{}" y2="460" stroke="black" stroke-width="2"/>"#,
        left_width, left_width
    ));
    svg.push('\n');

    // Right section: 5-day forecast stacked vertically
    svg.push_str(&format!(
        r#"  <text x="{}" y="35" font-family="Arial" font-size="24" font-weight="bold" fill="black">5-Day Forecast</text>"#,
        left_width + 20.0
    ));
    svg.push('\n');

    let right_x = left_width + 20.0;
    let forecast_start_y = 70.0;
    let row_height = 80.0;

    for (idx, day) in weather.daily.iter().skip(1).take(5).enumerate() {
        let y = forecast_start_y + (idx as f32 * row_height);

        let day_time = Utc
            .timestamp_opt(day.dt, 0)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(&tz_offset);
        let day_name = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ][day_time.weekday() as usize];

        // Day name and date
        svg.push_str(&format!(
            r#"  <text x="{}" y="{}" font-family="Arial" font-size="22" font-weight="bold" fill="black">{}</text>"#,
            right_x, y + 12.0, day_name
        ));
        svg.push('\n');

        // Weather icon (small)
        if let Some(w) = day.weather.first() {
            // Embed small weather icon as a data URI
            let icon_code = display_icon_for_daily_weather(&w.icon, day.pop, day.rain);
            if let Ok(data_uri) = load_weather_icon_as_data_uri(icon_code) {
                svg.push_str(&format!(
                    r#"  <image x="{}" y="{}" width="80" height="80" href="{}"/>"#,
                    right_x + 150.0,
                    y - 20.0,
                    data_uri
                ));
                svg.push('\n');
            }
        }

        // Temperature bar indicator
        // Calculate positions for min and max temperatures on the bar
        let min_fill = temperature_fill(day.temp.min);
        let max_fill = temperature_fill(day.temp.max);
        let forecast_bar_width = 125.0; // 5 pixels longer
        let forecast_bar_height = 16.0;

        // The fill spans min..max rather than starting at the left edge.
        let available_width = forecast_bar_width - BAR_INSET * 2.0;
        let available_height = forecast_bar_height - BAR_INSET * 2.0;
        let mut fill_start_offset = available_width * (min_fill / 100.0);
        let mut fill_width = available_width * ((max_fill - min_fill) / 100.0);

        // Ensure fill_width is at least as wide as it is tall (minimum square)
        if fill_width < available_height {
            let difference = available_height - fill_width;
            // Adjust start position by half the difference to center it
            fill_start_offset = (fill_start_offset - difference / 2.0)
                .max(0.0)
                .min(available_width - available_height);
            fill_width = available_height;
        }

        svg.push_str(
            &GradientBar {
                x: right_x,
                y: y + 22.0,
                width: forecast_bar_width,
                height: forecast_bar_height,
                gradient_id: "tempGradient",
                clip_id: &format!("forecastTempClip{}", idx),
                inner_radius: 1,
            }
            .render(fill_start_offset, fill_width),
        );
        svg.push('\n');

        // Rain amount bar under the temperature bar, paired with the rain icon
        if let Some(pct) = rain_fill(day.pop, day.rain) {
            svg.push_str(
                &GradientBar {
                    x: right_x,
                    y: y + 44.0,
                    width: forecast_bar_width,
                    height: forecast_bar_height / 2.0,
                    gradient_id: "rainGradient",
                    clip_id: &format!("forecastRainClip{}", idx),
                    inner_radius: 1,
                }
                .render_pct(pct),
            );
            svg.push('\n');
        }

        // Wind indicator (Beaufort scale icon, color-coded by danger level)
        let beaufort = wind_speed_to_beaufort(day.wind_gust.unwrap_or(day.wind_speed));
        if let Ok(beaufort_icon_uri) = load_beaufort_icon_as_data_uri(beaufort) {
            svg.push_str(&format!(
                r#"  <image x="{}" y="{}" width="80" height="80" href="{}"/>"#,
                right_x + 220.0,
                y - 10.0,
                beaufort_icon_uri
            ));
            svg.push('\n');
        }
    }

    // Footer with battery and last updated
    let footer_y = 470;

    // Battery bar (if provided) - now on the left
    let pct = battery_pct.unwrap_or(50);
    let battery_x = 75.0;
    let battery_y = footer_y as f64 - 10.0;

    // Label
    svg.push_str(&svg_common::battery_label_svg(
        10.0,
        footer_y as f64,
        "start",
        12,
    ));
    svg.push('\n');

    svg.push_str(&svg_common::battery_bar_svg(
        battery_x,
        battery_y,
        pct,
        2.0,
        "batteryClip",
    ));
    svg.push('\n');

    // Last updated timestamp - now on the right
    svg.push_str("  ");
    svg.push_str(&svg_common::last_updated_text(
        790.0,
        footer_y as f64,
        "end",
    ));
    svg.push('\n');

    svg.push_str("</svg>");
    svg
}

/// OpenWeatherMap icon-code prefixes that have a hand-drawn tri-colour icon in
/// `assets/static/tricolor/{20,70}px/`.
const TRICOLOR_ICON_CODES: [&str; 9] = ["01", "02", "03", "04", "09", "10", "11", "13", "50"];

/// Loads a white/black/red PNG icon as a data URI. The full-colour icons turn to
/// mush when snapped to three colours at this size, so the tri-colour display
/// has its own set. Unknown codes fall back to the cloud icon.
fn load_tricolor_icon_as_data_uri(icon_code: &str, size: u32) -> Result<String, std::io::Error> {
    let prefix = icon_code.get(..2).unwrap_or("");
    let code = if TRICOLOR_ICON_CODES.contains(&prefix) {
        prefix
    } else {
        "03"
    };
    let png = fs::read(format!("assets/static/tricolor/{}px/{}.png", size, code))?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(&png)
    ))
}

/// Peak gust (mph) the tri-colour display flags in red.
const MINI_GUST_ALERT_MPH: f32 = 30.0;

/// Peak UV index the tri-colour display flags in red: the "high" band, where
/// sun protection is recommended.
const MINI_UV_ALERT: f32 = 6.0;

/// Temperatures the tri-colour display flags in red.
fn mini_temp_color(temp: f32) -> &'static str {
    if temp <= 32.0 || temp >= 90.0 {
        "red"
    } else {
        "black"
    }
}

/// An hour that gets a rain bar on the tri-colour display.
fn is_rainy_hour(hour: &HourlyWeather) -> bool {
    hour.pop >= RAIN_ICON_POP_CUTOFF && hour.rain_mm() > 0.0
}

/// From the start of the first rainy hour to the end of the last one, with the
/// total in inches; `None` when no hour is rainy.
fn rain_window(hours: &[HourlyWeather]) -> Option<(i64, i64, f32)> {
    let mut wet = hours.iter().filter(|h| is_rainy_hour(h));
    let first = wet.next()?;
    let (last, rest_mm) = wet.fold((first, 0.0), |(_, mm), h| (h, mm + h.rain_mm()));
    Some((
        first.dt,
        last.dt + 3600,
        (first.rain_mm() + rest_mm) / MM_PER_INCH,
    ))
}

/// `text` cut to `max` characters, with an ellipsis if anything was dropped.
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}

/// Sort key for an alert: warnings first, then watches, then advisories and
/// anything else (statements, outlooks).
fn alert_severity_rank(event: &str) -> u8 {
    let last = event.split_whitespace().last().unwrap_or("");
    match last {
        "Warning" | "Emergency" => 0,
        "Watch" => 1,
        "Advisory" => 2,
        _ => 3,
    }
}

/// "Wind Advisory" -> "Wind Adv", for squeezing several alerts onto one line.
fn abbreviate_alert(event: &str) -> String {
    match event.rsplit_once(' ') {
        Some((head, "Advisory")) => format!("{head} Adv"),
        Some((head, "Warning")) => format!("{head} Wrn"),
        Some((head, "Statement")) => format!("{head} Stmt"),
        _ => event.to_string(),
    }
}

/// One line naming the active alerts, most severe first, cut to `max_chars`.
/// With several, a leading count — "(3) Wind Adv, Cold Wrn, …" — survives any
/// cut, so nothing needs measuring. Duplicate events (the same alert issued by
/// overlapping offices) count once. Names are shortened only when the full
/// names don't fit.
fn alert_line(alerts: &[WeatherAlert], max_chars: usize) -> String {
    let mut events: Vec<&str> = Vec::new();
    for alert in alerts {
        let event = alert.event.trim();
        if !event.is_empty() && !events.contains(&event) {
            events.push(event);
        }
    }
    events.sort_by_key(|e| alert_severity_rank(e)); // stable: ties keep API order

    let prefix = match events.len() {
        0 | 1 => String::new(),
        n => format!("({n}) "),
    };
    let full = format!("{prefix}{}", events.join(", "));
    if full.chars().count() <= max_chars {
        return full;
    }
    let short: Vec<String> = events.iter().map(|e| abbreviate_alert(e)).collect();
    truncate_chars(&format!("{prefix}{}", short.join(", ")), max_chars)
}

/// Bold DejaVu Sans text for the tri-colour display. Below 12px the strokes
/// break up once snapped to three colours, so nothing here is smaller than 11.
fn mini_text(svg: &mut String, x: f32, y: f32, size: u32, anchor: &str, fill: &str, body: &str) {
    svg.push_str(&format!(
        r#"  <text x="{:.1}" y="{:.1}" text-anchor="{}" font-family="DejaVu Sans" font-weight="bold" font-size="{}" fill="{}">{}</text>"#,
        x,
        y,
        anchor,
        size,
        fill,
        svg_common::escape_xml(body)
    ));
    svg.push('\n');
}

/// The weather screen for the 296x128 white/black/red panel: today's icon and
/// hi/lo, the next 24 hours (feels-like line, rain bars), four more days, and a
/// footer that shows, in order of preference, an alert, the rain window, or
/// sunrise/sunset. Red only marks things worth attention.
pub fn generate_weather_mini_svg(
    weather: &WeatherData,
    battery_pct: Option<u8>,
    geocoder: &ReverseGeocoder,
) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="296" height="128" viewBox="0 0 296 128">"#,
    );
    svg.push('\n');
    svg.push_str(r#"  <rect width="296" height="128" fill="white"/>"#);
    svg.push('\n');

    let Some(today) = weather.daily.first() else {
        mini_text(
            &mut svg,
            148.0,
            68.0,
            12,
            "middle",
            "black",
            "No daily forecast",
        );
        svg.push_str("</svg>");
        return svg;
    };

    let tz = chrono::FixedOffset::east_opt(weather.timezone_offset).unwrap_or_else(|| Utc.fix());
    let local = |t: i64| {
        Utc.timestamp_opt(t, 0)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(&tz)
    };

    // Header: date and place, battery on the right.
    let place = geocoder.search((weather.lat as f64, weather.lon as f64));
    let header = format!(
        "{} · {}",
        local(today.dt).format("%a %b %-d"),
        place.record.name
    );
    mini_text(
        &mut svg,
        2.0,
        12.0,
        12,
        "start",
        "black",
        &truncate_chars(&header, 34),
    );
    if let Some(pct) = battery_pct {
        let fill = if pct < 20 { "red" } else { "black" };
        mini_text(&mut svg, 294.0, 12.0, 12, "end", fill, &format!("{}%", pct));
    }
    svg.push_str(
        r#"  <line x1="0" y1="16.5" x2="296" y2="16.5" stroke="black" stroke-width="1"/>"#,
    );
    svg.push('\n');

    // Today: small icon beside the high, then the low; peak gust and UV follow
    // once the hourly window is known.
    if let Some(w) = today.weather.first() {
        let code = display_icon_for_daily_weather(&w.icon, today.pop, today.rain);
        if let Ok(uri) = load_tricolor_icon_as_data_uri(code, 20) {
            svg.push_str(&format!(
                r#"  <image x="1" y="22" width="20" height="20" href="{}"/>"#,
                uri
            ));
            svg.push('\n');
        }
    }
    let (hi, lo) = (today.temp.max, today.temp.min);
    mini_text(
        &mut svg,
        23.0,
        40.0,
        20,
        "start",
        mini_temp_color(hi),
        &format!("{:.0}°", hi),
    );
    mini_text(
        &mut svg,
        23.0,
        61.0,
        15,
        "start",
        mini_temp_color(lo),
        &format!("{:.0}°", lo),
    );

    // Next 24 hours: a feels-like row over a rain row, on one time axis.
    // Every y below is on a half pixel: an unantialiased 1px line must sit on
    // a pixel centre, or it renders as two grey pixels that snap to white.
    const CHART_X: f32 = 66.0;
    const CHART_W: f32 = 132.0;
    const TEMP_TOP: f32 = 21.5;
    const TEMP_H: f32 = 44.0;
    const RAIN_TOP: f32 = 69.5;
    const RAIN_H: f32 = 18.0;
    /// Room above the feels-like line's peak and below its low for their
    /// labels; the low's label also stays clear of the row's baseline.
    const TEMP_LABEL_ROOM_ABOVE: f32 = 10.0;
    const TEMP_LABEL_ROOM_BELOW: f32 = 13.0;
    let hours = &weather.hourly[..weather.hourly.len().min(SPARK_HOURS + 1)];

    // Peaks over the same 24 hours as the chart, so they stay true until the
    // next wake (unlike a "now" reading).
    if !hours.is_empty() {
        let gust = hours.iter().map(|h| h.gust()).fold(0.0, f32::max);
        let uv = hours.iter().map(|h| h.uvi).fold(0.0, f32::max);
        let gust_fill = if gust >= MINI_GUST_ALERT_MPH {
            "red"
        } else {
            "black"
        };
        let uv_fill = if uv >= MINI_UV_ALERT { "red" } else { "black" };
        mini_text(
            &mut svg,
            2.0,
            82.0,
            11,
            "start",
            gust_fill,
            &format!("Gust {:.0}", gust),
        );
        mini_text(
            &mut svg,
            2.0,
            98.0,
            11,
            "start",
            uv_fill,
            &format!("UV {:.0}", uv),
        );
    }
    match (hours.first(), hours.last()) {
        (Some(first), Some(last)) if hours.len() >= 2 => {
            let frame = |y: f32, h: f32, min: f32, max: f32| SparkFrame {
                x: CHART_X,
                y,
                w: CHART_W,
                h,
                start: first.dt,
                end: last.dt,
                min,
                max,
            };
            let axis = frame(TEMP_TOP, RAIN_TOP + RAIN_H - TEMP_TOP, 0.0, 1.0);

            // Night, behind the temperature row only: a sparse dot pattern,
            // which stays crisp without dithering.
            svg.push_str(r#"  <defs><pattern id="miniNight" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="1" height="1" fill="black"/></pattern></defs>"#);
            svg.push('\n');
            let days: Vec<(i64, i64)> = weather
                .daily
                .iter()
                .map(|d| (d.sunrise, d.sunset))
                .collect();
            for (a, b) in night_spans(first.dt, last.dt, &days) {
                svg.push_str(&format!(
                    r#"  <rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="url(#miniNight)"/>"#,
                    axis.tx(a),
                    TEMP_TOP,
                    axis.tx(b) - axis.tx(a),
                    TEMP_H
                ));
                svg.push('\n');
            }

            // Ticks every 6 hours, labelled where they clear each other. A tick
            // too near the left edge to centre its label is labelled only when
            // it is within 10 minutes of now, so the edge label reads as now.
            const NEAR_NOW_SECS: i64 = 10 * 60;
            const EDGE_LABEL_HALF_W: f32 = 10.0;
            let mut last_label_x = f32::NEG_INFINITY;
            for hour in hours {
                let at = local(hour.dt);
                if at.hour() % 6 != 0 {
                    continue;
                }
                let x = axis.tx(hour.dt).floor() + 0.5;
                svg.push_str(&format!(
                    r#"  <line x1="{x:.1}" y1="{:.0}" x2="{x:.1}" y2="{:.0}" stroke="black" stroke-width="1" stroke-dasharray="1,3"/>"#,
                    // Whole-pixel ends, so each 1px dash fills one pixel.
                    TEMP_TOP + 0.5,
                    RAIN_TOP + RAIN_H + 2.5
                ));
                svg.push('\n');
                if x < CHART_X + EDGE_LABEL_HALF_W {
                    if (hour.dt - weather.current.dt).abs() <= NEAR_NOW_SECS {
                        mini_text(
                            &mut svg,
                            CHART_X,
                            99.0,
                            11,
                            "start",
                            "black",
                            &short_hour(at.hour()),
                        );
                        last_label_x = CHART_X + EDGE_LABEL_HALF_W;
                    }
                } else if x - last_label_x >= 24.0 && x <= CHART_X + CHART_W - 8.0 {
                    mini_text(
                        &mut svg,
                        x,
                        99.0,
                        11,
                        "middle",
                        "black",
                        &short_hour(at.hour()),
                    );
                    last_label_x = x;
                }
            }

            let temps: Vec<(i64, f32)> = hours.iter().map(|h| (h.dt, h.feels_like)).collect();
            let t_lo = temps.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let t_hi = temps.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            // The line fills the row minus label room top and bottom, so its
            // peak and low can carry their values. At least 10°F tall, so an
            // ordinary day doesn't look dramatic.
            let mid = (t_lo + t_hi) / 2.0;
            let half = ((t_hi - t_lo) / 2.0).max(5.0);
            let temp = frame(
                TEMP_TOP + TEMP_LABEL_ROOM_ABOVE,
                TEMP_H - TEMP_LABEL_ROOM_ABOVE - TEMP_LABEL_ROOM_BELOW,
                mid - half,
                mid + half,
            );
            svg.push_str(&format!(
                r#"  <path d="{}" fill="none" stroke="black" stroke-width="2" stroke-linejoin="round"/>"#,
                temp.line_path(&temps)
            ));
            svg.push('\n');
            let label = |svg: &mut String, &(t, v): &(i64, f32), dy: f32| {
                let x = temp.tx(t).clamp(CHART_X + 10.0, CHART_X + CHART_W - 10.0);
                // A white outline keeps the digits clear of night dots and the line.
                svg.push_str(&format!(
                    r#"  <text x="{:.1}" y="{:.1}" text-anchor="middle" font-family="DejaVu Sans" font-weight="bold" font-size="11" fill="{}" stroke="white" stroke-width="3" paint-order="stroke">{:.0}°</text>"#,
                    x,
                    temp.vy(v) + dy,
                    mini_temp_color(v),
                    v
                ));
                svg.push('\n');
            };
            let hottest = temps.iter().max_by(|a, b| a.1.total_cmp(&b.1));
            let coldest = temps.iter().min_by(|a, b| a.1.total_cmp(&b.1));
            if let (Some(hot), Some(cold)) = (hottest, coldest) {
                label(&mut svg, hot, -3.0);
                if cold.1.round() != hot.1.round() {
                    label(&mut svg, cold, 10.0);
                }
            }
            svg.push_str(&frame(TEMP_TOP, TEMP_H, 0.0, 1.0).baseline());
            svg.push('\n');

            let peak_mm = hours.iter().map(|h| h.rain_mm()).fold(0.0, f32::max);
            let rain = frame(RAIN_TOP, RAIN_H, 0.0, SPARK_RAIN_MAX_MM.max(peak_mm));
            for pair in hours.windows(2) {
                if !is_rainy_hour(&pair[0]) {
                    continue;
                }
                let (x0, x1) = (rain.tx(pair[0].dt), rain.tx(pair[1].dt));
                let height = (rain.bottom() - rain.vy(pair[0].rain_mm())).max(3.0);
                svg.push_str(&format!(
                    r#"  <rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="red"/>"#,
                    x0,
                    rain.bottom() - height,
                    (x1 - x0).max(1.0),
                    height
                ));
                svg.push('\n');
            }
            svg.push_str(&rain.baseline());
            svg.push('\n');
        }
        _ => mini_text(
            &mut svg,
            CHART_X + CHART_W / 2.0,
            56.0,
            11,
            "middle",
            "black",
            "No hourly data",
        ),
    }

    // Next four days.
    svg.push_str(
        r#"  <line x1="204.5" y1="19" x2="204.5" y2="104" stroke="black" stroke-width="1"/>"#,
    );
    svg.push('\n');
    for (i, day) in weather.daily.iter().skip(1).take(4).enumerate() {
        let y = 19.0 + 21.5 * i as f32;
        mini_text(
            &mut svg,
            208.0,
            y + 15.0,
            11,
            "start",
            "black",
            &local(day.dt).format("%a").to_string(),
        );
        if let Some(w) = day.weather.first() {
            let code = display_icon_for_daily_weather(&w.icon, day.pop, day.rain);
            if let Ok(uri) = load_tricolor_icon_as_data_uri(code, 20) {
                svg.push_str(&format!(
                    r#"  <image x="236" y="{:.1}" width="20" height="20" href="{}"/>"#,
                    y, uri
                ));
                svg.push('\n');
            }
        }
        svg.push_str(&format!(
            r#"  <text x="295" y="{:.1}" text-anchor="end" font-family="DejaVu Sans" font-weight="bold" font-size="11"><tspan fill="{}">{:.0}</tspan><tspan fill="black">/</tspan><tspan fill="{}">{:.0}</tspan></text>"#,
            y + 15.0,
            mini_temp_color(day.temp.max),
            day.temp.max,
            mini_temp_color(day.temp.min),
            day.temp.min
        ));
        svg.push('\n');
    }

    // Footer: an alert, else the rain window, else sunrise and sunset.
    if !weather.alerts.is_empty() {
        svg.push_str(r#"  <rect x="0" y="107" width="296" height="21" fill="red"/>"#);
        svg.push('\n');
        // 40 bold 12px characters fill the 296px bar.
        let line = alert_line(&weather.alerts, 40);
        mini_text(&mut svg, 4.0, 122.0, 12, "start", "white", &line);
    } else {
        svg.push_str(
            r#"  <line x1="0" y1="106.5" x2="296" y2="106.5" stroke="black" stroke-width="1"/>"#,
        );
        svg.push('\n');
        let footer =
            rain_window(&hours[..hours.len().saturating_sub(1)]).map(|(from, to, inches)| {
                format!(
                    "Rain {}–{}  {:.2}\"",
                    short_hour(local(from).hour()),
                    short_hour(local(to).hour()),
                    inches
                )
            });
        match footer {
            Some(text) => mini_text(&mut svg, 4.0, 122.0, 13, "start", "red", &text),
            None => mini_text(
                &mut svg,
                4.0,
                122.0,
                12,
                "start",
                "black",
                &format!(
                    "Sunrise {}   Sunset {}",
                    local(today.sunrise).format("%-I:%M%P"),
                    local(today.sunset).format("%-I:%M%P")
                ),
            ),
        }
    }

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::{
        alert_line, display_icon_for_daily_weather, generate_weather_mini_svg, night_spans,
        rain_fill, rain_window, short_hour, temperature_spark_range, truncate_chars,
        CurrentWeather, DailyWeather, FeelsLike, HourlyPrecip, HourlyWeather, SparkFrame,
        TempRange, Weather, WeatherAlert, WeatherData,
    };
    use reverse_geocoder::ReverseGeocoder;

    const HOUR: i64 = 3600;
    /// 2026-11-18 00:00 UTC.
    const T0: i64 = 1_763_424_000;

    fn hour(i: i64, feels_like: f32, pop: f32, rain_mm: f32) -> HourlyWeather {
        HourlyWeather {
            dt: T0 + i * HOUR,
            feels_like,
            wind_speed: 5.0,
            wind_gust: None,
            uvi: 0.0,
            pop,
            rain: (rain_mm > 0.0).then_some(HourlyPrecip { one_hour: rain_mm }),
        }
    }

    fn day(i: i64, min: f32, max: f32) -> DailyWeather {
        DailyWeather {
            dt: T0 + i * 24 * HOUR + 12 * HOUR,
            temp: TempRange {
                day: max,
                min,
                max,
                morn: min,
                night: min,
            },
            feels_like: FeelsLike {
                day: max,
                night: min,
                eve: max,
                morn: min,
            },
            humidity: 50,
            wind_speed: 5.0,
            wind_gust: None,
            sunrise: T0 + i * 24 * HOUR + 15 * HOUR,
            sunset: T0 + i * 24 * HOUR + 25 * HOUR,
            weather: vec![Weather {
                description: "clouds".into(),
                main: "Clouds".into(),
                icon: "03d".into(),
            }],
            pop: 0.0,
            rain: 0.0,
            uvi: None,
            clouds: None,
        }
    }

    fn weather(hourly: Vec<HourlyWeather>, alerts: Vec<WeatherAlert>) -> WeatherData {
        WeatherData {
            lat: 37.66,
            lon: -121.87,
            timezone_offset: -8 * 3600,
            current: CurrentWeather {
                dt: T0,
                temp: 55.0,
                humidity: 50,
                weather: vec![],
            },
            daily: (0..6).map(|i| day(i, 45.0, 64.0 + i as f32)).collect(),
            hourly,
            alerts,
        }
    }

    fn parses(svg: &str) -> bool {
        usvg::Tree::from_str(svg, &usvg::Options::default()).is_ok()
    }

    #[test]
    fn rain_window_spans_the_rainy_hours() {
        let hours: Vec<HourlyWeather> = (0..10)
            .map(|i| match i {
                3 => hour(i, 50.0, 0.6, 1.0),
                4 => hour(i, 50.0, 0.1, 5.0), // unlikely: not counted
                6 => hour(i, 50.0, 0.8, 3.0),
                _ => hour(i, 50.0, 0.0, 0.0),
            })
            .collect();
        let (from, to, inches) = rain_window(&hours).unwrap();
        assert_eq!((from, to), (T0 + 3 * HOUR, T0 + 7 * HOUR));
        assert!((inches - 4.0 / 25.4).abs() < 1e-4);
        assert_eq!(rain_window(&hours[..3]), None);
    }

    #[test]
    fn truncation_adds_an_ellipsis() {
        assert_eq!(truncate_chars("Wind Advisory", 20), "Wind Advisory");
        assert_eq!(truncate_chars("Wind Advisory", 6), "Wind …");
    }

    #[test]
    fn mini_axis_labels_the_left_edge_only_near_now() {
        let geocoder = ReverseGeocoder::new();
        // Hourly data from 18:00 local (T0 is 16:00 at UTC-8).
        let hours: Vec<HourlyWeather> = (2..27).map(|i| hour(i, 50.0, 0.0, 0.0)).collect();
        let edge_label = r#"x="66.0" y="99.0" text-anchor="start""#;

        let mut data = weather(hours, vec![]);
        data.current.dt = T0 + 2 * HOUR + 5 * 60;
        let svg = generate_weather_mini_svg(&data, None, &geocoder);
        assert!(!svg.contains(">Now<"));
        assert!(svg.contains(&format!("{edge_label} font-family=\"DejaVu Sans\" font-weight=\"bold\" font-size=\"11\" fill=\"black\">6p<")));

        data.current.dt = T0 + 2 * HOUR + 30 * 60;
        let svg = generate_weather_mini_svg(&data, None, &geocoder);
        assert!(!svg.contains(edge_label));
        // The later ticks are still labelled.
        assert!(svg.contains(">12a<"));
    }

    fn alerts(events: &[&str]) -> Vec<WeatherAlert> {
        events
            .iter()
            .map(|e| WeatherAlert {
                event: e.to_string(),
                description: String::new(),
                start: T0,
                end: T0 + 6 * HOUR,
            })
            .collect()
    }

    #[test]
    fn alert_line_dedupes_and_keeps_full_names_when_they_fit() {
        let line = alert_line(
            &alerts(&["Wind Advisory", "Frost Advisory", "Wind Advisory"]),
            40,
        );
        assert_eq!(line, "(2) Wind Advisory, Frost Advisory");
        assert_eq!(alert_line(&alerts(&["Wind Advisory"]), 40), "Wind Advisory");
    }

    #[test]
    fn alert_line_sorts_by_severity_and_abbreviates_to_fit() {
        let line = alert_line(
            &alerts(&["Frost Advisory", "Flood Watch", "Wind Warning"]),
            40,
        );
        assert_eq!(line, "(3) Wind Wrn, Flood Watch, Frost Adv");
    }

    #[test]
    fn alert_line_keeps_the_count_when_cut() {
        let line = alert_line(
            &alerts(&[
                "Winter Storm Warning",
                "Avalanche Warning",
                "Wind Chill Advisory",
                "Special Weather Statement",
            ]),
            40,
        );
        assert_eq!(line, "(4) Winter Storm Wrn, Avalanche Wrn, Wi…");
    }

    #[test]
    fn mini_footer_prefers_alert_then_rain_then_sun() {
        let geocoder = ReverseGeocoder::new();
        let dry: Vec<HourlyWeather> = (0..25).map(|i| hour(i, 50.0, 0.0, 0.0)).collect();
        let svg = generate_weather_mini_svg(&weather(dry, vec![]), Some(80), &geocoder);
        assert!(parses(&svg));
        assert!(svg.contains("Sunrise 7:00am"));
        assert!(svg.contains(">Gust 5<"));
        assert!(svg.contains(">UV 0<"));
        assert!(!svg.contains("Now 55"));
        assert!(svg.contains(">80%<"));

        let wet: Vec<HourlyWeather> = (0..25)
            .map(|i| {
                if (6..9).contains(&i) {
                    hour(i, 50.0, 0.7, 2.0)
                } else {
                    hour(i, 50.0, 0.0, 0.0)
                }
            })
            .collect();
        let svg = generate_weather_mini_svg(&weather(wet, vec![]), None, &geocoder);
        assert!(parses(&svg));
        // 06:00-09:00 UTC is 10pm-1am at UTC-8.
        assert!(svg.contains("Rain 10p–1a  0.24\""));
        assert!(!svg.contains("Sunrise"));

        let alert = WeatherAlert {
            event: "Wind & Dust Advisory".into(),
            description: String::new(),
            start: T0,
            end: T0 + 6 * HOUR,
        };
        let svg = generate_weather_mini_svg(&weather(vec![], vec![alert]), None, &geocoder);
        assert!(parses(&svg));
        assert!(svg.contains(">Wind &amp; Dust Advisory<"));
        assert!(svg.contains(r#"fill="red"/>"#));
        assert!(svg.contains("No hourly data"));
    }

    #[test]
    fn night_spans_are_the_gaps_between_daylight() {
        // Window 0..100, daylight 20..50 and 80..130.
        assert_eq!(
            night_spans(0, 100, &[(80, 130), (20, 50)]),
            vec![(0, 20), (50, 80)]
        );
        // Window entirely in daylight.
        assert_eq!(night_spans(30, 40, &[(20, 50)]), vec![]);
        // No daylight data: all night.
        assert_eq!(night_spans(0, 10, &[]), vec![(0, 10)]);
    }

    #[test]
    fn temperature_range_is_at_least_twenty_degrees() {
        assert_eq!(temperature_spark_range(55.0, 60.0), (45.5, 69.5));
        assert_eq!(temperature_spark_range(40.0, 80.0), (38.0, 82.0));
    }

    #[test]
    fn short_hour_labels() {
        let labels: Vec<String> = [0, 6, 12, 18].into_iter().map(short_hour).collect();
        assert_eq!(labels, ["12a", "6a", "12p", "6p"]);
    }

    #[test]
    fn smooth_gradient_is_anchored_at_the_stop_values() {
        // Frame shows 40..80 over y 100..140; the gradient spans 32..100.
        let frame = SparkFrame {
            x: 0.0,
            y: 100.0,
            w: 50.0,
            h: 40.0,
            start: 0,
            end: 1,
            min: 40.0,
            max: 80.0,
        };
        let g = frame.smooth_gradient("g", &[(32.0, "blue"), (66.0, "green"), (100.0, "red")]);
        // 32 sits 8px below the frame, 100 sits 20px above it.
        assert!(g.contains(r#"y1="148.0" x2="0" y2="80.0""#));
        assert!(g.contains(r#"offset="50.0%" style="stop-color:green"#));
        assert_eq!(frame.vy(50.0), 130.0);
        assert_eq!(frame.vy(500.0), 100.0); // clamped to the top
    }

    #[test]
    fn rain_bar_scales_to_one_inch_and_matches_icon_cutoff() {
        let close = |a: Option<f32>, b: f32| (a.unwrap() - b).abs() < 0.01;
        assert!(close(rain_fill(0.25, 2.54), 10.0)); // 0.1"
        assert!(close(rain_fill(0.80, 12.7), 50.0)); // 0.5"
        assert!(close(rain_fill(0.80, 50.8), 100.0)); // 2" clamps to full
        assert_eq!(rain_fill(0.24, 20.0), None);
        assert_eq!(rain_fill(0.90, 2.53), None);
    }

    #[test]
    fn low_pop_or_low_rain_rain_icons_are_replaced_with_clouds() {
        assert_eq!(display_icon_for_daily_weather("09d", 0.24, 1.0), "03d");
        assert_eq!(display_icon_for_daily_weather("09n", 0.24, 1.0), "03n");
        assert_eq!(display_icon_for_daily_weather("10d", 0.24, 1.0), "04d");
        assert_eq!(display_icon_for_daily_weather("10n", 0.24, 1.0), "04n");
        // pop >= cutoff, rain below 2.54mm cutoff → replace with clouds
        assert_eq!(display_icon_for_daily_weather("10d", 0.25, 2.53), "04d");
        // pop >= cutoff, rain at/above 2.54mm cutoff → keep rain icon
        assert_eq!(display_icon_for_daily_weather("10d", 0.25, 2.54), "10d");
        assert_eq!(display_icon_for_daily_weather("01d", 0.10, 0.0), "01d");
    }
}
