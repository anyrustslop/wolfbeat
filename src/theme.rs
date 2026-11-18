//! Wolfbeat's look: "Midnight Sunset" palette, Windows 11 fonts and Fluent icons, gradients, and the logo.

use egui::epaint::Mesh;
use egui::*;

// ---------- palette ----------
pub const BG: Color32 = Color32::from_rgb(11, 12, 16); // page background
// Frosted glass: dark, semi-transparent panels over the animated background (premultiplied alpha).
pub const SIDEBAR: Color32 = Color32::from_rgba_premultiplied(9, 10, 14, 196); // sidebar + player bar glass
pub const GLASS_EDGE: Color32 = Color32::from_rgba_premultiplied(26, 26, 30, 26); // thin light rim on glass
pub const MENU: Color32 = Color32::from_rgb(24, 26, 34); // popups stay solid for readability
pub const RAISED: Color32 = Color32::from_rgba_premultiplied(17, 17, 19, 17); // inputs, chips (white tint)
pub const HOVER: Color32 = Color32::from_rgba_premultiplied(24, 24, 27, 24);
pub const SELECTED: Color32 = Color32::from_rgba_premultiplied(35, 35, 39, 35);
pub const LINE: Color32 = Color32::from_rgba_premultiplied(22, 22, 25, 22);
pub const TEXT: Color32 = Color32::from_rgb(243, 244, 247);
pub const SUBTLE: Color32 = Color32::from_rgb(158, 164, 178);
pub const DIM: Color32 = Color32::from_rgb(103, 109, 124);
pub const ACCENT: Color32 = Color32::from_rgb(255, 110, 72); // sunset orange
pub const ACCENT2: Color32 = Color32::from_rgb(255, 58, 132); // into pink

// ---------- Segoe Fluent Icons (Windows 11) ----------
pub mod icon {
    pub const SEARCH: &str = "\u{E721}";
    pub const MUSIC: &str = "\u{EC4F}";
    pub const HEART: &str = "\u{EB51}";
    pub const HEART_FILL: &str = "\u{EB52}";
    pub const QUEUE: &str = "\u{E8FD}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const ADD: &str = "\u{E710}";
    pub const IMPORT: &str = "\u{E896}";
    pub const PLAY: &str = "\u{E768}";
    pub const PAUSE: &str = "\u{E769}";
    pub const PREV: &str = "\u{E892}";
    pub const NEXT: &str = "\u{E893}";
    pub const SHUFFLE: &str = "\u{E8B1}";
    pub const REPEAT: &str = "\u{E8EE}";
    pub const REPEAT_ONE: &str = "\u{E8ED}";
    pub const VOLUME: &str = "\u{E767}";
    pub const VOLUME_LOW: &str = "\u{E993}";
    pub const MUTE: &str = "\u{E74F}";
    pub const DELETE: &str = "\u{E74D}";
    pub const CLOCK: &str = "\u{E823}";
    pub const CLOSE: &str = "\u{E711}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const LIST: &str = "\u{E8FD}";
    pub const EDIT: &str = "\u{E70F}";
    pub const LINK: &str = "\u{E71B}";
    pub const NEXT_UP: &str = "\u{E7C8}";
    pub const LYRICS: &str = "\u{E720}";
    pub const HOME: &str = "\u{E80F}";
    pub const MINIMIZE: &str = "\u{E921}";
    pub const MAXIMIZE: &str = "\u{E922}";
    pub const RESTORE: &str = "\u{E923}";
    pub const WIN_CLOSE: &str = "\u{E8BB}";
    pub const CHEVRON_DOWN: &str = "\u{E70D}";
    pub const THEME: &str = "\u{E790}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const INFO: &str = "\u{E946}";
}

/// Default background colors when nothing is playing (or the cover has no art).
pub const AURORA_DEFAULT: [Color32; 3] = [ACCENT, ACCENT2, Color32::from_rgb(108, 82, 255)];

pub fn radial_blob(p: &Painter, center: Pos2, radius: f32, color: Color32) {
    // Three rings fading to transparent: reads like a heavy blur, costs ~100 triangles.
    let rings = [(0.0f32, 1.0f32), (0.45, 0.55), (0.75, 0.2), (1.0, 0.0)];
    let segs = 40u32;
    let mut m = Mesh::default();
    m.colored_vertex(center, color);
    for (r, a) in &rings[1..] {
        for i in 0..segs {
            let ang = i as f32 / segs as f32 * std::f32::consts::TAU;
            m.colored_vertex(center + vec2(ang.cos(), ang.sin()) * radius * *r, color.gamma_multiply(*a));
        }
    }
    for i in 0..segs {
        let j = (i + 1) % segs;
        m.add_triangle(0, 1 + i, 1 + j);
        for ring in 0..2 {
            let a = 1 + ring * segs;
            let b = a + segs;
            m.add_triangle(a + i, b + i, b + j);
            m.add_triangle(a + i, b + j, a + j);
        }
    }
    p.add(Shape::mesh(m));
}

/// Slowly drifting, blurred color glows (tinted by the cover) — the animated "aurora" background.
pub fn paint_aurora(p: &Painter, rect: Rect, colors: [Color32; 3], t: f64, strength: f32) {
    p.rect_filled(rect, 0.0, BG);
    let (w, h) = (rect.width(), rect.height());
    let big = w.max(h);
    let blobs = [
        (0usize, 0.18f32, 0.20f32, 0.62f32, 0.050f64, 0.0f64),
        (1, 0.82, 0.25, 0.55, 0.037, 1.7),
        (2, 0.30, 0.85, 0.60, 0.043, 3.1),
        (0, 0.75, 0.80, 0.45, 0.061, 4.4),
    ];
    for (ci, bx, by, br, speed, phase) in blobs {
        let tt = t * speed + phase;
        let center = rect.min + vec2(w * (bx + 0.10 * tt.sin() as f32), h * (by + 0.12 * (tt * 1.3).cos() as f32));
        let radius = big * br * (0.92 + 0.08 * (tt * 0.7).sin() as f32);
        radial_blob(p, center, radius, colors[ci].gamma_multiply(0.42 * strength));
    }
    // Darken toward the bottom so text and controls stay readable.
    vgradient(p, rect, Color32::from_black_alpha(40), Color32::from_black_alpha(150));
}

pub fn icons(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

// ---------- setup ----------
pub fn setup(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::path::Path::new(&std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
    let mut load = |key: &str, files: &[&str]| -> bool {
        for f in files {
            if let Ok(bytes) = std::fs::read(dir.join(f)) {
                fonts.font_data.insert(key.into(), FontData::from_owned(bytes));
                return true;
            }
        }
        false
    };
    let has_regular = load("ui", &["segoeui.ttf"]);
    let has_semi = load("ui-semi", &["seguisb.ttf", "segoeui.ttf"]);
    let has_bold = load("ui-bold", &["segoeuib.ttf", "seguisb.ttf"]);
    let has_icons = load("fluent", &["SegoeIcons.ttf", "segmdl2.ttf"]);
    let has_sym = load("symbols", &["seguisym.ttf"]);

    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: Option<&str>| {
        let mut v: Vec<String> = first.into_iter().map(String::from).collect();
        v.extend(defaults.iter().cloned());
        if has_sym {
            v.push("symbols".into());
        }
        if has_icons {
            v.push("fluent".into());
        }
        v
    };
    fonts.families.insert(FontFamily::Proportional, family(has_regular.then_some("ui")));
    fonts.families.insert(FontFamily::Name("semibold".into()), family(has_semi.then_some("ui-semi")));
    fonts.families.insert(FontFamily::Name("bold".into()), family(has_bold.then_some("ui-bold")));
    fonts.families.insert(FontFamily::Name("icons".into()), family(has_icons.then_some("fluent")));
    ctx.set_fonts(fonts);

    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = MENU;
    v.window_stroke = Stroke::new(1.0_f32, LINE);
    v.window_rounding = Rounding::same(12.0);
    v.menu_rounding = Rounding::same(10.0);
    v.extreme_bg_color = RAISED;
    v.faint_bg_color = SIDEBAR;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    for (w, fill) in [
        (&mut v.widgets.noninteractive, SIDEBAR),
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, HOVER),
        (&mut v.widgets.active, SELECTED),
        (&mut v.widgets.open, SELECTED),
    ] {
        w.rounding = Rounding::same(8.0);
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke.color = TEXT;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.noninteractive.fg_stroke.color = SUBTLE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, LINE);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(12.0, 7.0);
        s.spacing.menu_margin = Margin::same(6.0);
        s.spacing.scroll = style::ScrollStyle::floating();
        s.interaction.selectable_labels = false;
        s.text_styles.insert(TextStyle::Body, body(14.0));
        s.text_styles.insert(TextStyle::Button, body(14.0));
        s.text_styles.insert(TextStyle::Small, body(12.0));
        s.animation_time = 0.15;
    });
}

// ---------- helpers ----------
pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    // Colors are premultiplied, so blending component-wise is correct even for translucent ones.
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// 0→1 hover progress for an id, animated over ~120 ms.
pub fn hover_t(ui: &Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time(id.with("hover"), on, 0.12)
}

/// Vertical gradient (no rounding) — used for page hero backgrounds.
pub fn vgradient(p: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(rect.left_top(), top);
    m.colored_vertex(rect.right_top(), top);
    m.colored_vertex(rect.left_bottom(), bottom);
    m.colored_vertex(rect.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 3, 2);
    p.add(Shape::mesh(m));
}

fn rounded_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut pts = Vec::with_capacity(4 * 9);
    let corners = [
        (rect.right() - r, rect.bottom() - r, 0.0f32),
        (rect.left() + r, rect.bottom() - r, 90.0),
        (rect.left() + r, rect.top() + r, 180.0),
        (rect.right() - r, rect.top() + r, 270.0),
    ];
    for (cx, cy, start) in corners {
        for i in 0..=8 {
            let a = (start + 90.0 * i as f32 / 8.0).to_radians();
            pts.push(pos2(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts
}

/// Rounded shape filled with the diagonal accent gradient (anti-aliased edge underneath).
pub fn gradient_fill(p: &Painter, rect: Rect, r: f32, a: Color32, b: Color32) {
    p.rect_filled(rect, r, lerp(a, b, 0.5));
    let inner = rect.shrink(0.8);
    let pts = rounded_outline(inner, (r - 0.8).max(0.0));
    let col = |q: Pos2| lerp(a, b, ((q.x - rect.left()) / rect.width() * 0.55 + (q.y - rect.top()) / rect.height() * 0.45).clamp(0.0, 1.0));
    let mut m = Mesh::default();
    let c = inner.center();
    m.colored_vertex(c, col(c));
    for q in &pts {
        m.colored_vertex(*q, col(*q));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(m));
}

/// Soft drop shadow under a rounded rect.
pub fn shadow(p: &Painter, rect: Rect, r: f32, strength: f32) {
    for i in 0..6 {
        let e = 2.0 + i as f32 * 2.5;
        let a = (strength * 22.0 / (1.0 + i as f32)) as u8;
        p.rect_filled(rect.expand(e).translate(vec2(0.0, 4.0 + i as f32)), r + e, Color32::from_black_alpha(a));
    }
}

// ---------- logo ----------
/// Wolfbeat's logo: a wolf head made of equalizer bars on a sunset squircle (see logo.rs).
/// `beat` makes the bars pulse (pass the time while music plays).
pub fn paint_logo(p: &Painter, rect: Rect, beat: Option<f64>) {
    let s = rect.width().min(rect.height());
    let rect = Rect::from_center_size(rect.center(), vec2(s, s));
    gradient_fill(p, rect, s * 0.28, ACCENT, ACCENT2);
    let (bars, bw) = crate::logo::bars();
    let unit = s * crate::logo::HEAD_SIZE / 100.0;
    let origin = rect.min + vec2(s * crate::logo::HEAD_X, s * crate::logo::HEAD_Y);
    let to_screen = |x: f32, y: f32| origin + vec2(x * unit, y * unit);
    let w = bw * unit;
    for (i, (x, top, bottom)) in bars.iter().enumerate() {
        // Each bar pulses a little around its middle, like an equalizer.
        let k = match beat {
            Some(t) => 0.86 + 0.14 * ((t * (4.0 + (i % 5) as f64 * 0.9) + i as f64 * 1.3).sin().abs() as f32),
            None => 1.0,
        };
        let mid = (top + bottom) / 2.0;
        let half = (bottom - top) / 2.0 * k;
        let a = to_screen(*x, mid - half);
        let b = to_screen(*x, mid + half);
        let bar = Rect::from_min_max(pos2(a.x - w / 2.0, a.y), pos2(a.x + w / 2.0, b.y.max(a.y + w)));
        p.rect_filled(bar, w / 2.0, Color32::WHITE);
    }
    // Eyes and nose are cut out of the bars (painted in the squircle's color at that spot).
    for tri in [crate::logo::EYE_L, crate::logo::EYE_R, crate::logo::NOSE] {
        let pts: Vec<Pos2> = tri.iter().map(|(x, y)| to_screen(*x, *y)).collect();
        let c = pts.iter().fold(vec2(0.0, 0.0), |acc, q| acc + q.to_vec2()) / 3.0;
        let t = ((c.x - rect.left()) / s * 0.55 + (c.y - rect.top()) / s * 0.45).clamp(0.0, 1.0);
        p.add(Shape::convex_polygon(pts, lerp(ACCENT, ACCENT2, t), Stroke::NONE));
    }
}

/// The same logo rendered in software for the window/taskbar icon.
pub fn window_icon() -> IconData {
    let n = 64usize;
    IconData { rgba: crate::logo::render_rgba(n), width: n as u32, height: n as u32 }
}

// ---------- accent that follows the current cover ----------
use std::sync::atomic::{AtomicU32, Ordering};
static ACC1: AtomicU32 = AtomicU32::new(0);
static ACC2: AtomicU32 = AtomicU32::new(0);

fn pack(c: Color32) -> u32 {
    u32::from_be_bytes([c.r(), c.g(), c.b(), 255])
}
fn unpack(v: u32, fallback: Color32) -> Color32 {
    if v == 0 {
        return fallback;
    }
    let [r, g, b, _] = v.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

/// Bright, saturated version of a cover color so it works as a UI accent on dark glass.
fn vivid(c: Color32, hue_shift: f32) -> Color32 {
    let h = ecolor::Hsva::from(c);
    let s = if h.s < 0.12 { 0.0 } else { h.s.max(0.62) }; // keep greys grey
    Color32::from(ecolor::Hsva::new((h.h + hue_shift).rem_euclid(1.0), s, h.v.max(0.95), 1.0))
}

/// Called every frame with the background colors; buttons/progress/markers pick it up.
pub fn set_accent_from(colors: [Color32; 3]) {
    let a = vivid(colors[0], 0.0);
    let mut b = vivid(colors[1], 0.0);
    let (ha, hb) = (ecolor::Hsva::from(a).h, ecolor::Hsva::from(b).h);
    if (ha - hb).abs() < 0.04 || ecolor::Hsva::from(b).s < 0.1 {
        b = vivid(colors[0], 0.07); // too similar: shift the hue a little for a visible gradient
    }
    let grey = ecolor::Hsva::from(a).s < 0.1;
    ACC1.store(if grey { 0 } else { pack(a) }, Ordering::Relaxed);
    ACC2.store(if grey { 0 } else { pack(b) }, Ordering::Relaxed);
}
pub fn accent() -> Color32 {
    unpack(ACC1.load(Ordering::Relaxed), ACCENT)
}
pub fn accent2() -> Color32 {
    unpack(ACC2.load(Ordering::Relaxed), ACCENT2)
}


// ---------- "Scenery": animated anime-style landscape background ----------
// Painted live (no image files): a sky that follows your local time of day, layered mountains,
// drifting clouds, a few birds, and grass swaying in the wind.
use std::sync::atomic::{AtomicBool, AtomicU8};
static BG_MODE: AtomicU8 = AtomicU8::new(0); // 0 scenery, 1 aurora, 2 plain dark
static ANIMATE: AtomicBool = AtomicBool::new(true);

pub fn set_background(mode: u8) {
    BG_MODE.store(mode.min(2), Ordering::Relaxed);
}
pub fn set_animate(on: bool) {
    ANIMATE.store(on, Ordering::Relaxed);
}
pub fn animate() -> bool {
    ANIMATE.load(Ordering::Relaxed)
}

/// Background for the current setting.
pub fn paint_background(p: &Painter, rect: Rect, colors: [Color32; 3], t: f64, strength: f32) {
    let t = if animate() { t } else { 0.0 }; // frozen when animation is off
    match BG_MODE.load(Ordering::Relaxed) {
        0 => paint_landscape(p, rect, colors, t, strength),
        1 => paint_aurora(p, rect, colors, t, strength),
        _ => {
            p.rect_filled(rect, 0.0, BG);
            vgradient(p, rect, colors[0].gamma_multiply(0.10), Color32::TRANSPARENT);
        }
    }
}

struct Sky {
    top: Color32,
    mid: Color32,
    horizon: Color32,
    sun: Color32,
    far: Color32,
    near: Color32,
    hill_top: Color32,
    hill_bottom: Color32,
    cloud: Color32,
    cloud_shade: Color32,
    night: f32,
}

fn sky_for_hour(h: u16) -> Sky {
    let rgb = Color32::from_rgb;
    match h {
        // Golden morning / sunset
        5..=7 | 17..=19 => Sky {
            top: rgb(54, 66, 138),
            mid: rgb(196, 112, 152),
            horizon: rgb(255, 186, 128),
            sun: rgb(255, 214, 160),
            far: rgb(156, 118, 160),
            near: rgb(98, 76, 118),
            hill_top: rgb(84, 92, 92),
            hill_bottom: rgb(38, 42, 52),
            cloud: rgb(255, 214, 206),
            cloud_shade: rgb(214, 136, 160),
            night: 0.0,
        },
        // Bright day
        8..=16 => Sky {
            top: rgb(52, 118, 210),
            mid: rgb(112, 172, 236),
            horizon: rgb(206, 230, 246),
            sun: rgb(255, 252, 232),
            far: rgb(130, 164, 206),
            near: rgb(80, 122, 158),
            hill_top: rgb(92, 158, 92),
            hill_bottom: rgb(40, 92, 58),
            cloud: rgb(255, 255, 255),
            cloud_shade: rgb(196, 214, 238),
            night: 0.0,
        },
        // Starry night
        _ => Sky {
            top: rgb(7, 10, 32),
            mid: rgb(20, 32, 78),
            horizon: rgb(58, 70, 126),
            sun: rgb(232, 238, 255),
            far: rgb(40, 50, 92),
            near: rgb(24, 30, 62),
            hill_top: rgb(20, 38, 46),
            hill_bottom: rgb(8, 16, 22),
            cloud: rgb(96, 108, 150),
            cloud_shade: rgb(52, 62, 104),
            night: 1.0,
        },
    }
}

fn h01(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Mountain silhouette height (0..1) at x (0..1): ridged sine layers give sharp-ish peaks.
fn ridge_height(x: f32, seed: f32, sharp: f32) -> f32 {
    let r = |f: f32, ph: f32| 1.0 - (x * f + ph).sin().abs();
    let v = 0.55 * r(3.1, seed).powf(sharp) + 0.30 * r(7.7, seed * 1.9).powf(sharp) + 0.15 * (x * 19.0 + seed * 3.1).sin() * 0.5 + 0.075;
    v.clamp(0.0, 1.0)
}

fn mountains(p: &Painter, rect: Rect, base: f32, amp: f32, seed: f32, sharp: f32, top: Color32, bottom: Color32) {
    let n = 90u32;
    let mut m = Mesh::default();
    for i in 0..=n {
        let fx = i as f32 / n as f32;
        let x = rect.left() + fx * rect.width();
        let y = base - amp * ridge_height(fx, seed, sharp);
        m.colored_vertex(pos2(x, y), top);
        m.colored_vertex(pos2(x, rect.bottom()), bottom);
    }
    for i in 0..n {
        let a = 2 * i;
        m.add_triangle(a, a + 1, a + 2);
        m.add_triangle(a + 1, a + 3, a + 2);
    }
    p.add(Shape::mesh(m));
}

fn cloud(p: &Painter, c: Pos2, s: f32, light: Color32, shade: Color32) {
    // Puffy cumulus: a shaded underside, then bright puffs on top, flat-ish base.
    let puffs = [(0.0, 0.0, 0.62), (-0.62, 0.14, 0.44), (0.66, 0.12, 0.48), (-0.28, -0.26, 0.5), (0.32, -0.32, 0.56), (1.08, 0.24, 0.3), (-1.0, 0.26, 0.28)];
    for (dx, dy, r) in puffs {
        p.circle_filled(c + vec2(dx * s, dy * s + s * 0.08), r * s, shade);
    }
    for (dx, dy, r) in puffs {
        p.circle_filled(c + vec2(dx * s - s * 0.03, dy * s - s * 0.04), r * s * 0.94, light);
    }
}

pub fn paint_landscape(p: &Painter, rect: Rect, colors: [Color32; 3], t: f64, strength: f32) {
    let sky = sky_for_hour(crate::winstyle::local_hour());
    let (w, h) = (rect.width(), rect.height());
    let tf = t as f32;
    // A hint of the cover's color at the horizon ties the scene to the song.
    let horizon_col = lerp(sky.horizon, colors[0], 0.12);

    // Sky: three-stop gradient.
    let horizon_y = rect.top() + h * 0.62;
    let mid_y = rect.top() + h * 0.34;
    vgradient(p, Rect::from_min_max(rect.min, pos2(rect.right(), mid_y)), sky.top, sky.mid);
    vgradient(p, Rect::from_min_max(pos2(rect.left(), mid_y), pos2(rect.right(), horizon_y + 2.0)), sky.mid, horizon_col);
    p.rect_filled(Rect::from_min_max(pos2(rect.left(), horizon_y), rect.max), 0.0, horizon_col);

    // Stars (night): twinkling dots.
    if sky.night > 0.0 {
        for k in 0..140u32 {
            let x = rect.left() + w * h01(k, 11);
            let y = rect.top() + h * 0.55 * h01(k, 12).powf(1.4);
            let tw = 0.5 + 0.5 * (tf * (1.0 + 2.0 * h01(k, 13)) + k as f32).sin();
            p.circle_filled(pos2(x, y), 0.6 + 1.2 * h01(k, 14), Color32::WHITE.gamma_multiply(sky.night * (0.25 + 0.6 * tw)));
        }
    }

    // Sun / moon with a soft glow.
    let sun = pos2(rect.left() + w * 0.72, rect.top() + h * if sky.night > 0.0 { 0.18 } else { 0.30 });
    radial_blob(p, sun, h * 0.55, sky.sun.gamma_multiply(0.35));
    radial_blob(p, sun, h * 0.16, sky.sun.gamma_multiply(0.55));
    p.circle_filled(sun, h * 0.045, sky.sun);

    // Drifting clouds (wrap around the screen).
    for k in 0..6u32 {
        let s = h * (0.028 + 0.035 * h01(k, 21));
        let speed = 3.0 + 9.0 * h01(k, 22);
        let span = w + 6.0 * s;
        let x = rect.left() - 3.0 * s + (w * h01(k, 23) + tf * speed).rem_euclid(span);
        let y = rect.top() + h * (0.06 + 0.2 * h01(k, 24));
        let behind = lerp(sky.top, sky.mid, ((y - rect.top()) / (mid_y - rect.top())).clamp(0.0, 1.0));
        let a = 0.5 + 0.35 * h01(k, 25);
        cloud(p, pos2(x, y), s, lerp(behind, sky.cloud, a), lerp(behind, sky.cloud_shade, a));
    }

    // Mountains: far (hazy, tall), then nearer (darker).
    let far_top = lerp(sky.far, horizon_col, 0.1);
    mountains(p, rect, horizon_y + h * 0.02, h * 0.34, 1.3, 1.6, far_top, lerp(sky.far, horizon_col, 0.45));
    mountains(p, rect, horizon_y + h * 0.08, h * 0.20, 4.2, 1.2, sky.near, lerp(sky.near, sky.hill_bottom, 0.5));

    // Birds gliding across.
    for k in 0..3u32 {
        let x = rect.left() - 60.0 + ((tf * (26.0 + 8.0 * k as f32) + k as f32 * 420.0) % (w + 160.0));
        let y = rect.top() + h * (0.22 + 0.08 * k as f32) + (tf * 0.6 + k as f32).sin() * 12.0;
        let flap = (tf * 7.0 + k as f32 * 2.0).sin() * 4.0;
        let col = if sky.night > 0.0 { Color32::from_white_alpha(60) } else { Color32::from_black_alpha(140) };
        let st = Stroke::new(1.6_f32, col);
        p.line_segment([pos2(x - 7.0, y - flap), pos2(x, y)], st);
        p.line_segment([pos2(x, y), pos2(x + 7.0, y - flap)], st);
    }

    // Rolling hills / plain.
    let n = 120u32;
    let mut m = Mesh::default();
    for i in 0..=n {
        let fx = i as f32 / n as f32;
        let x = rect.left() + fx * w;
        let y = horizon_y + h * (0.16 + 0.035 * (fx * 5.0 + 0.7).sin() + 0.02 * (fx * 13.0).sin());
        m.colored_vertex(pos2(x, y), sky.hill_top);
        m.colored_vertex(pos2(x, rect.bottom()), sky.hill_bottom);
    }
    for i in 0..n {
        let a = 2 * i;
        m.add_triangle(a, a + 1, a + 2);
        m.add_triangle(a + 1, a + 3, a + 2);
    }
    p.add(Shape::mesh(m));

    // Grass swaying in the wind (foreground).
    let grass = lerp(sky.hill_bottom, sky.hill_top, 0.6);
    for k in 0..150u32 {
        let x = rect.left() + w * h01(k, 31);
        let depth = h01(k, 32); // 0 = far, 1 = near
        let y = rect.bottom() - h * 0.14 * (1.0 - depth);
        let len = 8.0 + 26.0 * depth;
        let sway = ((tf * 1.4 + x * 0.012).sin() * 0.7 + (tf * 2.3 + x * 0.03).sin() * 0.3) * len * 0.35;
        let col = lerp(grass, sky.hill_bottom, depth * 0.6).gamma_multiply(0.95);
        p.line_segment([pos2(x, y), pos2(x + sway, y - len)], Stroke::new(1.2 + 1.3 * depth, col));
    }

    // Readability veil: darker toward the bottom; lighter in immersive views (higher strength).
    let veil = (1.0 / strength.max(0.6)).min(1.0);
    p.rect_filled(rect, 0.0, Color32::from_black_alpha((70.0 * veil) as u8));
    vgradient(p, rect, Color32::from_black_alpha(0), Color32::from_black_alpha((120.0 * veil) as u8));
}
