//! Desktop kamerani tekshirish: bitta kadr olib, PNG'ga yozadi.
//! `cargo run --example camera_snap -- /tmp/kadr.png`
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "kadr.png".into());
    for c in nokhwa::query(ApiBackend::Auto)? {
        println!("kamera: {} ({})", c.human_name(), c.index());
    }
    let wanted = CameraFormat::new(Resolution::new(640, 480), FrameFormat::MJPEG, 30);
    let mut cam = Camera::new(CameraIndex::Index(0), RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(wanted)))?;
    cam.open_stream()?;
    println!("format: {}", cam.camera_format());
    let t0 = std::time::Instant::now();
    let mut img = None;
    for _ in 0..10 {
        img = Some(cam.frame()?.decode_image::<RgbFormat>()?);
    }
    println!("10 kadr: {:?}", t0.elapsed());
    img.unwrap().save(&out)?;
    println!("saqlandi: {out}");
    Ok(())
}
