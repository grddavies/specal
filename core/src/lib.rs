// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Gethin Davies
// Derived in part from Spek <https://github.com/alexkay/spek>

//! specal-core: Audio Spectrum Analysis
//!
//! This crate turns an audio file into a [`Spectrogram`] (a grid of dB values) plus the
//! [`AudioInfo`] needed to label it, and provides the geometry ([`layout`], [`ticks`]) that front
//! ends use to draw axes themselves.
//!
//! The optional `svg` feature adds a standalone SVG exporter.

mod analysis;
mod audio;
pub mod layout;
pub mod palette;
#[cfg(feature = "svg")]
pub mod svg;
pub mod ticks;

use std::path::PathBuf;

pub use analysis::{
  Analysis, Columns, MAX_AUTO_COLUMNS, MIN_AUTO_COLUMNS, Spectrogram, WindowFunction, analyse,
  analyse_cancellable,
};
pub use audio::AudioInfo;
pub use palette::Palette;

/// FFT size limits, as `log2(window length)`.
pub const MIN_FFT_BITS: u32 = 8;
pub const MAX_FFT_BITS: u32 = 14;

/// Analysis settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
  /// Which channel to analyse (0-based).
  pub channel: usize,
  /// `log2` of the FFT window length. 11 gives 2048-sample windows and 1025 bands.
  pub fft_bits: u32,
  pub window: WindowFunction,
}

impl Default for Settings {
  fn default() -> Self {
    Self { channel: 0, fft_bits: 11, window: WindowFunction::Hann }
  }
}

/// How dB values map onto the palette. Values at or below `lower` get the
/// palette's first colour; values at or above `upper` get its last.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DbRange {
  pub lower: f32,
  pub upper: f32,
}

impl Default for DbRange {
  fn default() -> Self {
    Self { lower: -120.0, upper: 0.0 }
  }
}

/// An 8-bit RGBA image, rows top to bottom.
#[derive(Debug, Clone)]
pub struct Rgba {
  pub width: u32,
  pub height: u32,
  pub pixels: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("cannot open {path}: {source}")]
  Open { path: PathBuf, source: std::io::Error },
  #[error("unsupported or unrecognised format: {0}")]
  Format(symphonia::core::errors::Error),
  #[error("the file contains no audio streams")]
  NoAudio,
  #[error("cannot find a decoder for this codec: {0}")]
  NoDecoder(symphonia::core::errors::Error),
  #[error("unknown duration")]
  NoDuration,
  #[error("unknown sample rate")]
  NoSampleRate,
  #[error("channel {requested} requested but the file has {available}")]
  NoChannel { requested: usize, available: usize },
  #[error("decoding failed: {0}")]
  Decode(symphonia::core::errors::Error),
  #[error("FFT size must be between 2^{MIN_FFT_BITS} and 2^{MAX_FFT_BITS}")]
  FftSize,
  #[error("the spectrogram needs at least one column")]
  NoColumns,
  #[error("analysis cancelled")]
  Cancelled,
}
