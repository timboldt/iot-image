use std::{
    fs::File,
    io::{BufWriter, Write},
};

use image::{
    codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding}, GrayImage, ImageResult,
};
// use plotters::prelude::*;
// use plotters_bitmap::BitMapBackend;

extern crate image;
extern crate num_complex;

// async fn plot(_name: web::Path<String>) -> impl Responder {
//     const IMG_X: u32 = 640;
//     const IMG_Y: u32 = 480;
//     let mut buf: [u8; (IMG_X * IMG_Y) as usize] = [0; (IMG_X * IMG_Y) as usize];

//     // XXXX just write to a file for now - it's simpler.

//     let root = BitMapBackend::with_buffer(&mut buf, (IMG_X, IMG_Y)).into_drawing_area();

//     let mut chart = ChartBuilder::on(&root)
//         .x_label_area_size(35)
//         .y_label_area_size(40)
//         .margin(5)
//         .caption("Histogram Test", ("sans-serif", 50.0))
//         .build_cartesian_2d((0u32..10u32).into_segmented(), 0u32..10u32).unwrap();

//     chart
//         .configure_mesh()
//         .disable_x_mesh()
//         .bold_line_style(&WHITE.mix(0.3))
//         .y_desc("Count")
//         .x_desc("Bucket")
//         .axis_desc_style(("sans-serif", 15))
//         .draw().unwrap();

//     let data = [
//         0u32, 1, 1, 1, 4, 2, 5, 7, 8, 6, 4, 2, 1, 8, 3, 3, 3, 4, 4, 3, 3, 3,
//     ];

//     chart.draw_series(
//         Histogram::vertical(&chart)
//             .style(RED.mix(0.5).filled())
//             .data(data.iter().map(|x: &u32| (*x, 1))),
//     ).unwrap();

//     // XXXX now load the file and send it.

//     let mut v: Vec<u8> = Vec::new();
//     imgbuf
//         .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
//         .unwrap();
//     HttpResponse::Ok().content_type("image/png").body(v)
// }

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
        let f = File::create("test.pbm")?;
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
}
