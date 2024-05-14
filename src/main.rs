use std::{
    borrow::BorrowMut,
    fs::File,
    io::{BufWriter, Write},
};

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
        let root = BitMapBackend::with_buffer(&mut plot_buf, (IMG_X, IMG_Y)).into_drawing_area();

        let mut chart = ChartBuilder::on(&root)
            .x_label_area_size(35)
            .y_label_area_size(40)
            .margin(5)
            .caption("Histogram Test", ("sans-serif", 50.0))
            .build_cartesian_2d((0u32..10u32).into_segmented(), 0u32..10u32)
            .unwrap();

        chart
            .configure_mesh()
            .disable_x_mesh()
            .bold_line_style(&WHITE.mix(0.3))
            .y_desc("Count")
            .x_desc("Bucket")
            .axis_desc_style(("sans-serif", 15))
            .draw()
            .unwrap();

        let data = [
            0u32, 1, 1, 1, 4, 2, 5, 7, 8, 6, 4, 2, 1, 8, 3, 3, 3, 4, 4, 3, 3, 3,
        ];

        chart
            .draw_series(
                Histogram::vertical(&chart)
                    .style(RED.mix(0.5).filled())
                    .data(data.iter().map(|x: &u32| (*x, 1))),
            )
            .unwrap();
    }

    {
        let plot_img = image::RgbImage::from_raw(IMG_X, IMG_Y, plot_buf.to_vec()).unwrap();
        let mut imgbuf = GrayImage::new(IMG_X, IMG_Y);

        for x in 0..IMG_X {
            for y in 0..IMG_Y {
                let src = plot_img.get_pixel_checked(x, y).unwrap();
                let luma = (src.0[0] / 3 + src.0[1] / 3 + src.0[2] / 3);
                println!("{}", luma);
                imgbuf.get_pixel_mut_checked(x, y).unwrap().0[0] = if luma < 10 { 255 } else { 0 };
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
