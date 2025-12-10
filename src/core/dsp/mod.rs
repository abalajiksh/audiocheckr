//! Digital Signal Processing utilities
//!
//! Core DSP functions used throughout AudioCheckr:
//! - FFT processing with various window functions
//! - Spectral analysis utilities
//! - Signal filtering and resampling
//! - Statistical functions

mod fft;
mod windows;
mod filters;
mod stats;

// Re-export all DSP functionality
pub use fft::{FftProcessor};
pub use windows::{WindowType, create_window};
pub use filters::{pre_emphasis, de_emphasis, upsample_sinc, downsample_simple};
pub use stats::{
    moving_average, median, rms, peak_amplitude,
    amplitude_to_db, db_to_amplitude,
    compute_envelope, find_transients,
    zero_crossing_rate, autocorrelation,
    spectral_centroid, spectral_spread, spectral_flatness,
    spectral_rolloff, spectral_flux, spectral_contrast,
};
