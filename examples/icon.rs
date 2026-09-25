//! Renders the Kerf app icon (B2 "hairline kerf") for every macOS icon size.
//!
//!   cargo run --example icon
//!
//! Writes `assets/icon/kerf.svg` (master), `assets/icon/kerf-{16,32,…}.png` and
//! `assets/icon/Kerf.iconset/`. `scripts/bundle-macos.sh` turns the iconset into `Kerf.icns`.
//!
//! Small sizes are drawn, not just scaled: the frost hairline (the "kerf") thickens and the
//! bars widen slightly so the cut stays visible at 16 and 32 px.

use std::fs;
use std::path::Path;

/// macOS icon grid: artwork is an 824-pt rounded square centred on a 1024 canvas.
const CANVAS: f32 = 1024.;
const ART: f32 = 824.;
const INSET: f32 = (CANVAS - ART) / 2.;

/// The icon in its 120-unit design space, tuned for a target pixel size.
fn svg(px: u32) -> String {
    // Hairline width and bar widths in design units (120 = full artwork).
    let (slit, bar, gap) = match px {
        0..=16 => (7.0, 25.0, 3.0),
        17..=32 => (4.0, 24.5, 1.5),
        33..=64 => (2.4, 23.0, 2.0),
        _ => (1.5, 22.0, 2.25),
    };
    let s = ART / 120.;
    let u = |v: f32| INSET + v * s; // design units → canvas (with inset)
    let d = |v: f32| v * s; // design length → canvas length
    let cx = 60.;
    let left_x = cx - slit / 2. - gap - bar;
    let right_x = cx + slit / 2. + gap;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{c}" height="{c}" viewBox="0 0 {c} {c}">
  <rect x="{i}" y="{i}" width="{a}" height="{a}" rx="{r}" fill="#000000"/>
  <rect x="{i}" y="{i}" width="{a}" height="{a}" rx="{r}" fill="none" stroke="#2e2e2e" stroke-width="{sw}"/>
  <rect x="{lx}" y="{ly}" width="{bw}" height="{bh}" rx="{br}" fill="#e0706c"/>
  <rect x="{rx}" y="{ry}" width="{bw}" height="{bh}" rx="{br}" fill="#8fc49a"/>
  <rect x="{sx}" y="{sy}" width="{sw2}" height="{sh}" fill="#a9c4d9"/>
</svg>
"##,
        c = CANVAS,
        i = INSET,
        a = ART,
        r = d(28.),
        sw = d(0.8),
        lx = u(left_x),
        ly = u(28.),
        rx = u(right_x),
        ry = u(38.),
        bw = d(bar),
        bh = d(58.),
        br = d(3.),
        sx = u(cx - slit / 2.),
        sy = u(20.),
        sw2 = d(slit),
        sh = d(84.),
    )
}

fn render(svg: &str, px: u32) -> Vec<u8> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).expect("valid svg");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px).expect("pixmap");
    let scale = px as f32 / CANVAS;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().expect("png")
}

fn main() {
    let out = Path::new("assets/icon");
    let set = out.join("Kerf.iconset");
    fs::create_dir_all(&set).unwrap();
    fs::write(out.join("kerf.svg"), svg(1024)).unwrap();
    fs::write(out.join("kerf-16.svg"), svg(16)).unwrap();

    for px in [16u32, 32, 64, 128, 256, 512, 1024] {
        fs::write(out.join(format!("kerf-{px}.png")), render(&svg(px), px)).unwrap();
    }
    // Apple iconset naming: icon_<pt>x<pt>[@2x].png
    for (pt, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]
    {
        let px = pt * scale;
        let name = if scale == 1 { format!("icon_{pt}x{pt}.png") } else { format!("icon_{pt}x{pt}@2x.png") };
        fs::write(set.join(name), render(&svg(px), px)).unwrap();
    }
    // Windows: .ico with PNG entries (16–256), embedded into kerf.exe by build.rs.
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&px| render(&svg(px), px)).collect();
    fs::write(out.join("kerf.ico"), ico(&sizes, &pngs)).unwrap();
    println!("icon written to {}", out.display());
}

/// Minimal ICO container holding PNG images (supported since Windows Vista).
fn ico(sizes: &[u32], pngs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (px, png) in sizes.iter().zip(pngs) {
        let dim = if *px >= 256 { 0 } else { *px as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        out.extend_from_slice(png);
    }
    out
}
