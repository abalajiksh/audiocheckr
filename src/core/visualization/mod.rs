//! Visualization utilities

pub mod spectrogram;

pub use spectrogram::{
    generate_mel_spectrogram, generate_linear_spectrogram, generate_spectrogram_image,
    SpectrogramConfig, Colormap,
};
