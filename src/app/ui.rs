//! Everything Wolfbeat draws: sidebar, pages, track lists, player bar, toasts — with hover and page transitions.

use super::*;
use crate::theme::{self, icon, *};
use crate::theme::lerp;
use egui::text::{LayoutJob, TextFormat, TextWrapping};

const ROW_H: f32 = 56.0;
const PAD: f32 = 28.0; // page side padding

/// What to show in a page header's big cover tile.
enum Tile {
    Cover(Option<TextureHandle>, String),
    Liked,
    Icon(&'static str, Color32),
}

impl App {
    /// Lays out the whole window.
    pub(super) fn draw(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        if self.anim_view != self.view {
            self.anim_view = self.view.clone();
            self.view_t0 = now;
        }

        // Animated aurora behind everything, tinted by the current cover (colors glide when the song changes).
        let target = self
            .current
            .and_then(|c| art_key(&self.store.tracks[c]))
            .and_then(|k| self.art.palette(&k))
            .filter(|_| self.store.cover_colors)
            .unwrap_or(theme::AURORA_DEFAULT);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        for (c, t) in self.aurora.iter_mut().zip(target) {
            *c = lerp(*c, t, (dt * 1.6).min(1.0));
        }
        let focused = ctx.input(|i| i.focused);
        let strength = match self.view {
            View::Lyrics => 1.45,
            View::NowPlaying => 1.9,
            _ => 1.0,
        };
        theme::set_accent_from(self.aurora);
        theme::paint_background(&ctx.layer_painter(LayerId::background()), ctx.screen_rect(), self.aurora, now, strength);
        // ~30 fps while the window is focused; slow down in the background to save power.
        if theme::animate() || self.view == View::NowPlaying {
            ctx.request_repaint_after(Duration::from_millis(if focused { 33 } else { 250 }));
        }

        let glass = |m: Margin| Frame::none().fill(SIDEBAR).inner_margin(m).stroke(Stroke::new(1.0_f32, GLASS_EDGE));
        TopBottomPanel::top("titlebar").exact_height(38.0).frame(Frame::none().fill(SIDEBAR).stroke(Stroke::new(1.0_f32, GLASS_EDGE))).show(ctx, |ui| self.title_bar(ui));
        TopBottomPanel::bottom("player").exact_height(92.0).frame(glass(Margin::symmetric(18.0, 10.0))).show(ctx, |ui| self.player_bar(ui));

        if self.view != View::NowPlaying {
        SidePanel::left("sidebar")
            .exact_width(256.0)
            .resizable(false)
            .frame(glass(Margin { left: 14.0, right: 14.0, top: 18.0, bottom: 10.0 }))
            .show(ctx, |ui| self.sidebar(ui));
        }

        CentralPanel::default().frame(Frame::none()).show(ctx, |ui| {
            // Page transition: fade + short slide up.
            let t = ease_out(((now - self.view_t0) / 0.28) as f32);
            if t < 1.0 {
                ctx.request_repaint();
            }
            ui.set_opacity(0.2 + 0.8 * t);
            self.main_view(ui, (1.0 - t) * 14.0);
        });

        self.toast_overlay(ctx);
        resize_edges(ctx);
    }

    // ---------------- sidebar ----------------

    fn sidebar(&mut self, ui: &mut Ui) {
        // Logo + wordmark (the bars dance while music plays).
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
        let beat = self.playing().then(|| ui.input(|i| i.time));
        if beat.is_some() {
            ui.ctx().request_repaint();
        }
        theme::paint_logo(ui.painter(), Rect::from_min_size(r.min + vec2(4.0, 2.0), vec2(34.0, 34.0)), beat);
        ui.painter().text(pos2(r.left() + 50.0, r.center().y), Align2::LEFT_CENTER, "Wolfbeat", bold(24.0), TEXT);
        if resp.clicked() {
            self.actions.push(Action::Go(View::Search));
        }
        ui.add_space(20.0);

        let local = self.store.tracks.iter().filter(|t| web::parse_path(&t.path).is_none()).count();
        let queued = self.up_next.len() + self.order.len().saturating_sub(self.pos + 1);
        let items = [
            (View::Home, icon::HOME, "Home", String::new()),
            (View::Search, icon::SEARCH, "Search", String::new()),
            (View::Songs, icon::MUSIC, "Your Songs", local.to_string()),
            (View::Liked, icon::HEART_FILL, "Liked Songs", self.liked.len().to_string()),
            (View::Queue, icon::QUEUE, "Queue", queued.to_string()),
            (View::Folders, icon::FOLDER, "Music Folders", String::new()),
        ];
        for (v, ic, label, badge) in items {
            if nav_item(ui, ic, label, &badge, self.view == v).clicked() {
                self.actions.push(Action::Go(v));
            }
        }

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            caps(ui, "Playlists", DIM);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, icon::ADD, 15.0, SUBTLE, "New playlist").clicked() {
                    self.actions.push(Action::NewPlaylist(None));
                }
                let col = if self.import_open { accent() } else { SUBTLE };
                if icon_button(ui, icon::IMPORT, 15.0, col, "Import a Spotify playlist or album").clicked() {
                    self.import_open = !self.import_open;
                }
            });
        });
        ui.add_space(4.0);

        // Import box slides open.
        let open_t = ui.ctx().animate_bool_with_time(Id::new("import-open"), self.import_open || self.import_progress.is_some(), 0.18);
        if open_t > 0.0 {
            ui.set_opacity(open_t);
            Frame::none().fill(RAISED).rounding(12.0).inner_margin(10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.import_progress.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Importing from Spotify…").color(SUBTLE).size(13.0));
                    });
                    return;
                }
                ui.label(RichText::new("Paste a Spotify playlist or album link").color(SUBTLE).size(12.5));
                ui.add_space(2.0);
                let r = ui.add(
                    TextEdit::singleline(&mut self.import_link)
                        .hint_text("open.spotify.com/playlist/…")
                        .desired_width(f32::INFINITY)
                        .margin(vec2(8.0, 6.0)),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if accent_button(ui, "Import", 30.0).clicked() || enter {
                        self.actions.push(Action::ImportPlaylist);
                    }
                    if ghost_button(ui, "Cancel", 30.0).clicked() {
                        self.import_open = false;
                    }
                });
            });
            ui.set_opacity(1.0);
            ui.add_space(6.0);
        }

        ScrollArea::vertical().auto_shrink([false; 2]).max_height((ui.available_height() - 52.0).max(60.0)).id_source("pl").show(ui, |ui| {
            for pl in &self.store.playlists {
                let first = pl.tracks.iter().find_map(|p| self.by_path.get(p)).map(|&i| &self.store.tracks[i]);
                let tex = first.and_then(art_key).and_then(|k| self.art.get(&k));
                let active = self.view == View::Playlist(pl.id);
                let r = playlist_item(ui, pl.id, &pl.name, pl.tracks.len(), tex, active);
                if r.clicked() {
                    self.actions.push(Action::Go(View::Playlist(pl.id)));
                }
                r.context_menu(|ui| {
                    if menu_item(ui, icon::EDIT, "Rename") {
                        self.actions.push(Action::Go(View::Playlist(pl.id)));
                        self.renaming = Some((pl.id, pl.name.clone()));
                    }
                    if menu_item(ui, icon::DELETE, "Delete") {
                        self.actions.push(Action::DeletePlaylist(pl.id));
                    }
                });
            }
            if self.store.playlists.is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Create one with + or import from Spotify.").color(DIM).size(12.5));
                });
            }
        });
        // Bottom-left: settings.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            if nav_item(ui, icon::SETTINGS, "Settings", "", self.view == View::Settings).clicked() {
                self.actions.push(Action::Go(View::Settings));
            }
        });
    }

    // ---------------- pages ----------------

    fn main_view(&mut self, ui: &mut Ui, slide: f32) {
        ui.add_space(16.0 + slide);
        if self.view != View::NowPlaying {
            self.top_bar(ui);
        }
        ui.add_space(6.0);

        let q = self.search.trim().to_lowercase();
        let matches = |i: &usize| -> bool {
            if q.is_empty() || self.view == View::Search {
                return true;
            }
            let t = &self.store.tracks[*i];
            t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q) || t.album.to_lowercase().contains(&q)
        };

        match self.view.clone() {
            View::Songs => {
                if !self.store.tracks.iter().any(|t| web::parse_path(&t.path).is_none()) {
                    return self.empty_library(ui);
                }
                let list: Vec<usize> = (0..self.store.tracks.len()).filter(|&i| !self.is_web(i)).filter(matches).collect();
                let sub = songs_summary(&self.store, &list);
                self.hero(ui, "Library", "Your Songs", &sub, &list, "Songs", None, Tile::Icon(icon::MUSIC, Color32::from_rgb(92, 107, 255)));
                self.track_list(ui, &list, "Songs", None);
            }
            View::Liked => {
                let list: Vec<usize> = self.store.liked.iter().rev().filter_map(|p| self.by_path.get(p).copied()).filter(matches).collect();
                let sub = songs_summary(&self.store, &list);
                self.hero(ui, "Collection", "Liked Songs", &sub, &list, "Liked Songs", None, Tile::Liked);
                if list.is_empty() {
                    empty_hint(ui, icon::HEART, "Songs you like show up here", "Hover a song and click the heart.");
                } else {
                    self.track_list(ui, &list, "Liked Songs", None);
                }
            }
            View::Playlist(id) => {
                let Some(pl) = self.store.playlists.iter().find(|p| p.id == id).cloned() else {
                    self.view = View::Home;
                    return;
                };
                let list: Vec<usize> = pl.tracks.iter().filter_map(|p| self.by_path.get(p).copied()).filter(matches).collect();
                let tex = list.first().and_then(|&i| art_key(&self.store.tracks[i])).and_then(|k| self.art.get(&format!("big:{k}")).or_else(|| self.art.get(&k)));
                let sub = songs_summary(&self.store, &list);
                self.hero(ui, "Playlist", &pl.name, &sub, &list, &pl.name, Some(id), Tile::Cover(tex, pl.name.clone()));
                if list.is_empty() {
                    empty_hint(ui, icon::LIST, "This playlist is empty", "Right-click any song → Add to playlist.");
                } else {
                    self.track_list(ui, &list, &pl.name, Some(id));
                }
            }
            View::Queue => self.queue_view(ui),
            View::Folders => self.folders_view(ui),
            View::Search => self.search_view(ui),
            View::Home => self.home_view(ui),
            View::NowPlaying => self.now_playing_view(ui),
            View::Settings => self.settings_view(ui),
            View::Mix(n) => self.mix_view(ui, n),
            View::Lyrics => self.lyrics_view(ui),
        }
    }

    /// Big cover + synced lyrics: the current line lights up and scrolls to the middle; click a line to jump there.
    fn lyrics_view(&mut self, ui: &mut Ui) {
        let Some(c) = self.current else {
            return empty_hint(ui, icon::LYRICS, "Nothing playing", "Play a song to see its lyrics here.");
        };
        let t = self.store.tracks[c].clone();
        let area = ui.available_rect_before_wrap();
        let wide = area.width() > 900.0;
        let left_w = if wide { 360.0 } else { 0.0 };

        // Left column: large cover with title and artist.
        if wide {
            let cover = Rect::from_min_size(pos2(area.left() + PAD + 8.0, area.top() + 24.0), vec2(300.0, 300.0));
            shadow(ui.painter(), cover, 16.0, 1.6);
            let tex = art_key(&t).and_then(|k| self.art.get(&format!("big:{k}")).or_else(|| self.art.get(&k)));
            paint_cover(ui, cover, tex, &t.album, 16.0);
            let p = ui.painter();
            trunc(p, pos2(cover.left(), cover.bottom() + 30.0), &t.title, bold(22.0), TEXT, 300.0);
            trunc(p, pos2(cover.left(), cover.bottom() + 56.0), &t.artist, body(15.0), SUBTLE, 300.0);
            if let Some(src) = self.via {
                trunc(p, pos2(cover.left(), cover.bottom() + 80.0), &format!("via {}", src.name()), body(12.5), DIM, 300.0);
            }
        }

        let lyr_rect = Rect::from_min_max(pos2(area.left() + left_w + PAD, area.top()), pos2(area.right() - PAD, area.bottom()));
        self.lyrics_panel(ui, c, lyr_rect, 30.0);
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            let w = (ui.available_width() - PAD - 180.0).clamp(260.0, 520.0);
            let (rect, _) = ui.allocate_exact_size(vec2(w, 42.0), Sense::hover());
            let edit_id = Id::new("search-box");
            let focused = ui.ctx().memory(|m| m.has_focus(edit_id));
            let ft = ui.ctx().animate_bool_with_time(edit_id.with("focus"), focused, 0.15);
            let p = ui.painter();
            p.rect_filled(rect, 21.0, lerp(RAISED, HOVER, ft));
            p.rect_stroke(rect, 21.0, Stroke::new(1.0 + ft, lerp(LINE, accent(), ft)));
            p.text(pos2(rect.left() + 18.0, rect.center().y), Align2::LEFT_CENTER, icon::SEARCH, icons(15.0), lerp(DIM, TEXT, ft));
            let inner = Rect::from_min_max(pos2(rect.left() + 42.0, rect.top() + 4.0), pos2(rect.right() - 36.0, rect.bottom() - 4.0));
            let r = ui.put(
                inner,
                TextEdit::singleline(&mut self.search)
                    .id(edit_id)
                    .frame(false)
                    .hint_text(RichText::new("Search songs, artists… press Enter to search online").color(DIM))
                    .font(body(14.5))
                    .vertical_align(Align::Center)
                    .desired_width(f32::INFINITY),
            );
            if ui.input(|i| i.modifiers.command && i.key_pressed(Key::F)) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.search.trim().is_empty() {
                self.actions.push(Action::SearchOnline);
            }
            if !self.search.is_empty() {
                let cr = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), vec2(24.0, 24.0));
                let c = ui.interact(cr, Id::new("search-clear"), Sense::click());
                let col = if c.hovered() { TEXT } else { DIM };
                ui.painter().text(cr.center(), Align2::CENTER_CENTER, icon::CLOSE, icons(11.0), col);
                if c.clicked() {
                    self.search.clear();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(PAD);
                if let Some(n) = self.scanning {
                    Frame::none().fill(RAISED).rounding(14.0).inner_margin(Margin::symmetric(12.0, 6.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(format!("Scanning {n}")).color(SUBTLE).size(12.5));
                        });
                    });
                }
            });
        });
    }

    /// Big page header: tinted gradient, cover tile, title, stats and the play / shuffle row.
    #[allow(clippy::too_many_arguments)]
    fn hero(&mut self, ui: &mut Ui, kind: &str, title: &str, sub: &str, list: &[usize], label: &str, playlist: Option<u64>, tile: Tile) {
        let tint = match &tile {
            Tile::Liked => Color32::from_rgb(120, 70, 255),
            Tile::Icon(_, c) => *c,
            Tile::Cover(_, seed) => seed_color(seed),
        };
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 200.0), Sense::hover());
        let bg = Rect::from_min_max(pos2(rect.left(), rect.top() - 70.0), pos2(rect.right(), rect.bottom() + 90.0));
        vgradient(ui.painter(), bg, tint.linear_multiply(0.38), Color32::TRANSPARENT);

        let tile_r = Rect::from_min_size(pos2(rect.left() + PAD, rect.top() + 20.0), vec2(168.0, 168.0));
        shadow(ui.painter(), tile_r, 10.0, 1.0);
        match tile {
            Tile::Cover(tex, seed) => paint_cover(ui, tile_r, tex, &seed, 10.0),
            Tile::Liked => {
                gradient_fill(ui.painter(), tile_r, 10.0, Color32::from_rgb(120, 70, 255), Color32::from_rgb(70, 190, 255));
                ui.painter().text(tile_r.center(), Align2::CENTER_CENTER, icon::HEART_FILL, icons(56.0), Color32::WHITE);
            }
            Tile::Icon(ic, c) => {
                gradient_fill(ui.painter(), tile_r, 10.0, c, lerp(c, accent2(), 0.55));
                ui.painter().text(tile_r.center(), Align2::CENTER_CENTER, ic, icons(56.0), Color32::WHITE);
            }
        }

        let tx = tile_r.right() + 26.0;
        let max_w = rect.right() - tx - PAD;
        let p = ui.painter();
        caps_at(p, pos2(tx, tile_r.top() + 52.0), kind, SUBTLE);
        // Title (click to rename playlists).
        let renaming = playlist.and_then(|id| self.renaming.as_ref().filter(|(rid, _)| *rid == id).map(|_| id));
        if let Some(id) = renaming {
            let tr = Rect::from_min_size(pos2(tx, tile_r.top() + 66.0), vec2(max_w.min(520.0), 50.0));
            if let Some((_, name)) = self.renaming.as_mut() {
                let r = ui.put(tr, TextEdit::singleline(name).font(bold(32.0)).margin(vec2(8.0, 6.0)));
                r.request_focus();
                if r.lost_focus() {
                    self.actions.push(Action::RenamePlaylist(id, name.clone()));
                }
            }
        } else {
            let size = if title.chars().count() > 26 { 34.0 } else { 46.0 };
            let tr = trunc(p, pos2(tx, tile_r.top() + 92.0), title, bold(size), TEXT, max_w);
            if let Some(id) = playlist {
                let r = ui.interact(tr, Id::new(("rename", id)), Sense::click()).on_hover_text("Click to rename");
                if r.clicked() {
                    self.renaming = Some((id, title.to_string()));
                }
            }
        }
        trunc(ui.painter(), pos2(tx, tile_r.top() + 132.0), sub, body(13.5), SUBTLE, max_w);

        // Action row
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            let here = self.label == label && self.current.is_some();
            let playing_here = here && self.playing();
            if play_circle(ui, Id::new(("hero-play", label)), 54.0, playing_here).clicked() && !list.is_empty() {
                if here {
                    self.actions.push(Action::TogglePlay);
                } else {
                    self.actions.push(Action::Play { tracks: list.to_vec(), start: None, label: label.into() });
                }
            }
            ui.add_space(10.0);
            let col = if self.store.shuffle { accent() } else { SUBTLE };
            if icon_button(ui, icon::SHUFFLE, 20.0, col, "Shuffle").clicked() {
                self.actions.push(Action::ToggleShuffle);
            }
            if let Some(id) = playlist {
                if icon_button(ui, icon::DELETE, 17.0, SUBTLE, "Delete playlist").clicked() {
                    self.actions.push(Action::DeletePlaylist(id));
                }
            }
        });
        ui.add_space(14.0);
    }

    pub(super) fn search_view(&mut self, ui: &mut Ui) {
        if self.query.is_empty() {
            return empty_hint(ui, icon::SEARCH, "Search everything", "Type above and press Enter. Wolfbeat searches Spotify, YouTube Music and YouTube at once.");
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            ui.label(RichText::new(format!("Results for \u{201c}{}\u{201d}", self.query)).font(bold(30.0)).color(TEXT));
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            let all_n: usize = self.hits.values().map(|v| v.len()).sum();
            let mut chips: Vec<(Option<Source>, String)> = vec![(None, format!("All  {all_n}"))];
            for s in Source::ALL {
                let st = match self.hit_state.get(&s).cloned().unwrap_or(SearchState::Idle) {
                    SearchState::Busy => "…".to_string(),
                    SearchState::Done(n) => n.to_string(),
                    SearchState::Failed(_) => "!".to_string(),
                    SearchState::Idle => String::new(),
                };
                chips.push((Some(s), format!("{}  {st}", s.name())));
            }
            for (f, label) in chips {
                if chip(ui, &label, self.filter == f).clicked() {
                    self.filter = f;
                }
            }
            if self.hit_state.values().any(|s| *s == SearchState::Busy) {
                ui.add_space(4.0);
                ui.spinner();
            }
        });
        for s in Source::ALL {
            if let Some(SearchState::Failed(e)) = self.hit_state.get(&s) {
                if self.filter.is_none() || self.filter == Some(s) {
                    hint(ui, &format!("{}: {e}", s.name()));
                }
            }
        }
        ui.add_space(14.0);
        let order = match self.filter {
            Some(s) => vec![s],
            None => Source::ALL.to_vec(), // Spotify first, then YouTube Music, then YouTube
        };
        let list: Vec<usize> = order.iter().flat_map(|s| self.hits.get(s).cloned().unwrap_or_default()).collect();
        self.track_list(ui, &list, "Search", None);
    }

    fn track_list(&mut self, ui: &mut Ui, list: &[usize], label: &str, playlist: Option<u64>) {
        let w = ui.available_width();
        let left = ui.min_rect().left() + PAD - 8.0;
        let right = left + w - 2.0 * PAD + 16.0;
        let cols = Cols::new(left, right);

        let (hr, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let p = ui.painter();
        let cy = hr.center().y;
        p.text(pos2(cols.num, cy), Align2::CENTER_CENTER, "#", body(12.0), DIM);
        caps_at(p, pos2(cols.title, cy), "Title", DIM);
        caps_at(p, pos2(cols.album, cy), "Album", DIM);
        p.text(pos2(cols.dur, cy), Align2::RIGHT_CENTER, icon::CLOCK, icons(12.0), DIM);
        p.line_segment([pos2(left, hr.bottom()), pos2(right, hr.bottom())], Stroke::new(1.0_f32, LINE));
        ui.add_space(6.0);

        let time = ui.input(|i| i.time);
        let playing = self.playing();
        ScrollArea::vertical().auto_shrink([false; 2]).id_source(("list", label)).show_rows(ui, ROW_H, list.len(), |ui, range| {
            for row in range {
                let ti = list[row];
                let t = &self.store.tracks[ti];
                let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
                let r = Rect::from_min_max(pos2(left, r.top() + 2.0), pos2(right, r.bottom() - 2.0));
                let id = Id::new(("row", label, row));
                let ht = hover_t(ui, id, resp.hovered());
                let is_cur = self.current == Some(ti);
                let sel = self.selected == Some(ti);
                let base = if sel { SELECTED } else if is_cur { accent().gamma_multiply(0.07) } else { Color32::TRANSPARENT };
                let fill = lerp(base, if sel { SELECTED } else { HOVER }, ht);
                ui.painter().rect_filled(r, 10.0, fill);
                let cy = r.center().y;

                // number / equalizer / play
                let p = ui.painter();
                if ht > 0.5 {
                    p.text(pos2(cols.num, cy), Align2::CENTER_CENTER, if is_cur && playing { icon::PAUSE } else { icon::PLAY }, icons(13.0), TEXT);
                } else if is_cur && playing {
                    for i in 0..3 {
                        let h = 4.0 + 10.0 * ((time * [7.0, 9.0, 5.5][i] + [0.0, 1.7, 3.1][i]).sin().abs() as f32);
                        let x = cols.num - 6.5 + i as f32 * 5.0;
                        p.rect_filled(Rect::from_min_max(pos2(x, cy + 7.0 - h), pos2(x + 3.0, cy + 7.0)), 1.5, accent());
                    }
                } else {
                    p.text(pos2(cols.num, cy), Align2::CENTER_CENTER, (row + 1).to_string(), body(13.5), if is_cur { accent() } else { DIM });
                }

                let cover = Rect::from_min_size(pos2(cols.title, cy - 20.0), vec2(40.0, 40.0));
                let tex = art_key(t).and_then(|k| self.art.get(&k));
                paint_cover(ui, cover, tex, &t.album, 6.0);
                let p = ui.painter();
                let tx = cols.title + 54.0;
                let tw = cols.album - tx - 16.0;
                trunc(p, pos2(tx, cy - 9.0), &t.title, semibold(14.5), if is_cur { accent() } else { TEXT }, tw);
                trunc(p, pos2(tx, cy + 11.0), &t.artist, body(13.0), SUBTLE, tw);
                album_cell(p, pos2(cols.album, cy), &t.album, cols.heart - cols.album - 24.0);
                p.text(pos2(cols.dur, cy), Align2::RIGHT_CENTER, fmt_dur(t.duration), body(13.0), DIM);

                let liked = self.liked.contains(&ti);
                let hrect = Rect::from_center_size(pos2(cols.heart, cy), vec2(30.0, 30.0));
                let hresp = ui.interact(hrect, id.with("heart"), Sense::click());
                let alpha = if liked { 1.0 } else { ht };
                if alpha > 0.01 {
                    let col = if liked { accent() } else if hresp.hovered() { TEXT } else { SUBTLE };
                    ui.painter().text(hrect.center(), Align2::CENTER_CENTER, if liked { icon::HEART_FILL } else { icon::HEART }, icons(15.0), col.gamma_multiply(alpha));
                }
                if hresp.clicked() {
                    self.actions.push(Action::ToggleLike(ti));
                }

                if resp.clicked() {
                    self.selected = Some(ti);
                }
                let on_num = resp.interact_pointer_pos().map_or(false, |p| p.x < cols.num + 16.0);
                if resp.double_clicked() || (resp.clicked() && on_num) {
                    if is_cur && on_num {
                        self.actions.push(Action::TogglePlay);
                    } else {
                        self.actions.push(Action::Play { tracks: list.to_vec(), start: Some(row), label: label.into() });
                    }
                }
                resp.context_menu(|ui| {
                    self.selected = Some(ti);
                    if menu_item(ui, icon::PLAY, "Play") {
                        self.actions.push(Action::Play { tracks: list.to_vec(), start: Some(row), label: label.into() });
                    }
                    if menu_item(ui, icon::NEXT_UP, "Play next") {
                        self.actions.push(Action::PlayNext(ti));
                    }
                    if menu_item(ui, icon::QUEUE, "Add to queue") {
                        self.actions.push(Action::Enqueue(ti));
                    }
                    if menu_item(ui, icon::MUSIC, "Start radio") {
                        self.actions.push(Action::StartRadio(ti));
                    }
                    ui.separator();
                    if menu_item(ui, if liked { icon::HEART_FILL } else { icon::HEART }, if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" }) {
                        self.actions.push(Action::ToggleLike(ti));
                    }
                    ui.menu_button(RichText::new(format!("{}   Add to playlist", icon::ADD)), |ui| {
                        if menu_item(ui, icon::ADD, "New playlist") {
                            self.actions.push(Action::NewPlaylist(Some(ti)));
                        }
                        ui.separator();
                        for pl in &self.store.playlists {
                            if menu_item(ui, icon::LIST, &pl.name) {
                                self.actions.push(Action::AddToPlaylist(pl.id, ti));
                            }
                        }
                    });
                    if let Some(id) = playlist {
                        if menu_item(ui, icon::CLOSE, "Remove from this playlist") {
                            self.actions.push(Action::RemoveFromPlaylist(id, ti));
                        }
                    }
                    ui.separator();
                    let online = web::parse_path(&t.path).is_some();
                    if menu_item(ui, if online { icon::LINK } else { icon::FOLDER }, if online { "Open in browser" } else { "Show in Explorer" }) {
                        self.actions.push(Action::ShowInExplorer(ti));
                    }
                });
            }
            ui.add_space(24.0);
        });
    }

    fn queue_view(&mut self, ui: &mut Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            ui.label(RichText::new("Queue").font(bold(34.0)).color(TEXT));
        });
        ui.add_space(8.0);
        let mut rows: Vec<(Option<String>, usize, QueueKind)> = vec![];
        if let Some(c) = self.current {
            rows.push((Some("Now playing".into()), c, QueueKind::Current));
        }
        for (i, &t) in self.up_next.iter().enumerate() {
            rows.push(((i == 0).then(|| "Next in queue".into()), t, QueueKind::UpNext(i)));
        }
        for (k, p) in (self.pos + 1..self.order.len().min(self.pos + 101)).enumerate() {
            rows.push(((k == 0).then(|| format!("Next from {}", self.label)), self.context[self.order[p]], QueueKind::Context(p)));
        }
        if rows.is_empty() {
            return empty_hint(ui, icon::QUEUE, "Nothing queued", "Play something, or right-click a song → Add to queue.");
        }
        ScrollArea::vertical().auto_shrink([false; 2]).id_source("queue").show(ui, |ui| {
            for (section, ti, kind) in rows {
                if let Some(s) = section {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(PAD);
                        caps(ui, &s, SUBTLE);
                    });
                    ui.add_space(4.0);
                }
                let t = &self.store.tracks[ti];
                let tex = art_key(t).and_then(|k| self.art.get(&k));
                let r = compact_row(ui, Id::new(("q", ti, format!("{kind:?}"))), t, tex, kind == QueueKind::Current);
                match kind {
                    QueueKind::Context(p) if r.double_clicked() => self.actions.push(Action::JumpTo(p)),
                    QueueKind::UpNext(i) => {
                        r.context_menu(|ui| {
                            if menu_item(ui, icon::CLOSE, "Remove from queue") {
                                self.actions.push(Action::RemoveUpNext(i));
                            }
                        });
                    }
                    _ => {}
                }
            }
            ui.add_space(24.0);
        });
    }

    fn folders_view(&mut self, ui: &mut Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            ui.vertical(|ui| {
                ui.label(RichText::new("Music Folders").font(bold(34.0)).color(TEXT));
                ui.label(RichText::new("Wolfbeat plays MP3, FLAC, OGG, WAV, M4A and AAC files from these folders.").color(SUBTLE));
                ui.add_space(16.0);
                let w = (ui.available_width() - PAD).min(720.0);
                for (i, f) in self.store.folders.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
                    let ht = hover_t(ui, Id::new(("folder", i)), resp.hovered());
                    ui.painter().rect_filled(r, 12.0, lerp(RAISED, HOVER, ht));
                    ui.painter().text(pos2(r.left() + 20.0, r.center().y), Align2::CENTER_CENTER, icon::FOLDER, icons(17.0), accent());
                    trunc(ui.painter(), pos2(r.left() + 42.0, r.center().y), &f.display().to_string(), body(14.0), TEXT, w - 140.0);
                    let br = Rect::from_center_size(pos2(r.right() - 24.0, r.center().y), vec2(30.0, 30.0));
                    let b = ui.interact(br, Id::new(("rm-folder", i)), Sense::click()).on_hover_text("Remove folder");
                    ui.painter().text(br.center(), Align2::CENTER_CENTER, icon::DELETE, icons(14.0), if b.hovered() { TEXT } else { DIM });
                    if b.clicked() {
                        self.actions.push(Action::RemoveFolder(i));
                    }
                    ui.add_space(6.0);
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if accent_button(ui, &format!("{}  Add folder", icon::ADD), 38.0).clicked() {
                        self.actions.push(Action::AddFolder);
                    }
                    if self.scanning.is_none() && ghost_button(ui, &format!("{}  Rescan", icon::REFRESH), 38.0).clicked() {
                        self.actions.push(Action::Rescan);
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Tip: drag & drop a folder onto the window.").color(DIM).size(12.5));
            });
        });
    }

    fn empty_library(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.22).max(20.0));
            let (r, _) = ui.allocate_exact_size(vec2(88.0, 88.0), Sense::hover());
            shadow(ui.painter(), r.shrink(6.0), 24.0, 1.2);
            theme::paint_logo(ui.painter(), r, Some(ui.input(|i| i.time)));
            ui.ctx().request_repaint();
            ui.add_space(18.0);
            ui.label(RichText::new("Your music, all in one place").font(bold(28.0)).color(TEXT));
            ui.add_space(4.0);
            ui.label(RichText::new("Add a folder with music files, or search Spotify, YouTube Music and YouTube.").color(SUBTLE));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let w = 340.0;
                ui.add_space((ui.available_width() - w) / 2.0);
                if accent_button(ui, &format!("{}  Add a music folder", icon::FOLDER), 42.0).clicked() {
                    self.actions.push(Action::AddFolder);
                }
                if ghost_button(ui, &format!("{}  Search online", icon::SEARCH), 42.0).clicked() {
                    self.actions.push(Action::Go(View::Search));
                }
            });
        });
    }

    // ---------------- player bar ----------------

    fn player_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let third = rect.width() / 3.0;
        let cy = rect.center().y;

        if let Some(c) = self.current {
            let t = &self.store.tracks[c];
            let art = Rect::from_min_size(pos2(rect.left(), cy - 30.0), vec2(60.0, 60.0));
            let art_resp = ui.interact(art, Id::new("bar-cover"), Sense::click()).on_hover_text("Now Playing").on_hover_cursor(CursorIcon::PointingHand);
            if art_resp.clicked() {
                self.actions.push(Action::Go(if self.view == View::NowPlaying { self.before_np.clone() } else { View::NowPlaying }));
            }
            let glow = art_key(t).and_then(|k| self.art.palette(&k)).map(|p| p[0]).unwrap_or(accent());
            theme::radial_blob(ui.painter(), art.center(), 78.0, glow.gamma_multiply(0.55));
            shadow(ui.painter(), art, 8.0, 0.6);
            let tex = art_key(t).and_then(|k| self.art.get(&k));
            paint_cover(ui, art, tex, &t.album, 8.0);
            let tw = third - 130.0;
            let tr = trunc(ui.painter(), pos2(art.right() + 16.0, cy - 10.0), &t.title, semibold(15.0), TEXT, tw);
            let ar = trunc(ui.painter(), pos2(art.right() + 16.0, cy + 11.0), &t.artist, body(13.0), SUBTLE, tw - 110.0);
            if let Some(src) = self.via {
                let label = format!("via {}", src.name());
                let g = ui.painter().layout_no_wrap(label, body(11.0), SUBTLE);
                let pr = Rect::from_min_size(pos2(ar.right() + 8.0, cy + 11.0 - 9.0), vec2(g.size().x + 14.0, 18.0));
                ui.painter().rect_filled(pr, 9.0, RAISED);
                ui.painter().galley(pos2(pr.left() + 7.0, pr.center().y - g.size().y / 2.0), g, SUBTLE);
            }
            let liked = self.liked.contains(&c);
            let hr = Rect::from_center_size(pos2(tr.right() + 22.0, cy - 10.0), vec2(28.0, 28.0));
            let hresp = ui.interact(hr, Id::new("bar-heart"), Sense::click()).on_hover_text(if liked { "Remove from Liked Songs" } else { "Save to Liked Songs" });
            let col = if liked { accent() } else if hresp.hovered() { TEXT } else { SUBTLE };
            ui.painter().text(hr.center(), Align2::CENTER_CENTER, if liked { icon::HEART_FILL } else { icon::HEART }, icons(15.0), col);
            if hresp.clicked() {
                self.actions.push(Action::ToggleLike(c));
            }
        }

        // transport
        let cx = rect.center().x;
        let ctl_y = cy - 13.0;
        let playing = self.playing();
        let pr = Rect::from_center_size(pos2(cx, ctl_y), vec2(40.0, 40.0));
        let play = ui.interact(pr, Id::new("bar-play"), Sense::click());
        let ht = hover_t(ui, Id::new("bar-play"), play.hovered());
        let pressed = ui.ctx().animate_bool_with_time(Id::new("bar-play-press"), play.is_pointer_button_down_on(), 0.08);
        let rad = 18.0 + 1.5 * ht - 1.5 * pressed;
        ui.painter().circle_filled(pr.center(), rad, TEXT);
        ui.painter().text(pr.center() + vec2(if playing { 0.0 } else { 1.0 }, 0.0), Align2::CENTER_CENTER, if playing { icon::PAUSE } else { icon::PLAY }, icons(15.0), BG);
        if play.clicked() {
            self.actions.push(Action::TogglePlay);
        }
        let repeat_on = self.store.repeat != Repeat::Off;
        let buttons = [
            (-92.0, icon::SHUFFLE, self.store.shuffle, 0, "Shuffle"),
            (-48.0, icon::PREV, false, 1, "Previous"),
            (48.0, icon::NEXT, false, 2, "Next"),
            (92.0, if self.store.repeat == Repeat::One { icon::REPEAT_ONE } else { icon::REPEAT }, repeat_on, 3, "Repeat"),
        ];
        for (dx, glyph, active, act, tip) in buttons {
            let rr = Rect::from_center_size(pos2(cx + dx, ctl_y), vec2(32.0, 32.0));
            let resp = ui.interact(rr, Id::new(("bar-btn", act)), Sense::click()).on_hover_text(tip);
            let h = hover_t(ui, Id::new(("bar-btn", act)), resp.hovered());
            let base = if active { accent() } else { SUBTLE };
            ui.painter().text(rr.center(), Align2::CENTER_CENTER, glyph, icons(15.0 + h), if active { base } else { lerp(base, TEXT, h) });
            if active {
                ui.painter().circle_filled(pos2(rr.center().x, rr.center().y + 13.0), 2.0, accent());
            }
            if resp.clicked() {
                self.actions.push(match act {
                    0 => Action::ToggleShuffle,
                    1 => Action::Prev,
                    2 => Action::Next,
                    _ => Action::CycleRepeat,
                });
            }
        }

        // progress
        let mut dur = self.current.map(|c| self.store.tracks[c].duration).unwrap_or(0.0);
        if self.cur_is_web() && self.web_state.dur > 0.0 && !self.web_state.ad {
            dur = self.web_state.dur;
        }
        let pos = self.position().min(dur);
        let bar_w = (third * 1.25).min(600.0);
        let seek_y = cy + 24.0;
        let bar = Rect::from_center_size(pos2(cx, seek_y), vec2(bar_w, 4.0));
        let shown = self.seek_drag.map(|f| f * dur).unwrap_or(pos);
        let loading = self.cur_is_web() && self.web_state.dur == 0.0 && self.current.is_some();
        let left_label = if self.cur_is_web() && self.web_state.ad { "Ad".to_string() } else if loading { "···".into() } else { fmt_dur(shown) };
        ui.painter().text(pos2(bar.left() - 12.0, seek_y), Align2::RIGHT_CENTER, left_label, body(11.5), DIM);
        ui.painter().text(pos2(bar.right() + 12.0, seek_y), Align2::LEFT_CENTER, fmt_dur(dur), body(11.5), DIM);
        let frac = if dur > 0.0 { shown / dur } else { 0.0 };
        if let Some(new) = slider(ui, bar, Id::new("seek"), frac, &mut self.seek_drag, true) {
            if dur > 0.0 {
                self.actions.push(Action::Seek(new * dur));
            }
        }

        // right: queue + volume
        let vol_bar = Rect::from_min_size(pos2(rect.right() - 112.0, cy - 2.0), vec2(112.0, 4.0));
        let mut vdrag = None;
        let vol_frac = if self.audio.muted { 0.0 } else { self.audio.volume };
        let committed = slider(ui, vol_bar, Id::new("vol"), vol_frac, &mut vdrag, false);
        if let Some(v) = committed.or(vdrag) {
            self.audio.muted = false;
            self.audio.set_volume(v);
            self.web.set_volume(self.eff_volume());
            if committed.is_some() {
                self.dirty = true;
            }
        }
        let vr = Rect::from_center_size(pos2(vol_bar.left() - 22.0, cy), vec2(30.0, 30.0));
        let vresp = ui.interact(vr, Id::new("mute"), Sense::click()).on_hover_text("Mute");
        let vg = if self.audio.muted || self.audio.volume == 0.0 { icon::MUTE } else if self.audio.volume < 0.5 { icon::VOLUME_LOW } else { icon::VOLUME };
        let vh = hover_t(ui, Id::new("mute"), vresp.hovered());
        ui.painter().text(vr.center(), Align2::CENTER_CENTER, vg, icons(15.0), lerp(SUBTLE, TEXT, vh));
        if vresp.clicked() {
            self.audio.toggle_mute();
            self.web.set_volume(self.eff_volume());
        }
        let qr = Rect::from_center_size(pos2(vr.left() - 26.0, cy), vec2(30.0, 30.0));
        let qresp = ui.interact(qr, Id::new("queue"), Sense::click()).on_hover_text("Queue");
        let qh = hover_t(ui, Id::new("queue"), qresp.hovered());
        let qc = if self.view == View::Queue { accent() } else { lerp(SUBTLE, TEXT, qh) };
        ui.painter().text(qr.center(), Align2::CENTER_CENTER, icon::QUEUE, icons(15.0), qc);
        if qresp.clicked() {
            self.actions.push(Action::Go(if self.view == View::Queue { View::Home } else { View::Queue }));
        }
        let lr = Rect::from_center_size(pos2(qr.left() - 22.0, cy), vec2(30.0, 30.0));
        let lresp = ui.interact(lr, Id::new("lyrics-btn"), Sense::click()).on_hover_text("Lyrics");
        let lh = hover_t(ui, Id::new("lyrics-btn"), lresp.hovered());
        let lc = if self.view == View::Lyrics { accent() } else { lerp(SUBTLE, TEXT, lh) };
        ui.painter().text(lr.center(), Align2::CENTER_CENTER, icon::LYRICS, icons(15.0), lc);
        if lresp.clicked() {
            self.actions.push(Action::Go(if self.view == View::Lyrics { View::Home } else { View::Lyrics }));
        }
    }

    // ---------------- toast ----------------

    fn toast_overlay(&mut self, ctx: &Context) {
        let Some((msg, at)) = &self.toast else { return };
        let age = at.elapsed().as_secs_f32();
        let life = 2.6;
        if age > life {
            self.toast = None;
            return;
        }
        let t = ease_out((age / 0.22).min(1.0)) * (1.0 - ((age - (life - 0.3)) / 0.3).clamp(0.0, 1.0));
        ctx.request_repaint();
        Area::new(Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -108.0 + (1.0 - t) * 16.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(t);
                Frame::none()
                    .fill(MENU)
                    .stroke(Stroke::new(1.0_f32, GLASS_EDGE))
                    .rounding(14.0)
                    .inner_margin(Margin::symmetric(18.0, 11.0))
                    .shadow(epaint::Shadow { offset: vec2(0.0, 8.0), blur: 24.0, spread: 0.0, color: Color32::from_black_alpha(120) })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            gradient_fill(ui.painter(), dot, 4.0, accent(), accent2());
                            ui.label(RichText::new(msg.as_str()).color(TEXT).size(13.5));
                        });
                    });
            });
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum QueueKind {
    Current,
    UpNext(usize),
    Context(usize),
}

// ---------------- widgets ----------------

struct Cols {
    num: f32,
    title: f32,
    album: f32,
    heart: f32,
    dur: f32,
}

impl Cols {
    fn new(left: f32, right: f32) -> Self {
        let title = left + 44.0;
        let flex = right - title - 110.0;
        Self { num: left + 20.0, title, album: title + flex * 0.58, heart: right - 82.0, dur: right - 14.0 }
    }
}

pub(super) fn trunc(p: &Painter, left_center: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(max_w.max(10.0));
    let g = p.layout_job(job);
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - g.size().y / 2.0), g.size());
    p.galley(r.min, g, color);
    r
}

/// Small spaced-out uppercase label ("PLAYLISTS").
fn caps_job(text: &str, color: Color32) -> LayoutJob {
    let mut f = TextFormat::simple(semibold(11.0), color);
    f.extra_letter_spacing = 1.2;
    LayoutJob::single_section(text.to_uppercase(), f)
}
fn caps(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(caps_job(text, color));
}
fn caps_at(p: &Painter, left_center: Pos2, text: &str, color: Color32) {
    let g = p.layout_job(caps_job(text, color));
    p.galley(pos2(left_center.x, left_center.y - g.size().y / 2.0), g, color);
}

/// "Spotify", "YouTube Music" and "YouTube" get a small colored dot; local albums are plain text.
fn album_cell(p: &Painter, left_center: Pos2, album: &str, max_w: f32) {
    let dot = match album {
        "Spotify" => Some(Color32::from_rgb(30, 215, 96)),
        "YouTube Music" => Some(Color32::from_rgb(255, 70, 70)),
        "YouTube" => Some(Color32::from_rgb(255, 120, 120)),
        _ => None,
    };
    match dot {
        Some(c) => {
            p.circle_filled(pos2(left_center.x + 4.0, left_center.y), 3.5, c);
            trunc(p, pos2(left_center.x + 14.0, left_center.y), album, body(13.0), SUBTLE, max_w - 14.0);
        }
        None => {
            trunc(p, left_center, album, body(13.0), SUBTLE, max_w);
        }
    }
}

fn nav_item(ui: &mut Ui, ic: &str, label: &str, badge: &str, active: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let id = Id::new(("nav", label));
    let ht = hover_t(ui, id, resp.hovered());
    let at = ui.ctx().animate_bool_with_time(id.with("active"), active, 0.2);
    let p = ui.painter();
    p.rect_filled(r, 10.0, lerp(lerp(Color32::TRANSPARENT, HOVER, ht), SELECTED, at));
    if at > 0.0 {
        let h = 18.0 * at;
        let bar = Rect::from_center_size(pos2(r.left() + 2.0, r.center().y), vec2(3.0, h));
        gradient_fill(p, bar, 1.5, accent(), accent2());
    }
    let col = lerp(lerp(SUBTLE, TEXT, ht), TEXT, at);
    p.text(pos2(r.left() + 22.0, r.center().y), Align2::CENTER_CENTER, ic, icons(15.0), lerp(col, accent(), at));
    p.text(pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, label, if active { semibold(14.0) } else { body(14.0) }, col);
    if !badge.is_empty() && badge != "0" {
        p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, badge, body(12.0), DIM);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn playlist_item(ui: &mut Ui, id: u64, name: &str, n: usize, tex: Option<TextureHandle>, active: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
    let hid = Id::new(("pl-item", id));
    let ht = hover_t(ui, hid, resp.hovered());
    let at = ui.ctx().animate_bool_with_time(hid.with("active"), active, 0.2);
    ui.painter().rect_filled(r, 10.0, lerp(lerp(Color32::TRANSPARENT, HOVER, ht), SELECTED, at));
    let cover = Rect::from_min_size(pos2(r.left() + 7.0, r.center().y - 20.0), vec2(40.0, 40.0));
    paint_cover(ui, cover, tex, name, 6.0);
    let w = r.width() - 64.0;
    let p = ui.painter();
    trunc(p, pos2(r.left() + 58.0, r.center().y - 9.0), name, semibold(13.5), if active { accent() } else { TEXT }, w);
    trunc(p, pos2(r.left() + 58.0, r.center().y + 10.0), &format!("Playlist · {n} songs"), body(12.0), DIM, w);
    resp
}

fn compact_row(ui: &mut Ui, id: Id, t: &crate::library::Track, tex: Option<TextureHandle>, current: bool) -> Response {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::click());
    let r = Rect::from_min_max(pos2(r.left() + PAD - 8.0, r.top() + 2.0), pos2((r.left() + PAD - 8.0 + 900.0).min(r.right() - PAD + 8.0), r.bottom() - 2.0));
    let ht = hover_t(ui, id, resp.hovered());
    ui.painter().rect_filled(r, 10.0, lerp(Color32::TRANSPARENT, HOVER, ht));
    let cover = Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 19.0), vec2(38.0, 38.0));
    paint_cover(ui, cover, tex, &t.album, 6.0);
    let p = ui.painter();
    let tw = r.width() - 140.0;
    trunc(p, pos2(cover.right() + 14.0, r.center().y - 9.0), &t.title, semibold(14.0), if current { accent() } else { TEXT }, tw);
    trunc(p, pos2(cover.right() + 14.0, r.center().y + 10.0), &t.artist, body(12.5), SUBTLE, tw);
    p.text(pos2(r.right() - 14.0, r.center().y), Align2::RIGHT_CENTER, fmt_dur(t.duration), body(12.5), DIM);
    resp
}

fn icon_button(ui: &mut Ui, ic: &str, size: f32, color: Color32, tip: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size + 16.0, size + 16.0), Sense::click());
    let ht = hover_t(ui, resp.id, resp.hovered());
    ui.painter().rect_filled(r, 8.0, lerp(Color32::TRANSPARENT, HOVER, ht));
    ui.painter().text(r.center(), Align2::CENTER_CENTER, ic, icons(size), if color == SUBTLE { lerp(SUBTLE, TEXT, ht) } else { color });
    resp.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand)
}

fn play_circle(ui: &mut Ui, id: Id, size: f32, playing: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let ht = hover_t(ui, id, resp.hovered());
    let s = size * (0.94 + 0.06 * ht);
    let c = Rect::from_center_size(r.center(), vec2(s, s));
    shadow(ui.painter(), c, s / 2.0, 0.5 + 0.5 * ht);
    gradient_fill(ui.painter(), c, s / 2.0, accent(), accent2());
    ui.painter().text(c.center() + vec2(if playing { 0.0 } else { 1.5 }, 0.0), Align2::CENTER_CENTER, if playing { icon::PAUSE } else { icon::PLAY }, icons(size * 0.36), Color32::WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn accent_button(ui: &mut Ui, text: &str, h: f32) -> Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), semibold(14.0), Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, h), Sense::click());
    let ht = hover_t(ui, resp.id, resp.hovered());
    let rr = r.expand(ht * 1.5);
    gradient_fill(ui.painter(), rr, h / 2.0, lerp(accent(), Color32::WHITE, ht * 0.08), accent2());
    ui.painter().galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn ghost_button(ui: &mut Ui, text: &str, h: f32) -> Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), semibold(14.0), TEXT);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 32.0, h), Sense::click());
    let ht = hover_t(ui, resp.id, resp.hovered());
    ui.painter().rect_filled(r, h / 2.0, lerp(RAISED, HOVER, ht));
    ui.painter().rect_stroke(r, h / 2.0, Stroke::new(1.0_f32, lerp(LINE, SUBTLE, ht)));
    ui.painter().galley(r.center() - g.size() / 2.0, g, TEXT);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let fg = if on { BG } else { TEXT };
    let g = ui.painter().layout_no_wrap(text.to_string(), semibold(13.0), fg);
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 26.0, 32.0), Sense::click());
    let ht = hover_t(ui, resp.id, resp.hovered());
    let at = ui.ctx().animate_bool_with_time(resp.id.with("on"), on, 0.15);
    ui.painter().rect_filled(r, 16.0, lerp(lerp(RAISED, HOVER, ht), TEXT, at));
    ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

fn menu_item(ui: &mut Ui, ic: &str, label: &str) -> bool {
    let clicked = ui.button(RichText::new(format!("{ic}   {label}"))).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// Thin slider that grows on hover; gradient fill. Returns the value on release; `drag` holds the live value.
fn slider(ui: &mut Ui, bar: Rect, id: Id, frac: f32, drag: &mut Option<f32>, gradient: bool) -> Option<f32> {
    let resp = ui.interact(bar.expand2(vec2(6.0, 10.0)), id, Sense::click_and_drag());
    let live = resp
        .interact_pointer_pos()
        .filter(|_| resp.is_pointer_button_down_on() || resp.dragged())
        .map(|p| ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0));
    if live.is_some() {
        *drag = live;
    }
    let ht = hover_t(ui, id, resp.hovered() || resp.dragged());
    let shown = drag.unwrap_or(frac).clamp(0.0, 1.0);
    let thick = 4.0 + 2.0 * ht;
    let track = Rect::from_center_size(bar.center(), vec2(bar.width(), thick));
    let p = ui.painter();
    p.rect_filled(track, thick / 2.0, Color32::from_rgb(52, 56, 70));
    let filled = Rect::from_min_max(track.min, pos2(track.left() + track.width() * shown, track.bottom()));
    if filled.width() > 0.5 {
        if gradient || ht > 0.0 {
            let base = if gradient { accent() } else { lerp(TEXT, accent(), ht) };
            let end = if gradient { accent2() } else { lerp(TEXT, accent2(), ht) };
            gradient_fill(p, filled, thick / 2.0, base, end);
        } else {
            p.rect_filled(filled, thick / 2.0, TEXT);
        }
    }
    if ht > 0.01 {
        p.circle_filled(pos2(filled.right(), track.center().y), 6.5 * ht, Color32::WHITE);
    }
    if resp.drag_stopped() || resp.clicked() {
        return drag.take().or(live);
    }
    None
}

pub(super) fn hint(ui: &mut Ui, s: &str) {
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.label(RichText::new(s).color(SUBTLE));
    });
}

fn empty_hint(ui: &mut Ui, ic: &str, title: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.18).max(16.0));
        let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 36.0, RAISED);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, ic, icons(26.0), SUBTLE);
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(bold(20.0)).color(TEXT));
        ui.label(RichText::new(sub).color(SUBTLE));
    });
}

fn songs_summary(store: &Store, list: &[usize]) -> String {
    let secs: f32 = list.iter().map(|&i| store.tracks[i].duration).sum();
    let mins = (secs / 60.0).round() as u32;
    let total = if mins >= 60 { format!("{} hr {} min", mins / 60, mins % 60) } else { format!("{mins} min") };
    format!("{} songs · {total}", list.len())
}

pub(super) fn seed_color(s: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    Color32::from(ecolor::Hsva::new((h % 360) as f32 / 360.0, 0.55, 0.62, 1.0))
}

/// Where a song's cover comes from: its online image, or the picture embedded in a local file.
pub(super) fn art_key(t: &crate::library::Track) -> Option<String> {
    if let Some(u) = &t.art {
        return Some(u.clone());
    }
    web::parse_path(&t.path).is_none().then(|| format!("file:{}", t.path.display()))
}

/// Draws a cover, or a soft gradient tile with a note while it loads / if there's none.
pub(super) fn paint_cover(ui: &Ui, rect: Rect, tex: Option<TextureHandle>, seed: &str, rounding: f32) {
    match tex {
        Some(t) => {
            Image::new(egui::load::SizedTexture::from_handle(&t)).rounding(rounding).paint_at(ui, rect);
        }
        None => {
            let c = seed_color(seed);
            gradient_fill(ui.painter(), rect, rounding, c.linear_multiply(0.7), lerp(c, BG, 0.55));
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, icon::MUSIC, icons(rect.height() * 0.38), Color32::from_white_alpha(200));
        }
    }
}

// ---------------- title bar, window resizing, home ----------------

#[derive(Clone, Copy)]
enum QuickTile {
    Liked,
    Songs,
    Playlist(u64),
}

impl App {
    /// Wolfbeat's own glass title bar: drag to move, double-click to maximize, custom window buttons.
    fn title_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        let drag = ui.interact(rect, Id::new("titlebar-drag"), Sense::click_and_drag());
        let near_edge = ui
            .input(|i| i.pointer.press_origin())
            .map_or(false, |p| p.y < rect.top() + 6.0 || p.x < rect.left() + 6.0 || p.x > rect.right() - 6.0);
        if drag.drag_started_by(PointerButton::Primary) && !near_edge {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }

        // Center: what's playing.
        let label = match self.current {
            Some(c) => {
                let t = &self.store.tracks[c];
                format!("{}  ·  {}", t.title, t.artist)
            }
            None => "Wolfbeat".to_string(),
        };
        let max_w = rect.width() - 2.0 * 170.0;
        let g = ui.painter().layout_no_wrap(label.clone(), body(12.0), DIM);
        if g.size().x <= max_w {
            ui.painter().galley(rect.center() - g.size() / 2.0, g, DIM);
        } else {
            trunc(ui.painter(), pos2(rect.center().x - max_w / 2.0, rect.center().y), &label, body(12.0), DIM, max_w);
        }

        // Right: minimize / maximize / close.
        let buttons = [(icon::MINIMIZE, 0), (if maximized { icon::RESTORE } else { icon::MAXIMIZE }, 1), (icon::WIN_CLOSE, 2)];
        for (i, (glyph, kind)) in buttons.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(rect.right() - 46.0 * (3 - i) as f32, rect.top()), vec2(46.0, rect.height()));
            let resp = ui.interact(r, Id::new(("win-btn", kind)), Sense::click());
            let ht = hover_t(ui, Id::new(("win-btn", kind)), resp.hovered());
            let hover_fill = if kind == 2 { Color32::from_rgb(232, 17, 35) } else { HOVER };
            ui.painter().rect_filled(r, 0.0, lerp(Color32::TRANSPARENT, hover_fill, ht));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, icons(10.0), lerp(SUBTLE, TEXT, ht));
            if resp.clicked() {
                ui.ctx().send_viewport_cmd(match kind {
                    0 => ViewportCommand::Minimized(true),
                    1 => ViewportCommand::Maximized(!maximized),
                    _ => ViewportCommand::Close,
                });
            }
        }
    }

    fn tile_list(&self, t: QuickTile) -> (Vec<usize>, String, View) {
        match t {
            QuickTile::Liked => (self.store.liked.iter().rev().filter_map(|p| self.by_path.get(p).copied()).collect(), "Liked Songs".into(), View::Liked),
            QuickTile::Songs => ((0..self.store.tracks.len()).filter(|&i| !self.is_web(i)).collect(), "Songs".into(), View::Songs),
            QuickTile::Playlist(id) => {
                let pl = self.store.playlists.iter().find(|p| p.id == id);
                let list = pl.map(|p| p.tracks.iter().filter_map(|x| self.by_path.get(x).copied()).collect()).unwrap_or_default();
                (list, pl.map(|p| p.name.clone()).unwrap_or_default(), View::Playlist(id))
            }
        }
    }

    fn home_view(&mut self, ui: &mut Ui) {
        let greet = match crate::winstyle::local_hour() {
            5..=11 => "Good morning",
            12..=17 => "Good afternoon",
            _ => "Good evening",
        };
        let mut tiles = vec![];
        if !self.liked.is_empty() {
            tiles.push(QuickTile::Liked);
        }
        if self.store.tracks.iter().any(|t| web::parse_path(&t.path).is_none()) {
            tiles.push(QuickTile::Songs);
        }
        tiles.extend(self.store.playlists.iter().filter(|p| !p.tracks.is_empty()).map(|p| QuickTile::Playlist(p.id)));
        tiles.truncate(6);
        let mut seen = HashSet::new();
        let recent: Vec<usize> = self.store.recent.iter().filter_map(|p| self.by_path.get(p).copied()).filter(|i| seen.insert(*i)).take(12).collect();
        let playlists: Vec<(u64, String, usize)> = self.store.playlists.iter().map(|p| (p.id, p.name.clone(), p.tracks.len())).collect();

        ScrollArea::vertical().id_source("home").auto_shrink([false; 2]).show(ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(PAD);
                ui.label(RichText::new(greet).font(bold(36.0)).color(TEXT));
            });
            ui.add_space(16.0);

            if tiles.is_empty() && recent.is_empty() {
                ui.add_space(20.0);
                return empty_hint(ui, icon::SEARCH, "Find some music", "Search above, import a Spotify playlist, or add a music folder.");
            }

            // Quick tiles (2 or 3 columns).
            if !tiles.is_empty() {
                let w = ui.available_width() - 2.0 * PAD;
                let cols = if w > 820.0 { 3 } else { 2 };
                let gap = 12.0;
                let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
                let rows = (tiles.len() + cols - 1) / cols;
                let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * (64.0 + gap)), Sense::hover());
                for (i, tile) in tiles.iter().enumerate() {
                    let r = Rect::from_min_size(area.min + vec2(PAD + (i % cols) as f32 * (tw + gap), (i / cols) as f32 * (64.0 + gap)), vec2(tw, 64.0));
                    let (list, label, view) = self.tile_list(*tile);
                    let id = Id::new(("tile", label.as_str(), i));
                    let resp = ui.interact(r, id, Sense::click());
                    let ht = hover_t(ui, id, resp.hovered());
                    ui.painter().rect_filled(r, 12.0, lerp(RAISED, HOVER, ht));
                    let cover = Rect::from_min_size(r.min, vec2(64.0, 64.0));
                    match tile {
                        QuickTile::Liked => {
                            gradient_fill(ui.painter(), cover, 12.0, Color32::from_rgb(120, 70, 255), Color32::from_rgb(70, 190, 255));
                            ui.painter().text(cover.center(), Align2::CENTER_CENTER, icon::HEART_FILL, icons(22.0), Color32::WHITE);
                        }
                        QuickTile::Songs => {
                            gradient_fill(ui.painter(), cover, 12.0, Color32::from_rgb(92, 107, 255), accent2());
                            ui.painter().text(cover.center(), Align2::CENTER_CENTER, icon::MUSIC, icons(22.0), Color32::WHITE);
                        }
                        QuickTile::Playlist(_) => {
                            let tex = list.first().and_then(|&t| art_key(&self.store.tracks[t])).and_then(|k| self.art.get(&k));
                            paint_cover(ui, cover, tex, &label, 12.0);
                        }
                    }
                    let name = if matches!(tile, QuickTile::Songs) { "Your Songs" } else { label.as_str() };
                    trunc(ui.painter(), pos2(cover.right() + 14.0, r.center().y), name, semibold(14.5), TEXT, tw - 64.0 - 70.0);
                    // Play button fades in on hover.
                    let mut play_clicked = false;
                    if ht > 0.02 {
                        let pr = Rect::from_center_size(pos2(r.right() - 30.0, r.center().y + 4.0 * (1.0 - ht)), vec2(38.0, 38.0));
                        let presp = ui.interact(pr, id.with("play"), Sense::click());
                        gradient_fill(ui.painter(), pr, 19.0, accent().gamma_multiply(ht), accent2().gamma_multiply(ht));
                        ui.painter().text(pr.center() + vec2(1.0, 0.0), Align2::CENTER_CENTER, icon::PLAY, icons(13.0), Color32::WHITE.gamma_multiply(ht));
                        play_clicked = presp.clicked();
                    }
                    if play_clicked && !list.is_empty() {
                        self.actions.push(Action::Play { tracks: list, start: None, label });
                    } else if resp.clicked() {
                        self.actions.push(Action::Go(view));
                    }
                }
                ui.add_space(14.0);
            }

            if !recent.is_empty() {
                section_title(ui, "Recently played");
                let items: Vec<CardItem> = recent
                    .iter()
                    .map(|&t| {
                        let tr = &self.store.tracks[t];
                        CardItem { key: art_key(tr), title: tr.title.clone(), sub: tr.artist.clone(), seed: tr.album.clone() }
                    })
                    .collect();
                if let Some((i, act)) = self.card_grid(ui, "recent", &items, 2, true) {
                    let t = recent[i];
                    self.actions.push(match act {
                        CardAct::Open | CardAct::Play => Action::Play { tracks: recent.clone(), start: Some(i), label: "Recently played".into() },
                        CardAct::PlayNext => Action::PlayNext(t),
                        CardAct::Enqueue => Action::Enqueue(t),
                        CardAct::Radio => Action::StartRadio(t),
                        CardAct::Remove => Action::RemoveRecent(t),
                    });
                }
            }

            // Recommendations (YouTube Music style): Your Mix + Because you listened to…
            if !self.mix.is_empty() || self.recs_loading {
                section_title(ui, "Made for you");
                if self.recs_loading && self.mix.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(PAD);
                        ui.spinner();
                        ui.label(RichText::new("Finding songs you'll like…").color(SUBTLE));
                    });
                    ui.add_space(12.0);
                }
                let mut mixes: Vec<(usize, CardItem, Vec<usize>)> = vec![];
                for n in 0..=self.because.len() {
                    let (title, list) = self.mix_list(n);
                    if list.is_empty() {
                        continue;
                    }
                    let key = art_key(&self.store.tracks[list[0]]);
                    let sub = format!("{} songs · Made for you", list.len());
                    mixes.push((n, CardItem { key, title, sub, seed: format!("mix{n}") }, list));
                }
                let items: Vec<CardItem> = mixes.iter().map(|(_, c, _)| CardItem { key: c.key.clone(), title: c.title.clone(), sub: c.sub.clone(), seed: c.seed.clone() }).collect();
                if let Some((i, act)) = self.card_grid(ui, "mixes", &items, 1, false) {
                    let (n, item, list) = &mixes[i];
                    self.actions.push(match act {
                        CardAct::Play => Action::Play { tracks: list.clone(), start: None, label: item.title.clone() },
                        _ => Action::Go(View::Mix(*n)),
                    });
                }
            }

            if !playlists.is_empty() {
                section_title(ui, "Your playlists");
                let items: Vec<CardItem> = playlists
                    .iter()
                    .map(|(id, name, n)| {
                        let (list, _, _) = self.tile_list(QuickTile::Playlist(*id));
                        let key = list.first().and_then(|&t| art_key(&self.store.tracks[t]));
                        CardItem { key, title: name.clone(), sub: format!("Playlist · {n} songs"), seed: name.clone() }
                    })
                    .collect();
                if let Some((i, act)) = self.card_grid(ui, "playlists", &items, 3, false) {
                    let (list, label, view) = self.tile_list(QuickTile::Playlist(playlists[i].0));
                    if act == CardAct::Play && !list.is_empty() {
                        self.actions.push(Action::Play { tracks: list, start: None, label });
                    } else {
                        self.actions.push(Action::Go(view));
                    }
                }
            }
            ui.add_space(30.0);
        });
    }

    /// Grid of cover cards that lift on hover and reveal a play button. Returns (index, play button clicked).
    fn card_grid(&mut self, ui: &mut Ui, key: &str, items: &[CardItem], max_rows: usize, removable: bool) -> Option<(usize, CardAct)> {
        let w = ui.available_width() - 2.0 * PAD;
        let gap = 16.0;
        let min_w = 168.0;
        let cols = (((w + gap) / (min_w + gap)).floor() as usize).max(2);
        let cw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = cw + 58.0;
        let n = items.len().min(cols * max_rows);
        let rows = (n + cols - 1) / cols;
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), rows as f32 * (ch + gap)), Sense::hover());
        let mut out = None;
        for (i, it) in items.iter().take(n).enumerate() {
            let r = Rect::from_min_size(area.min + vec2(PAD + (i % cols) as f32 * (cw + gap), (i / cols) as f32 * (ch + gap)), vec2(cw, ch));
            let id = Id::new(("card", key, i));
            let resp = ui.interact(r, id, Sense::click());
            let ht = hover_t(ui, id, resp.hovered());
            let lift = -5.0 * ht;
            ui.painter().rect_filled(r.translate(vec2(0.0, lift)), 16.0, lerp(Color32::TRANSPARENT, RAISED, ht));
            let pad = 10.0;
            let cover = Rect::from_min_size(r.min + vec2(pad, pad + lift), vec2(cw - 2.0 * pad, cw - 2.0 * pad));
            if ht > 0.0 {
                shadow(ui.painter(), cover, 12.0, ht);
            }
            let tex = it.key.as_ref().and_then(|k| self.art.get(&format!("big:{k}")).or_else(|| self.art.get(k)));
            paint_cover(ui, cover, tex, &it.seed, 12.0);
            let p = ui.painter();
            trunc(p, pos2(cover.left(), cover.bottom() + 16.0), &it.title, semibold(14.0), TEXT, cover.width());
            trunc(p, pos2(cover.left(), cover.bottom() + 36.0), &it.sub, body(12.5), SUBTLE, cover.width());
            let mut act = None;
            if ht > 0.02 {
                let pr = Rect::from_center_size(cover.right_bottom() - vec2(28.0, 28.0 - 8.0 * (1.0 - ht)), vec2(44.0, 44.0));
                let presp = ui.interact(pr, id.with("play"), Sense::click());
                shadow(ui.painter(), pr, 22.0, 0.8 * ht);
                gradient_fill(ui.painter(), pr, 22.0, accent().gamma_multiply(ht), accent2().gamma_multiply(ht));
                ui.painter().text(pr.center() + vec2(1.5, 0.0), Align2::CENTER_CENTER, icon::PLAY, icons(15.0), Color32::WHITE.gamma_multiply(ht));
                if presp.clicked() {
                    act = Some(CardAct::Play);
                }
                // Remove (×) button in the corner for removable cards.
                if removable {
                    let xr = Rect::from_center_size(cover.right_top() + vec2(-16.0, 16.0), vec2(26.0, 26.0));
                    let xresp = ui.interact(xr, id.with("remove"), Sense::click()).on_hover_text("Remove from Home");
                    let xh = hover_t(ui, id.with("remove"), xresp.hovered());
                    ui.painter().circle_filled(xr.center(), 13.0, lerp(Color32::from_black_alpha(150), Color32::from_rgb(232, 17, 35), xh).gamma_multiply(ht));
                    ui.painter().text(xr.center(), Align2::CENTER_CENTER, icon::CLOSE, icons(9.5), Color32::WHITE.gamma_multiply(ht));
                    if xresp.clicked() {
                        act = Some(CardAct::Remove);
                    }
                }
            }
            if act.is_none() && resp.clicked() {
                act = Some(CardAct::Open);
            }
            let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            if removable {
                resp.context_menu(|ui| {
                    if menu_item(ui, icon::PLAY, "Play") {
                        act = Some(CardAct::Play);
                    }
                    if menu_item(ui, icon::NEXT_UP, "Play next") {
                        act = Some(CardAct::PlayNext);
                    }
                    if menu_item(ui, icon::QUEUE, "Add to queue") {
                        act = Some(CardAct::Enqueue);
                    }
                    if menu_item(ui, icon::MUSIC, "Start radio") {
                        act = Some(CardAct::Radio);
                    }
                    ui.separator();
                    if menu_item(ui, icon::CLOSE, "Remove from Recently played") {
                        act = Some(CardAct::Remove);
                    }
                });
            }
            if let Some(a) = act {
                out = Some((i, a));
            }
        }
        ui.add_space(8.0);
        out
    }
}

struct CardItem {
    key: Option<String>,
    title: String,
    sub: String,
    seed: String,
}

fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.label(RichText::new(text).font(bold(22.0)).color(TEXT));
    });
    ui.add_space(8.0);
}

/// Borderless window: resize from the window edges.
fn resize_edges(ctx: &Context) {
    use egui::viewport::ResizeDirection as D;
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let r = ctx.screen_rect();
    let m = 6.0;
    let (l, rt, t, b) = (pos.x < r.left() + m, pos.x > r.right() - m, pos.y < r.top() + m, pos.y > r.bottom() - m);
    let dir = match (l, rt, t, b) {
        (true, _, true, _) => D::NorthWest,
        (_, true, true, _) => D::NorthEast,
        (true, _, _, true) => D::SouthWest,
        (_, true, _, true) => D::SouthEast,
        (true, ..) => D::West,
        (_, true, ..) => D::East,
        (_, _, true, _) => D::North,
        (_, _, _, true) => D::South,
        _ => return,
    };
    ctx.set_cursor_icon(match dir {
        D::North | D::South => CursorIcon::ResizeVertical,
        D::East | D::West => CursorIcon::ResizeHorizontal,
        D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
        _ => CursorIcon::ResizeNeSw,
    });
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}

// ---------------- Now Playing (fullscreen visual) ----------------

/// Small deterministic hash → 0..1, for particle placement.
fn hash01(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

impl App {
    /// Immersive view: floating cover, ring equalizer, particles and the live lyric line,
    /// all in the cover's colors. Opened by clicking the cover in the player bar.
    fn now_playing_view(&mut self, ui: &mut Ui) {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.actions.push(Action::Go(self.before_np.clone()));
        }
        let Some(c) = self.current else {
            return empty_hint(ui, icon::MUSIC, "Nothing playing", "Play a song, then click its cover.");
        };
        let t = self.store.tracks[c].clone();
        let area = ui.max_rect();
        let time = ui.input(|i| i.time) as f32;
        let playing = self.playing();
        // Smoothly ramp the motion up/down when you press play/pause.
        let energy = ui.ctx().animate_bool_with_time(Id::new("np-energy"), playing, 0.6);
        let pal = art_key(&t).and_then(|k| self.art.palette(&k)).unwrap_or(self.aurora);
        let (c1, c2, c3) = (lerp(pal[0], accent(), 0.25), lerp(pal[1], accent2(), 0.25), pal[2]);

        // A steady animated beat (~118 BPM): a quick attack and a soft decay.
        let bpm = 118.0f32;
        let phase = (time * bpm / 60.0).fract();
        let beat = (1.0 - phase).powf(3.0) * energy;

        let p = ui.painter().clone();

        // Floating light particles drifting upward.
        for k in 0..46u32 {
            let speed = 0.025 + 0.05 * hash01(k, 1);
            let prog = (time * speed + hash01(k, 2)).fract();
            let x = area.left() + area.width() * hash01(k, 3) + 18.0 * (time * 0.4 + k as f32).sin();
            let y = area.bottom() - prog * (area.height() + 40.0);
            let r = 1.2 + 2.8 * hash01(k, 4);
            let fade = (prog * 4.0).min(1.0) * ((1.0 - prog) * 3.0).min(1.0);
            let col = [c1, c2, c3][(k % 3) as usize];
            p.circle_filled(pos2(x, y), r, col.gamma_multiply(0.55 * fade * (0.4 + 0.6 * energy)));
        }

        // Layout: cover centered a bit above the middle; text underneath.
        // Spotify-style split: cover on the left, lyrics on the right (when lyrics exist and the window is wide).
        let split = self.store.np_lyrics && area.width() > 860.0 && matches!(self.lyrics.get(&c), Some(Lyrics::Synced(_)) | Some(Lyrics::Plain(_)));
        let cx = if split { area.left() + area.width() * 0.29 } else { area.center().x };
        let size = if split { (area.height() * 0.44).min(area.width() * 0.25).clamp(170.0, 380.0) } else { (area.height() * 0.46).min(area.width() * 0.36).clamp(180.0, 420.0) };
        let float = (time * 0.9).sin() * 6.0 * (0.3 + 0.7 * energy);
        let center = pos2(cx, area.top() + area.height() * 0.40 + float);
        let scale = 1.0 + 0.018 * beat;
        let cover = Rect::from_center_size(center, vec2(size, size) * scale);

        // Glow that breathes with the beat.
        theme::radial_blob(&p, center, size * (1.05 + 0.12 * beat), c1.gamma_multiply(0.35 + 0.35 * beat));
        theme::radial_blob(&p, center + vec2(size * 0.25, size * 0.2), size * 0.8, c2.gamma_multiply(0.25 + 0.2 * energy));

        // Ring equalizer around the cover.
        let bars = 72;
        let ring_r = size * 0.5 * std::f32::consts::SQRT_2 + 14.0;
        for i in 0..bars {
            let a = i as f32 / bars as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let fi = i as f32;
            let n = 0.5 + 0.5 * (fi * 0.37 + time * 3.1).sin() * (fi * 0.11 - time * 2.3).cos()
                + 0.25 * (fi * 1.7 + time * 5.3).sin();
            let h = 4.0 + (n.clamp(0.0, 1.2) * 34.0) * (0.35 + 0.65 * beat) * energy + 3.0 * energy;
            let dir = vec2(a.cos(), a.sin());
            let from = center + dir * ring_r;
            let to = center + dir * (ring_r + h);
            let col = lerp(lerp(c1, c2, (a.sin() * 0.5 + 0.5).clamp(0.0, 1.0)), c3, ((fi * 0.07 + time * 0.2).sin() * 0.5 + 0.5) * 0.4);
            p.line_segment([from, to], Stroke::new(3.2_f32, col.gamma_multiply(0.85)));
        }

        // The cover itself (click it to go back).
        shadow(&p, cover, 18.0, 2.2);
        let tex = art_key(&t).and_then(|k| self.art.get(&format!("big:{k}")).or_else(|| self.art.get(&k)));
        paint_cover(ui, cover, tex, &t.album, 18.0);
        let cresp = ui.interact(cover, Id::new("np-cover"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if cresp.clicked() {
            self.actions.push(Action::Go(self.before_np.clone()));
        }

        // Title, artist, and the current lyric line.
        let ty = area.top() + area.height() * 0.40 + size * 0.5 * std::f32::consts::SQRT_2 + 66.0;
        let max_w = if split { area.width() * 0.5 - 60.0 } else { area.width() - 120.0 };
        let title_g = p.layout_no_wrap(t.title.clone(), bold(30.0), TEXT);
        let title_w = title_g.size().x.min(max_w);
        trunc(&p, pos2(cx - title_w / 2.0, ty), &t.title, bold(30.0), TEXT, max_w);
        let artist = match self.via {
            Some(src) => format!("{}  ·  via {}", t.artist, src.name()),
            None => t.artist.clone(),
        };
        p.text(pos2(cx, ty + 30.0), Align2::CENTER_CENTER, artist, body(16.0), SUBTLE);

        if let Some(Lyrics::Synced(lines)) = self.lyrics.get(&c).filter(|_| self.store.np_lyrics && !split) {
            let pos = self.position() + 0.25;
            if let Some(i) = lines.iter().rposition(|(s, _)| *s <= pos) {
                let line = if lines[i].1.is_empty() { "♪".to_string() } else { lines[i].1.clone() };
                // Fade each new line in.
                let since = pos - lines[i].0;
                let a = (since / 0.35).clamp(0.0, 1.0);
                let mut job = LayoutJob::single_section(line, TextFormat::simple(bold(24.0), Color32::WHITE.gamma_multiply(a)));
                job.wrap.max_width = max_w.min(900.0);
                job.halign = Align::Center;
                let g = p.layout_job(job);
                p.galley(pos2(area.center().x, ty + 66.0 + (1.0 - a) * 8.0), g, Color32::WHITE);
                if let Some((_, next)) = lines.get(i + 1) {
                    p.text(pos2(area.center().x, ty + 112.0), Align2::CENTER_CENTER, next, body(15.0), Color32::from_white_alpha(90));
                }
            }
        }

        // Right side: scrolling lyrics.
        if split {
            let lr = Rect::from_min_max(pos2(area.left() + area.width() * 0.56, area.top() + 56.0), pos2(area.right() - PAD - 10.0, area.bottom() - 10.0));
            self.lyrics_panel(ui, c, lr, 28.0);
        }

        // Back button (top-left) + hint.
        let br = Rect::from_min_size(area.min + vec2(20.0, 16.0), vec2(36.0, 36.0));
        let bresp = ui.interact(br, Id::new("np-back"), Sense::click()).on_hover_text("Back (Esc)");
        let bh = hover_t(ui, Id::new("np-back"), bresp.hovered());
        p.circle_filled(br.center(), 18.0, lerp(RAISED, HOVER, bh));
        p.text(br.center(), Align2::CENTER_CENTER, icon::CHEVRON_DOWN, icons(13.0), TEXT);
        if bresp.clicked() {
            self.actions.push(Action::Go(self.before_np.clone()));
        }
        caps_at(&p, pos2(br.right() + 12.0, br.center().y), "Now playing", SUBTLE);
    }
}

// ---------------- Settings ----------------

impl App {
    fn settings_view(&mut self, ui: &mut Ui) {
        ScrollArea::vertical().id_source("settings").auto_shrink([false; 2]).show(ui, |ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(PAD);
                ui.label(RichText::new("Settings").font(bold(34.0)).color(TEXT));
            });
            ui.add_space(10.0);
            let w = (ui.available_width() - 2.0 * PAD).min(760.0);

            // Appearance
            settings_card(ui, w, "Appearance", |ui| {
                setting_row(ui, icon::THEME, "Background", "The scene behind Wolfbeat", |ui| {
                    for (mode, label) in [(2u8, "Plain"), (1, "Aurora"), (0, "Scenery")] {
                        if chip(ui, label, self.store.background == mode).clicked() {
                            self.store.background = mode;
                            theme::set_background(mode);
                            self.dirty = true;
                        }
                    }
                });
                let mut v = self.store.animate;
                setting_row(ui, icon::REFRESH, "Animated background", "Clouds, grass and glows move. Turn off to save battery.", |ui| {
                    if toggle(ui, Id::new("set-anim"), &mut v) {
                        theme::set_animate(v);
                    }
                });
                if v != self.store.animate {
                    self.store.animate = v;
                    self.dirty = true;
                }
                let mut v = self.store.cover_colors;
                setting_row(ui, icon::MUSIC, "Colors follow the cover", "Buttons and glows take the current song's colors", |ui| {
                    toggle(ui, Id::new("set-cover"), &mut v);
                });
                if v != self.store.cover_colors {
                    self.store.cover_colors = v;
                    self.dirty = true;
                }
                let mut v = self.store.np_lyrics;
                setting_row(ui, icon::LYRICS, "Lyrics in Now Playing", "Show the live lyric line under the big cover", |ui| {
                    toggle(ui, Id::new("set-nplyr"), &mut v);
                });
                if v != self.store.np_lyrics {
                    self.store.np_lyrics = v;
                    self.dirty = true;
                }
            });

            // Library
            let n_recent = self.store.recent.len();
            settings_card(ui, w, "Library", |ui| {
                setting_row(ui, icon::FOLDER, "Music folders", &format!("{} folder(s) on this PC", self.store.folders.len()), |ui| {
                    if ghost_button(ui, "Manage", 30.0).clicked() {
                        self.actions.push(Action::Go(View::Folders));
                    }
                });
                let mut v = self.store.autoplay;
                setting_row(ui, icon::MUSIC, "Autoplay similar songs", "When your queue ends, keep playing songs like the last one", |ui| {
                    toggle(ui, Id::new("set-autoplay"), &mut v);
                });
                if v != self.store.autoplay {
                    self.store.autoplay = v;
                    self.dirty = true;
                }
                setting_row(ui, icon::CLOCK, "Recently played", &format!("{n_recent} song(s) remembered"), |ui| {
                    if ghost_button(ui, "Clear", 30.0).clicked() {
                        self.store.recent.clear();
                        self.dirty = true;
                        self.notify("Recently played cleared");
                    }
                });
            });

            // About
            settings_card(ui, w, "About", |ui| {
                setting_row(ui, icon::INFO, "Wolfbeat 1.0", "Built in Rust · lyrics from LRCLIB · search via Spotify, YouTube Music and YouTube", |ui| {
                    if ghost_button(ui, "Open data folder", 30.0).clicked() {
                        if let Some(dir) = dirs::data_dir().map(|d| d.join("Wolfbeat")) {
                            let _ = std::process::Command::new("explorer").arg(dir).spawn();
                        }
                    }
                });
            });
            ui.add_space(30.0);
        });
    }
}

fn settings_card(ui: &mut Ui, w: f32, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        caps(ui, title, SUBTLE);
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        Frame::none().fill(SIDEBAR).stroke(Stroke::new(1.0_f32, GLASS_EDGE)).rounding(14.0).inner_margin(Margin::symmetric(16.0, 8.0)).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.vertical(|ui| add(ui));
        });
    });
    ui.add_space(10.0);
}

fn setting_row(ui: &mut Ui, ic: &str, title: &str, sub: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(56.0);
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        ui.painter().rect_filled(r, 10.0, RAISED);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, ic, icons(15.0), accent());
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(title).font(semibold(14.5)).color(TEXT));
            ui.label(RichText::new(sub).size(12.5).color(SUBTLE));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| control(ui));
    });
}

/// Animated on/off switch. Returns true when flipped.
fn toggle(ui: &mut Ui, id: Id, on: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    let resp = ui.interact(r, id, Sense::click()).union(resp).on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(id, *on, 0.18);
    if t > 0.01 {
        gradient_fill(ui.painter(), r, 12.0, accent().gamma_multiply(t), accent2().gamma_multiply(t));
    }
    ui.painter().rect_filled(r, 12.0, lerp(Color32::from_rgb(60, 64, 78), Color32::TRANSPARENT, t));
    let x = lerp_f(r.left() + 12.0, r.right() - 12.0, t);
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    resp.clicked()
}

fn lerp_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl App {
    /// Scrolling lyrics (current line lit, click to seek). Used by the Lyrics page and Now Playing.
    fn lyrics_panel(&mut self, ui: &mut Ui, c: usize, lyr_rect: Rect, font_size: f32) {
        let mut child = ui.child_ui(lyr_rect, Layout::top_down(Align::Min), None);
        let ui = &mut child;
        match self.lyrics.get(&c).cloned() {
            None | Some(Lyrics::Loading) => {
                ui.add_space(40.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Finding lyrics…").color(SUBTLE).size(15.0));
                });
            }
            Some(Lyrics::NotFound) => {
                empty_hint(ui, icon::LYRICS, "No lyrics found", "LRCLIB doesn't have this song yet.");
            }
            Some(Lyrics::Instrumental) => {
                empty_hint(ui, icon::MUSIC, "Instrumental", "Just enjoy the music.");
            }
            Some(Lyrics::Plain(lines)) => {
                ScrollArea::vertical().auto_shrink([false; 2]).id_source(("lyr", c)).show(ui, |ui| {
                    ui.add_space(30.0);
                    ui.label(RichText::new("These lyrics aren't time-synced").color(DIM).size(12.5));
                    ui.add_space(10.0);
                    for l in lines {
                        ui.label(RichText::new(if l.is_empty() { " ".into() } else { l }).font(bold(font_size * 0.8)).color(SUBTLE));
                    }
                    ui.add_space(120.0);
                });
            }
            Some(Lyrics::Synced(lines)) => {
                let pos = self.position() + 0.25; // show a line a hair early, like other players
                let cur = lines.iter().rposition(|(s, _)| *s <= pos);
                let changed = cur != self.lyric_line;
                self.lyric_line = cur;
                let max_w = lyr_rect.width();
                ScrollArea::vertical().auto_shrink([false; 2]).id_source(("lyr", c)).scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
                    ui.add_space(lyr_rect.height() * 0.35);
                    for (i, (start, text)) in lines.iter().enumerate() {
                        let active = Some(i) == cur;
                        let past = cur.map_or(false, |c| i < c);
                        let id = Id::new(("lyric", c, i));
                        let at = ui.ctx().animate_bool_with_time(id, active, 0.35);
                        let text = if text.is_empty() { "♪" } else { text.as_str() };
                        let base = if past { Color32::from_white_alpha(70) } else { Color32::from_white_alpha(110) };
                        let color = lerp(base, Color32::WHITE, at);
                        let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(bold(font_size), color));
                        job.wrap.max_width = max_w - 20.0;
                        let galley = ui.painter().layout_job(job);
                        let (r, resp) = ui.allocate_exact_size(vec2(max_w, galley.size().y + 18.0), Sense::click());
                        let hover = hover_t(ui, id.with("h"), resp.hovered());
                        if hover > 0.0 {
                            ui.painter().rect_filled(r.expand2(vec2(6.0, 0.0)), 12.0, lerp(Color32::TRANSPARENT, RAISED, hover));
                        }
                        // Active line slides slightly right as it lights up.
                        ui.painter().galley(pos2(r.left() + 4.0 + 6.0 * at, r.top() + 9.0), galley, color);
                        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            self.actions.push(Action::Seek(*start));
                        }
                        if active && changed {
                            ui.scroll_to_rect(r, Some(Align::Center));
                        }
                    }
                    ui.add_space(lyr_rect.height() * 0.5);
                });
            }
        }
    }
}

// ---------------- recommended mixes ----------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum CardAct {
    Open,
    Play,
    PlayNext,
    Enqueue,
    Radio,
    Remove,
}

impl App {
    /// A recommended playlist ("Your Mix" or "Because you listened to …"): play it, save it, or refresh it.
    fn mix_view(&mut self, ui: &mut Ui, n: usize) {
        let (title, list) = self.mix_list(n);
        if list.is_empty() {
            if self.recs_loading {
                return empty_hint(ui, icon::MUSIC, "Building your mix…", "Wolfbeat is finding songs similar to what you listen to.");
            }
            return empty_hint(ui, icon::MUSIC, "No recommendations yet", "Play or like a few songs, then come back.");
        }
        let tex = art_key(&self.store.tracks[list[0]]).and_then(|k| self.art.get(&format!("big:{k}")).or_else(|| self.art.get(&k)));
        let secs: f32 = list.iter().map(|&i| self.store.tracks[i].duration).sum();
        let sub = format!("{} songs · {} min · based on what you play and like", list.len(), (secs / 60.0).round());
        let label = title.clone();
        self.hero(ui, "Made for you", &title, &sub, &list, &label, None, Tile::Cover(tex, format!("mix{n}")));
        ui.horizontal(|ui| {
            ui.add_space(PAD);
            if ghost_button(ui, &format!("{}  Save as playlist", icon::ADD), 34.0).clicked() {
                self.actions.push(Action::SaveMix(n));
            }
            if !self.recs_loading && ghost_button(ui, &format!("{}  Refresh", icon::REFRESH), 34.0).clicked() {
                self.actions.push(Action::RefreshRecs);
            }
            if self.recs_loading {
                ui.spinner();
            }
        });
        ui.add_space(10.0);
        self.track_list(ui, &list, &label, None);
    }
}
