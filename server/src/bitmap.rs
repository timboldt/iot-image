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

/// Map RGB color to nearest e-ink display color
fn rgb_to_epd_color(r: u8, g: u8, b: u8) -> EpdColor {
    // Calculate luminance for black/white determination
    let luminance = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as u8;

    // Very dark = black
    if luminance < 32 {
        return EpdColor::Black;
    }

    // Check for specific color patterns before checking if it's too light
    let max_channel = r.max(g).max(b);
    let min_channel = r.min(g).min(b);
    let saturation = if max_channel > 0 {
        ((max_channel - min_channel) as f32 / max_channel as f32) * 100.0
    } else {
        0.0
    };

    // Low saturation means grayscale - use black or white based on luminance
    if saturation < 20.0 {
        return if luminance < 128 {
            EpdColor::Black
        } else {
            EpdColor::White
        };
    }

    // High saturation - determine which color it is
    // Check for yellow first (high R and G, low B)
    if r > 200 && g > 200 && b < 100 {
        return EpdColor::Yellow;
    }

    // Check for red (high R, low G and B)
    if r > 200 && g < 150 && b < 150 {
        return EpdColor::Red;
    }

    // Check for green (high G, low R and B)
    if g > 200 && r < 150 && b < 150 {
        return EpdColor::Green;
    }

    // Check for blue (high B, low R and G)
    if b > 200 && r < 150 && g < 150 {
        return EpdColor::Blue;
    }

    // For colors that don't match cleanly, use channel dominance
    if r >= g && r >= b {
        // Red dominant
        if g > b + 50 {
            EpdColor::Yellow // Red-ish with green = yellow
        } else {
            EpdColor::Red
        }
    } else if g >= r && g >= b {
        // Green dominant
        if r > b + 50 {
            EpdColor::Yellow // Green-ish with red = yellow
        } else {
            EpdColor::Green
        }
    } else if b >= r && b >= g {
        EpdColor::Blue
    } else {
        // Fallback based on luminance
        if luminance < 128 {
            EpdColor::Black
        } else {
            EpdColor::White
        }
    }
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

    // Convert pixmap to e-ink bitmap
    let mut bitmap = EpdBitmap::new(width, height);

    // Track color usage for debugging
    let mut color_counts = std::collections::HashMap::new();

    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap
                .pixel(x as u32, y as u32)
                .ok_or("Failed to get pixel")?;

            let color = rgb_to_epd_color(pixel.red(), pixel.green(), pixel.blue());

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
