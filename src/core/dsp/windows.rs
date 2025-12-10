//! Window function implementations

use std::f32::consts::PI;

/// Window function types
#[derive(Debug, Clone, Copy)]
pub enum WindowType {
    Hann,
    Hamming,
    Blackman,
    BlackmanHarris,
    FlatTop,
    Kaiser(f32),  // Beta parameter
}

/// Create window function
pub fn create_window(size: usize, window_type: WindowType) -> Vec<f32> {
    let n = size as f32;
    (0..size)
        .map(|i| {
            let x = i as f32;
            match window_type {
                WindowType::Hann => {
                    0.5 * (1.0 - (2.0 * PI * x / n).cos())
                }
                WindowType::Hamming => {
                    0.54 - 0.46 * (2.0 * PI * x / n).cos()
                }
                WindowType::Blackman => {
                    0.42 - 0.5 * (2.0 * PI * x / n).cos() 
                        + 0.08 * (4.0 * PI * x / n).cos()
                }
                WindowType::BlackmanHarris => {
                    0.35875 - 0.48829 * (2.0 * PI * x / n).cos()
                        + 0.14128 * (4.0 * PI * x / n).cos()
                        - 0.01168 * (6.0 * PI * x / n).cos()
                }
                WindowType::FlatTop => {
                    // Good for amplitude accuracy
                    0.21557895 - 0.41663158 * (2.0 * PI * x / n).cos()
                        + 0.277263158 * (4.0 * PI * x / n).cos()
                        - 0.083578947 * (6.0 * PI * x / n).cos()
                        + 0.006947368 * (8.0 * PI * x / n).cos()
                }
                WindowType::Kaiser(beta) => {
                    let alpha = (n - 1.0) / 2.0;
                    let ratio = (x - alpha) / alpha;
                    let arg = beta * (1.0 - ratio * ratio).max(0.0).sqrt();
                    bessel_i0(arg) / bessel_i0(beta)
                }
            }
        })
        .collect()
}

/// Modified Bessel function I0 (for Kaiser window)
fn bessel_i0(x: f32) -> f32 {
    let mut sum = 1.0f32;
    let mut term = 1.0f32;
    let x2 = x * x;
    
    for k in 1..50 {
        term *= x2 / (4.0 * k as f32 * k as f32);
        sum += term;
        if term < 1e-10 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hann_window() {
        let window = create_window(4, WindowType::Hann);
        assert!((window[0]).abs() < 0.01);  // Should be ~0 at edges
        assert!((window[2] - 1.0).abs() < 0.01);  // Should be ~1 at center
    }
}
