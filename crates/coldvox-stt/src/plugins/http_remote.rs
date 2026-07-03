//! HTTP Remote STT Plugin
//!
//! Sends audio to an OpenAI-compatible `/v1/audio/transcriptions` endpoint.
//! Buffers PCM frames during speech, encodes to WAV on finalize, and POSTs to the service.

use crate::plugin::{PluginCapabilities, PluginInfo, SttPlugin, SttPluginFactory};
use crate::types::{TranscriptionConfig, TranscriptionEvent};
use async_trait::async_trait;
use coldvox_foundation::error::{ColdVoxError, SttError};
use reqwest::{multipart, Client, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::net::IpAddr;
use std::time::Duration;

/// Configuration for the HTTP remote plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRemoteConfig {
    /// Stable profile identifier for explicit selection/failover ordering.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Base URL of the STT service (e.g., "http://localhost:5096")
    pub base_url: String,
    /// API path (e.g., "/v1/audio/transcriptions")
    pub api_path: String,
    /// Health path (e.g., "/health")
    #[serde(default = "default_health_path")]
    pub health_path: String,
    /// Model name to send in the request (e.g., "moonshine/base")
    pub model_name: String,
    /// Display name for logging/UI
    pub display_name: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Sample rate of the audio being sent (typically 16000)
    pub sample_rate: u32,
    /// Optional extra HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Optional bearer-token environment variable name
    #[serde(default)]
    pub bearer_token_env_var: Option<String>,
    /// Maximum encoded WAV bytes allowed before request send
    #[serde(default = "default_max_audio_bytes")]
    pub max_audio_bytes: u64,
    /// Maximum utterance duration in seconds allowed before request send
    #[serde(default = "default_max_audio_seconds")]
    pub max_audio_seconds: u32,
    /// Maximum estimated multipart payload bytes allowed before request send
    #[serde(default = "default_max_payload_bytes")]
    pub max_payload_bytes: u64,
}

fn default_health_path() -> String {
    "/health".to_string()
}

fn default_max_audio_bytes() -> u64 {
    2_097_152
}

fn default_max_audio_seconds() -> u32 {
    30
}

fn default_max_payload_bytes() -> u64 {
    2_621_440
}

impl Default for HttpRemoteConfig {
    fn default() -> Self {
        Self {
            profile_id: Some("moonshine-base".into()),
            base_url: "http://localhost:5096".into(),
            api_path: "/v1/audio/transcriptions".into(),
            health_path: default_health_path(),
            model_name: "moonshine/base".into(),
            display_name: "Moonshine (HTTP)".into(),
            timeout_ms: 10_000,
            sample_rate: 16_000,
            headers: HashMap::new(),
            bearer_token_env_var: None,
            max_audio_bytes: default_max_audio_bytes(),
            max_audio_seconds: default_max_audio_seconds(),
            max_payload_bytes: default_max_payload_bytes(),
        }
    }
}

impl HttpRemoteConfig {
    /// Canonical Parakeet CPU profile for the `http-remote` plugin.
    ///
    /// The canonical deployment of this service is the k8s cluster endpoint
    /// `http://192.168.30.207:5092` (Deployment `parakeet` in namespace `apps`,
    /// coldaine-k8cluster repo), which `config/default.toml` points at via
    /// `[stt.remote] base_url`. The `localhost:5092` value below is a built-in
    /// fallback matching the local compose profile (`ops/parakeet/docker-compose.yml`)
    /// for offline dev; runtime config always overrides it.
    pub fn canonical_parakeet_cpu() -> Self {
        Self {
            profile_id: Some("http-remote".into()),
            base_url: "http://localhost:5092".into(),
            api_path: "/v1/audio/transcriptions".into(),
            health_path: "/health".into(),
            model_name: "parakeet-tdt-0.6b-v2".into(),
            display_name: "Parakeet CPU (HTTP)".into(),
            timeout_ms: 15_000,
            sample_rate: 16_000,
            headers: HashMap::new(),
            bearer_token_env_var: None,
            max_audio_bytes: default_max_audio_bytes(),
            max_audio_seconds: default_max_audio_seconds(),
            max_payload_bytes: default_max_payload_bytes(),
        }
    }
}

/// Response from an OpenAI-compatible STT service.
#[derive(Debug, Deserialize)]
struct SttResponse {
    text: String,
}

const PLUGIN_ID_PREFIX: &str = "http-remote";

pub struct HttpRemotePlugin {
    config: HttpRemoteConfig,
    audio_buffer: Vec<i16>,
    utterance_id: u64,
}

impl Debug for HttpRemotePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRemotePlugin")
            .field("config", &self.config)
            .field("buffer_size", &self.audio_buffer.len())
            .field("utterance_id", &self.utterance_id)
            .finish()
    }
}

fn is_local_url(base_url: &str) -> bool {
    parse_base_url(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .map(|addr| addr.is_loopback())
                    .unwrap_or(false)
        })
}

fn join_url_paths(base_path: &str, api_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    let api = api_path.trim_start_matches('/');
    if base.is_empty() {
        format!("/{api}")
    } else if api.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{api}")
    }
}

fn parse_base_url(base_url: &str) -> Result<Url, ColdVoxError> {
    let url = Url::parse(base_url).map_err(|e| {
        SttError::TranscriptionFailed(format!("Invalid HTTP remote base URL '{base_url}': {e}"))
    })?;

    if url.scheme() != "http" {
        return Err(SttError::TranscriptionFailed(format!(
            "Unsupported HTTP remote base URL '{base_url}': expected http://"
        ))
        .into());
    }

    if url.host_str().is_none() {
        return Err(SttError::TranscriptionFailed(format!(
            "Invalid HTTP remote base URL '{base_url}': missing host"
        ))
        .into());
    }

    if url.query().is_some() || url.fragment().is_some() {
        return Err(SttError::TranscriptionFailed(format!(
            "Invalid HTTP remote base URL '{base_url}': query strings and fragments are not supported"
        ))
        .into());
    }

    Ok(url)
}

fn validate_endpoint_path<'a>(field_name: &str, path: &'a str) -> Result<&'a str, ColdVoxError> {
    if path.is_empty() {
        return Err(SttError::TranscriptionFailed(format!(
            "Invalid HTTP remote {field_name} '{path}': path must not be empty"
        ))
        .into());
    }

    if !path.starts_with('/') {
        return Err(SttError::TranscriptionFailed(format!(
            "Invalid HTTP remote {field_name} '{path}': expected an absolute path starting with '/'"
        ))
        .into());
    }

    if path.contains('?') || path.contains('#') {
        return Err(SttError::TranscriptionFailed(format!(
            "Invalid HTTP remote {field_name} '{path}': query strings and fragments are not supported"
        ))
        .into());
    }

    Ok(path)
}

fn build_endpoint_url(
    base_url: &str,
    field_name: &str,
    endpoint_path: &str,
) -> Result<Url, ColdVoxError> {
    let mut url = parse_base_url(base_url)?;
    let endpoint_path = validate_endpoint_path(field_name, endpoint_path)?;
    let joined_path = join_url_paths(url.path(), endpoint_path);
    url.set_path(&joined_path);
    Ok(url)
}

fn response_body_summary(body: &[u8]) -> String {
    let trimmed = String::from_utf8_lossy(body).trim().to_string();
    if trimmed.is_empty() {
        "empty response body".to_string()
    } else if trimmed.chars().count() > 200 {
        let prefix = trimmed.chars().take(200).collect::<String>();
        format!("body starts with {:?}", prefix)
    } else {
        format!("body {:?}", trimmed)
    }
}

fn map_http_client_error(context: &str, error: reqwest::Error) -> ColdVoxError {
    let message = if error.is_timeout() {
        format!("{context} timed out: {error}")
    } else if error.is_connect() {
        format!("{context} connect failed: {error}")
    } else if error.is_request() {
        format!("{context} request failed: {error}")
    } else if error.is_body() {
        format!("{context} response read failed: {error}")
    } else if error.is_decode() {
        format!("{context} decode failed: {error}")
    } else {
        format!("{context}: {error}")
    };

    SttError::TranscriptionFailed(message).into()
}

fn canonicalize_profile_id_fragment(input: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_separator = false;

    for ch in input.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            last_was_separator = false;
        } else if !last_was_separator {
            normalized.push('-');
            last_was_separator = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

fn derive_profile_id(config: &HttpRemoteConfig) -> String {
    [
        canonicalize_profile_id_fragment(&config.model_name),
        canonicalize_profile_id_fragment(&config.base_url),
        canonicalize_profile_id_fragment(&config.api_path),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("-")
}

fn plugin_id_from_config(config: &HttpRemoteConfig) -> String {
    let explicit_id = config
        .profile_id
        .as_deref()
        .map(canonicalize_profile_id_fragment)
        .filter(|id| !id.is_empty());

    if explicit_id.as_deref() == Some(PLUGIN_ID_PREFIX) {
        return PLUGIN_ID_PREFIX.to_string();
    }

    let suffix = explicit_id.unwrap_or_else(|| derive_profile_id(config));
    if suffix.is_empty() {
        PLUGIN_ID_PREFIX.to_string()
    } else if suffix.starts_with(&format!("{PLUGIN_ID_PREFIX}-")) {
        suffix
    } else {
        format!("{PLUGIN_ID_PREFIX}-{suffix}")
    }
}

fn plugin_info_from_config(config: &HttpRemoteConfig) -> PluginInfo {
    PluginInfo {
        id: plugin_id_from_config(config),
        name: config.display_name.clone(),
        description: format!(
            "Transcribe via HTTP API ({}, model {})",
            config.base_url, config.model_name
        ),
        requires_network: true,
        is_local: is_local_url(&config.base_url),
        is_available: true,
        supported_languages: vec!["en".to_string()],
        memory_usage_mb: Some(10),
    }
}

impl HttpRemotePlugin {
    pub fn new(config: HttpRemoteConfig) -> Self {
        Self {
            config,
            audio_buffer: Vec::new(),
            utterance_id: 0,
        }
    }

    fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.config.timeout_ms)
    }

    fn estimated_audio_duration_secs(&self) -> f64 {
        if self.config.sample_rate == 0 {
            return 0.0;
        }

        self.audio_buffer.len() as f64 / self.config.sample_rate as f64
    }

    fn estimate_payload_bytes(&self, wav_data: &[u8]) -> u64 {
        let static_overhead = 768_u64;
        wav_data.len() as u64 + self.config.model_name.len() as u64 + static_overhead
    }

    fn validate_request_guardrails(&self, wav_data: &[u8]) -> Result<(), ColdVoxError> {
        if wav_data.len() as u64 > self.config.max_audio_bytes {
            return Err(SttError::TranscriptionFailed(format!(
                "Encoded WAV size {} exceeds configured max_audio_bytes {}",
                wav_data.len(),
                self.config.max_audio_bytes
            ))
            .into());
        }

        let duration_secs = self.estimated_audio_duration_secs();
        if duration_secs > self.config.max_audio_seconds as f64 {
            return Err(SttError::TranscriptionFailed(format!(
                "Utterance duration {:.2}s exceeds configured max_audio_seconds {}",
                duration_secs, self.config.max_audio_seconds
            ))
            .into());
        }

        let estimated_payload_bytes = self.estimate_payload_bytes(wav_data);
        if estimated_payload_bytes > self.config.max_payload_bytes {
            return Err(SttError::TranscriptionFailed(format!(
                "Estimated multipart payload {} exceeds configured max_payload_bytes {}",
                estimated_payload_bytes, self.config.max_payload_bytes
            ))
            .into());
        }

        Ok(())
    }

    fn build_http_client(&self) -> Result<Client, ColdVoxError> {
        Client::builder()
            .connect_timeout(self.request_timeout())
            .timeout(self.request_timeout())
            .build()
            .map_err(|e| {
                SttError::TranscriptionFailed(format!("Failed to build HTTP client: {e}")).into()
            })
    }

    fn build_service_url(
        &self,
        field_name: &str,
        endpoint_path: &str,
    ) -> Result<Url, ColdVoxError> {
        build_endpoint_url(&self.config.base_url, field_name, endpoint_path)
    }

    fn apply_common_headers(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ColdVoxError> {
        let mut request = request;

        for (header_name, header_value) in &self.config.headers {
            request = request.header(header_name, header_value);
        }

        if let Some(env_var_name) = &self.config.bearer_token_env_var {
            let token = std::env::var(env_var_name).map_err(|_| {
                SttError::TranscriptionFailed(format!(
                    "Configured bearer token env var '{}' is not set",
                    env_var_name
                ))
            })?;
            request = request.bearer_auth(token);
        }

        Ok(request)
    }

    fn encode_wav(&self) -> Result<Vec<u8>, ColdVoxError> {
        let mut cursor = Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.config.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| {
            SttError::TranscriptionFailed(format!("Failed to create WAV writer: {e}"))
        })?;

        for &sample in &self.audio_buffer {
            writer.write_sample(sample).map_err(|e| {
                SttError::TranscriptionFailed(format!("Failed to write WAV sample: {e}"))
            })?;
        }

        writer
            .finalize()
            .map_err(|e| SttError::TranscriptionFailed(format!("Failed to finalize WAV: {e}")))?;

        Ok(cursor.into_inner())
    }

    async fn send_transcription_request(
        &self,
        wav_data: &[u8],
    ) -> Result<SttResponse, ColdVoxError> {
        self.validate_request_guardrails(wav_data)?;
        let client = self.build_http_client()?;
        let url = self.build_service_url("api_path", &self.config.api_path)?;
        let wav_part = multipart::Part::bytes(wav_data.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| {
                SttError::TranscriptionFailed(format!(
                    "Failed to build HTTP multipart WAV part: {e}"
                ))
            })?;
        let form = multipart::Form::new()
            .text("model", self.config.model_name.clone())
            .text("response_format", "json")
            .part("file", wav_part);

        let response = self
            .apply_common_headers(
                client
                    .post(url.clone())
                    .header(reqwest::header::ACCEPT, "application/json"),
            )?
            .multipart(form)
            .send()
            .await
            .map_err(|e| map_http_client_error("HTTP transcription request failed", e))?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| map_http_client_error("HTTP transcription response read failed", e))?;

        if !status.is_success() {
            return Err(SttError::TranscriptionFailed(format!(
                "Service returned error {status} from {url}: {}",
                response_body_summary(&body)
            ))
            .into());
        }

        serde_json::from_slice::<SttResponse>(&body).map_err(|e| {
            SttError::TranscriptionFailed(format!(
                "Failed to parse STT response from {url}: {e}; {}",
                response_body_summary(&body)
            ))
            .into()
        })
    }

    async fn probe_health(&self) -> Result<bool, ColdVoxError> {
        let client = match self.build_http_client() {
            Ok(client) => client,
            Err(_) => return Ok(false),
        };
        let url = match self.build_service_url("health_path", &self.config.health_path) {
            Ok(url) => url,
            Err(_) => return Ok(false),
        };

        match self.apply_common_headers(client.get(url)) {
            Ok(request) => match request.send().await {
                Ok(response) => Ok(response.status().is_success()),
                Err(_) => Ok(false),
            },
            Err(_) => Ok(false),
        }
    }
}

#[async_trait]
impl SttPlugin for HttpRemotePlugin {
    fn info(&self) -> PluginInfo {
        plugin_info_from_config(&self.config)
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            streaming: false,
            batch: true,
            word_timestamps: false,
            confidence_scores: false,
            speaker_diarization: false,
            auto_punctuation: true,
            custom_vocabulary: false,
        }
    }

    async fn is_available(&self) -> Result<bool, ColdVoxError> {
        self.probe_health().await
    }

    async fn initialize(&mut self, _config: TranscriptionConfig) -> Result<(), ColdVoxError> {
        self.audio_buffer.clear();
        Ok(())
    }

    async fn process_audio(
        &mut self,
        samples: &[i16],
    ) -> Result<Option<TranscriptionEvent>, ColdVoxError> {
        self.audio_buffer.extend_from_slice(samples);
        Ok(None)
    }

    async fn finalize(&mut self) -> Result<Option<TranscriptionEvent>, ColdVoxError> {
        if self.audio_buffer.is_empty() {
            return Ok(None);
        }

        let wav_data = self.encode_wav()?;
        let stt_res = self.send_transcription_request(&wav_data).await?;

        let event = TranscriptionEvent::Final {
            utterance_id: self.utterance_id,
            text: stt_res.text,
            words: None,
        };

        self.audio_buffer.clear();
        self.utterance_id += 1;

        Ok(Some(event))
    }

    async fn reset(&mut self) -> Result<(), ColdVoxError> {
        self.audio_buffer.clear();
        self.utterance_id += 1;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), ColdVoxError> {
        self.audio_buffer.clear();
        Ok(())
    }
}

pub struct HttpRemotePluginFactory {
    config: HttpRemoteConfig,
}

impl HttpRemotePluginFactory {
    pub fn new(config: HttpRemoteConfig) -> Self {
        Self { config }
    }

    pub fn moonshine_base() -> Self {
        Self::new(HttpRemoteConfig::default())
    }

    pub fn canonical_parakeet_cpu() -> Self {
        Self::new(HttpRemoteConfig::canonical_parakeet_cpu())
    }

    pub fn parakeet_gpu() -> Self {
        Self::new(HttpRemoteConfig {
            profile_id: Some("parakeet-gpu".into()),
            base_url: "http://localhost:8200".into(),
            api_path: "/audio/transcriptions".into(),
            health_path: "/healthz".into(),
            model_name: "parakeet".into(),
            display_name: "Parakeet GPU (Docker)".into(),
            timeout_ms: 10_000,
            sample_rate: 16_000,
            headers: HashMap::new(),
            bearer_token_env_var: None,
            max_audio_bytes: default_max_audio_bytes(),
            max_audio_seconds: default_max_audio_seconds(),
            max_payload_bytes: default_max_payload_bytes(),
        })
    }
}

impl SttPluginFactory for HttpRemotePluginFactory {
    fn create(&self) -> Result<Box<dyn SttPlugin>, ColdVoxError> {
        Ok(Box::new(HttpRemotePlugin::new(self.config.clone())))
    }

    fn plugin_info(&self) -> PluginInfo {
        plugin_info_from_config(&self.config)
    }

    fn check_requirements(&self) -> Result<(), ColdVoxError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::SttPlugin;
    use crate::types::{TranscriptionConfig, TranscriptionEvent};
    use std::future::Future;
    use std::sync::mpsc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream as TestStream};
    use tokio::task::JoinHandle;
    use tokio::time::{sleep, timeout};

    fn test_samples() -> Vec<i16> {
        vec![0, 1024, -1024, 16_384, -16_384, 32_767, -32_768, 256]
    }

    fn test_config(base_url: String) -> HttpRemoteConfig {
        HttpRemoteConfig {
            profile_id: Some("test-profile".into()),
            base_url,
            api_path: "/v1/audio/transcriptions".into(),
            model_name: "moonshine/test".into(),
            display_name: "HTTP Remote Test".into(),
            timeout_ms: 100,
            sample_rate: 16_000,
            ..Default::default()
        }
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn extract_http_body(response: &[u8]) -> Result<&[u8], ColdVoxError> {
        let marker = b"\r\n\r\n";
        response
            .windows(marker.len())
            .position(|window| window == marker)
            .map(|idx| &response[idx + marker.len()..])
            .ok_or_else(|| {
                SttError::TranscriptionFailed("HTTP response missing header terminator".to_string())
                    .into()
            })
    }

    fn header_value(headers: &str, header_name: &str) -> Option<String> {
        headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case(header_name) {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
    }

    fn multipart_boundary(content_type: &str) -> Option<String> {
        content_type.split(';').find_map(|part| {
            part.trim()
                .strip_prefix("boundary=")
                .map(|boundary| boundary.trim_matches('"').to_string())
        })
    }

    async fn read_http_request(stream: &mut TestStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        let mut expected_len = None;

        loop {
            let read = timeout(Duration::from_millis(500), stream.read(&mut chunk))
                .await
                .expect("timed out reading HTTP request")
                .expect("read HTTP request");
            if read == 0 {
                break;
            }

            request.extend_from_slice(&chunk[..read]);

            if expected_len.is_none() {
                if let Some(header_end) = find_bytes(&request, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end + 4]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("Content-Length: ")
                                .or_else(|| line.strip_prefix("content-length: "))
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    expected_len = Some(header_end + 4 + content_length);
                }
            }

            if let Some(expected_len) = expected_len {
                if request.len() >= expected_len {
                    break;
                }
            }
        }

        request
    }

    async fn spawn_stub_server<F, Fut>(handler: F) -> (String, JoinHandle<()>)
    where
        F: FnOnce(TestStream) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub server");
        let addr = listener.local_addr().expect("stub server address");
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept stub connection");
            handler(stream).await;
        });

        (format!("http://127.0.0.1:{}", addr.port()), handle)
    }

    #[test]
    fn test_wav_encoding() {
        let config = HttpRemoteConfig::default();
        let mut plugin = HttpRemotePlugin::new(config);

        let samples: Vec<i16> = (0..16000)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 16000.0).sin() * 32767.0)
            .map(|s| s as i16)
            .collect();

        plugin.audio_buffer.extend_from_slice(&samples);

        let wav_data = plugin.encode_wav().expect("Should encode WAV");
        assert!(wav_data.len() > 32000);

        let mut reader = hound::WavReader::new(Cursor::new(wav_data)).expect("Should read WAV");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 16);

        let read_samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(read_samples.len(), 16000);
        assert_eq!(read_samples[0], samples[0]);
    }

    #[test]
    fn test_info_is_correct() {
        let config = HttpRemoteConfig {
            base_url: "http://localhost:1234".into(),
            display_name: "Test Plugin".into(),
            ..Default::default()
        };
        let plugin = HttpRemotePlugin::new(config);
        let info = plugin.info();

        assert_eq!(info.id, "http-remote-moonshine-base");
        assert!(info.is_local);
        assert!(info.description.contains("moonshine/base"));
    }

    #[test]
    fn test_canonical_parakeet_profile_uses_canonical_id() {
        let config = HttpRemoteConfig::canonical_parakeet_cpu();
        let plugin = HttpRemotePlugin::new(config.clone());
        let info = plugin.info();

        assert_eq!(info.id, "http-remote");
        assert_eq!(config.health_path, "/health");
        assert_eq!(config.base_url, "http://localhost:5092");
        assert_eq!(config.model_name, "parakeet-tdt-0.6b-v2");
    }

    #[test]
    fn test_parakeet_gpu_profile_uses_gpu_contract() {
        let factory = HttpRemotePluginFactory::parakeet_gpu();
        let info = factory.plugin_info();

        assert_eq!(info.id, "http-remote-parakeet-gpu");
        assert!(info.description.contains("http://localhost:8200"));

        let plugin = factory.create().expect("create gpu plugin");
        assert_eq!(plugin.info().id, "http-remote-parakeet-gpu");
    }

    #[test]
    fn test_info_derives_deterministic_id_when_profile_id_missing() {
        let config = HttpRemoteConfig {
            profile_id: None,
            base_url: "http://127.0.0.1:5092/service".into(),
            api_path: "/v1/audio/transcriptions".into(),
            model_name: "parakeet/test".into(),
            display_name: "Derived Identity".into(),
            timeout_ms: 100,
            sample_rate: 16_000,
            ..HttpRemoteConfig::canonical_parakeet_cpu()
        };

        let plugin_a = HttpRemotePlugin::new(config.clone());
        let plugin_b = HttpRemotePlugin::new(config);

        assert_eq!(plugin_a.info().id, plugin_b.info().id);
    }

    #[tokio::test]
    async fn test_finalize_posts_wav_and_metadata_to_http_service() {
        let (tx, rx) = mpsc::channel();
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let request = read_http_request(&mut stream).await;
            tx.send(request).expect("capture HTTP request");

            let body = r#"{"text":"stub transcript"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write HTTP response");
        })
        .await;

        let mut plugin = HttpRemotePlugin::new(test_config(format!("{base_url}/service")));
        let samples = test_samples();

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&samples)
            .await
            .expect("buffer audio samples");

        let event = plugin.finalize().await.expect("finalize succeeds");
        handle.await.expect("join stub server");

        match event {
            Some(TranscriptionEvent::Final {
                utterance_id,
                text,
                words,
            }) => {
                assert_eq!(utterance_id, 0);
                assert_eq!(text, "stub transcript");
                assert!(words.is_none());
            }
            other => panic!("expected final transcription event, got {other:?}"),
        }

        let request = rx.recv().expect("captured request");
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("POST /service/v1/audio/transcriptions HTTP/1.1\r\n"));
        let content_type =
            header_value(&request_text, "content-type").expect("content-type header");
        assert!(content_type.contains("multipart/form-data"));
        assert_eq!(
            header_value(&request_text, "accept").as_deref(),
            Some("application/json")
        );

        let body = extract_http_body(&request).expect("extract request body");
        let body_text = String::from_utf8_lossy(body);
        assert!(body_text.contains("name=\"model\"\r\n\r\nmoonshine/test\r\n"));
        assert!(body_text.contains("name=\"response_format\"\r\n\r\njson\r\n"));
        assert!(body_text.contains("filename=\"audio.wav\""));

        let boundary = multipart_boundary(&content_type).expect("multipart boundary");

        let wav_start = find_bytes(body, b"Content-Type: audio/wav\r\n\r\n")
            .expect("locate WAV part start")
            + b"Content-Type: audio/wav\r\n\r\n".len();
        let wav_end_marker = format!("\r\n--{boundary}--");
        let wav_end = find_bytes(body, wav_end_marker.as_bytes()).expect("locate WAV part end");
        let wav_data = &body[wav_start..wav_end];

        let mut reader = hound::WavReader::new(Cursor::new(wav_data)).expect("read request WAV");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.bits_per_sample, 16);

        let roundtrip_samples: Vec<i16> = reader
            .samples::<i16>()
            .map(|sample| sample.unwrap())
            .collect();
        assert_eq!(roundtrip_samples, samples);
    }

    #[tokio::test]
    async fn test_finalize_fails_for_non_200_response() {
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let _request = read_http_request(&mut stream).await;
            let body = r#"{"error":"unavailable"}"#;
            let response = format!(
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write HTTP response");
        })
        .await;
        let mut plugin = HttpRemotePlugin::new(test_config(base_url));

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&test_samples())
            .await
            .expect("buffer audio samples");

        let error = plugin
            .finalize()
            .await
            .expect_err("non-200 response should fail");
        handle.await.expect("join stub server");
        let message = error.to_string();
        assert!(message.contains("Service returned error 503 Service Unavailable"));
        assert!(message.contains("unavailable"));
    }

    #[tokio::test]
    async fn test_finalize_fails_for_malformed_json_response() {
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let _request = read_http_request(&mut stream).await;
            let body = "not-json!!!?";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write malformed JSON response");
        })
        .await;
        let mut plugin = HttpRemotePlugin::new(test_config(base_url));

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&test_samples())
            .await
            .expect("buffer audio samples");

        let error = plugin
            .finalize()
            .await
            .expect_err("malformed JSON should fail");
        handle.await.expect("join stub server");
        assert!(error.to_string().contains("Failed to parse STT response"));
    }

    #[tokio::test]
    async fn test_finalize_rejects_invalid_api_path() {
        let mut config = test_config("http://127.0.0.1:5092".to_string());
        config.api_path = "v1/audio/transcriptions".into();
        let mut plugin = HttpRemotePlugin::new(config);

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&test_samples())
            .await
            .expect("buffer audio samples");

        let error = plugin
            .finalize()
            .await
            .expect_err("relative api path should fail");
        assert!(error.to_string().contains("Invalid HTTP remote api_path"));
    }

    #[tokio::test]
    async fn test_finalize_fails_when_response_times_out() {
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let _request = read_http_request(&mut stream).await;
            sleep(Duration::from_millis(250)).await;
        })
        .await;
        let mut config = test_config(base_url);
        config.timeout_ms = 50;
        let mut plugin = HttpRemotePlugin::new(config);

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&test_samples())
            .await
            .expect("buffer audio samples");

        let error = plugin
            .finalize()
            .await
            .expect_err("timed-out response should fail");
        handle.await.expect("join stub server");
        let message = error.to_string();
        assert!(
            message.contains("timed out")
                || message.contains("deadline has elapsed")
                || message.contains("operation timed out")
                || message.contains("os error 10060"),
            "unexpected timeout message: {message}"
        );
    }

    #[tokio::test]
    async fn test_is_available_probes_health_endpoint() {
        let (tx, rx) = mpsc::channel();
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let request = read_http_request(&mut stream).await;
            tx.send(request).expect("capture health request");

            let body = r#"{"status":"ok"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write health response");
        })
        .await;
        let plugin = HttpRemotePlugin::new(test_config(format!("{base_url}/service")));

        assert!(plugin.is_available().await.expect("health probe succeeds"));
        handle.await.expect("join stub server");

        let request = rx.recv().expect("captured health request");
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("GET /service/health HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn test_health_probe_uses_configured_health_path() {
        let (tx, rx) = mpsc::channel();
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let request = read_http_request(&mut stream).await;
            tx.send(request).expect("capture custom health request");

            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Length: 0\r\n",
                "Connection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write custom health response");
        })
        .await;

        let mut config = test_config(format!("{base_url}/service"));
        config.health_path = "/readyz".into();
        let plugin = HttpRemotePlugin::new(config);

        assert!(plugin.is_available().await.expect("health probe completes"));
        handle.await.expect("join stub server");

        let request = rx.recv().expect("captured health request");
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("GET /service/readyz HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn test_finalize_rejects_audio_that_exceeds_guardrails() {
        let mut config = test_config("http://127.0.0.1:5092".to_string());
        config.max_audio_bytes = 128;
        let mut plugin = HttpRemotePlugin::new(config);

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&vec![1_i16; 512])
            .await
            .expect("buffer audio samples");

        let error = plugin
            .finalize()
            .await
            .expect_err("oversized payload should fail before request send");
        assert!(error.to_string().contains("max_audio_bytes"));
    }

    #[tokio::test]
    async fn test_finalize_applies_configured_headers_and_auth() {
        let (tx, rx) = mpsc::channel();
        let (base_url, handle) = spawn_stub_server(move |mut stream| async move {
            let request = read_http_request(&mut stream).await;
            tx.send(request).expect("capture HTTP request");

            let body = r#"{"text":"stub transcript"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write HTTP response");
        })
        .await;

        let token_var = "COLDVOX_TEST_REMOTE_TOKEN";
        std::env::set_var(token_var, "secret-token");

        let mut config = test_config(base_url);
        config
            .headers
            .insert("x-test-header".into(), "enabled".into());
        config.bearer_token_env_var = Some(token_var.into());
        let mut plugin = HttpRemotePlugin::new(config);

        plugin
            .initialize(TranscriptionConfig::default())
            .await
            .expect("initialize plugin");
        plugin
            .process_audio(&test_samples())
            .await
            .expect("buffer audio samples");
        let _ = plugin.finalize().await.expect("finalize succeeds");
        handle.await.expect("join stub server");
        std::env::remove_var(token_var);

        let request = rx.recv().expect("captured request");
        let request_text = String::from_utf8_lossy(&request);
        assert_eq!(
            header_value(&request_text, "x-test-header").as_deref(),
            Some("enabled")
        );
        assert_eq!(
            header_value(&request_text, "authorization").as_deref(),
            Some("Bearer secret-token")
        );
    }
}
