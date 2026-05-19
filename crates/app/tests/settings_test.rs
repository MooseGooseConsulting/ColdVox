use coldvox_app::Settings;
use coldvox_foundation::skip_test_unless;
use serial_test::serial;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use tempfile::NamedTempFile;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var_os(key);
        env::set_var(key, value);
        Self { key, previous }
    }

    fn remove(key: &'static str) -> Self {
        let previous = env::var_os(key);
        env::remove_var(key);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            env::set_var(self.key, previous);
        } else {
            env::remove_var(self.key);
        }
    }
}

fn get_test_config_path() -> PathBuf {
    // Try workspace root first (for integration tests)
    let workspace_config = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("config/default.toml");

    if workspace_config.exists() {
        return workspace_config;
    }

    // Fallback to relative path from crate root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config/default.toml")
}

fn write_temp_config(contents: &str) -> NamedTempFile {
    let file = tempfile::Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("create temp config file");
    std::fs::write(file.path(), contents).expect("write temp config");
    file
}

#[test]
#[serial]
fn test_settings_new_default() {
    // Ensure we test pure defaults without loading repository config files
    let _skip_discovery = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
    let _preferred = EnvVarGuard::remove("COLDVOX__STT__PREFERRED");
    // Test default loading without file - Settings::new() will use defaults if no config found
    let settings = Settings::new().unwrap();
    assert_eq!(settings.resampler_quality.to_lowercase(), "balanced");
    assert_eq!(settings.activation_mode.to_lowercase(), "vad");
    assert_eq!(settings.injection.max_total_latency_ms, 800);
    assert_eq!(settings.stt.preferred.as_deref(), Some("mock"));
    assert!(settings.stt.failover_threshold > 0);
    assert_eq!(settings.stt.remote.base_url, "http://localhost:5092");
    assert_eq!(settings.stt.remote.api_path, "/v1/audio/transcriptions");
    assert_eq!(settings.stt.remote.health_path, "/health");
    assert_eq!(settings.stt.remote.model_name, "parakeet-tdt-0.6b-v2");
    assert_eq!(settings.stt.remote.timeout_ms, 15_000);
    assert_eq!(settings.stt.remote.sample_rate, 16_000);
    assert_eq!(settings.stt.remote.max_audio_bytes, 2_097_152);
    assert_eq!(settings.stt.remote.max_audio_seconds, 30);
    assert_eq!(settings.stt.remote.max_payload_bytes, 2_621_440);
    assert!(settings.stt.remote.headers.is_empty());
    assert!(settings.stt.remote.auth.bearer_token_env_var.is_none());
}

#[test]
#[serial]
fn test_settings_from_path_uses_mock_startup_and_repo_remote_transport_defaults() {
    let _preferred = EnvVarGuard::remove("COLDVOX__STT__PREFERRED");
    let settings = Settings::from_path(get_test_config_path()).expect("load default config");

    assert_eq!(settings.stt.preferred.as_deref(), Some("mock"));
    let selection = settings
        .runtime_plugin_selection()
        .expect("build startup plugin selection");
    assert_eq!(
        selection.preferred_plugin.as_deref(),
        Some("mock"),
        "repo-root config/plugins.json persistence must not select startup STT"
    );
    assert_eq!(settings.stt.remote.base_url, "http://localhost:5092");
    assert_eq!(settings.stt.remote.api_path, "/v1/audio/transcriptions");
    assert_eq!(settings.stt.remote.health_path, "/health");
    assert_eq!(settings.stt.remote.model_name, "parakeet-tdt-0.6b-v2");
    assert_eq!(settings.stt.remote.timeout_ms, 15_000);
    assert_eq!(settings.stt.remote.sample_rate, 16_000);
    assert_eq!(settings.stt.remote.max_audio_bytes, 2_097_152);
    assert_eq!(settings.stt.remote.max_audio_seconds, 30);
    assert_eq!(settings.stt.remote.max_payload_bytes, 2_621_440);
    assert!(settings.stt.remote.headers.is_empty());
    assert!(settings.stt.remote.auth.bearer_token_env_var.is_none());
}

#[test]
#[serial]
fn test_env_override_can_select_http_remote_startup() {
    let _preferred = EnvVarGuard::set("COLDVOX__STT__PREFERRED", "http-remote");

    let settings = Settings::from_path(get_test_config_path()).expect("load default config");
    let selection = settings
        .runtime_plugin_selection()
        .expect("build env-selected startup plugin selection");

    assert_eq!(settings.stt.preferred.as_deref(), Some("http-remote"));
    assert_eq!(selection.preferred_plugin.as_deref(), Some("http-remote"));
}

#[test]
fn test_settings_from_path_rejects_invalid_remote_values() {
    let config_file = write_temp_config(
        r#"
            [stt.remote]
            base_url = "https://localhost:5092"
            api_path = "v1/audio/transcriptions"
            health_path = "health"
            model_name = ""
            timeout_ms = 0
            sample_rate = 0
            max_audio_bytes = 4096
            max_audio_seconds = 0
            max_payload_bytes = 1024

            [stt.remote.auth]
            bearer_token_env_var = "   "
        "#,
    );

    let err = Settings::from_path(config_file.path()).expect_err("reject invalid remote config");
    assert!(
        err.contains("base_url 'https://localhost:5092' must start with http://"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("api_path 'v1/audio/transcriptions' must start with '/'"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("health_path 'health' must start with '/'"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("model_name must not be empty"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("timeout_ms must be >0"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("sample_rate must be >0"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("max_audio_seconds must be >0"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("max_payload_bytes (1024) must be >= max_audio_bytes (4096)"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains("auth.bearer_token_env_var must not be blank"),
        "unexpected error: {err}"
    );
}

#[test]
#[ignore]
fn test_settings_new_invalid_env_var_deserial() {
    skip_test_unless!(coldvox_foundation::test_env::TestRequirements::new()
        .requires_env_var("COLDVOX_TEST_ENV_OVERRIDE"));
    // Avoid reading files; exercise env-only path
    env::set_var("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
    // New environment parsing uses double-underscore segment separator
    env::set_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS", "abc"); // Invalid for u64
    let result = Settings::new();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("invalid digit found in string") || err.contains("deserialize"),
        "unexpected error message: {}",
        err
    );
    env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
    env::remove_var("COLDVOX_SKIP_CONFIG_DISCOVERY");
}

#[test]
fn test_settings_validate_zero_timeout() {
    let mut settings = Settings::default();
    settings.injection.max_total_latency_ms = 0;
    let result = settings.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("max_total_latency_ms"));
}

#[test]
fn test_settings_validate_invalid_mode() {
    let config_path = get_test_config_path();
    let mut settings = Settings::from_path(&config_path).expect("Failed to load config");
    settings.resampler_quality = "invalid".to_string();
    let result = settings.validate();
    assert!(result.is_ok()); // Warns but defaults applied
    assert_eq!(settings.resampler_quality, "balanced");
}

#[test]
fn test_settings_validate_invalid_rate() {
    let config_path = get_test_config_path();
    let mut settings = Settings::from_path(&config_path).expect("Failed to load config");
    settings.injection.keystroke_rate_cps = 200; // Too high
    let result = settings.validate();
    assert!(result.is_ok()); // Warns and clamps
    assert_eq!(settings.injection.keystroke_rate_cps, 20);
}

#[test]
fn test_settings_validate_success_rate() {
    let config_path = get_test_config_path();
    let mut settings = Settings::from_path(&config_path).expect("Failed to load config");
    settings.injection.min_success_rate = 1.5;
    let result = settings.validate();
    assert!(result.is_ok()); // Warns and clamps
    assert_eq!(settings.injection.min_success_rate, 0.3);
}

#[test]
fn test_settings_validate_zero_validation() {
    let mut settings = Settings::default();
    settings.stt.failover_threshold = 0;
    let result = settings.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("failover_threshold"));
}

#[test]
#[ignore]
fn test_settings_new_with_env_override() {
    skip_test_unless!(coldvox_foundation::test_env::TestRequirements::new()
        .requires_env_var("COLDVOX_TEST_ENV_OVERRIDE"));
    env::set_var("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
    // New environment parsing uses double-underscore segment separator
    env::set_var("COLDVOX__ACTIVATION_MODE", "hotkey");
    let settings = Settings::new().unwrap();
    assert_eq!(settings.activation_mode, "hotkey");
    env::remove_var("COLDVOX__ACTIVATION_MODE");
    env::remove_var("COLDVOX_SKIP_CONFIG_DISCOVERY");
}

#[test]
#[ignore]
fn test_settings_new_validation_err() {
    skip_test_unless!(coldvox_foundation::test_env::TestRequirements::new()
        .requires_env_var("COLDVOX_TEST_ENV_OVERRIDE"));
    env::set_var("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
    // New environment parsing uses double-underscore segment separator
    env::set_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS", "0");
    let result = Settings::new();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("max_total_latency_ms"));
    env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
    env::remove_var("COLDVOX_SKIP_CONFIG_DISCOVERY");
}
