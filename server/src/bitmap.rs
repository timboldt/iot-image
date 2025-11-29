//! E-ink display bitmap generator
//!
//! Generates raw bitmaps in the native format for GxEPD2 7-color displays
//! Format: EPBM header + raw pixel data (1 byte per pixel)

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
