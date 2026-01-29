// audio/file_transcription_commands.rs
//
// Tauri commands for transcribing pre-recorded audio/video files.
// Supports local transcription (Whisper/Parakeet) and cloud (Deepgram) providers.

use log::{error, info};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{command, AppHandle, Emitter, Runtime};

use super::file_loader::{load_audio_file, validate_audio_file, AudioFileInfo};
use super::transcription::{
    DeepgramOptions, DeepgramProvider, TranscriptionError, TranscriptionProvider,
};
use crate::parakeet_engine::commands::PARAKEET_ENGINE;
use crate::whisper_engine::commands::WHISPER_ENGINE;

// Global Deepgram provider instance
static DEEPGRAM_PROVIDER: Mutex<Option<DeepgramProvider>> = Mutex::new(None);

/// Result of file transcription
#[derive(Debug, Clone, Serialize)]
pub struct FileTranscriptionResult {
    /// Transcribed text
    pub text: String,
    /// Duration of audio in seconds
    pub duration_seconds: f64,
    /// Provider used for transcription
    pub provider: String,
    /// Model used (if applicable)
    pub model: Option<String>,
    /// Confidence score (if available)
    pub confidence: Option<f32>,
    /// Original file info
    pub file_info: AudioFileInfo,
}

/// Progress update during transcription
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionProgress {
    /// Current stage: "loading", "processing", "transcribing", "complete"
    pub stage: String,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Current status message
    pub message: String,
}

/// Available transcription providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionProviderType {
    Whisper,
    Parakeet,
    Deepgram,
}

impl std::fmt::Display for TranscriptionProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptionProviderType::Whisper => write!(f, "whisper"),
            TranscriptionProviderType::Parakeet => write!(f, "parakeet"),
            TranscriptionProviderType::Deepgram => write!(f, "deepgram"),
        }
    }
}

/// Validate an audio file before transcription
#[command]
pub async fn validate_file_for_transcription(file_path: String) -> Result<AudioFileInfo, String> {
    let path = PathBuf::from(&file_path);

    validate_audio_file(&path).map_err(|e| format!("File validation failed: {}", e))
}

/// Transcribe an audio/video file
///
/// # Arguments
/// * `file_path` - Path to the audio/video file
/// * `provider` - Transcription provider to use ("whisper", "parakeet", or "deepgram")
/// * `language` - Optional language code (e.g., "en", "es")
#[command]
pub async fn transcribe_file<R: Runtime>(
    app: AppHandle<R>,
    file_path: String,
    provider: TranscriptionProviderType,
    language: Option<String>,
) -> Result<FileTranscriptionResult, String> {
    let path = PathBuf::from(&file_path);

    info!(
        "Starting file transcription: {} with provider {:?}",
        file_path, provider
    );

    // Emit progress: loading
    emit_progress(&app, "loading", 0, "Loading audio file...");

    // Validate and get file info
    let file_info =
        validate_audio_file(&path).map_err(|e| format!("File validation failed: {}", e))?;

    emit_progress(&app, "loading", 20, "Decoding audio...");

    // Load audio file
    let audio_samples =
        load_audio_file(&path).map_err(|e| format!("Failed to load audio: {}", e))?;

    emit_progress(&app, "processing", 40, "Preparing for transcription...");

    // Transcribe based on provider
    let (text, confidence, model_name) = match provider {
        TranscriptionProviderType::Whisper => {
            transcribe_with_whisper(&app, audio_samples, language.clone()).await?
        }
        TranscriptionProviderType::Parakeet => {
            transcribe_with_parakeet(&app, audio_samples, language.clone()).await?
        }
        TranscriptionProviderType::Deepgram => {
            transcribe_with_deepgram(&app, audio_samples, language.clone()).await?
        }
    };

    emit_progress(&app, "complete", 100, "Transcription complete!");

    info!("File transcription complete: {} characters", text.len());

    Ok(FileTranscriptionResult {
        text,
        duration_seconds: file_info.duration_seconds,
        provider: provider.to_string(),
        model: model_name,
        confidence,
        file_info,
    })
}

/// Transcribe using local Whisper engine
async fn transcribe_with_whisper<R: Runtime>(
    app: &AppHandle<R>,
    audio: Vec<f32>,
    language: Option<String>,
) -> Result<(String, Option<f32>, Option<String>), String> {
    emit_progress(app, "transcribing", 50, "Transcribing with Whisper...");

    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Check if model is loaded
        if !engine.is_model_loaded().await {
            return Err("Whisper model not loaded. Please load a model first.".to_string());
        }

        emit_progress(app, "transcribing", 60, "Processing audio...");

        let result = engine
            .transcribe_audio(audio, language)
            .await
            .map_err(|e| format!("Whisper transcription failed: {}", e))?;

        let model_name = engine.get_current_model().await;

        emit_progress(app, "transcribing", 90, "Finalizing...");

        Ok((result, None, model_name))
    } else {
        Err("Whisper engine not initialized. Please initialize Whisper first.".to_string())
    }
}

/// Transcribe using local Parakeet engine
async fn transcribe_with_parakeet<R: Runtime>(
    app: &AppHandle<R>,
    audio: Vec<f32>,
    _language: Option<String>, // Parakeet doesn't support language selection yet
) -> Result<(String, Option<f32>, Option<String>), String> {
    emit_progress(app, "transcribing", 50, "Transcribing with Parakeet...");

    let engine = {
        let guard = PARAKEET_ENGINE.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(engine) = engine {
        // Check if model is loaded
        if !engine.is_model_loaded().await {
            return Err("Parakeet model not loaded. Please load a model first.".to_string());
        }

        emit_progress(app, "transcribing", 60, "Processing audio...");

        // Note: Parakeet doesn't support language parameter
        let result = engine
            .transcribe_audio(audio)
            .await
            .map_err(|e| format!("Parakeet transcription failed: {}", e))?;

        let model_name = engine.get_current_model().await;

        emit_progress(app, "transcribing", 90, "Finalizing...");

        Ok((result, None, model_name))
    } else {
        Err("Parakeet engine not initialized. Please initialize Parakeet first.".to_string())
    }
}

/// Transcribe using Deepgram API
async fn transcribe_with_deepgram<R: Runtime>(
    app: &AppHandle<R>,
    audio: Vec<f32>,
    language: Option<String>,
) -> Result<(String, Option<f32>, Option<String>), String> {
    emit_progress(app, "transcribing", 50, "Sending to Deepgram API...");

    let provider = {
        let guard = DEEPGRAM_PROVIDER.lock().unwrap();
        guard.as_ref().cloned()
    };

    if let Some(provider) = provider {
        if !provider.is_configured().await {
            return Err(
                "Deepgram API key not configured. Please set your API key first.".to_string(),
            );
        }

        emit_progress(app, "transcribing", 60, "Processing with Deepgram...");

        let result = provider
            .transcribe(audio, language)
            .await
            .map_err(|e| format!("Deepgram transcription failed: {}", e))?;

        let model_name = provider.get_current_model().await;

        emit_progress(app, "transcribing", 90, "Finalizing...");

        Ok((result.text, result.confidence, model_name))
    } else {
        Err("Deepgram provider not initialized. Please initialize Deepgram first.".to_string())
    }
}

/// Initialize the Deepgram provider
#[command]
pub async fn init_deepgram_provider() -> Result<(), String> {
    let mut guard = DEEPGRAM_PROVIDER.lock().unwrap();
    if guard.is_none() {
        *guard = Some(DeepgramProvider::new());
        info!("Deepgram provider initialized");
    }
    Ok(())
}

/// Configure Deepgram API key
#[command]
pub async fn set_deepgram_api_key(api_key: String) -> Result<(), String> {
    let mut guard = DEEPGRAM_PROVIDER.lock().unwrap();
    if guard.is_none() {
        *guard = Some(DeepgramProvider::new());
    }

    if let Some(provider) = guard.as_ref() {
        // Clone the provider to avoid holding the lock during async operation
        let provider = provider.clone();
        drop(guard);
        provider.set_api_key(api_key).await;
        info!("Deepgram API key configured");
        Ok(())
    } else {
        Err("Failed to get Deepgram provider".to_string())
    }
}

/// Check if Deepgram is configured
#[command]
pub async fn is_deepgram_configured() -> Result<bool, String> {
    let guard = DEEPGRAM_PROVIDER.lock().unwrap();
    if let Some(provider) = guard.as_ref() {
        let provider = provider.clone();
        drop(guard);
        Ok(provider.is_configured().await)
    } else {
        Ok(false)
    }
}

/// Set Deepgram transcription options
#[command]
pub async fn set_deepgram_options(
    model: Option<String>,
    language: Option<String>,
    diarize: Option<bool>,
    punctuate: Option<bool>,
    smart_format: Option<bool>,
) -> Result<(), String> {
    let guard = DEEPGRAM_PROVIDER.lock().unwrap();
    if let Some(provider) = guard.as_ref() {
        let provider = provider.clone();
        drop(guard);

        let mut options = DeepgramOptions::default();

        if let Some(m) = model {
            options.model = m;
        }
        if let Some(l) = language {
            options.language = Some(l);
        }
        if let Some(d) = diarize {
            options.diarize = d;
        }
        if let Some(p) = punctuate {
            options.punctuate = p;
        }
        if let Some(s) = smart_format {
            options.smart_format = s;
        }

        provider.set_options(options).await;
        info!("Deepgram options updated");
        Ok(())
    } else {
        Err("Deepgram provider not initialized".to_string())
    }
}

/// Get list of available transcription providers
#[command]
pub async fn get_available_file_transcription_providers() -> Vec<String> {
    let mut providers = vec!["whisper".to_string(), "parakeet".to_string()];

    // Check if Deepgram is configured
    let deepgram_configured = {
        let guard = DEEPGRAM_PROVIDER.lock().unwrap();
        if let Some(provider) = guard.as_ref() {
            let provider = provider.clone();
            drop(guard);
            provider.is_configured().await
        } else {
            false
        }
    };

    if deepgram_configured {
        providers.push("deepgram".to_string());
    }

    providers
}

/// Helper to emit progress events
fn emit_progress<R: Runtime>(app: &AppHandle<R>, stage: &str, progress: u8, message: &str) {
    let update = TranscriptionProgress {
        stage: stage.to_string(),
        progress,
        message: message.to_string(),
    };

    if let Err(e) = app.emit("file-transcription-progress", &update) {
        error!("Failed to emit progress event: {}", e);
    }
}
