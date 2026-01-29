// audio/transcription/deepgram_provider.rs
//
// Deepgram API transcription provider.
// Sends audio to Deepgram's cloud API for fast, accurate transcription.

use async_trait::async_trait;
use log::{debug, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};

/// Deepgram API endpoint
const DEEPGRAM_API_URL: &str = "https://api.deepgram.com/v1/listen";

/// Deepgram transcription options
#[derive(Debug, Clone, Serialize)]
pub struct DeepgramOptions {
    /// Model to use (e.g., "nova-2", "nova-2-general", "whisper-medium")
    pub model: String,
    /// Language code (e.g., "en", "es", "fr")
    pub language: Option<String>,
    /// Enable speaker diarization
    pub diarize: bool,
    /// Enable punctuation
    pub punctuate: bool,
    /// Enable smart formatting
    pub smart_format: bool,
}

impl Default for DeepgramOptions {
    fn default() -> Self {
        Self {
            model: "nova-2".to_string(),
            language: Some("en".to_string()),
            diarize: false,
            punctuate: true,
            smart_format: true,
        }
    }
}

/// Response from Deepgram API
#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: Option<DeepgramResults>,
    #[serde(default)]
    metadata: Option<DeepgramMetadata>,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: String,
    confidence: f64,
    #[serde(default)]
    words: Vec<DeepgramWord>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    word: String,
    start: f64,
    end: f64,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct DeepgramMetadata {
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    channels: i32,
}

/// Error response from Deepgram
#[derive(Debug, Deserialize)]
struct DeepgramError {
    #[serde(default)]
    err_code: String,
    #[serde(default)]
    err_msg: String,
}

/// Deepgram transcription provider
pub struct DeepgramProvider {
    api_key: Arc<RwLock<Option<String>>>,
    options: Arc<RwLock<DeepgramOptions>>,
    client: Client,
}

impl DeepgramProvider {
    /// Create a new Deepgram provider
    pub fn new() -> Self {
        Self {
            api_key: Arc::new(RwLock::new(None)),
            options: Arc::new(RwLock::new(DeepgramOptions::default())),
            client: Client::new(),
        }
    }

    /// Create with API key
    pub fn with_api_key(api_key: String) -> Self {
        Self {
            api_key: Arc::new(RwLock::new(Some(api_key))),
            options: Arc::new(RwLock::new(DeepgramOptions::default())),
            client: Client::new(),
        }
    }

    /// Set the API key
    pub async fn set_api_key(&self, api_key: String) {
        let mut guard = self.api_key.write().await;
        *guard = Some(api_key);
    }

    /// Set transcription options
    pub async fn set_options(&self, options: DeepgramOptions) {
        let mut guard = self.options.write().await;
        *guard = options;
    }

    /// Check if API key is configured
    pub async fn is_configured(&self) -> bool {
        let guard = self.api_key.read().await;
        guard.is_some()
    }

    /// Transcribe audio using Deepgram API
    ///
    /// # Arguments
    /// * `audio_bytes` - Raw audio data (WAV format recommended)
    /// * `content_type` - MIME type of the audio (e.g., "audio/wav")
    ///
    /// # Returns
    /// Transcription result with text and confidence
    pub async fn transcribe_bytes(
        &self,
        audio_bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<TranscriptResult, TranscriptionError> {
        let api_key = {
            let guard = self.api_key.read().await;
            guard.clone().ok_or_else(|| {
                TranscriptionError::EngineFailed("Deepgram API key not configured".to_string())
            })?
        };

        let options = {
            let guard = self.options.read().await;
            guard.clone()
        };

        // Build query parameters
        let mut url = url::Url::parse(DEEPGRAM_API_URL)
            .map_err(|e| TranscriptionError::EngineFailed(format!("Invalid URL: {}", e)))?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("model", &options.model);
            query.append_pair("punctuate", &options.punctuate.to_string());
            query.append_pair("diarize", &options.diarize.to_string());
            query.append_pair("smart_format", &options.smart_format.to_string());

            if let Some(lang) = &options.language {
                query.append_pair("language", lang);
            }
        }

        debug!("Sending request to Deepgram: {}", url);

        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Token {}", api_key))
            .header("Content-Type", content_type)
            .body(audio_bytes)
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();

            // Try to parse as Deepgram error
            if let Ok(error) = serde_json::from_str::<DeepgramError>(&error_text) {
                return Err(TranscriptionError::EngineFailed(format!(
                    "Deepgram API error ({}): {} - {}",
                    status, error.err_code, error.err_msg
                )));
            }

            return Err(TranscriptionError::EngineFailed(format!(
                "Deepgram API error ({}): {}",
                status, error_text
            )));
        }

        let result: DeepgramResponse = response.json().await.map_err(|e| {
            TranscriptionError::EngineFailed(format!("Failed to parse response: {}", e))
        })?;

        // Extract transcript from response
        let transcript = result
            .results
            .and_then(|r| r.channels.first().cloned())
            .and_then(|ch| ch.alternatives.first().cloned())
            .map(|alt| (alt.transcript, alt.confidence))
            .ok_or_else(|| {
                TranscriptionError::EngineFailed("No transcript in Deepgram response".to_string())
            })?;

        info!(
            "Deepgram transcription complete: {} characters, confidence: {:.2}",
            transcript.0.len(),
            transcript.1
        );

        Ok(TranscriptResult {
            text: transcript.0,
            confidence: Some(transcript.1 as f32),
            is_partial: false,
        })
    }

    /// Convert f32 audio samples to WAV bytes
    pub fn samples_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
        let num_samples = samples.len();
        let byte_rate = sample_rate * 2; // 16-bit mono
        let data_size = num_samples * 2;
        let file_size = 36 + data_size;

        let mut wav_data = Vec::with_capacity(44 + data_size);

        // RIFF header
        wav_data.extend_from_slice(b"RIFF");
        wav_data.extend_from_slice(&(file_size as u32).to_le_bytes());
        wav_data.extend_from_slice(b"WAVE");

        // fmt chunk
        wav_data.extend_from_slice(b"fmt ");
        wav_data.extend_from_slice(&16u32.to_le_bytes()); // Chunk size
        wav_data.extend_from_slice(&1u16.to_le_bytes()); // Audio format (PCM)
        wav_data.extend_from_slice(&1u16.to_le_bytes()); // Num channels (mono)
        wav_data.extend_from_slice(&sample_rate.to_le_bytes()); // Sample rate
        wav_data.extend_from_slice(&byte_rate.to_le_bytes()); // Byte rate
        wav_data.extend_from_slice(&2u16.to_le_bytes()); // Block align
        wav_data.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

        // data chunk
        wav_data.extend_from_slice(b"data");
        wav_data.extend_from_slice(&(data_size as u32).to_le_bytes());

        // Convert f32 samples to i16 and write
        for &sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let i16_sample = (clamped * 32767.0) as i16;
            wav_data.extend_from_slice(&i16_sample.to_le_bytes());
        }

        wav_data
    }
}

#[async_trait]
impl TranscriptionProvider for DeepgramProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        // Update language if provided
        if let Some(lang) = language {
            let mut options = self.options.write().await;
            options.language = Some(lang);
        }

        // Validate we have samples
        if audio.len() < 1600 {
            // Less than 0.1 seconds at 16kHz
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: 1600,
            });
        }

        // Convert samples to WAV bytes
        let wav_bytes = Self::samples_to_wav(&audio, 16000);

        // Send to Deepgram
        self.transcribe_bytes(wav_bytes, "audio/wav").await
    }

    async fn is_model_loaded(&self) -> bool {
        // Deepgram is cloud-based, always "loaded" if configured
        self.is_configured().await
    }

    async fn get_current_model(&self) -> Option<String> {
        let options = self.options.read().await;
        Some(format!("deepgram:{}", options.model))
    }

    fn provider_name(&self) -> &'static str {
        "deepgram"
    }
}

impl Default for DeepgramProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DeepgramProvider {
    fn clone(&self) -> Self {
        Self {
            api_key: self.api_key.clone(),
            options: self.options.clone(),
            client: reqwest::Client::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_samples_to_wav() {
        let samples = vec![0.0f32; 16000]; // 1 second of silence
        let wav = DeepgramProvider::samples_to_wav(&samples, 16000);

        // Check WAV header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");

        // Should have header (44 bytes) + data (16000 * 2 bytes)
        assert_eq!(wav.len(), 44 + 32000);
    }

    #[tokio::test]
    async fn test_provider_not_configured() {
        let provider = DeepgramProvider::new();
        assert!(!provider.is_configured().await);
    }

    #[tokio::test]
    async fn test_provider_with_api_key() {
        let provider = DeepgramProvider::with_api_key("test-key".to_string());
        assert!(provider.is_configured().await);
    }
}
