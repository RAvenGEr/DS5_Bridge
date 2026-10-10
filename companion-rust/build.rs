use std::fs;
use std::path::Path;

fn generate_icon(path: &Path) {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut imgbuf = image::ImageBuffer::new(64, 64);
        for (x, y, pixel) in imgbuf.enumerate_pixels_mut() {
            let dx = (x as f32) - 32.0;
            let dy = (y as f32) - 32.0;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= 28.0 {
                let alpha = if dist > 26.0 {
                    ((28.0 - dist) / 2.0 * 255.0) as u8
                } else {
                    255
                };
                let r = (99.0 + (x as f32 / 64.0) * 50.0) as u8;
                let g = (102.0 + (y as f32 / 64.0) * 30.0) as u8;
                let b = 241;
                *pixel = image::Rgba([r, g, b, alpha]);
            } else {
                *pixel = image::Rgba([0, 0, 0, 0]);
            }
        }
        imgbuf.save(path).unwrap();
    }
}

fn main() {
    generate_icon(Path::new("assets/icon.png"));
    generate_icon(Path::new("ui/assets/icon.png"));

    slint_build::compile("ui/appwindow.slint").unwrap();
}
