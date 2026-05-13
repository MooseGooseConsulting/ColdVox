//! End-to-end wiring test: Settings → runtime_plugin_selection →
//! runtime_http_remote_config → HttpRemotePlugin → real parakeet-cpu.
//!
//! Closes the app↔plugin integration gap that unit tests do not cover.
//! Requires the parakeet-cpu docker-compose service on :5092. Run with:
//!
//! ```
//! cargo test -p coldvox-app --features http-remote \
//!   --test http_remote_wiring_live -- --ignored --nocapture
//! ```

#![cfg(feature = "http-remote")]

use std::path::PathBuf;

use coldvox_app::Settings;
use coldvox_stt::plugin::SttPlugin;
use coldvox_stt::plugins::http_remote::HttpRemotePlugin;
use coldvox_stt::types::{TranscriptionConfig, TranscriptionEvent};
use serial_test::serial;

const EXPECTED_TRANSCRIPT: &str = "On august twenty seventh, eighteen thirty seven, she writes.";

fn repo_root() -> PathBuf {
    // crates/app/ -> crates/ -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("locate repo root")
        .to_path_buf()
}

fn load_test_1_samples() -> Vec<i16> {
    let wav = repo_root().join("crates/app/test_data/test_1.wav");
    let mut reader =
        hound::WavReader::open(&wav).unwrap_or_else(|e| panic!("open {}: {e}", wav.display()));
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    reader
        .samples::<i16>()
        .map(|s| s.expect("wav sample"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires running parakeet-cpu container on :5092"]
#[serial]
async fn windows_parakeet_settings_wire_through_to_real_transcript() {
    // Mirror what scripts/run-coldvox.ps1 does on Windows: point at the
    // windows-parakeet override. We drive Settings::from_path directly so
    // the test does not depend on discovery order.
    let config_path = repo_root().join("config/windows-parakeet.toml");
    let settings = Settings::from_path(&config_path).expect("load settings");

    // Step 1: the selection resolves to the canonical http-remote plugin.
    let selection = settings
        .runtime_plugin_selection()
        .expect("runtime plugin selection must succeed");
    assert_eq!(
        selection.preferred_plugin.as_deref(),
        Some("http-remote"),
        "windows-parakeet.toml must wire to the http-remote plugin"
    );

    // Step 2: the resolved remote config must point at the canonical
    // parakeet-cpu endpoint and model — same contract main.rs depends on.
    let remote = settings.runtime_http_remote_config();
    assert_eq!(remote.base_url, "http://localhost:5092");
    assert_eq!(remote.api_path, "/v1/audio/transcriptions");
    assert_eq!(remote.health_path, "/health");
    assert_eq!(remote.model_name, "parakeet-tdt-0.6b-v2");

    // Step 3: instantiate the plugin from that settings-derived config and
    // drive it end-to-end against the real container.
    let mut plugin = HttpRemotePlugin::new(remote);
    assert!(
        plugin.is_available().await.expect("health probe"),
        "parakeet-cpu container must be healthy at http://localhost:5092/health"
    );

    plugin
        .initialize(TranscriptionConfig::default())
        .await
        .expect("initialize");

    plugin
        .process_audio(&load_test_1_samples())
        .await
        .expect("process_audio");

    let event = plugin
        .finalize()
        .await
        .expect("finalize")
        .expect("plugin must emit Final for non-empty audio");

    match event {
        TranscriptionEvent::Final { text, .. } => assert_eq!(
            text, EXPECTED_TRANSCRIPT,
            "settings-wired parakeet transcript drifted from canonical expected text"
        ),
        other => panic!("expected TranscriptionEvent::Final, got {other:?}"),
    }
}
