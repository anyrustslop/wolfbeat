use crate::audio::Audio;
use crate::library::{fmt_dur, spawn_scan, Playlist, Repeat, ScanMsg, Store, Track};
use crate::lyrics::Lyrics;
use crate::web::{self, Engine, PlayerState, Source, WebMsg};
use egui::*;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

mod ui;


#[derive(Clone, Debug, PartialEq)]
enum View {
    Home,
    Songs,
    Liked,
    Playlist(u64),
    Queue,
    Folders,
    Search,
    Lyrics,
    NowPlaying,
    Settings,
    /// 0 = "Your Mix", n = "Because you listened to" mix n-1.
    Mix(usize),
}

#[derive(Clone, PartialEq)]
enum SearchState {
    Idle,
    Busy,
    Done(usize),
    Failed(String),
}

enum Action {
    Play { tracks: Vec<usize>, start: Option<usize>, label: String },
    PlayNext(usize),
    Enqueue(usize),
    ToggleLike(usize),
    AddToPlaylist(u64, usize),
    NewPlaylist(Option<usize>),
    RemoveFromPlaylist(u64, usize),
    DeletePlaylist(u64),
    RenamePlaylist(u64, String),
    Go(View),
    AddFolder,
    RemoveFolder(usize),
    Rescan,
    SearchOnline,
    ImportPlaylist,
    RefreshRecs,
    StartRadio(usize),
    SaveMix(usize),
    RemoveRecent(usize),
    ShowInExplorer(usize),
    TogglePlay,
    Next,
    Prev,
    Seek(f32),
    ToggleShuffle,
    CycleRepeat,
    JumpTo(usize),
    RemoveUpNext(usize),
}

enum RecMsg {
    Recs(crate::recs::Recs),
    Radio(Vec<web::Hit>, String),
}

pub struct App {
    store: Store,
    audio: Audio,
    by_path: HashMap<PathBuf, usize>,
    liked: HashSet<usize>,
    // playback
    context: Vec<usize>,
    order: Vec<usize>,
    pos: usize,
    up_next: VecDeque<usize>,
    current: Option<usize>,
    label: String,
    // ui
    view: View,
    search: String,
    selected: Option<usize>,
    renaming: Option<(u64, String)>,
    seek_drag: Option<f32>,
    toast: Option<(String, Instant)>,
    actions: Vec<Action>,
    // bookkeeping
    scan_rx: Option<Receiver<ScanMsg>>,
    scanning: Option<usize>,
    // online
    web: Engine,
    art: crate::art::ArtCache,
    via: Option<Source>,
    fb_tx: std::sync::mpsc::Sender<(usize, Option<(Source, String)>)>,
    fb_rx: Receiver<(usize, Option<(Source, String)>)>,
    ctx: Context,
    web_state: PlayerState,
    query: String,
    query_gen: u64,
    hits: HashMap<Source, Vec<usize>>,
    hit_state: HashMap<Source, SearchState>,
    filter: Option<Source>,
    import_link: String,
    import_open: bool,
    import_progress: Option<usize>,
    dirty: bool,
    last_save: Instant,
    anim_view: View,
    view_t0: f64,
    web_state_at: Instant,
    styled: bool,
    before_np: View,
    taskbar: Option<crate::taskbar::Taskbar>,
    /// YouTube id the hidden player is on; whether we already fell back to the full page.
    playing_id: Option<String>,
    full_page: bool,
    /// Hidden player freed after a long pause: where to resume.
    paused_since: Option<Instant>,
    resume_at: Option<f32>,
    mix: Vec<usize>,
    because: Vec<(String, Vec<usize>)>,
    recs_loading: bool,
    rec_tx: std::sync::mpsc::Sender<RecMsg>,
    rec_rx: Receiver<RecMsg>,
    aurora: [Color32; 3],
    lyrics: HashMap<usize, Lyrics>,
    lyric_line: Option<usize>,
    ly_tx: std::sync::mpsc::Sender<(usize, Lyrics)>,
    ly_rx: Receiver<(usize, Lyrics)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        crate::theme::setup(&cc.egui_ctx);
        let mut store = Store::load();
        // Forget online songs that aren't liked or in a playlist (search results from old sessions).
        let keep: HashSet<PathBuf> = store.liked.iter().chain(store.recent.iter()).chain(store.playlists.iter().flat_map(|p| p.tracks.iter())).cloned().collect();
        store.tracks.retain(|t| web::parse_path(&t.path).is_none() || keep.contains(&t.path));
        crate::theme::set_background(store.background);
        crate::theme::set_animate(store.animate);
        let audio = Audio::new(store.volume);
        let (fb_tx, fb_rx) = std::sync::mpsc::channel();
        let (ly_tx, ly_rx) = std::sync::mpsc::channel();
        let (rec_tx, rec_rx) = std::sync::mpsc::channel();
        let mut app = Self {
            audio,
            by_path: HashMap::new(),
            liked: HashSet::new(),
            context: vec![],
            order: vec![],
            pos: 0,
            up_next: VecDeque::new(),
            current: None,
            label: String::new(),
            view: View::Home,
            search: String::new(),
            selected: None,
            renaming: None,
            seek_drag: None,
            toast: None,
            actions: vec![],
            scan_rx: None,
            scanning: None,
            web: Engine::new(),
            art: crate::art::ArtCache::new(cc.egui_ctx.clone()),
            via: None,
            fb_tx,
            fb_rx,
            ctx: cc.egui_ctx.clone(),
            web_state: PlayerState::default(),
            query: String::new(),
            query_gen: 0,
            hits: HashMap::new(),
            hit_state: HashMap::new(),
            filter: None,
            import_link: String::new(),
            import_open: false,
            import_progress: None,
            dirty: false,
            last_save: Instant::now(),
            anim_view: View::Home,
            view_t0: 0.0,
            web_state_at: Instant::now(),
            styled: false,
            before_np: View::Home,
            taskbar: None,
            playing_id: None,
            full_page: false,
            paused_since: None,
            resume_at: None,
            mix: vec![],
            because: vec![],
            recs_loading: false,
            rec_tx,
            rec_rx,
            aurora: crate::theme::AURORA_DEFAULT,
            lyrics: HashMap::new(),
            lyric_line: None,
            ly_tx,
            ly_rx,
            store,
        };
        app.reindex();
        app.refresh_recs();
        match std::env::var("WOLFBEAT_TEST_VIEW").as_deref() {
            // testing aid: open on a given page
            Ok("liked") => app.view = View::Liked,
            Ok("queue") => app.view = View::Queue,
            Ok("folders") => app.view = View::Folders,
            Ok("songs") => app.view = View::Songs,
            Ok("lyrics") => app.view = View::Lyrics,
            Ok("settings") => app.view = View::Settings,
            Ok("mix") => app.view = View::Mix(0),
            _ => {}
        }
        if let Ok(link) = std::env::var("WOLFBEAT_TEST_IMPORT") {
            app.import_link = link; // testing aid
            app.actions.push(Action::ImportPlaylist);
        }
        if let Ok(q) = std::env::var("WOLFBEAT_TEST_SEARCH") {
            app.search = q; // testing aid: run an online search at startup
            app.actions.push(Action::SearchOnline);
        }
        if let Some(e) = app.audio.init_error.clone() {
            app.notify(e);
        }
        if !app.store.folders.is_empty() {
            app.rescan(&cc.egui_ctx); // picks up new/changed files quickly (tags are cached)
        }
        app
    }

    fn seed_of(&self, t: usize) -> crate::recs::Seed {
        let tr = &self.store.tracks[t];
        crate::recs::Seed { path: tr.path.clone(), title: tr.title.clone(), artist: tr.artist.clone(), duration: tr.duration }
    }

    /// Builds "Your Mix" + "Because you listened to" in the background from recent plays and likes.
    fn refresh_recs(&mut self) {
        if self.recs_loading {
            return;
        }
        let mut picks: Vec<usize> = vec![];
        let recent = self.store.recent.iter().filter_map(|p| self.by_path.get(p).copied());
        let liked = self.store.liked.iter().rev().filter_map(|p| self.by_path.get(p).copied());
        let mut keys = HashSet::new();
        for t in recent.take(3).chain(liked.take(4)) {
            let tr = &self.store.tracks[t];
            if keys.insert(crate::recs::song_key(&tr.artist, &tr.title)) {
                picks.push(t);
            }
            if picks.len() >= 5 {
                break;
            }
        }
        if picks.is_empty() {
            return;
        }
        // Songs you already know are left out of recommendations.
        let known: HashSet<String> = self
            .store
            .recent
            .iter()
            .chain(self.store.liked.iter())
            .filter_map(|p| self.by_path.get(p))
            .map(|&i| crate::recs::song_key(&self.store.tracks[i].artist, &self.store.tracks[i].title))
            .collect();
        let seeds: Vec<_> = picks.iter().map(|&t| self.seed_of(t)).collect();
        self.recs_loading = true;
        let (tx, ctx) = (self.rec_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(RecMsg::Recs(crate::recs::build(seeds, known)));
            ctx.request_repaint();
        });
    }

    /// Plays a radio of songs similar to `t` (YouTube Mix).
    fn start_radio(&mut self, t: usize) {
        let seed = self.seed_of(t);
        let label = format!("Radio · {}", seed.title);
        self.notify(format!("Starting radio based on {}", seed.title));
        let (tx, ctx) = (self.rec_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(RecMsg::Radio(crate::recs::radio(seed), label));
            ctx.request_repaint();
        });
    }

    fn poll_recs(&mut self) {
        while let Ok(msg) = self.rec_rx.try_recv() {
            match msg {
                RecMsg::Recs(r) => {
                    self.recs_loading = false;
                    self.mix = r.your_mix.into_iter().map(|h| self.intern(h)).collect();
                    self.because = r.because.into_iter().map(|(title, list)| (title, list.into_iter().map(|h| self.intern(h)).collect())).collect();
                }
                RecMsg::Radio(hits, label) => {
                    let ids: Vec<usize> = hits.into_iter().map(|h| self.intern(h)).collect();
                    if ids.is_empty() {
                        self.notify("Couldn't build a radio for this song");
                    } else {
                        self.play_context(ids, Some(0), label);
                    }
                }
            }
        }
    }

    /// Tracks and title of a mix page (0 = Your Mix).
    fn mix_list(&self, n: usize) -> (String, Vec<usize>) {
        if n == 0 {
            ("Your Mix".into(), self.mix.clone())
        } else {
            self.because.get(n - 1).map(|(t, l)| (format!("Because you listened to {t}"), l.clone())).unwrap_or_default()
        }
    }

    fn reindex(&mut self) {
        self.by_path = self.store.tracks.iter().enumerate().map(|(i, t)| (t.path.clone(), i)).collect();
        self.liked = self.store.liked.iter().filter_map(|p| self.by_path.get(p).copied()).collect();
    }

    fn is_web(&self, t: usize) -> bool {
        web::parse_path(&self.store.tracks[t].path).is_some()
    }

    fn cur_is_web(&self) -> bool {
        self.current.map_or(false, |c| self.is_web(c))
    }


    /// Looks up the same song on YouTube Music / YouTube in the background, then plays it.
    fn play_fallback(&mut self, t: usize) {
        let tr = &self.store.tracks[t];
        let (artist, title, dur) = (tr.artist.clone(), tr.title.clone(), tr.duration);
        let (tx, ctx) = (self.fb_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send((t, web::find_stream(&artist, &title, dur)));
            ctx.request_repaint();
        });
    }

    /// Fetches lyrics for the current song in the background (once per song).
    fn request_lyrics(&mut self) {
        let Some(c) = self.current else { return };
        if self.lyrics.contains_key(&c) {
            return;
        }
        self.lyrics.insert(c, Lyrics::Loading);
        let t = &self.store.tracks[c];
        let (artist, title, dur) = (t.artist.clone(), t.title.clone(), t.duration);
        let (tx, ctx) = (self.ly_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send((c, crate::lyrics::fetch(&artist, &title, dur)));
            ctx.request_repaint();
        });
    }

    fn poll_fallback(&mut self) {
        let dbg = std::env::var_os("WOLFBEAT_DEBUG").is_some();
        while let Ok((t, found)) = self.fb_rx.try_recv() {
            if dbg {
                eprintln!("[app] fallback for {:?} -> {:?} (current {:?})", self.store.tracks[t].title, found, self.current.map(|c| self.store.tracks[c].title.clone()));
            }
            if self.current != Some(t) {
                continue; // you moved on to another song meanwhile
            }
            match found {
                Some((src, id)) => {
                    self.via = Some(src);
                    self.playing_id = Some(id.clone());
                    if self.store.tracks[t].art.is_none() {
                        self.store.tracks[t].art = Some(web::thumb_for(&id)); // show the YouTube cover from now on
                        self.dirty = true;
                    }
                    self.web_state = PlayerState { paused: false, ..Default::default() };
                    self.web.play(src, &id);
                    self.web.set_volume(self.eff_volume());
                }
                None => {
                    self.notify("Couldn't find this song on YouTube");
                    self.next(true);
                }
            }
        }
    }

    fn playing(&self) -> bool {
        if self.cur_is_web() {
            self.web.is_active() && !self.web_state.paused
        } else {
            self.audio.is_playing()
        }
    }

    fn notify(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    fn rescan(&mut self, ctx: &Context) {
        if self.scan_rx.is_none() {
            self.scanning = Some(0);
            self.scan_rx = Some(spawn_scan(self.store.folders.clone(), self.store.tracks.clone(), ctx.clone()));
        }
    }

    fn start_search(&mut self, ctx: &Context) {
        let q = self.search.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.query = q.clone();
        self.query_gen += 1;
        self.hits.clear();
        for s in Source::ALL {
            self.hit_state.insert(s, SearchState::Busy);
        }
        self.web.search(&q, self.query_gen, ctx);
        self.view = View::Search;
    }

    /// Adds (or finds) an online song in the library so it works with likes, playlists and the queue.
    fn intern(&mut self, h: web::Hit) -> usize {
        let path = web::web_path(h.source, &h.id);
        if let Some(&i) = self.by_path.get(&path) {
            return i;
        }
        self.store.tracks.push(Track {
            path: path.clone(),
            title: h.title,
            artist: if h.artist.is_empty() { h.source.name().into() } else { h.artist },
            album: h.source.name().into(),
            track_no: None,
            duration: h.duration,
            mtime: 0,
            art: h.art,
        });
        let i = self.store.tracks.len() - 1;
        self.by_path.insert(path, i);
        i
    }

    fn poll_web(&mut self) {
        while let Ok(msg) = self.web.rx.try_recv() {
            match msg {
                WebMsg::Hits(src, gen, res) => {
                    let gen = if src == Source::Spotify { self.web.spotify_done() } else { gen };
                    if gen != self.query_gen {
                        continue; // result of an older search
                    }
                    match res {
                        Ok(hits) => {
                            let ids: Vec<usize> = hits.into_iter().map(|h| self.intern(h)).collect();
                            self.hit_state.insert(src, SearchState::Done(ids.len()));
                            if self.current.is_none() && std::env::var("WOLFBEAT_TEST_PLAY").ok().as_deref() == Some(src.name()) && !ids.is_empty() {
                                self.play_context(ids.clone(), Some(0), "Search".into()); // testing aid
                                if std::env::var("WOLFBEAT_TEST_VIEW").ok().as_deref() == Some("lyrics") {
                                    self.view = View::Lyrics;
                                }
                                if std::env::var("WOLFBEAT_TEST_VIEW").ok().as_deref() == Some("np") {
                                    self.before_np = View::Search;
                                    self.view = View::NowPlaying;
                                }
                            }
                            self.hits.insert(src, ids);
                        }
                        Err(e) => {
                            self.hit_state.insert(src, SearchState::Failed(e));
                        }
                    }
                }
                WebMsg::Imported(res) => {
                    self.import_progress = None;
                    let (name, hits) = match res {
                        Ok(r) => r,
                        Err(e) => {
                            self.notify(format!("Import failed: {e}"));
                            continue;
                        }
                    };
                    let n = hits.len();
                    let ids: Vec<usize> = hits.into_iter().map(|h| self.intern(h)).collect();
                    let tracks: Vec<PathBuf> = ids.iter().map(|&i| self.store.tracks[i].path.clone()).collect();
                    let id = self.store.next_id;
                    self.store.next_id += 1;
                    let name = if name.is_empty() { format!("Imported playlist #{id}") } else { name };
                    self.store.playlists.push(Playlist { id, name: name.clone(), tracks });
                    self.view = View::Playlist(id);
                    self.dirty = true;
                    self.notify(format!("Imported {n} songs into “{name}”"));
                    if std::env::var("WOLFBEAT_TEST_PLAY").ok().as_deref() == Some("import") {
                        self.play_context(ids, Some(0), name); // testing aid
                    }
                }
                WebMsg::State(s) => {
                    if !self.cur_is_web() {
                        continue;
                    }
                    if s.error && !self.full_page {
                        if let Some(id) = self.playing_id.clone() {
                            self.full_page = true; // this video can't be embedded: use the full YouTube page
                            self.web.play_full_page(&id);
                            continue;
                        }
                    }
                    if let Some(p) = self.resume_at {
                        if s.dur > 0.0 {
                            self.web.seek(p);
                            self.resume_at = None;
                        }
                    }
                    let was_ended = self.web_state.ended;
                    self.web_state = s;
                    self.web_state_at = Instant::now();
                    if s.ended && !was_ended {
                        self.next(false);
                    }
                }
            }
        }
        if let Some(e) = self.web.error.take() {
            self.notify(e);
        }
    }

    /// Creates the hidden pages the web engine has queued (they need the window handle).
    fn sync_web(&mut self, frame: &eframe::Frame) {
        self.web.tick(frame);
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan_rx else { return };
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                ScanMsg::Progress(n) => self.scanning = Some(n),
                ScanMsg::Done(t) => done = Some(t),
            }
        }
        if let Some(tracks) = done {
            // Remap playback state from old indices to new ones via paths.
            let path = |i: usize| self.store.tracks[i].path.clone();
            let cur = self.current.map(path);
            let ctx_paths: Vec<_> = self.context.iter().map(|&i| path(i)).collect();
            let up: Vec<_> = self.up_next.iter().map(|&i| path(i)).collect();
            let sel = self.selected.map(path);
            // Keep online songs (they aren't files on disk) at the end of the list.
            let online: Vec<Track> = self.store.tracks.iter().filter(|t| web::parse_path(&t.path).is_some()).cloned().collect();
            self.store.tracks = tracks;
            self.store.tracks.extend(online);
            self.reindex();
            let look = |p: &PathBuf| self.by_path.get(p).copied();
            self.current = cur.as_ref().and_then(look);
            let new_ctx: Vec<usize> = ctx_paths.iter().filter_map(look).collect();
            if new_ctx.len() != self.context.len() {
                self.order = (0..new_ctx.len()).collect();
                self.pos = self.current.and_then(|c| new_ctx.iter().position(|&x| x == c)).unwrap_or(0);
            }
            self.context = new_ctx;
            self.up_next = up.iter().filter_map(look).collect();
            self.selected = sel.as_ref().and_then(look);
            if self.current.is_none() {
                self.audio.stop();
            }
            self.scan_rx = None;
            self.scanning = None;
            self.dirty = true;
            let n = self.store.tracks.len();
            self.notify(format!("Library updated · {n} songs"));
        }
    }

    // ---------- playback ----------

    fn play_context(&mut self, tracks: Vec<usize>, start: Option<usize>, label: String) {
        if tracks.is_empty() {
            return;
        }
        self.context = tracks;
        self.label = label;
        self.up_next.clear();
        self.build_order(start);
        self.play_pos(self.pos);
    }

    /// `start` is an index into `context` that should play first.
    fn build_order(&mut self, start: Option<usize>) {
        let n = self.context.len();
        let mut order: Vec<usize> = (0..n).collect();
        if self.store.shuffle {
            order.shuffle(&mut rand::thread_rng());
            if let Some(s) = start {
                if let Some(i) = order.iter().position(|&x| x == s) {
                    order.swap(0, i);
                }
            }
            self.pos = 0;
        } else {
            self.pos = start.unwrap_or(0);
        }
        self.order = order;
    }

    fn play_pos(&mut self, pos: usize) {
        if let Some(&ci) = self.order.get(pos) {
            self.pos = pos;
            self.play_track(self.context[ci]);
        }
    }

    fn play_track(&mut self, t: usize) {
        self.current = Some(t);
        self.via = None;
        self.resume_at = None;
        self.full_page = false;
        self.paused_since = None;
        self.playing_id = None;
        self.lyric_line = None;
        self.request_lyrics();
        let p = self.store.tracks[t].path.clone();
        self.store.recent.retain(|x| *x != p);
        self.store.recent.insert(0, p);
        self.store.recent.truncate(40);
        self.dirty = true;
        let path = self.store.tracks[t].path.clone();
        if let Some((src, id)) = web::parse_path(&path) {
            self.audio.stop();
            self.web_state = PlayerState { paused: false, ..Default::default() };
            if src == Source::Spotify {
                self.play_fallback(t); // like Nuclear: Spotify is only for search; the audio comes from YouTube

                return;
            }
            self.playing_id = Some(id.clone());
            self.web.play(src, &id);
            self.web.set_volume(self.eff_volume());
            return;
        }
        self.web.stop(); // free the hidden browser's memory while playing local files
        if let Err(e) = self.audio.load(&path) {
            self.audio.stop();
            self.notify(e);
        }
    }

    fn eff_volume(&self) -> f32 {
        if self.audio.muted {
            0.0
        } else {
            self.audio.volume * self.audio.volume
        }
    }

    fn position(&self) -> f32 {
        if self.cur_is_web() {
            // The hidden player reports twice a second; glide between reports so the bar moves smoothly.
            let extra = if self.web_state.paused || self.web_state.dur == 0.0 { 0.0 } else { self.web_state_at.elapsed().as_secs_f32().min(1.0) };
            self.web_state.pos + extra
        } else {
            self.audio.position()
        }
    }

    fn seek(&self, s: f32) {
        if self.cur_is_web() {
            self.web.seek(s)
        } else {
            self.audio.seek(s)
        }
    }

    fn next(&mut self, manual: bool) {
        if std::env::var_os("WOLFBEAT_DEBUG").is_some() {
            eprintln!("[app] next(manual={manual}) from pos {} state {:?}", self.pos, self.web_state);
        }
        if !manual && self.store.repeat == Repeat::One {
            if let Some(c) = self.current {
                return self.play_track(c);
            }
        }
        if let Some(t) = self.up_next.pop_front() {
            return self.play_track(t);
        }
        if self.pos + 1 < self.order.len() {
            self.play_pos(self.pos + 1);
        } else if self.store.repeat != Repeat::Off && !self.order.is_empty() {
            if self.store.shuffle {
                self.build_order(None);
            }
            self.play_pos(0);
        } else if self.store.autoplay && !manual && self.current.is_some() {
            // Queue finished: keep going with similar songs, like YouTube Music.
            let c = self.current.unwrap();
            self.start_radio(c);
        } else {
            self.audio.stop();
            self.web.pause();
        }
    }

    fn prev(&mut self) {
        if self.position() > 3.0 || self.pos == 0 {
            self.seek(0.0);
        } else {
            self.play_pos(self.pos - 1);
        }
    }

    fn toggle_play(&mut self) {
        if self.cur_is_web() && self.web.is_active() {
            self.web.toggle();
            self.web_state.paused = !self.web_state.paused; // instant feedback; corrected by next state update
        } else if self.audio.has_track() && !self.audio.finished() {
            self.audio.toggle();
        } else if let Some(c) = self.current {
            let resume = self.resume_at;
            self.play_track(c);
            self.resume_at = resume; // continue where you paused
        } else {
            let all: Vec<usize> = (0..self.store.tracks.len()).filter(|&i| !self.is_web(i)).collect();
            self.play_context(all, None, "Songs".into());
        }
    }

    // ---------- actions ----------

    fn apply(&mut self, a: Action, ctx: &Context) {
        match a {
            Action::Play { tracks, start, label } => self.play_context(tracks, start, label),
            Action::PlayNext(t) => {
                self.up_next.push_front(t);
                self.notify("Playing next");
            }
            Action::Enqueue(t) => {
                self.up_next.push_back(t);
                self.notify("Added to queue");
            }
            Action::ToggleLike(t) => {
                let p = self.store.tracks[t].path.clone();
                if let Some(i) = self.store.liked.iter().position(|x| *x == p) {
                    self.store.liked.remove(i);
                    self.liked.remove(&t);
                    self.notify("Removed from Liked Songs");
                } else {
                    self.store.liked.push(p);
                    self.liked.insert(t);
                    self.notify("Added to Liked Songs");
                }
                self.dirty = true;
            }
            Action::AddToPlaylist(id, t) => {
                let p = self.store.tracks[t].path.clone();
                if let Some(pl) = self.store.playlists.iter_mut().find(|p| p.id == id) {
                    pl.tracks.push(p);
                    let msg = format!("Added to {}", pl.name);
                    self.notify(msg);
                    self.dirty = true;
                }
            }
            Action::NewPlaylist(with) => {
                let id = self.store.next_id;
                self.store.next_id += 1;
                let name = format!("My Playlist #{}", self.store.playlists.len() + 1);
                let tracks = with.map(|t| vec![self.store.tracks[t].path.clone()]).unwrap_or_default();
                self.store.playlists.push(Playlist { id, name: name.clone(), tracks });
                self.view = View::Playlist(id);
                self.renaming = Some((id, name));
                self.dirty = true;
            }
            Action::RemoveFromPlaylist(id, t) => {
                let p = &self.store.tracks[t].path;
                if let Some(pl) = self.store.playlists.iter_mut().find(|x| x.id == id) {
                    if let Some(i) = pl.tracks.iter().position(|x| x == p) {
                        pl.tracks.remove(i);
                        self.dirty = true;
                    }
                }
            }
            Action::DeletePlaylist(id) => {
                self.store.playlists.retain(|p| p.id != id);
                if self.view == View::Playlist(id) {
                    self.view = View::Home;
                }
                self.dirty = true;
            }
            Action::RenamePlaylist(id, name) => {
                if let Some(pl) = self.store.playlists.iter_mut().find(|x| x.id == id) {
                    if !name.trim().is_empty() {
                        pl.name = name.trim().to_string();
                    }
                }
                self.renaming = None;
                self.dirty = true;
            }
            Action::Go(v) => {
                if v == View::NowPlaying && self.view != View::NowPlaying {
                    self.before_np = self.view.clone();
                }
                self.view = v;
            }
            Action::SearchOnline => self.start_search(ctx),
            Action::RefreshRecs => {
                self.refresh_recs();
                self.notify("Refreshing your recommendations…");
            }
            Action::StartRadio(t) => self.start_radio(t),
            Action::SaveMix(n) => {
                let (name, list) = self.mix_list(n);
                let id = self.store.next_id;
                self.store.next_id += 1;
                let tracks = list.iter().map(|&i| self.store.tracks[i].path.clone()).collect();
                self.store.playlists.push(Playlist { id, name: name.clone(), tracks });
                self.dirty = true;
                self.notify(format!("Saved \u{201c}{name}\u{201d} to your playlists"));
            }
            Action::RemoveRecent(t) => {
                let p = self.store.tracks[t].path.clone();
                self.store.recent.retain(|x| *x != p);
                self.dirty = true;
            }
            Action::ImportPlaylist => match web::spotify_link(&self.import_link) {
                Some(url) => {
                    self.web.import(url, ctx);
                    self.import_progress = Some(0);
                    self.import_open = false;
                    self.import_link.clear();
                }
                None => self.notify("That doesn't look like a Spotify playlist or album link"),
            },
            Action::AddFolder => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Add a music folder").pick_folder() {
                    self.add_folder(dir, ctx);
                }
            }
            Action::RemoveFolder(i) => {
                if i < self.store.folders.len() {
                    self.store.folders.remove(i);
                    self.rescan(ctx);
                }
            }
            Action::Rescan => self.rescan(ctx),
            Action::ShowInExplorer(t) => {
                let path = self.store.tracks[t].path.clone();
                if let Some((src, id)) = web::parse_path(&path) {
                    let url = match src {
                        Source::Spotify => format!("https://open.spotify.com/track/{id}"),
                        Source::YtMusic => format!("https://music.youtube.com/watch?v={id}"),
                        Source::YouTube => format!("https://www.youtube.com/watch?v={id}"),
                    };
                    ctx.open_url(OpenUrl::new_tab(url));
                } else {
                    let p = path.display().to_string();
                    let _ = std::process::Command::new("explorer").arg(format!("/select,{p}")).spawn();
                }
            }
            Action::TogglePlay => self.toggle_play(),
            Action::Next => self.next(true),
            Action::Prev => self.prev(),
            Action::Seek(s) => self.seek(s),
            Action::ToggleShuffle => {
                self.store.shuffle = !self.store.shuffle;
                let cur = self.order.get(self.pos).copied();
                self.build_order(cur);
                self.dirty = true;
            }
            Action::CycleRepeat => {
                self.store.repeat = match self.store.repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
                self.dirty = true;
            }
            Action::JumpTo(pos) => self.play_pos(pos),
            Action::RemoveUpNext(i) => {
                self.up_next.remove(i);
            }
        }
    }

    fn add_folder(&mut self, dir: PathBuf, ctx: &Context) {
        if !self.store.folders.contains(&dir) {
            self.store.folders.push(dir);
            self.rescan(ctx);
        }
    }

    fn save(&mut self) {
        self.store.volume = self.audio.volume;
        if let Err(e) = self.store.save() {
            self.notify(format!("Couldn't save library: {e}"));
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }

}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.poll_scan();
        self.poll_web();
        self.poll_fallback();
        self.poll_recs();
        while let Ok((t, l)) = self.ly_rx.try_recv() {
            self.lyrics.insert(t, l);
        }
        self.art.poll();
        if self.hit_state.values().any(|s| *s == SearchState::Busy) || self.import_progress.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        // Free the hidden player's memory after 3 minutes paused (play resumes at the same second).
        if self.cur_is_web() && self.web.is_active() && self.web_state.paused && !self.web_state.ended {
            let since = *self.paused_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_secs(180) {
                self.resume_at = Some(self.web_state.pos);
                self.web.stop();
                self.paused_since = None;
            }
        } else {
            self.paused_since = None;
        }

        // Auto-advance when a local track ends (online tracks report "ended" via poll_web).
        if !self.cur_is_web() && self.audio.has_track() && self.audio.finished() {
            self.next(false);
        }

        // Drag & drop folders onto the window.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for p in dropped {
            let dir = if p.is_dir() { Some(p) } else { p.parent().map(|d| d.to_path_buf()) };
            if let Some(d) = dir {
                self.add_folder(d, ctx);
            }
        }

        // Keyboard shortcuts (when not typing).
        if ctx.memory(|m| m.focused().is_none()) {
            let (space, next, prev) = ctx.input(|i| {
                (
                    i.key_pressed(Key::Space),
                    i.modifiers.command && i.key_pressed(Key::ArrowRight),
                    i.modifiers.command && i.key_pressed(Key::ArrowLeft),
                )
            });
            if space {
                self.actions.push(Action::TogglePlay);
            }
            if next {
                self.actions.push(Action::Next);
            }
            if prev {
                self.actions.push(Action::Prev);
            }
        }

        self.draw(ctx);

        let prev_cur = self.current;
        for a in std::mem::take(&mut self.actions) {
            self.apply(a, ctx);
        }
        self.sync_web(frame);
        if !self.styled {
            crate::winstyle::style_window(frame);
            self.taskbar = crate::taskbar::Taskbar::new(frame, ctx);
            self.styled = true;
        }
        // Taskbar preview buttons: apply clicks, keep the Play/Pause icon in sync.
        for b in crate::taskbar::take_clicks() {
            if std::env::var_os("WOLFBEAT_DEBUG").is_some() {
                eprintln!("[taskbar] click {b:?}");
            }
            self.actions.push(match b {
                crate::taskbar::Button::Prev => Action::Prev,
                crate::taskbar::Button::PlayPause => Action::TogglePlay,
                crate::taskbar::Button::Next => Action::Next,
            });
        }
        let playing = self.playing();
        if let Some(tb) = self.taskbar.as_mut() {
            tb.sync(playing);
        }
        if self.current != prev_cur {
            self.lyric_line = None;
            self.request_lyrics();
            let title = self.current.map(|c| {
                let t = &self.store.tracks[c];
                format!("{} · {} — Wolfbeat", t.title, t.artist)
            });
            ctx.send_viewport_cmd(ViewportCommand::Title(title.unwrap_or_else(|| "Wolfbeat".into())));
        }

        if self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            self.save();
        }
        if self.playing() || self.web.is_active() || self.scanning.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
    }
}

