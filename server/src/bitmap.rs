//! E-ink display bitmap generator
//!
//! Generates raw bitmaps in the native format for GxEPD2 7-color displays
//! Format: EPBM header + raw pixel data (1 byte per pixel)

use std::fs;
use std::path::Path;

/// Approximate RGB values for E Ink Spectra 6 pigments
const PALETTE: [[u8; 3]; 6] = [
    [0, 0, 0],       // Black
    [255, 255, 255], // White
    [0, 255, 0],     // Green
    [0, 0, 255],     // Blue
    [255, 0, 0],     // Red (Spectra Red is fairly pure)
    [255, 255, 0],   // Yellow
];

/// Pre-computed CIELAB values for the palette (for faster color matching)
/// Computed from the RGB values above using D65 illuminant
const PALETTE_LAB: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],         // Black: L=0
    [100.0, 0.0, 0.0],       // White: L=100
    [87.73, -86.18, 83.18],  // Green
    [32.30, 79.19, -107.86], // Blue
    [53.24, 80.09, 67.20],   // Red
    [97.14, -21.55, 94.48],  // Yellow
];

/// E-ink display color palette (E Ink Spectra 6)
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
#[allow(dead_code)]
fn epd_color_to_rgb(color: EpdColor) -> (u8, u8, u8) {
    let idx = color as usize;
    (PALETTE[idx][0], PALETTE[idx][1], PALETTE[idx][2])
}

/// Convert sRGB (0-255) to linear RGB (0.0-1.0)
/// Applies inverse gamma correction
fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert linear RGB to XYZ color space (D65 illuminant)
fn rgb_to_xyz(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r_lin = srgb_to_linear(r);
    let g_lin = srgb_to_linear(g);
    let b_lin = srgb_to_linear(b);

    // sRGB to XYZ matrix (D65)
    let x = r_lin * 0.4124564 + g_lin * 0.3575761 + b_lin * 0.1804375;
    let y = r_lin * 0.2126729 + g_lin * 0.7151522 + b_lin * 0.0721750;
    let z = r_lin * 0.0193339 + g_lin * 0.119_192 + b_lin * 0.9503041;

    (x * 100.0, y * 100.0, z * 100.0) // Scale to 0-100 range
}

/// XYZ to CIELAB conversion helper function
fn xyz_to_lab_component(t: f32) -> f32 {
    const DELTA: f32 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// Convert XYZ to CIELAB color space (D65 illuminant)
/// Returns (L*, a*, b*) where L* is lightness (0-100), a* and b* are color components
fn xyz_to_lab(x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    // D65 reference white point
    const XN: f32 = 95.047;
    const YN: f32 = 100.000;
    const ZN: f32 = 108.883;

    let fx = xyz_to_lab_component(x / XN);
    let fy = xyz_to_lab_component(y / YN);
    let fz = xyz_to_lab_component(z / ZN);

    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);

    (l, a, b)
}

/// Convert RGB to CIELAB color space
fn rgb_to_lab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (x, y, z) = rgb_to_xyz(r, g, b);
    xyz_to_lab(x, y, z)
}

/// Calculate perceptual color difference in CIELAB space (Delta E)
fn delta_e(l1: f32, a1: f32, b1: f32, l2: f32, a2: f32, b2: f32) -> f32 {
    let dl = l1 - l2;
    let da = a1 - a2;
    let db = b1 - b2;
    (dl * dl + da * da + db * db).sqrt()
}

/// Convert RGB to HSL color space
/// Returns (hue [0-360], saturation [0-1], lightness [0-1])
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    // Lightness
    let l = (max + min) / 2.0;

    // Saturation
    let s = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * l - 1.0).abs())
    };

    // Hue
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    (h, s, l)
}

/// Convert HSL to RGB color space
/// Takes (hue [0-360], saturation [0-1], lightness [0-1])
/// Returns (r, g, b) in [0-255]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// Boost saturation to compensate for e-paper's less vivid pigments
/// Increases saturation by the given factor (e.g., 1.2 for 20% boost)
/// This helps the dithering algorithm favor primary colors over black/white
fn boost_saturation(r: u8, g: u8, b: u8, boost_factor: f32) -> (u8, u8, u8) {
    let (h, s, l) = rgb_to_hsl(r, g, b);

    // Boost saturation, clamping to [0, 1]
    let s_boosted = (s * boost_factor).min(1.0);

    hsl_to_rgb(h, s_boosted, l)
}

/// Atkinson dithering error diffusion pattern
/// Diffuses only 6/8 (75%) of error to neighbors, absorbing the rest
/// Pattern (fractions of error distributed):
///       X   1/8 1/8
///   1/8 1/8 1/8
///       1/8
const ATKINSON_KERNEL: [(i32, i32, f32); 6] = [
    (1, 0, 1.0 / 8.0),  // Right
    (2, 0, 1.0 / 8.0),  // Right + 1
    (-1, 1, 1.0 / 8.0), // Below left
    (0, 1, 1.0 / 8.0),  // Below
    (1, 1, 1.0 / 8.0),  // Below right
    (0, 2, 1.0 / 8.0),  // Below + 1
];

/// Map RGB color to nearest e-ink display color using perceptual color matching
/// This function finds the closest color without dithering (used by error diffusion)
fn rgb_to_epd_color(r: u8, g: u8, b: u8) -> EpdColor {
    // Pre-process: Boost saturation by 20% to compensate for e-paper's less vivid pigments
    let (r, g, b) = boost_saturation(r, g, b, 1.2);

    // Convert input RGB to CIELAB
    let (l, a, b) = rgb_to_lab(r, g, b);

    // Calculate perceptual distances to all palette colors
    let colors = [
        EpdColor::Black,
        EpdColor::White,
        EpdColor::Green,
        EpdColor::Blue,
        EpdColor::Red,
        EpdColor::Yellow,
    ];

    colors
        .iter()
        .enumerate()
        .map(|(idx, &color)| {
            let lab = PALETTE_LAB[idx];
            let dist = delta_e(l, a, b, lab[0], lab[1], lab[2]);
            (color, dist)
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap()
        .0
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

            // Map color value back to RGB using palette
            let (r, g, b) = if (color_val as usize) < PALETTE.len() {
                let c = &PALETTE[color_val as usize];
                (c[0], c[1], c[2])
            } else {
                (128, 128, 128) // Unknown - gray
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

/// Render SVG file to e-ink bitmap using Atkinson error diffusion dithering
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

    // Convert pixmap to e-ink bitmap with Atkinson error diffusion dithering
    let mut bitmap = EpdBitmap::new(width, height);

    // Create error buffer for Atkinson dithering (stores RGB error values)
    let mut errors = vec![vec![(0.0f32, 0.0f32, 0.0f32); width as usize]; height as usize];

    // Track color usage for debugging
    let mut color_counts = std::collections::HashMap::new();

    // Process pixels in raster order for error diffusion
    for y in 0..height {
        for x in 0..width {
            let pixel = pixmap
                .pixel(x as u32, y as u32)
                .ok_or("Failed to get pixel")?;

            // Get accumulated error from previous pixels
            let (err_r, err_g, err_b) = errors[y as usize][x as usize];

            // Add error to pixel value
            let r = (pixel.red() as f32 + err_r).clamp(0.0, 255.0) as u8;
            let g = (pixel.green() as f32 + err_g).clamp(0.0, 255.0) as u8;
            let b = (pixel.blue() as f32 + err_b).clamp(0.0, 255.0) as u8;

            // Find closest e-ink color
            let color = rgb_to_epd_color(r, g, b);

            // Get the actual RGB values of the chosen color
            let (cr, cg, cb) = epd_color_to_rgb(color);

            // Calculate quantization error
            let quant_err_r = r as f32 - cr as f32;
            let quant_err_g = g as f32 - cg as f32;
            let quant_err_b = b as f32 - cb as f32;

            // Distribute error to neighboring pixels using Atkinson kernel
            for &(dx, dy, weight) in &ATKINSON_KERNEL {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;

                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let err = &mut errors[ny as usize][nx as usize];
                    err.0 += quant_err_r * weight;
                    err.1 += quant_err_g * weight;
                    err.2 += quant_err_b * weight;
                }
            }

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
