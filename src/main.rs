use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};

extern crate image;
extern crate num_complex;

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    format!("Hello {name}!")
}

#[get("/pic/{name}")]
async fn pic(_name: web::Path<String>) -> impl Responder {
    let imgx = 800;
    let imgy = 800;

    let scalex = 3.0 / imgx as f32;
    let scaley = 3.0 / imgy as f32;

    let mut imgbuf = image::ImageBuffer::new(imgx, imgy);
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
            let data = (*pixel as image::Rgb<u8>).0;
            *pixel = image::Rgb([data[0], i as u8, data[2]]);
        }
    }

    let mut v: Vec<u8> = Vec::new();
    imgbuf
        .write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png)
        .unwrap();
    HttpResponse::Ok().content_type("image/png").body(v)
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(greet).service(pic))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
