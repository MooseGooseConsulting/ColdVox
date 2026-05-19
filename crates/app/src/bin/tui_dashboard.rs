// Logging behavior:
// - Writes logs to both stdout and a daily-rotated file at logs/coldvox.log.
// - Controlled via RUST_LOG (e.g., "info", "debug").
// - File output uses a non-blocking writer; logs/ is created if missing.
// - Useful for post-session analysis even when the TUI is active.
use chrono::Local;
use clap::{builder::BoolishValueParser, Parser, ValueEnum};
use coldvox_app::runtime::{self as app_runtime, ActivationMode};
#[cfg(any(feature = "moonshine", feature = "parakeet"))]
use coldvox_app::stt::TranscriptionEvent;
use coldvox_vad::types::VadEvent;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use hound::{SampleFormat, WavSpec, WavWriter};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    Frame, Terminal,
};
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_logging(cli_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("logs")?;
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "coldvox.log");
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);
    // Prefer CLI-provided level; fall back to RUST_LOG; then default to info for reasonable verbosity
    let effective_level = if !cli_level.is_empty() {
        cli_level.to_string()
    } else {
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
    };
    let env_filter = EnvFilter::try_new(effective_level).unwrap_or_else(|_| EnvFilter::new("info"));

    // Only use file logging for TUI mode to avoid corrupting the display
    let file_layer = fmt::layer()
        .with_writer(non_blocking_file)
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(false)
        .with_level(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();
    std::mem::forget(_guard);
    Ok(())
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "TUI Dashboard with real-time audio monitoring"
)]
struct Cli {
    /// Audio device name
    #[arg(short = 'D', long)]
    device: Option<String>,
    /// Activation mode: vad or hotkey
    #[arg(long = "activation-mode", default_value = "vad", value_enum)]
    activation_mode: CliActivationMode,
    /// Resampler quality: fast, balanced, quality
    #[arg(long = "resampler-quality", default_value = "balanced")]
    resampler_quality: String,
    /// Log level filter (overrides RUST_LOG)
    #[arg(
        long = "log-level",
        // Default to info level to reduce verbosity (use "debug" or "trace" for more detail)
        default_value = "info"
    )]
    log_level: String,

    /// Enable or disable dumping raw audio to disk (defaults to enabled)
    #[arg(
        long = "dump-audio",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = BoolishValueParser::new()
    )]
    dump_audio: Option<bool>,

    /// Directory to save audio dumps (defaults to logs/audio_dumps)
    #[arg(long = "dump-dir")]
    dump_dir: Option<String>,

    /// Dump format: pcm or wav
    #[arg(long = "dump-format", value_enum, default_value = "pcm")]
    dump_format: DumpFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliActivationMode {
    Vad,
    Hotkey,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DumpFormat {
    Pcm,
    Wav,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tab {
    Audio,
    Logs,
    Plugins,
}

#[allow(dead_code)]
enum AppEvent {
    Log(LogLevel, String),
    Vad(VadEvent),
    /// Internal control signal: runtime replaced (after restart)
    AppReplaced(app_runtime::AppHandle),
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    Transcription(TranscriptionEvent),
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    PluginLoad(String),
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    PluginUnload(String),
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    PluginSwitch(String),
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    PluginStatusUpdate,
}

struct PipelineMetricsSnapshot {
    current_rms: u64,
    current_peak: i16,
    audio_level_db: i16,
    capture_fps: u64,
    chunker_fps: u64,
    vad_fps: u64,
    capture_buffer_fill: usize,
    chunker_buffer_fill: usize,
    vad_buffer_fill: usize,
    stage_capture: bool,
    stage_chunker: bool,
    stage_vad: bool,
    stage_output: bool,
    capture_frames: u64,
    chunker_frames: u64,
}

struct DashboardState {
    level_history: VecDeque<u8>,
    peak_history: VecDeque<u8>,
    is_speaking: bool,
    speech_segments: u64,
    last_vad_event: Option<String>,
    is_running: bool,
    selected_device: String,
    start_time: Instant,
    vad_frames: u64,
    logs: VecDeque<LogEntry>,
    app: Option<app_runtime::AppHandle>,
    activation_mode: ActivationMode,
    resampler_quality: coldvox_audio::ResamplerQuality,
    metrics: PipelineMetricsSnapshot,
    has_metrics_snapshot: bool,
    current_tab: Tab,
    /// Last final transcript (if STT enabled)
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    last_transcript: Option<String>,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_manager:
        Option<Arc<tokio::sync::RwLock<coldvox_app::stt::plugin_manager::SttPluginManager>>>,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_current: Option<String>,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_active_count: usize,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_transcription_requests: u64,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_success: u64,

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    plugin_failures: u64,

    // Audio dump options
    dump_audio: bool,
    dump_dir: Option<String>,
    dump_format: DumpFormat,
}

#[derive(Clone)]
struct LogEntry {
    timestamp: Instant,
    level: LogLevel,
    message: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // Allow unused variants for now
enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

impl Default for DashboardState {
    fn default() -> Self {
        let mut level_history = VecDeque::with_capacity(60);
        let mut peak_history = VecDeque::with_capacity(60);
        for _ in 0..60 {
            level_history.push_back(0);
            peak_history.push_back(0);
        }

        let mut logs = VecDeque::new();
        logs.push_back(LogEntry {
            timestamp: Instant::now(),
            level: LogLevel::Info,
            message: "Dashboard started. Press 'S' to start pipeline, 'Q' to quit.".to_string(),
        });

        Self {
            level_history,
            peak_history,
            is_speaking: false,
            speech_segments: 0,
            last_vad_event: None,
            is_running: false,
            selected_device: "default".to_string(),
            start_time: Instant::now(),
            vad_frames: 0,
            logs,
            app: None,
            activation_mode: ActivationMode::Vad,
            resampler_quality: coldvox_audio::ResamplerQuality::Balanced,
            metrics: PipelineMetricsSnapshot {
                current_rms: 0,
                current_peak: 0,
                audio_level_db: -900, // -90.0 dB * 10
                capture_fps: 0,
                chunker_fps: 0,
                vad_fps: 0,
                capture_buffer_fill: 0,
                chunker_buffer_fill: 0,
                vad_buffer_fill: 0,
                stage_capture: false,
                stage_chunker: false,
                stage_vad: false,
                stage_output: false,
                capture_frames: 0,
                chunker_frames: 0,
            },
            has_metrics_snapshot: false,
            current_tab: Tab::Audio,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            last_transcript: None,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_manager: None,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_current: None,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_active_count: 0,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_transcription_requests: 0,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_success: 0,
            #[cfg(any(feature = "moonshine", feature = "parakeet"))]
            plugin_failures: 0,

            dump_audio: true,
            dump_dir: None,
            dump_format: DumpFormat::Pcm,
        }
    }
}

impl DashboardState {
    fn activation_label(mode: ActivationMode) -> &'static str {
        match mode {
            ActivationMode::Vad => "Always-on (VAD)",
            ActivationMode::Hotkey => "Push-to-talk (preview inject)",
            ActivationMode::AlwaysOnPushToTranscribe => "Always-on (push-to-transcribe)",
        }
    }

    fn log(&mut self, level: LogLevel, message: String) {
        self.logs.push_back(LogEntry {
            timestamp: Instant::now(),
            level,
            message,
        });

        while self.logs.len() > 100 {
            self.logs.pop_front();
        }
    }

    fn update_level_history(&mut self) {
        // current_rms is stored as RMS * 1000
        let rms = self.metrics.current_rms as f64 / 1000.0;
        let level = ((rms / 32768.0) * 100.0).min(100.0) as u8;

        let peak = self.metrics.current_peak;
        let peak_level = ((peak as f64 / 32768.0) * 100.0).min(100.0) as u8;

        self.level_history.pop_front();
        self.level_history.push_back(level);

        self.peak_history.pop_front();
        self.peak_history.push_back(peak_level);
    }

    fn reset_metrics(&mut self) {
        self.vad_frames = 0;
        self.speech_segments = 0;
        self.log(LogLevel::Info, "Metrics reset".to_string());
    }

    fn toggle_activation_mode(&mut self) {
        self.activation_mode = match self.activation_mode {
            ActivationMode::Vad => ActivationMode::Hotkey,
            ActivationMode::Hotkey => ActivationMode::AlwaysOnPushToTranscribe,
            ActivationMode::AlwaysOnPushToTranscribe => ActivationMode::Vad,
        };
        self.log(
            LogLevel::Info,
            format!(
                "Switched activation mode to {}",
                Self::activation_label(self.activation_mode)
            ),
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    init_logging(&cli.log_level)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel(100);

    let mut state = DashboardState::default();
    if let Some(device) = cli.device {
        state.selected_device = device;
    }
    // Map CLI activation mode and resampler quality
    state.activation_mode = match cli.activation_mode {
        CliActivationMode::Vad => ActivationMode::Vad,
        CliActivationMode::Hotkey => ActivationMode::Hotkey,
    };
    state.resampler_quality = match cli.resampler_quality.to_lowercase().as_str() {
        "fast" => coldvox_audio::ResamplerQuality::Fast,
        "quality" => coldvox_audio::ResamplerQuality::Quality,
        _ => coldvox_audio::ResamplerQuality::Balanced,
    };

    // Audio dump settings from CLI
    state.dump_audio = cli.dump_audio.unwrap_or(true);
    state.dump_dir = cli.dump_dir;
    state.dump_format = cli.dump_format;

    let res = run_app(&mut terminal, &mut state, tx, rx).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut DashboardState,
    tx: mpsc::Sender<AppEvent>,
    mut rx: mpsc::Receiver<AppEvent>,
) -> io::Result<()> {
    let mut ui_update_interval = tokio::time::interval(Duration::from_millis(50));

    loop {
        terminal.draw(|f| draw_ui(f, state))?;

        tokio::select! {
            Some(event) = async {
                if event::poll(Duration::from_millis(10)).unwrap_or(false) {
                    event::read().ok()
                } else {
                    None
                }
            } => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            if state.is_running {
                                if let Some(app) = state.app.take() {
                                    // Best-effort shutdown
                                    tokio::spawn(async move { Arc::new(app).shutdown().await; });
                                }
                            }
                            return Ok(());
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            if !state.is_running {
                                state.log(LogLevel::Info, "Starting audio pipeline...".to_string());
                                // Build runtime options
                                let settings = coldvox_app::Settings::new()
                                    .map_err(io::Error::other)?;
                                let stt_selection = Some(
                                    settings
                                        .runtime_plugin_selection()
                                        .map_err(io::Error::other)?,
                                );
                                #[cfg(feature = "http-remote")]
                                let http_remote_config = Some(settings.runtime_http_remote_config());
                                let mut opts = app_runtime::AppRuntimeOptions {
                                    device: if state.selected_device == "default" || state.selected_device.is_empty() { None } else { Some(state.selected_device.clone()) },
                                    activation_mode: state.activation_mode,
                                    resampler_quality: state.resampler_quality,
                                    stt_selection,
                                    #[cfg(feature = "http-remote")]
                                    http_remote_config,
                                    enable_device_monitor: false,
                                    capture_buffer_samples: settings.audio.capture_buffer_samples,
                                    ..Default::default()
                                };

                                opts.injection = None;

                                let ui_tx = tx.clone();
                                // Start runtime synchronously and then wire up event forwarders
                                match app_runtime::start(opts).await {
                                    Ok(app) => {
                                        #[allow(unused_mut)]
                                        let mut app = app;
                                        // Extract plugin manager for UI access before moving app
                                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                                        {
                                            state.plugin_manager = app.plugin_manager.clone();
                                        }
                                        // Forward VAD events to UI
                                        let mut vad_rx = app.subscribe_vad();
                                        tokio::spawn(async move {
                                            while let Ok(ev) = vad_rx.recv().await {
                                                let _ = ui_tx.send(AppEvent::Vad(ev)).await;
                                            }
                                        });

                                        // Forward STT events to UI (if enabled)
                                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                                        if let Some(mut stt_rx) = app.stt_rx.take() {
                                            let ui_tx2 = tx.clone();
                                            tokio::spawn(async move {
                                                while let Some(ev) = stt_rx.recv().await {
                                                    let _ = ui_tx2.send(AppEvent::Transcription(ev)).await;
                                                }
                                            });
                                        }

                                        // Optional: dump raw audio to disk if enabled and not disabled by env
                                        let dump_enabled = state.dump_audio;
                                        let dump_dir = state.dump_dir.clone();
                                        let dump_format = state.dump_format;
                                        if dump_enabled {
                                            let env_disable = std::env::var("COLDVOX_DISABLE_AUDIO_DUMP").unwrap_or_default().to_lowercase();
                                            if matches!(env_disable.as_str(), "1" | "true" | "yes") {
                                                let _ = tx.send(AppEvent::Log(LogLevel::Warning, "Audio dump disabled by COLDVOX_DISABLE_AUDIO_DUMP".to_string())).await;
                                            } else {
                                                let mut audio_rx = app.subscribe_audio();
                                                let ui_tx3 = tx.clone();
                                                tokio::spawn(async move {
                                                    // Resolve output directory
                                                    let base_dir = dump_dir.unwrap_or_else(|| "logs/audio_dumps".to_string());
                                                    if let Err(e) = fs::create_dir_all(&base_dir) {
                                                        let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("Failed to create dump dir '{}': {}", base_dir, e))).await;
                                                        return;
                                                    }

                                                    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
                                                    match dump_format {
                                                        DumpFormat::Pcm => {
                                                            let mut path = PathBuf::from(&base_dir);
                                                            path.push(format!("audio_{}.pcm", ts));
                                                            let _ = ui_tx3.send(AppEvent::Log(LogLevel::Info, format!("Audio dump enabled: {}", path.display()))).await;

                                                            let mut writer = match std::fs::File::create(&path).map(std::io::BufWriter::new) {
                                                                Ok(w) => w,
                                                                Err(e) => {
                                                                    let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("Failed to open '{}': {}", path.display(), e))).await;
                                                                    return;
                                                                }
                                                            };

                                                            loop {
                                                                match audio_rx.recv().await {
                                                                    Ok(frame) => {
                                                                        // Write i16 samples as little-endian PCM
                                                                        let mut buf = Vec::with_capacity(frame.samples.len() * 2);
                                                                        for &s in frame.samples.iter() {
                                                                            let b = s.to_le_bytes();
                                                                            buf.push(b[0]);
                                                                            buf.push(b[1]);
                                                                        }
                                                                        if let Err(e) = writer.write_all(&buf) {
                                                                            let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("PCM write error: {}", e))).await;
                                                                            break;
                                                                        }
                                                                    }
                                                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                                                        let _ = ui_tx3.send(AppEvent::Log(LogLevel::Debug, format!("Audio dump lagged; dropped {} frames", n))).await;
                                                                        continue;
                                                                    }
                                                                    Err(_) => {
                                                                        // Channel closed or canceled
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                            let _ = writer.flush();
                                                            let _ = ui_tx3.send(AppEvent::Log(LogLevel::Info, "Audio dump stopped".to_string())).await;
                                                        }
                                                        DumpFormat::Wav => {
                                                            let mut path = PathBuf::from(&base_dir);
                                                            path.push(format!("audio_{}.wav", ts));
                                                            // Grab the first frame to determine sample rate
                                                            let first_frame = match audio_rx.recv().await {
                                                                Ok(f) => f,
                                                                Err(e) => {
                                                                    let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("Failed to start WAV dump: {}", e))).await;
                                                                    return;
                                                                }
                                                            };
                                                            let spec = WavSpec {
                                                                channels: 1,
                                                                sample_rate: first_frame.sample_rate,
                                                                bits_per_sample: 16,
                                                                sample_format: SampleFormat::Int,
                                                            };
                                                            let mut wav = match WavWriter::create(&path, spec) {
                                                                Ok(w) => w,
                                                                Err(e) => {
                                                                    let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("Failed to open '{}': {}", path.display(), e))).await;
                                                                    return;
                                                                }
                                                            };
                                                            let _ = ui_tx3.send(AppEvent::Log(LogLevel::Info, format!("Audio dump enabled: {} ({} Hz)", path.display(), first_frame.sample_rate))).await;
                                                            // Write first frame
                                                            for &s in first_frame.samples.iter() {
                                                                if wav.write_sample(s).is_err() { break; }
                                                            }
                                                            // Remaining frames
                                                            loop {
                                                                match audio_rx.recv().await {
                                                                    Ok(frame) => {
                                                                        for &s in frame.samples.iter() {
                                                                            if let Err(e) = wav.write_sample(s) {
                                                                                let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("WAV write error: {}", e))).await;
                                                                                break;
                                                                            }
                                                                        }
                                                                    }
                                                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                                                        let _ = ui_tx3.send(AppEvent::Log(LogLevel::Debug, format!("Audio dump lagged; dropped {} frames", n))).await;
                                                                        continue;
                                                                    }
                                                                    Err(_) => break,
                                                                }
                                                            }
                                                            let _ = wav.flush();
                                                            if let Err(e) = wav.finalize() {
                                                                let _ = ui_tx3.send(AppEvent::Log(LogLevel::Error, format!("Error finalizing WAV: {}", e))).await;
                                                            } else {
                                                                let _ = ui_tx3.send(AppEvent::Log(LogLevel::Info, "Audio dump stopped".to_string())).await;
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        }

                                        state.app = Some(app);
                                        state.is_running = true;
                                        state.log(LogLevel::Success, "Pipeline fully started".to_string());
                                        state.log(LogLevel::Success, "Pipeline fully started".to_string());
                                    }
                                    Err(e) => {
                                        state.log(LogLevel::Error, format!("Failed to start runtime: {}", e));
                                    }
                                }
                            }
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            // Toggle activation mode; if running, reconfigure runtime without restart
                            state.toggle_activation_mode();
                            if state.is_running {
                                if let Some(app) = &mut state.app {
                                    let new_mode = state.activation_mode;
                                    if let Err(e) = app.set_activation_mode(new_mode).await {
                                        state.log(LogLevel::Error, format!("Failed to set activation mode: {}", e));
                                    } else {
                                        state.log(LogLevel::Info, "Activation mode updated".to_string());
                                    }
                                }
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            state.reset_metrics();
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            // Toggle between tabs
                            state.current_tab = match state.current_tab {
                                Tab::Audio => Tab::Logs,
                                Tab::Logs => Tab::Plugins,
                                Tab::Plugins => Tab::Audio,
                            };
                            state.log(LogLevel::Info, format!("Switched to {:?} tab", state.current_tab));
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            // Load plugin (only when running)
                            if state.is_running {
                                #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                                {
                                    if let Some(ref pm) = state.plugin_manager {
                                        let pm_clone = pm.clone();
                                        let tx_clone = tx.clone();
                                        tokio::spawn(async move {
                                            let result = pm_clone.write().await.switch_plugin("mock").await;
                                            let _ = tx_clone.send(AppEvent::PluginSwitch("mock".to_string())).await;
                                            if result.is_ok() {
                                                let _ = tx_clone.send(AppEvent::PluginLoad("mock".to_string())).await;
                                            }
                                        });
                                    }
                                }
                                state.log(LogLevel::Info, "Loading plugin...".to_string());
                            }
                        }
                        KeyCode::Char('u') | KeyCode::Char('U') => {
                            // Unload plugin (only when running)
                            if state.is_running {
                                #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                                {
                                    if let Some(ref pm) = state.plugin_manager {
                                        let pm_clone = pm.clone();
                                        let tx_clone = tx.clone();
                                        tokio::spawn(async move {
                                            let _result = pm_clone.write().await.unload_plugin("mock").await;
                                            let _ = tx_clone.send(AppEvent::PluginUnload("mock".to_string())).await;
                                        });
                                    }
                                }
                                state.log(LogLevel::Info, "Unloading plugin...".to_string());
                            }
                        }
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            // Switch plugin (only when running and in plugins tab)
                            if state.is_running && matches!(state.current_tab, Tab::Plugins) {
                                #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                                {
                                    if let Some(ref pm) = state.plugin_manager {
                                        let pm_clone = pm.clone();
                                        let tx_clone = tx.clone();
                                        tokio::spawn(async move {
                                            let _result = pm_clone.write().await.switch_plugin("noop").await;
                                            let _ = tx_clone.send(AppEvent::PluginSwitch("noop".to_string())).await;
                                        });
                                    }
                                }
                                state.log(LogLevel::Info, "Switching plugin...".to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }

            Some(event) = rx.recv() => {
                match event {
                    AppEvent::Log(level, msg) => state.log(level, msg),
                    AppEvent::Vad(vad_event) => {
                        state.vad_frames += 1;
                        match vad_event {
                            VadEvent::SpeechStart { timestamp_ms, energy_db } => {
                                state.is_speaking = true;
                                state.speech_segments += 1;
                                state.last_vad_event = Some(format!("Speech START @ {}ms ({:.1}dB)", timestamp_ms, energy_db));
                                state.log(LogLevel::Success, format!("Speech detected @ {}ms", timestamp_ms));
                            }
                            VadEvent::SpeechEnd { timestamp_ms, duration_ms, energy_db } => {
                                state.is_speaking = false;
                                state.last_vad_event = Some(format!("Speech END @ {}ms ({}ms, {:.1}dB)", timestamp_ms, duration_ms, energy_db));
                                state.log(LogLevel::Info, format!("Speech ended, duration: {}ms", duration_ms));
                            }
                        }
                    }
                    AppEvent::AppReplaced(app) => {
                        state.app = Some(app);
                        state.is_running = true;
                    }
                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                        AppEvent::Transcription(tevent) => {
                            match tevent.clone() {
                                TranscriptionEvent::Partial { utterance_id, text, .. } => {
                                    if !text.trim().is_empty() {
                                        state.log(LogLevel::Info, format!("[STT partial:{}] {}", utterance_id, text));
                                    }
                                }
                                TranscriptionEvent::Final { utterance_id, text, .. } => {
                                    if !text.trim().is_empty() {
                                        state.log(LogLevel::Success, format!("[STT final:{}] {}", utterance_id, text));
                                        state.last_transcript = Some(text);
                                    }
                                }
                                TranscriptionEvent::Error { code, message } => {
                                    state.log(LogLevel::Error, format!("[STT error:{}] {}", code, message));
                                }
                            }
                        }
                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                        AppEvent::PluginLoad(plugin_id) => {
                            state.plugin_current = Some(plugin_id.clone());
                            state.plugin_active_count += 1;
                            // Update metrics from shared sink
                            if let Some(app) = &state.app {
                                state.plugin_transcription_requests = app.metrics.stt_transcription_requests.load(Ordering::Relaxed);
                                state.plugin_success = app.metrics.stt_transcription_success.load(Ordering::Relaxed);
                                state.plugin_failures = app.metrics.stt_transcription_failures.load(Ordering::Relaxed);
                            }
                            state.log(LogLevel::Success, format!("Plugin loaded: {}", plugin_id));
                        }
                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                        AppEvent::PluginUnload(plugin_id) => {
                            if state.plugin_current.as_ref() == Some(&plugin_id) {
                                state.plugin_current = None;
                            }
                            if state.plugin_active_count > 0 {
                                state.plugin_active_count -= 1;
                            }
                            // Update metrics from shared sink
                            if let Some(app) = &state.app {
                                state.plugin_transcription_requests = app.metrics.stt_transcription_requests.load(Ordering::Relaxed);
                                state.plugin_success = app.metrics.stt_transcription_success.load(Ordering::Relaxed);
                                state.plugin_failures = app.metrics.stt_transcription_failures.load(Ordering::Relaxed);
                            }
                            state.log(LogLevel::Info, format!("Plugin unloaded: {}", plugin_id));
                        }
                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                        AppEvent::PluginSwitch(plugin_id) => {
                            state.plugin_current = Some(plugin_id.clone());
                            // Update metrics from shared sink
                            if let Some(app) = &state.app {
                                state.plugin_transcription_requests = app.metrics.stt_transcription_requests.load(Ordering::Relaxed);
                                state.plugin_success = app.metrics.stt_transcription_success.load(Ordering::Relaxed);
                                state.plugin_failures = app.metrics.stt_transcription_failures.load(Ordering::Relaxed);
                            }
                            state.log(LogLevel::Info, format!("Switched to plugin: {}", plugin_id));
                        }
                        #[cfg(any(feature = "moonshine", feature = "parakeet"))]
                        AppEvent::PluginStatusUpdate => {
                            // Update plugin status (could refresh metrics)
                            state.log(LogLevel::Debug, "Plugin status updated".to_string());
                        }
                }
            }

            _ = ui_update_interval.tick() => {
                if state.is_running {
                    if let Some(app) = &state.app {
                        let m = &app.metrics;
                        // Take a live snapshot of metrics
                        state.metrics = PipelineMetricsSnapshot {
                            current_rms: m.current_rms.load(Ordering::Relaxed),
                            current_peak: m.current_peak.load(Ordering::Relaxed),
                            audio_level_db: m.audio_level_db.load(Ordering::Relaxed),
                            capture_fps: m.capture_fps.load(Ordering::Relaxed),
                            chunker_fps: m.chunker_fps.load(Ordering::Relaxed),
                            vad_fps: m.vad_fps.load(Ordering::Relaxed),
                            capture_buffer_fill: m.capture_buffer_fill.load(Ordering::Relaxed),
                            chunker_buffer_fill: m.chunker_buffer_fill.load(Ordering::Relaxed),
                            vad_buffer_fill: m.vad_buffer_fill.load(Ordering::Relaxed),
                            stage_capture: m.stage_capture.load(Ordering::Relaxed),
                            stage_chunker: m.stage_chunker.load(Ordering::Relaxed),
                            stage_vad: m.stage_vad.load(Ordering::Relaxed),
                            stage_output: m.stage_output.load(Ordering::Relaxed),
                            capture_frames: m.capture_frames.load(Ordering::Relaxed),
                            chunker_frames: m.chunker_frames.load(Ordering::Relaxed),
                        };
                        state.has_metrics_snapshot = true;
                        state.update_level_history();
                    }
                }
            }
        }
    }
}

fn draw_ui(f: &mut Frame, state: &DashboardState) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12),
            Constraint::Min(10),
            Constraint::Length(8),
        ])
        .split(f.area());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[0]);

    draw_audio_levels(f, top_chunks[0], state);
    draw_pipeline_flow(f, top_chunks[1], state);

    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[1]);

    match state.current_tab {
        Tab::Audio => {
            draw_metrics(f, middle_chunks[0], state);
            draw_status(f, middle_chunks[1], state);
        }
        Tab::Logs => {
            draw_logs(f, middle_chunks[0], state);
            draw_status(f, middle_chunks[1], state);
        }
        Tab::Plugins => {
            draw_plugins(f, middle_chunks[0], state);
            draw_plugin_status(f, middle_chunks[1], state);
        }
    }

    draw_logs(f, main_chunks[2], state);
}

fn draw_audio_levels(f: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default().title("Audio Levels").borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .split(inner);

    let db = state.metrics.audio_level_db as f64 / 10.0;
    let level_percent = ((db + 90.0) / 90.0 * 100.0).clamp(0.0, 100.0) as u16;

    let gauge = Gauge::default()
        .block(Block::default().title("Level"))
        .gauge_style(if level_percent > 80 {
            Style::default().fg(Color::Red)
        } else if level_percent > 60 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Green)
        })
        .percent(level_percent)
        .label(format!("{:.1} dB", db));
    f.render_widget(gauge, chunks[0]);

    let rms_scaled = state.metrics.current_rms as f64 / 1000.0; // stored as RMS*1000
    let rms_db = if rms_scaled > 0.0 {
        20.0 * (rms_scaled / 32767.0).log10()
    } else {
        -90.0
    };
    let peak = state.metrics.current_peak as f64;
    let peak_db = if peak > 0.0 {
        20.0 * (peak / 32767.0).log10()
    } else {
        -90.0
    };

    let db_text = Paragraph::new(format!("Peak: {:.1} dB | RMS: {:.1} dB", peak_db, rms_db))
        .alignment(Alignment::Center);
    f.render_widget(db_text, chunks[1]);

    let sparkline_data: Vec<u64> = state.level_history.iter().map(|&v| v as u64).collect();

    let sparkline = Sparkline::default()
        .block(Block::default().title("History (60 samples)"))
        .data(&sparkline_data)
        .style(Style::default().fg(Color::Cyan))
        .max(100);
    f.render_widget(sparkline, chunks[2]);
}

fn draw_pipeline_flow(f: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default()
        .title("Pipeline Flow")
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(inner);

    let stages = [
        ("1. Capture", state.metrics.stage_capture),
        ("2. Chunker", state.metrics.stage_chunker),
        ("3. VAD", state.metrics.stage_vad),
        ("4. Output", state.metrics.stage_output),
    ];

    for (i, (name, active)) in stages.iter().enumerate() {
        let color = if *active {
            Color::Green
        } else if state.is_running {
            Color::Gray
        } else {
            Color::DarkGray
        };

        let indicator = if *active { "●" } else { "○" };
        let count_text = match i {
            0 => {
                if state.has_metrics_snapshot {
                    format!("{} events", state.metrics.capture_frames)
                } else {
                    "N/A".to_string()
                }
            }
            1 => {
                if state.has_metrics_snapshot {
                    format!("{} events", state.metrics.chunker_frames)
                } else {
                    "N/A".to_string()
                }
            }
            2 => format!("{} events", state.vad_frames),
            3 => format!("{} events", state.speech_segments),
            _ => "".to_string(),
        };
        let text = format!("{} {} [{}]", indicator, name, count_text);

        let paragraph = Paragraph::new(text).style(Style::default().fg(color));
        f.render_widget(paragraph, chunks[i]);
    }
}

fn draw_metrics(f: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default().title("Metrics").borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let elapsed = state.start_time.elapsed().as_secs();
    let metrics_text = vec![
        Line::from(format!("Runtime: {}s", elapsed)),
        Line::from(""),
        Line::from(format!(
            "Capture FPS: {:.1}",
            state.metrics.capture_fps as f64 / 10.0
        )),
        Line::from(format!(
            "Chunker FPS: {:.1}",
            state.metrics.chunker_fps as f64 / 10.0
        )),
        Line::from(format!(
            "VAD FPS: {:.1}",
            state.metrics.vad_fps as f64 / 10.0
        )),
        Line::from(""),
        Line::from("Buffer Fill:"),
        Line::from(format!("  Capture: {}%", state.metrics.capture_buffer_fill)),
        Line::from(format!("  Chunker: {}%", state.metrics.chunker_buffer_fill)),
        Line::from(format!("  VAD: {}%", state.metrics.vad_buffer_fill)),
    ];

    let paragraph = Paragraph::new(metrics_text);
    f.render_widget(paragraph, inner);
}

fn draw_status(f: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default().title("Status & VAD").borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let status_color = if state.is_running {
        if state.is_speaking {
            Color::Yellow
        } else {
            Color::Green
        }
    } else {
        Color::Gray
    };

    let mut status_text: Vec<Line> = Vec::new();
    status_text.push(Line::from(vec![
        Span::raw("Pipeline: "),
        Span::styled(
            if state.is_running {
                "RUNNING"
            } else {
                "STOPPED"
            },
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    status_text.push(Line::from(format!("Device: {}", state.selected_device)));
    status_text.push(Line::from(format!(
        "Activation: {}",
        DashboardState::activation_label(state.activation_mode)
    )));
    status_text.push(Line::from(""));
    status_text.push(Line::from(vec![
        Span::raw("Speaking: "),
        Span::styled(
            if state.is_speaking { "YES" } else { "NO" },
            Style::default().fg(if state.is_speaking {
                Color::Green
            } else {
                Color::Gray
            }),
        ),
    ]));
    status_text.push(Line::from(format!(
        "Speech Segments: {}",
        state.speech_segments
    )));
    status_text.push(Line::from(""));
    status_text.push(Line::from("Last VAD Event:"));
    status_text.push(Line::from(
        state.last_vad_event.as_deref().unwrap_or("None"),
    ));
    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    {
        status_text.push(Line::from(""));
        status_text.push(Line::from("Last Transcript (final):"));
        let txt = state.last_transcript.as_deref().unwrap_or("None");
        let trunc = if txt.len() > 80 {
            format!("{}…", &txt[..80])
        } else {
            txt.to_string()
        };
        status_text.push(Line::from(trunc));
    }
    status_text.push(Line::from(""));
    status_text.push(Line::from("Controls:"));
    status_text.push(Line::from(
        "[S] Start  [A] Toggle VAD/PTT  [R] Reset  [Q] Quit",
    ));

    let paragraph = Paragraph::new(status_text);
    f.render_widget(paragraph, inner);
}

fn draw_logs(f: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default().title("Logs").borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let start_time = state
        .logs
        .front()
        .map(|e| e.timestamp)
        .unwrap_or_else(Instant::now);

    let log_lines: Vec<Line> = state
        .logs
        .iter()
        .rev()
        .take(inner.height as usize)
        .rev()
        .map(|entry| {
            let elapsed = entry.timestamp.duration_since(start_time).as_secs_f64();
            let color = match entry.level {
                LogLevel::Info => Color::White,
                LogLevel::Success => Color::Green,
                LogLevel::Warning => Color::Yellow,
                LogLevel::Error => Color::Red,
                LogLevel::Debug => Color::Cyan,
            };

            Line::from(vec![
                Span::styled(
                    format!("[{:7.2}s] ", elapsed),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(&entry.message, Style::default().fg(color)),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(log_lines);
    f.render_widget(paragraph, inner);
}

fn draw_plugins(f: &mut Frame, area: Rect, #[allow(unused_variables)] state: &DashboardState) {
    let block = Block::default()
        .title("Available Plugins")
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut plugin_lines: Vec<Line> = Vec::new();

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    {
        // Display available plugins from plugin manager
        if let Some(ref pm) = state.plugin_manager {
            // Try to get plugin list without blocking
            if let Ok(pm_guard) = pm.try_read() {
                let plugins = pm_guard.list_plugins_sync();
                for plugin in plugins {
                    let status = if Some(&plugin.id) == state.plugin_current.as_ref() {
                        " [ACTIVE]"
                    } else {
                        ""
                    };
                    plugin_lines.push(Line::from(format!(
                        "{} - {}{}",
                        plugin.id, plugin.name, status
                    )));
                }
            } else {
                plugin_lines.push(Line::from("Loading plugins..."));
            }
        } else {
            plugin_lines.push(Line::from("Plugin manager not available"));
        }
    }

    #[cfg(not(any(feature = "moonshine", feature = "parakeet")))]
    {
        plugin_lines.push(Line::from(
            "STT plugins require 'moonshine' or 'parakeet' feature",
        ));
    }

    let paragraph = Paragraph::new(plugin_lines);
    f.render_widget(paragraph, inner);
}

fn draw_plugin_status(
    f: &mut Frame,
    area: Rect,
    #[allow(unused_variables)] state: &DashboardState,
) {
    let block = Block::default()
        .title("Plugin Status")
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut status_lines: Vec<Line> = Vec::new();

    #[cfg(any(feature = "moonshine", feature = "parakeet"))]
    {
        // Current plugin
        let current = state.plugin_current.as_deref().unwrap_or("None");
        status_lines.push(Line::from(vec![
            Span::raw("Current: "),
            Span::styled(
                current,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Active count
        status_lines.push(Line::from(format!("Active: {}", state.plugin_active_count)));

        // Real-time metrics from shared sink
        if let Some(app) = &state.app {
            let metrics = &app.metrics;
            status_lines.push(Line::from(format!(
                "Requests: {}",
                metrics.stt_transcription_requests.load(Ordering::Relaxed)
            )));
            status_lines.push(Line::from(format!(
                "Success: {}",
                metrics.stt_transcription_success.load(Ordering::Relaxed)
            )));
            status_lines.push(Line::from(format!(
                "Failures: {}",
                metrics.stt_transcription_failures.load(Ordering::Relaxed)
            )));
            status_lines.push(Line::from(format!(
                "Load Count: {}",
                metrics.stt_load_count.load(Ordering::Relaxed)
            )));
            status_lines.push(Line::from(format!(
                "Unload Count: {}",
                metrics.stt_unload_count.load(Ordering::Relaxed)
            )));
            status_lines.push(Line::from(format!(
                "Failovers: {}",
                metrics.stt_failover_count.load(Ordering::Relaxed)
            )));
        } else {
            status_lines.push(Line::from("Requests: N/A"));
            status_lines.push(Line::from("Success: N/A"));
            status_lines.push(Line::from("Failures: N/A"));
        }

        status_lines.push(Line::from(""));
        status_lines.push(Line::from("Controls:"));
        status_lines.push(Line::from("[P] Toggle Tab  [L] Load Plugin"));
        status_lines.push(Line::from("[U] Unload Plugin  [W] Switch"));
    }

    #[cfg(not(any(feature = "moonshine", feature = "parakeet")))]
    {
        status_lines.push(Line::from(
            "STT plugins require 'moonshine' or 'parakeet' feature",
        ));
    }

    let paragraph = Paragraph::new(status_lines);
    f.render_widget(paragraph, inner);
}
