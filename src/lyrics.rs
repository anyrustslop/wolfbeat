//! Lyrics from LRCLIB (https://lrclib.net) — a free, open lyrics database. No account or key.
//! Only the song's title, artist and length are sent.

use serde_json::Value;

#[derive(Clone)]
pub enum Lyrics {
    Loading,
    /// (start time in seconds, line) — sorted by time.
    Synced(Vec<(f32, String)>),
    Plain(Vec<String>),
    Instrumental,
    NotFound,
}

const UA: &str = "Wolfbeat music player (Rust desktop app)";

/// "Get Lucky (Radio Edit) [feat. X]" → "Get Lucky"; "Artist - Song (Official Video)" → "Song".
fn clean_title(title: &str) -> String {
    let mut t = title.to_string();
    for sep in [" (", " [", " ft.", " feat.", " | "] {
        if let Some(i) = t.find(sep) {
            t.truncate(i);
        }
    }
    if let Some((_, rest)) = t.split_once(" - ") {
        t = rest.to_string(); // YouTube-style "Artist - Song"
    }
    t.trim().to_string()
}

fn first_artist(artist: &str) -> String {
    artist.split([',', '&']).next().unwrap_or(artist).trim_end_matches(" - Topic").trim().to_string()
}

fn get(url: &str, query: &[(&str, &str)]) -> Option<Value> {
    let mut req = ureq::get(url).set("User-Agent", UA).timeout(std::time::Duration::from_secs(12));
    for (k, v) in query {
        req = req.query(k, v);
    }
    req.call().ok()?.into_json().ok()
}

fn from_record(v: &Value) -> Option<Lyrics> {
    if v["instrumental"].as_bool() == Some(true) {
        return Some(Lyrics::Instrumental);
    }
    if let Some(s) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return Some(Lyrics::Synced(lines));
        }
    }
    let plain = v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
    Some(Lyrics::Plain(plain.lines().map(|l| l.trim().to_string()).collect()))
}

/// Looks up lyrics: exact match first (title + artist + length), then a search.
pub fn fetch(artist: &str, title: &str, duration: f32) -> Lyrics {
    let mut artist = first_artist(artist);
    if let Some((a, _)) = title.split_once(" - ") {
        if !a.trim().is_empty() {
            artist = first_artist(a); // "Daft Punk - Get Lucky" uploaded by some channel
        }
    }
    let title = clean_title(title);
    let dur = (duration.round() as u32).to_string();
    if duration > 0.0 {
        let exact = get("https://lrclib.net/api/get", &[("artist_name", &artist), ("track_name", &title), ("duration", &dur)]);
        if let Some(l) = exact.as_ref().and_then(from_record) {
            return l;
        }
    }
    let results = match get("https://lrclib.net/api/search", &[("track_name", &title), ("artist_name", &artist)]) {
        Some(Value::Array(r)) if !r.is_empty() => r,
        _ => match get("https://lrclib.net/api/search", &[("q", &format!("{artist} {title}"))]) {
            Some(Value::Array(r)) => r,
            _ => return Lyrics::NotFound,
        },
    };
    // Prefer a synced result with a similar length.
    let close = |v: &&Value| duration <= 0.0 || v["duration"].as_f64().map_or(true, |d| (d as f32 - duration).abs() <= 8.0);
    let synced = |v: &&Value| v["syncedLyrics"].as_str().map_or(false, |s| !s.is_empty());
    results
        .iter()
        .find(|v| close(v) && synced(v))
        .or_else(|| results.iter().find(close))
        .or_else(|| results.first())
        .and_then(from_record)
        .unwrap_or(Lyrics::NotFound)
}

/// Parses LRC text: "[01:23.45] line" (a line may carry several time tags).
pub fn parse_lrc(s: &str) -> Vec<(f32, String)> {
    let mut out = vec![];
    for line in s.lines() {
        let mut rest = line.trim();
        let mut times = vec![];
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            rest = stripped[end + 1..].trim_start();
            let mut parts = tag.split(':');
            if let (Some(m), Some(s)) = (parts.next(), parts.next()) {
                if let (Ok(m), Ok(s)) = (m.trim().parse::<f32>(), s.trim().parse::<f32>()) {
                    times.push(m * 60.0 + s);
                }
            }
        }
        for t in times {
            out.push((t, rest.to_string()));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}
