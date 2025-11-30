use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct WeatherData {
    pub lat: f32,
    pub lon: f32,
    pub timezone_offset: i32,
    pub current: CurrentWeather,
    pub daily: Vec<DailyWeather>,
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

fn temperature_text(temp: f32) -> &'static str {
    if temp < 30.0 {
        "Frz"
    } else if temp < 50.0 {
        "Cold"
    } else if temp < 70.0 {
        "Cool"
    } else if temp < 80.0 {
        "Mild"
    } else if temp < 90.0 {
        "Warm"
    } else {
        "Hot"
    }
}

fn humidity_text(humidity: i32, temp: f32) -> &'static str {
    if humidity < 20 {
        "Dry"
    } else if humidity < 60 {
        "Norm"
    } else if temp >= 70.0 {
        "Hum"
    } else {
        "Norm"
    }
}

fn wind_text(wind_speed: f32) -> &'static str {
    if wind_speed < 5.0 {
        "Calm"
    } else if wind_speed < 15.0 {
        "Brzy"
    } else if wind_speed < 30.0 {
        "Windy"
    } else {
        "Storm"
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
///
/// # Returns
/// A String containing the SVG markup
pub fn generate_weather_svg(weather: &WeatherData) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480" viewBox="0 0 800 480">"#,
    );
    svg.push('\n');

    // Background
    svg.push_str(r#"  <rect width="800" height="480" fill="white"/>"#);
    svg.push('\n');

    // Title
    svg.push_str(r#"  <text x="400" y="40" font-family="Arial" font-size="32" font-weight="bold" text-anchor="middle" fill="black">"#);
    svg.push_str("Weather Forecast");
    svg.push_str("</text>\n");

    // Current weather section
    let y_current = 80;
    svg.push_str(&format!(r#"  <text x="20" y="{}" font-family="Arial" font-size="24" font-weight="bold" fill="black">Current:</text>"#, y_current));
    svg.push('\n');

    svg.push_str(&format!(r#"  <text x="150" y="{}" font-family="Arial" font-size="20" fill="black">{:.1}°F ({})</text>"#,
        y_current, weather.current.temp, temperature_text(weather.current.temp)));
    svg.push('\n');

    svg.push_str(&format!(
        r#"  <text x="320" y="{}" font-family="Arial" font-size="20" fill="black">{}% ({})</text>"#,
        y_current,
        weather.current.humidity,
        humidity_text(weather.current.humidity, weather.current.temp)
    ));
    svg.push('\n');

    if let Some(w) = weather.current.weather.first() {
        svg.push_str(&format!(
            r#"  <text x="450" y="{}" font-family="Arial" font-size="20" fill="black">{}</text>"#,
            y_current, w.main
        ));
        svg.push('\n');
    }

    // 5-day forecast
    svg.push_str(r#"  <text x="20" y="130" font-family="Arial" font-size="24" font-weight="bold" fill="black">5-Day Forecast:</text>"#);
    svg.push('\n');

    let days_to_show = weather.daily.len().min(5);
    let column_width = 780.0 / days_to_show as f32;

    for (day_idx, day) in weather.daily.iter().take(days_to_show).enumerate() {
        let x = 10.0 + (day_idx as f32 * column_width);
        let y_base = 160.0;

        let day_time = Utc.timestamp_opt(day.dt, 0).unwrap();
        let day_name =
            ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][day_time.weekday() as usize];

        // Day header with border
        svg.push_str(&format!(r#"  <rect x="{}" y="{}" width="{}" height="300" fill="none" stroke="black" stroke-width="1"/>"#,
            x, y_base, column_width, ));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="18" font-weight="bold" text-anchor="middle" fill="black">{}</text>"#,
            x + column_width / 2.0, y_base + 20.0, day_name));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="14" text-anchor="middle" fill="black">{}</text>"#,
            x + column_width / 2.0, y_base + 38.0, day_time.format("%m/%d")));
        svg.push('\n');

        // Weather condition
        if let Some(w) = day.weather.first() {
            svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="16" text-anchor="middle" fill="black">{}</text>"#,
                x + column_width / 2.0, y_base + 60.0, w.main));
            svg.push('\n');
        }

        // Temperature info
        let y_temp = y_base + 90.0;
        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="14" text-anchor="middle" fill="black">Day: {:.0}°F</text>"#,
            x + column_width / 2.0, y_temp, day.temp.day));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="12" text-anchor="middle" fill="gray">{}</text>"#,
            x + column_width / 2.0, y_temp + 18.0, temperature_text(day.temp.day)));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="12" text-anchor="middle" fill="black">Hi: {:.0}°F</text>"#,
            x + column_width / 2.0, y_temp + 38.0, day.temp.max));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="12" text-anchor="middle" fill="black">Lo: {:.0}°F</text>"#,
            x + column_width / 2.0, y_temp + 53.0, day.temp.min));
        svg.push('\n');

        // Humidity
        let y_hum = y_base + 165.0;
        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="14" text-anchor="middle" fill="black">Humidity: {}%</text>"#,
            x + column_width / 2.0, y_hum, day.humidity));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="12" text-anchor="middle" fill="gray">{}</text>"#,
            x + column_width / 2.0, y_hum + 18.0, humidity_text(day.humidity, day.temp.day)));
        svg.push('\n');

        // Wind
        let y_wind = y_base + 210.0;
        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="14" text-anchor="middle" fill="black">Wind: {:.0} mph</text>"#,
            x + column_width / 2.0, y_wind, day.wind_speed));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="12" text-anchor="middle" fill="gray">{}</text>"#,
            x + column_width / 2.0, y_wind + 18.0, wind_text(day.wind_speed)));
        svg.push('\n');

        // Sunrise/sunset
        let sunrise_time = Utc.timestamp_opt(day.sunrise, 0).unwrap();
        let sunset_time = Utc.timestamp_opt(day.sunset, 0).unwrap();

        let y_sun = y_base + 255.0;
        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="11" text-anchor="middle" fill="black">↑{}</text>"#,
            x + column_width / 2.0 - 25.0, y_sun, sunrise_time.format("%H:%M")));
        svg.push('\n');

        svg.push_str(&format!(r#"  <text x="{}" y="{}" font-family="Arial" font-size="11" text-anchor="middle" fill="black">↓{}</text>"#,
            x + column_width / 2.0 + 25.0, y_sun, sunset_time.format("%H:%M")));
        svg.push('\n');
    }

    svg.push_str("</svg>");
    svg
}
