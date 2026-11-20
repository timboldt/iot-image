mod air_quality;
mod bitmap;
mod fred;
mod kalman;
mod stocks;
mod svg_common;
mod weather;
mod weight;

use air_quality::fetch_air_quality;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bitmap::EpdColor;
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
    fetch_weather, generate_weather_mini_svg, generate_weather_svg, generate_weather_week_svg,
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

/// A panel the server renders for: the bitmap filename it requests, its size,
/// and the colours it can show.
struct DisplayProfile {
    suffix: &'static str,
    width: u16,
    height: u16,
    palette: &'static [EpdColor],
    dither: bool,
}

/// The reTerminal e1002: 7.3" Spectra 6, 800x480.
const SEED_E1002: DisplayProfile = DisplayProfile {
    suffix: "seed-e1002.bin",
    width: 800,
    height: 480,
    palette: bitmap::SIX_COLOR,
    dither: true,
};

/// The 2.9" white/black/red FeatherWing, 296x128. Small text does not survive
/// dithering at this resolution, so pixels snap to the nearest colour.
const FEATHER_TRICOLOR: DisplayProfile = DisplayProfile {
    suffix: "tricolor-296.bin",
    width: 296,
    height: 128,
    palette: bitmap::TRI_COLOR,
    dither: false,
};

/// Every endpoint, served at `/<base>/<suffix>`, `/<base>/svg` and `/<base>/png`.
const ENDPOINTS: [(&str, &DisplayProfile); 7] = [
    ("weather", &SEED_E1002),
    ("weather-overview", &SEED_E1002),
    ("stocks", &SEED_E1002),
    ("fred", &SEED_E1002),
    ("weight/forecast", &SEED_E1002),
    ("weight/velocity", &SEED_E1002),
    ("weather-mini", &FEATHER_TRICOLOR),
];

fn render(svg: &str, display: &DisplayProfile) -> Result<bitmap::EpdBitmap, String> {
    bitmap::render_svg_to_bitmap(
        svg.as_bytes(),
        display.width,
        display.height,
        display.palette,
        display.dither,
    )
}

fn test_bitmap(display: &DisplayProfile) -> bitmap::EpdBitmap {
    bitmap::generate_test_bitmap(display.width, display.height, display.palette)
}

/// The bitmap for `svg`, or the colour-bar test pattern if it cannot be built.
fn render_or_test_pattern(svg: SvgResult, display: &DisplayProfile) -> bitmap::EpdBitmap {
    match svg.and_then(|svg| render(&svg, display)) {
        Ok(bitmap) => bitmap,
        Err(e) => {
            eprintln!("Error: {}", e);
            test_bitmap(display)
        }
    }
}

fn error_svg(e: impl Display, display: &DisplayProfile) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">
            <text x="{x}" y="{y}" text-anchor="middle" font-size="{size}">Error: {e}</text>
        </svg>"#,
        w = display.width,
        h = display.height,
        x = display.width / 2,
        y = display.height / 2,
        size = if display.width < 400 { 12 } else { 20 },
        e = svg_common::escape_xml(&e.to_string()),
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

fn bitmap_response(svg: SvgResult, display: &DisplayProfile) -> Response {
    let bytes = render_or_test_pattern(svg, display).to_bytes();
    ([("Content-Type", "application/octet-stream")], bytes).into_response()
}

fn svg_response(svg: SvgResult, display: &DisplayProfile) -> Response {
    let body = match svg {
        Ok(svg) => svg,
        Err(e) => error_svg(e, display),
    };
    ([("Content-Type", "image/svg+xml")], body).into_response()
}

/// The device bitmap as a PNG, so the quantised/dithered result can be checked
/// in a browser. Errors are rendered as text, as `/svg` does.
fn png_response(svg: SvgResult, display: &DisplayProfile) -> Response {
    let svg = svg.or_else(|e| Ok(error_svg(e, display)));
    match render_or_test_pattern(svg, display).to_png() {
        Ok(png) => ([("Content-Type", "image/png")], png).into_response(),
        Err(e) => svg_response(Err(e), display),
    }
}

/// Encodes an endpoint's SVG (or its error) as one response format.
type Responder = fn(SvgResult, &DisplayProfile) -> Response;

/// Registers the `.bin`, `/svg` and `/png` variants of one endpoint, which
/// differ only in how the same SVG is encoded for the response.
fn register<F, Fut>(
    router: Router<Arc<AppState>>,
    base: &str,
    display: &'static DisplayProfile,
    producer: F,
) -> Router<Arc<AppState>>
where
    F: Fn(Arc<AppState>, QueryArgs) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = SvgResult> + Send,
{
    let variants: [(&str, Responder); 3] = [
        (display.suffix, bitmap_response),
        ("svg", svg_response),
        ("png", png_response),
    ];
    variants
        .into_iter()
        .fold(router, |router, (suffix, respond)| {
            let producer = producer.clone();
            router.route(
                &format!("/{}/{}", base, suffix),
                get(
                    move |State(state): State<Arc<AppState>>, Query(query): Query<QueryArgs>| {
                        let producer = producer.clone();
                        async move { respond(producer(state, query).await, display) }
                    },
                ),
            )
        })
}

async fn weather_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let (lat, lon) = weather_coordinates(&state, &query);
    fetch_weather(lat, lon, &state.api_key)
        .await
        .map(|weather| generate_weather_svg(&weather, query.battery_pct, &state.geocoder))
        .map_err(|e| format!("fetching weather: {}", e))
}

async fn weather_mini_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let (lat, lon) = weather_coordinates(&state, &query);
    fetch_weather(lat, lon, &state.api_key)
        .await
        .map(|weather| generate_weather_mini_svg(&weather, query.battery_pct, &state.geocoder))
        .map_err(|e| format!("fetching weather: {}", e))
}

/// The week-ahead page. Served at `weather-overview`, the URL the device's
/// left button already requests, which once showed OpenWeatherMap's text
/// overview. Air quality is optional: the page renders without it.
async fn weather_week_endpoint(state: Arc<AppState>, query: QueryArgs) -> SvgResult {
    let (lat, lon) = weather_coordinates(&state, &query);
    let key = &state.api_key;
    // Errors become Strings inside each future so that neither holds a
    // non-Send error across the join.
    let (weather, air) = tokio::join!(
        async {
            fetch_weather(lat, lon, key)
                .await
                .map_err(|e| format!("fetching weather: {}", e))
        },
        async { fetch_air_quality(lat, lon, key).await.ok() },
    );
    Ok(generate_weather_week_svg(
        &weather?,
        air.as_ref(),
        query.battery_pct,
        &state.geocoder,
    ))
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
    for (base, display) in ENDPOINTS {
        println!(
            "    - http://localhost:{}/{}/{} ({}x{}, {} colors)",
            args.port,
            base,
            display.suffix,
            display.width,
            display.height,
            display.palette.len()
        );
    }
    println!("  SVG Preview:");
    for (base, _) in ENDPOINTS {
        println!("    - http://localhost:{}/{}/svg", args.port, base);
    }
    println!("  PNG Preview (as dithered for the panel):");
    for (base, _) in ENDPOINTS {
        println!("    - http://localhost:{}/{}/png", args.port, base);
    }
    println!("Format: Raw e-ink bitmap (EPBM)");
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
    app = register(app, "weather", &SEED_E1002, weather_endpoint);
    app = register(app, "weather-overview", &SEED_E1002, weather_week_endpoint);
    app = register(app, "stocks", &SEED_E1002, stocks_endpoint);
    app = register(app, "fred", &SEED_E1002, fred_endpoint);
    app = register(
        app,
        "weight/forecast",
        &SEED_E1002,
        weight_forecast_endpoint,
    );
    app = register(
        app,
        "weight/velocity",
        &SEED_E1002,
        weight_velocity_endpoint,
    );
    app = register(
        app,
        "weather-mini",
        &FEATHER_TRICOLOR,
        weather_mini_endpoint,
    );
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
