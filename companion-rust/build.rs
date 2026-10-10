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
    generate_icon(Path::new("ui/assets/icon.png"));

    #[cfg(windows)]
    embed_windows_resources();

    slint_build::compile("ui/appwindow.slint").unwrap();
}
