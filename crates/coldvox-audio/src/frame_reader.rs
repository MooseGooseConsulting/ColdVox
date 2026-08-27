use std::sync::Arc;
use std::time::Instant;

use coldvox_telemetry::{BufferType, PipelineMetrics};

use super::capture::AudioFrame;
use super::ring_buffer::AudioConsumer;

/// Reads audio frames from ring buffer and reconstructs metadata
pub struct FrameReader {
    consumer: AudioConsumer,
    device_sample_rate: u32,
    device_channels: u16,
    samples_read: u64,
    start_time: Instant,
    metrics: Option<Arc<PipelineMetrics>>,
    capacity: usize,
}

impl FrameReader {
    /// Create a new FrameReader
    pub fn new(
        consumer: AudioConsumer,
        device_sample_rate: u32,
        device_channels: u16,
        capacity: usize,
        metrics: Option<Arc<PipelineMetrics>>,
    ) -> Self {
        Self {
            consumer,
            device_sample_rate,
            device_channels,
            samples_read: 0,
            start_time: Instant::now(),
            metrics,
            capacity,
        }
    }

    /// Read next audio frame, reconstructing timestamp from sample count
    pub fn read_frame(&mut self, max_samples: usize) -> Option<AudioFrame> {
        if let Some(metrics) = &self.metrics {
            let available = self.consumer.slots();
            let fill_percent = (available * 100).checked_div(self.capacity).unwrap_or(0);
            metrics.update_buffer_fill(BufferType::Capture, fill_percent);
        }

        let mut buffer = vec![0i16; max_samples];
        let samples_read = self.consumer.read(&mut buffer);

        if samples_read == 0 {
            tracing::trace!("FrameReader: No samples available to read");
            return None;
        }

        buffer.truncate(samples_read);

        // Calculate timestamp based on samples read
        let elapsed_samples = self.samples_read;
        let elapsed_ms = (elapsed_samples * 1000) / self.device_sample_rate as u64;
        let timestamp = self.start_time + std::time::Duration::from_millis(elapsed_ms);

        self.samples_read += samples_read as u64;

        tracing::trace!(
            "FrameReader: Read {} samples at {}Hz {}ch, timestamp: {:?}",
            samples_read,
            self.device_sample_rate,
            self.device_channels,
            timestamp
        );

        Some(AudioFrame {
            samples: buffer,
            timestamp,
            sample_rate: self.device_sample_rate,
            channels: self.device_channels,
        })
    }

    /// Check how many samples are available to read
    pub fn available_samples(&self) -> usize {
        self.consumer.slots()
    }

    /// Update device configuration when it changes
    pub fn update_device_config(&mut self, sample_rate: u32, channels: u16) {
        if self.device_sample_rate != sample_rate || self.device_channels != channels {
            tracing::info!(
                "FrameReader: Device config changed from {}Hz {}ch to {}Hz {}ch",
                self.device_sample_rate,
                self.device_channels,
                sample_rate,
                channels
            );
            self.device_sample_rate = sample_rate;
            self.device_channels = channels;
        }
    }
}
