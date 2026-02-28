mod bitmap;
mod fred;
mod stocks;
mod weather;
mod weight;

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use fred::{fetch_fred, generate_fred_svg};
use serde::Deserialize;
use std::fmt::Display;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use stocks::{fetch_stocks, generate_stocks_svg};
use weather::{
    fetch_weather, fetch_weather_overview, generate_weather_overview_svg, generate_weather_svg,
};
use weight::{fetch_weight_data, generate_forecast_svg, generate_velocity_svg};

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
    /// Directory containing weight data CSV files
    #[arg(long)]
    weight_data_dir: String,
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
    weight_data_dir: String,
}

#[derive(Deserialize)]
struct QueryArgs {
    battery_pct: Option<u8>,
    date: Option<String>,    // Optional end date in YYYYMMDD format
    duration: Option<usize>, // Optional duration in days
    user: Option<String>,    // User for weight data (defaults to "weight")
}

const DISPLAY_WIDTH: u16 = 800;
const DISPLAY_HEIGHT: u16 = 480;

fn render_svg_bytes(svg_content: String, temp_svg_path: &str) -> Vec<u8> {
    let bitmap = match std::fs::write(temp_svg_path, &svg_content) {
        Ok(_) => match bitmap::render_svg_to_bitmap(
            Path::new(temp_svg_path),
            DISPLAY_WIDTH,
            DISPLAY_HEIGHT,
        ) {
            Ok(bmp) => bmp,
            Err(e) => {
                eprintln!("Error rendering SVG: {}", e);
                bitmap::generate_test_bitmap(DISPLAY_WIDTH, DISPLAY_HEIGHT)
            }
        },
        Err(e) => {
            eprintln!("Error writing SVG file: {}", e);
            bitmap::generate_test_bitmap(DISPLAY_WIDTH, DISPLAY_HEIGHT)
        }
    };

    bitmap.to_bytes()
}

fn fallback_bitmap_bytes(error_context: &str, e: impl Display) -> Vec<u8> {
    eprintln!("Error {}: {}", error_context, e);
    bitmap::generate_test_bitmap(DISPLAY_WIDTH, DISPLAY_HEIGHT).to_bytes()
}

fn error_svg(e: impl Display) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
            <text x="400" y="240" text-anchor="middle" font-size="20">Error: {}</text>
        </svg>"#,
        e
    )
}

fn weight_csv_path(state: &AppState, user: Option<&str>) -> String {
    format!("{}/{}.csv", state.weight_data_dir, user.unwrap_or("weight"))
}

async fn get_weather_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let bitmap = match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => render_svg_bytes(
            generate_weather_svg(&weather, query.battery_pct),
            "/tmp/weather.svg",
        ),
        Err(e) => fallback_bitmap_bytes("fetching weather", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_stocks_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let bitmap = match fetch_stocks(&state.stocks_api_key, &state.stock_symbols).await {
        Ok(stocks) => render_svg_bytes(
            generate_stocks_svg(&stocks, query.battery_pct),
            "/tmp/stocks.svg",
        ),
        Err(e) => fallback_bitmap_bytes("fetching stocks", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_weather_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    match fetch_weather(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_svg(&weather, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

async fn get_weather_overview_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let bitmap = match fetch_weather_overview(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => render_svg_bytes(
            generate_weather_overview_svg(&weather, query.battery_pct),
            "/tmp/weather_overview.svg",
        ),
        Err(e) => fallback_bitmap_bytes("fetching weather overview", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_weather_overview_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    match fetch_weather_overview(&state.lat, &state.lon, &state.api_key).await {
        Ok(weather) => {
            let svg_content = generate_weather_overview_svg(&weather, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

async fn get_stocks_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    match fetch_stocks(&state.stocks_api_key, &state.stock_symbols).await {
        Ok(stocks) => {
            let svg_content = generate_stocks_svg(&stocks, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

async fn get_fred_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let bitmap = match fetch_fred(&state.fred_api_key, query.date.as_deref(), query.duration).await
    {
        Ok(fred) => render_svg_bytes(generate_fred_svg(&fred, query.battery_pct), "/tmp/fred.svg"),
        Err(e) => fallback_bitmap_bytes("fetching FRED data", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_fred_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    match fetch_fred(&state.fred_api_key, query.date.as_deref(), query.duration).await {
        Ok(fred) => {
            let svg_content = generate_fred_svg(&fred, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

async fn get_weight_forecast_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let csv_path = weight_csv_path(&state, query.user.as_deref());
    let bitmap = match fetch_weight_data(Path::new(&csv_path)).await {
        Ok(data) => render_svg_bytes(
            generate_forecast_svg(&data, query.battery_pct),
            "/tmp/weight_forecast.svg",
        ),
        Err(e) => fallback_bitmap_bytes("fetching weight data", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_weight_forecast_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let csv_path = weight_csv_path(&state, query.user.as_deref());
    match fetch_weight_data(Path::new(&csv_path)).await {
        Ok(data) => {
            let svg_content = generate_forecast_svg(&data, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

async fn get_weight_velocity_bitmap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let csv_path = weight_csv_path(&state, query.user.as_deref());
    let bitmap = match fetch_weight_data(Path::new(&csv_path)).await {
        Ok(data) => render_svg_bytes(
            generate_velocity_svg(&data, query.battery_pct),
            "/tmp/weight_velocity.svg",
        ),
        Err(e) => fallback_bitmap_bytes("fetching weight data", e),
    };

    ([("Content-Type", "application/octet-stream")], bitmap)
}

async fn get_weight_velocity_svg(
    State(state): State<Arc<AppState>>,
    Query(query): Query<QueryArgs>,
) -> impl IntoResponse {
    let csv_path = weight_csv_path(&state, query.user.as_deref());
    match fetch_weight_data(Path::new(&csv_path)).await {
        Ok(data) => {
            let svg_content = generate_velocity_svg(&data, query.battery_pct);
            ([("Content-Type", "image/svg+xml")], svg_content)
        }
        Err(e) => ([("Content-Type", "image/svg+xml")], error_svg(e)),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // HTTP server mode
    println!("\n=== iot-image Server Starting ===");
    println!("Serving e-ink bitmaps on port {}", args.port);
    println!(
        "Endpoints:\n  Binary (EPBM):\n    - http://localhost:{}/weather/seed-e1002.bin\n    - http://localhost:{}/weather-overview/seed-e1002.bin\n    - http://localhost:{}/stocks/seed-e1002.bin\n    - http://localhost:{}/fred/seed-e1002.bin\n    - http://localhost:{}/weight/forecast/seed-e1002.bin\n    - http://localhost:{}/weight/velocity/seed-e1002.bin\n  SVG Preview:\n    - http://localhost:{}/weather/svg\n    - http://localhost:{}/weather-overview/svg\n    - http://localhost:{}/stocks/svg\n    - http://localhost:{}/fred/svg\n    - http://localhost:{}/weight/forecast/svg\n    - http://localhost:{}/weight/velocity/svg",
        args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port, args.port
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
        weight_data_dir: args.weight_data_dir.clone(),
    });

    let app = Router::new()
        .route("/weather/seed-e1002.bin", get(get_weather_bitmap))
        .route(
            "/weather-overview/seed-e1002.bin",
            get(get_weather_overview_bitmap),
        )
        .route("/stocks/seed-e1002.bin", get(get_stocks_bitmap))
        .route("/fred/seed-e1002.bin", get(get_fred_bitmap))
        .route(
            "/weight/forecast/seed-e1002.bin",
            get(get_weight_forecast_bitmap),
        )
        .route(
            "/weight/velocity/seed-e1002.bin",
            get(get_weight_velocity_bitmap),
        )
        .route("/weather/svg", get(get_weather_svg))
        .route("/weather-overview/svg", get(get_weather_overview_svg))
        .route("/stocks/svg", get(get_stocks_svg))
        .route("/fred/svg", get(get_fred_svg))
        .route("/weight/forecast/svg", get(get_weight_forecast_svg))
        .route("/weight/velocity/svg", get(get_weight_velocity_svg))
        .with_state(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Failed to bind server listener: {}", e);
            return;
        }
    };

    println!("Server listening on http://{}", addr);

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
    }
}
