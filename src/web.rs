//! Online music: unified search (Spotify, YouTube Music, YouTube) and a single hidden player.
//!
//! RAM strategy:
//! - YouTube / YouTube Music search use plain HTTPS requests (no browser).
//! - Spotify search uses a temporary hidden page that is closed as soon as results arrive.
//! - Playback uses ONE hidden page, created on first online play and dropped when a local file plays.
//! - All pages share one WebView2 profile with the GPU process disabled.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Source {
    Spotify,
    YtMusic,
    YouTube,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Spotify, Source::YtMusic, Source::YouTube];
    pub fn name(self) -> &'static str {
        match self {
            Source::Spotify => "Spotify",
            Source::YtMusic => "YouTube Music",
            Source::YouTube => "YouTube",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Source::Spotify => "sp:",
            Source::YtMusic => "ytm:",
            Source::YouTube => "yt:",
        }
    }
    fn play_url(self, id: &str) -> String {
        match self {
            Source::Spotify => format!("https://open.spotify.com/track/{id}"),
            _ => format!("https://www.youtube.com/embed/{id}?autoplay=1&rel=0&playsinline=1&iv_load_policy=3"),
        }
    }
}

/// Online tracks live in the library with a pseudo path like `sp:4uLU6hMCjMI75M1A2tKUQC`.
pub fn web_path(s: Source, id: &str) -> PathBuf {
    PathBuf::from(format!("{}{id}", s.prefix()))
}

pub fn parse_path(p: &Path) -> Option<(Source, String)> {
    let s = p.to_str()?;
    Source::ALL.into_iter().find_map(|src| s.strip_prefix(src.prefix()).map(|id| (src, id.to_string())))
}

pub struct Hit {
    pub source: Source,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: f32,
    pub art: Option<String>,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct PlayerState {
    pub pos: f32,
    pub dur: f32,
    pub paused: bool,
    pub ended: bool,
    pub ad: bool,
    /// The lightweight embed player refused this video (embedding disabled).
    pub error: bool,
}

pub enum WebMsg {
    Hits(Source, u64, Result<Vec<Hit>, String>),
    State(PlayerState),
    Imported(Result<(String, Vec<Hit>), String>),
}

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

// ---------------- YouTube / YouTube Music search (plain HTTPS) ----------------

fn innertube(host: &str, client: &str, version: &str, query: &str, params: Option<&str>) -> Result<Value, String> {
    let mut body = json!({
        "context": {"client": {"clientName": client, "clientVersion": version, "hl": "en", "gl": "US"}},
        "query": query,
    });
    if let Some(p) = params {
        body["params"] = p.into();
    }
    ureq::post(&format!("https://{host}/youtubei/v1/search?prettyPrint=false"))
        .set("User-Agent", UA)
        .set("Origin", &format!("https://{host}"))
        .set("Referer", &format!("https://{host}/"))
        .timeout(std::time::Duration::from_secs(15))
        .send_json(body)
        .map_err(|e| format!("Network error: {e}"))?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == key {
                    out.push(x);
                } else {
                    find_all(x, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, out)),
        _ => {}
    }
}

fn text(v: &Value) -> String {
    if let Some(s) = v["simpleText"].as_str() {
        return s.to_string();
    }
    v["runs"].as_array().map(|r| r.iter().filter_map(|x| x["text"].as_str()).collect()).unwrap_or_default()
}

pub fn parse_dur(s: &str) -> Option<f32> {
    let parts: Vec<u32> = s.trim().split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    if parts.len() < 2 {
        return None;
    }
    Some(parts.iter().fold(0u32, |acc, p| acc * 60 + p) as f32)
}

fn thumb(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")
}

/// Nuclear-style fallback: find the same song on YouTube Music (official audio) or YouTube.
/// Prefers a result whose length is within 10 s of the original, YouTube Music first.
pub fn find_stream(artist: &str, title: &str, duration: f32) -> Option<(Source, String)> {
    // Compare on letters/digits only, and ignore "(feat. …)" / "- Remastered" style suffixes.
    fn norm(s: &str) -> String {
        let s = s.split(" (").next().unwrap_or(s).split(" - ").next().unwrap_or(s).split(" [").next().unwrap_or(s);
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
    }
    let want = norm(title);
    let lead_artist = norm(artist.split(',').next().unwrap_or(artist));
    // Score: the title must match; then prefer the right artist and a similar length.
    let score = |h: &Hit| -> Option<i32> {
        let t = norm(&h.title);
        if want.is_empty() || !t.contains(&want) {
            return None;
        }
        let mut s = 0;
        if !lead_artist.is_empty() && (norm(&h.artist).contains(&lead_artist) || t.contains(&lead_artist)) {
            s += 10;
        }
        if duration > 0.0 {
            let diff = (h.duration - duration).abs();
            s += if diff <= 3.0 { 8 } else if diff <= 10.0 { 5 } else if diff <= 30.0 { 1 } else { -10 };
        }
        if h.source == Source::YtMusic {
            s += 2; // official audio, no video intro/outro
        }
        Some(s)
    };
    let q = format!("{artist} {title}");
    let mut hits = search_ytmusic(&q).unwrap_or_default();
    if !hits.iter().any(|h| score(h).map_or(false, |s| s >= 15)) {
        hits.extend(search_youtube(&q).unwrap_or_default());
    }
    hits.iter()
        .filter_map(|h| score(h).map(|s| (s, h)))
        .max_by_key(|(s, _)| *s)
        .map(|(_, h)| (h.source, h.id.clone()))
}

fn search_youtube(q: &str) -> Result<Vec<Hit>, String> {
    let v = innertube("www.youtube.com", "WEB", "2.20251101.01.00", q, Some("EgIQAQ%3D%3D"))?;
    let mut rs = vec![];
    find_all(&v, "videoRenderer", &mut rs);
    Ok(rs
        .into_iter()
        .filter_map(|r| {
            Some(Hit {
                source: Source::YouTube,
                id: r["videoId"].as_str()?.to_string(),
                title: text(&r["title"]),
                artist: text(&r["ownerText"]),
                duration: parse_dur(&text(&r["lengthText"]))?, // skips live streams
                art: r["videoId"].as_str().map(thumb),
            })
        })
        .take(25)
        .collect())
}

/// YouTube Music's own search API won't return songs without a login, so this finds the same official
/// tracks through YouTube: YouTube Music songs are published on auto-generated "Artist - Topic" channels.
fn search_ytmusic(q: &str) -> Result<Vec<Hit>, String> {
    let v = innertube("www.youtube.com", "WEB", "2.20251101.01.00", &format!("{q} auto-generated by youtube"), Some("EgIQAQ%3D%3D"))?;
    let mut rs = vec![];
    find_all(&v, "videoRenderer", &mut rs);
    Ok(rs
        .into_iter()
        .filter_map(|r| {
            let channel = text(&r["ownerText"]);
            let artist = channel.strip_suffix(" - Topic")?.to_string();
            Some(Hit {
                source: Source::YtMusic,
                id: r["videoId"].as_str()?.to_string(),
                title: text(&r["title"]),
                artist,
                duration: parse_dur(&text(&r["lengthText"]))?,
                art: r["videoId"].as_str().map(thumb),
            })
        })
        .take(20)
        .collect())
}

// ---------------- Hidden browser pages ----------------

const SPOTIFY_SEARCH_JS: &str = r#"
(function(){
  if (!location.hostname.includes('spotify') || !location.pathname.startsWith('/search')) return;
  var n = 0;
  var t = setInterval(function(){
    n++;
    var rows = Array.from(document.querySelectorAll('[data-testid="tracklist-row"]'));
    if (rows.length || n > 40) {
      clearInterval(t);
      var items = rows.slice(0, 30).map(function(r){
        var a = r.querySelector('a[href*="/track/"]');
        if (!a) return null;
        var artists = Array.from(r.querySelectorAll('a[href*="/artist/"]')).map(function(x){return x.textContent}).join(', ');
        var d = (r.innerText.match(/\b\d{1,2}:\d{2}\b/g) || []).pop() || '';
        var img = r.querySelector('img'); var art = img ? (img.getAttribute('src') || '') : '';
        return {id: a.getAttribute('href').split('/track/')[1].split('?')[0], title: a.textContent, artist: artists, dur: d, art: art};
      }).filter(Boolean);
      window.ipc.postMessage(JSON.stringify({kind: 'sp_hits', items: items}));
    }
  }, 500);
})();
"#;

const PLAYER_JS: &str = r#"
(function(){
  // Hop page: jump to the embed player so YouTube sees a referring page.
  if (location.pathname === '/robots.txt' && location.hash.indexOf('#wolf=') === 0) {
    var target = decodeURIComponent(location.hash.slice(6));
    if (document.readyState === 'loading') { document.addEventListener('DOMContentLoaded', function(){ location.href = target; }); }
    else { location.href = target; }
    return;
  }
  var R = window.__wolfbeat = {vol: 1, media: [], clicked: false};
  var op = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function(){
    if (R.media.indexOf(this) < 0) R.media.push(this);
    try { this.volume = R.vol; } catch(e) {}
    return op.apply(this, arguments);
  };
  var spotify = location.hostname.includes('spotify');
  function el(){
    var all = R.media.concat(Array.from(document.querySelectorAll('video,audio')));
    return all.find(function(m){return !m.paused}) || all[all.length-1];
  }
  function q(s){ return document.querySelector(s); }
  function secs(s){ if(!s) return 0; var v = s.split(':').reduce(function(a,p){return a*60+(+p)},0); return isNaN(v) ? 0 : v; }
  R.toggle = function(){
    if (spotify) { var b = q('[data-testid="control-button-playpause"]'); if (b) b.click(); return; }
    var m = el(); if (m) { m.paused ? m.play() : m.pause(); }
  };
  R.pause = function(){
    if (spotify) { var b = q('[data-testid="control-button-playpause"]'); if (b && b.getAttribute('aria-label') === 'Pause') b.click(); return; }
    var m = el(); if (m) m.pause();
  };
  R.seek = function(s){ var m = el(); if (m) m.currentTime = s; };
  R.setVol = function(v){ R.vol = v; R.media.concat(Array.from(document.querySelectorAll('video,audio'))).forEach(function(m){ try { m.volume = v; } catch(e) {} }); };
  var startTrack = spotify ? (location.pathname.split('/track/')[1] || '').split('?')[0] : (new URLSearchParams(location.search).get('v') || (location.pathname.split('/embed/')[1] || '').split('?')[0]);
  setInterval(function(){
    var s = {kind: 'state', pos: 0, dur: 0, paused: true, ended: false, ad: false, login: false};
    if (spotify) {
      if (!R.clicked) {
        var b = q('[data-testid="action-bar-row"] [data-testid="play-button"]');
        if (b) { R.clicked = true; R.clickedAt = Date.now(); b.click(); }
      }
      var pp = q('[data-testid="control-button-playpause"]');
      s.paused = !pp || pp.getAttribute('aria-label') !== 'Pause';
      s.pos = secs((q('[data-testid="playback-position"]') || {}).textContent);
      s.dur = secs((q('[data-testid="playback-duration"]') || {}).textContent);
      var now = q('[data-testid="now-playing-widget"] a[href*="/track/"]');
      var nowId = now ? (now.getAttribute('href').split('/track/')[1] || '').split('?')[0] : '';
      // Logged out, Spotify shows a "Preview of Spotify / Sign up free" bar and plays nothing
      // (no audio element ever starts, the time shows -:--), even though its button says "Pause".
      var timeText = ((q('[data-testid="playback-position"]') || {}).textContent || '').trim();
      var silent = R.media.length === 0 && (timeText === '' || timeText.indexOf('-') >= 0);
      s.login = R.clicked && Date.now() - R.clickedAt > 3000 && silent;
      if (s.login) s.paused = true;
      // Spotify moves on to its own recommendations at the end: treat a track change as "ended".
      s.ended = !s.login && !!(nowId && startTrack && nowId !== startTrack);
    } else {
      var mp = q('#movie_player');
      if (mp && mp.setPlaybackQualityRange && !R.lowq) { try { mp.setPlaybackQualityRange('tiny','tiny'); R.lowq = true; } catch(e) {} }
      var m = el();
      s.ad = !!q('.ad-showing');
      var err = q('.ytp-error');
      s.error = !!(err && err.offsetParent !== null && location.pathname.indexOf('/embed/') === 0);
      if (m) { s.pos = m.currentTime || 0; s.dur = isFinite(m.duration) ? m.duration : 0; s.paused = m.paused; s.ended = m.ended && !s.ad; }
      var v = new URLSearchParams(location.search).get('v');
      if (v && startTrack && v !== startTrack) { s.ended = true; if (m) m.pause(); }
    }
    window.ipc.postMessage(JSON.stringify(s));
  }, 500);
})();
"#;

/// Asks WebView2 to keep this page's memory as small as possible (it trims caches and working set).
fn set_low_memory(w: &WebView, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows::core::Interface;
    use wry::WebViewExtWindows;
    let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
    unsafe {
        if let Ok(core) = w.controller().CoreWebView2() {
            if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                let _ = c19.SetMemoryUsageTargetLevel(level);
            }
        }
    }
}

/// Accepts a Spotify playlist/album link (or `spotify:playlist:ID`) and returns its web page URL.
pub fn spotify_link(s: &str) -> Option<String> {
    let s = s.trim();
    for kind in ["playlist", "album"] {
        let id = s
            .split(&format!("/{kind}/"))
            .nth(1)
            .or_else(|| s.strip_prefix(&format!("spotify:{kind}:")))
            .map(|r| r.split(['?', '/', '#']).next().unwrap_or(""))
            .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()));
        if let Some(id) = id {
            return Some(format!("https://open.spotify.com/{kind}/{id}"));
        }
    }
    None
}

fn sp_items(v: &Value, cover: Option<String>) -> Vec<Hit> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Hit {
                        source: Source::Spotify,
                        id: x["id"].as_str()?.to_string(),
                        title: x["title"].as_str()?.to_string(),
                        artist: x["artist"].as_str().unwrap_or("").to_string(),
                        duration: parse_dur(x["dur"].as_str().unwrap_or("")).unwrap_or(0.0),
                        art: x["art"].as_str().filter(|s| s.starts_with("http")).map(String::from).or_else(|| cover.clone()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn offscreen() -> Rect {
    Rect { position: PhysicalPosition::new(-20, -20).into(), size: PhysicalSize::new(1u32, 1u32).into() }
}

/// Imports a Spotify playlist/album without a browser: Spotify's public embed page (the player
/// websites show) contains the full song list as JSON. No account or API key needed.
fn import_spotify(page_url: &str) -> Result<(String, Vec<Hit>), String> {
    let embed = page_url.replacen("open.spotify.com/", "open.spotify.com/embed/", 1);
    let html = ureq::get(&embed)
        .set("User-Agent", UA)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| format!("Couldn't reach Spotify: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let start_tag = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let data = html
        .split(start_tag)
        .nth(1)
        .and_then(|s| s.split("</script>").next())
        .ok_or("Spotify didn't return the song list (is the playlist public?)")?;
    let v: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    let e = &v["props"]["pageProps"]["state"]["data"]["entity"];
    let name = e["name"].as_str().or(e["title"].as_str()).unwrap_or("").to_string();
    // Albums have one cover for every song; playlist songs get their cover from YouTube once played.
    let is_album = e["type"].as_str() == Some("album");
    let cover = e["visualIdentity"]["image"]
        .as_array()
        .and_then(|imgs| imgs.iter().find(|i| i["maxWidth"].as_u64() == Some(300)).or(imgs.first()))
        .and_then(|i| i["url"].as_str())
        .map(String::from);
    let hits = e["trackList"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    Some(Hit {
                        source: Source::Spotify,
                        id: t["uri"].as_str()?.rsplit(':').next()?.to_string(),
                        title: t["title"].as_str()?.to_string(),
                        artist: t["subtitle"].as_str().unwrap_or("").to_string(),
                        duration: t["duration"].as_f64().unwrap_or(0.0) as f32 / 1000.0,
                        art: if is_album { cover.clone() } else { None },
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if hits.is_empty() {
        return Err("No songs found (is the playlist public?)".into());
    }
    Ok((name, hits))
}

pub fn thumb_for(id: &str) -> String {
    thumb(id)
}


pub fn enc(q: &str) -> String {
    q.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

enum Pending {
    SpotifySearch(String, u64),
    Play(String),
}

pub struct Engine {
    ctx: Option<WebContext>,
    player: Option<WebView>,
    search: Option<WebView>,
    pending: Vec<Pending>,
    spotify_gen: u64,
    player_low: Option<bool>,
    search_low: bool,
    tx: Sender<WebMsg>,
    pub rx: Receiver<WebMsg>,
    pub error: Option<String>,
}

impl Engine {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self { ctx: None, player: None, search: None, pending: vec![], spotify_gen: 0, player_low: None, search_low: false, tx, rx, error: None }
    }

    /// Starts a search on all three sources. `gen` lets the app ignore results from older searches.
    pub fn search(&mut self, q: &str, gen: u64, ctx: &egui::Context) {
        for (src, f) in [(Source::YtMusic, search_ytmusic as fn(&str) -> Result<Vec<Hit>, String>), (Source::YouTube, search_youtube)] {
            let (tx, q, ctx) = (self.tx.clone(), q.to_string(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(WebMsg::Hits(src, gen, f(&q)));
                ctx.request_repaint();
            });
        }
        self.search = None; // cancel any previous Spotify search page
        self.pending.retain(|p| !matches!(p, Pending::SpotifySearch(..)));
        self.pending.push(Pending::SpotifySearch(q.to_string(), gen));
    }

    /// Plays on the full YouTube page (used when a video can't be embedded).
    pub fn play_full_page(&mut self, id: &str) {
        self.pending.retain(|p| !matches!(p, Pending::Play(_)));
        self.pending.push(Pending::Play(format!("https://www.youtube.com/watch?v={id}")));
    }

    pub fn play(&mut self, src: Source, id: &str) {
        self.pending.retain(|p| !matches!(p, Pending::Play(_)));
        self.pending.push(Pending::Play(src.play_url(id)));
    }

    /// Frees the player page (and its memory) when online playback isn't needed.
    pub fn stop(&mut self) {
        self.pending.retain(|p| !matches!(p, Pending::Play(_)));
        self.player = None;
        if self.search.is_none() {
            self.ctx = None;
        }
    }

    pub fn is_active(&self) -> bool {
        self.player.is_some()
    }

    fn eval(&self, js: &str) {
        if let Some(p) = &self.player {
            let _ = p.evaluate_script(&format!("window.__wolfbeat && {js}"));
        }
    }
    pub fn toggle(&self) {
        self.eval("__wolfbeat.toggle()");
    }
    pub fn pause(&self) {
        self.eval("__wolfbeat.pause()");
    }
    pub fn seek(&self, s: f32) {
        self.eval(&format!("__wolfbeat.seek({s})"));
    }
    pub fn set_volume(&self, v: f32) {
        self.eval(&format!("__wolfbeat.setVol({v})"));
    }


    fn build(&mut self, parent: &impl HasWindowHandle, url: &str, script: &'static str, rect: Rect) -> Result<WebView, String> {
        let ctx = self.ctx.get_or_insert_with(|| {
            let dir = dirs::data_dir().unwrap_or_else(|| ".".into()).join("Wolfbeat").join("browser");
            WebContext::new(Some(dir))
        });
        let tx = self.tx.clone();
        WebViewBuilder::with_web_context(ctx)
            .with_url(url)
            .with_bounds(rect)
            .with_autoplay(true)
            // Low-RAM flags: no GPU, no images (nothing is visible anyway), one renderer process, no site isolation.
            .with_additional_browser_args(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,BackForwardCache --autoplay-policy=no-user-gesture-required \
                 --disable-gpu --disable-background-media-suspend --blink-settings=imagesEnabled=false \
                 --renderer-process-limit=1 --disable-site-isolation-trials --disable-extensions",
            )
            .with_initialization_script(script)
            .with_ipc_handler(move |req| {
                let Ok(v) = serde_json::from_str::<Value>(req.body()) else { return };
                if std::env::var_os("WOLFBEAT_DEBUG").is_some() {
                    eprintln!("[web] {}", req.body().chars().take(900).collect::<String>());
                }
                match v["kind"].as_str() {
                    Some("state") => {
                        let f = |k: &str| v[k].as_f64().unwrap_or(0.0) as f32;
                        let b = |k: &str| v[k].as_bool().unwrap_or(false);
                        let _ = tx.send(WebMsg::State(PlayerState {
                            pos: f("pos"),
                            dur: f("dur"),
                            paused: b("paused"),
                            ended: b("ended"),
                            ad: b("ad"),
                            error: b("error"),
                        }));
                    }
                    Some("sp_hits") => {
                        // generation is filled in by the app via spotify_done()
                        let _ = tx.send(WebMsg::Hits(Source::Spotify, u64::MAX, Ok(sp_items(&v["items"], None))));
                    }
                    _ => {}
                }
            })
            .build_as_child(parent)
            .map_err(|e| format!("Couldn't start the web player: {e}"))
    }

    /// Must be called every frame with the window: creates queued hidden pages and keeps them small.
    pub fn tick(&mut self, parent: &impl HasWindowHandle) {
        for p in std::mem::take(&mut self.pending) {
            let res = match p {
                Pending::SpotifySearch(q, gen) => {
                    self.spotify_gen = gen;
                    let url = format!("https://open.spotify.com/search/{}/tracks", enc(&q));
                    self.build(parent, &url, SPOTIFY_SEARCH_JS, offscreen()).map(|w| { self.search = Some(w); self.search_low = false; })
                }
                Pending::Play(url) => {
                    let ready = match &self.player {
                        Some(_) => Ok(()),
                        None => self.build(parent, "about:blank", PLAYER_JS, offscreen()).map(|w| {
                            self.player = Some(w);
                            self.player_low = None;
                        }),
                    };
                    ready.and_then(|_| {
                        let w = self.player.as_ref().unwrap();
                        if url.contains("/embed/") {
                            // YouTube's embed player refuses to play without a referring page (Error 153).
                            // Open a tiny YouTube page first; PLAYER_JS then jumps to the embed from there.
                            w.load_url(&format!("https://www.google.com/robots.txt#wolf={}", enc(&url))).map_err(|e| e.to_string())
                        } else {
                            w.load_url(&url).map_err(|e| e.to_string())
                        }
                    })
                }
            };
            if let Err(e) = res {
                self.error = Some(e);
            }
        }
        if let Some(s) = &self.search {
            if !self.search_low {
                set_low_memory(s, true);
                self.search_low = true;
            }
        }
        if let Some(p) = &self.player {
            if self.player_low != Some(true) {
                set_low_memory(p, true);
                self.player_low = Some(true);
            }
        }
    }

    /// Spotify results come back through the IPC handler without a generation; stamp and close the search page.
    pub fn spotify_done(&mut self) -> u64 {
        self.search = None;
        self.free_if_idle();
        self.spotify_gen
    }

    /// Reads a Spotify playlist/album on a background thread (plain HTTPS, no browser).
    pub fn import(&self, url: String, ctx: &egui::Context) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(WebMsg::Imported(import_spotify(&url)));
            ctx.request_repaint();
        });
    }

    fn free_if_idle(&mut self) {
        if self.player.is_none() && self.search.is_none() {
            self.ctx = None;
        }
    }
}

// ---------------- YouTube "Mix" (related songs radio) ----------------

/// Turns "Artist - Song (Official Video)" uploaded by "ArtistVEVO" into artist "Artist", title "Song".
fn tidy(mut h: Hit) -> Hit {
    let mut channel = h.artist.trim_end_matches(" - Topic").to_string();
    if let Some(stripped) = channel.strip_suffix("VEVO") {
        channel = stripped.trim().to_string();
    }
    let mut title = h.title.clone();
    for junk in [
        "(Official Music Video)", "(Official Video)", "(Official Audio)", "(Official Lyric Video)", "(Lyric Video)", "(Lyrics)", "(Audio)",
        "(Visualizer)", "[Official Video]", "[Official Music Video]", "[Official Audio]", "(Clip officiel)", "[Clip officiel]", "(Clip Officiel)",
    ] {
        if let Some(i) = title.to_lowercase().find(&junk.to_lowercase()) {
            title.replace_range(i..i + junk.len(), "");
        }
    }
    let title = title.trim().to_string();
    match title.split_once(" - ") {
        Some((a, s)) if !a.trim().is_empty() && !s.trim().is_empty() => {
            // Usually "Artist - Song"; some uploads use "Song - Artist": trust the side that matches the channel.
            let n = |x: &str| -> String { x.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect() };
            let (ch, na, ns) = (n(&channel), n(a), n(s));
            let swapped = !ch.is_empty() && ns.starts_with(&ch) && !na.starts_with(&ch);
            let (artist, song) = if swapped { (s, a) } else { (a, s) };
            h.artist = artist.trim().to_string();
            h.title = song.trim().to_string();
        }
        _ => {
            h.artist = channel;
            h.title = title;
        }
    }
    h
}

/// Songs YouTube would play after `video_id` (its "Mix" radio), without the seed itself.
pub fn youtube_mix(video_id: &str) -> Result<Vec<Hit>, String> {
    let body = json!({
        "context": {"client": {"clientName": "WEB", "clientVersion": "2.20251101.01.00", "hl": "en", "gl": "US"}},
        "videoId": video_id,
        "playlistId": format!("RD{video_id}"),
    });
    let v: Value = ureq::post("https://www.youtube.com/youtubei/v1/next?prettyPrint=false")
        .set("User-Agent", UA)
        .set("Origin", "https://www.youtube.com")
        .timeout(std::time::Duration::from_secs(15))
        .send_json(body)
        .map_err(|e| format!("Network error: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let mut rs = vec![];
    find_all(&v, "playlistPanelVideoRenderer", &mut rs);
    let mut seen = std::collections::HashSet::new();
    Ok(rs
        .into_iter()
        .filter_map(|r| {
            let id = r["videoId"].as_str()?.to_string();
            if id == video_id || !seen.insert(id.clone()) {
                return None;
            }
            let duration = parse_dur(&text(&r["lengthText"]))?; // skips live streams
            let channel = text(&r["shortBylineText"]);
            let source = if channel.ends_with(" - Topic") { Source::YtMusic } else { Source::YouTube };
            Some(tidy(Hit { source, art: Some(thumb(&id)), id, title: text(&r["title"]), artist: channel, duration }))
        })
        .collect())
}

/// The YouTube video to use as a radio seed for any song (resolving Spotify/local songs by search).
pub fn seed_video(path: &std::path::Path, artist: &str, title: &str, duration: f32) -> Option<String> {
    match parse_path(path) {
        Some((Source::YouTube | Source::YtMusic, id)) => Some(id),
        _ => find_stream(artist, title, duration).map(|(_, id)| id),
    }
}
