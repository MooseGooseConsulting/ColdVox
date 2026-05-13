// Logging behavior:
// - Writes logs to both stderr and a daily-rotated file at logs/coldvox.log.
// - Default log level is INFO to reduce verbosity. Control via RUST_LOG environment variable:
//   * RUST_LOG=info                     # Standard logging (default, recommended)
//   * RUST_LOG=debug                    # Verbose debugging (includes silence detection)
//   * RUST_LOG=trace                    # Maximum verbosity (includes every audio chunk)
//   * RUST_LOG=coldvox=info,stt_debug=trace  # Fine-grained per-module control
// - The logs/ directory is created on startup if missing; file output uses a non-blocking writer.
// - File layer disables ANSI to keep logs clean for analysis.
use std::fs;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;

use clap::Parser;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use coldvox_app::runtime::{self as app_runtime, ActivationMode as RuntimeMode, AppRuntimeOptions};
use coldvox_app::Settings;
use coldvox_audio::{DeviceManager, ResamplerQuality};
use coldvox_foundation::{AppState, HealthMonitor, ShutdownHandler, StateManager};

#[cfg(feature = "tui")]
use coldvox_app::tui;

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard, Box<dyn std::error::Error>>
{
    std::fs::create_dir_all("logs")?;
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "coldvox.log");
    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

    // Default to INFO level to reduce verbosity. Use RUST_LOG to override:
    // - RUST_LOG=trace                    # Maximum verbosity (includes all audio chunk logs)
    // - RUST_LOG=debug                    # Verbose debugging (includes silence detection)
    // - RUST_LOG=info                     # Standard logging (default, recommended)
    // - RUST_LOG=coldvox=info,stt_debug=trace  # Fine-grained control per module
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let stderr_layer = fmt::layer().with_writer(std::io::stderr);
    let file_layer = fmt::layer().with_writer(non_blocking_file).with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();
    Ok(guard)
}

/// Prune rotated log files in `logs/` older than `retention_days` days.
/// If `retention_days` is `Some(0)` pruning is disabled. Default is 7 days when `None`.
fn prune_old_logs(retention_days: Option<u64>) {
    let retention = retention_days.unwrap_or(7);
    if retention == 0 {
        tracing::debug!("Log retention disabled (retention_days=0)");
        return;
    }

    let cutoff = match SystemTime::now().checked_sub(Duration::from_secs(retention * 24 * 60 * 60))
    {
        Some(t) => t,
        None => return,
    };

    let logs_dir = Path::new("logs");
    if !logs_dir.exists() {
        return;
    }

    match fs::read_dir(logs_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    // Only consider rotated files with date suffix like `coldvox.log.YYYY-MM-DD`
                    if name.starts_with("coldvox.log.") {
                        if let Ok(meta) = entry.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if modified < cutoff {
                                    if let Err(e) = fs::remove_file(&path) {
                                        tracing::warn!(
                                            "Failed to remove old log {}: {}",
                                            path.display(),
                                            e
                                        );
                                    } else {
                                        tracing::info!("Removed old log file: {}", path.display());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => tracing::warn!("Failed to read logs directory for pruning: {}", e),
    }
}

#[derive(Parser, Debug)]
#[command(name = "coldvox", author, version, about = "ColdVox voice pipeline")]
struct Cli {
    /// List available input devices and exit
    #[arg(long = "list-devices")]
    list_devices: bool,

    /// Enable TUI dashboard
    #[arg(long = "tui")]
    tui: bool,

    /// Exit immediately if all injection methods fail
    #[arg(long = "injection-fail-fast")]
    injection_fail_fast: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Give PipeWire better routing hints if using its ALSA bridge (Linux only)
    #[cfg(target_os = "linux")]
    std::env::set_var(
        "PIPEWIRE_PROPS",
        "{ application.name=ColdVox media.role=capture }",
    );
    let _log_guard = init_logging()?;
    // Prune old rotated logs. Set COLDVOX_LOG_RETENTION_DAYS=0 to disable pruning.
    let retention_days = std::env::var("COLDVOX_LOG_RETENTION_DAYS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    prune_old_logs(retention_days);
    tracing::info!("Starting ColdVox application");

    let cli = Cli::parse();
    let mut settings = Settings::new().unwrap_or_else(|e| {
        tracing::error!("Failed to load settings: {}", e);
        Settings::default()
    });

    // Override settings with CLI flags
    if cli.injection_fail_fast {
        settings.injection.fail_fast = true;
    }

    if cli.list_devices {
        let dm = DeviceManager::new()?;
        tracing::info!("CPAL host: {:?}", dm.host_id());
        let devices = dm.enumerate_devices();
        println!("Input devices (host: {:?}):", dm.host_id());
        for d in devices {
            let def = if d.is_default { " (default)" } else { "" };
            println!("- {}{}", d.name, def);
        }
        return Ok(());
    }

    // Unified runtime start
    let state_manager = StateManager::new();
    let _health_monitor = HealthMonitor::new(Duration::from_secs(10)).start();
    let shutdown = ShutdownHandler::new().install().await;

    state_manager.transition(AppState::Running)?;
    tracing::info!("Application state: Running");

    let stt_selection = Some(
        settings
            .runtime_plugin_selection()
            .map_err(std::io::Error::other)?,
    );

    let device = settings.device.clone();
    let resampler_quality = match settings.resampler_quality.to_lowercase().as_str() {
        "fast" => ResamplerQuality::Fast,
        "quality" => ResamplerQuality::Quality,
        _ => ResamplerQuality::Balanced,
    };
    let activation_mode = match settings.activation_mode.as_str() {
        "vad" => RuntimeMode::Vad,
        "hotkey" => RuntimeMode::Hotkey,
        _ => RuntimeMode::Vad,
    };

    let mut opts = AppRuntimeOptions {
        device,
        resampler_quality,
        activation_mode,
        stt_selection,
        enable_device_monitor: settings.enable_device_monitor,
        capture_buffer_samples: settings.audio.capture_buffer_samples,
        ..Default::default()
    };

    opts.injection = Some(coldvox_app::runtime::InjectionOptions {
        enable: true,
        allow_kdotool: settings.injection.allow_kdotool,
        allow_enigo: settings.injection.allow_enigo,
        inject_on_unknown_focus: settings.injection.inject_on_unknown_focus,
        max_total_latency_ms: Some(settings.injection.max_total_latency_ms),
        per_method_timeout_ms: Some(settings.injection.per_method_timeout_ms),
        cooldown_initial_ms: Some(settings.injection.cooldown_initial_ms),
        fail_fast: settings.injection.fail_fast,
    });
    let app = app_runtime::start(opts)
        .await
        .map_err(|e| e as Box<dyn std::error::Error>)?;
    // make sharable for spawn + shutdown
    let app = std::sync::Arc::new(app);

    // Spawn TUI if requested
    #[cfg(feature = "tui")]
    if cli.tui {
        tracing::info!("Starting TUI dashboard...");
        tracing::debug!("About to call tui::run_tui - validating module import");
        let tui_app = app.clone();
        let tui_handle = tokio::spawn(async move {
            if let Err(e) = tui::run_tui(tui_app).await {
                tracing::error!("TUI error: {}", e);
            }
        });

        // Wait for TUI to complete
        if let Err(e) = tui_handle.await {
            tracing::error!("TUI task error: {}", e);
        }
    } else {
        // Standard mode: periodic stats log
        let mut stats_interval = tokio::time::interval(Duration::from_secs(30));
        let metrics = app.metrics.clone();
        tokio::select! {
            _ = shutdown.wait() => {
                tracing::debug!("Shutdown signal received");
            }
            _ = async {
                loop {
                    stats_interval.tick().await;
                    let cap_fps = metrics.capture_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let chk_fps = metrics.chunker_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let vad_fps = metrics.vad_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let cap_fill = metrics.capture_buffer_fill.load(std::sync::atomic::Ordering::Relaxed);
                    let chk_fill = metrics.chunker_buffer_fill.load(std::sync::atomic::Ordering::Relaxed);
                    tracing::info!(
                        capture_fps = cap_fps,
                        chunker_fps = chk_fps,
                        vad_fps = vad_fps,
                        capture_buffer_fill_pct = cap_fill,
                        chunker_buffer_fill_pct = chk_fill,
                        "Pipeline running..."
                    );
                }
            } => {}
        }
    }

    #[cfg(not(feature = "tui"))]
    {
        // Standard mode: periodic stats log
        let mut stats_interval = tokio::time::interval(Duration::from_secs(30));
        let metrics = app.metrics.clone();
        tokio::select! {
            _ = shutdown.wait() => {
                tracing::debug!("Shutdown signal received");
            }
            _ = async {
                loop {
                    stats_interval.tick().await;
                    let cap_fps = metrics.capture_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let chk_fps = metrics.chunker_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let vad_fps = metrics.vad_fps.load(std::sync::atomic::Ordering::Relaxed);
                    let cap_fill = metrics.capture_buffer_fill.load(std::sync::atomic::Ordering::Relaxed);
                    let chk_fill = metrics.chunker_buffer_fill.load(std::sync::atomic::Ordering::Relaxed);
                    tracing::info!(
                        capture_fps = cap_fps,
                        chunker_fps = chk_fps,
                        vad_fps = vad_fps,
                        capture_buffer_fill_pct = cap_fill,
                        chunker_buffer_fill_pct = chk_fill,
                        "Pipeline running..."
                    );
                }
            } => {}
        }
    }

    // Shutdown
    tracing::debug!("Beginning graceful shutdown");
    state_manager.transition(AppState::Stopping)?;
    // Shutdown directly on the Arc<AppHandle>
    app.shutdown().await;
    state_manager.transition(AppState::Stopped)?;
    tracing::debug!("Shutdown complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use serial_test::serial;
    use std::env;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = env::var(key).ok();
            env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.previous.take() {
                env::set_var(self.key, prev);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    #[test]
    #[serial]
    fn test_settings_new_default() {
        // Clean up any leftover env vars from previous tests
        env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        // Test default loading without file
        let settings = Settings::new().unwrap();
        assert_eq!(settings.resampler_quality.to_lowercase(), "balanced");
        assert_eq!(settings.activation_mode.to_lowercase(), "vad");
        assert_eq!(settings.injection.max_total_latency_ms, 800);
        assert!(settings.stt.failover_threshold > 0);
    }

    #[test]
    #[serial]
    fn test_settings_new_invalid_env_var_deserial() {
        // Clean up OTHER env vars from previous tests
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let _guard = EnvVarGuard::set("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS", "abc"); // Invalid for u64
        let result = Settings::new();
        // Use a more specific assertion to check for the expected error
        let err = result.expect_err("expected invalid env var to cause error");
        assert!(
            err.contains("invalid digit found in string")
                || err.contains("Failed to deserialize settings from config")
                || err.contains("deserialize"),
            "unexpected error message: {err}"
        );
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
    #[serial]
    fn test_settings_validate_invalid_mode() {
        // Clean up any leftover env vars from previous tests
        env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let mut settings = Settings::new().unwrap();
        settings.resampler_quality = "invalid".to_string();
        let result = settings.validate();
        assert!(result.is_ok()); // Warns but defaults applied
        assert_eq!(settings.resampler_quality, "balanced");
    }

    #[test]
    #[serial]
    fn test_settings_validate_invalid_rate() {
        // Clean up any leftover env vars from previous tests
        env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let mut settings = Settings::new().unwrap();
        settings.injection.keystroke_rate_cps = 200; // Too high
        let result = settings.validate();
        assert!(result.is_ok()); // Warns and clamps
        assert_eq!(settings.injection.keystroke_rate_cps, 20);
    }

    #[test]
    #[serial]
    fn test_settings_validate_success_rate() {
        // Clean up any leftover env vars from previous tests
        env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let mut settings = Settings::new().unwrap();
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
    #[serial]
    fn test_settings_new_with_env_override() {
        // Clean up any leftover env vars from previous tests
        env::remove_var("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let _guard = EnvVarGuard::set("COLDVOX__ACTIVATION_MODE", "hotkey");
        let settings = Settings::new().unwrap();
        assert_eq!(settings.activation_mode, "hotkey");
    }

    #[test]
    #[serial]
    fn test_settings_new_validation_err() {
        // Clean up OTHER env vars from previous tests
        env::remove_var("COLDVOX__ACTIVATION_MODE");
        let _guard_skip = EnvVarGuard::set("COLDVOX_SKIP_CONFIG_DISCOVERY", "1");
        let _guard = EnvVarGuard::set("COLDVOX__INJECTION__MAX_TOTAL_LATENCY_MS", "0");
        let result = Settings::new();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max_total_latency_ms"));
    }
}
