//! Foundation crate tests
//!
//! Tests cover:
//! - Clock abstraction (RealClock, TestClock, SharedClock)
//! - Error types (ColdVoxError variants, AudioError, SttError, VadError, InjectionError)
//! - Test environment utilities

use coldvox_foundation::clock::{Clock, RealClock, TestClock, real_clock, test_clock};
use coldvox_foundation::error::{
    AudioError, ColdVoxError, ConfigError, InjectionError, PluginError, SttError, VadError,
};
use std::time::{Duration, Instant};

// ─── RealClock Tests ────────────────────────────────────────────────

#[test]
fn real_clock_now_returns_current_time() {
    let clock = RealClock::new();
    let before = Instant::now();
    let clock_time = clock.now();
    let after = Instant::now();
    assert!(clock_time >= before);
    assert!(clock_time <= after);
}

#[test]
fn real_clock_factory_function() {
    let clock = real_clock();
    let t = clock.now();
    assert!(t.elapsed() < Duration::from_secs(1));
}

// ─── TestClock Tests ────────────────────────────────────────────────

#[test]
fn test_clock_starts_at_current_time() {
    let before = Instant::now();
    let clock = TestClock::new();
    let clock_time = clock.now();
    // TestClock initialized with Instant::now(), should be very close
    assert!(clock_time.duration_since(before) < Duration::from_millis(100));
}

#[test]
fn test_clock_advance() {
    let clock = TestClock::new();
    let t0 = clock.now();
    clock.advance(Duration::from_secs(5));
    let t1 = clock.now();
    assert_eq!(t1.duration_since(t0), Duration::from_secs(5));
}

#[test]
fn test_clock_advance_accumulates() {
    let clock = TestClock::new();
    let start = clock.now();
    clock.advance(Duration::from_millis(100));
    clock.advance(Duration::from_millis(200));
    clock.advance(Duration::from_millis(300));
    let elapsed = clock.now().duration_since(start);
    assert_eq!(elapsed, Duration::from_millis(600));
}

#[test]
fn test_clock_sleep_advances_time() {
    let clock = TestClock::new();
    let t0 = clock.now();
    clock.sleep(Duration::from_secs(10));
    let t1 = clock.now();
    assert_eq!(t1.duration_since(t0), Duration::from_secs(10));
}

#[test]
fn test_clock_set_time() {
    let clock = TestClock::new();
    let target = Instant::now() + Duration::from_secs(1000);
    clock.set_time(target);
    assert_eq!(clock.now(), target);
}

#[test]
fn test_clock_factory_function() {
    let clock = test_clock();
    let t = clock.now();
    clock.sleep(Duration::from_secs(1));
    let t2 = clock.now();
    assert_eq!(t2.duration_since(t), Duration::from_secs(1));
}

// ─── Error Type Tests ───────────────────────────────────────────────

#[test]
fn audio_error_device_not_found() {
    let err = AudioError::DeviceNotFound { name: Some("test_mic".to_string()) };
    let msg = format!("{}", err);
    assert!(msg.contains("test_mic"));
}

#[test]
fn audio_error_buffer_overflow() {
    let err = AudioError::BufferOverflow { count: 512 };
    let msg = format!("{}", err);
    assert!(msg.contains("512"));
}

#[test]
fn audio_error_format_not_supported() {
    let err = AudioError::FormatNotSupported { format: "f64".to_string() };
    let msg = format!("{}", err);
    assert!(msg.contains("f64"));
}

#[test]
fn stt_error_transcription_failed() {
    let err = SttError::TranscriptionFailed("timeout".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("timeout"));
}

#[test]
fn stt_error_model_not_found() {
    let err = SttError::ModelNotFound { path: "/models/whisper".into() };
    let msg = format!("{}", err);
    assert!(msg.contains("whisper"));
}

#[test]
fn vad_error_invalid_frame_size() {
    let err = VadError::InvalidFrameSize { expected: 512, actual: 256 };
    let msg = format!("{}", err);
    assert!(msg.contains("512"));
    assert!(msg.contains("256"));
}

#[test]
fn injection_error_timeout() {
    let err = InjectionError::Timeout(5000);
    let msg = format!("{}", err);
    assert!(msg.contains("5000"));
}

#[test]
fn injection_error_no_editable_focus() {
    let err = InjectionError::NoEditableFocus;
    let msg = format!("{}", err);
    assert!(msg.contains("editable focus"));
}

#[test]
fn coldvox_error_from_audio_error() {
    let audio_err = AudioError::DeviceDisconnected;
    let err: ColdVoxError = audio_err.into();
    assert!(matches!(err, ColdVoxError::Audio(_)));
}

#[test]
fn coldvox_error_from_stt_error() {
    let stt_err = SttError::TranscriptionFailed("test".to_string());
    let err: ColdVoxError = stt_err.into();
    assert!(matches!(err, ColdVoxError::Stt(_)));
}

#[test]
fn coldvox_error_from_vad_error() {
    let vad_err = VadError::ProcessingFailed("test".to_string());
    let err: ColdVoxError = vad_err.into();
    assert!(matches!(err, ColdVoxError::Vad(_)));
}

#[test]
fn coldvox_error_from_injection_error() {
    let inj_err = InjectionError::NoEditableFocus;
    let err: ColdVoxError = inj_err.into();
    assert!(matches!(err, ColdVoxError::Injection(_)));
}

#[test]
fn coldvox_error_shutdown() {
    let err = ColdVoxError::ShutdownRequested;
    let msg = format!("{}", err);
    assert!(msg.contains("Shutdown"));
}

#[test]
fn coldvox_error_fatal() {
    let err = ColdVoxError::Fatal("critical failure".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("critical failure"));
}

#[test]
fn config_error_validation() {
    let err = ConfigError::Validation {
        field: "sample_rate".to_string(),
        reason: "must be 16000".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("sample_rate"));
}

#[test]
fn plugin_error_lifecycle() {
    let err = PluginError::Lifecycle {
        plugin: "moonshine".to_string(),
        operation: "initialize".to_string(),
        reason: "Python not found".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("moonshine"));
    assert!(msg.contains("initialize"));
}
