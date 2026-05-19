//! STT Plugin Manager for ColdVox
//!
//! This module manages STT plugin selection and fallback logic

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use std::sync::atomic::Ordering;

use coldvox_foundation::error::{ColdVoxError, ConfigError, PluginError, SttError};
use coldvox_stt::plugin::{PluginSelectionConfig, SttPlugin, SttPluginRegistry};
#[cfg(feature = "http-remote")]
use coldvox_stt::plugins::http_remote::{HttpRemoteConfig, HttpRemotePluginFactory};
use coldvox_stt::TranscriptionConfig;
use coldvox_telemetry::pipeline_metrics::PipelineMetrics;
use serde_json;
use tokio::fs;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

/// Manages STT plugin lifecycle and selection
pub struct SttPluginManager {
    registry: Arc<RwLock<SttPluginRegistry>>,
    current_plugin: Arc<RwLock<Option<Box<dyn SttPlugin>>>>,
    selection_config: PluginSelectionConfig,

    // Failover tracking
    consecutive_errors: Arc<RwLock<HashMap<String, u32>>>,
    last_failover: Arc<RwLock<Option<Instant>>>,
    failed_plugins_cooldown: Arc<RwLock<HashMap<String, Instant>>>,

    // GC management
    gc_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    last_activity: Arc<RwLock<HashMap<String, Instant>>>,

    // Metrics (internal counters + optional shared pipeline metrics sink)
    failover_count: Arc<std::sync::atomic::AtomicU64>,
    total_errors: Arc<std::sync::atomic::AtomicU64>,
    metrics_sink: Option<Arc<PipelineMetrics>>,
    start_instant: Instant,
    metrics_task: Arc<RwLock<Option<JoinHandle<()>>>>,

    // Configuration persistence
    config_path: PathBuf,

    // Idempotent unload tracking
    last_unloaded_plugin_id: Arc<RwLock<Option<String>>>,
}

impl Default for SttPluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SttPluginManager {
    #[cfg(feature = "http-remote")]
    pub async fn configure_http_remote_factory(&mut self, config: HttpRemoteConfig) {
        let mut registry = self.registry.write().await;
        registry.register_or_replace(Box::new(HttpRemotePluginFactory::new(config)));
    }

    /// Warn if legacy or duplicate plugin selection config files are present.
    /// Canonical location is `config/plugins.json`. Any other copies will be ignored.
    fn warn_on_duplicate_configs(&self) {
        use std::fs;
        let canonical = std::path::Path::new("config/plugins.json");
        let legacy_paths = [
            std::path::Path::new("plugins.json"),
            std::path::Path::new("crates/app/plugins.json"),
            std::path::Path::new("crates/app/config/plugins.json"),
        ];

        for p in legacy_paths.iter() {
            if *p == canonical {
                continue;
            }
            if fs::metadata(p).is_ok() {
                warn!(
                    target: "coldvox::stt",
                    path = %p.display(),
                    canonical = %canonical.display(),
                    "Detected legacy plugins.json at '{}' (deprecated, ignored). Use '{}' instead.",
                    p.display(),
                    canonical.display()
                );
            }
        }
    }
    /// Create a new plugin manager with default configuration
    pub fn new() -> Self {
        let config_path = crate::discover_plugin_selection_config_path()
            .unwrap_or_else(|| PathBuf::from("config/plugins.json"));
        Self::new_with_config_path(config_path)
    }

    /// Create a new plugin manager with custom config path
    pub fn new_with_config_path(config_path: PathBuf) -> Self {
        let mut registry = SttPluginRegistry::new();

        // Register built-in plugins
        Self::register_builtin_plugins(&mut registry);

        let mut manager = Self {
            registry: Arc::new(RwLock::new(registry)),
            current_plugin: Arc::new(RwLock::new(None)),
            selection_config: PluginSelectionConfig::default(),
            consecutive_errors: Arc::new(RwLock::new(HashMap::new())),
            last_failover: Arc::new(RwLock::new(None)),
            failed_plugins_cooldown: Arc::new(RwLock::new(HashMap::new())),
            gc_task: Arc::new(RwLock::new(None)),
            last_activity: Arc::new(RwLock::new(HashMap::new())),
            failover_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            metrics_sink: None,
            start_instant: Instant::now(),
            metrics_task: Arc::new(RwLock::new(None)),
            config_path,
            last_unloaded_plugin_id: Arc::new(RwLock::new(None)),
        };

        if let Err(err) = manager.load_config_sync() {
            warn!(
                target: "coldvox::stt",
                error = %err,
                config_path = %manager.config_path.display(),
                "Failed to load plugin configuration during startup"
            );
        }

        manager
    }

    /// Set custom configuration file path
    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = path;
        self
    }

    /// Attach a shared PipelineMetrics sink for STT metric propagation
    pub fn with_metrics_sink(mut self, metrics: Arc<PipelineMetrics>) -> Self {
        self.metrics_sink = Some(metrics);
        self
    }

    /// Set / replace metrics sink after construction
    pub fn set_metrics_sink(&mut self, metrics: Arc<PipelineMetrics>) {
        self.metrics_sink = Some(metrics);
    }

    /// Update plugin selection configuration at runtime
    pub async fn set_selection_config(
        &mut self,
        cfg: PluginSelectionConfig,
    ) -> Result<(), ColdVoxError> {
        self.apply_selection_config(cfg, true).await
    }

    /// Apply plugin selection for the current process without rewriting persisted config.
    pub async fn set_runtime_selection_config(
        &mut self,
        cfg: PluginSelectionConfig,
    ) -> Result<(), ColdVoxError> {
        self.apply_selection_config(cfg, false).await
    }

    async fn apply_selection_config(
        &mut self,
        cfg: PluginSelectionConfig,
        persist: bool,
    ) -> Result<(), ColdVoxError> {
        cfg.validate_runtime_policy()?;
        let gc_enabled = cfg.gc_policy.as_ref().is_some_and(|gc| gc.enabled);
        let metrics_enabled = cfg.metrics.is_some();

        self.selection_config = cfg;

        if persist {
            // Save configuration to disk only for explicit persistence updates.
            if let Err(e) = self.save_config().await {
                warn!(
                    target: "coldvox::stt",
                    error = ?e,
                    "Failed to save plugin configuration"
                );
            }
        }

        // Start or stop GC task based on configuration
        if gc_enabled {
            self.start_gc_task().await;
        } else {
            self.stop_gc_task().await;
        }

        // Start/stop metrics logging task
        if metrics_enabled {
            if let Err(e) = self.start_metrics_task().await {
                warn!(target: "coldvox::stt", error = ?e, "Failed to start metrics task");
            }
        } else {
            self.stop_metrics_task().await;
        }

        info!(
            target: "coldvox::stt",
            event = "config_updated",
            persisted = persist,
            "Updated STT plugin selection configuration"
        );

        Ok(())
    }

    fn load_config_sync(&mut self) -> Result<(), ColdVoxError> {
        if !self.config_path.exists() {
            return Ok(());
        }

        let config_data = std::fs::read_to_string(&self.config_path)
            .map_err(|e| ConfigError::MissingField(format!("Failed to read config file: {}", e)))?;
        let config: PluginSelectionConfig = serde_json::from_str(&config_data).map_err(|e| {
            ConfigError::Parse(config::ConfigError::Message(format!(
                "Failed to parse config: {}",
                e
            )))
        })?;
        config.validate_runtime_policy()?;
        self.selection_config = config;

        info!(
            target: "coldvox::stt",
            config_path = %self.config_path.display(),
            "Loaded plugin configuration from disk"
        );

        Ok(())
    }

    /// Load configuration from disk
    async fn load_config(&mut self) -> Result<(), ColdVoxError> {
        if !self.config_path.exists() {
            debug!(
                target: "coldvox::stt",
                config_path = %self.config_path.display(),
                "Configuration file does not exist, using defaults"
            );
            return Ok(());
        }
        let config_data = fs::read_to_string(&self.config_path)
            .await
            .map_err(|e| ConfigError::MissingField(format!("Failed to read config file: {}", e)))?;
        let config: PluginSelectionConfig = serde_json::from_str(&config_data).map_err(|e| {
            ConfigError::Parse(config::ConfigError::Message(format!(
                "Failed to parse config: {}",
                e
            )))
        })?;
        config.validate_runtime_policy()?;

        // Apply loaded configuration
        self.set_selection_config(config).await?;

        info!(
            target: "coldvox::stt",
            config_path = %self.config_path.display(),
            "Loaded plugin configuration from disk"
        );

        Ok(())
    }

    /// Save current configuration to disk
    async fn save_config(&self) -> Result<(), ColdVoxError> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ConfigError::Validation {
                    field: "config".to_string(),
                    reason: format!("Failed to create config directory: {}", e),
                })?;
        }
        let config_data = serde_json::to_string_pretty(&self.selection_config).map_err(|e| {
            ConfigError::Parse(config::ConfigError::Message(format!(
                "Failed to serialize config: {}",
                e
            )))
        })?;
        fs::write(&self.config_path, config_data)
            .await
            .map_err(|e| ConfigError::Validation {
                field: "config".to_string(),
                reason: format!("Failed to write config file: {}", e),
            })?;
        debug!(
            target: "coldvox::stt",
            config_path = %self.config_path.display(),
            "Saved plugin configuration to disk"
        );

        Ok(())
    }

    /// Get the current configuration file path
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// Set a new configuration file path and reload if file exists
    pub async fn set_config_path(&mut self, path: PathBuf) -> Result<(), ColdVoxError> {
        self.config_path = path;
        self.load_config().await
    }

    /// Start the garbage collection task
    async fn start_gc_task(&self) {
        let gc_policy = match &self.selection_config.gc_policy {
            Some(policy) if policy.enabled => policy.clone(),
            _ => return,
        };

        let mut gc_task = self.gc_task.write().await;

        // Stop existing task if running
        if let Some(handle) = gc_task.take() {
            handle.abort();
        }

        let last_activity = self.last_activity.clone();
        let current_plugin = self.current_plugin.clone();
        let metrics_sink = self.metrics_sink.clone();
        let ttl_secs = if gc_policy.model_ttl_secs == 0 {
            1
        } else {
            gc_policy.model_ttl_secs
        };

        // Spawn new GC task
        let handle = tokio::spawn(async move {
            let interval_secs = (ttl_secs / 2).max(1);
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_secs.into()));

            loop {
                interval.tick().await;

                let now = Instant::now();

                // First, collect the IDs of inactive plugins
                let inactive_plugins: Vec<String> = {
                    let activity = last_activity.read().await;
                    let current = current_plugin.read().await;
                    let current_id = current.as_ref().map(|p| p.info().id.clone());

                    activity
                        .iter()
                        .filter_map(|(plugin_id, last_used)| {
                            // NEVER GC the actively selected plugin (#284)
                            if Some(plugin_id.clone()) == current_id {
                                return None;
                            }
                            if now.duration_since(*last_used).as_secs() > ttl_secs as u64 {
                                Some(plugin_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect()
                };

                // Process each inactive plugin
                for plugin_id in inactive_plugins {
                    debug!(
                        target: "coldvox::stt",
                        plugin_id = %plugin_id,
                        event = "gc_unload",
                        "GC: Unloading inactive plugin"
                    );

                    // Check if this is the current plugin and unload it
                    let mut plugin_guard = current_plugin.write().await;
                    if let Some(ref mut plugin) = *plugin_guard {
                        if plugin.info().id == plugin_id {
                            match plugin.unload().await {
                                Ok(()) => {
                                    info!(
                                        target: "coldvox::stt",
                                        plugin_id = %plugin_id,
                                        event = "gc_unload_success",
                                        "GC: Successfully unloaded plugin"
                                    );
                                    // Clear the current plugin after successful unload
                                    *plugin_guard = None;

                                    // Update metrics if available
                                    if let Some(ref metrics) = metrics_sink {
                                        metrics
                                            .stt_unload_count
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }
                                }
                                Err(ColdVoxError::Plugin(PluginError::Lifecycle { .. })) => {
                                    // Plugin is already unloaded, just clear it
                                    info!(
                                        target: "coldvox::stt",
                                        plugin_id = %plugin_id,
                                        event = "gc_already_unloaded",
                                        "GC: Plugin was already unloaded"
                                    );
                                    *plugin_guard = None;
                                }
                                Err(e) => {
                                    warn!(
                                        target: "coldvox::stt",
                                        plugin_id = %plugin_id,
                                        event = "gc_unload_error",
                                        error = ?e,
                                        "GC: Failed to unload plugin"
                                    );
                                    // Don't clear the plugin on unload failure to avoid data loss

                                    // Update error metrics if available
                                    if let Some(ref metrics) = metrics_sink {
                                        metrics
                                            .stt_unload_errors
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                    }

                    // Remove from activity tracking
                    let mut activity = last_activity.write().await;
                    activity.remove(&plugin_id);
                }
            }
        });

        *gc_task = Some(handle);
        info!(
            target: "coldvox::stt",
            event = "gc_task_started",
            ttl_seconds = ttl_secs,
            "Started STT plugin GC task"
        );
    }
    /// Stop the garbage collection task
    pub async fn stop_gc_task(&self) {
        let mut gc_task = self.gc_task.write().await;
        if let Some(handle) = gc_task.take() {
            handle.abort();
            debug!(
                target: "coldvox::stt",
                event = "gc_task_stopped",
                "Stopped STT plugin GC task"
            );
        }
    }

    /// Start the metrics logging task
    async fn start_metrics_task(&self) -> Result<(), ColdVoxError> {
        let metrics_sink = match self.metrics_sink {
            Some(ref m) => m.clone(),
            None => return Ok(()),
        };
        let mut metrics_task = self.metrics_task.write().await;
        if metrics_task.is_some() {
            return Ok(()); // Already running
        }

        // Get log interval from configuration, default to 30 seconds
        let log_interval_secs = self
            .selection_config
            .metrics
            .as_ref()
            .and_then(|m| m.log_interval_secs)
            .unwrap_or(30);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(log_interval_secs as u64));

            loop {
                interval.tick().await;

                // Log current plugin metrics
                info!(
                    target: "coldvox::stt::metrics",
                    active_plugins = metrics_sink.stt_active_plugins.load(Ordering::Relaxed),
                    load_count = metrics_sink.stt_load_count.load(Ordering::Relaxed),
                    load_errors = metrics_sink.stt_load_errors.load(Ordering::Relaxed),
                    unload_count = metrics_sink.stt_unload_count.load(Ordering::Relaxed),
                    unload_errors = metrics_sink.stt_unload_errors.load(Ordering::Relaxed),
                    failover_count = metrics_sink.stt_failover_count.load(Ordering::Relaxed),
                    transcription_requests = metrics_sink.stt_transcription_requests.load(Ordering::Relaxed),
                    transcription_success = metrics_sink.stt_transcription_success.load(Ordering::Relaxed),
                    transcription_failures = metrics_sink.stt_transcription_failures.load(Ordering::Relaxed),
                    gc_runs = metrics_sink.stt_gc_runs.load(Ordering::Relaxed),
                    "STT plugin metrics summary"
                );
            }
        });
        *metrics_task = Some(handle);
        Ok(())
    }

    pub async fn stop_metrics_task(&self) {
        let mut task_guard = self.metrics_task.write().await;
        if let Some(handle) = task_guard.take() {
            handle.abort();
        }
        debug!(
            target: "coldvox::stt",
            event = "metrics_task_stopped",
            "Stopped STT metrics logging task"
        );
    }

    /// Garbage collect inactive plugin models.
    pub async fn gc_inactive_models(&self) {
        if let Some(ref metrics) = self.metrics_sink {
            metrics.stt_gc_runs.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(ref metrics) = self.metrics_sink {
            metrics.stt_gc_runs.fetch_add(1, Ordering::Relaxed);
        }

        let gc_policy = match &self.selection_config.gc_policy {
            Some(policy) if policy.enabled => policy,
            _ => return,
        };

        let time_threshold = Instant::now();
        let ttl_secs = gc_policy.model_ttl_secs as u64;

        // First, collect the IDs of inactive plugins
        let inactive_plugins: Vec<String> = {
            let activity = self.last_activity.read().await;
            let current = self.current_plugin.read().await;
            let current_id = current.as_ref().map(|p| p.info().id.clone());

            activity
                .iter()
                .filter_map(|(plugin_id, last_used)| {
                    // NEVER GC the actively selected plugin (#284)
                    if Some(plugin_id.clone()) == current_id {
                        return None;
                    }
                    if time_threshold.duration_since(*last_used).as_secs() > ttl_secs {
                        Some(plugin_id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Process each inactive plugin
        for plugin_id in inactive_plugins {
            debug!(
                target: "coldvox::stt",
                plugin_id = %plugin_id,
                event = "gc_unload",
                "GC: Unloading inactive plugin"
            );

            // Check if this is the current plugin and unload it
            let mut current_plugin = self.current_plugin.write().await;
            if let Some(ref mut plugin) = *current_plugin {
                if plugin.info().id == plugin_id {
                    match plugin.unload().await {
                        Ok(()) => {
                            debug!(
                                target: "coldvox::stt",
                                plugin_id = %plugin_id,
                                event = "gc_unload_success",
                                "GC: Successfully unloaded plugin"
                            );
                            // Clear the current plugin after successful unload
                            *current_plugin = None;

                            // Update metrics if available
                            if let Some(ref metrics) = self.metrics_sink {
                                metrics
                                    .stt_unload_count
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        Err(ColdVoxError::Plugin(PluginError::Lifecycle { .. })) => {
                            // Plugin is already unloaded, just clear it
                            debug!(
                                target: "coldvox::stt",
                                plugin_id = %plugin_id,
                                event = "gc_already_unloaded",
                                "GC: Plugin was already unloaded"
                            );
                            *current_plugin = None;
                        }
                        Err(e) => {
                            warn!(
                                target: "coldvox::stt",
                                plugin_id = %plugin_id,
                                event = "gc_unload_error",
                                error = ?e,
                                "GC: Failed to unload plugin"
                            );
                            // Don't clear the plugin on unload failure to avoid data loss

                            // Update error metrics if available
                            if let Some(ref metrics) = self.metrics_sink {
                                metrics
                                    .stt_unload_errors
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }
                }
            }

            // Remove from activity tracking
            let mut activity = self.last_activity.write().await;
            activity.remove(&plugin_id);
        }

        trace!("GC completed, {} plugins remain active", {
            let activity = self.last_activity.read().await;
            activity.len()
        });
    }

    fn register_builtin_plugins(_registry: &mut SttPluginRegistry) {
        // Register mock plugin for tests
        #[cfg(test)]
        {
            use coldvox_stt::plugins::mock::MockPluginFactory;
            _registry.register(Box::new(MockPluginFactory::default()));
        }

        #[cfg(feature = "http-remote")]
        {
            _registry.register(Box::new(HttpRemotePluginFactory::canonical_parakeet_cpu()));
            _registry.register(Box::new(HttpRemotePluginFactory::parakeet_gpu()));
        }

        // Register Parakeet plugin if the parakeet feature is enabled
        #[cfg(feature = "parakeet")]
        {
            use coldvox_stt::plugins::parakeet::ParakeetPluginFactory;
            _registry.register(Box::new(ParakeetPluginFactory::new()));
        }

        // Register Moonshine plugin if the moonshine feature is enabled
        #[cfg(feature = "moonshine")]
        {
            use coldvox_stt::plugins::moonshine::MoonshinePluginFactory;
            _registry.register(Box::new(MoonshinePluginFactory::new()));
        }

        // Register Moonshine plugin if the moonshine feature is enabled
        #[cfg(feature = "moonshine")]
        {
            use coldvox_stt::plugins::moonshine::MoonshinePluginFactory;
            registry.register(Box::new(MoonshinePluginFactory::new()));
        }
    }

    /// Initialize the plugin manager and select the best available plugin
    pub async fn initialize(&mut self) -> Result<String, ColdVoxError> {
        // Surface any legacy/duplicate config files to help users consolidate configs
        self.warn_on_duplicate_configs();
        self.selection_config.validate_runtime_policy()?;
        let registry = self.registry.read().await;
        let init_start = Instant::now();

        // List available plugins
        let available = registry.available_plugins();
        info!(
            target: "coldvox::stt",
            event = "plugin_discovery",
            plugin_count = available.len(),
            "Available STT plugins discovered"
        );
        for plugin_info in &available {
            info!(
                target: "coldvox::stt",
                plugin_id = %plugin_info.id,
                plugin_name = %plugin_info.name,
                event = "plugin_info",
                available = plugin_info.is_available,
                "Plugin discovered: {} - {}",
                plugin_info.name,
                plugin_info.description
            );
        }

        // Try to create the best available plugin
        let plugin_result = if let Some(ref preferred) = self.selection_config.preferred_plugin {
            // Try preferred plugin first
            match registry.create_plugin(preferred) {
                Ok(p) => {
                    info!(
                        target: "coldvox::stt",
                        plugin_id = %preferred,
                        event = "plugin_selected",
                        "Using preferred STT plugin"
                    );
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics.stt_load_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(p)
                }
                Err(e) => {
                    warn!(
                        target: "coldvox::stt",
                        preferred_plugin = %preferred,
                        error = %e,
                        available_plugins = ?available.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                        "Preferred plugin not available, falling back to best available"
                    );
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics.stt_load_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    // Fall back to best available
                    match self.create_fallback_plugin(&registry) {
                        Ok(p) => {
                            if let Some(ref metrics) = self.metrics_sink {
                                metrics.stt_load_count.fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(p)
                        }
                        Err(e2) => {
                            if let Some(ref metrics) = self.metrics_sink {
                                metrics.stt_load_errors.fetch_add(1, Ordering::Relaxed);
                            }
                            error!(
                                target: "coldvox::stt",
                                preferred_plugin = %preferred,
                                preferred_error = %e,
                                fallback_error = %e2,
                                available_plugins = ?available.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                                "All STT plugin attempts failed"
                            );
                            Err(e2)
                        }
                    }
                }
            }
        } else {
            // Use best available
            match self.create_fallback_plugin(&registry) {
                Ok(p) => {
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics.stt_load_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(p)
                }
                Err(e) => {
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics.stt_load_errors.fetch_add(1, Ordering::Relaxed);
                    }
                    error!(
                        target: "coldvox::stt",
                        error = %e,
                        available_plugins = ?available.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                        "No STT plugin could be created"
                    );
                    Err(e)
                }
            }
        };

        let plugin = match plugin_result {
            Ok(p) => {
                if let Some(ref metrics) = self.metrics_sink {
                    metrics.stt_init_success.fetch_add(1, Ordering::Relaxed);
                    metrics
                        .stt_last_init_duration_ms
                        .store(init_start.elapsed().as_millis() as u64, Ordering::Relaxed);
                    metrics.stt_active_plugins.store(1, Ordering::Relaxed);
                }
                p
            }
            Err(e) => {
                if let Some(ref metrics) = self.metrics_sink {
                    metrics.stt_init_failures.fetch_add(1, Ordering::Relaxed);
                }
                return Err(e);
            }
        };

        let mut plugin = plugin;
        // Initialize the plugin with a default config. The processor can re-initialize with specific settings if needed.
        let init_result = plugin.initialize(TranscriptionConfig::default()).await;
        let plugin_id = plugin.info().id.clone();

        match init_result {
            Ok(()) => {
                tracing::info!(
                    target: "coldvox::stt",
                    plugin_id = %plugin_id,
                    init_duration_ms = init_start.elapsed().as_millis(),
                    "STT plugin initialized successfully"
                );
            }
            Err(e) => {
                tracing::error!(
                    target: "coldvox::stt",
                    plugin_id = %plugin_id,
                    init_duration_ms = init_start.elapsed().as_millis(),
                    error = %e,
                    plugin_name = %plugin.info().name,
                    plugin_description = %plugin.info().description,
                    "STT plugin initialization failed"
                );
                if let Some(ref metrics) = self.metrics_sink {
                    metrics.stt_init_failures.fetch_add(1, Ordering::Relaxed);
                }
                return Err(e);
            }
        }

        // Store the selected plugin
        let mut current = self.current_plugin.write().await;
        *current = Some(plugin);

        // Record initial activity to avoid immediate GC
        {
            let mut activity = self.last_activity.write().await;
            activity.insert(plugin_id.clone(), Instant::now());
        }

        tracing::info!(target: "coldvox::stt", selected_plugin = %plugin_id, "STT initialized with plugin");

        Ok(plugin_id)
    }

    /// Create a fallback plugin when preferred isn't available
    fn create_fallback_plugin(
        &self,
        registry: &SttPluginRegistry,
    ) -> Result<Box<dyn SttPlugin>, ColdVoxError> {
        // Try fallback plugins in order
        for fallback_id in &self.selection_config.fallback_plugins {
            match registry.create_plugin(fallback_id) {
                Ok(p) => {
                    info!(
                        target: "coldvox::stt",
                        plugin_id = %fallback_id,
                        event = "plugin_fallback",
                        "Using fallback STT plugin"
                    );
                    return Ok(p);
                }
                Err(e) => {
                    warn!(target: "coldvox::stt", fallback_id = %fallback_id, error = %e, "Fallback plugin unavailable, trying next");
                }
            }
        }

        // Last resort: try any available plugin, but never auto-select NoOp
        match registry.create_best_available() {
            Ok(p) => {
                let id = p.info().id.clone();
                if id == "noop" {
                    return Err(ConfigError::Validation {
                        field: "stt".to_string(),
                        reason: "No real STT plugin available (NoOp is not allowed as fallback)"
                            .to_string(),
                    }
                    .into());
                }
                info!(
                    target: "coldvox::stt",
                    plugin_id = %id,
                    event = "plugin_auto_selected",
                    "Using best available STT plugin"
                );
                Ok(p)
            }
            Err(_) => {
                // No fallback to NoOp; require real STT
                Err(ConfigError::Validation{
                    field: "stt".to_string(),
                    reason: "No STT plugin available. Ensure required models and libraries are installed.".to_string(),
                }
                .into())
            }
        }
    }

    /// Get the current plugin
    pub async fn current_plugin(&self) -> Option<String> {
        let current = self.current_plugin.read().await;
        current.as_ref().map(|p| p.info().id.clone())
    }

    /// Switch to a different plugin
    pub async fn switch_plugin(&mut self, plugin_id: &str) -> Result<(), ColdVoxError> {
        let registry = self.registry.read().await;
        let new_plugin = registry.create_plugin(plugin_id)?;

        info!(
            target: "coldvox::stt",
            plugin_id = %plugin_id,
            event = "plugin_switch",
            "Switching to STT plugin"
        );

        let mut current = self.current_plugin.write().await;

        // Unload the current plugin before switching
        if let Some(ref mut old_plugin) = *current {
            let old_id = old_plugin.info().id.clone();
            let unload_start = Instant::now();
            match old_plugin.unload().await {
                Ok(()) => {
                    debug!(
                        target: "coldvox::stt",
                        plugin_id = %old_id,
                        event = "plugin_unload",
                        "Successfully unloaded previous plugin"
                    );

                    if let Some(ref metrics) = self.metrics_sink {
                        metrics
                            .stt_last_unload_duration_ms
                            .store(unload_start.elapsed().as_millis() as u64, Ordering::Relaxed);
                        metrics
                            .stt_unload_count
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        metrics.stt_active_plugins.store(0, Ordering::Relaxed);
                    }
                }
                Err(ColdVoxError::Plugin(PluginError::Lifecycle { .. })) => {
                    debug!(
                        target: "coldvox::stt",
                        plugin_id = %old_id,
                        event = "plugin_already_unloaded",
                        "Previous plugin was already unloaded"
                    );
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics.stt_active_plugins.store(0, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to unload previous plugin {} during switch: {:?}",
                        old_id, e
                    );

                    // Update error metrics if available
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics
                            .stt_unload_errors
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    // Continue with switch even if unload fails
                }
            }
        }

        *current = Some(new_plugin);

        // Update activity tracking
        {
            let mut activity = self.last_activity.write().await;
            activity.insert(plugin_id.to_string(), Instant::now());
        }

        Ok(())
    }

    /// Unload a specific plugin by ID
    pub async fn unload_plugin(&self, plugin_id: &str) -> Result<(), ColdVoxError> {
        let mut current = self.current_plugin.write().await;
        let mut last_unloaded = self.last_unloaded_plugin_id.write().await;

        if let Some(ref mut plugin) = *current {
            if plugin.info().id == plugin_id {
                match plugin.unload().await {
                    Ok(()) => {
                        debug!(
                            target: "coldvox::stt",
                            plugin_id = %plugin_id,
                            event = "plugin_unload",
                            "Successfully unloaded plugin"
                        );
                        *last_unloaded = Some(plugin_id.to_string());
                        *current = None;

                        // Update metrics if available
                        if let Some(ref metrics) = self.metrics_sink {
                            metrics
                                .stt_unload_count
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }

                        Ok(())
                    }
                    Err(ColdVoxError::Plugin(PluginError::Lifecycle { .. })) => {
                        debug!(
                            target: "coldvox::stt",
                            plugin_id = %plugin_id,
                            event = "plugin_already_unloaded",
                            "Plugin was already unloaded"
                        );
                        *last_unloaded = Some(plugin_id.to_string());
                        *current = None;
                        Ok(())
                    }
                    Err(e) => {
                        warn!("Failed to unload plugin {}: {:?}", plugin_id, e);

                        // Update error metrics if available
                        if let Some(ref metrics) = self.metrics_sink {
                            metrics
                                .stt_unload_errors
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }

                        Err(e)
                    }
                }
            } else {
                Err(SttError::NotAvailable {
                    plugin: plugin_id.to_string(),
                    reason: format!("Plugin '{}' is not currently loaded", plugin_id),
                }
                .into())
            }
        } else {
            // Check if this is an idempotent unload of the last unloaded plugin
            if let Some(ref last_id) = *last_unloaded {
                if last_id == plugin_id {
                    debug!(
                        target: "coldvox::stt",
                        plugin_id = %plugin_id,
                        event = "plugin_unload_idempotent",
                        "Idempotent unload of previously unloaded plugin"
                    );
                    Ok(())
                } else {
                    Err(SttError::NotAvailable {
                        plugin: plugin_id.to_string(),
                        reason: "No plugin is currently loaded".to_string(),
                    }
                    .into())
                }
            } else {
                Err(SttError::NotAvailable {
                    plugin: plugin_id.to_string(),
                    reason: "No plugin is currently loaded".to_string(),
                }
                .into())
            }
        }
    }

    /// Unload all plugins (for shutdown cleanup)
    pub async fn unload_all_plugins(&self) -> Result<(), ColdVoxError> {
        let mut current = self.current_plugin.write().await;

        if let Some(ref mut plugin) = *current {
            let plugin_id = plugin.info().id.clone();
            match plugin.unload().await {
                Ok(()) => {
                    debug!(
                        target: "coldvox::stt",
                        plugin_id = %plugin_id,
                        event = "unload_all_success",
                        "Successfully unloaded all plugins"
                    );
                    *current = None;

                    // Update metrics if available
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics
                            .stt_unload_count
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    Ok(())
                }
                Err(ColdVoxError::Plugin(PluginError::Lifecycle { .. })) => {
                    info!(
                        target: "coldvox::stt",
                        plugin_id = %plugin_id,
                        event = "plugin_already_unloaded",
                        "Plugin was already unloaded"
                    );
                    *current = None;
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to unload plugin {}: {:?}", plugin_id, e);

                    // Update error metrics if available
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics
                            .stt_unload_errors
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    Err(e)
                }
            }
        } else {
            // No plugin loaded, consider this success
            debug!("No plugins to unload");
            Ok(())
        }
    }

    /// Get information about all available plugins (synchronous for UI)
    pub fn list_plugins_sync(&self) -> Vec<coldvox_stt::plugin::PluginInfo> {
        let registry = self.registry.try_read();
        if let Ok(registry_guard) = registry {
            registry_guard.available_plugins()
        } else {
            Vec::new()
        }
    }

    /// Process audio with the current plugin, handling failover on errors
    pub async fn process_audio(
        &mut self,
        samples: &[i16],
    ) -> Result<Option<coldvox_stt::types::TranscriptionEvent>, String> {
        if let Some(ref metrics) = self.metrics_sink {
            metrics
                .stt_transcription_requests
                .fetch_add(1, Ordering::Relaxed);
        }

        let mut current = self.current_plugin.write().await;

        if let Some(ref mut plugin) = *current {
            tracing::trace!(target: "stt_debug", plugin_id = %plugin.info().id, sample_count = samples.len(), "plugin_manager.process_audio() called");
            let plugin_id = plugin.info().id.clone();

            // Update last activity for GC
            {
                let mut activity = self.last_activity.write().await;
                activity.insert(plugin_id.clone(), Instant::now());
            }

            match plugin.process_audio(samples).await {
                Ok(result) => {
                    tracing::trace!(target: "stt_debug", plugin_id = %plugin_id, has_event = %result.is_some(), "plugin_manager.process_audio() ok");
                    // Reset error count on success
                    {
                        let mut errors = self.consecutive_errors.write().await;
                        errors.remove(&plugin_id);
                    }
                    // Update transcription success metrics
                    if let Some(ref metrics) = self.metrics_sink {
                        metrics
                            .stt_transcription_success
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(result)
                }
                Err(e) => {
                    tracing::warn!(target: "stt_debug", plugin_id = %plugin_id, error = %e, "plugin_manager.process_audio() error");
                    // Track error and potentially trigger failover
                    self.total_errors.fetch_add(1, Ordering::Relaxed);
                    if let Some(ref sink) = self.metrics_sink {
                        sink.stt_total_errors.fetch_add(1, Ordering::Relaxed);
                        sink.stt_transcription_failures
                            .fetch_add(1, Ordering::Relaxed);
                    }

                    let (should_failover, errors_consecutive) = {
                        let mut errors = self.consecutive_errors.write().await;
                        let current_errors = errors.entry(plugin_id.clone()).or_insert(0);
                        *current_errors += 1;

                        let threshold = self
                            .selection_config
                            .failover
                            .as_ref()
                            .map_or(3, |f| f.failover_threshold);

                        (*current_errors >= threshold, *current_errors)
                    };

                    if should_failover {
                        warn!(
                            target: "coldvox::stt",
                            plugin_id = %plugin_id,
                            event = "failover_attempt",
                            errors_consecutive = errors_consecutive,
                            "Plugin exceeded error threshold, attempting failover"
                        );
                        drop(current); // Release the lock before attempting failover

                        // Attempt failover
                        match self.attempt_failover(&plugin_id).await {
                            Ok(new_plugin_id) => {
                                info!(
                                    target: "coldvox::stt",
                                    plugin_id = %plugin_id,
                                    event = "failover_success",
                                    new_plugin_id = %new_plugin_id,
                                    "Successfully failed over to new plugin"
                                );
                                self.failover_count
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                {
                                    let mut lf = self.last_failover.write().await;
                                    *lf = Some(Instant::now());
                                }
                                if let Some(ref sink) = self.metrics_sink {
                                    sink.stt_failover_count
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    if let Some(lf_inst) = *self.last_failover.read().await {
                                        let secs =
                                            lf_inst.duration_since(self.start_instant).as_secs();
                                        sink.stt_last_failover_secs
                                            .store(secs, std::sync::atomic::Ordering::Relaxed);
                                    }
                                }

                                // Record cooldown for failed plugin
                                {
                                    let mut cooldown = self.failed_plugins_cooldown.write().await;
                                    cooldown.insert(plugin_id, Instant::now());
                                }

                                // Try processing with new plugin
                                let mut current = self.current_plugin.write().await;
                                if let Some(ref mut new_plugin) = *current {
                                    tracing::debug!(target: "stt_debug", plugin_id = %new_plugin.info().id, "plugin_manager.process_audio() retry on new plugin");
                                    new_plugin
                                        .process_audio(samples)
                                        .await
                                        .map_err(|e| e.to_string())
                                } else {
                                    Err("Failover succeeded but no plugin available".to_string())
                                }
                            }
                            Err(failover_err) => {
                                error!("Failover failed: {}", failover_err);
                                Err(format!(
                                    "STT processing failed: {}, failover failed: {}",
                                    e, failover_err
                                ))
                            }
                        }
                    } else {
                        Err(e.to_string())
                    }
                }
            }
        } else {
            Err("No STT plugin selected".to_string())
        }
    }

    /// Finalize current utterance with the current plugin
    pub async fn finalize(
        &mut self,
    ) -> Result<Option<coldvox_stt::types::TranscriptionEvent>, String> {
        let mut current = self.current_plugin.write().await;
        if let Some(ref mut plugin) = *current {
            tracing::debug!(target: "stt_debug", plugin_id = %plugin.info().id, "plugin_manager.finalize() called");
            match plugin.finalize().await {
                Ok(result) => Ok(result),
                Err(e) => {
                    self.total_errors
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(e.to_string())
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Prepares the plugin for a new utterance, functionally equivalent to reset.
    pub async fn begin_utterance(&mut self) -> Result<(), String> {
        self.reset().await
    }

    /// Cancels the current utterance, functionally equivalent to reset.
    pub async fn cancel_utterance(&mut self) -> Result<(), String> {
        self.reset().await
    }

    /// Reset current plugin state for a new utterance
    pub async fn reset(&mut self) -> Result<(), String> {
        let mut current = self.current_plugin.write().await;
        if let Some(ref mut plugin) = *current {
            plugin.reset().await.map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    /// Apply a TranscriptionConfig to the currently loaded plugin.
    /// This allows the app/processor to override defaults (e.g., enable=true).
    pub async fn apply_transcription_config(
        &mut self,
        config: coldvox_stt::TranscriptionConfig,
    ) -> Result<(), String> {
        let mut current = self.current_plugin.write().await;
        if let Some(ref mut plugin) = *current {
            plugin.initialize(config).await.map_err(|e| e.to_string())
        } else {
            Err("No STT plugin selected".to_string())
        }
    }

    /// Attempt to failover to a different plugin
    async fn attempt_failover(&mut self, failed_plugin_id: &str) -> Result<String, String> {
        let registry = self.registry.read().await;
        let now = Instant::now();

        // Get cooldown period
        let cooldown_secs = self
            .selection_config
            .failover
            .as_ref()
            .map_or(30, |f| f.failover_cooldown_secs);
        let cooldown_duration = std::time::Duration::from_secs(cooldown_secs as u64);

        // Check cooldown for failed plugins
        let cooldown = self.failed_plugins_cooldown.read().await;

        // Try fallback plugins in order, skipping ones in cooldown
        for fallback_id in &self.selection_config.fallback_plugins {
            if fallback_id == failed_plugin_id {
                continue; // Skip the failed plugin
            }

            // Check if plugin is in cooldown
            if let Some(last_failure) = cooldown.get(fallback_id) {
                if now.duration_since(*last_failure) < cooldown_duration {
                    debug!("Plugin {} still in cooldown, skipping", fallback_id);
                    continue;
                }
            }

            match registry.create_plugin(fallback_id) {
                Ok(new_plugin) => {
                    let new_plugin_id = new_plugin.info().id.clone();

                    // Replace current plugin
                    {
                        let mut current = self.current_plugin.write().await;
                        *current = Some(new_plugin);
                    }

                    // Reset error count for new plugin
                    {
                        let mut errors = self.consecutive_errors.write().await;
                        errors.remove(&new_plugin_id);
                    }

                    return Ok(new_plugin_id);
                }
                Err(e) => {
                    debug!("Fallback plugin {} not available: {}", fallback_id, e);
                }
            }
        }

        // No last-resort fallback: fail explicitly so callers/tests never silently use NoOp
        Err("No available STT plugins for failover".to_string())
    }

    /// Get current failover metrics
    pub fn get_metrics(&self) -> (u64, u64) {
        let failover_count = self
            .failover_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let total_errors = self.total_errors.load(std::sync::atomic::Ordering::Relaxed);
        (failover_count, total_errors)
    }

    /// Get Instant of last failover (if any)
    pub async fn last_failover_instant(&self) -> Option<Instant> {
        *self.last_failover.read().await
    }
}

impl Drop for SttPluginManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.gc_task.try_write() {
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }
        if let Ok(mut guard) = self.metrics_task.try_write() {
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldvox_stt::plugin::{FailoverConfig, GcPolicy};
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    use coldvox_stt::TranscriptionEvent;
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    use coldvox_vad::constants::FRAME_SIZE_SAMPLES;
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static TEST_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn create_test_config_path() -> PathBuf {
        let unique = TEST_CONFIG_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "coldvox-plugin-manager-test-{}-{}.json",
            std::process::id(),
            unique
        ))
    }

    fn create_isolated_manager() -> SttPluginManager {
        SttPluginManager::new_with_config_path(create_test_config_path())
    }

    /// Create a test manager - uses Mock plugin for tests to avoid model dependencies
    fn create_test_manager() -> SttPluginManager {
        let mut manager = create_isolated_manager();
        // For tests, prefer Mock plugin to avoid model dependencies
        manager.selection_config.preferred_plugin = Some("mock".to_string());
        // Do not include NoOp in fallbacks for stricter behavior
        manager.selection_config.fallback_plugins = vec![];
        manager
    }

    #[test]
    fn test_new_uses_canonical_plugin_config_path() {
        let manager = SttPluginManager::new();
        let path = manager.config_path();
        let normalized = path.to_string_lossy().replace('\\', "/");

        assert!(normalized.ends_with("config/plugins.json"));
        assert!(!normalized.ends_with("crates/app/config/plugins.json"));
    }

    #[tokio::test]
    async fn test_unload_plugin() {
        let mut manager = create_test_manager();

        // Initialize with a plugin
        let plugin_id = manager.initialize().await.unwrap();

        // Verify plugin is loaded
        let current = manager.current_plugin().await;
        assert!(current.is_some());
        assert_eq!(current, Some(plugin_id.clone()));

        // Unload the plugin
        let result = manager.unload_plugin(&plugin_id).await;
        assert!(result.is_ok());

        // Verify plugin is unloaded
        let current_after = manager.current_plugin().await;
        assert!(current_after.is_none());
    }

    #[tokio::test]
    async fn test_unload_all_plugins() {
        let mut manager = create_test_manager();

        // Initialize with a plugin
        let _plugin_id = manager.initialize().await.unwrap();

        // Verify plugin is loaded
        let current = manager.current_plugin().await;
        assert!(current.is_some());

        // Unload all plugins
        let result = manager.unload_all_plugins().await;
        assert!(result.is_ok());

        // Verify no plugin is loaded
        let current_after = manager.current_plugin().await;
        assert!(current_after.is_none());
    }

    #[tokio::test]
    async fn test_unload_nonexistent_plugin() {
        let manager = create_isolated_manager();

        // Try to unload a plugin that doesn't exist
        let result = manager.unload_plugin("nonexistent").await;
        assert!(result.is_err());

        // Verify error type
        match result {
            Err(ColdVoxError::Stt(SttError::NotAvailable { .. })) => {
                // Expected
            }
            _ => panic!("Expected NotAvailable error"),
        }
    }

    #[tokio::test]
    async fn test_plugin_manager_initialization() {
        let mut manager = create_test_manager();

        // Should initialize with some plugin
        let plugin_id = manager.initialize().await.unwrap();
        assert!(!plugin_id.is_empty());

        // Should be able to list plugins
        let plugins = manager.list_plugins_sync();
        assert!(!plugins.is_empty());

        // Test builds should expose Mock, and runtime builds expose canonical http-remote when enabled.
        let plugin_ids: Vec<String> = plugins.iter().map(|p| p.id.clone()).collect();
        assert!(plugin_ids.contains(&"mock".to_string()));
        #[cfg(feature = "http-remote")]
        {
            assert!(plugin_ids.contains(&"http-remote".to_string()));
            assert!(plugin_ids.contains(&"http-remote-parakeet-gpu".to_string()));
        }
    }

    #[tokio::test]
    async fn test_plugin_switching() {
        let mut manager = create_test_manager();
        manager.initialize().await.unwrap();

        // Should be able to switch to mock plugin
        manager.switch_plugin("mock").await.unwrap();
        assert_eq!(manager.current_plugin().await, Some("mock".to_string()));
    }

    #[tokio::test]
    async fn test_invalid_preferred_plugin_falls_back_to_best_available_plugin() {
        let mut manager = create_isolated_manager();

        // Configure to prefer a non-existent plugin and no fallbacks
        manager.selection_config.preferred_plugin = Some("non-existent".to_string());
        manager.selection_config.fallback_plugins = vec![];

        // Initialization should still succeed via best-available plugin selection.
        let plugin_id = manager.initialize().await.unwrap();
        assert!(!plugin_id.is_empty());
    }

    #[tokio::test]
    async fn test_preferred_plugin_unavailable_falls_back_to_best_available_plugin_for_noncanonical_profiles(
    ) {
        let mut manager = create_isolated_manager();
        manager.selection_config.preferred_plugin = Some("non-existent".to_string());
        manager.selection_config.fallback_plugins = vec!["mock".to_string()];

        let plugin_id = manager.initialize().await.unwrap();

        assert_eq!(plugin_id, "mock");
        assert_eq!(manager.current_plugin().await, Some("mock".to_string()));
    }

    #[tokio::test]
    async fn test_http_remote_rejects_mock_fallback_policy() {
        let mut manager = create_isolated_manager();

        let err = manager
            .set_selection_config(PluginSelectionConfig {
                preferred_plugin: Some("http-remote".to_string()),
                fallback_plugins: vec!["mock".to_string()],
                require_local: false,
                max_memory_mb: None,
                required_language: Some("en".to_string()),
                failover: None,
                gc_policy: None,
                metrics: None,
                auto_extract_model: true,
            })
            .await
            .expect_err("canonical http-remote profile must reject mock fallback");

        assert!(err
            .to_string()
            .contains("mock is test-only and cannot be a production fallback"));
    }

    #[tokio::test]
    async fn test_runtime_selection_config_does_not_rewrite_persisted_config() {
        let config_path = create_test_config_path();
        let persisted_config = r#"{
  "preferred_plugin": null,
  "fallback_plugins": [],
  "require_local": false,
  "max_memory_mb": null,
  "required_language": "en",
  "failover": null,
  "gc_policy": null,
  "metrics": null,
  "auto_extract_model": true
}"#;
        std::fs::write(&config_path, persisted_config).expect("write persisted plugin config");

        let mut manager = SttPluginManager::new_with_config_path(config_path.clone());

        manager
            .set_runtime_selection_config(PluginSelectionConfig {
                preferred_plugin: Some("mock".to_string()),
                fallback_plugins: vec![],
                require_local: false,
                max_memory_mb: None,
                required_language: Some("en".to_string()),
                failover: None,
                gc_policy: None,
                metrics: None,
                auto_extract_model: true,
            })
            .await
            .expect("apply runtime-only plugin selection");

        assert_eq!(manager.selection_config.preferred_plugin.as_deref(), Some("mock"));
        let persisted_after =
            std::fs::read_to_string(&config_path).expect("read persisted plugin config");
        assert_eq!(persisted_after, persisted_config);
    }

    #[cfg(feature = "http-remote")]
    #[tokio::test]
    async fn test_http_remote_preferred_plugin_initializes_canonical_profile() {
        let mut manager = create_isolated_manager();

        manager
            .set_selection_config(PluginSelectionConfig {
                preferred_plugin: Some("http-remote".to_string()),
                fallback_plugins: vec![],
                require_local: false,
                max_memory_mb: None,
                required_language: Some("en".to_string()),
                failover: None,
                gc_policy: None,
                metrics: None,
                auto_extract_model: true,
            })
            .await
            .unwrap();

        let plugin_id = manager
            .initialize()
            .await
            .expect("initialize canonical http-remote");
        assert_eq!(plugin_id, "http-remote");
        assert_eq!(
            manager.current_plugin().await.as_deref(),
            Some("http-remote")
        );
    }

    #[cfg(feature = "http-remote")]
    #[tokio::test]
    async fn test_http_remote_preferred_plugin_initializes_gpu_profile() {
        let mut manager = create_isolated_manager();

        manager
            .set_selection_config(PluginSelectionConfig {
                preferred_plugin: Some("http-remote-parakeet-gpu".to_string()),
                fallback_plugins: vec![],
                require_local: false,
                max_memory_mb: None,
                required_language: Some("en".to_string()),
                failover: None,
                gc_policy: None,
                metrics: None,
                auto_extract_model: true,
            })
            .await
            .unwrap();

        let plugin_id = manager
            .initialize()
            .await
            .expect("initialize gpu http-remote profile");
        assert_eq!(plugin_id, "http-remote-parakeet-gpu");
        assert_eq!(
            manager.current_plugin().await.as_deref(),
            Some("http-remote-parakeet-gpu")
        );
    }

    #[tokio::test]
    async fn test_unload_metrics() {
        let metrics = Arc::new(PipelineMetrics::default());
        let mut manager = create_test_manager().with_metrics_sink(metrics.clone());

        // Initialize with a plugin
        let plugin_id = manager.initialize().await.unwrap();

        // Verify initial metrics
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );

        // Unload the plugin
        let result = manager.unload_plugin(&plugin_id).await;
        assert!(result.is_ok());

        // Verify metrics were updated
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn test_unload_error_metrics() {
        let metrics = Arc::new(PipelineMetrics::default());
        let mut manager = create_test_manager().with_metrics_sink(metrics.clone());

        // Initialize with a plugin
        let _plugin_id = manager.initialize().await.unwrap();

        // Verify initial metrics
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );

        // Try to unload a plugin that doesn't exist
        let result = manager.unload_plugin("nonexistent").await;
        assert!(result.is_err());

        // Verify error metrics were updated
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        ); // No error metric for this case
    }

    #[tokio::test]
    async fn test_switch_plugin_unload_metrics() {
        let metrics = Arc::new(PipelineMetrics::default());
        let mut manager = create_test_manager().with_metrics_sink(metrics.clone());

        // Initialize with a plugin
        let _plugin_id = manager.initialize().await.unwrap();

        // Verify initial metrics
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );

        // Switch to mock plugin
        let result = manager.switch_plugin("mock").await;
        assert!(result.is_ok());

        // Verify metrics were updated
        assert_eq!(
            metrics
                .stt_unload_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
        assert_eq!(
            metrics
                .stt_unload_errors
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn test_unload_idempotency() {
        let mut manager = create_test_manager();

        // Initialize with a plugin
        let plugin_id = manager.initialize().await.unwrap();

        // Verify plugin is loaded
        let current = manager.current_plugin().await;
        assert!(current.is_some());
        assert_eq!(current, Some(plugin_id.clone()));

        // Unload the plugin
        let result = manager.unload_plugin(&plugin_id).await;
        assert!(result.is_ok());

        // Verify plugin is unloaded
        let current_after = manager.current_plugin().await;
        assert!(current_after.is_none());

        // Try to unload again (should succeed with AlreadyUnloaded handled)
        let result2 = manager.unload_plugin(&plugin_id).await;
        assert!(result2.is_ok());

        // Verify plugin is still unloaded
        let current_final = manager.current_plugin().await;
        assert!(current_final.is_none());
    }

    #[tokio::test]
    async fn test_unload_all_idempotency() {
        let mut manager = create_test_manager();

        // Initialize with a plugin
        let _plugin_id = manager.initialize().await.unwrap();

        // Verify plugin is loaded
        let current = manager.current_plugin().await;
        assert!(current.is_some());

        // Unload all plugins
        let result = manager.unload_all_plugins().await;
        assert!(result.is_ok());

        // Verify no plugin is loaded
        let current_after = manager.current_plugin().await;
        assert!(current_after.is_none());

        // Try to unload all again (should succeed)
        let result2 = manager.unload_all_plugins().await;
        assert!(result2.is_ok());

        // Verify no plugin is still loaded
        let current_final = manager.current_plugin().await;
        assert!(current_final.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_process_audio_and_gc_no_double_borrow() {
        let test_manager = create_test_manager();
        let manager = Arc::new(tokio::sync::RwLock::new(test_manager));

        // Initialize with a plugin
        {
            let mut mgr = manager.write().await;
            let _plugin_id = mgr.initialize().await.unwrap();
        }

        // Enable GC with short TTL for testing
        {
            let mut mgr = manager.write().await;
            mgr.set_selection_config(PluginSelectionConfig {
                preferred_plugin: Some("noop".to_string()),
                fallback_plugins: vec!["mock".to_string()],
                require_local: true,
                max_memory_mb: None,
                required_language: None,
                failover: Some(FailoverConfig {
                    failover_threshold: 3,
                    failover_cooldown_secs: 1,
                }),
                gc_policy: Some(GcPolicy {
                    model_ttl_secs: 1, // Very short TTL for testing
                    enabled: true,
                }),
                metrics: None,
                auto_extract_model: false,
            })
            .await
            .unwrap();
        }

        // Create some test audio data
        let test_audio = vec![0i16; 16000]; // 1 second of audio at 16kHz

        // Spawn multiple concurrent tasks that call process_audio
        let mut process_tasks = Vec::new();
        for _i in 0..5 {
            let manager_clone = manager.clone();
            let audio_clone = test_audio.clone();
            let task = tokio::spawn(async move {
                for _ in 0..10 {
                    let mut mgr = manager_clone.write().await;
                    let _result = mgr.process_audio(&audio_clone).await;
                    // Small delay to allow GC to run
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            });
            process_tasks.push(task);
        }

        // Spawn GC tasks that run concurrently
        let mut gc_tasks = Vec::new();
        for _ in 0..3 {
            let manager_clone = manager.clone();
            let task = tokio::spawn(async move {
                for _ in 0..5 {
                    let mgr = manager_clone.read().await;
                    mgr.gc_inactive_models().await;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            });
            gc_tasks.push(task);
        }

        // Wait for all tasks to complete without panicking
        for task in process_tasks {
            let _ = task.await;
        }
        for task in gc_tasks {
            let _ = task.await;
        }

        // Verify the manager is still in a valid state
        let current = manager.read().await.current_plugin().await;
        // Plugin should still be available (GC shouldn't have unloaded it due to recent activity)
        assert!(current.is_some());
    }
}
