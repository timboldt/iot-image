mod bitmap;
mod weather;

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use chrono::prelude::*;
use clap::Parser;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use weather::fetch_weather;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate weather images for IoT devices")]
struct Args {
    /// Latitude (e.g. 37.7749)
    #[arg(long)]
    lat: Option<String>,
    /// Longitude (e.g. -122.4194)
    #[arg(long)]
    lon: Option<String>,
    /// OpenWeather API key
    #[arg(long)]
    open_weather_key: Option<String>,
    /// HTTP server port
    #[arg(long, default_value = "8080")]
    port: u16,
    /// Enable HTTP server mode
    #[arg(long, default_value = "false")]
    serve: bool,
    /// SVG file path to render as bitmap
    #[arg(long)]
    svg: Option<String>,
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

#[derive(Clone)]
struct AppState {
    svg_path: Option<String>,
}

async fn get_bitmap(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Generate bitmap for e-ink display (800x480, 7 colors)
    let bitmap = if let Some(svg_path) = &state.svg_path {
        match bitmap::render_svg_to_bitmap(Path::new(svg_path), 800, 480) {
            Ok(bmp) => bmp,
            Err(e) => {
                eprintln!("Error rendering SVG: {}", e);
                bitmap::generate_test_bitmap(800, 480)
            }
        }
    } else {
        bitmap::generate_test_bitmap(800, 480)
    };

    let bytes = bitmap.to_bytes();

    ([("Content-Type", "application/octet-stream")], bytes)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.serve {
        // HTTP server mode
        println!("\n=== iot-image Server Starting ===");
        println!("Serving e-ink bitmaps on port {}", args.port);
        println!(
            "Endpoint: http://localhost:{}/weather/seed-e1002.bin",
            args.port
        );
        println!("Format: Raw e-ink bitmap (EPBM)");
        println!("Display: 800x480, 7 colors");

        if let Some(ref svg_path) = args.svg {
            println!("SVG input: {}\n", svg_path);
        } else {
            println!("Using test pattern (no SVG specified)\n");
        }

        let state = Arc::new(AppState {
            svg_path: args.svg.clone(),
        });

        let app = Router::new()
            .route("/weather/seed-e1002.bin", get(get_bitmap))
            .with_state(state);
        let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        println!("Server listening on http://{}", addr);

        axum::serve(listener, app).await.unwrap();
    } else {
        // Weather data fetching mode (original functionality)
        let lat = args.lat.expect("--lat required for weather mode");
        let lon = args.lon.expect("--lon required for weather mode");
        let key = args
            .open_weather_key
            .expect("--open-weather-key required for weather mode");

        match fetch_weather(&lat, &lon, &key).await {
            Ok(weather) => {
                println!("\n=== Weather Forecast ===");
                println!("Location: ({:.2}, {:.2})", weather.lat, weather.lon);
                println!("Timezone offset: {} seconds\n", weather.timezone_offset);

                // Display current weather
                println!("Current Conditions:");
                println!("  Temperature: {:.1}°F", weather.current.temp);
                println!("  Humidity: {}%", weather.current.humidity);
                if let Some(w) = weather.current.weather.first() {
                    println!("  Condition: {} ({})", w.main, w.description);
                }

                // Display 5-day forecast
                println!("\n5-Day Forecast:");
                for (day_idx, day) in weather.daily.iter().take(6).enumerate() {
                    let day_time = Utc.timestamp_opt(day.dt, 0).unwrap();
                    println!(
                        "\nDay {}: {} ({})",
                        day_idx,
                        day_time.format("%Y-%m-%d"),
                        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                            [day_time.weekday() as usize]
                    );

                    println!(
                        "  Morning temp: {:.1}°F ({})",
                        day.temp.morn,
                        temperature_text(day.temp.morn)
                    );
                    println!(
                        "  Day temp: {:.1}°F ({})",
                        day.temp.day,
                        temperature_text(day.temp.day)
                    );
                    println!(
                        "  Night temp: {:.1}°F ({})",
                        day.temp.night,
                        temperature_text(day.temp.night)
                    );
                    println!("  Min/Max: {:.1}°F - {:.1}°F", day.temp.min, day.temp.max);
                    println!(
                        "  Humidity: {}% ({})",
                        day.humidity,
                        humidity_text(day.humidity, day.temp.day)
                    );
                    println!(
                        "  Wind: {:.1} mph ({})",
                        day.wind_speed,
                        wind_text(day.wind_speed)
                    );

                    let sunrise_time = Utc.timestamp_opt(day.sunrise, 0).unwrap();
                    let sunset_time = Utc.timestamp_opt(day.sunset, 0).unwrap();
                    println!("  Sunrise: {}", sunrise_time.format("%H:%M"));
                    println!("  Sunset: {}", sunset_time.format("%H:%M"));

                    if let Some(w) = day.weather.first() {
                        println!("  Condition: {} ({})", w.main, w.description);
                        println!("  Icon: {}", w.icon);
                    }
                }
            }
            Err(e) => {
                eprintln!("Could not fetch weather data: {}", e);
                std::process::exit(1);
            }
        }
    }
}
