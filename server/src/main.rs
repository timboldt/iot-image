mod bitmap;
mod fred;
mod kalman;
mod stocks;
mod svg_common;
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
use reverse_geocoder::ReverseGeocoder;
use serde::Deserialize;
use std::fmt::Display;
use std::future::Future;
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

struct AppState {
    lat: String,
    lon: String,
    api_key: String,
    stocks_api_key: String,
    stock_symbols: String,
    fred_api_key: String,
    weight_data_dir: String,
    /// Built once at startup: constructing it loads and indexes the city dataset,
    /// which is too expensive to redo on every weather request.
    geocoder: ReverseGeocoder,
}

#[derive(Deserialize)]
struct QueryArgs {
    battery_pct: Option<u8>,
    date: Option<String>,    // Optional end date in YYYYMMDD format
    duration: Option<usize>, // Optional duration in days
    user: Option<String>,    // User for weight data (defaults to "weight")
    lat: Option<String>,
    lon: Option<String>,
}

const DISPLAY_WIDTH: u16 = 800;
const DISPLAY_HEIGHT: u16 = 480;

/// Filename the client requests for the raw bitmap form of every endpoint.
const BITMAP_SUFFIX: &str = "seed-e1002.bin";

/// Every endpoint is served at both `/<base>/seed-e1002.bin` and `/<base>/svg`.
const ENDPOINT_BASES: [&str; 6] = [
    "weather",
    "weather-overview",
    "stocks",
    "fred",
    "weight/forecast",
    "weight/velocity",
];

fn render_svg_bytes(svg_content: String) -> Vec<u8> {
    let bitmap =
        match bitmap::render_svg_to_bitmap(svg_content.as_bytes(), DISPLAY_WIDTH, DISPLAY_HEIGHT) {
            Ok(bmp) => bmp,
            Err(e) => {
                eprintln!("Error rendering SVG: {}", e);
                bitmap::generate_test_bitmap(DISPLAY_WIDTH, DISPLAY_HEIGHT)
            }
        };

    bitmap.to_bytes()
}

fn fallback_bitmap_bytes(error: &str) -> Vec<u8> {
    eprintln!("Error {}", error);
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

fn weight_csv_path(state: &AppState, user: Option<&str>) -> Result<String, String> {
    let user = user.unwrap_or("weight");
    // Reject any path separators or traversal sequences to prevent directory traversal.
    if user.contains('/') || user.contains('\\') || user.contains("..") {
        return Err(format!("Invalid user parameter: {}", user));
    }
    Ok(format!("{}/{}.csv", state.weight_data_dir, user))
}

fn weather_coordinates<'a>(state: &'a AppState, query: &'a QueryArgs) -> (&'a str, &'a str) {
    (
        query.lat.as_deref().unwrap_or(&state.lat),
        query.lon.as_deref().unwrap_or(&state.lon),
    )
}

/// The SVG for one endpoint, or a message describing why it could not be built.
type SvgResult = Result<String, String>;

fn bitmap_response(svg: SvgResult) -> impl IntoResponse {
    let bytes = match svg {
        Ok(svg) => render_svg_bytes(svg),
        Err(e) => fallback_bitmap_bytes(&e),
    };
    ([("Content-Type", "application/octet-stream")], bytes)
}

fn svg_response(svg: SvgResult) -> impl IntoResponse {
    let body = match svg {
        Ok(svg) => svg,
        Err(e) => error_svg(e),
    };
    ([("Content-Type", "image/svg+xml")], body)
}

/// Registers the `.bin` and `/svg` variants of one endpoint, which differ only
/// in how the same SVG is encoded for the response.
fn register<F, Fut>(router: Router<Arc<AppState>>, base: &str, producer: F) -> Router<Arc<AppState>>
where
    F: Fn(Arc<AppState>, QueryArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = SvgResult> + Send,
{
    let for_bitmap = producer.clone();
    router
        .route(
            &format!("/{}/{}", base, BITMAP_SUFFIX),
            get(
                move |State(state): State<Arc<AppState>>, Query(query): Query<QueryArgs>| {
                    let producer = for_bitmap.clone();
                    async move { bitmap_response(producer(state, query).await) }
                },
            ),
        )
        .route(
            &format!("/{}/svg", base),
            get(
                move |State(state): State<Arc<AppState>>, Query(query): Query<QueryArgs>| {
                    let producer = producer.clone();
                    async move { svg_response(producer(state, query).await) }
                },
            ),
        )
}

async fn weather_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let (lat, lon) = weather_coordinates(&state, &query);
    fetch_weather(lat, lon, &state.api_key)
        .await
        .map(|weather| generate_weather_svg(&weather, query.battery_pct, &state.geocoder))
        .map_err(|e| format!("fetching weather: {}", e))
}

async fn weather_overview_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let (lat, lon) = weather_coordinates(&state, &query);
    fetch_weather_overview(lat, lon, &state.api_key)
        .await
        .map(|weather| {
            generate_weather_overview_svg(&weather, query.battery_pct, &state.geocoder)
        })
        .map_err(|e| format!("fetching weather overview: {}", e))
}

async fn stocks_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    fetch_stocks(&state.stocks_api_key, &state.stock_symbols)
        .await
        .map(|stocks| generate_stocks_svg(&stocks, query.battery_pct))
        .map_err(|e| format!("fetching stocks: {}", e))
}

async fn fred_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    fetch_fred(&state.fred_api_key, query.date.as_deref(), query.duration)
        .await
        .map(|fred| generate_fred_svg(&fred, query.battery_pct))
        .map_err(|e| format!("fetching FRED data: {}", e))
}

async fn weight_forecast_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let csv_path = weight_csv_path(&state, query.user.as_deref())?;
    fetch_weight_data(Path::new(&csv_path))
        .await
        .map(|data| generate_forecast_svg(&data, query.battery_pct))
        .map_err(|e| format!("fetching weight data: {}", e))
}

async fn weight_velocity_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let csv_path = weight_csv_path(&state, query.user.as_deref())?;
    fetch_weight_data(Path::new(&csv_path))
        .await
        .map(|data| generate_velocity_svg(&data, query.battery_pct))
        .map_err(|e| format!("fetching weight data: {}", e))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    println!("\n=== iot-image Server Starting ===");
    println!("Serving e-ink bitmaps on port {}", args.port);
    println!("Endpoints:");
    println!("  Binary (EPBM):");
    for base in ENDPOINT_BASES {
        println!(
            "    - http://localhost:{}/{}/{}",
            args.port, base, BITMAP_SUFFIX
        );
    }
    println!("  SVG Preview:");
    for base in ENDPOINT_BASES {
        println!("    - http://localhost:{}/{}/svg", args.port, base);
    }
    println!("Format: Raw e-ink bitmap (EPBM)");
    println!("Display: {}x{}, 6 colors", DISPLAY_WIDTH, DISPLAY_HEIGHT);
    println!("Weather location: ({}, {})\n", args.lat, args.lon);

    let port = args.port;
    let state = Arc::new(AppState {
        lat: args.lat,
        lon: args.lon,
        api_key: args.open_weather_key,
        stocks_api_key: args.stocks_api_key,
        stock_symbols: args.stock_symbols,
        fred_api_key: args.fred_api_key,
        weight_data_dir: args.weight_data_dir,
        geocoder: ReverseGeocoder::new(),
    });

    let mut app = Router::new();
    app = register(app, "weather", weather_endpoint);
    app = register(app, "weather-overview", weather_overview_endpoint);
    app = register(app, "stocks", stocks_endpoint);
    app = register(app, "fred", fred_endpoint);
    app = register(app, "weight/forecast", weight_forecast_endpoint);
    app = register(app, "weight/velocity", weight_velocity_endpoint);
    let app = app.with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
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

#[cfg(test)]
mod tests {
    use super::{weather_coordinates, AppState, QueryArgs};
    use reverse_geocoder::ReverseGeocoder;

    fn test_state() -> AppState {
        AppState {
            lat: "37.7749".to_string(),
            lon: "-122.4194".to_string(),
            api_key: "weather-key".to_string(),
            stocks_api_key: "stocks-key".to_string(),
            stock_symbols: "QQQ".to_string(),
            fred_api_key: "fred-key".to_string(),
            weight_data_dir: "/tmp".to_string(),
            geocoder: ReverseGeocoder::new(),
        }
    }

    #[test]
    fn weather_coordinates_default_to_state() {
        let state = test_state();
        let query = QueryArgs {
            battery_pct: None,
            date: None,
            duration: None,
            user: None,
            lat: None,
            lon: None,
        };

        assert_eq!(
            weather_coordinates(&state, &query),
            ("37.7749", "-122.4194")
        );
    }

    #[test]
    fn weather_coordinates_use_query_overrides() {
        let state = test_state();
        let query = QueryArgs {
            battery_pct: None,
            date: None,
            duration: None,
            user: None,
            lat: Some("40.7128".to_string()),
            lon: Some("-74.0060".to_string()),
        };

        assert_eq!(weather_coordinates(&state, &query), ("40.7128", "-74.0060"));
    }
}
