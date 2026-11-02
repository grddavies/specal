// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Gethin Davies
// Derived in part from Spek <https://github.com/alexkay/spek>

//! Page layout, shared by every front end so they agree on where the
//! spectrogram, header and colour bar go. Units are logical pixels.

/// Space around the spectrogram
pub const LEFT: f32 = 60.0;
pub const TOP: f32 = 60.0;
pub const RIGHT: f32 = 90.0;
pub const BOTTOM: f32 = 40.0;
/// Gap between the plot and labels.
pub const GAP: f32 = 10.0;
/// Width of the colour bar.
pub const BAR: f32 = 10.0;
/// Length of an axis tick mark.
pub const TICK: f32 = 4.0;

/// Font sizes in pixels.
pub const TITLE_SIZE: f32 = 13.0;
pub const BODY_SIZE: f32 = 12.0;
pub const LABEL_SIZE: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
}

impl Rect {
  pub fn right(&self) -> f32 {
    self.x + self.width
  }

  pub fn bottom(&self) -> f32 {
    self.y + self.height
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
  /// Where the spectrogram image goes.
  pub plot: Rect,
  /// Where the palette bar goes, right of the plot.
  pub bar: Rect,
  /// Top-left corners of the two header lines (file name, then description).
  pub title: (f32, f32),
  pub description: (f32, f32),
  /// Top-left corner of the app name, above the bar.
  pub app_name: (f32, f32),
}

impl Layout {
  pub fn new(width: f32, height: f32) -> Self {
    let plot = Rect {
      x: LEFT,
      y: TOP,
      width: (width - LEFT - RIGHT).max(0.0),
      height: (height - TOP - BOTTOM).max(0.0),
    };
    let bar = Rect { x: plot.right() + GAP, y: TOP, width: BAR, height: plot.height };
    let description_y = TOP - GAP - BODY_SIZE * 1.2;
    let title_y = description_y - GAP - TITLE_SIZE * 1.2;
    Self {
      plot,
      bar,
      title: (LEFT, title_y),
      description: (LEFT, description_y),
      app_name: (bar.x, title_y),
    }
  }

  /// How many spectrogram columns fit the plot at 1:1.
  pub fn columns(&self) -> usize {
    self.plot.width.floor() as usize
  }
}
