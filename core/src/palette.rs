// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Gethin Davies
// Derived in part from Spek <https://github.com/alexkay/spek>

//! Colour maps from a normalised level (0..=1) to RGB

use std::f64::consts::PI;
use std::fmt;

use crate::Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Palette {
  /// Rainbow palette, a modified version of Dan Bruton's algorithm.
  Spectrum,
  /// The default SoX palette, by Rob Sykes.
  #[default]
  Sox,
  /// Greyscale.
  Mono,
  /// Specal colour scheme
  Specal,
}

impl Palette {
  pub const ALL: [Palette; 4] = [Self::Spectrum, Self::Sox, Self::Mono, Self::Specal];

  pub fn color(self, level: f32) -> [u8; 3] {
    let level = level.clamp(0.0, 1.0) as f64;
    let (r, g, b) = match self {
      Self::Spectrum => spectrum(level),
      Self::Sox => sox(level),
      Self::Mono => (level, level, level),
      Self::Specal => gradient(&SPECAL_STOPS, level),
    };
    [r, g, b].map(|c| (c * 255.0 + 0.5) as u8)
  }

  /// A vertical colour bar `height` pixels tall, highest level at the top.
  pub fn bar(self, height: u32) -> Rgba {
    let h = height.max(1);
    let pixels = (0..h)
      .flat_map(|y| {
        let [r, g, b] = self.color(1.0 - y as f32 / (h - 1).max(1) as f32);
        [r, g, b, 255]
      })
      .collect();
    Rgba { width: 1, height: h, pixels }
  }
}

impl fmt::Display for Palette {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Spectrum => "Spectrum",
      Self::Sox => "SoX",
      Self::Mono => "Mono",
      Self::Specal => "Specal",
    })
  }
}

/// Colour stops for the Specal palette as (level, rgb)
const SPECAL_STOPS: [(f64, [u8; 3]); 6] = [
  (0.00, [0x00, 0x00, 0x00]), // black #000000
  (0.15, [0x0D, 0x18, 0x21]), // ink   #0D1821
  (0.35, [0x71, 0x61, 0xEF]), // blue  #7161EF
  (0.50, [0xF7, 0x86, 0xAA]), // pink  #F786AA
  (0.70, [0xFB, 0xCA, 0xDA]), // pale  #FBCADA
  (1.00, [0xF0, 0xF4, 0xEF]), // porc  #F0F4EF
];

/// Linearly interpolate between `stops` (level, rgb) for a level in 0..=1.
fn gradient(stops: &[(f64, [u8; 3])], level: f64) -> (f64, f64, f64) {
  let level = level.clamp(0.0, 1.0);
  // First stop above `level`; the segment is the pair around it.
  let hi = stops.iter().position(|&(p, _)| p > level).unwrap_or(stops.len() - 1).max(1);
  let (p0, c0) = stops[hi - 1];
  let (p1, c1) = stops[hi];
  let t = ((level - p0) / (p1 - p0)).clamp(0.0, 1.0);
  let [r, g, b] =
    std::array::from_fn(|c| (c0[c] as f64 + (c1[c] as f64 - c0[c] as f64) * t) / 255.0);
  (r, g, b)
}

fn spectrum(level: f64) -> (f64, f64, f64) {
  let level = level * 0.6625;
  let (r, g, b) = if level < 0.15 {
    ((0.15 - level) / (0.15 + 0.075), 0.0, 1.0)
  } else if level < 0.275 {
    (0.0, (level - 0.15) / (0.275 - 0.15), 1.0)
  } else if level < 0.325 {
    (0.0, 1.0, (0.325 - level) / (0.325 - 0.275))
  } else if level < 0.5 {
    ((level - 0.325) / (0.5 - 0.325), 1.0, 0.0)
  } else if level < 0.6625 {
    (1.0, (0.6625 - level) / (0.6625 - 0.5), 0.0)
  } else {
    (0.0, 0.0, 0.0)
  };
  // Intensity correction: fade to black at the bottom of the range.
  let cf = if level < 0.1 { level / 0.1 } else { 1.0 };
  (r * cf, g * cf, b * cf)
}

fn sox(level: f64) -> (f64, f64, f64) {
  let r = if level >= 0.73 {
    1.0
  } else if level >= 0.13 {
    ((level - 0.13) / 0.60 * PI / 2.0).sin()
  } else {
    0.0
  };
  let g = if level >= 0.91 {
    1.0
  } else if level >= 0.6 {
    ((level - 0.6) / 0.31 * PI / 2.0).sin()
  } else {
    0.0
  };
  let b = if level < 0.6 {
    0.5 * (level / 0.6 * PI).sin()
  } else if level >= 0.78 {
    (level - 0.78) / 0.22
  } else {
    0.0
  };
  (r, g, b)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn endpoints() {
    assert_eq!(Palette::Sox.color(0.0), [0, 0, 0]);
    assert_eq!(Palette::Sox.color(1.0), [255, 255, 255]);
    assert_eq!(Palette::Mono.color(1.0), [255, 255, 255]);
    assert_eq!(Palette::Spectrum.color(0.0), [0, 0, 0]);
    assert_eq!(Palette::Specal.color(0.0), [0x0D, 0x18, 0x21]);
    assert_eq!(Palette::Specal.color(0.30), [0x71, 0x61, 0xEF]);
    assert_eq!(Palette::Specal.color(1.0), [0xF0, 0xF4, 0xEF]);
  }
}
