use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct TimeSeriesDaily {
    #[serde(rename = "Time Series (Daily)")]
    pub time_series: HashMap<String, DailyData>,
}

#[derive(Debug, Deserialize)]
pub struct DailyData {
    #[serde(rename = "1. open")]
    pub open: String,
    #[serde(rename = "2. high")]
    pub high: String,
    #[serde(rename = "3. low")]
    pub low: String,
    #[serde(rename = "4. close")]
    pub close: String,
}

#[derive(Debug, Deserialize)]
pub struct DigitalCurrencyDaily {
    #[serde(rename = "Time Series (Digital Currency Daily)")]
    pub time_series: HashMap<String, DigitalDailyData>,
}

#[derive(Debug, Deserialize)]
pub struct DigitalDailyData {
    #[serde(rename = "1. open")]
    pub open_usd: String,
    #[serde(rename = "2. high")]
    pub high_usd: String,
    #[serde(rename = "3. low")]
    pub low_usd: String,
    #[serde(rename = "4. close")]
    pub close_usd: String,
}

#[derive(Debug)]
pub struct StockPoint {
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

#[derive(Debug)]
pub struct StockData {
    pub symbol: String,
    pub points: Vec<StockPoint>,
}

#[derive(Debug)]
pub struct StocksData {
    pub stocks: Vec<StockData>,
}

pub async fn fetch_stocks(api_key: &str) -> Result<StocksData, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut stocks = Vec::new();

    // Fetch Bitcoin (BTC)
    let btc_url = format!(
        "https://www.alphavantage.co/query?function=DIGITAL_CURRENCY_DAILY&symbol=BTC&market=USD&apikey={}",
        api_key
    );
    let btc_text = client.get(&btc_url).send().await?.text().await?;
    eprintln!(
        "BTC API Response (first 500 chars): {}",
        &btc_text.chars().take(500).collect::<String>()
    );

    let btc_response: DigitalCurrencyDaily = serde_json::from_str(&btc_text).map_err(|e| {
        format!(
            "Failed to parse BTC response: {}. Response: {}",
            e,
            &btc_text.chars().take(200).collect::<String>()
        )
    })?;
    let btc_points = parse_digital_currency_data(btc_response);
    stocks.push(StockData {
        symbol: "BTC".to_string(),
        points: btc_points,
    });

    // Fetch regular stocks
    let symbols = vec!["QQQ", "IONQ", "TSLA"];
    for symbol in symbols {
        let url = format!(
            "https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol={}&apikey={}",
            symbol, api_key
        );
        let text = client.get(&url).send().await?.text().await?;
        eprintln!(
            "{} API Response (first 500 chars): {}",
            symbol,
            &text.chars().take(500).collect::<String>()
        );

        let response: TimeSeriesDaily = serde_json::from_str(&text).map_err(|e| {
            format!(
                "Failed to parse {} response: {}. Response: {}",
                symbol,
                e,
                &text.chars().take(200).collect::<String>()
            )
        })?;
        let points = parse_time_series_data(response);
        stocks.push(StockData {
            symbol: symbol.to_string(),
            points,
        });
    }

    Ok(StocksData { stocks })
}

fn parse_time_series_data(data: TimeSeriesDaily) -> Vec<StockPoint> {
    let mut points: Vec<StockPoint> = data
        .time_series
        .iter()
        .filter_map(|(date, daily)| {
            let open = daily.open.parse::<f64>().ok()?;
            let high = daily.high.parse::<f64>().ok()?;
            let low = daily.low.parse::<f64>().ok()?;
            let close = daily.close.parse::<f64>().ok()?;
            Some(StockPoint {
                date: date.clone(),
                open,
                high,
                low,
                close,
            })
        })
        .collect();

    points.sort_by(|a, b| b.date.cmp(&a.date)); // Sort descending (newest first)
    points.truncate(60); // Last 60 days
    points.reverse(); // Reverse to ascending order for charting
    points
}

fn parse_digital_currency_data(data: DigitalCurrencyDaily) -> Vec<StockPoint> {
    let mut points: Vec<StockPoint> = data
        .time_series
        .iter()
        .filter_map(|(date, daily)| {
            let open = daily.open_usd.parse::<f64>().ok()?;
            let high = daily.high_usd.parse::<f64>().ok()?;
            let low = daily.low_usd.parse::<f64>().ok()?;
            let close = daily.close_usd.parse::<f64>().ok()?;
            Some(StockPoint {
                date: date.clone(),
                open,
                high,
                low,
                close,
            })
        })
        .collect();

    points.sort_by(|a, b| b.date.cmp(&a.date)); // Sort descending (newest first)
    points.truncate(60); // Last 60 days
    points.reverse(); // Reverse to ascending order for charting
    points
}

pub fn generate_stocks_svg(stocks: &StocksData) -> String {
    let width = 800;
    let height = 480;
    let mut svg = String::new();

    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg">"#,
        width, height
    ));

    // Background
    svg.push_str(&format!(
        r#"<rect width="{}" height="{}" fill="white"/>"#,
        width, height
    ));

    // Title
    svg.push_str(r#"<text x="400" y="30" text-anchor="middle" font-size="24" font-weight="bold" fill="black">Stock Charts</text>"#);

    // Create 2x2 grid of charts
    let chart_width = 380;
    let chart_height = 200;
    let positions = [
        (10, 50),   // Top-left
        (410, 50),  // Top-right
        (10, 270),  // Bottom-left
        (410, 270), // Bottom-right
    ];

    for (i, stock) in stocks.stocks.iter().enumerate() {
        if i >= positions.len() {
            break;
        }
        let (x, y) = positions[i];
        svg.push_str(&generate_chart_svg(stock, x, y, chart_width, chart_height));
    }

    svg.push_str("</svg>");
    svg
}

fn generate_chart_svg(stock: &StockData, x: i32, y: i32, width: i32, height: i32) -> String {
    let mut svg = String::new();

    // Chart border
    svg.push_str(&format!(
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>"#,
        x, y, width, height
    ));

    // Title
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="middle" font-size="18" font-weight="bold" fill="black">{}</text>"#,
        x + width / 2,
        y + 20,
        stock.symbol
    ));

    if stock.points.is_empty() {
        return svg;
    }

    // Find min and max prices for scaling (use high/low from candlesticks)
    let min_price = stock
        .points
        .iter()
        .map(|p| p.low)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    let max_price = stock
        .points
        .iter()
        .map(|p| p.high)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(100.0);

    let price_range = if max_price > min_price {
        max_price - min_price
    } else {
        1.0
    };

    // Chart area (leave space for title and labels)
    let chart_x = x + 40;
    let chart_y = y + 30;
    let chart_w = width - 50;
    let chart_h = height - 60;

    // Draw grid lines
    for i in 0..5 {
        let grid_y = chart_y + (chart_h * i) / 4;
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#cccccc\" stroke-width=\"1\"/>",
            chart_x,
            grid_y,
            chart_x + chart_w,
            grid_y
        ));
    }

    // Draw candlesticks
    let num_points = stock.points.len();
    if num_points > 0 {
        let candle_width = if num_points > 1 {
            (chart_w as f64 / num_points as f64 * 0.7).max(1.0) as i32
        } else {
            10
        };

        for (i, point) in stock.points.iter().enumerate() {
            let px = chart_x + (chart_w * i as i32) / num_points.max(1) as i32 + candle_width / 2;

            // Calculate y positions
            let high_y = chart_y + chart_h
                - ((point.high - min_price) / price_range * chart_h as f64) as i32;
            let low_y =
                chart_y + chart_h - ((point.low - min_price) / price_range * chart_h as f64) as i32;
            let open_y = chart_y + chart_h
                - ((point.open - min_price) / price_range * chart_h as f64) as i32;
            let close_y = chart_y + chart_h
                - ((point.close - min_price) / price_range * chart_h as f64) as i32;

            // Determine candle color (green if close >= open, red otherwise)
            let is_bullish = point.close >= point.open;
            let color = if is_bullish { "#00AA00" } else { "#CC0000" };

            // Draw high-low line (wick)
            svg.push_str(&format!(
                r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="1"/>"#,
                px, high_y, px, low_y, color
            ));

            // Draw open-close rectangle (body)
            let body_top = open_y.min(close_y);
            let body_height = (open_y - close_y).abs().max(1);
            svg.push_str(&format!(
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
                px - candle_width / 2, body_top, candle_width, body_height, color, color
            ));
        }
    }

    // Display current price and change
    if let (Some(first), Some(last)) = (stock.points.first(), stock.points.last()) {
        let change = last.close - first.close;
        let change_pct = (change / first.close) * 100.0;
        let change_sign = if change >= 0.0 { "+" } else { "" };

        svg.push_str(&format!(
            r#"<text x="{}" y="{}" font-size="14" fill="black">${:.2}</text>"#,
            x + 5,
            y + height - 20,
            last.close
        ));

        let change_color = if change >= 0.0 { "green" } else { "red" };
        svg.push_str(&format!(
            r#"<text x="{}" y="{}" font-size="12" fill="{}">{}{:.2} ({}{:.1}%)</text>"#,
            x + 5,
            y + height - 5,
            change_color,
            change_sign,
            change,
            change_sign,
            change_pct
        ));
    }

    // Y-axis labels (min and max)
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">${:.0}</text>"#,
        chart_x - 5,
        chart_y + 5,
        max_price
    ));
    svg.push_str(&format!(
        r#"<text x="{}" y="{}" text-anchor="end" font-size="10" fill="black">${:.0}</text>"#,
        chart_x - 5,
        chart_y + chart_h,
        min_price
    ));

    svg
}
