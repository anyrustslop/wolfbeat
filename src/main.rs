#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

mod app;
mod art;
mod audio;
mod library;
mod recs;
#[allow(dead_code)]
mod logo;
mod lyrics;
mod theme;
mod taskbar;
mod web;
mod winstyle;

/// Riff was renamed Wolfbeat: move the old library, settings and playlists over once.
fn migrate_from_riff() {
    if let Some(d) = dirs::data_dir() {
        let (old, new) = (d.join("Riff"), d.join("Wolfbeat"));
        if old.exists() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

fn main() -> eframe::Result<()> {
    migrate_from_riff();
    winstyle::join_job();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Wolfbeat")
            .with_decorations(false) // Wolfbeat draws its own glass title bar
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_drag_and_drop(true)
            .with_icon(std::sync::Arc::new(theme::window_icon())),
        ..Default::default()
    };
    eframe::run_native("Wolfbeat", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
