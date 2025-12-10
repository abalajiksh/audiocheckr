// src/core/decoder.rs
//
// Audio decoding module with enhanced metadata extraction and validation.
// Uses Symphonia for format-agnostic decoding.

use anyhow::{Context, Result, bail};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::fs::File;
use std::path::Path;

/// Container for decoded audio data and metadata
#[derive(Debug, Clone)]
pub struct AudioData {
    /// Interleaved samples normalized to [-1.0, 1.0]
    pub samples: Vec<f32>,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of audio channels
    pub channels: usize,
    /// Bit depth as reported by the file (may not reflect actual precision)
    pub claimed_bit_depth: u32,
    /// Whether bit depth was inferred vs read from metadata
    pub bit_depth_inferred: bool,
    /// Duration in seconds
    pub duration_secs: f64,
    /// Original codec name
    pub codec_name: String,
    /// Container format
    pub format_name: String,
}

/// Decode audio file to floating-point samples
pub fn decode_audio(path: &Path) -> Result<AudioData> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open file: {}", path.display()))?;
    
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension() {
        hint.with_extension(ext.to_str().unwrap_or(""));
    }

    let meta_opts = MetadataOptions::default();
    let fmt_opts = FormatOptions::default();

    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .context("Failed to probe file format - may be corrupted or unsupported")?;

    let format_name = probed.format.metadata()
        .current()
        .map(|m| format!("{:?}", m))
        .unwrap_or_else(|| "Unknown".to_string());
    
    let track = probed.format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("No supported audio track found in file")?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate
        .context("File does not specify sample rate")?;
    
    let channels = track.codec_params.channels
        .map(|c| c.count())
        .unwrap_or(2);
    
    if channels == 0 {
        bail!("File reports 0 audio channels");
    }

    let (claimed_bit_depth, bit_depth_inferred) = 
        if let Some(bps) = track.codec_params.bits_per_sample {
            (bps, false)
        } else if let Some(bps) = track.codec_params.bits_per_coded_sample {
            (bps, true)
        } else {
            // Infer from file extension as fallback
            let inferred = infer_bit_depth_from_extension(path);
            (inferred, true)
        };

    let codec_name = format!("{:?}", track.codec_params.codec);

    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .context("Failed to create decoder for audio codec")?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match probed.format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(ref e)) 
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };

        if sample_buf.is_none() {
            let spec = *decoded.spec();
            let duration = decoded.capacity() as u64;
            sample_buf = Some(SampleBuffer::new(duration, spec));
        }

        if let Some(ref mut buf) = sample_buf {
            buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(buf.samples());
        }
    }

    if samples.is_empty() {
        bail!("No audio samples decoded from file");
    }

    let duration_secs = samples.len() as f64 / (sample_rate as f64 * channels as f64);

    Ok(AudioData {
        samples,
        sample_rate,
        channels,
        claimed_bit_depth,
        bit_depth_inferred,
        duration_secs,
        codec_name,
        format_name,
    })
}

/// Infer bit depth from file extension
fn infer_bit_depth_from_extension(path: &Path) -> u32 {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
        // Lossless formats - typically 16 or 24 bit
        Some("flac") => 24,
        Some("wav") => 24,
        Some("aiff") | Some("aif") => 24,
        Some("alac") | Some("m4a") => 24,
        Some("ape") => 24,
        Some("wv") => 24,
        
        // Lossy formats - effectively 16-bit or less precision
        Some("mp3") => 16,
        Some("aac") => 16,
        Some("ogg") => 16,
        Some("opus") => 16,
        Some("wma") => 16,
        
        // Default assumption
        _ => 16,
    }
}

/// Extract mono samples from potentially multi-channel audio
pub fn extract_mono(audio: &AudioData) -> Vec<f32> {
    if audio.channels == 1 {
        return audio.samples.clone();
    }
    
    let num_samples = audio.samples.len() / audio.channels;
    let mut mono = Vec::with_capacity(num_samples);
    
    for i in 0..num_samples {
        let mut sum = 0.0f32;
        for ch in 0..audio.channels {
            sum += audio.samples[i * audio.channels + ch];
        }
        mono.push(sum / audio.channels as f32);
    }
    
    mono
}

/// Extract stereo pair (left, right)
pub fn extract_stereo(audio: &AudioData) -> Option<(Vec<f32>, Vec<f32>)> {
    if audio.channels < 2 {
        return None;
    }
    
    let num_samples = audio.samples.len() / audio.channels;
    let mut left = Vec::with_capacity(num_samples);
    let mut right = Vec::with_capacity(num_samples);
    
    for i in 0..num_samples {
        left.push(audio.samples[i * audio.channels]);
        right.push(audio.samples[i * audio.channels + 1]);
    }
    
    Some((left, right))
}

/// Compute mid-side from stereo
pub fn compute_mid_side(left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mid: Vec<f32> = left.iter()
        .zip(right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    
    let side: Vec<f32> = left.iter()
        .zip(right.iter())
        .map(|(l, r)| (l - r) * 0.5)
        .collect();
    
    (mid, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mono() {
        let audio = AudioData {
            samples: vec![0.5, -0.5, 0.3, -0.3],
            sample_rate: 44100,
            channels: 2,
            claimed_bit_depth: 16,
            bit_depth_inferred: false,
            duration_secs: 0.0,
            codec_name: "Test".to_string(),
            format_name: "Test".to_string(),
        };
        
        let mono = extract_mono(&audio);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.0).abs() < 0.001);
        assert!((mono[1] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_mid_side() {
        let left = vec![1.0, 0.5];
        let right = vec![1.0, -0.5];
        
        let (mid, side) = compute_mid_side(&left, &right);
        
        assert!((mid[0] - 1.0).abs() < 0.001);
        assert!((side[0] - 0.0).abs() < 0.001);
        assert!((mid[1] - 0.0).abs() < 0.001);
        assert!((side[1] - 0.5).abs() < 0.001);
    }
    
    #[test]
    fn test_infer_bit_depth() {
        use std::path::PathBuf;
        
        assert_eq!(infer_bit_depth_from_extension(&PathBuf::from("test.flac")), 24);
        assert_eq!(infer_bit_depth_from_extension(&PathBuf::from("test.mp3")), 16);
        assert_eq!(infer_bit_depth_from_extension(&PathBuf::from("test.wav")), 24);
        assert_eq!(infer_bit_depth_from_extension(&PathBuf::from("test.ogg")), 16);
    }
}
