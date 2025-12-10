// src/core/visualization/spectrogram.rs
//
// Spectrogram generation for visual audio analysis

use anyhow::Result;
use image::{ImageBuffer, Rgb};
use std::path::Path;

/// Spectrogram configuration
#[derive(Debug, Clone)]
pub struct SpectrogramConfig {
    pub width: u32,
    pub height: u32,
    pub fft_size: usize,
    pub hop_size: usize,
    pub min_db: f32,
    pub max_db: f32,
    pub max_seconds: Option<f32>,
}

impl Default for SpectrogramConfig {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 400,
            fft_size: 4096,
            hop_size: 1024,
            min_db: -90.0,
            max_db: 0.0,
            max_seconds: Some(15.0),
        }
    }
}

/// Color map for spectrogram
#[derive(Debug, Clone, Copy)]
pub enum Colormap {
    Viridis,
    Magma,
    Inferno,
    Grayscale,
}

impl Default for Colormap {
    fn default() -> Self {
        Self::Viridis
    }
}

/// Generate mel-scale spectrogram
pub fn generate_mel_spectrogram(
    samples: &[f32],
    sample_rate: u32,
    config: &SpectrogramConfig,
    output_path: &Path,
) -> Result<()> {
    generate_spectrogram_internal(samples, sample_rate, config, output_path, true)
}

/// Generate linear-scale spectrogram
pub fn generate_linear_spectrogram(
    samples: &[f32],
    sample_rate: u32,
    config: &SpectrogramConfig,
    output_path: &Path,
) -> Result<()> {
    generate_spectrogram_internal(samples, sample_rate, config, output_path, false)
}

/// Generate spectrogram image
pub fn generate_spectrogram_image(
    samples: &[f32],
    sample_rate: u32,
    config: &SpectrogramConfig,
    output_path: &Path,
    mel_scale: bool,
) -> Result<()> {
    generate_spectrogram_internal(samples, sample_rate, config, output_path, mel_scale)
}

fn generate_spectrogram_internal(
    samples: &[f32],
    sample_rate: u32,
    config: &SpectrogramConfig,
    output_path: &Path,
    mel_scale: bool,
) -> Result<()> {
    use rustfft::{FftPlanner, num_complex::Complex};
    use std::f32::consts::PI;
    
    // Limit samples if max_seconds is set
    let max_samples = config.max_seconds
        .map(|s| (s * sample_rate as f32) as usize)
        .unwrap_or(samples.len());
    let samples = &samples[..samples.len().min(max_samples)];
    
    // Calculate number of frames
    let num_frames = (samples.len().saturating_sub(config.fft_size)) / config.hop_size + 1;
    if num_frames == 0 {
        anyhow::bail!("Audio too short for spectrogram generation");
    }
    
    // Create FFT planner
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(config.fft_size);
    
    // Create window
    let window: Vec<f32> = (0..config.fft_size)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / config.fft_size as f32).cos()))
        .collect();
    
    // Compute spectrogram
    let freq_bins = config.fft_size / 2;
    let mut spectrogram = vec![vec![0.0f32; num_frames]; freq_bins];
    
    for frame in 0..num_frames {
        let start = frame * config.hop_size;
        let end = (start + config.fft_size).min(samples.len());
        
        // Apply window and FFT
        let mut buffer: Vec<Complex<f32>> = (0..config.fft_size)
            .map(|i| {
                let sample = if start + i < end { samples[start + i] } else { 0.0 };
                Complex::new(sample * window[i], 0.0)
            })
            .collect();
        
        fft.process(&mut buffer);
        
        // Convert to magnitude in dB
        for (bin, complex) in buffer.iter().take(freq_bins).enumerate() {
            let magnitude = (complex.re * complex.re + complex.im * complex.im).sqrt();
            let db = if magnitude > 1e-10 {
                20.0 * magnitude.log10()
            } else {
                config.min_db
            };
            spectrogram[bin][frame] = db.clamp(config.min_db, config.max_db);
        }
    }
    
    // Create image
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = 
        ImageBuffer::new(config.width, config.height);
    
    let x_scale = num_frames as f32 / config.width as f32;
    let y_scale = freq_bins as f32 / config.height as f32;
    
    for y in 0..config.height {
        for x in 0..config.width {
            let frame_idx = ((x as f32 * x_scale) as usize).min(num_frames - 1);
            
            // Flip Y for display (low frequencies at bottom)
            let bin_idx = if mel_scale {
                // Mel scale mapping
                let mel_y = (config.height - 1 - y) as f32 / config.height as f32;
                let mel = mel_y * freq_to_mel(sample_rate as f32 / 2.0);
                let freq = mel_to_freq(mel);
                let bin = (freq / sample_rate as f32 * config.fft_size as f32) as usize;
                bin.min(freq_bins - 1)
            } else {
                // Linear scale
                ((config.height - 1 - y) as f32 * y_scale) as usize
            };
            
            let db = spectrogram[bin_idx.min(freq_bins - 1)][frame_idx];
            let normalized = (db - config.min_db) / (config.max_db - config.min_db);
            let color = db_to_color(normalized);
            
            img.put_pixel(x, y, color);
        }
    }
    
    img.save(output_path)?;
    Ok(())
}

fn freq_to_mel(freq: f32) -> f32 {
    2595.0 * (1.0 + freq / 700.0).log10()
}

fn mel_to_freq(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

fn db_to_color(value: f32) -> Rgb<u8> {
    // Viridis-like colormap
    let v = value.clamp(0.0, 1.0);
    
    let r = (68.0 + v * (235.0 - 68.0)) as u8;
    let g = (1.0 + v * (237.0 - 1.0)) as u8;
    let b = (84.0 + v * (32.0 - 84.0 + (1.0 - v) * 150.0)) as u8;
    
    Rgb([r, g, b])
}
