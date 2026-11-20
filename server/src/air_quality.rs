//! Air quality from OpenWeatherMap's air-pollution forecast, reported on the
//! US EPA AQI scale.
//!
//! OpenWeatherMap's own `aqi` field is a 1-5 European-style index that does
//! not match the numbers US readers know, so the AQI is computed here from the
//! pollutant concentrations instead.

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct AirForecast {
    /// Hourly samples, starting at the current hour and running ~4 days.
    pub list: Vec<AirSample>,
}

#[derive(Deserialize, Debug)]
pub struct AirSample {
    pub dt: i64,
    pub components: Components,
}

/// Concentrations in µg/m³.
#[derive(Deserialize, Debug)]
pub struct Components {
    #[serde(default)]
    pub pm2_5: f32,
    #[serde(default)]
    pub pm10: f32,
    #[serde(default)]
    pub o3: f32,
}

pub async fn fetch_air_quality(
    lat: &str,
    lon: &str,
    key: &str,
) -> Result<AirForecast, Box<dyn std::error::Error>> {
    let url = format!(
        "https://api.openweathermap.org/data/2.5/air_pollution/forecast?lat={}&lon={}&appid={}",
        lat, lon, key
    );
    let forecast = reqwest::Client::new()
        .get(&url)
        .send()
        .await?
        .json::<AirForecast>()
        .await?;
    Ok(forecast)
}

/// EPA breakpoints: (concentration low, high, AQI low, high).
type Breakpoints = [(f32, f32, f32, f32)];

/// PM2.5 in µg/m³ (the 2024 revision of the scale).
const PM25: &Breakpoints = &[
    (0.0, 9.0, 0.0, 50.0),
    (9.1, 35.4, 51.0, 100.0),
    (35.5, 55.4, 101.0, 150.0),
    (55.5, 125.4, 151.0, 200.0),
    (125.5, 225.4, 201.0, 300.0),
    (225.5, 325.4, 301.0, 500.0),
];

/// PM10 in µg/m³.
const PM10: &Breakpoints = &[
    (0.0, 54.0, 0.0, 50.0),
    (55.0, 154.0, 51.0, 100.0),
    (155.0, 254.0, 101.0, 150.0),
    (255.0, 354.0, 151.0, 200.0),
    (355.0, 424.0, 201.0, 300.0),
    (425.0, 604.0, 301.0, 500.0),
];

/// Ozone in ppb (the 8-hour table, which tops out at "very unhealthy").
const O3_PPB: &Breakpoints = &[
    (0.0, 54.0, 0.0, 50.0),
    (55.0, 70.0, 51.0, 100.0),
    (71.0, 85.0, 101.0, 150.0),
    (86.0, 105.0, 151.0, 200.0),
    (106.0, 200.0, 201.0, 300.0),
];

/// µg/m³ of ozone per ppb at 25°C.
const O3_UG_PER_PPB: f32 = 1.96;

/// Linear interpolation within the breakpoint band holding `c`. Values in the
/// small gaps between bands (EPA truncates readings to the table's precision)
/// take the higher band; values past the table cap at its top.
fn sub_index(c: f32, table: &Breakpoints) -> f32 {
    for &(c_lo, c_hi, i_lo, i_hi) in table {
        if c <= c_hi {
            let c = c.max(c_lo);
            return i_lo + (i_hi - i_lo) * (c - c_lo) / (c_hi - c_lo);
        }
    }
    table.last().map_or(0.0, |b| b.3)
}

/// The US AQI for one sample and the pollutant that sets it. These are hourly
/// concentrations rather than the 24-hour (particulates) and 8-hour (ozone)
/// averages the EPA uses, so this is a "right now" reading, as AirNow's
/// NowCast and PurpleAir also give.
pub fn us_aqi(c: &Components) -> (u16, &'static str) {
    let (aqi, pollutant) = [
        (sub_index(c.pm2_5, PM25), "PM2.5"),
        (sub_index(c.pm10, PM10), "PM10"),
        (sub_index(c.o3 / O3_UG_PER_PPB, O3_PPB), "ozone"),
    ]
    .into_iter()
    // Strictly greater, so a tie (typically all zero) goes to PM2.5.
    .fold(
        (0.0, "PM2.5"),
        |best, next| if next.0 > best.0 { next } else { best },
    );
    (aqi.round() as u16, pollutant)
}

/// The EPA category name, chip fill, and the text colour that reads on it.
pub fn aqi_category(aqi: u16) -> (&'static str, &'static str, &'static str) {
    match aqi {
        // Pure green: SVG's "green" is #008000, which only dithers.
        0..=50 => ("Good", "#00ff00", "black"),
        51..=100 => ("Moderate", "yellow", "black"),
        101..=150 => ("Unhealthy for some", "orange", "black"),
        151..=200 => ("Unhealthy", "red", "white"),
        201..=300 => ("Very unhealthy", "purple", "white"),
        _ => ("Hazardous", "black", "white"),
    }
}

#[cfg(test)]
mod tests {
    use super::{aqi_category, us_aqi, Components};

    fn c(pm2_5: f32, pm10: f32, o3: f32) -> Components {
        Components { pm2_5, pm10, o3 }
    }

    #[test]
    fn aqi_matches_epa_breakpoints() {
        assert_eq!(us_aqi(&c(0.0, 0.0, 0.0)), (0, "PM2.5"));
        assert_eq!(us_aqi(&c(9.0, 0.0, 0.0)), (50, "PM2.5"));
        assert_eq!(us_aqi(&c(35.4, 0.0, 0.0)), (100, "PM2.5"));
        // Wildfire smoke.
        assert_eq!(us_aqi(&c(150.0, 0.0, 0.0)).0, 225);
        assert_eq!(us_aqi(&c(1000.0, 0.0, 0.0)).0, 500);
    }

    #[test]
    fn the_worst_pollutant_sets_the_aqi() {
        // 90.8 µg/m³ ozone is ~46 ppb: AQI 43, above the clean particulates.
        assert_eq!(us_aqi(&c(0.65, 2.79, 90.79)), (43, "ozone"));
        assert_eq!(us_aqi(&c(3.0, 200.0, 20.0)).1, "PM10");
    }

    #[test]
    fn categories_change_at_the_band_edges() {
        assert_eq!(aqi_category(50).0, "Good");
        assert_eq!(aqi_category(51).0, "Moderate");
        assert_eq!(aqi_category(151).0, "Unhealthy");
        assert_eq!(aqi_category(301).0, "Hazardous");
    }
}
