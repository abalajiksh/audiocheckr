// src/spectrogram.rs

use anyhow::Result;
use image::{ImageBuffer, Rgb, RgbImage};
use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;
use crate::decoder::AudioData;

pub fn generate_spectrogram_image(
    audio: &AudioData, 
    output_path: &Path,
    use_linear_scale: bool
) -> Result<()> {
    let window_size = 2048;
    let hop_size = window_size / 4;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(window_size);

    // Extract mono channel
    let mono: Vec<f32> = audio.samples
        .chunks(audio.channels)
        .map(|chunk| chunk[0])
        .collect();

    let num_frames = (mono.len() - window_size) / hop_size;
    let num_bins = window_size / 2;
    let mut spectrogram = vec![vec![0.0f32; num_frames]; num_bins];

    // Compute FFT
    for frame_idx in 0..num_frames {
        let start = frame_idx * hop_size;
        let end = start + window_size;
        if end > mono.len() {
            break;
        }

        let mut buffer: Vec<Complex<f32>> = mono[start..end]
            .iter()
            .enumerate()
            .map(|(i, &sample)| {
                // Hann window
                let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / window_size as f32).cos());
                Complex::new(sample * window, 0.0)
            })
            .collect();

        fft.process(&mut buffer);

        for (bin_idx, complex) in buffer[..num_bins].iter().enumerate() {
            let magnitude = (complex.re * complex.re + complex.im * complex.im).sqrt();
            spectrogram[bin_idx][frame_idx] = magnitude;
        }
    }

    // Generate mel-scale or linear-scale spectrogram
    if use_linear_scale {
        render_linear_spectrogram(&spectrogram, output_path, audio.sample_rate)
    } else {
        render_mel_spectrogram(&spectrogram, output_path, audio.sample_rate, num_bins)
    }
}

fn render_mel_spectrogram(
    spectrogram: &[Vec<f32>],
    output_path: &Path,
    sample_rate: u32,
    num_bins: usize
) -> Result<()> {
    let num_mel_bins = 128; // Compact mel bins
    let num_frames = spectrogram[0].len();
    
    // Create mel filterbank
    let mel_filters = create_mel_filterbank(num_mel_bins, num_bins, sample_rate);
    
    // Apply mel filterbank
    let mut mel_spec = vec![vec![0.0f32; num_frames]; num_mel_bins];
    for (mel_idx, filter) in mel_filters.iter().enumerate() {
        for frame_idx in 0..num_frames {
            let mut sum = 0.0;
            for (bin_idx, &weight) in filter.iter().enumerate() {
                sum += spectrogram[bin_idx][frame_idx] * weight;
            }
            mel_spec[mel_idx][frame_idx] = sum;
        }
    }

    // Normalize to log scale
    normalize_spectrogram(&mut mel_spec);

    // Render with frequency legend
    let legend_width = 80;
    let img_width = num_frames as u32;
    let img_height = num_mel_bins as u32;
    let total_width = img_width + legend_width;
    
    let mut img = ImageBuffer::new(total_width, img_height);
    
    // Draw spectrogram
    for x in 0..img_width {
        for y in 0..img_height {
            let bin = (img_height - 1 - y) as usize;
            let frame = x as usize;
            let value = mel_spec[bin][frame];
            let color = value_to_color(value);
            img.put_pixel(x, y, Rgb([color.0, color.1, color.2]));
        }
    }
    
    // Draw frequency legend
    draw_mel_legend(&mut img, img_width, img_height, num_mel_bins, sample_rate);
    
    img.save(output_path)?;
    Ok(())
}

fn render_linear_spectrogram(
    spectrogram: &[Vec<f32>],
    output_path: &Path,
    sample_rate: u32
) -> Result<()> {
    let num_bins = spectrogram.len();
    let num_frames = spectrogram[0].len();
    
    let mut spec = spectrogram.to_vec();
    normalize_spectrogram(&mut spec);

    let legend_width = 80;
    let img_width = num_frames as u32;
    let img_height = num_bins as u32;
    let total_width = img_width + legend_width;
    
    let mut img = ImageBuffer::new(total_width, img_height);
    
    // Draw spectrogram
    for x in 0..img_width {
        for y in 0..img_height {
            let bin = (img_height - 1 - y) as usize;
            let frame = x as usize;
            let value = spec[bin][frame];
            let color = value_to_color(value);
            img.put_pixel(x, y, Rgb([color.0, color.1, color.2]));
        }
    }
    
    // Draw frequency legend
    draw_linear_legend(&mut img, img_width, img_height, sample_rate);
    
    img.save(output_path)?;
    Ok(())
}

fn normalize_spectrogram(spec: &mut [Vec<f32>]) {
    let max_val = spec.iter()
        .flat_map(|row| row.iter())
        .cloned()
        .fold(0.0f32, f32::max);

    for row in spec.iter_mut() {
        for val in row.iter_mut() {
            *val = if *val > 0.0 {
                20.0 * (*val / max_val).log10().max(-80.0) / 80.0 + 1.0
            } else {
                0.0
            };
        }
    }
}

fn create_mel_filterbank(num_mel_bins: usize, num_fft_bins: usize, sample_rate: u32) -> Vec<Vec<f32>> {
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sample_rate as f32 / 2.0);
    
    let mel_points: Vec<f32> = (0..=num_mel_bins + 1)
        .map(|i| min_mel + (max_mel - min_mel) * i as f32 / (num_mel_bins + 1) as f32)
        .collect();
    
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();
    let bin_points: Vec<f32> = hz_points.iter()
        .map(|&f| (num_fft_bins as f32 * f / (sample_rate as f32 / 2.0)))
        .collect();
    
    let mut filters = vec![vec![0.0; num_fft_bins]; num_mel_bins];
    
    for i in 0..num_mel_bins {
        let left = bin_points[i];
        let center = bin_points[i + 1];
        let right = bin_points[i + 2];
        
        for j in 0..num_fft_bins {
            let j_f = j as f32;
            if j_f >= left && j_f <= center {
                filters[i][j] = (j_f - left) / (center - left);
            } else if j_f > center && j_f <= right {
                filters[i][j] = (right - j_f) / (right - center);
            }
        }
    }
    
    filters
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0)
}

fn draw_mel_legend(img: &mut RgbImage, spec_width: u32, height: u32, num_bins: usize, sample_rate: u32) {
    let legend_start = spec_width;
    let bg_color = Rgb([240u8, 240u8, 240u8]);
    
    // Fill background
    for x in legend_start..img.width() {
        for y in 0..height {
            img.put_pixel(x, y, bg_color);
        }
    }
    
    // Draw frequency markers
    let frequencies = [100, 500, 1000, 2000, 5000, 10000, 20000];
    
    for &freq in &frequencies {
        if freq as f32 > sample_rate as f32 / 2.0 {
            continue;
        }
        
        let mel = hz_to_mel(freq as f32);
        let max_mel = hz_to_mel(sample_rate as f32 / 2.0);
        let y_pos = height - (mel / max_mel * height as f32) as u32;
        
        if y_pos < height {
            // Draw tick line
            for x in legend_start..legend_start + 10 {
                img.put_pixel(x, y_pos, Rgb([0, 0, 0]));
            }
            
            // Draw label (simplified text rendering)
            draw_text(img, legend_start + 12, y_pos.saturating_sub(3), &format_freq(freq));
        }
    }
}

fn draw_linear_legend(img: &mut RgbImage, spec_width: u32, height: u32, sample_rate: u32) {
    let legend_start = spec_width;
    let bg_color = Rgb([240u8, 240u8, 240u8]);
    
    // Fill background
    for x in legend_start..img.width() {
        for y in 0..height {
            img.put_pixel(x, y, bg_color);
        }
    }
    
    let nyquist = sample_rate / 2;
    let frequencies = [100, 500, 1000, 2000, 5000, 10000, 15000, 20000];
    
    for &freq in &frequencies {
        if freq > nyquist {
            continue;
        }
        
        let y_pos = height - (freq as f32 / nyquist as f32 * height as f32) as u32;
        
        if y_pos < height {
            for x in legend_start..legend_start + 10 {
                img.put_pixel(x, y_pos, Rgb([0, 0, 0]));
            }
            
            draw_text(img, legend_start + 12, y_pos.saturating_sub(3), &format_freq(freq));
        }
    }
}

fn format_freq(freq: u32) -> String {
    if freq >= 1000 {
        format!("{}kHz", freq / 1000)
    } else {
        format!("{}Hz", freq)
    }
}

fn draw_text(img: &mut RgbImage, x: u32, y: u32, text: &str) {
    // Simple 5x7 bitmap font for digits and letters
    let text_color = Rgb([0u8, 0u8, 0u8]);
    
    for (i, ch) in text.chars().enumerate() {
        let offset_x = x + (i as u32 * 6);
        draw_char(img, offset_x, y, ch, text_color);
    }
}

fn draw_char(img: &mut RgbImage, x: u32, y: u32, ch: char, color: Rgb<u8>) {
    // Minimal bitmap font - only essential characters
    let pattern = match ch {
        '0' => &[0x7E, 0x81, 0x81, 0x81, 0x7E],
        '1' => &[0x00, 0x82, 0xFF, 0x80, 0x00],
        '2' => &[0xC2, 0xA1, 0x91, 0x89, 0x86],
        '5' => &[0x8F, 0x89, 0x89, 0x89, 0x71],
        'k' => &[0xFF, 0x10, 0x28, 0x44, 0x00],
        'H' => &[0xFF, 0x08, 0x08, 0x08, 0xFF],
        'z' => &[0x61, 0x51, 0x49, 0x45, 0x43],
        _ => &[0x00, 0x00, 0x00, 0x00, 0x00],
    };
    
    for (row, &bits) in pattern.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << col) != 0 {
                let px = x + col;
                let py = y + row as u32;
                if px < img.width() && py < img.height() {
                    img.put_pixel(px, py, color);
                }
            }
        }
    }
}

fn value_to_color(value: f32) -> (u8, u8, u8) {
    // Magma color scale
    let v = value.clamp(0.0, 1.0);
    if v < 0.25 {
        let t = v / 0.25;
        ((t * 20.0) as u8, 0, (t * 50.0) as u8)
    } else if v < 0.5 {
        let t = (v - 0.25) / 0.25;
        ((20.0 + t * 100.0) as u8, (t * 20.0) as u8, (50.0 + t * 80.0) as u8)
    } else if v < 0.75 {
        let t = (v - 0.5) / 0.25;
        ((120.0 + t * 100.0) as u8, (20.0 + t * 140.0) as u8, (130.0 - t * 100.0) as u8)
    } else {
        let t = (v - 0.75) / 0.25;
        ((220.0 + t * 35.0) as u8, (160.0 + t * 95.0) as u8, (30.0 + t * 60.0) as u8)
    }
}

