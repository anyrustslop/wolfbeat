use lofty::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::UNIX_EPOCH;

pub const AUDIO_EXTS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "mp4", "aac"];

pub fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(default)]
    pub track_no: Option<u32>,
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub mtime: u64,
    /// Cover image URL for online songs (local files use their embedded cover).
    #[serde(default)]
    pub art: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub tracks: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub folders: Vec<PathBuf>,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub liked: Vec<PathBuf>,
    /// Most recently played first (max 40).
    pub recent: Vec<PathBuf>,
    /// Background: 0 = scenery (anime landscape), 1 = aurora, 2 = plain dark.
    pub background: u8,
    pub animate: bool,
    /// Accent and background colors follow the current cover.
    pub cover_colors: bool,
    /// Show the live lyric line in Now Playing.
    pub np_lyrics: bool,
    /// When the queue ends, keep playing similar songs (radio).
    pub autoplay: bool,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub next_id: u64,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            folders: vec![],
            tracks: vec![],
            playlists: vec![],
            liked: vec![],
            recent: vec![],
            background: 0,
            animate: true,
            cover_colors: true,
            np_lyrics: true,
            autoplay: true,
            volume: 0.7,
            shuffle: false,
            repeat: Repeat::Off,
            next_id: 1,
        }
    }
}

fn store_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Wolfbeat")
        .join("library.json")
}

impl Store {
    pub fn load() -> Self {
        std::fs::read(store_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = store_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, p)
    }
}

pub fn read_track(p: &Path, mtime: u64) -> Track {
    let mut t = Track {
        path: p.to_path_buf(),
        title: p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        artist: "Unknown Artist".into(),
        album: "Unknown Album".into(),
        track_no: None,
        duration: 0.0,
        mtime,
        art: None,
    };
    if let Ok(tf) = lofty::read_from_path(p) {
        t.duration = tf.properties().duration().as_secs_f32();
        if let Some(tag) = tf.primary_tag().or_else(|| tf.first_tag()) {
            let set = |dst: &mut String, v: Option<std::borrow::Cow<str>>| {
                if let Some(v) = v {
                    let v = v.trim();
                    if !v.is_empty() {
                        *dst = v.to_string();
                    }
                }
            };
            set(&mut t.title, tag.title());
            set(&mut t.artist, tag.artist());
            set(&mut t.album, tag.album());
            t.track_no = tag.track();
        }
    }
    t
}

pub enum ScanMsg {
    Progress(usize),
    Done(Vec<Track>),
}

/// Scans folders on a background thread. Files whose modification time is unchanged reuse cached tags.
pub fn spawn_scan(folders: Vec<PathBuf>, old: Vec<Track>, ctx: egui::Context) -> Receiver<ScanMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let old: HashMap<PathBuf, Track> = old.into_iter().map(|t| (t.path.clone(), t)).collect();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for folder in &folders {
            for e in walkdir::WalkDir::new(folder).follow_links(true).into_iter().filter_map(|e| e.ok()) {
                let p = e.path();
                if !e.file_type().is_file() || !is_audio(p) || !seen.insert(p.to_path_buf()) {
                    continue;
                }
                let mtime = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                match old.get(p) {
                    Some(t) if t.mtime == mtime && mtime != 0 => out.push(t.clone()),
                    _ => out.push(read_track(p, mtime)),
                }
                if out.len() % 50 == 0 {
                    let _ = tx.send(ScanMsg::Progress(out.len()));
                    ctx.request_repaint();
                }
            }
        }
        out.sort_by(|a, b| {
            (a.artist.to_lowercase(), a.album.to_lowercase(), a.track_no, a.title.to_lowercase()).cmp(&(
                b.artist.to_lowercase(),
                b.album.to_lowercase(),
                b.track_no,
                b.title.to_lowercase(),
            ))
        });
        let _ = tx.send(ScanMsg::Done(out));
        ctx.request_repaint();
    });
    rx
}

pub fn fmt_dur(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
