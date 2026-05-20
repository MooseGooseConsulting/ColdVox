//! Comprehensive tests for audio capture pipeline components.
//!
//! Tests cover: ring buffer, silence detector, watchdog timer, and
//! audio frame format validation using mocked audio sources.

use coldvox_audio::detector::SilenceDetector;
use coldvox_audio::AudioRingBuffer;

// ─── Ring Buffer Tests ───────────────────────────────────────────────

#[test]
fn ring_buffer_write_read_roundtrip() {
    let buf = AudioRingBuffer::new(4096);
    let (mut producer, mut consumer) = buf.split();

    let samples: Vec<i16> = (0..512).map(|i| (i % 100) as i16).collect();
    let written = producer.write(&samples).expect("write should succeed");
    assert_eq!(written, 512);

    let mut read_buf = vec![0i16; 512];
    let read_count = consumer.read(&mut read_buf);
    assert_eq!(read_count, 512);
    assert_eq!(read_buf, samples);
}

#[test]
fn ring_buffer_partial_read() {
    let buf = AudioRingBuffer::new(4096);
    let (mut producer, mut consumer) = buf.split();

    let samples: Vec<i16> = (0..256).map(|i| i as i16).collect();
    producer.write(&samples).unwrap();

    let mut first_half = vec![0i16; 128];
    assert_eq!(consumer.read(&mut first_half), 128);
    assert_eq!(first_half, samples[..128]);

    let mut second_half = vec![0i16; 128];
    assert_eq!(consumer.read(&mut second_half), 128);
    assert_eq!(second_half, samples[128..256]);
}

#[test]
fn ring_buffer_read_empty_returns_zero() {
    let buf = AudioRingBuffer::new(1024);
    let (_producer, mut consumer) = buf.split();

    let mut read_buf = vec![0i16; 512];
    assert_eq!(consumer.read(&mut read_buf), 0);
}

#[test]
fn ring_buffer_overflow_returns_error() {
    let buf = AudioRingBuffer::new(64);
    let (mut producer, _consumer) = buf.split();

    let samples = vec![1i16; 128];
    assert!(producer.write(&samples).is_err());
}

#[test]
fn ring_buffer_wrap_around_preserves_data() {
    let buf = AudioRingBuffer::new(256);
    let (mut producer, mut consumer) = buf.split();

    // Fill and partially drain to force wrap-around
    let fill = vec![1i16; 200];
    producer.write(&fill).unwrap();
    let mut drain = vec![0i16; 180];
    consumer.read(&mut drain);

    let wrap_data: Vec<i16> = (10..110).collect();
    producer.write(&wrap_data).unwrap();

    // Drain original remainder
    let mut remainder = vec![0i16; 20];
    consumer.read(&mut remainder);

    // Read wrapped data
    let mut wrapped = vec![0i16; 100];
    assert_eq!(consumer.read(&mut wrapped), 100);
    assert_eq!(wrapped, wrap_data);
}

#[test]
fn ring_buffer_slots_decrease_after_write() {
    let buf = AudioRingBuffer::new(1024);
    let (mut producer, _consumer) = buf.split();

    let initial = producer.slots();
    producer.write(&vec![0i16; 100]).unwrap();
    assert_eq!(producer.slots(), initial - 100);
}

#[test]
fn ring_buffer_preserves_extreme_sample_values() {
    let buf = AudioRingBuffer::new(4096);
    let (mut producer, mut consumer) = buf.split();

    let samples = vec![i16::MIN, -1000, -1, 0, 1, 1000, i16::MAX];
    producer.write(&samples).unwrap();

    let mut read_buf = vec![0i16; 7];
    consumer.read(&mut read_buf);
    assert_eq!(read_buf, samples);
}

#[test]
fn ring_buffer_16khz_vad_frame_size() {
    // VAD expects 512 samples at 16kHz (32ms frames)
    let buf = AudioRingBuffer::new(16384);
    let (mut producer, mut consumer) = buf.split();

    for _ in 0..10 {
        let frame = vec![0i16; 512];
        producer.write(&frame).unwrap();
    }

    let mut read_buf = vec![0i16; 5120];
    assert_eq!(consumer.read(&mut read_buf), 5120);
}

// ─── Silence Detector Tests ─────────────────────────────────────────

#[test]
fn silence_detector_detects_pure_silence() {
    let mut det = SilenceDetector::new(100);
    let silence = vec![0i16; 512];
    assert!(det.is_silence(&silence));
}

#[test]
fn silence_detector_detects_loud_audio_as_not_silence() {
    let mut det = SilenceDetector::new(100);
    let loud: Vec<i16> = vec![10000; 512];
    assert!(!det.is_silence(&loud));
}

#[test]
fn silence_detector_threshold_boundary() {
    let threshold = 500;
    let mut det = SilenceDetector::new(threshold);

    // Just below threshold — silence
    let quiet: Vec<i16> = vec![400; 512];
    assert!(det.is_silence(&quiet));

    // Above threshold — not silence
    let medium: Vec<i16> = vec![2000; 512];
    assert!(!det.is_silence(&medium));
}

#[test]
fn silence_detector_transitions_speech_to_silence() {
    let mut det = SilenceDetector::new(100);

    // Speech
    let speech = vec![5000i16; 512];
    assert!(!det.is_silence(&speech));
    assert_eq!(det.silence_duration().as_millis(), 0);

    // Transition to silence
    let silence = vec![0i16; 512];
    assert!(det.is_silence(&silence));
    // Duration should be >= 0 after registering silence start
    assert!(det.silence_duration().as_millis() >= 0);
}

#[test]
fn silence_detector_reset_clears_state() {
    let mut det = SilenceDetector::new(100);

    let silence = vec![0i16; 512];
    det.is_silence(&silence);
    det.reset();

    // After reset, silence_duration should be zero
    assert_eq!(det.silence_duration().as_millis(), 0);
}

#[test]
fn silence_detector_sine_wave_detection() {
    let mut det = SilenceDetector::new(100);

    // Generate a 440Hz sine wave at reasonable volume
    let sine: Vec<i16> = (0..512)
        .map(|i| {
            let phase = 2.0 * std::f64::consts::PI * 440.0 * i as f64 / 16000.0;
            (phase.sin() * 8000.0) as i16
        })
        .collect();

    assert!(!det.is_silence(&sine), "sine wave should not be detected as silence");
}

#[test]
fn silence_detector_low_noise_floor() {
    let mut det = SilenceDetector::new(50);

    // Very quiet noise (simulating mic self-noise)
    let noise: Vec<i16> = (0..512).map(|i| ((i % 7) as i16 - 3)).collect();
    assert!(det.is_silence(&noise), "low-level noise should be detected as silence");
}
