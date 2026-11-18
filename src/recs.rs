//! Recommendations, YouTube Music style.
//!
//! 1. Seeds: your most recent plays and liked songs (up to 5, distinct).
//! 2. For each seed, fetch YouTube's "Mix" radio (songs it would play next).
//! 3. Rank: songs suggested by several seeds rank highest; earlier positions score more.
//! 4. Filter: skip what you already listened to/liked, and at most 2 songs per artist.
//! The result is "Your Mix" plus one "Because you listened to …" mix per seed.

use crate::web::{self, Hit};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct Seed {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub duration: f32,
}

pub struct Recs {
    pub your_mix: Vec<Hit>,
    /// (seed title, its mix)
    pub because: Vec<(String, Vec<Hit>)>,
}

/// Normalized "artist|title" used to spot duplicates and songs you already know.
pub fn song_key(artist: &str, title: &str) -> String {
    let n = |s: &str| -> String {
        let s = s.split(" (").next().unwrap_or(s).split(" [").next().unwrap_or(s);
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
    };
    let a = artist.split([',', '&']).next().unwrap_or(artist);
    format!("{}|{}", n(a), n(title))
}

pub fn build(seeds: Vec<Seed>, known: HashSet<String>) -> Recs {
    let mut mixes: Vec<(String, Vec<Hit>)> = vec![];
    for s in &seeds {
        let Some(vid) = web::seed_video(&s.path, &s.artist, &s.title, s.duration) else { continue };
        if let Ok(list) = web::youtube_mix(&vid) {
            if !list.is_empty() {
                mixes.push((s.title.clone(), list));
            }
        }
    }

    // Score every candidate across all seed mixes.
    struct Cand {
        hit: Hit,
        score: f32,
        seeds: HashSet<usize>,
    }
    let mut cands: HashMap<String, Cand> = HashMap::new();
    for (si, (_, list)) in mixes.iter().enumerate() {
        for (rank, h) in list.iter().enumerate() {
            let key = song_key(&h.artist, &h.title);
            if known.contains(&key) {
                continue;
            }
            let c = cands.entry(key).or_insert_with(|| Cand { hit: clone_hit(h), score: 0.0, seeds: HashSet::new() });
            c.score += 1.0 / (1.0 + rank as f32 * 0.12);
            c.seeds.insert(si);
        }
    }
    let mut ranked: Vec<Cand> = cands.into_values().collect();
    ranked.sort_by(|a, b| {
        let ka = a.score * (1.0 + 0.6 * (a.seeds.len() as f32 - 1.0));
        let kb = b.score * (1.0 + 0.6 * (b.seeds.len() as f32 - 1.0));
        kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Diversity: at most 2 songs per artist in Your Mix.
    let mut per_artist: HashMap<String, usize> = HashMap::new();
    let your_mix: Vec<Hit> = ranked
        .into_iter()
        .filter(|c| {
            let a = song_key(&c.hit.artist, "");
            let n = per_artist.entry(a).or_insert(0);
            *n += 1;
            *n <= 2
        })
        .take(40)
        .map(|c| c.hit)
        .collect();

    let because = mixes
        .into_iter()
        .map(|(title, list)| {
            let mut seen = HashSet::new();
            let list = list
                .into_iter()
                .filter(|h| {
                    let k = song_key(&h.artist, &h.title);
                    !known.contains(&k) && seen.insert(k)
                })
                .take(25)
                .collect();
            (title, list)
        })
        .collect();
    Recs { your_mix, because }
}

/// Radio for one song: its mix, minus duplicates.
pub fn radio(seed: Seed) -> Vec<Hit> {
    let Some(vid) = web::seed_video(&seed.path, &seed.artist, &seed.title, seed.duration) else { return vec![] };
    let mut seen = HashSet::new();
    web::youtube_mix(&vid).unwrap_or_default().into_iter().filter(|h| seen.insert(song_key(&h.artist, &h.title))).collect()
}

fn clone_hit(h: &Hit) -> Hit {
    Hit { source: h.source, id: h.id.clone(), title: h.title.clone(), artist: h.artist.clone(), duration: h.duration, art: h.art.clone() }
}
