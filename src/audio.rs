use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

pub struct Audio {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    pub volume: f32,
    pub muted: bool,
    pub init_error: Option<String>,
}

impl Audio {
    pub fn new(volume: f32) -> Self {
        let (stream, handle, err) = match OutputStream::try_default() {
            Ok((s, h)) => (Some(s), Some(h), None),
            Err(e) => (None, None, Some(format!("No audio output device: {e}"))),
        };
        Self { _stream: stream, handle, sink: None, volume, muted: false, init_error: err }
    }

    fn eff_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume * self.volume // perceptual curve
        }
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let handle = self.handle.as_ref().ok_or("No audio output device")?;
        let file = File::open(path).map_err(|e| format!("Can't open file: {e}"))?;
        // Some decoders panic on unusual files; turn that into an error instead of crashing the app.
        let dec = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Decoder::new(BufReader::new(file))))
            .map_err(|_| "This file format isn't supported".to_string())?
            .map_err(|e| format!("Can't decode: {e}"))?;
        let sink = Sink::try_new(handle).map_err(|e| e.to_string())?;
        sink.set_volume(self.eff_volume());
        sink.append(dec);
        self.sink = Some(sink); // dropping the old sink stops it
        Ok(())
    }

    pub fn has_track(&self) -> bool {
        self.sink.is_some()
    }
    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().map(|s| !s.is_paused() && !s.empty()).unwrap_or(false)
    }
    pub fn finished(&self) -> bool {
        self.sink.as_ref().map(|s| s.empty()).unwrap_or(false)
    }
    pub fn toggle(&self) {
        if let Some(s) = &self.sink {
            if s.is_paused() {
                s.play()
            } else {
                s.pause()
            }
        }
    }
    pub fn position(&self) -> f32 {
        self.sink.as_ref().map(|s| s.get_pos().as_secs_f32()).unwrap_or(0.0)
    }
    pub fn seek(&self, secs: f32) {
        if let Some(s) = &self.sink {
            if !s.empty() {
                let _ = s.try_seek(Duration::from_secs_f32(secs.max(0.0)));
            }
        }
    }
    pub fn stop(&mut self) {
        self.sink = None;
    }
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        self.apply();
    }
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.apply();
    }
    fn apply(&self) {
        if let Some(s) = &self.sink {
            s.set_volume(self.eff_volume());
        }
    }
}
