//! E-ink display bitmap generator
//!
//! Generates raw bitmaps in the native format for GxEPD2 7-color displays
//! Format: EPBM header + raw pixel data (1 byte per pixel)

use std::fs;
use std::path::Path;

/// E-ink display color palette (GxEPD2 7-color)
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum EpdColor {
    Black = 0,
    White = 1,
    Green = 2,
    Blue = 3,
    Red = 4,
    Yellow = 5,
}

/// Raw bitmap buffer for e-ink display
pub struct EpdBitmap {
    width: u16,
    height: u16,
    data: Vec<u8>,
}

impl EpdBitmap {
    /// Create a new bitmap with given dimensions
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        Self {
            width,
            height,
            data: vec![EpdColor::White as u8; size],
        }
    }

    /// Set pixel at (x, y) to given color
    pub fn set_pixel(&mut self, x: u16, y: u16, color: EpdColor) {
        if x < self.width && y < self.height {
            let index = y as usize * self.width as usize + x as usize;
            self.data[index] = color as u8;
        }
    }

    /// Write bitmap to bytes with header
    /// Format: "EPBM" magic (4 bytes) + width (2 bytes) + height (2 bytes) + pixel data
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.data.len());

        // Magic number "EPBM" (E-Paper BitMap)
        bytes.extend_from_slice(b"EPBM");

        // Width (big-endian)
        bytes.extend_from_slice(&self.width.to_be_bytes());

        // Height (big-endian)
        bytes.extend_from_slice(&self.height.to_be_bytes());

        // Pixel data (1 byte per pixel)
        bytes.extend_from_slice(&self.data);

        bytes
    }
}

/// Generate a test pattern bitmap with color bars
pub fn generate_test_bitmap(width: u16, height: u16) -> EpdBitmap {
    let mut bitmap = EpdBitmap::new(width, height);

    // Color bars - each 1/6th of the width
    let bar_width = width / 6;
    let colors = [
        EpdColor::Black,
        EpdColor::White,
        EpdColor::Green,
        EpdColor::Blue,
        EpdColor::Red,
        EpdColor::Yellow,
    ];

    // Draw horizontal color bars in top half
    for (i, &color) in colors.iter().enumerate() {
        let x1 = i as u16 * bar_width;
        let x2 = ((i + 1) as u16 * bar_width).min(width);

        for y in 0..height / 2 {
            for x in x1..x2 {
                bitmap.set_pixel(x, y, color);
            }
        }
    }

    // White area in bottom half (for future text/graphics)
    for y in height / 2..height {
        for x in 0..width {
            bitmap.set_pixel(x, y, EpdColor::White);
        }
    }

    bitmap
}

/// Generate weather display bitmap
#[allow(dead_code)]
pub fn generate_weather_bitmap(width: u16, height: u16, _weather_data: &str) -> EpdBitmap {
    // For now, just generate a test pattern
    // TODO: Integrate with weather data and render forecast
    generate_test_bitmap(width, height)
}

/// Convert e-ink display color to approximate RGB values
fn epd_color_to_rgb(color: EpdColor) -> (u8, u8, u8) {
    match color {
        EpdColor::Black => (0, 0, 0),
        EpdColor::White => (255, 255, 255),
        EpdColor::Green => (0, 255, 0),
        EpdColor::Blue => (0, 0, 255),
        EpdColor::Red => (255, 0, 0),
        EpdColor::Yellow => (255, 255, 0),
    }
}

/// 4x4 Bayer matrix for ordered dithering
/// Values range from 0-15, normalized to 0.0-1.0
const BAYER_MATRIX_4X4: [[f32; 4]; 4] = [
    [0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0],
    [12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0],
    [3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0],
    [15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0],
];

/// Apply ordered dithering with variable ratio between two colors
/// Uses Bayer matrix to determine which color based on the ratio
fn ordered_dither_color(
    x: u16,
    y: u16,
    color1: EpdColor,
    color2: EpdColor,
    dist1: f32,
    dist2: f32,
) -> EpdColor {
    // Calculate ratio: how much should be color1 vs color2
    // If dist1 is small, we want mostly color1
    // If dist2 is small, we want mostly color2
    let total_dist = dist1 + dist2;
    if total_dist < 0.001 {
        return color1; // Avoid division by zero
    }

    // Ratio of color2 (0.0 = all color1, 1.0 = all color2)
    let color2_ratio = dist1 / total_dist;

    // Get Bayer threshold for this pixel position
    let bayer_x = (x % 4) as usize;
    let bayer_y = (y % 4) as usize;
    let threshold = BAYER_MATRIX_4X4[bayer_y][bayer_x];

    // If color2_ratio is higher than threshold, use color2
    if color2_ratio > threshold {
        color2
    } else {
        color1
    }
}

/// Map RGB color to nearest e-ink display color with ordered dithering
fn rgb_to_epd_color_dithered(r: u8, g: u8, b: u8, x: u16, y: u16) -> EpdColor {
    // Calculate distances to all available colors
    let colors = [
        EpdColor::Black,
        EpdColor::White,
        EpdColor::Green,
        EpdColor::Blue,
        EpdColor::Red,
        EpdColor::Yellow,
    ];

    let mut distances: Vec<(EpdColor, f32)> = colors
        .iter()
        .map(|&color| {
            let (cr, cg, cb) = epd_color_to_rgb(color);
            let dist = ((r as f32 - cr as f32).powi(2)
                + (g as f32 - cg as f32).powi(2)
                + (b as f32 - cb as f32).powi(2))
            .sqrt();
            (color, dist)
        })
        .collect();

    // Sort by distance
    distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    // Get two closest colors
    let color1 = distances[0].0;
    let color2 = distances[1].0;
    let dist1 = distances[0].1;
    let dist2 = distances[1].1;

    // Only skip dithering if the color is VERY close to a pure e-ink color
    // This keeps pure black, pure white, etc. crisp
    // Threshold of 15 is about 3.4% of max RGB distance (441)
    if dist1 < 15.0 {
        return color1;
    }

    // Use ordered dithering with variable ratio based on distances
    ordered_dither_color(x, y, color1, color2, dist1, dist2)
}

/// Convert e-ink bitmap back to PNG for debugging
pub fn bitmap_to_png(bitmap: &EpdBitmap) -> Result<Vec<u8>, String> {
    let mut pixmap = tiny_skia::Pixmap::new(bitmap.width as u32, bitmap.height as u32)
        .ok_or("Failed to create pixmap")?;

    // Convert each pixel back to RGB
    for y in 0..bitmap.height {
        for x in 0..bitmap.width {
            let index = y as usize * bitmap.width as usize + x as usize;
            let color_val = bitmap.data[index];

            // Map color value back to RGB
            let (r, g, b) = match color_val {
                0 => (0, 0, 0),       // Black
                1 => (255, 255, 255), // White
                2 => (0, 255, 0),     // Green
                3 => (0, 0, 255),     // Blue
                4 => (255, 0, 0),     // Red
                5 => (255, 255, 0),   // Yellow
                _ => (128, 128, 128), // Unknown - gray
            };

            let pixel = tiny_skia::ColorU8::from_rgba(r, g, b, 255);
            pixmap.pixels_mut()[y as usize * bitmap.width as usize + x as usize] =
                pixel.premultiply();
        }
    }

    pixmap
        .encode_png()
        .map_err(|e| format!("Failed to encode PNG: {}", e))
}

/// Render SVG file to e-ink bitmap
pub fn render_svg_to_bitmap(svg_path: &Path, width: u16, height: u16) -> Result<EpdBitmap, String> {
    // Read SVG file
    let svg_data = fs::read(svg_path).map_err(|e| format!("Failed to read SVG file: {}", e))?;

    // Parse SVG with font configuration
    let mut opts = usvg::Options::default();

    // Load system fonts
    opts.fontdb_mut().load_system_fonts();

    // Set generic font families as fallbacks
    opts.fontdb_mut().set_sans_serif_family("DejaVu Sans");
    opts.fontdb_mut().set_serif_family("DejaVu Serif");
    opts.fontdb_mut().set_monospace_family("DejaVu Sans Mono");

    println!("Loaded {} fonts from system", opts.fontdb_mut().len());

    let tree = usvg::Tree::from_data(&svg_data, &opts)
        .map_err(|e| format!("Failed to parse SVG: {}", e))?;

    // Create pixmap for rendering with white background
    let mut pixmap =
        tiny_skia::Pixmap::new(width as u32, height as u32).ok_or("Failed to create pixmap")?;

    // Fill with white background (SVG backgrounds are transparent by default)
    pixmap.fill(tiny_skia::Color::WHITE);

    // Calculate transform to fit SVG to target size
    let svg_size = tree.size();
    let scale_x = width as f32 / svg_size.width();
    let scale_y = height as f32 / svg_size.height();
    let scale = scale_x.min(scale_y);

    let transform = tiny_skia::Transform::from_scale(scale, scale);

    // Render SVG to pixmap
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Save debug PNG to see what was rendered
    if let Err(e) = pixmap.save_png("debug_render.png") {
        eprintln!("Warning: Could not save debug PNG: {}", e);
    }

    // Convert pixmap to e-ink bitmap with ordered dithering
    let mut bitmap = EpdBitmap::new(width, height);

    // Track color usage for debugging
    let mut color_counts = std::collections::HashMap::new();

    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap
                .pixel(x as u32, y as u32)
                .ok_or("Failed to get pixel")?;

            // Convert to e-ink color with ordered dithering
            let color = rgb_to_epd_color_dithered(pixel.red(), pixel.green(), pixel.blue(), x, y);

            *color_counts.entry(color as u8).or_insert(0) += 1;
            bitmap.set_pixel(x, y, color);
        }
    }

    // Print color statistics
    println!("Color usage in converted bitmap:");
    for (color_val, count) in color_counts.iter() {
        let color_name = match color_val {
            0 => "Black",
            1 => "White",
            2 => "Green",
            3 => "Blue",
            4 => "Red",
            5 => "Yellow",
            _ => "Unknown",
        };
        println!(
            "  {}: {} pixels ({:.2}%)",
            color_name,
            count,
            (*count as f32 / (width as usize * height as usize) as f32) * 100.0
        );
    }

    Ok(bitmap)
}
