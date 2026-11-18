//! Cover art: fetched on background threads, shrunk to small squares, kept only in memory.
//! Each cover also yields a 3-color palette that tints the animated background.

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

const SIZE: u32 = 128; // list rows and the player bar
const BIG: u32 = 640; // Now Playing, lyrics, headers and cards (request with a "big:" key prefix)
const MAX_CACHED: usize = 400;

pub type Palette = [Color32; 3];

enum State {
    Loading,
    Missing,
    Ready(TextureHandle, Palette),
}

pub struct ArtCache {
    map: HashMap<String, State>,
    req: Sender<String>,
    res: Receiver<(String, Option<(ColorImage, Palette)>)>,
    ctx: Context,
}

/// Picks three vivid colors: averages a 4×4 grid of the cover and keeps the most colorful cells.
fn palette(img: &image::RgbaImage) -> Palette {
    let small = image::imageops::resize(img, 4, 4, image::imageops::FilterType::Triangle);
    let mut cells: Vec<(f32, Color32)> = small
        .pixels()
        .map(|p| {
            let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
            // Prefer saturated, not-too-dark colors.
            (sat * 0.7 + (max / 255.0) * 0.3, Color32::from_rgb(p[0], p[1], p[2]))
        })
        .collect();
    cells.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |i: usize| cells.get(i * 3).or(cells.first()).map(|c| c.1).unwrap_or(Color32::GRAY);
    [pick(0), pick(1), pick(2)]
}

/// Better-quality versions of a cover URL to try first.
/// Spotify covers come in 64 / 300 / 640 px variants (size code after "ab67616d0000");
/// YouTube has HD thumbnails (maxresdefault, then sddefault/hqdefault which have black bars).
fn candidates(src: &str, big: bool) -> Vec<String> {
    let mut out = vec![];
    if let Some(i) = src.find("ab67616d0000") {
        let code_at = i + "ab67616d0000".len();
        if src.len() >= code_at + 4 {
            let want = if big { "b273" } else { "1e02" };
            out.push(format!("{}{}{}", &src[..code_at], want, &src[code_at + 4..]));
        }
    } else if src.contains("i.ytimg.com/vi/") {
        if let Some(base) = src.rsplit_once('/').map(|(b, _)| b) {
            let names: &[&str] = if big { &["maxresdefault", "sddefault", "hqdefault"] } else { &["hqdefault"] };
            out.extend(names.iter().map(|n| format!("{base}/{n}.jpg")));
        }
    }
    out.push(src.to_string());
    out
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(10)).call().ok()?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader().take(8 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Removes black letterbox bars (YouTube's 4:3 thumbnails of 16:9 videos).
fn trim_bars(img: image::DynamicImage) -> image::DynamicImage {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let dark_row = |y: u32| {
        let step = (w / 32).max(1);
        let (mut sum, mut n) = (0u32, 0u32);
        for x in (0..w).step_by(step as usize) {
            let p = rgb.get_pixel(x, y);
            sum += p[0] as u32 + p[1] as u32 + p[2] as u32;
            n += 1;
        }
        sum / (n * 3).max(1) < 14
    };
    let mut top = 0;
    while top < h / 3 && dark_row(top) {
        top += 1;
    }
    let mut bottom = h;
    while bottom > h * 2 / 3 && dark_row(bottom - 1) {
        bottom -= 1;
    }
    if top > 2 || bottom + 2 < h {
        img.crop_imm(0, top, w, bottom - top)
    } else {
        img
    }
}

/// `key` is an http(s) URL or `file:<path>`, optionally prefixed with `big:` for a sharp, large copy.
fn load(key: &str) -> Option<(ColorImage, Palette)> {
    let (src, size, big) = match key.strip_prefix("big:") {
        Some(rest) => (rest, BIG, true),
        None => (key, SIZE, false),
    };
    let bytes = if let Some(path) = src.strip_prefix("file:") {
        use lofty::prelude::*;
        let tf = lofty::read_from_path(Path::new(path)).ok()?;
        let tag = tf.primary_tag().or_else(|| tf.first_tag())?;
        tag.pictures().first()?.data().to_vec()
    } else {
        candidates(src, big).iter().find_map(|u| fetch(u))?
    };
    let img = trim_bars(image::load_from_memory(&bytes).ok()?);
    // Center-crop to a square (YouTube thumbnails are 16:9), then shrink.
    let (w, h) = (img.width(), img.height());
    let s = w.min(h);
    let img = img.crop_imm((w - s) / 2, (h - s) / 2, s, s).resize_exact(size.min(s), size.min(s), image::imageops::FilterType::Lanczos3);
    let rgba = img.to_rgba8();
    let pal = palette(&rgba);
    Some((ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), pal))
}

impl ArtCache {
    pub fn new(ctx: Context) -> Self {
        let (req, req_rx) = channel::<String>();
        let (res_tx, res) = channel();
        let req_rx = Arc::new(Mutex::new(req_rx));
        for _ in 0..3 {
            let (rx, tx, ctx) = (req_rx.clone(), res_tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let key = match rx.lock().unwrap().recv() {
                    Ok(k) => k,
                    Err(_) => return,
                };
                let img = load(&key);
                if tx.send((key, img)).is_err() {
                    return;
                }
                ctx.request_repaint();
            });
        }
        Self { map: HashMap::new(), req, res, ctx }
    }

    pub fn poll(&mut self) {
        while let Ok((key, img)) = self.res.try_recv() {
            let st = match img {
                Some((img, pal)) => State::Ready(self.ctx.load_texture(&key, img, TextureOptions::LINEAR), pal),
                None => State::Missing,
            };
            self.map.insert(key, st);
        }
    }

    /// Returns the cover if loaded; otherwise starts loading it and returns None.
    pub fn get(&mut self, key: &str) -> Option<TextureHandle> {
        match self.map.get(key) {
            Some(State::Ready(t, _)) => Some(t.clone()),
            Some(_) => None,
            None => {
                if self.map.len() > MAX_CACHED {
                    self.map.retain(|_, s| matches!(s, State::Loading)); // keep memory bounded
                }
                self.map.insert(key.to_string(), State::Loading);
                let _ = self.req.send(key.to_string());
                None
            }
        }
    }

    /// The cover's colors, once it has loaded.
    pub fn palette(&self, key: &str) -> Option<Palette> {
        match self.map.get(key) {
            Some(State::Ready(_, p)) => Some(*p),
            _ => None,
        }
    }
}
