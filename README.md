# Audio Quality Checker

Detect fake lossless files, transcodes, and upsampled audio with spectral analysis.

## Features
- Detects MP3, Ogg Vorbis, AAC/M4A, and Opus transcodes
- Identifies bit depth mismatches (16-bit vs 24-bit)
- Detects upsampling (44.1→96kHz, 96→192kHz, etc.)
- Generates spectrograms for visual inspection
- Batch processing of entire directories

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# You're in /home/user/music/jazz/
cd ~/music/jazz/

# Analyze all files in current directory, output spectrograms here
audio-quality-checker -s -o current

# Analyze specific file, spectrogram in same folder as audio
audio-quality-checker -i "Miles Davis - So What.flac" -s

# Analyze entire directory tree, spectrograms next to source files
audio-quality-checker -i . -s -o source

# Quick check without moving from your music directory
audio-quality-checker -i album/ -b 24 -u -v

# Custom output folder
audio-quality-checker -i . -s -o ~/Desktop/analysis/
```