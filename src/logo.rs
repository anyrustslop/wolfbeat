// Wolfbeat's original logo: a wolf head drawn with equalizer bars, white on a sunset squircle.
// Dependency-free so build.rs can `include!` it to make the exe icon; theme.rs uses `bars()` to
// paint the (animated) in-app version.

/// Wolf head outline, eyes and nose in a 100 x 100 box.
const HEAD: &[(f32, f32)] = &[
    (50.0, 24.0), (60.0, 28.0), (80.0, 5.0), (85.0, 37.0), (96.0, 50.0), (86.0, 55.0), (92.0, 66.0),
    (76.0, 70.0), (62.0, 90.0), (50.0, 97.0), (38.0, 90.0), (24.0, 70.0), (8.0, 66.0), (14.0, 55.0),
    (4.0, 50.0), (15.0, 37.0), (20.0, 5.0), (40.0, 28.0),
];
pub const EYE_L: [(f32, f32); 3] = [(27.0, 47.0), (44.0, 53.0), (32.0, 57.0)];
pub const EYE_R: [(f32, f32); 3] = [(73.0, 47.0), (56.0, 53.0), (68.0, 57.0)];
pub const NOSE: [(f32, f32); 3] = [(44.0, 79.0), (56.0, 79.0), (50.0, 86.0)];

pub const BAR_COUNT: usize = 13;
/// Bar width as a fraction of the spacing between bars.
pub const BAR_FILL: f32 = 0.62;
/// Where the head sits inside the squircle (fractions of the icon size).
pub const HEAD_X: f32 = 0.14;
pub const HEAD_Y: f32 = 0.13;
pub const HEAD_SIZE: f32 = 0.72;

pub const SUNSET_A: [u8; 3] = [255, 110, 72];
pub const SUNSET_B: [u8; 3] = [255, 58, 132];

/// Top and bottom of the wolf outline at column `x` (head units).
fn span(x: f32) -> (f32, f32) {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    let n = HEAD.len();
    for i in 0..n {
        let (x1, y1) = HEAD[i];
        let (x2, y2) = HEAD[(i + 1) % n];
        if (x1 <= x && x < x2) || (x2 <= x && x < x1) {
            let y = y1 + (x - x1) * (y2 - y1) / (x2 - x1);
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    (lo, hi)
}

/// The bars: (center x, top y, bottom y) in head units (0..100), plus bar width.
pub fn bars() -> ([(f32, f32, f32); BAR_COUNT], f32) {
    let (x0, x1) = (10.0f32, 90.0f32);
    let step = (x1 - x0) / (BAR_COUNT - 1) as f32;
    let mut out = [(0.0, 0.0, 0.0); BAR_COUNT];
    for (i, b) in out.iter_mut().enumerate() {
        let x = x0 + i as f32 * step;
        let (t, bt) = span(x);
        *b = (x, t, bt);
    }
    (out, step * BAR_FILL)
}

fn in_poly(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            c = !c;
        }
        j = i;
    }
    c
}

pub fn in_cutout(x: f32, y: f32) -> bool {
    in_poly(&EYE_L, x, y) || in_poly(&EYE_R, x, y) || in_poly(&NOSE, x, y)
}

/// Straight RGBA, top row first: the full icon (squircle + wolf bars).
pub fn render_rgba(size: usize) -> Vec<u8> {
    let ss = 4usize;
    let (bars, bw) = bars();
    let r = bw / 2.0;
    let mut out = vec![0u8; size * size * 4];
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = (px as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32;
                    let fy = (py as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32;
                    let rr = 0.28f32;
                    let dx = ((fx - 0.5).abs() - (0.5 - rr)).max(0.0);
                    let dy = ((fy - 0.5).abs() - (0.5 - rr)).max(0.0);
                    if (dx * dx + dy * dy).sqrt() > rr {
                        continue;
                    }
                    let t = (fx * 0.55 + fy * 0.45).clamp(0.0, 1.0);
                    let mut c = [0, 1, 2].map(|k| SUNSET_A[k] as f32 + (SUNSET_B[k] as f32 - SUNSET_A[k] as f32) * t);
                    let hx = (fx - HEAD_X) / HEAD_SIZE * 100.0;
                    let hy = (fy - HEAD_Y) / HEAD_SIZE * 100.0;
                    for (bx, top, bottom) in bars {
                        if (hx - bx).abs() <= r {
                            let yy = hy.clamp(top + r, bottom - r);
                            if ((hx - bx).powi(2) + (hy - yy).powi(2)).sqrt() <= r && !in_cutout(hx, hy) {
                                c = [255.0, 255.0, 255.0];
                            }
                        }
                    }
                    acc[0] += c[0];
                    acc[1] += c[1];
                    acc[2] += c[2];
                    acc[3] += 255.0;
                }
            }
            let i = (py * size + px) * 4;
            if acc[3] > 0.0 {
                let cover = acc[3] / 255.0;
                out[i] = (acc[0] / cover) as u8;
                out[i + 1] = (acc[1] / cover) as u8;
                out[i + 2] = (acc[2] / cover) as u8;
                out[i + 3] = (acc[3] / (ss * ss) as f32) as u8;
            }
        }
    }
    out
}

/// A Windows .ico with 32-bit images at the given sizes.
pub fn make_ico(sizes: &[usize]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = Vec::new();
    for &s in sizes {
        let rgba = render_rgba(s);
        let mut img = Vec::new();
        img.extend_from_slice(&40u32.to_le_bytes());
        img.extend_from_slice(&(s as u32).to_le_bytes());
        img.extend_from_slice(&((s * 2) as u32).to_le_bytes());
        img.extend_from_slice(&1u16.to_le_bytes());
        img.extend_from_slice(&32u16.to_le_bytes());
        img.extend_from_slice(&[0u8; 24]);
        for y in (0..s).rev() {
            for x in 0..s {
                let i = (y * s + x) * 4;
                img.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        img.extend(std::iter::repeat(0u8).take(((s + 31) / 32) * 4 * s));
        images.push(img);
    }
    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (k, &s) in sizes.iter().enumerate() {
        let d = if s >= 256 { 0u8 } else { s as u8 };
        ico.extend_from_slice(&[d, d, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(images[k].len() as u32).to_le_bytes());
        ico.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += images[k].len();
    }
    for img in images {
        ico.extend_from_slice(&img);
    }
    ico
}
