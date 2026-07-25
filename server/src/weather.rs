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
const RAIN_ICON_AMOUNT_CUTOFF: f32 = 2.54; // 0.1 inch in mm

fn display_icon_for_daily_weather(icon_code: &str, pop: f32, rain: f32) -> &str {
    if pop < RAIN_ICON_POP_CUTOFF || rain < RAIN_ICON_AMOUNT_CUTOFF {
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

fn humidity_fill(humidity: i32) -> f32 {
    humidity.clamp(0, 100) as f32
}

fn wind_fill(wind_speed: f32) -> f32 {
    // 0-60 mph spans the bar.
    (wind_speed / 60.0 * 100.0).clamp(0.0, 100.0)
}

fn cloudiness_fill(clouds: i32) -> f32 {
    clouds.clamp(0, 100) as f32
}

fn uvi_fill(uvi: f32) -> f32 {
    // 0-11+ spans the bar.
    (uvi / 11.0 * 100.0).clamp(0.0, 100.0)
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
        "https://api.openweathermap.org/data/3.0/onecall?lat={}&lon={}&units=imperial&exclude=minutely,hourly&appid={}",
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
    svg.push_str(&svg_common::last_updated_text(790.0, footer_y as f64, "end"));
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

    // Humidity gradient: red(extreme) -> orange (dry) -> blue (humid).
    svg.push_str(r#"    <linearGradient id="humidityGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="20%" style="stop-color:orange;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // Battery gradient: red (low) -> green (full)
    svg.push_str(svg_common::BATTERY_GRADIENT_DEF);
    svg.push('\n');

    // Wind gradient: green (calm) -> orange (windy) -> red (dangerous)
    svg.push_str(r#"    <linearGradient id="windGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="50%" style="stop-color:orange;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // Cloudiness gradient: blue (clear) -> black (overcast)
    svg.push_str(
        r#"    <linearGradient id="cloudinessGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#,
    );
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:lightblue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:black;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // UVI gradient: green (low) -> yellow (moderate) -> orange (high) -> red (very high) -> purple (extreme)
    svg.push_str(r#"    <linearGradient id="uviGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="27%" style="stop-color:yellow;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="54%" style="stop-color:orange;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="72%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:purple;stop-opacity:1" />"#);
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

    // Morning/Day/Eve temperatures in a row
    let temp_y = 120.0;
    let temp_spacing = 140.0;

    // Morning
    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="18" fill="black">Morning</text>"#,
        temp_y - 10.0
    ));
    svg.push('\n');

    let bar_width = 100.0;
    let bar_height = 20.0;
    let temp_bar = |x: f32, clip_id: &'static str| GradientBar {
        x,
        y: temp_y + 5.0,
        width: bar_width,
        height: bar_height,
        gradient_id: "tempGradient",
        clip_id,
        inner_radius: 2,
    };

    svg.push_str(
        &temp_bar(35.0, "mornTempClip").render_pct(temperature_fill(today.feels_like.morn)),
    );
    svg.push('\n');

    // Day
    svg.push_str(&format!(
        r#"  <text x="{}" y="{}" font-family="Arial" font-size="18" fill="black">Day</text>"#,
        40.0 + temp_spacing,
        temp_y - 10.0
    ));
    svg.push('\n');

    svg.push_str(
        &temp_bar(35.0 + temp_spacing, "dayTempClip")
            .render_pct(temperature_fill(today.feels_like.day)),
    );
    svg.push('\n');

    // Eve
    svg.push_str(&format!(
        r#"  <text x="{}" y="{}" font-family="Arial" font-size="18" fill="black">Evening</text>"#,
        40.0 + 2.0 * temp_spacing,
        temp_y - 10.0
    ));
    svg.push('\n');

    svg.push_str(
        &temp_bar(35.0 + 2.0 * temp_spacing, "eveTempClip")
            .render_pct(temperature_fill(today.feels_like.eve)),
    );
    svg.push('\n');

    // Humidity and Wind in a row
    let detail_y = 200.0;

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Humidity</text>"#,
        detail_y
    ));
    svg.push('\n');

    let hum_bar_width = 150.0;
    let detail_bar = |y: f32, gradient_id: &'static str, clip_id: &'static str| GradientBar {
        x: 170.0,
        y,
        width: hum_bar_width,
        height: bar_height,
        gradient_id,
        clip_id,
        inner_radius: 2,
    };

    svg.push_str(
        &detail_bar(detail_y - 15.0, "humidityGradient", "humClip")
            .render_pct(humidity_fill(today.humidity)),
    );
    svg.push('\n');

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Wind</text>"#,
        detail_y + 35.0
    ));
    svg.push('\n');

    svg.push_str(
        &detail_bar(detail_y + 20.0, "windGradient", "windClip")
            .render_pct(wind_fill(today.wind_gust.unwrap_or(today.wind_speed))),
    );
    svg.push('\n');

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

    // Always show cloudiness/UVI/sunrise/sunset, plus warning titles if present

    // Cloudiness bar
    svg.push_str(&format!(
            r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Cloudiness</text>"#,
            detail_y + 70.0
    ));
    svg.push('\n');

    svg.push_str(
        &detail_bar(detail_y + 55.0, "cloudinessGradient", "cloudsClip")
            .render_pct(cloudiness_fill(today.clouds.unwrap_or(0))),
    );
    svg.push('\n');

    // UVI bar
    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">UV Index</text>"#,
        detail_y + 105.0
    ));
    svg.push('\n');

    svg.push_str(
        &detail_bar(detail_y + 90.0, "uviGradient", "uviClip")
            .render_pct(uvi_fill(today.uvi.unwrap_or(0.0))),
    );
    svg.push('\n');

    // Sunrise text (moved down)
    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Sunrise: {}</text>"#,
        detail_y + 160.0, sunrise_time.format("%l:%M %P")
    ));
    svg.push('\n');

    // Sunset text (moved down)
    svg.push_str(&format!(
        r#"  <text x="280" y="{}" font-family="Arial" font-size="20" fill="black">Sunset: {}</text>"#,
        detail_y + 160.0, sunset_time.format("%l:%M %P")
    ));
    svg.push('\n');

    // Weather warning titles (if any)
    if !weather.alerts.is_empty() {
        let mut alert_y = detail_y + 190.0;
        for alert in &weather.alerts {
            let (Some(start_utc), Some(end_utc)) = (
                chrono::DateTime::from_timestamp(alert.start, 0),
                chrono::DateTime::from_timestamp(alert.end, 0),
            ) else {
                continue;
            };
            let start_time = start_utc.with_timezone(&tz_offset);
            let mut end_time = end_utc.with_timezone(&tz_offset);

            // Round start down (use hour as-is), round end up (add hour if has minutes)
            let start_hour = start_time.hour();
            if end_time.minute() > 0 || end_time.second() > 0 {
                end_time += chrono::Duration::hours(1);
            }
            let end_hour = end_time.hour();

            // Convert to 12-hour format with am/pm
            let (start_12h, start_ampm) = if start_hour == 0 {
                (12, "am")
            } else if start_hour < 12 {
                (start_hour, "am")
            } else if start_hour == 12 {
                (12, "pm")
            } else {
                (start_hour - 12, "pm")
            };

            let (end_12h, end_ampm) = if end_hour == 0 {
                (12, "am")
            } else if end_hour < 12 {
                (end_hour, "am")
            } else if end_hour == 12 {
                (12, "pm")
            } else {
                (end_hour - 12, "pm")
            };

            let time_range = format!(
                "({}/{} {}{} - {}/{} {}{})",
                start_time.month(),
                start_time.day(),
                start_12h,
                start_ampm,
                end_time.month(),
                end_time.day(),
                end_12h,
                end_ampm
            );

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
            right_x, y + 5.0, day_name
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
    svg.push_str(&svg_common::last_updated_text(790.0, footer_y as f64, "end"));
    svg.push('\n');

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::display_icon_for_daily_weather;

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
