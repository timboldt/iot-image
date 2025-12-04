use base64::{engine::general_purpose, Engine as _};
use chrono::prelude::*;
use chrono::Timelike;
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
pub struct WeatherAlert {
    pub event: String,
    pub description: String,
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
    pub humidity: i32,
    pub wind_speed: f32,
    pub sunrise: i64,
    pub sunset: i64,
    pub weather: Vec<Weather>,
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

/// Loads an SVG icon and returns its content as a base64-encoded data URI
fn load_weather_icon_as_data_uri(icon_code: &str) -> Result<String, std::io::Error> {
    let icon_filename = map_weather_icon(icon_code);
    let icon_path = format!("assets/static/fill-svg-static/{}", icon_filename);
    let svg_content = fs::read(&icon_path)?;

    // Encode as base64 data URI for embedding in SVG
    let encoded = general_purpose::STANDARD.encode(&svg_content);
    Ok(format!("data:image/svg+xml;base64,{}", encoded))
}

struct BarData {
    fill_percent: f32, // 0.0 to 100.0
    bar_color: &'static str,
}

fn temperature_bar(temp: f32) -> BarData {
    // Map temperature range 20°F to 100°F to 0-100% fill
    let fill_percent = ((temp - 20.0) / 80.0 * 100.0).clamp(0.0, 100.0);

    let bar_color = if temp < 40.0 {
        "blue" // Freezing/Very Cold
    } else if temp < 50.0 {
        "rgb(128,128,255)" // Cold (will dither blue+white for light blue)
    } else if temp < 60.0 {
        "rgb(0,200,128)" // Cool (cyan-green, will dither green+blue)
    } else if temp < 70.0 {
        "green" // Comfortable
    } else if temp < 80.0 {
        "rgb(200,255,0)" // Mild (will dither green+yellow for yellow-green)
    } else if temp < 90.0 {
        "orange" // Warm (will dither red+yellow)
    } else {
        "red" // Hot
    };

    BarData {
        fill_percent,
        bar_color,
    }
}

fn humidity_bar(humidity: i32, temp: f32) -> BarData {
    // Fill percent is directly proportional to humidity
    let fill_percent = humidity.clamp(0, 100) as f32;

    let bar_color = if humidity < 20 {
        "orange" // Very dry (will dither red+yellow)
    } else if humidity < 35 {
        "yellow" // Dry
    } else if humidity < 50 {
        "rgb(0,200,128)" // Comfortable-dry (cyan-green, will dither green+blue)
    } else if humidity < 65 {
        "green" // Comfortable
    } else if temp >= 75.0 {
        // High humidity with warm/hot temps - increasingly uncomfortable
        if humidity < 80 {
            "orange" // Humid and warm (will dither red+yellow)
        } else {
            "red" // Very humid and hot - uncomfortable
        }
    } else {
        // High humidity but cooler temps
        if humidity < 80 {
            "rgb(128,128,255)" // Moist (will dither blue+white for light blue)
        } else {
            "blue" // Very moist
        }
    };

    BarData {
        fill_percent,
        bar_color,
    }
}

fn wind_bar(wind_speed: f32) -> BarData {
    // Map wind speed 0-40 mph to 0-100% fill
    let fill_percent = (wind_speed / 40.0 * 100.0).clamp(0.0, 100.0);

    let bar_color = if wind_speed < 3.0 {
        "green" // Calm
    } else if wind_speed < 8.0 {
        "rgb(0,200,128)" // Light breeze (cyan-green, will dither green+blue)
    } else if wind_speed < 12.0 {
        "rgb(128,128,255)" // Gentle breeze (will dither blue+white for light blue)
    } else if wind_speed < 18.0 {
        "blue" // Moderate breeze
    } else if wind_speed < 25.0 {
        "yellow" // Windy
    } else if wind_speed < 32.0 {
        "orange" // Very windy (will dither red+yellow)
    } else {
        "red" // Storm/dangerous
    };

    BarData {
        fill_percent,
        bar_color,
    }
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

/// Generates an SVG weather display from weather data
///
/// # Arguments
/// * `weather` - The weather data to display
/// * `battery_pct` - Optional battery percentage to display
///
/// # Returns
/// A String containing the SVG markup
pub fn generate_weather_svg(weather: &WeatherData, battery_pct: Option<u8>) -> String {
    // Create timezone offset from the weather data
    let tz_offset = chrono::FixedOffset::east_opt(weather.timezone_offset).unwrap();

    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480">"#,
    );
    svg.push('\n');

    // Define gradients for bars
    svg.push_str(r#"  <defs>"#);
    svg.push('\n');

    // Temperature gradient: blue (cold) -> green (comfortable) -> red (hot)
    svg.push_str(r#"    <linearGradient id="tempGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="50%" style="stop-color:green;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // Humidity gradient: orange (dry) -> blue (humid)
    svg.push_str(r#"    <linearGradient id="humidityGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:orange;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:blue;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"    </linearGradient>"#);
    svg.push('\n');

    // Battery gradient: red (low) -> orange (medium) -> green (full)
    svg.push_str(r#"    <linearGradient id="batteryGradient" x1="0%" y1="0%" x2="100%" y2="0%">"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="0%" style="stop-color:red;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="50%" style="stop-color:orange;stop-opacity:1" />"#);
    svg.push('\n');
    svg.push_str(r#"      <stop offset="100%" style="stop-color:green;stop-opacity:1" />"#);
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
        .unwrap()
        .with_timezone(&tz_offset);
    svg.push_str(&format!(
        r#"  <text x="20" y="35" font-family="Arial" font-size="28" font-weight="bold" fill="black">{}</text>"#,
        today_time.format("%A, %B %e")
    ));
    svg.push('\n');

    // Latitude/Longitude
    svg.push_str(&format!(
        r#"  <text x="20" y="58" font-family="Arial" font-size="16" fill="black">{:.4}°, {:.4}°</text>"#,
        weather.lat, weather.lon
    ));
    svg.push('\n');

    // Weather icon (large, centered in left section)
    if let Some(w) = today.weather.first() {
        // Embed weather icon as a data URI
        if let Ok(data_uri) = load_weather_icon_as_data_uri(&w.icon) {
            svg.push_str(&format!(
                r#"  <image x="350" y="30" width="100" height="100" href="{}"/>"#,
                data_uri
            ));
            svg.push('\n');
        }
    }

    // Morning/Day/Night temperatures in a row
    let temp_y = 150.0;
    let temp_spacing = 140.0;

    // Morning
    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="18" fill="black">Morning</text>"#,
        temp_y - 10.0
    ));
    svg.push('\n');

    let morn_bar = temperature_bar(today.temp.morn);
    let bar_width = 100.0;
    let bar_height = 20.0;
    let bar_inset = 2.0; // Inset to avoid covering border
    let fill_width = (bar_width - bar_inset * 2.0) * (morn_bar.fill_percent / 100.0);

    // Background (container) rectangle
    svg.push_str(&format!(
        r#"  <rect x="35" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
        temp_y + 5.0, bar_width, bar_height
    ));
    svg.push('\n');

    // ClipPath for morning temp
    svg.push_str(r#"  <clipPath id="mornTempClip">"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="2"/>"#,
        35.0 + bar_inset,
        temp_y + 5.0 + bar_inset,
        fill_width,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');
    svg.push_str(r#"  </clipPath>"#);
    svg.push('\n');

    // Full-width gradient rect, clipped
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#tempGradient)" clip-path="url(#mornTempClip)" rx="2"/>"#,
        35.0 + bar_inset,
        temp_y + 5.0 + bar_inset,
        bar_width - bar_inset * 2.0,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');

    // Day
    svg.push_str(&format!(
        r#"  <text x="{}" y="{}" font-family="Arial" font-size="18" fill="black">Day</text>"#,
        40.0 + temp_spacing,
        temp_y - 10.0
    ));
    svg.push('\n');

    let day_bar = temperature_bar(today.temp.day);
    let fill_width = (bar_width - bar_inset * 2.0) * (day_bar.fill_percent / 100.0);

    // Background (container) rectangle
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
        35.0 + temp_spacing, temp_y + 5.0, bar_width, bar_height
    ));
    svg.push('\n');

    // ClipPath for day temp
    svg.push_str(r#"  <clipPath id="dayTempClip">"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="2"/>"#,
        35.0 + temp_spacing + bar_inset,
        temp_y + 5.0 + bar_inset,
        fill_width,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');
    svg.push_str(r#"  </clipPath>"#);
    svg.push('\n');

    // Full-width gradient rect, clipped
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#tempGradient)" clip-path="url(#dayTempClip)" rx="2"/>"#,
        35.0 + temp_spacing + bar_inset,
        temp_y + 5.0 + bar_inset,
        bar_width - bar_inset * 2.0,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');

    // Night
    svg.push_str(&format!(
        r#"  <text x="{}" y="{}" font-family="Arial" font-size="18" fill="black">Night</text>"#,
        40.0 + 2.0 * temp_spacing,
        temp_y - 10.0
    ));
    svg.push('\n');

    let night_bar = temperature_bar(today.temp.night);
    let fill_width = (bar_width - bar_inset * 2.0) * (night_bar.fill_percent / 100.0);

    // Background (container) rectangle
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
        35.0 + 2.0 * temp_spacing, temp_y + 5.0, bar_width, bar_height
    ));
    svg.push('\n');

    // ClipPath for night temp
    svg.push_str(r#"  <clipPath id="nightTempClip">"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="2"/>"#,
        35.0 + 2.0 * temp_spacing + bar_inset,
        temp_y + 5.0 + bar_inset,
        fill_width,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');
    svg.push_str(r#"  </clipPath>"#);
    svg.push('\n');

    // Full-width gradient rect, clipped
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#tempGradient)" clip-path="url(#nightTempClip)" rx="2"/>"#,
        35.0 + 2.0 * temp_spacing + bar_inset,
        temp_y + 5.0 + bar_inset,
        bar_width - bar_inset * 2.0,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');

    // Humidity and Wind in a row
    let detail_y = 230.0;

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Humidity</text>"#,
        detail_y
    ));
    svg.push('\n');

    let hum_bar = humidity_bar(today.humidity, today.temp.day);
    let hum_bar_width = 150.0;
    let hum_fill_width = (hum_bar_width - bar_inset * 2.0) * (hum_bar.fill_percent / 100.0);

    // Background rectangle
    svg.push_str(&format!(
        r#"  <rect x="170" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
        detail_y - 15.0, hum_bar_width, bar_height
    ));
    svg.push('\n');

    // Create clipPath for this specific humidity bar
    let hum_clip_id = "humClip";
    svg.push_str(&format!(r#"  <clipPath id="{}">"#, hum_clip_id));
    svg.push('\n');
    svg.push_str(&format!(
        r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="2"/>"#,
        170.0 + bar_inset,
        detail_y - 15.0 + bar_inset,
        hum_fill_width,
        bar_height - bar_inset * 2.0
    ));
    svg.push('\n');
    svg.push_str(r#"  </clipPath>"#);
    svg.push('\n');

    // Full-width gradient rect, clipped to fill width
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#humidityGradient)" clip-path="url(#{})" rx="2"/>"#,
        170.0 + bar_inset,
        detail_y - 15.0 + bar_inset,
        hum_bar_width - bar_inset * 2.0,
        bar_height - bar_inset * 2.0,
        hum_clip_id
    ));
    svg.push('\n');

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Wind</text>"#,
        detail_y + 35.0
    ));
    svg.push('\n');

    let wind_bar = wind_bar(today.wind_speed);
    let wind_fill_width = (hum_bar_width - bar_inset * 2.0) * (wind_bar.fill_percent / 100.0);

    // Background rectangle
    svg.push_str(&format!(
        r#"  <rect x="170" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
        detail_y + 20.0, hum_bar_width, bar_height
    ));
    svg.push('\n');

    // Filled portion (inset to not cover border)
    svg.push_str(&format!(
        r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="{}" rx="2"/>"#,
        170.0 + bar_inset,
        detail_y + 20.0 + bar_inset,
        wind_fill_width,
        bar_height - bar_inset * 2.0,
        wind_bar.bar_color
    ));
    svg.push('\n');

    // Sunrise/sunset
    let sunrise_time = Utc
        .timestamp_opt(today.sunrise, 0)
        .unwrap()
        .with_timezone(&tz_offset);
    let sunset_time = Utc
        .timestamp_opt(today.sunset, 0)
        .unwrap()
        .with_timezone(&tz_offset);

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Sunrise: {}</text>"#,
        detail_y + 70.0, sunrise_time.format("%l:%M %P")
    ));
    svg.push('\n');

    svg.push_str(&format!(
        r#"  <text x="40" y="{}" font-family="Arial" font-size="20" fill="black">Sunset: {}</text>"#,
        detail_y + 105.0, sunset_time.format("%l:%M %P")
    ));
    svg.push('\n');

    // Weather Alerts
    if !weather.alerts.is_empty() {
        let alert_y = detail_y + 150.0;
        svg.push_str(&format!(
            r#"  <text x="40" y="{}" font-family="Arial" font-size="18" font-weight="bold" fill="red">ALERTS:</text>"#,
            alert_y
        ));
        svg.push('\n');

        let mut current_y = alert_y + 25.0;
        for alert in &weather.alerts {
            // Alert event name
            svg.push_str(&format!(
                r#"  <text x="40" y="{}" font-family="Arial" font-size="16" font-weight="bold" fill="black">{}</text>"#,
                current_y, alert.event
            ));
            svg.push('\n');
            current_y += 20.0;

            // Alert description (wrap at ~60 characters)
            let words: Vec<&str> = alert.description.split_whitespace().collect();
            let mut line = String::new();
            for word in words {
                if line.len() + word.len() + 1 > 60 {
                    svg.push_str(&format!(
                        r#"  <text x="40" y="{}" font-family="Arial" font-size="14" fill="black">{}</text>"#,
                        current_y, line.trim()
                    ));
                    svg.push('\n');
                    current_y += 18.0;
                    line = String::from(word);
                } else {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.push_str(word);
                }
            }
            if !line.is_empty() {
                svg.push_str(&format!(
                    r#"  <text x="40" y="{}" font-family="Arial" font-size="14" fill="black">{}</text>"#,
                    current_y, line.trim()
                ));
                svg.push('\n');
                current_y += 25.0;
            }
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
            .unwrap()
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
            if let Ok(data_uri) = load_weather_icon_as_data_uri(&w.icon) {
                svg.push_str(&format!(
                    r#"  <image x="{}" y="{}" width="100" height="100" href="{}"/>"#,
                    right_x + 150.0,
                    y - 30.0,
                    data_uri
                ));
                svg.push('\n');
            }
        }

        // Temperature bar indicator
        let temp_bar = temperature_bar(day.temp.day);
        let forecast_bar_width = 120.0;
        let forecast_bar_height = 16.0;
        let forecast_inset = 2.0;
        let forecast_fill_width =
            (forecast_bar_width - forecast_inset * 2.0) * (temp_bar.fill_percent / 100.0);

        // Background rectangle
        svg.push_str(&format!(
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="3"/>"#,
            right_x, y + 30.0, forecast_bar_width, forecast_bar_height
        ));
        svg.push('\n');

        // ClipPath for this forecast day's temp bar
        let clip_id = format!("forecastTempClip{}", idx);
        svg.push_str(&format!(r#"  <clipPath id="{}">"#, clip_id));
        svg.push('\n');
        svg.push_str(&format!(
            r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="1"/>"#,
            right_x + forecast_inset,
            y + 30.0 + forecast_inset,
            forecast_fill_width,
            forecast_bar_height - forecast_inset * 2.0
        ));
        svg.push('\n');
        svg.push_str(r#"  </clipPath>"#);
        svg.push('\n');

        // Full-width gradient rect, clipped
        svg.push_str(&format!(
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#tempGradient)" clip-path="url(#{})" rx="1"/>"#,
            right_x + forecast_inset,
            y + 30.0 + forecast_inset,
            forecast_bar_width - forecast_inset * 2.0,
            forecast_bar_height - forecast_inset * 2.0,
            clip_id
        ));
        svg.push('\n');
    }

    // Footer with battery and last updated
    let footer_y = 470;

    // Battery bar (if provided) - now on the left
    let pct = battery_pct.unwrap_or(50);
    let battery_bar_width = 100.0;
    let battery_bar_height = 12.0;
    let battery_x = 75.0;
    let battery_y = footer_y as f32 - 10.0;
    let battery_inset = 2.0;
    let battery_fill_width = (battery_bar_width - battery_inset * 2.0) * (pct as f32 / 100.0);

    // Label
    svg.push_str(&format!(
        r#"  <text x="10" y="{}" font-size="12" fill="black">Battery:</text>"#,
        footer_y
    ));
    svg.push('\n');

    // Background (container) rectangle
    svg.push_str(&format!(
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2" rx="2"/>"#,
            battery_x, battery_y, battery_bar_width, battery_bar_height
        ));
    svg.push('\n');

    // ClipPath for battery bar
    svg.push_str(r#"  <clipPath id="batteryClip">"#);
    svg.push('\n');
    svg.push_str(&format!(
        r#"    <rect x="{}" y="{}" width="{}" height="{}" rx="1"/>"#,
        battery_x + battery_inset,
        battery_y + battery_inset,
        battery_fill_width,
        battery_bar_height - battery_inset * 2.0
    ));
    svg.push('\n');
    svg.push_str(r#"  </clipPath>"#);
    svg.push('\n');

    // Full-width gradient rect, clipped
    svg.push_str(&format!(
            r#"  <rect x="{}" y="{}" width="{}" height="{}" fill="url(#batteryGradient)" clip-path="url(#batteryClip)" rx="1"/>"#,
            battery_x + battery_inset,
            battery_y + battery_inset,
            battery_bar_width - battery_inset * 2.0,
            battery_bar_height - battery_inset * 2.0
        ));
    svg.push('\n');

    // Last updated timestamp - now on the right
    let now = Local::now();
    let timestamp = format!(
        "Last updated: {:02}:{:02}:{:02}",
        now.hour(),
        now.minute(),
        now.second()
    );
    svg.push_str(&format!(
        r#"  <text x="790" y="{}" text-anchor="end" font-size="12" fill="black">{}</text>"#,
        footer_y, timestamp
    ));
    svg.push('\n');

    svg.push_str("</svg>");
    svg
}
