//! Comprehensive VAD (Voice Activity Detection) tests
//!
//! Tests cover:
//! - Energy calculation (RMS, dBFS)
//! - Adaptive threshold (noise floor tracking, onset/offset)
//! - State machine (silence→speech→silence transitions, debouncing)
//! - VadConfig validation
//! - Speech boundary detection accuracy

use coldvox_vad::types::{VadConfig, VadEvent, VadState};
use coldvox_vad::constants::{FRAME_SIZE_SAMPLES, SAMPLE_RATE_HZ, FRAME_DURATION_MS};
use coldvox_vad::energy::EnergyCalculator;
use coldvox_vad::state::VadStateMachine;
use coldvox_vad::threshold::AdaptiveThreshold;

// ─── Energy Calculator Tests ─────────────────────────────────────────

#[test]
fn energy_silence_is_very_low_dbfs() {
    let calc = EnergyCalculator::new();
    let silence = vec![0i16; FRAME_SIZE_SAMPLES];
    let db = calc.calculate_dbfs(&silence);
    assert!(db <= -100.0, "silence should be <= -100 dBFS, got {}", db);
}

#[test]
fn energy_full_scale_near_zero_dbfs() {
    let calc = EnergyCalculator::new();
    let full = vec![i16::MAX; FRAME_SIZE_SAMPLES];
    let db = calc.calculate_dbfs(&full);
    assert!((db - 0.0).abs() < 0.1, "full scale should be ~0 dBFS, got {}", db);
}

#[test]
fn energy_rms_sine_wave() {
    let calc = EnergyCalculator::new();
    let sine: Vec<i16> = (0..FRAME_SIZE_SAMPLES)
        .map(|i| {
            let phase = 2.0 * std::f32::consts::PI * i as f32 / FRAME_SIZE_SAMPLES as f32;
            (phase.sin() * 16384.0) as i16
        })
        .collect();

    let rms = calc.calculate_rms(&sine);
    // Sine wave RMS = peak / sqrt(2) ≈ 0.707 * peak
    // 16384 / 32768 = 0.5, RMS ≈ 0.5 / sqrt(2) ≈ 0.354
    assert!((rms - 0.354).abs() < 0.02, "sine wave RMS should be ~0.354, got {}", rms);
}

#[test]
fn energy_rms_empty_frame_returns_zero() {
    let calc = EnergyCalculator::new();
    let empty: Vec<i16> = vec![];
    assert_eq!(calc.calculate_rms(&empty), 0.0);
}

#[test]
fn energy_dbfs_monotonically_increases_with_amplitude() {
    let calc = EnergyCalculator::new();
    let mut prev_db = f32::NEG_INFINITY;

    for amplitude in [100, 500, 1000, 5000, 10000, 20000, 30000] {
        let frame = vec![amplitude as i16; FRAME_SIZE_SAMPLES];
        let db = calc.calculate_dbfs(&frame);
        assert!(db > prev_db, "dBFS should increase with amplitude: {} dB at amplitude {}", db, amplitude);
        prev_db = db;
    }
}

#[test]
fn energy_ratio_positive_when_above_reference() {
    let calc = EnergyCalculator::new();
    let loud = vec![10000i16; FRAME_SIZE_SAMPLES];
    let ratio = calc.calculate_energy_ratio(&loud, -50.0);
    assert!(ratio > 0.0, "energy ratio should be positive above reference");
}

#[test]
fn energy_ratio_negative_when_below_reference() {
    let calc = EnergyCalculator::new();
    let quiet = vec![10i16; FRAME_SIZE_SAMPLES];
    let ratio = calc.calculate_energy_ratio(&quiet, -10.0);
    assert!(ratio < 0.0, "energy ratio should be negative below reference");
}

// ─── Adaptive Threshold Tests ────────────────────────────────────────

#[test]
fn threshold_initialization_from_config() {
    let config = VadConfig::default();
    let threshold = AdaptiveThreshold::new(&config);

    assert_eq!(threshold.current_floor(), -50.0);
    assert_eq!(threshold.onset_threshold(), -50.0 + 9.0);  // -41.0
    assert_eq!(threshold.offset_threshold(), -50.0 + 6.0); // -44.0
}

#[test]
fn threshold_adapts_noise_floor_during_silence() {
    let config = VadConfig {
        ema_alpha: 0.1,
        ..Default::default()
    };
    let mut t = AdaptiveThreshold::new(&config);

    t.update(-40.0, false);
    let floor_after_one = t.current_floor();
    // EMA: (1 - 0.1) * (-50) + 0.1 * (-40) = -45 + -4 = -49
    assert!((floor_after_one - (-49.0)).abs() < 0.01);
}

#[test]
fn threshold_does_not_adapt_during_speech() {
    let config = VadConfig::default();
    let mut t = AdaptiveThreshold::new(&config);

    let initial = t.current_floor();
    t.update(-30.0, true);
    t.update(-25.0, true);
    assert_eq!(t.current_floor(), initial);
}

#[test]
fn threshold_activation_detection() {
    let config = VadConfig::default();
    let t = AdaptiveThreshold::new(&config);

    // onset_threshold = -50 + 9 = -41
    assert!(t.should_activate(-40.0));   // above onset
    assert!(!t.should_activate(-42.0));  // below onset
}

#[test]
fn threshold_deactivation_detection() {
    let config = VadConfig::default();
    let t = AdaptiveThreshold::new(&config);

    // offset_threshold = -50 + 6 = -44
    assert!(t.should_deactivate(-45.0));  // below offset
    assert!(!t.should_deactivate(-43.0)); // above offset
}

#[test]
fn threshold_reset_restores_initial_floor() {
    let config = VadConfig {
        ema_alpha: 0.5,
        initial_floor_db: -50.0,
        ..Default::default()
    };
    let mut t = AdaptiveThreshold::new(&config);

    // Modify floor
    t.update(-30.0, false);
    assert_ne!(t.current_floor(), -50.0);

    // Reset
    t.reset(-50.0);
    assert_eq!(t.current_floor(), -50.0);
}

#[test]
fn threshold_floor_clamped_to_valid_range() {
    let config = VadConfig::default();
    let mut t = AdaptiveThreshold::new(&config);

    // Try extremely low value
    t.reset(-200.0);
    assert!(t.current_floor() >= -80.0, "floor should be clamped to min");

    // Try extremely high value
    t.reset(0.0);
    assert!(t.current_floor() <= -20.0, "floor should be clamped to max");
}

// ─── State Machine Tests ─────────────────────────────────────────────

#[test]
fn state_machine_starts_in_silence() {
    let config = VadConfig::default();
    let sm = VadStateMachine::new(&config);
    assert_eq!(sm.current_state(), VadState::Silence);
}

#[test]
fn state_machine_transitions_to_speech_after_debounce() {
    let config = VadConfig {
        speech_debounce_ms: 64,  // 2 frames at 32ms each
        ..Default::default()
    };
    let mut sm = VadStateMachine::new(&config);

    // First speech frame — still in debounce
    let event1 = sm.process(true, -30.0);
    assert!(event1.is_none());
    assert_eq!(sm.current_state(), VadState::Silence);

    // Second speech frame — debounce met, should transition
    let event2 = sm.process(true, -30.0);
    assert!(event2.is_some());
    if let Some(VadEvent::SpeechStart { energy_db, .. }) = event2 {
        assert!((energy_db - (-30.0)).abs() < 0.01);
    } else {
        panic!("Expected SpeechStart event");
    }
    assert_eq!(sm.current_state(), VadState::Speech);
}

#[test]
fn state_machine_transitions_to_silence_after_debounce() {
    let config = VadConfig {
        speech_debounce_ms: 32,   // 1 frame
        silence_debounce_ms: 64,  // 2 frames
        ..Default::default()
    };
    let mut sm = VadStateMachine::new(&config);

    // Enter speech state
    sm.process(true, -30.0);

    // First silence frame — still debouncing
    let ev1 = sm.process(false, -50.0);
    assert!(ev1.is_none());
    assert_eq!(sm.current_state(), VadState::Speech);

    // Second silence frame — debounce met
    let ev2 = sm.process(false, -50.0);
    assert!(ev2.is_some());
    if let Some(VadEvent::SpeechEnd { duration_ms, .. }) = ev2 {
        assert!(duration_ms > 0, "speech duration should be > 0");
    } else {
        panic!("Expected SpeechEnd event");
    }
    assert_eq!(sm.current_state(), VadState::Silence);
}

#[test]
fn state_machine_speech_interrupted_by_brief_silence() {
    let config = VadConfig {
        speech_debounce_ms: 32,   // 1 frame
        silence_debounce_ms: 96,  // 3 frames
        ..Default::default()
    };
    let mut sm = VadStateMachine::new(&config);

    // Enter speech
    sm.process(true, -30.0);
    assert_eq!(sm.current_state(), VadState::Speech);

    // Brief silence (1 frame) — doesn't meet debounce
    sm.process(false, -50.0);
    assert_eq!(sm.current_state(), VadState::Speech);

    // Back to speech — resets silence counter
    sm.process(true, -30.0);
    assert_eq!(sm.current_state(), VadState::Speech);
}

#[test]
fn state_machine_reset() {
    let config = VadConfig {
        speech_debounce_ms: 32,
        ..Default::default()
    };
    let mut sm = VadStateMachine::new(&config);

    // Enter speech
    sm.process(true, -30.0);
    assert_eq!(sm.current_state(), VadState::Speech);

    // Reset
    sm.reset();
    assert_eq!(sm.current_state(), VadState::Silence);
}

#[test]
fn state_machine_multiple_speech_segments() {
    let config = VadConfig {
        speech_debounce_ms: 32,
        silence_debounce_ms: 32,
        ..Default::default()
    };
    let mut sm = VadStateMachine::new(&config);

    let mut speech_starts = 0;
    let mut speech_ends = 0;

    // Simulate 3 speech segments with silence gaps
    for _segment in 0..3 {
        // Speech on
        for _ in 0..5 {
            if let Some(VadEvent::SpeechStart { .. }) = sm.process(true, -30.0) {
                speech_starts += 1;
            }
        }
        // Silence gap
        for _ in 0..5 {
            if let Some(VadEvent::SpeechEnd { .. }) = sm.process(false, -60.0) {
                speech_ends += 1;
            }
        }
    }

    assert_eq!(speech_starts, 3, "should have 3 speech starts");
    assert_eq!(speech_ends, 3, "should have 3 speech ends");
}

// ─── VadConfig Tests ─────────────────────────────────────────────────

#[test]
fn vad_config_default_values() {
    let config = VadConfig::default();
    assert_eq!(config.onset_threshold_db, 9.0);
    assert_eq!(config.offset_threshold_db, 6.0);
    assert_eq!(config.ema_alpha, 0.02);
    assert_eq!(config.speech_debounce_ms, 200);
    assert_eq!(config.silence_debounce_ms, 400);
    assert_eq!(config.initial_floor_db, -50.0);
    assert_eq!(config.frame_size_samples, FRAME_SIZE_SAMPLES);
    assert_eq!(config.sample_rate_hz, SAMPLE_RATE_HZ);
}

#[test]
fn vad_config_frame_duration_calculation() {
    let config = VadConfig::default();
    // 512 samples / 16000 Hz * 1000 = 32ms
    assert!((config.frame_duration_ms() - 32.0).abs() < 0.01);
}

#[test]
fn vad_config_debounce_frame_counts() {
    let config = VadConfig {
        speech_debounce_ms: 200,
        silence_debounce_ms: 400,
        frame_size_samples: 512,
        sample_rate_hz: 16000,
        ..Default::default()
    };

    // 200ms / 32ms = 6.25 → ceil = 7
    assert_eq!(config.speech_debounce_frames(), 7);
    // 400ms / 32ms = 12.5 → ceil = 13
    assert_eq!(config.silence_debounce_frames(), 13);
}

// ─── Constants Tests ─────────────────────────────────────────────────

#[test]
fn vad_constants_standard_values() {
    assert_eq!(SAMPLE_RATE_HZ, 16_000);
    assert_eq!(FRAME_SIZE_SAMPLES, 512);
    // 512 / 16000 * 1000 = 32.0
    assert!((FRAME_DURATION_MS - 32.0).abs() < 0.01);
}

// ─── Integration: Simulated Audio Pipeline ───────────────────────────

#[test]
fn vad_detects_speech_in_simulated_audio_stream() {
    let config = VadConfig {
        speech_debounce_ms: 32,   // 1 frame
        silence_debounce_ms: 64,  // 2 frames
        onset_threshold_db: 9.0,
        offset_threshold_db: 6.0,
        initial_floor_db: -50.0,
        ..Default::default()
    };
    let calc = EnergyCalculator::new();
    let mut threshold = AdaptiveThreshold::new(&config);
    let mut sm = VadStateMachine::new(&config);

    let mut events: Vec<VadEvent> = Vec::new();

    // Simulate: 5 silence frames → 10 speech frames → 5 silence frames
    let silence_frame = vec![0i16; FRAME_SIZE_SAMPLES];
    let speech_frame: Vec<i16> = (0..FRAME_SIZE_SAMPLES)
        .map(|i| {
            let phase = 2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0;
            (phase.sin() * 10000.0) as i16
        })
        .collect();

    // Silence warmup
    for _ in 0..5 {
        let db = calc.calculate_dbfs(&silence_frame);
        let is_speech = threshold.should_activate(db);
        threshold.update(db, is_speech);
        if let Some(ev) = sm.process(is_speech, db) {
            events.push(ev);
        }
    }

    // Speech
    for _ in 0..10 {
        let db = calc.calculate_dbfs(&speech_frame);
        let is_speech = threshold.should_activate(db);
        threshold.update(db, is_speech);
        if let Some(ev) = sm.process(is_speech, db) {
            events.push(ev);
        }
    }

    // Silence tail
    for _ in 0..5 {
        let db = calc.calculate_dbfs(&silence_frame);
        let is_speech = threshold.should_deactivate(db);
        // Note: during speech state, we check deactivation
        let below_offset = threshold.should_deactivate(db);
        threshold.update(db, !below_offset);
        if let Some(ev) = sm.process(!below_offset, db) {
            events.push(ev);
        }
    }

    // Should have detected at least one SpeechStart
    let starts: Vec<_> = events.iter().filter(|e| matches!(e, VadEvent::SpeechStart { .. })).collect();
    assert!(!starts.is_empty(), "should detect speech start; events: {:?}", events);
}
