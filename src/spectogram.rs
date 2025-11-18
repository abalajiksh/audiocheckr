// src/spectrogram.rs
use anyhow::Result;
use image::{ImageBuffer, Rgb};
use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;
use crate::decoder::AudioData;

pub fn generate_spectrogram_image(audio: &AudioData, output_path: &Path) -> Result<()> {
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

    // Normalize and convert to log scale
    let max_val = spectrogram.iter()
        .flat_map(|row| row.iter())
        .cloned()
        .fold(0.0f32, f32::max);

    for row in spectrogram.iter_mut() {
        for val in row.iter_mut() {
            *val = if *val > 0.0 {
                20.0 * (*val / max_val).log10().max(-80.0) / 80.0 + 1.0
            } else {
                0.0
            };
        }
    }

    // Create image (flip vertically so low frequencies are at bottom)
    let img_width = num_frames as u32;
    let img_height = num_bins as u32;
    let mut img = ImageBuffer::new(img_width, img_height);

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let bin = (img_height - 1 - y) as usize;
        let frame = x as usize;
        let value = spectrogram[bin][frame];
        let color = value_to_color(value);
        *pixel = Rgb([color.0, color.1, color.2]);
    }

    img.save(output_path)?;
    Ok(())
}

fn value_to_color(value: f32) -> (u8, u8, u8) {
    // Magma color scale (similar to professional audio software)
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
