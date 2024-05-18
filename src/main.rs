use std::{
    borrow::BorrowMut,
    fs::File,
    io::{BufWriter, Write},
};

use chrono::offset::{Local, TimeZone};
use chrono::{Date, Duration};
use image::{
    codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding},
    GrayImage, ImageResult,
};
use plotters::prelude::*;
use plotters_bitmap::BitMapBackend;

extern crate image;
extern crate num_complex;

fn plot() -> ImageResult<()> {
    const IMG_X: u32 = 400;
    const IMG_Y: u32 = 300;

    let mut plot_buf: [u8; (IMG_X * IMG_Y * 3) as usize] = [0; (IMG_X * IMG_Y * 3) as usize];

    {
        let data = get_data();
        let (to_date, from_date) = (
            parse_time(data[0].0) + Duration::days(1),
            parse_time(data[29].0) - Duration::days(1),
        );

        let root = BitMapBackend::with_buffer(&mut plot_buf, (IMG_X, IMG_Y)).into_drawing_area();
        root.fill(&WHITE).unwrap();

        let (to_date, from_date) = (
            parse_time(data[0].0) + Duration::days(1),
            parse_time(data[29].0) - Duration::days(1),
        );

        let mut chart = ChartBuilder::on(&root)
            .margin_bottom(10)
            .x_label_area_size(0)
            .y_label_area_size(40)
            .caption("MSFT Stock Price", ("sans-serif", 20.0).into_font())
            .build_cartesian_2d(from_date..to_date, 110f32..135f32)
            .unwrap();

        chart
            .configure_mesh()
            .light_line_style(WHITE)
            .draw()
            .unwrap();

        chart
            .draw_series(data.iter().map(|x| {
                CandleStick::new(parse_time(x.0), x.1, x.2, x.3, x.4, GREEN.filled(), RED, 7)
            }))
            .unwrap();
    }

    {
        let plot_img = image::RgbImage::from_raw(IMG_X, IMG_Y, plot_buf.to_vec()).unwrap();
        plot_img.save("plot.png").unwrap();
    }

    {
        let plot_img = image::RgbImage::from_raw(IMG_X, IMG_Y, plot_buf.to_vec()).unwrap();
        let mut imgbuf = GrayImage::new(IMG_X, IMG_Y);

        for x in 0..IMG_X {
            for y in 0..IMG_Y {
                let src = plot_img.get_pixel_checked(x, y).unwrap();
                let luma = (src.0[0] / 3 + src.0[1] / 3 + src.0[2] / 3);
                // if luma != 0 && luma != 75 {
                //     println!("{}", luma);
                // }
                imgbuf.get_pixel_mut_checked(x, y).unwrap().0[0] = if luma > 128 { 255 } else { 0 };
            }
        }

        let f = File::create("plot.pbm")?;
        let mut writer = BufWriter::new(f);
        let encoder =
            PnmEncoder::new(&mut writer).with_subtype(PnmSubtype::Bitmap(SampleEncoding::Binary));
        imgbuf.write_with_encoder(encoder)?;
        writer.flush()?;
    }

    Ok(())
}

fn parse_time(t: &str) -> Date<Local> {
    Local
        .datetime_from_str(&format!("{} 0:0", t), "%Y-%m-%d %H:%M")
        .unwrap()
        .date()
}

fn get_data() -> Vec<(&'static str, f32, f32, f32, f32)> {
    vec![
        ("2019-04-25", 130.06, 131.37, 128.83, 129.15),
        ("2019-04-24", 125.79, 125.85, 124.52, 125.01),
        ("2019-04-23", 124.1, 125.58, 123.83, 125.44),
        ("2019-04-22", 122.62, 124.0000, 122.57, 123.76),
        ("2019-04-18", 122.19, 123.52, 121.3018, 123.37),
        ("2019-04-17", 121.24, 121.85, 120.54, 121.77),
        ("2019-04-16", 121.64, 121.65, 120.1, 120.77),
        ("2019-04-15", 120.94, 121.58, 120.57, 121.05),
        ("2019-04-12", 120.64, 120.98, 120.37, 120.95),
        ("2019-04-11", 120.54, 120.85, 119.92, 120.33),
        ("2019-04-10", 119.76, 120.35, 119.54, 120.19),
        ("2019-04-09", 118.63, 119.54, 118.58, 119.28),
        ("2019-04-08", 119.81, 120.02, 118.64, 119.93),
        ("2019-04-05", 119.39, 120.23, 119.37, 119.89),
        ("2019-04-04", 120.1, 120.23, 118.38, 119.36),
        ("2019-04-03", 119.86, 120.43, 119.15, 119.97),
        ("2019-04-02", 119.06, 119.48, 118.52, 119.19),
        ("2019-04-01", 118.95, 119.1085, 118.1, 119.02),
        ("2019-03-29", 118.07, 118.32, 116.96, 117.94),
        ("2019-03-28", 117.44, 117.58, 116.13, 116.93),
        ("2019-03-27", 117.875, 118.21, 115.5215, 116.77),
        ("2019-03-26", 118.62, 118.705, 116.85, 117.91),
        ("2019-03-25", 116.56, 118.01, 116.3224, 117.66),
        ("2019-03-22", 119.5, 119.59, 117.04, 117.05),
        ("2019-03-21", 117.135, 120.82, 117.09, 120.22),
        ("2019-03-20", 117.39, 118.75, 116.71, 117.52),
        ("2019-03-19", 118.09, 118.44, 116.99, 117.65),
        ("2019-03-18", 116.17, 117.61, 116.05, 117.57),
        ("2019-03-15", 115.34, 117.25, 114.59, 115.91),
        ("2019-03-14", 114.54, 115.2, 114.33, 114.59),
    ]
}

fn pic() -> ImageResult<()> {
    let imgx = 400;
    let imgy = 300;

    let scalex = 3.0 / imgx as f32;
    let scaley = 3.0 / imgy as f32;

    let mut imgbuf = GrayImage::new(imgx, imgy);
    for x in 0..imgx {
        for y in 0..imgy {
            let cx = y as f32 * scalex - 1.5;
            let cy = x as f32 * scaley - 1.5;

            let c = num_complex::Complex::new(-0.4, 0.6);
            let mut z = num_complex::Complex::new(cx, cy);

            let mut i = 0;
            while i < 255 && z.norm() <= 2.0 {
                z = z * z + c;
                i += 1;
            }

            let pixel = imgbuf.get_pixel_mut(x, y);
            let v = if i > 32 { 0u8 } else { 255u8 };
            *pixel = image::Luma([v]);
        }
    }

    imgbuf.save("pic.png").unwrap();

    {
        let f = File::create("pic.pbm")?;
        let mut writer = BufWriter::new(f);
        let encoder =
            PnmEncoder::new(&mut writer).with_subtype(PnmSubtype::Bitmap(SampleEncoding::Binary));
        imgbuf.write_with_encoder(encoder)?;
        writer.flush()?;
    }

    Ok(())
}

fn main() {
    pic().unwrap();
    plot().unwrap();
}
