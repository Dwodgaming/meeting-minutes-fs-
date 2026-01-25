// audio/file_loader.rs
//
// Audio file loading module for transcription of pre-recorded files.
// Supports loading audio from various formats (mp3, wav, m4a, mp4, ogg, flac, webm)
// and converting to the format required by transcription engines (16kHz mono f32).

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::audio_processing::resample;

/// Information about an audio file
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioFileInfo {
    /// Duration in seconds
    pub duration_seconds: f64,
    /// Original sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// File format/codec
    pub format: String,
    /// File size in bytes
    pub file_size_bytes: u64,
}

/// Supported audio file extensions
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "m4a", "mp4", "ogg", "flac", "webm", "aac", "wma", "opus",
];

/// Target sample rate for transcription (Whisper expects 16kHz)
const TARGET_SAMPLE_RATE: u32 = 16000;

/// Validate that a file is a supported audio format
pub fn validate_audio_file(file_path: &Path) -> Result<AudioFileInfo> {
    // Check file exists
    if !file_path.exists() {
        anyhow::bail!("File does not exist: {}", file_path.display());
    }

    // Check file extension
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .ok_or_else(|| anyhow::anyhow!("File has no extension"))?;

    if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        anyhow::bail!(
            "Unsupported audio format: .{}. Supported formats: {}",
            extension,
            SUPPORTED_EXTENSIONS.join(", ")
        );
    }

    // Get file size
    let metadata = std::fs::metadata(file_path).context("Failed to read file metadata")?;
    let file_size_bytes = metadata.len();

    // Open and probe the file to get audio info
    let file = std::fs::File::open(file_path).context("Failed to open audio file")?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension(&extension);

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("Failed to probe audio format")?;

    let format = probed.format;

    // Get the default track
    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("No audio tracks found in file"))?;

    let codec_params = &track.codec_params;

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| anyhow::anyhow!("Could not determine sample rate"))?;

    let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);

    // Calculate duration
    let duration_seconds = if let Some(n_frames) = codec_params.n_frames {
        n_frames as f64 / sample_rate as f64
    } else {
        // Estimate from file size (rough approximation)
        let bits_per_sample = codec_params.bits_per_sample.unwrap_or(16) as f64;
        let bytes_per_second = (sample_rate as f64 * channels as f64 * bits_per_sample) / 8.0;
        if bytes_per_second > 0.0 {
            file_size_bytes as f64 / bytes_per_second
        } else {
            0.0
        }
    };

    // Get format name
    let format_name = extension.clone();

    info!(
        "Audio file validated: {} - {:.1}s, {}Hz, {} channels",
        file_path.display(),
        duration_seconds,
        sample_rate,
        channels
    );

    Ok(AudioFileInfo {
        duration_seconds,
        sample_rate,
        channels,
        format: format_name,
        file_size_bytes,
    })
}

/// Load an audio file and convert to 16kHz mono f32 samples for transcription
///
/// This function:
/// 1. Opens and decodes the audio file using symphonia
/// 2. Converts to mono if needed
/// 3. Resamples to 16kHz
/// 4. Returns f32 samples ready for Whisper/Parakeet transcription
pub fn load_audio_file(file_path: &Path) -> Result<Vec<f32>> {
    info!("Loading audio file: {}", file_path.display());

    // Validate first
    let info = validate_audio_file(file_path)?;

    // Open the file
    let file = std::fs::File::open(file_path).context("Failed to open audio file")?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    let mut hint = Hint::new();
    hint.with_extension(extension);

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("Failed to probe audio format")?;

    let mut format = probed.format;

    // Get the default track
    let track = format
        .default_track()
        .ok_or_else(|| anyhow::anyhow!("No audio tracks found"))?
        .clone();

    let track_id = track.id;

    // Create decoder
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("Failed to create audio decoder")?;

    let source_sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow::anyhow!("Could not determine sample rate"))?;

    let source_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

    // Collect all samples
    let mut all_samples: Vec<f32> = Vec::new();

    // Decode all packets
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                // End of file
                break;
            }
            Err(e) => {
                warn!("Error reading packet: {}", e);
                break;
            }
        };

        // Skip packets from other tracks
        if packet.track_id() != track_id {
            continue;
        }

        // Decode the packet
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(e) => {
                warn!("Error decoding packet: {}", e);
                continue;
            }
        };

        // Get audio spec
        let spec = *decoded.spec();
        let duration = decoded.capacity() as usize;

        // Create sample buffer
        let mut sample_buf = SampleBuffer::<f32>::new(duration as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        // Get samples
        let samples = sample_buf.samples();

        // Convert to mono if needed
        if source_channels > 1 {
            // Average channels to mono
            for chunk in samples.chunks(source_channels) {
                let mono_sample: f32 = chunk.iter().sum::<f32>() / source_channels as f32;
                all_samples.push(mono_sample);
            }
        } else {
            all_samples.extend_from_slice(samples);
        }
    }

    debug!(
        "Decoded {} samples at {}Hz",
        all_samples.len(),
        source_sample_rate
    );

    // Resample to 16kHz if needed
    let final_samples = if source_sample_rate != TARGET_SAMPLE_RATE {
        info!(
            "Resampling from {}Hz to {}Hz",
            source_sample_rate, TARGET_SAMPLE_RATE
        );
        resample(&all_samples, source_sample_rate, TARGET_SAMPLE_RATE)
            .context("Failed to resample audio")?
    } else {
        all_samples
    };

    info!(
        "Audio loaded: {} samples ({:.1}s at {}Hz)",
        final_samples.len(),
        final_samples.len() as f64 / TARGET_SAMPLE_RATE as f64,
        TARGET_SAMPLE_RATE
    );

    Ok(final_samples)
}

/// Load audio file and return both samples and file info
pub fn load_audio_file_with_info(file_path: &Path) -> Result<(Vec<f32>, AudioFileInfo)> {
    let info = validate_audio_file(file_path)?;
    let samples = load_audio_file(file_path)?;
    Ok((samples, info))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_supported_extensions() {
        assert!(SUPPORTED_EXTENSIONS.contains(&"mp3"));
        assert!(SUPPORTED_EXTENSIONS.contains(&"wav"));
        assert!(SUPPORTED_EXTENSIONS.contains(&"m4a"));
    }

    #[test]
    fn test_validate_nonexistent_file() {
        let result = validate_audio_file(Path::new("/nonexistent/file.mp3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_unsupported_extension() {
        // Create a temp file with unsupported extension
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test.txt");
        std::fs::write(&temp_file, "test").unwrap();

        let result = validate_audio_file(&temp_file);
        assert!(result.is_err());

        std::fs::remove_file(&temp_file).ok();
    }
}
