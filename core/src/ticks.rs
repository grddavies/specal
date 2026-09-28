//! Where to put axis ticks and what to label them, ported from Spek's ruler.
//!
//! These functions only do the maths; drawing is up to the front end. Each
//! takes the axis length in pixels and the extent of a typical label along the
//! axis (its width for the horizontal time axis, its height for the vertical
//! ones) so that labels don't overlap.

/// One tick on an axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
  /// Position along the axis from 0.0 to 1.0, measured from the left for
  /// horizontal axes and from the **bottom** for vertical ones.
  pub pos: f32,
  pub label: String,
}

/// Representative labels for measuring, as Spek uses.
pub const TIME_SAMPLE_LABEL: &str = "00:00";
pub const FREQUENCY_SAMPLE_LABEL: &str = "00 kHz";
pub const DB_SAMPLE_LABEL: &str = "-00 dB";

/// Time ticks for the horizontal axis, labelled `m:ss`.
pub fn time(duration_secs: f64, axis_px: f32, label_extent: f32) -> Vec<Tick> {
  if duration_secs <= 0.0 {
    return Vec::new();
  }
  const FACTORS: &[i64] = &[1, 2, 5, 10, 20, 30, 60, 120, 300, 600, 1200, 1800];
  let max = duration_secs as i64;
  let scale = axis_px / duration_secs as f32;
  ruler(0, max, FACTORS, 1.5, scale, label_extent)
    .map(|s| Tick { pos: s as f32 / duration_secs as f32, label: format!("{}:{:02}", s / 60, s % 60) })
    .collect()
}

/// Frequency ticks from 0 Hz to the Nyquist frequency, labelled in kHz.
pub fn frequency(sample_rate: u32, axis_px: f32, label_extent: f32) -> Vec<Tick> {
  let nyquist = (sample_rate / 2) as i64;
  if nyquist == 0 {
    return Vec::new();
  }
  const FACTORS: &[i64] = &[1000, 2000, 5000, 10000, 20000];
  let scale = axis_px / nyquist as f32;
  ruler(0, nyquist, FACTORS, 3.0, scale, label_extent)
    .map(|hz| Tick { pos: hz as f32 / nyquist as f32, label: format!("{} kHz", hz / 1000) })
    .collect()
}

/// dB ticks for the palette bar, counting down from `upper` (at the top).
pub fn db(range: crate::DbRange, axis_px: f32, label_extent: f32) -> Vec<Tick> {
  // Work in attenuation units (0, 10, 20, ...) so ticks are round numbers
  // counted from the top, as in Spek.
  let min = -range.upper.round() as i64;
  let max = -range.lower.round() as i64;
  if max <= min {
    return Vec::new();
  }
  const FACTORS: &[i64] = &[1, 2, 5, 10, 20, 50];
  let span = (max - min) as f32;
  ruler(min, max, FACTORS, 3.0, axis_px / span, label_extent)
    .map(|u| Tick { pos: 1.0 - (u - min) as f32 / span, label: format!("{} dB", -u) })
    .collect()
}

/// Spek's tick selection: always the two ends, plus multiples of the smallest
/// factor that leaves `spacing` label-lengths between ticks, stopping before a
/// tick would crowd the far end.
fn ruler(
  min: i64,
  max: i64,
  factors: &[i64],
  spacing: f32,
  px_per_unit: f32,
  label_extent: f32,
) -> impl Iterator<Item = i64> {
  let factor = factors.iter().copied().find(|&f| (px_per_unit * f as f32).abs() >= spacing * label_extent);
  let middle = factor.into_iter().flat_map(move |f| {
    (1..)
      .map(move |k| min + k * f)
      .take_while(move |&t| t < max && (px_per_unit * (max - t) as f32).abs() >= label_extent * 1.2)
  });
  std::iter::once(min).chain(middle).chain((max > min).then_some(max))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn labels(ticks: &[Tick]) -> Vec<&str> {
    ticks.iter().map(|t| t.label.as_str()).collect()
  }

  #[test]
  fn frequency_ticks() {
    let ticks = frequency(44_100, 800.0, 13.0);
    // 1 kHz steps would be 36px apart, under 3 label heights, so 2 kHz steps.
    assert_eq!(
      labels(&ticks),
      [
        "0 kHz", "2 kHz", "4 kHz", "6 kHz", "8 kHz", "10 kHz", "12 kHz", "14 kHz", "16 kHz", "18 kHz", "20 kHz",
        "22 kHz"
      ]
    );
    assert_eq!(ticks[0].pos, 0.0);
    assert_eq!(ticks.last().unwrap().pos, 1.0);
  }

  #[test]
  fn time_ticks_skip_crowded_end() {
    let ticks = time(185.5, 1000.0, 33.0);
    assert_eq!(ticks[0].label, "0:00");
    assert_eq!(ticks.last().unwrap().label, "3:05");
    assert!(ticks.iter().all(|t| t.pos <= 1.0));
    // 10s steps (54px apart): the 3:00 tick is too close to 3:05 and is dropped.
    assert_eq!(ticks[1].label, "0:10");
    assert!(!labels(&ticks).contains(&"3:00"));
  }

  #[test]
  fn db_ticks_count_down_from_top() {
    let ticks = db(crate::DbRange::default(), 800.0, 13.0);
    assert_eq!(ticks[0].label, "0 dB");
    assert_eq!(ticks[0].pos, 1.0);
    assert_eq!(ticks.last().unwrap().label, "-120 dB");
    assert_eq!(ticks.last().unwrap().pos, 0.0);
  }
}
