//! Short-time FFT over the whole file, reduced to a fixed number of columns.
//!
//! Like Spek, the file is split into `columns` equal intervals. Each interval
//! gets one FFT per full window it contains (or one FFT of the most recent
//! samples if it's shorter than a window), and the dB results are averaged.

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
          Self::BlackmanHarris => 0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos(),
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

  /// Render to an image `columns` wide and `bands` tall, highest frequency at the top.
  pub fn to_rgba(&self, palette: Palette, range: DbRange) -> Rgba {
    let (w, h) = (self.columns, self.bands);
    let span = range.upper - range.lower;
    let mut pixels = vec![0u8; w * h * 4];
    for x in 0..w {
      for (band, &db) in self.column(x).iter().enumerate() {
        let level = ((db - range.lower) / span).clamp(0.0, 1.0);
        let [r, g, b] = palette.color(level);
        let i = ((h - 1 - band) * w + x) * 4;
        pixels[i..i + 4].copy_from_slice(&[r, g, b, 255]);
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

/// Decode `path` and compute a spectrogram `columns` wide.
///
/// This blocks until the whole file has been decoded; run it off the UI thread.
pub fn analyse(path: &Path, settings: &Settings, columns: usize) -> Result<Analysis, Error> {
  analyse_cancellable(path, settings, columns, &AtomicBool::new(false))
}

/// Like [`analyse`], but returns [`Error::Cancelled`] soon after `cancel` is set.
pub fn analyse_cancellable(
  path: &Path,
  settings: &Settings,
  columns: usize,
  cancel: &AtomicBool,
) -> Result<Analysis, Error> {
  if !(MIN_FFT_BITS..=MAX_FFT_BITS).contains(&settings.fft_bits) {
    return Err(Error::FftSize);
  }
  if columns == 0 {
    return Err(Error::NoColumns);
  }

  let mut decoder = Decoder::open(path, settings.channel)?;
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
    let samples: Vec<f32> = (0..48_000).map(|i| (2.0 * PI * freq * i as f32 / rate).sin()).collect();
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
