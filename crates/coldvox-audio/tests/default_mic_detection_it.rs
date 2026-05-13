//! Integration test to verify that the capture stream attaches to the desktop's
//! current default microphone (via PulseAudio/PipeWire).
//!
//! This is a non-mock test that:
//! - Starts the audio capture thread without specifying a device (auto-select)
//! - Labels the PulseAudio application name via env so we can find our stream
//! - Uses `pactl` to read the default source and active source-outputs
//! - Asserts that our stream is connected to the default source
//!
//! Notes:
//! - Runs only on Linux and requires `pactl` (PulseAudio/PipeWire compatibility layer)
//! - If audio stack is unavailable, the test will skip gracefully

#![cfg(target_os = "linux")]

use coldvox_audio::{AudioCaptureThread, AudioRingBuffer};
use coldvox_foundation::AudioConfig;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const APP_TAG: &str = "ColdVoxMicTest";

#[cfg(target_os = "linux")]
#[test]
fn default_mic_is_detected_and_used_via_pulseaudio() {
    // 0) Opt-in guard: only run this environment-dependent test when explicitly enabled.
    // This avoids flaky failures on CI or systems without a stable desktop audio stack.
    if std::env::var("COLDVOX_RUN_AUDIO_IT").ok().as_deref() != Some("1") {
        eprintln!(
            "Skipping: set COLDVOX_RUN_AUDIO_IT=1 to run default mic detection integration test"
        );
        return; // skip
    }

    // 1) Pre-check: `pactl` availability and server readiness
    if Command::new("pactl").arg("info").output().is_err() {
        eprintln!("Skipping: pactl not available or PulseAudio/PipeWire not running");
        return; // skip
    }

    // 2) Get the default source name reported by PulseAudio
    let default_source = get_default_source_name();
    let default_source = match default_source {
        Some(name) => name,
        None => {
            eprintln!("Skipping: could not determine default source via pactl");
            return; // skip
        }
    };

    // 3) Start audio capture with a recognizable application name for pactl
    // PulseAudio respects PULSE_PROP_application.name passed via env
    std::env::set_var("PULSE_PROP_application.name", APP_TAG);
    std::env::set_var("PULSE_PROP_media.name", APP_TAG);

    let rb = AudioRingBuffer::new(16384);
    let (producer, mut consumer) = rb.split();
    let producer = Arc::new(Mutex::new(producer));
    let config = AudioConfig::default();

    let capture = match AudioCaptureThread::spawn(config, producer, None, false) {
        Ok((cap, _device_cfg, _cfg_rx, _dev_rx)) => cap,
        Err(e) => {
            eprintln!(
                "Skipping: failed to start capture ({}). Likely no audio backend.",
                e
            );
            return; // skip
        }
    };

    // 4) Wait for the stream to appear in PulseAudio and for first frames
    // Give a little time for cpal to initialize and connect
    let start = Instant::now();
    let mut found_in_pulseaudio = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(source_name) = find_our_source_output_source(APP_TAG) {
            // Compare to default source
            if source_name == default_source {
                found_in_pulseaudio = true;
                break;
            } else {
                // Allow a moment for any route change; continue polling
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    // 5) Also verify that we actually receive some frames (non-blocking)
    let mut got_any_samples = false;
    let read_deadline = Instant::now() + Duration::from_secs(3);
    let mut tmp = vec![0i16; 4096];
    while Instant::now() < read_deadline {
        let n = consumer.read(&mut tmp);
        if n > 0 {
            got_any_samples = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Clean up the capture thread
    capture.stop();

    // 6) Assertions
    assert!(
        found_in_pulseaudio,
        "stream not attached to default source: {}",
        default_source
    );
    assert!(
        got_any_samples,
        "no samples were captured from the input stream"
    );
}

#[cfg(target_os = "linux")]
fn get_default_source_name() -> Option<String> {
    // Prefer pactl get-default-source (newer), fallback to parsing pactl info
    if let Ok(out) = Command::new("pactl").arg("get-default-source").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    if let Ok(out) = Command::new("pactl").arg("info").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                if let Some(rest) = line.trim().strip_prefix("Default Source: ") {
                    let name = rest.trim();
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn find_our_source_output_source(app_name: &str) -> Option<String> {
    // Build a map of source index -> name
    let mut source_index_to_name: HashMap<String, String> = HashMap::new();
    if let Ok(out) = Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
    {
        if out.status.success() {
            let txt = String::from_utf8_lossy(&out.stdout);
            for line in txt.lines() {
                // Format: index\tname\tdriver\t... (tab-separated)
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    source_index_to_name
                        .insert(parts[0].trim().to_string(), parts[1].trim().to_string());
                }
            }
        }
    }

    // Scan source-outputs and locate our stream by application.name
    if let Ok(out) = Command::new("pactl")
        .args(["list", "source-outputs"])
        .output()
    {
        if out.status.success() {
            let txt = String::from_utf8_lossy(&out.stdout);
            let mut current_block_source_index: Option<String> = None;
            let mut current_block_is_ours = false;

            for line in txt.lines() {
                let line = line.trim();
                if line.starts_with("Source Output #") {
                    // New block
                    current_block_source_index = None;
                    current_block_is_ours = false;
                    continue;
                }

                if line.starts_with("Source: ") {
                    // On PulseAudio, this is a numeric source index
                    let idx = line.trim_start_matches("Source: ").trim().to_string();
                    current_block_source_index = Some(idx);
                    continue;
                }

                if line.starts_with("application.name = \"") && line.contains(app_name) {
                    current_block_is_ours = true;
                    continue;
                }

                // Block end: if we have both flags, resolve name
                if line.is_empty() && current_block_is_ours {
                    if let Some(idx) = current_block_source_index.take() {
                        if let Some(name) = source_index_to_name.get(&idx) {
                            return Some(name.clone());
                        }
                    }
                }
            }

            // In case the last block didn't end with an empty line
            if current_block_is_ours {
                if let Some(idx) = current_block_source_index {
                    if let Some(name) = source_index_to_name.get(&idx) {
                        return Some(name.clone());
                    }
                }
            }
        }
    }

    None
}
