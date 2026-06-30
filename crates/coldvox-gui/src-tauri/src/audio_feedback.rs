//! Start/stop audio feedback cues for the ColdVox overlay.
//!
//! Ported from Handy (`cjpais/handy` → `src-tauri/src/audio_feedback.rs`).
//! Handy plays user-selectable WAV files via `rodio`; ColdVox's GUI crate does
//! not bundle cue resources, so this port synthesizes a short sine-wave beep
//! at build time (no asset files needed) and gates the entire stack behind the
//! `audio-feedback` cargo feature.
//!
//! When the feature is **off** (the default), every entry point compiles to a
//! no-op, so the GUI builds free of the `rodio`/`cpal` audio-output stack.
//! When the feature is **on** (`--features audio-feedback`), `play()` consults
//! the persisted [`OverlaySettings::audio_feedback`](crate::settings::OverlaySettings)
//! flag before spawning a detached thread that opens the default output device
//! and plays a 120 ms tone — 660 Hz for start, 440 Hz for stop — swallowing all
//! errors so a missing audio device never breaks the pipeline.

use tauri::AppHandle;

/// Which cue to play.
#[derive(Debug, Clone, Copy)]
pub enum Cue {
    /// Played when the pipeline starts listening.
    Start,
    /// Played when the pipeline stops.
    Stop,
}

/// Play a feedback cue. No-op unless the `audio-feedback` feature is enabled.
/// When the feature is enabled, still honors the persisted
/// [`OverlaySettings::audio_feedback`](crate::settings::OverlaySettings::audio_feedback)
/// flag so the user can mute cues without recompiling. A failed settings load
/// is treated as `false` (no cue) — matching the default — so a broken store
/// never surprises the user with sound.
pub fn play(app: &AppHandle, cue: Cue) {
    #[cfg(feature = "audio-feedback")]
    {
        if !crate::settings::load(app).audio_feedback {
            return;
        }
        let freq = match cue {
            Cue::Start => 660.0,
            Cue::Stop => 440.0,
        };
        // Detached thread: never block the Tauri command path on audio output.
        std::thread::spawn(move || {
            if let Err(err) = play_beep(freq, 0.120) {
                log::debug!("audio feedback failed ({freq} Hz): {err}");
            }
        });
    }
    // No-op branch keeps the signature stable when the feature is off.
    #[cfg(not(feature = "audio-feedback"))]
    {
        let _ = (app, cue);
    }
}

#[cfg(feature = "audio-feedback")]
fn play_beep(freq: f32, duration_secs: f64) -> Result<(), Box<dyn std::error::Error>> {
    use rodio::{OutputStream, Sink, Source};

    let (_stream, handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&handle)?;
    let beep = Beep::new(freq, duration_secs);
    sink.append(beep);
    sink.sleep_until_end();
    // Keep the stream alive for the duration of playback. `_stream` drops here.
    drop(_stream);
    Ok(())
}

/// A minimal periodic sine-wave `Source`. Self-contained so the module does not
/// depend on `rodio::source::SineWave`'s shifting API across versions.
#[cfg(feature = "audio-feedback")]
struct Beep {
    freq: f32,
    sample_rate: u32,
    total_samples: u64,
    produced: u64,
}

#[cfg(feature = "audio-feedback")]
impl Beep {
    fn new(freq: f32, duration_secs: f64) -> Self {
        let sample_rate = 44_100u32;
        let total_samples = (sample_rate as f64 * duration_secs) as u64;
        Self {
            freq,
            sample_rate,
            total_samples,
            produced: 0,
        }
    }
}

#[cfg(feature = "audio-feedback")]
impl Iterator for Beep {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.produced >= self.total_samples {
            return None;
        }
        let t = self.produced as f32 / self.sample_rate as f32;
        // Simple envelope: linear fade-in/out to avoid clicks.
        let progress = self.produced as f32 / self.total_samples as f32;
        let env = (1.0 - (2.0 * progress - 1.0).abs()).max(0.0);
        let sample = (2.0 * std::f32::consts::PI * self.freq * t).sin() * 0.25 * env;
        self.produced += 1;
        Some(sample)
    }
}

#[cfg(feature = "audio-feedback")]
impl rodio::Source for Beep {
    fn current_frame_len(&self) -> Option<usize> {
        Some((self.total_samples - self.produced) as usize)
    }

    fn channels(&self) -> u16 {
        1 // mono
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs_f64(
            self.total_samples as f64 / self.sample_rate as f64,
        ))
    }
}
