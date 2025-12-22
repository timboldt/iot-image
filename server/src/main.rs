mod bitmap;
mod fred;
mod stocks;
mod weather;

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use fred::{fetch_fred, generate_fred_svg};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use stocks::{fetch_stocks, generate_stocks_svg};
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
    /// Twelve Data API key
    #[arg(long)]
    stocks_api_key: String,
    /// Stock symbols (comma-separated, e.g. "BTC/USD,QQQ,IONQ,TSLA")
    #[arg(long)]
    stock_symbols: String,
    /// FRED API key
    #[arg(long)]
    fred_api_key: String,
    /// HTTP server port
    #[arg(long, default_value = "8080")]
    port: u16,
}

#[derive(Clone)]
struct AppState {
    lat: String,
    lon: String,
    api_key: String,
    stocks_api_key: String,
    stock_symbols: String,
    fred_api_key: String,
}

#[derive(Deserialize)]
struct QueryArgs {
    battery_pct: Option<u8>,
    show_alerts: Option<bool>,
}

async fn get_weather_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch weather data and generate SVG
    let bitmap = match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_svg(&weather, query.battery_pct, query.show_alerts);

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

async fn get_stocks_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch stocks data and generate SVG
    let bitmap = match fetch_stocks(&state.stocks_api_key, &state.stock_symbols).await {
        Ok(stocks) => {
            let svg_content = generate_stocks_svg(&stocks, query.battery_pct);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/stocks.svg";
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
            eprintln!("Error fetching stocks: {}", e);
            bitmap::generate_test_bitmap(800, 480)
        }
    };

    let bytes = bitmap.to_bytes();

    ([("Content-Type", "application/octet-stream")], bytes)
}

async fn get_weather_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch weather data and generate SVG
    match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_svg(&weather, query.battery_pct, query.show_alerts);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => {
            let error_svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
                    <text x="400" y="240" text-anchor="middle" font-size="20">Error: {}</text>
                </svg>"#,
                e
            );
            ([("Content-Type", "image/svg+xml")], error_svg)
        }
    }
}

async fn get_stocks_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch stocks data and generate SVG
    match fetch_stocks(&state.stocks_api_key, &state.stock_symbols).await {
        Ok(stocks) => {
            let svg_content = generate_stocks_svg(&stocks, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => {
            let error_svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
                    <text x="400" y="240" text-anchor="middle" font-size="20">Error: {}</text>
                </svg>"#,
                e
            );
            ([("Content-Type", "image/svg+xml")], error_svg)
        }
    }
}

async fn get_weather_debug(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch weather data, render to bitmap with dithering, and return as PNG
    match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_svg(&weather, query.battery_pct, query.show_alerts);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/weather_debug.svg";
            match std::fs::write(temp_svg_path, &svg_content) {
                Ok(_) => {
                    // Render SVG to bitmap with dithering
                    match bitmap::render_svg_to_bitmap(Path::new(temp_svg_path), 800, 480) {
                        Ok(bmp) => {
                            // Convert bitmap back to PNG for visual inspection
                            match bitmap::bitmap_to_png(&bmp) {
                                Ok(png_bytes) => ([("Content-Type", "image/png")], png_bytes),
                                Err(e) => {
                                    eprintln!("Error converting to PNG: {}", e);
                                    let error_png = vec![];
                                    ([("Content-Type", "image/png")], error_png)
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error rendering SVG: {}", e);
                            let error_png = vec![];
                            ([("Content-Type", "image/png")], error_png)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error writing SVG file: {}", e);
                    let error_png = vec![];
                    ([("Content-Type", "image/png")], error_png)
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching weather: {}", e);
            let error_png = vec![];
            ([("Content-Type", "image/png")], error_png)
        }
    }
}

async fn get_stocks_debug(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch stocks data, render to bitmap with dithering, and return as PNG
    match fetch_stocks(&state.stocks_api_key, &state.stock_symbols).await {
        Ok(stocks) => {
            let svg_content = generate_stocks_svg(&stocks, query.battery_pct);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/stocks_debug.svg";
            match std::fs::write(temp_svg_path, &svg_content) {
                Ok(_) => {
                    // Render SVG to bitmap with dithering
                    match bitmap::render_svg_to_bitmap(Path::new(temp_svg_path), 800, 480) {
                        Ok(bmp) => {
                            // Convert bitmap back to PNG for visual inspection
                            match bitmap::bitmap_to_png(&bmp) {
                                Ok(png_bytes) => ([("Content-Type", "image/png")], png_bytes),
                                Err(e) => {
                                    eprintln!("Error converting to PNG: {}", e);
                                    let error_png = vec![];
                                    ([("Content-Type", "image/png")], error_png)
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error rendering SVG: {}", e);
                            let error_png = vec![];
                            ([("Content-Type", "image/png")], error_png)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error writing SVG file: {}", e);
                    let error_png = vec![];
                    ([("Content-Type", "image/png")], error_png)
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching stocks: {}", e);
            let error_png = vec![];
            ([("Content-Type", "image/png")], error_png)
        }
    }
}

async fn get_fred_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch FRED data and generate SVG
    let bitmap = match fetch_fred(&state.fred_api_key).await {
        Ok(fred) => {
            let svg_content = generate_fred_svg(&fred, query.battery_pct);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/fred.svg";
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
            eprintln!("Error fetching FRED data: {}", e);
            bitmap::generate_test_bitmap(800, 480)
        }
    };

    let bytes = bitmap.to_bytes();

    ([("Content-Type", "application/octet-stream")], bytes)
}

async fn get_fred_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch FRED data and generate SVG
    match fetch_fred(&state.fred_api_key).await {
        Ok(fred) => {
            let svg_content = generate_fred_svg(&fred, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => {
            let error_svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
                    <text x="400" y="240" text-anchor="middle" font-size="20">Error: {}</text>
                </svg>"#,
                e
            );
            ([("Content-Type", "image/svg+xml")], error_svg)
        }
    }
}

async fn get_fred_debug(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    // Fetch FRED data, render to bitmap with dithering, and return as PNG
    match fetch_fred(&state.fred_api_key).await {
        Ok(fred) => {
            let svg_content = generate_fred_svg(&fred, query.battery_pct);

            // Write SVG to temporary file
            let temp_svg_path = "/tmp/fred_debug.svg";
            match std::fs::write(temp_svg_path, &svg_content) {
                Ok(_) => {
                    // Render SVG to bitmap with dithering
                    match bitmap::render_svg_to_bitmap(Path::new(temp_svg_path), 800, 480) {
                        Ok(bmp) => {
                            // Convert bitmap back to PNG for visual inspection
                            match bitmap::bitmap_to_png(&bmp) {
                                Ok(png_bytes) => ([("Content-Type", "image/png")], png_bytes),
                                Err(e) => {
                                    eprintln!("Error converting to PNG: {}", e);
                                    let error_png = vec![];
                                    ([("Content-Type", "image/png")], error_png)
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error rendering SVG: {}", e);
                            let error_png = vec![];
                            ([("Content-Type", "image/png")], error_png)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error writing SVG file: {}", e);
                    let error_png = vec![];
                    ([("Content-Type", "image/png")], error_png)
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching FRED data: {}", e);
            let error_png = vec![];
            ([("Content-Type", "image/png")], error_png)
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // HTTP server mode
    println!("\n=== iot-image Server Starting ===");
    println!("Serving e-ink bitmaps on port {}", args.port);
    println!(
        "Endpoints:\n  Binary (EPBM):\n    - http://localhost:{}/weather/seed-e1002.bin\n    - http://localhost:{}/stocks/seed-e1002.bin\n    - http://localhost:{}/fred/seed-e1002.bin\n  SVG Preview:\n    - http://localhost:{}/weather/svg\n    - http://localhost:{}/stocks/svg\n    - http://localhost:{}/fred/svg\n  Debug (Dithered PNG):\n    - http://localhost:{}/weather/debug\n    - http://localhost:{}/stocks/debug\n    - http://localhost:{}/fred/debug",
        args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port
    );
    println!("Format: Raw e-ink bitmap (EPBM)");
    println!("Display: 800x480, 7 colors");
    println!("Weather location: ({}, {})\n", args.lat, args.lon);

    let state = Arc::new(AppState {
        lat: args.lat.clone(),
        lon: args.lon.clone(),
        api_key: args.open_weather_key.clone(),
        stocks_api_key: args.stocks_api_key.clone(),
        stock_symbols: args.stock_symbols.clone(),
        fred_api_key: args.fred_api_key.clone(),
    });

    let app = Router::new()
        .route("/weather/seed-e1002.bin", get(get_weather_bitmap))
        .route("/stocks/seed-e1002.bin", get(get_stocks_bitmap))
        .route("/fred/seed-e1002.bin", get(get_fred_bitmap))
        .route("/weather/svg", get(get_weather_svg))
        .route("/stocks/svg", get(get_stocks_svg))
        .route("/fred/svg", get(get_fred_svg))
        .route("/weather/debug", get(get_weather_debug))
        .route("/stocks/debug", get(get_stocks_debug))
        .route("/fred/debug", get(get_fred_debug))
        .with_state(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("Server listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap();
}
