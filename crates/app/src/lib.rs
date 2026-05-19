use coldvox_stt::plugin::PluginSelectionConfig;
#[cfg(feature = "http-remote")]
use coldvox_stt::plugins::http_remote::HttpRemoteConfig;
use config::{Case, Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct InjectionSettings {
    pub fail_fast: bool,
    pub allow_kdotool: bool,
    pub allow_enigo: bool,
    pub inject_on_unknown_focus: bool,
    pub require_focus: bool,
    pub pause_hotkey: String,
    pub redact_logs: bool,
    pub max_total_latency_ms: u64,
    pub per_method_timeout_ms: u64,
    pub paste_action_timeout_ms: u64,
    pub cooldown_initial_ms: u64,
    pub cooldown_backoff_factor: f64,
    pub cooldown_max_ms: u64,
    pub injection_mode: String,
    pub keystroke_rate_cps: u32,
    pub max_burst_chars: u32,
    pub paste_chunk_chars: u32,
    pub chunk_delay_ms: u64,
    pub focus_cache_duration_ms: u64,
    pub enable_window_detection: bool,
    pub clipboard_restore_delay_ms: u64,
    pub discovery_timeout_ms: u64,
    pub allowlist: Vec<String>,
    pub blocklist: Vec<String>,
    pub min_success_rate: f32,
    pub min_sample_size: u32,
}

impl Default for InjectionSettings {
    fn default() -> Self {
        Self {
            fail_fast: false,
            allow_kdotool: false,
            allow_enigo: false,
            inject_on_unknown_focus: true,
            require_focus: false,
            pause_hotkey: "".to_string(),
            redact_logs: true,
            max_total_latency_ms: 800,
            per_method_timeout_ms: 250,
            paste_action_timeout_ms: 200,
            cooldown_initial_ms: 10000,
            cooldown_backoff_factor: 2.0,
            cooldown_max_ms: 300000,
            injection_mode: "auto".to_string(),
            keystroke_rate_cps: 20,
            max_burst_chars: 50,
            paste_chunk_chars: 500,
            chunk_delay_ms: 30,
            focus_cache_duration_ms: 200,
            enable_window_detection: true,
            clipboard_restore_delay_ms: 500,
            discovery_timeout_ms: 1000,
            allowlist: Vec::new(),
            blocklist: Vec::new(),
            min_success_rate: 0.3,
            min_sample_size: 5,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct SttRemoteAuthSettings {
    pub bearer_token_env_var: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SttRemoteSettings {
    pub base_url: String,
    pub api_path: String,
    pub health_path: String,
    pub model_name: String,
    pub timeout_ms: u64,
    pub sample_rate: u32,
    pub headers: HashMap<String, String>,
    pub auth: SttRemoteAuthSettings,
    pub max_audio_bytes: u64,
    pub max_audio_seconds: u32,
    pub max_payload_bytes: u64,
}

impl Default for SttRemoteSettings {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:5092".to_string(),
            api_path: "/v1/audio/transcriptions".to_string(),
            health_path: "/health".to_string(),
            model_name: "parakeet-tdt-0.6b-v2".to_string(),
            timeout_ms: 15_000,
            sample_rate: 16_000,
            headers: HashMap::new(),
            auth: SttRemoteAuthSettings::default(),
            max_audio_bytes: 2_097_152,
            max_audio_seconds: 30,
            max_payload_bytes: 2_621_440,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SttSettings {
    pub preferred: Option<String>,
    pub fallbacks: Vec<String>,
    pub require_local: bool,
    pub max_mem_mb: Option<u32>,
    pub language: Option<String>,
    pub failover_threshold: u32,
    pub failover_cooldown_secs: u32,
    pub model_ttl_secs: u32,
    pub disable_gc: bool,
    pub metrics_log_interval_secs: u32,
    pub debug_dump_events: bool,
    pub auto_extract: bool,
    pub remote: SttRemoteSettings,
}

impl Default for SttSettings {
    fn default() -> Self {
        Self {
            preferred: None,
            fallbacks: Vec::new(),
            require_local: false,
            max_mem_mb: None,
            language: None,
            failover_threshold: 5,
            failover_cooldown_secs: 10,
            model_ttl_secs: 300,
            disable_gc: false,
            metrics_log_interval_secs: 30,
            debug_dump_events: false,
            auto_extract: true,
            remote: SttRemoteSettings::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AudioSettings {
    pub capture_buffer_samples: usize,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            capture_buffer_samples: 65_536,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub device: Option<String>,
    pub resampler_quality: String,
    pub enable_device_monitor: bool,
    pub activation_mode: String,
    pub audio: AudioSettings,
    pub injection: InjectionSettings,
    pub stt: SttSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            device: None,
            resampler_quality: "".to_string(), // Empty; config builder sets "balanced" if not overridden
            enable_device_monitor: true,
            activation_mode: "".to_string(), // Empty; config builder sets "vad" if not overridden
            audio: AudioSettings::default(),
            injection: InjectionSettings::default(),
            stt: SttSettings::default(),
        }
    }
}

impl Settings {
    fn build_runtime_plugin_selection_with_overrides(
        stt: &SttSettings,
        plugin_overrides: Option<&PluginSelectionConfig>,
    ) -> PluginSelectionConfig {
        PluginSelectionConfig {
            preferred_plugin: stt
                .preferred
                .clone()
                .or_else(|| plugin_overrides.and_then(|cfg| cfg.preferred_plugin.clone())),
            fallback_plugins: if stt.fallbacks.is_empty() {
                plugin_overrides
                    .map(|cfg| cfg.fallback_plugins.clone())
                    .unwrap_or_default()
            } else {
                stt.fallbacks.clone()
            },
            require_local: stt.require_local,
            max_memory_mb: stt.max_mem_mb,
            required_language: stt.language.clone(),
            failover: Some(coldvox_stt::plugin::FailoverConfig {
                failover_threshold: stt.failover_threshold,
                failover_cooldown_secs: stt.failover_cooldown_secs,
            }),
            gc_policy: Some(coldvox_stt::plugin::GcPolicy {
                model_ttl_secs: stt.model_ttl_secs,
                enabled: !stt.disable_gc,
            }),
            metrics: Some(coldvox_stt::plugin::MetricsConfig {
                log_interval_secs: if stt.metrics_log_interval_secs == 0 {
                    None
                } else {
                    Some(stt.metrics_log_interval_secs)
                },
                debug_dump_events: stt.debug_dump_events,
            }),
            auto_extract_model: stt.auto_extract,
        }
    }

    fn build_config(explicit_path: Option<PathBuf>) -> Result<Config, ConfigError> {
        let mut builder = Config::builder()
            .set_default("resampler_quality", "balanced")?
            .set_default("activation_mode", "vad")?
            .set_default("enable_device_monitor", true)?
            // Audio settings defaults
            .set_default("audio.capture_buffer_samples", 65_536)?
            // Injection settings defaults
            .set_default("injection.fail_fast", false)?
            .set_default("injection.allow_kdotool", false)?
            .set_default("injection.allow_enigo", false)?
            .set_default("injection.inject_on_unknown_focus", true)?
            .set_default("injection.require_focus", false)?
            .set_default("injection.pause_hotkey", "")?
            .set_default("injection.redact_logs", true)?
            .set_default("injection.max_total_latency_ms", 800)?
            .set_default("injection.per_method_timeout_ms", 250)?
            .set_default("injection.paste_action_timeout_ms", 200)?
            .set_default("injection.cooldown_initial_ms", 10000)?
            .set_default("injection.cooldown_backoff_factor", 2.0)?
            .set_default("injection.cooldown_max_ms", 300000)?
            .set_default("injection.injection_mode", "auto")?
            .set_default("injection.keystroke_rate_cps", 20)?
            .set_default("injection.max_burst_chars", 50)?
            .set_default("injection.paste_chunk_chars", 500)?
            .set_default("injection.chunk_delay_ms", 30)?
            .set_default("injection.focus_cache_duration_ms", 200)?
            .set_default("injection.enable_window_detection", true)?
            .set_default("injection.clipboard_restore_delay_ms", 500)?
            .set_default("injection.discovery_timeout_ms", 1000)?
            .set_default("injection.allowlist", Vec::<String>::new())?
            .set_default("injection.blocklist", Vec::<String>::new())?
            .set_default("injection.min_success_rate", 0.3)?
            .set_default("injection.min_sample_size", 5)?
            // STT settings defaults
            .set_default("stt.preferred", Option::<String>::None)?
            .set_default("stt.fallbacks", Vec::<String>::new())?
            .set_default("stt.require_local", false)?
            .set_default("stt.max_mem_mb", Option::<u32>::None)?
            .set_default("stt.language", Option::<String>::None)?
            .set_default("stt.failover_threshold", 5)?
            .set_default("stt.failover_cooldown_secs", 10)?
            .set_default("stt.model_ttl_secs", 300)?
            .set_default("stt.disable_gc", false)?
            .set_default("stt.metrics_log_interval_secs", 30)?
            .set_default("stt.debug_dump_events", false)?
            .set_default("stt.auto_extract", true)?
            .set_default("stt.remote.base_url", "http://localhost:5092")?
            .set_default("stt.remote.api_path", "/v1/audio/transcriptions")?
            .set_default("stt.remote.health_path", "/health")?
            .set_default("stt.remote.model_name", "parakeet-tdt-0.6b-v2")?
            .set_default("stt.remote.timeout_ms", 15_000)?
            .set_default("stt.remote.sample_rate", 16_000)?
            .set_default("stt.remote.headers", HashMap::<String, String>::new())?
            .set_default(
                "stt.remote.auth.bearer_token_env_var",
                Option::<String>::None,
            )?
            .set_default("stt.remote.max_audio_bytes", 2_097_152)?
            .set_default("stt.remote.max_audio_seconds", 30)?
            .set_default("stt.remote.max_payload_bytes", 2_621_440)?;

        // Allow tests or callers to skip config file discovery entirely
        let skip_discovery = std::env::var("COLDVOX_SKIP_CONFIG_DISCOVERY")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Check if a config file will be loaded
        let config_file_exists = if skip_discovery {
            false
        } else if let Some(path) = &explicit_path {
            path.exists()
        } else {
            Self::discover_config_path().is_some()
        };

        if let Some(path) = explicit_path {
            if path.exists() {
                builder = builder.add_source(File::from(path));
            } else {
                return Err(ConfigError::Message(format!(
                    "Config file not found at {}",
                    path.display()
                )));
            }
        } else if !skip_discovery {
            if let Some(path) = Self::discover_config_path() {
                builder = builder.add_source(File::from(path));
            }
        }

        builder = builder.add_source(
            Environment::with_prefix("COLDVOX")
                .separator("__")
                .convert_case(Case::Snake)
                .try_parsing(true),
        );

        let config = builder.build()?;

        // Log a warning if no config file was found
        if !config_file_exists {
            tracing::warn!("No config file found, using default values only");
        }

        Ok(config)
    }

    /// Resolve the startup config path.
    ///
    /// `COLDVOX_CONFIG_PATH` wins first so live profiles can override the
    /// checked-in default without changing `config/default.toml`.
    fn discover_config_path() -> Option<PathBuf> {
        if let Ok(custom) = env::var("COLDVOX_CONFIG_PATH") {
            let path = PathBuf::from(custom);
            if path.exists() {
                tracing::info!(
                    "Using startup config from COLDVOX_CONFIG_PATH: {}",
                    path.display()
                );
                return Some(path);
            }
        }

        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            let candidate = Path::new(&manifest_dir)
                .join("../..")
                .join("config/default.toml");
            if candidate.exists() {
                return Some(candidate);
            }
        }

        let cwd_candidate = PathBuf::from("config/default.toml");
        if cwd_candidate.exists() {
            return Some(cwd_candidate);
        }

        if let Ok(cwd) = env::current_dir() {
            for ancestor in cwd.ancestors() {
                let candidate = ancestor.join("config/default.toml");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        if let Ok(xdg_home) = env::var("XDG_CONFIG_HOME") {
            let candidate = Path::new(&xdg_home).join("coldvox/default.toml");
            if candidate.exists() {
                return Some(candidate);
            }
        }

        if let Ok(home) = env::var("HOME") {
            let candidate = Path::new(&home).join(".config/coldvox/default.toml");
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    pub fn runtime_plugin_selection(&self) -> Result<PluginSelectionConfig, String> {
        let plugin_overrides = load_canonical_plugin_selection_config()?;
        let selection = Self::build_runtime_plugin_selection_with_overrides(
            &self.stt,
            plugin_overrides.as_ref(),
        );
        selection
            .validate_runtime_policy()
            .map_err(|err| err.to_string())?;
        Ok(selection)
    }

    #[cfg(feature = "http-remote")]
    pub fn runtime_http_remote_config(&self) -> HttpRemoteConfig {
        HttpRemoteConfig {
            profile_id: Some("http-remote".to_string()),
            base_url: self.stt.remote.base_url.clone(),
            api_path: self.stt.remote.api_path.clone(),
            health_path: self.stt.remote.health_path.clone(),
            model_name: self.stt.remote.model_name.clone(),
            display_name: "Parakeet CPU (HTTP)".to_string(),
            timeout_ms: self.stt.remote.timeout_ms,
            sample_rate: self.stt.remote.sample_rate,
            headers: self.stt.remote.headers.clone(),
            bearer_token_env_var: self.stt.remote.auth.bearer_token_env_var.clone(),
            max_audio_bytes: self.stt.remote.max_audio_bytes,
            max_audio_seconds: self.stt.remote.max_audio_seconds,
            max_payload_bytes: self.stt.remote.max_payload_bytes,
        }
    }

    /// Load settings from a specific config file path (for tests)
    pub fn from_path(config_path: impl AsRef<Path>) -> Result<Self, String> {
        let config = Self::build_config(Some(config_path.as_ref().to_path_buf()))
            .map_err(|e| format!("Failed to build config: {}", e))?;

        let mut settings: Settings = config
            .try_deserialize()
            .map_err(|e| format!("Failed to deserialize settings: {}", e))?;

        settings.validate().map_err(|e| e.to_string())?;
        Ok(settings)
    }

    pub fn new() -> Result<Self, String> {
        let config = Self::build_config(None)
            .map_err(|e| format!("Failed to build config (likely invalid env vars): {}", e))?;

        let mut settings: Settings = config
            .try_deserialize()
            .map_err(|e| format!("Failed to deserialize settings from config: {}", e))?;

        settings.validate().map_err(|e| e.to_string())?;

        Ok(settings)
    }

    pub fn validate(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();

        // Validate resampler_quality
        if !["fast", "balanced", "quality"]
            .contains(&self.resampler_quality.to_lowercase().as_str())
        {
            tracing::warn!(
                "Invalid resampler_quality '{}'. Defaulting to 'balanced'.",
                self.resampler_quality
            );
            self.resampler_quality = "balanced".to_string();
        }

        // Validate activation_mode
        if !["vad", "hotkey"].contains(&self.activation_mode.to_lowercase().as_str()) {
            tracing::warn!(
                "Invalid activation_mode '{}'. Defaulting to 'vad'.",
                self.activation_mode
            );
            self.activation_mode = "vad".to_string();
        }

        // Validate injection settings
        if self.injection.max_total_latency_ms == 0 {
            errors.push("Injection max_total_latency_ms must be >0".to_string());
        }
        if self.injection.per_method_timeout_ms == 0 {
            errors.push("Injection per_method_timeout_ms must be >0".to_string());
        }
        if self.injection.paste_action_timeout_ms == 0 {
            errors.push("Injection paste_action_timeout_ms must be >0".to_string());
        }
        if self.injection.cooldown_initial_ms == 0 {
            errors.push("Injection cooldown_initial_ms must be >0".to_string());
        }
        if self.injection.cooldown_max_ms == 0 {
            errors.push("Injection cooldown_max_ms must be >0".to_string());
        }
        if self.injection.cooldown_backoff_factor <= 0.0
            || self.injection.cooldown_backoff_factor > 10.0
        {
            tracing::warn!(
                "Invalid cooldown_backoff_factor {}. Clamping to 2.0.",
                self.injection.cooldown_backoff_factor
            );
            self.injection.cooldown_backoff_factor = 2.0;
        }
        if !["keystroke", "paste", "auto"]
            .contains(&self.injection.injection_mode.to_lowercase().as_str())
        {
            tracing::warn!(
                "Invalid injection_mode '{}'. Defaulting to 'auto'.",
                self.injection.injection_mode
            );
            self.injection.injection_mode = "auto".to_string();
        }
        if self.injection.keystroke_rate_cps == 0 || self.injection.keystroke_rate_cps > 100 {
            tracing::warn!(
                "Invalid keystroke_rate_cps {}. Clamping to 20.",
                self.injection.keystroke_rate_cps
            );
            self.injection.keystroke_rate_cps = 20;
        }
        if self.injection.max_burst_chars == 0 {
            errors.push("Injection max_burst_chars must be >0".to_string());
        }
        if self.injection.paste_chunk_chars == 0 {
            errors.push("Injection paste_chunk_chars must be >0".to_string());
        }
        if self.injection.chunk_delay_ms == 0 {
            errors.push("Injection chunk_delay_ms must be >0".to_string());
        }
        if self.injection.focus_cache_duration_ms == 0 {
            errors.push("Injection focus_cache_duration_ms must be >0".to_string());
        }
        if self.injection.clipboard_restore_delay_ms == 0 {
            errors.push("Injection clipboard_restore_delay_ms must be >0".to_string());
        }
        if self.injection.discovery_timeout_ms == 0 {
            errors.push("Injection discovery_timeout_ms must be >0".to_string());
        }
        if self.injection.min_success_rate < 0.0 || self.injection.min_success_rate > 1.0 {
            tracing::warn!(
                "Invalid min_success_rate {}. Clamping to 0.3.",
                self.injection.min_success_rate
            );
            self.injection.min_success_rate = 0.3;
        }
        if self.injection.min_sample_size == 0 {
            errors.push("Injection min_sample_size must be >0".to_string());
        }

        // Validate STT settings
        if self.stt.failover_threshold == 0 {
            errors.push("STT failover_threshold must be >0".to_string());
        }
        if self.stt.failover_cooldown_secs == 0 {
            errors.push("STT failover_cooldown_secs must be >0".to_string());
        }
        if self.stt.model_ttl_secs == 0 {
            errors.push("STT model_ttl_secs must be >0".to_string());
        }
        if self.stt.remote.base_url.trim().is_empty() {
            errors.push("STT remote base_url must not be empty".to_string());
        } else if !self.stt.remote.base_url.starts_with("http://") {
            errors.push(format!(
                "STT remote base_url '{}' must start with http://",
                self.stt.remote.base_url
            ));
        }
        if self.stt.remote.api_path.trim().is_empty() {
            errors.push("STT remote api_path must not be empty".to_string());
        } else if !self.stt.remote.api_path.starts_with('/') {
            errors.push(format!(
                "STT remote api_path '{}' must start with '/'",
                self.stt.remote.api_path
            ));
        }
        if self.stt.remote.health_path.trim().is_empty() {
            errors.push("STT remote health_path must not be empty".to_string());
        } else if !self.stt.remote.health_path.starts_with('/') {
            errors.push(format!(
                "STT remote health_path '{}' must start with '/'",
                self.stt.remote.health_path
            ));
        }
        if self.stt.remote.model_name.trim().is_empty() {
            errors.push("STT remote model_name must not be empty".to_string());
        }
        if self.stt.remote.timeout_ms == 0 {
            errors.push("STT remote timeout_ms must be >0".to_string());
        }
        if self.stt.remote.sample_rate == 0 {
            errors.push("STT remote sample_rate must be >0".to_string());
        }
        if self.stt.remote.max_audio_bytes == 0 {
            errors.push("STT remote max_audio_bytes must be >0".to_string());
        }
        if self.stt.remote.max_audio_seconds == 0 {
            errors.push("STT remote max_audio_seconds must be >0".to_string());
        }
        if self.stt.remote.max_payload_bytes == 0 {
            errors.push("STT remote max_payload_bytes must be >0".to_string());
        }
        if self.stt.remote.max_payload_bytes < self.stt.remote.max_audio_bytes {
            errors.push(format!(
                "STT remote max_payload_bytes ({}) must be >= max_audio_bytes ({})",
                self.stt.remote.max_payload_bytes, self.stt.remote.max_audio_bytes
            ));
        }
        for header_name in self.stt.remote.headers.keys() {
            if header_name.trim().is_empty() {
                errors.push("STT remote headers must not contain an empty header name".to_string());
                break;
            }
        }
        if let Some(env_var) = &self.stt.remote.auth.bearer_token_env_var {
            if env_var.trim().is_empty() {
                errors.push(
                    "STT remote auth.bearer_token_env_var must not be blank when set".to_string(),
                );
            }
        }

        if !errors.is_empty() {
            let error_msg = format!("Critical config validation errors: {:?}", errors);
            return Err(error_msg);
        }

        // Log non-critical warnings if any were applied
        tracing::info!("Configuration validation completed successfully.");

        Ok(())
    }
}

pub(crate) fn discover_plugin_selection_config_path() -> Option<PathBuf> {
    if let Some(custom) = env::var_os("COLDVOX_PLUGIN_CONFIG_PATH") {
        let path = PathBuf::from(custom);
        if path.is_file() {
            tracing::info!(
                "Using plugin config from COLDVOX_PLUGIN_CONFIG_PATH: {}",
                path.display()
            );
            return Some(path);
        }

        tracing::warn!(
            "COLDVOX_PLUGIN_CONFIG_PATH is set but does not point to an existing file: {}",
            path.display()
        );
        return None;
    }

    if explicit_startup_config_path().is_some() {
        return None;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from);
    let cwd = env::current_dir().ok();
    discover_plugin_selection_config_path_with(manifest_dir.as_deref(), cwd.as_deref())
}

fn discover_plugin_selection_config_path_with(
    manifest_dir: Option<&Path>,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(manifest_dir) = manifest_dir {
        let candidate = manifest_dir.join("../..").join("config/plugins.json");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(cwd) = cwd {
        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join("config/plugins.json");
            if candidate.exists() && !is_legacy_app_local_plugin_config_path(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

fn explicit_startup_config_path() -> Option<PathBuf> {
    let custom = env::var_os("COLDVOX_CONFIG_PATH")?;
    let path = PathBuf::from(custom);
    path.exists().then_some(path)
}

fn is_legacy_app_local_plugin_config_path(path: &Path) -> bool {
    let mut reversed = path.iter().rev();
    matches!(
        (
            reversed.next(),
            reversed.next(),
            reversed.next(),
            reversed.next()
        ),
        (Some(file), Some(config), Some(app), Some(crates))
            if file == "plugins.json" && config == "config" && app == "app" && crates == "crates"
    )
}

pub(crate) fn load_canonical_plugin_selection_config(
) -> Result<Option<PluginSelectionConfig>, String> {
    let Some(path) = discover_plugin_selection_config_path() else {
        return Ok(None);
    };

    let raw = fs::read_to_string(&path).map_err(|err| {
        format!(
            "Failed to read plugin selection config {}: {}",
            path.display(),
            err
        )
    })?;
    let config: PluginSelectionConfig = serde_json::from_str(&raw).map_err(|err| {
        format!(
            "Failed to parse plugin selection config {}: {}",
            path.display(),
            err
        )
    })?;
    config
        .validate_runtime_policy()
        .map_err(|err| err.to_string())?;
    Ok(Some(config))
}

pub mod audio;
pub mod clock;
pub mod foundation;
pub mod hotkey;
pub mod probes;
pub mod runtime;
pub mod sleep_instrumentation;
pub mod stt;
pub mod telemetry;
pub mod text_injection;
#[cfg(feature = "tui")]
pub mod tui;
pub mod vad;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn build_runtime_plugin_selection_prefers_settings_config() {
        let stt = SttSettings {
            preferred: Some("mock".to_string()),
            fallbacks: vec!["mock".to_string()],
            ..Default::default()
        };

        let selection = Settings::build_runtime_plugin_selection_with_overrides(
            &stt,
            Some(&PluginSelectionConfig {
                preferred_plugin: Some("http-remote".to_string()),
                fallback_plugins: vec![],
                require_local: false,
                max_memory_mb: None,
                required_language: Some("en".to_string()),
                failover: None,
                gc_policy: None,
                metrics: None,
                auto_extract_model: true,
            }),
        );

        assert_eq!(selection.preferred_plugin.as_deref(), Some("mock"));
        assert_eq!(selection.fallback_plugins, vec!["mock"]);
    }

    #[cfg(feature = "http-remote")]
    #[test]
    fn runtime_http_remote_config_matches_validated_settings() {
        let settings = Settings::from_path(PathBuf::from("../../config/default.toml"))
            .or_else(|_| Settings::from_path(PathBuf::from("config/default.toml")))
            .expect("load default config");

        let remote = settings.runtime_http_remote_config();
        assert_eq!(remote.profile_id.as_deref(), Some("http-remote"));
        assert_eq!(remote.base_url, "http://localhost:5092");
        assert_eq!(remote.api_path, "/v1/audio/transcriptions");
        assert_eq!(remote.health_path, "/health");
        assert_eq!(remote.model_name, "parakeet-tdt-0.6b-v2");
        assert_eq!(remote.max_audio_bytes, 2_097_152);
        assert_eq!(remote.max_payload_bytes, 2_621_440);
    }

    #[test]
    fn discover_plugin_selection_config_path_skips_app_local_copy() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let repo_root = temp.path();
        let root_config_dir = repo_root.join("config");
        let app_config_dir = repo_root.join("crates/app/config");
        fs::create_dir_all(&root_config_dir).expect("create root config dir");
        fs::create_dir_all(&app_config_dir).expect("create app config dir");
        fs::write(root_config_dir.join("plugins.json"), "{}").expect("write root plugin config");
        fs::write(app_config_dir.join("plugins.json"), "{}")
            .expect("write app-local plugin config");

        let resolved = discover_plugin_selection_config_path_with(
            Some(&repo_root.join("crates/app")),
            Some(&repo_root.join("crates/app")),
        )
        .expect("resolve canonical plugin config path");
        let resolved = resolved
            .canonicalize()
            .expect("canonicalize resolved plugin config path");
        let expected = repo_root
            .join("config/plugins.json")
            .canonicalize()
            .expect("canonicalize expected plugin config path");

        assert_eq!(resolved, expected);
    }

    #[test]
    fn repo_root_plugins_json_keeps_canonical_http_remote_profile() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let root_plugins_path = repo_root.join("config/plugins.json");
        let app_local_plugins_path = repo_root.join("crates/app/config/plugins.json");

        let raw =
            fs::read_to_string(&root_plugins_path).expect("read repo-root plugin selection config");
        let root_config: PluginSelectionConfig =
            serde_json::from_str(&raw).expect("parse repo-root plugin selection config");

        assert_eq!(root_config.preferred_plugin.as_deref(), Some("http-remote"));
        assert!(
            root_config.fallback_plugins.is_empty(),
            "canonical root plugin config must not fall back to mock/noop: {:?}",
            root_config.fallback_plugins
        );
        assert_eq!(root_config.required_language.as_deref(), Some("en"));
        assert!(!root_config.require_local);

        let resolved = discover_plugin_selection_config_path()
            .expect("resolve canonical plugin selection config path")
            .canonicalize()
            .expect("canonicalize resolved canonical plugin selection config path");
        let expected = root_plugins_path
            .canonicalize()
            .expect("canonicalize repo-root plugin selection config path");

        assert_eq!(resolved, expected);

        // The deprecated app-local path should either not exist (deleted) or not be used
        if app_local_plugins_path.exists() {
            let deprecated = app_local_plugins_path
                .canonicalize()
                .expect("canonicalize deprecated app-local plugin selection config path");
            assert_ne!(resolved, deprecated);
        }
        // If the deprecated file doesn't exist, the test passes implicitly since
        // the resolved path correctly points to the repo-root config
    }

    #[test]
    #[serial]
    fn explicit_startup_config_disables_implicit_plugin_overrides() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let repo_root = temp.path();
        let root_config_dir = repo_root.join("config");
        let app_dir = repo_root.join("crates/app");
        let explicit_config_path = root_config_dir.join("windows-parakeet.toml");
        fs::create_dir_all(&root_config_dir).expect("create root config dir");
        fs::create_dir_all(&app_dir).expect("create app dir");
        fs::write(root_config_dir.join("plugins.json"), "{}").expect("write root plugin config");
        fs::write(&explicit_config_path, "[stt]\npreferred = \"parakeet\"\n")
            .expect("write explicit startup config");

        let original_cwd = env::current_dir().expect("capture cwd");
        let original_config = env::var_os("COLDVOX_CONFIG_PATH");
        let original_plugin = env::var_os("COLDVOX_PLUGIN_CONFIG_PATH");

        env::set_current_dir(&app_dir).expect("set cwd");
        env::set_var("COLDVOX_CONFIG_PATH", &explicit_config_path);
        env::remove_var("COLDVOX_PLUGIN_CONFIG_PATH");

        let resolved = discover_plugin_selection_config_path();

        if let Some(value) = original_config {
            env::set_var("COLDVOX_CONFIG_PATH", value);
        } else {
            env::remove_var("COLDVOX_CONFIG_PATH");
        }
        if let Some(value) = original_plugin {
            env::set_var("COLDVOX_PLUGIN_CONFIG_PATH", value);
        } else {
            env::remove_var("COLDVOX_PLUGIN_CONFIG_PATH");
        }
        env::set_current_dir(original_cwd).expect("restore cwd");

        assert!(resolved.is_none());
    }

    #[test]
    #[serial]
    fn invalid_startup_config_keeps_implicit_plugin_overrides_available() {
        let temp = tempfile::tempdir().expect("create tempdir");
        let repo_root = temp.path();
        let root_config_dir = repo_root.join("config");
        let app_dir = repo_root.join("crates/app");
        let root_plugins_path = root_config_dir.join("plugins.json");
        let invalid_config_path = root_config_dir.join("missing.toml");
        fs::create_dir_all(&root_config_dir).expect("create root config dir");
        fs::create_dir_all(&app_dir).expect("create app dir");
        fs::write(&root_plugins_path, "{}").expect("write root plugin config");

        let original_config = env::var_os("COLDVOX_CONFIG_PATH");
        let original_plugin = env::var_os("COLDVOX_PLUGIN_CONFIG_PATH");

        env::set_var("COLDVOX_CONFIG_PATH", &invalid_config_path);
        env::remove_var("COLDVOX_PLUGIN_CONFIG_PATH");

        assert!(explicit_startup_config_path().is_none());

        let resolved = discover_plugin_selection_config_path_with(Some(&app_dir), Some(&app_dir))
            .expect("resolve canonical plugin config path")
            .canonicalize()
            .expect("canonicalize resolved plugin config path");

        if let Some(value) = original_config {
            env::set_var("COLDVOX_CONFIG_PATH", value);
        } else {
            env::remove_var("COLDVOX_CONFIG_PATH");
        }
        if let Some(value) = original_plugin {
            env::set_var("COLDVOX_PLUGIN_CONFIG_PATH", value);
        } else {
            env::remove_var("COLDVOX_PLUGIN_CONFIG_PATH");
        }

        let expected = root_plugins_path
            .canonicalize()
            .expect("canonicalize expected plugin config path");
        assert_eq!(resolved, expected);
    }
}
