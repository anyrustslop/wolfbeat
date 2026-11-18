//! Previous / Play-Pause / Next buttons under Wolfbeat's taskbar preview (Windows "thumbnail toolbar"),
//! like Spotify. Clicks arrive as WM_COMMAND messages, which we catch with a window subclass.

use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    DefSubclassProc, ITaskbarList3, SetWindowSubclass, TaskbarList, THBF_ENABLED, THBN_CLICKED, THB_FLAGS, THB_ICON, THB_TOOLTIP, THUMBBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO, WM_COMMAND};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button {
    Prev = 0,
    PlayPause = 1,
    Next = 2,
}

static CLICKS: Mutex<Vec<Button>> = Mutex::new(Vec::new());
static CTX: OnceLock<egui::Context> = OnceLock::new();

unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_COMMAND && ((wp.0 >> 16) & 0xffff) as u32 == THBN_CLICKED {
        let b = match wp.0 & 0xffff {
            0 => Some(Button::Prev),
            1 => Some(Button::PlayPause),
            2 => Some(Button::Next),
            _ => None,
        };
        if let Some(b) = b {
            if let Ok(mut c) = CLICKS.lock() {
                c.push(b);
            }
            if let Some(ctx) = CTX.get() {
                ctx.request_repaint(); // wake the app even when it's minimized
            }
            return LRESULT(0);
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

/// Takes the clicks since the last call.
pub fn take_clicks() -> Vec<Button> {
    CLICKS.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default()
}

/// Renders a white glyph (4× supersampled) into a 32×32 icon.
fn make_icon(inside: impl Fn(f32, f32) -> bool) -> Option<HICON> {
    const N: usize = 32;
    let mut bgra = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let fx = (x as f32 + (sx as f32 + 0.5) / 4.0) / N as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / 4.0) / N as f32;
                    if inside(fx, fy) {
                        hits += 1;
                    }
                }
            }
            let a = (hits * 255 / 16) as u8;
            let i = (y * N + x) * 4;
            bgra[i..i + 4].copy_from_slice(&[a, a, a, a]); // premultiplied white
        }
    }
    let mask = vec![0u8; N * N / 8];
    unsafe {
        let color = CreateBitmap(N as i32, N as i32, 1, 32, Some(bgra.as_ptr() as *const _));
        let mask = CreateBitmap(N as i32, N as i32, 1, 1, Some(mask.as_ptr() as *const _));
        let info = ICONINFO { fIcon: BOOL(1), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color);
        let _ = DeleteObject(mask);
        icon
    }
}

fn tri(x: f32, y: f32, x0: f32, x1: f32, h: f32, right: bool) -> bool {
    // Triangle between x0..x1 pointing right (or left), height h centered at 0.5.
    if x < x0 || x > x1 {
        return false;
    }
    let t = if right { (x - x0) / (x1 - x0) } else { (x1 - x) / (x1 - x0) };
    (y - 0.5).abs() <= h / 2.0 * (1.0 - t)
}
fn bar(x: f32, y: f32, x0: f32, x1: f32, h: f32) -> bool {
    x >= x0 && x <= x1 && (y - 0.5).abs() <= h / 2.0
}

pub struct Taskbar {
    list: ITaskbarList3,
    hwnd: HWND,
    icons: [HICON; 4], // prev, play, pause, next
    added: bool,
    shown_playing: Option<bool>,
    tries: u32,
}

impl Taskbar {
    pub fn new(window: &impl HasWindowHandle, ctx: &egui::Context) -> Option<Self> {
        let RawWindowHandle::Win32(h) = window.window_handle().ok()?.as_raw() else { return None };
        let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
        let _ = CTX.set(ctx.clone());
        let list: ITaskbarList3 = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()? };
        unsafe {
            list.HrInit().ok()?;
            let _ = SetWindowSubclass(hwnd, Some(subclass), 0x5249_4646, 0);
        }
        let icons = [
            make_icon(|x, y| bar(x, y, 0.22, 0.31, 0.5) || tri(x, y, 0.31, 0.78, 0.5, false))?,
            make_icon(|x, y| tri(x, y, 0.30, 0.78, 0.56, true))?,
            make_icon(|x, y| bar(x, y, 0.27, 0.42, 0.52) || bar(x, y, 0.58, 0.73, 0.52))?,
            make_icon(|x, y| tri(x, y, 0.22, 0.69, 0.5, true) || bar(x, y, 0.69, 0.78, 0.5))?,
        ];
        Some(Self { list, hwnd, icons, added: false, shown_playing: None, tries: 0 })
    }

    fn button(&self, id: u32, icon: HICON, tip: &str) -> THUMBBUTTON {
        let mut b = THUMBBUTTON { dwMask: THB_ICON | THB_TOOLTIP | THB_FLAGS, iId: id, hIcon: icon, dwFlags: THBF_ENABLED, ..Default::default() };
        for (i, c) in tip.encode_utf16().take(259).enumerate() {
            b.szTip[i] = c;
        }
        b
    }

    /// Adds the buttons once the taskbar button exists, and swaps Play/Pause to match playback.
    pub fn sync(&mut self, playing: bool) {
        let play = if playing { self.button(1, self.icons[2], "Pause") } else { self.button(1, self.icons[1], "Play") };
        if !self.added {
            if self.tries > 600 {
                return; // taskbar unavailable (e.g. Explorer not running): stop trying
            }
            self.tries += 1;
            let buttons = [self.button(0, self.icons[0], "Previous"), play, self.button(2, self.icons[3], "Next")];
            if unsafe { self.list.ThumbBarAddButtons(self.hwnd, &buttons) }.is_ok() {
                self.added = true;
                if std::env::var_os("WOLFBEAT_DEBUG").is_some() {
                    eprintln!("[taskbar] thumbnail buttons added after {} tries", self.tries);
                }
                self.shown_playing = Some(playing);
            }
            return;
        }
        if self.shown_playing != Some(playing) && unsafe { self.list.ThumbBarUpdateButtons(self.hwnd, &[play]) }.is_ok() {
            self.shown_playing = Some(playing);
        }
    }
}
