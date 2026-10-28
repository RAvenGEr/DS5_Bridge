use std::fs;
use std::path::Path;

type IconImage = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;

fn rounded_rect_contains(
    x: f32,
    y: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> bool {
    let nearest_x = x.clamp(left + radius, right - radius);
    let nearest_y = y.clamp(top + radius, bottom - radius);
    let dx = x - nearest_x;
    let dy = y - nearest_y;
    dx * dx + dy * dy <= radius * radius
}

fn fill_rect(image: &mut IconImage, left: u32, top: u32, right: u32, bottom: u32, color: [u8; 4]) {
    for y in top..bottom {
        for x in left..right {
            image.put_pixel(x, y, image::Rgba(color));
        }
    }
}

fn fill_circle(image: &mut IconImage, cx: f32, cy: f32, radius: f32, color: [u8; 4]) {
    let radius_squared = radius * radius;
    let left = (cx - radius).floor().max(0.0) as u32;
    let top = (cy - radius).floor().max(0.0) as u32;
    let right = (cx + radius).ceil().min(image.width() as f32) as u32;
    let bottom = (cy + radius).ceil().min(image.height() as f32) as u32;

    for y in top..bottom {
        for x in left..right {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= radius_squared {
                image.put_pixel(x, y, image::Rgba(color));
            }
        }
    }
}

fn polygon_contains(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut previous = points.len() - 1;

    for current in 0..points.len() {
        let (xi, yi) = points[current];
        let (xj, yj) = points[previous];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        previous = current;
    }

    inside
}

fn generate_icon(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut imgbuf = image::ImageBuffer::from_pixel(64, 64, image::Rgba([0, 0, 0, 0]));
    for y in 0..64 {
        for x in 0..64 {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if rounded_rect_contains(px, py, 3.0, 3.0, 61.0, 61.0, 13.0) {
                imgbuf.put_pixel(x, y, image::Rgba([91, 111, 228, 255]));
            }
            if rounded_rect_contains(px, py, 6.0, 6.0, 58.0, 58.0, 10.0) {
                imgbuf.put_pixel(x, y, image::Rgba([25, 31, 43, 255]));
            }
        }
    }

    // High contrast gamepad silhouette stays recognizable at 16px tray size.
    let controller = [
        (12.0, 21.0),
        (16.0, 18.0),
        (24.0, 19.0),
        (40.0, 19.0),
        (48.0, 18.0),
        (52.0, 21.0),
        (57.0, 41.0),
        (56.0, 47.0),
        (52.0, 50.0),
        (47.0, 47.0),
        (42.0, 39.0),
        (22.0, 39.0),
        (17.0, 47.0),
        (12.0, 50.0),
        (8.0, 47.0),
        (7.0, 41.0),
    ];
    for y in 16..53 {
        for x in 5..59 {
            if polygon_contains(x as f32 + 0.5, y as f32 + 0.5, &controller) {
                imgbuf.put_pixel(x, y, image::Rgba([235, 240, 248, 255]));
            }
        }
    }

    // D-pad, touch panel and thumbsticks.
    fill_rect(&mut imgbuf, 13, 25, 17, 38, [39, 48, 65, 255]);
    fill_rect(&mut imgbuf, 9, 29, 21, 34, [39, 48, 65, 255]);
    fill_rect(&mut imgbuf, 25, 22, 39, 25, [91, 111, 228, 255]);
    fill_circle(&mut imgbuf, 25.5, 32.0, 4.0, [39, 48, 65, 255]);
    fill_circle(&mut imgbuf, 39.0, 32.0, 4.0, [39, 48, 65, 255]);
    fill_circle(&mut imgbuf, 25.5, 32.0, 1.2, [117, 139, 174, 255]);
    fill_circle(&mut imgbuf, 39.0, 32.0, 1.2, [117, 139, 174, 255]);

    // Distinct face buttons and a small cyan bridge link mark.
    fill_circle(&mut imgbuf, 48.0, 27.0, 2.2, [96, 165, 250, 255]);
    fill_circle(&mut imgbuf, 52.0, 32.0, 2.2, [74, 222, 128, 255]);
    fill_circle(&mut imgbuf, 44.0, 32.0, 2.2, [250, 204, 21, 255]);
    fill_circle(&mut imgbuf, 48.0, 37.0, 2.2, [248, 113, 113, 255]);
    fill_rect(&mut imgbuf, 28, 45, 36, 48, [96, 165, 250, 255]);

    imgbuf.save(path).unwrap();
}

fn generate_windows_icon(png_path: &Path, ico_path: &Path) {
    use image::ImageEncoder;

    let icon = image::open(png_path).unwrap().to_rgba8();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            icon.as_raw(),
            icon.width(),
            icon.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

    // A single PNG-backed 64x64 frame is supported by current Windows shells.
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico.extend_from_slice(&[64, 64, 0, 0]); // width, height, palette, reserved
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&png);
    fs::write(ico_path, ico).unwrap();
}

/// Embed the app icon and version metadata into the built .exe so Explorer,
/// Start Menu shortcuts and the "Apps & Features" entry show real values
/// instead of a generic icon/blank version.
#[cfg(windows)]
fn embed_windows_resources() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/app.ico");
    res.set("ProductName", "DS5 Bridge Companion");
    res.set("FileDescription", "DS5 Bridge Companion");
    res.set("InternalName", "DS5BridgeCompanion.exe");
    res.set("OriginalFilename", "DS5BridgeCompanion.exe");
    res.set(
        "LegalCopyright",
        "DS5 Bridge contributors. Licensed under AGPL-3.0.",
    );

    // VERSIONINFO numeric fields need a four-part version ("1.2.3.4").
    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts: Vec<&str> = pkg.split('.').collect();
    parts.resize(4, "0");
    let quad = parts.join(".");
    res.set("FileVersion", &quad);
    res.set("ProductVersion", &quad);

    if let Err(err) = res.compile() {
        println!("cargo:warning=failed to embed Windows version resource: {err}");
    }
}

fn main() {
    generate_icon(Path::new("assets/icon.png"));
    generate_windows_icon(Path::new("assets/icon.png"), Path::new("assets/app.ico"));

    #[cfg(windows)]
    embed_windows_resources();
}
