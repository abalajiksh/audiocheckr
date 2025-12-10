//! Visualization tools for audio analysis
//!
//! Contains spectrogram generation and other visual representations
//! of audio data.

mod spectrogram;

pub use spectrogram::{
    generate_spectrogram_image,
    generate_mel_spectrogram,
    generate_linear_spectrogram,
    SpectrogramConfig,
    Colormap,
};
