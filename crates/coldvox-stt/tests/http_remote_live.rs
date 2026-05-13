//! Live integration test for the canonical Parakeet HTTP-remote plugin.
//!
//! Requires the `parakeet-cpu` docker-compose service to be running on
//! `http://localhost:5092`. Ignored by default; run with:
//!
//! ```
//! cargo test -p coldvox-stt --features http-remote \
//!   --test http_remote_live -- --ignored --nocapture
//! ```

#![cfg(feature = "http-remote")]

use std::path::PathBuf;

use coldvox_stt::plugin::SttPlugin;
use coldvox_stt::plugins::http_remote::{HttpRemoteConfig, HttpRemotePlugin};
use coldvox_stt::types::{TranscriptionConfig, TranscriptionEvent};

const EXPECTED_TRANSCRIPT: &str = "On august twenty seventh, eighteen thirty seven, she writes.";

fn load_test_1_samples() -> Vec<i16> {
    let wav_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/ parent")
        .join("app")
        .join("test_data")
        .join("test_1.wav");

    let mut reader = hound::WavReader::open(&wav_path)
        .unwrap_or_else(|e| panic!("open {}: {e}", wav_path.display()));
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000, "test_1.wav must be 16 kHz");
    assert_eq!(spec.channels, 1, "test_1.wav must be mono");

    reader
        .samples::<i16>()
        .map(|s| s.expect("wav sample"))
        .collect()
}

#[tokio::test]
#[ignore = "requires running parakeet-cpu container on :5092"]
async fn canonical_parakeet_cpu_transcribes_test_1_exactly() {
    let mut plugin = HttpRemotePlugin::new(HttpRemoteConfig::canonical_parakeet_cpu());

    assert!(
        plugin
            .is_available()
            .await
            .expect("is_available should not error"),
        "parakeet-cpu container must be healthy at http://localhost:5092/health \
         (run `docker compose -f ops/parakeet/docker-compose.yml up -d parakeet-cpu`)"
    );

    plugin
        .initialize(TranscriptionConfig::default())
        .await
        .expect("initialize");

    let samples = load_test_1_samples();
    assert!(!samples.is_empty(), "test_1.wav yielded no samples");

    plugin.process_audio(&samples).await.expect("process_audio");

    let event = plugin
        .finalize()
        .await
        .expect("finalize")
        .expect("plugin must emit Final for non-empty audio");

    match event {
        TranscriptionEvent::Final { text, .. } => {
            assert_eq!(
                text, EXPECTED_TRANSCRIPT,
                "parakeet-cpu transcript drifted from the canonical expected text"
            );
        }
        other => panic!("expected TranscriptionEvent::Final, got {other:?}"),
    }
}
