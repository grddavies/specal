// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Gethin Davies
// Derived in part from Spek <https://github.com/alexkay/spek>

//! Short-time FFT over the whole file, reduced to a fixed number of columns.
//!
//! The file is split into `columns` equal intervals. Each interval gets one FFT
//! per full window it contains (or one FFT of the most recent samples if it's
//! shorter than a window), and the dB results are averaged.

use std::f32::consts::PI;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::audio::{AudioInfo, Decoder};
use crate::{DbRange, Error, MAX_FFT_BITS, MIN_FFT_BITS, Palette, Rgba, Settings};

/// Power floor, so silence gives a finite value (-200 dB) instead of -inf.
const MIN_POWER: f32 = 1e-20;

/// Column bounds for [`Columns::Auto`]. The minimum keeps short files smooth;
/// the maximum bounds memory (8192 × 1025 bands × 4 bytes ≈ 34 MB) and stays
/// within common GPU texture limits.
pub const MIN_AUTO_COLUMNS: usize = 4096;
pub const MAX_AUTO_COLUMNS: usize = 8192;

/// How many time columns to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Columns {
  /// Exactly this many, e.g. one per output pixel.
  Exact(usize),
  /// One per FFT window, within [`MIN_AUTO_COLUMNS`]..=[`MAX_AUTO_COLUMNS`].
  /// Analyse once at this resolution, then [`Spectrogram::rebin`] to fit the
  /// display as it resizes.
  Auto,
}

impl Columns {
  fn resolve(self, frames: u64, fft_bits: u32) -> usize {
    match self {
      Self::Exact(n) => n,
      Self::Auto => {
        let windows = (frames >> fft_bits) as usize;
        windows.clamp(MIN_AUTO_COLUMNS, MAX_AUTO_COLUMNS).min(frames as usize)
      }
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowFunction {
  #[default]
  Hann,
  Hamming,
  BlackmanHarris,
}

impl WindowFunction {
  pub const ALL: [WindowFunction; 3] = [Self::Hann, Self::Hamming, Self::BlackmanHarris];

  fn coefficients(self, n: usize) -> Vec<f32> {
    let step = 2.0 * PI / (n as f32 - 1.0);
    (0..n)
      .map(|i| {
        let x = step * i as f32;
        match self {
          Self::Hann => 0.5 * (1.0 - x.cos()),
          Self::Hamming => 0.53836 - 0.46164 * x.cos(),
          Self::BlackmanHarris => {
            0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
          }
        }
      })
      .collect()
  }
}

impl fmt::Display for WindowFunction {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Hann => "Hann",
      Self::Hamming => "Hamming",
      Self::BlackmanHarris => "Blackman–Harris",
    })
  }
}

/// A grid of dB values: `columns` time slices by `bands` frequency bins.
#[derive(Debug, Clone)]
pub struct Spectrogram {
  pub columns: usize,
  /// Number of frequency bins, from 0 Hz up to the Nyquist frequency inclusive.
  pub bands: usize,
  /// Column-major: `data[column * bands + band]`, band 0 is 0 Hz.
  data: Vec<f32>,
}

impl Spectrogram {
  pub fn column(&self, column: usize) -> &[f32] {
    &self.data[column * self.bands..(column + 1) * self.bands]
  }

  /// Average down to `columns` columns, each the mean of the source columns it
  /// covers. Returns an unchanged copy if `columns` isn't smaller; let the
  /// display stretch it instead.
  pub fn rebin(&self, columns: usize) -> Spectrogram {
    let columns = columns.max(1);
    if columns >= self.columns {
      return self.clone();
    }
    let mut data = Vec::with_capacity(columns * self.bands);
    let mut sum = vec![0.0f32; self.bands];
    for x in 0..columns {
      let start = x * self.columns / columns;
      let end = (x + 1) * self.columns / columns;
      sum.fill(0.0);
      for src in start..end {
        for (s, &v) in sum.iter_mut().zip(self.column(src)) {
          *s += v;
        }
      }
      let count = (end - start) as f32;
      data.extend(sum.iter().map(|s| s / count));
    }
    Spectrogram { columns, bands: self.bands, data }
  }

  /// Render to an image `columns` wide and `bands` tall, highest frequency at
  /// the top.
  pub fn to_rgba(&self, palette: Palette, range: DbRange) -> Rgba {
    let (w, h) = (self.columns, self.bands);
    // The palette maths is too slow to run per pixel, so precompute a lookup table.
    // 1024 steps is finer than the 256 levels per channel of 8-bit colour.
    const STEPS: usize = 1024;
    // bytes per pixel, RGBA
    const BPP: usize = 4;
    let lut: Vec<[u8; BPP]> = (0..STEPS)
      .map(|i| {
        let [r, g, b] = palette.color(i as f32 / (STEPS - 1) as f32);
        [r, g, b, 255]
      })
      .collect();
    let scale = (STEPS - 1) as f32 / (range.upper - range.lower);

    let mut pixels = vec![0u8; w * h * BPP];
    for x in 0..w {
      for (band, &db) in self.column(x).iter().enumerate() {
        let step = ((db - range.lower) * scale).clamp(0.0, (STEPS - 1) as f32) as usize;
        let i = ((h - 1 - band) * w + x) * BPP;
        pixels[i..i + BPP].copy_from_slice(&lut[step]);
      }
    }
    Rgba { width: w as u32, height: h as u32, pixels }
  }
}

/// The result of analysing a file.
#[derive(Debug, Clone)]
pub struct Analysis {
  pub info: AudioInfo,
  pub settings: Settings,
  pub spectrogram: Spectrogram,
}

impl Analysis {
  pub fn description(&self) -> String {
    self.info.description(&self.settings)
  }
}

/// Decode `path` and compute its spectrogram.
///
/// This blocks until the whole file has been decoded; run it off the UI thread.
pub fn analyse(path: &Path, settings: &Settings, columns: Columns) -> Result<Analysis, Error> {
  analyse_cancellable(path, settings, columns, &AtomicBool::new(false))
}

/// Like [`analyse`], but returns [`Error::Cancelled`] soon after `cancel` is
/// set.
pub fn analyse_cancellable(
  path: &Path,
  settings: &Settings,
  columns: Columns,
  cancel: &AtomicBool,
) -> Result<Analysis, Error> {
  if !(MIN_FFT_BITS..=MAX_FFT_BITS).contains(&settings.fft_bits) {
    return Err(Error::FftSize);
  }
  if columns == Columns::Exact(0) {
    return Err(Error::NoColumns);
  }

  let mut decoder = Decoder::open(path, settings.channel)?;
  let columns = columns.resolve(decoder.info.frames, settings.fft_bits);
  let mut acc = Accumulator::new(settings, columns, decoder.info.frames);
  while let Some(samples) = decoder.next_chunk()? {
    if cancel.load(Ordering::Relaxed) {
      return Err(Error::Cancelled);
    }
    acc.push(samples);
    if acc.is_full() {
      break;
    }
  }

  Ok(Analysis { info: decoder.info, settings: settings.clone(), spectrogram: acc.finish() })
}

struct Accumulator {
  n: usize,
  bands: usize,
  fft: Arc<dyn RealToComplex<f32>>,
  window: Vec<f32>,
  fft_in: Vec<f32>,
  fft_out: Vec<Complex32>,
  scratch: Vec<Complex32>,
  /// The last `n` samples, oldest at `head`.
  history: Vec<f32>,
  head: usize,
  /// Samples seen so far.
  pos: u64,
  total: u64,
  columns: usize,
  column: usize,
  column_start: u64,
  column_end: u64,
  sum: Vec<f32>,
  ffts_in_column: u32,
  data: Vec<f32>,
}

impl Accumulator {
  fn new(settings: &Settings, columns: usize, total: u64) -> Self {
    let n = 1usize << settings.fft_bits;
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
    let bands = n / 2 + 1;
    let mut acc = Self {
      n,
      bands,
      fft_in: fft.make_input_vec(),
      fft_out: fft.make_output_vec(),
      scratch: fft.make_scratch_vec(),
      fft,
      window: settings.window.coefficients(n),
      history: vec![0.0; n],
      head: 0,
      pos: 0,
      total,
      columns,
      column: 0,
      column_start: 0,
      column_end: 0,
      sum: vec![0.0; bands],
      ffts_in_column: 0,
      data: Vec::with_capacity(columns * bands),
    };
    acc.column_end = acc.end_of(0);
    // Columns shorter than one sample (more columns than samples) are empty
    // before any audio arrives.
    acc.finish_empty_columns();
    acc
  }

  fn end_of(&self, column: usize) -> u64 {
    ((column as u128 + 1) * self.total as u128 / self.columns as u128) as u64
  }

  fn is_full(&self) -> bool {
    self.column >= self.columns
  }

  fn push(&mut self, samples: &[f32]) {
    for &s in samples {
      if self.is_full() {
        return;
      }
      self.history[self.head] = s;
      self.head = (self.head + 1) % self.n;
      self.pos += 1;

      if (self.pos - self.column_start).is_multiple_of(self.n as u64) {
        self.run_fft();
      }
      if self.pos == self.column_end {
        self.finish_column();
        self.finish_empty_columns();
      }
    }
  }

  fn run_fft(&mut self) {
    for i in 0..self.n {
      self.fft_in[i] = self.history[(self.head + i) % self.n] * self.window[i];
    }
    self
      .fft
      .process_with_scratch(&mut self.fft_in, &mut self.fft_out, &mut self.scratch)
      .expect("FFT buffers are sized by the planner");
    let norm = (self.n * self.n) as f32;
    for (sum, c) in self.sum.iter_mut().zip(&self.fft_out) {
      *sum += 10.0 * (c.norm_sqr() / norm).max(MIN_POWER).log10();
    }
    self.ffts_in_column += 1;
  }

  fn finish_column(&mut self) {
    if self.ffts_in_column == 0 {
      self.run_fft();
    }
    let count = self.ffts_in_column as f32;
    self.data.extend(self.sum.iter().map(|s| s / count));
    self.sum.fill(0.0);
    self.ffts_in_column = 0;
    self.column += 1;
    self.column_start = self.pos;
    self.column_end = self.end_of(self.column);
  }

  fn finish_empty_columns(&mut self) {
    while !self.is_full() && self.column_end <= self.pos {
      self.finish_column();
    }
  }

  fn finish(mut self) -> Spectrogram {
    // The decoder can deliver fewer samples than the container promised.
    // Close the partial column, then pad the rest with silence.
    if !self.is_full() && self.pos > self.column_start {
      self.finish_column();
    }
    let floor = 10.0 * MIN_POWER.log10();
    self.data.resize(self.columns * self.bands, floor);
    Spectrogram { columns: self.columns, bands: self.bands, data: self.data }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(samples: &[f32], columns: usize, fft_bits: u32) -> Spectrogram {
    let settings = Settings { fft_bits, ..Settings::default() };
    let mut acc = Accumulator::new(&settings, columns, samples.len() as u64);
    acc.push(samples);
    acc.finish()
  }

  #[test]
  fn sine_peaks_in_the_right_band() {
    let (rate, freq, n) = (48_000.0, 6_000.0, 1024);
    let samples: Vec<f32> =
      (0..48_000).map(|i| (2.0 * PI * freq * i as f32 / rate).sin()).collect();
    let spec = run(&samples, 10, 10);
    assert_eq!(spec.bands, n / 2 + 1);
    let expected_band = (freq / rate * n as f32).round() as usize;
    for x in 0..spec.columns {
      let col = spec.column(x);
      let peak = (0..col.len()).max_by(|&a, &b| col[a].total_cmp(&col[b])).unwrap();
      assert_eq!(peak, expected_band, "column {x}");
    }
  }

  #[test]
  fn rebin_averages_covered_columns() {
    let spec =
      Spectrogram { columns: 4, bands: 2, data: vec![0.0, 10.0, 2.0, 20.0, 4.0, 30.0, 6.0, 40.0] };
    let half = spec.rebin(2);
    assert_eq!(half.columns, 2);
    assert_eq!(half.data, [1.0, 15.0, 5.0, 35.0]);
    // Uneven: 4 → 3 covers [0], [1], [2, 3].
    assert_eq!(spec.rebin(3).data, [0.0, 10.0, 2.0, 20.0, 5.0, 35.0]);
    // Growing is left to the display.
    assert_eq!(spec.rebin(10).columns, 4);
  }

  #[test]
  fn auto_columns() {
    // 4 minutes at 44.1 kHz with 2048-sample windows: one column per window.
    assert_eq!(Columns::Auto.resolve(240 * 44_100, 11), 240 * 44_100 / 2048);
    // Short files get the minimum, long ones the maximum.
    assert_eq!(Columns::Auto.resolve(44_100, 11), MIN_AUTO_COLUMNS);
    assert_eq!(Columns::Auto.resolve(3600 * 44_100, 11), MAX_AUTO_COLUMNS);
    // Never more columns than samples.
    assert_eq!(Columns::Auto.resolve(100, 11), 100);
  }

  #[test]
  fn more_columns_than_samples() {
    let spec = run(&[0.5; 10], 40, 8);
    assert_eq!(spec.columns, 40);
    assert_eq!(spec.data.len(), 40 * spec.bands);
  }

  #[test]
  fn short_decode_is_padded() {
    let settings = Settings { fft_bits: 8, ..Settings::default() };
    let mut acc = Accumulator::new(&settings, 4, 4000);
    acc.push(&[0.1; 1500]);
    let spec = acc.finish();
    assert_eq!(spec.data.len(), 4 * spec.bands);
  }
}
