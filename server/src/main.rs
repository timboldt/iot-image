mod bitmap;
mod weather;

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use clap::Parser;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use weather::{fetch_weather, generate_weather_svg};

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate weather images for IoT devices")]
struct Args {
    /// Latitude (e.g. 37.7749)
    #[arg(long)]
    lat: String,
    /// Longitude (e.g. -122.4194)
    #[arg(long)]
    lon: String,
    /// OpenWeather API key
    #[arg(long)]
    open_weather_key: String,
    /// HTTP server port
    #[arg(long, default_value = "8080")]
    port: u16,
    /// Enable HTTP server mode
    #[arg(long, default_value = "false")]
    serve: bool,
}

#[derive(Clone)]
struct AppState {
    lat: String,
    lon: String,
    api_key: String,
}

async fn get_bitmap(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Fetch weather data and generate SVG
    let bitmap = match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_svg(&weather);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/weather.svg";
            match std::fs::write(temp_svg_path, &svg_content) {
                Ok(_) => {
                    // Render SVG to bitmap
                    match bitmap::render_svg_to_bitmap(Path::new(temp_svg_path), 800, 480) {
                        Ok(bmp) => bmp,
                        Err(e) => {
                            eprintln!("Error rendering SVG: {}", e);
                            bitmap::generate_test_bitmap(800, 480)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error writing SVG file: {}", e);
                    bitmap::generate_test_bitmap(800, 480)
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching weather: {}", e);
            bitmap::generate_test_bitmap(800, 480)
        }
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
        println!("Weather location: ({}, {})\n", args.lat, args.lon);

        let state = Arc::new(AppState {
            lat: args.lat.clone(),
            lon: args.lon.clone(),
            api_key: args.open_weather_key.clone(),
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
        match fetch_weather(&args.lat, &args.lon, &args.open_weather_key).await {
            Ok(weather) => {
                println!("\n=== Weather Forecast ===");
                println!("Location: ({:.2}, {:.2})", weather.lat, weather.lon);
                println!("Timezone offset: {} seconds\n", weather.timezone_offset);

                let svg_content = generate_weather_svg(&weather);

                let output_path = "weather.svg";
                match std::fs::write(output_path, svg_content) {
                    Ok(_) => println!("Weather SVG generated: {}", output_path),
                    Err(e) => eprintln!("Failed to write SVG file: {}", e),
                }
            }
            Err(e) => {
                eprintln!("Could not fetch weather data: {}", e);
                std::process::exit(1);
            }
        }
    }
}
