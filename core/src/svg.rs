//! Standalone SVG export: the spectrogram and colour bar as embedded PNGs,
//! with vector axes, ticks and text

use std::fmt::Write;
use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{ImageFormat, RgbaImage};

use crate::layout::{self, Layout};
use crate::{Analysis, DbRange, Palette, Rgba, ticks};

const FONT: &str = "Helvetica, Arial, sans-serif";
/// Rough average glyph width as a fraction of font size, used to space
/// time labels. Viewers render the text, so we can't measure it exactly.
const CHAR_WIDTH: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct Options {
  pub width: u32,
  pub height: u32,
  pub palette: Palette,
  pub range: DbRange,
  /// First header line, usually the file name.
  pub title: String,
}

/// Render `analysis` as an SVG document.
///
/// For a 1:1 image, analyse with `Layout::new(width, height).columns()` columns.
pub fn render(analysis: &Analysis, opts: &Options) -> String {
  let (w, h) = (opts.width as f32, opts.height as f32);
  let l = Layout::new(w, h);
  let plot = l.plot;
  let bar = l.bar;
  let label_height = layout::LABEL_SIZE * 1.2;
  let time_label_width = ticks::TIME_SAMPLE_LABEL.len() as f32 * layout::LABEL_SIZE * CHAR_WIDTH;

  let mut s = String::new();
  let _ = writeln!(
    s,
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{FONT}" fill="white">"#
  );
  let _ = writeln!(s, r#"<rect width="100%" height="100%" fill="black"/>"#);

  // Header.
  let _ = writeln!(
    s,
    r#"<text x="{}" y="{}" font-size="{}" font-weight="bold" dominant-baseline="hanging">{}</text>"#,
    l.title.0,
    l.title.1,
    layout::TITLE_SIZE,
    escape(&opts.title)
  );
  let _ = writeln!(
    s,
    r#"<text x="{}" y="{}" font-size="{}" dominant-baseline="hanging">{}</text>"#,
    l.description.0,
    l.description.1,
    layout::BODY_SIZE,
    escape(&analysis.description())
  );
  let _ = writeln!(
    s,
    r#"<text x="{}" y="{}" font-size="{}" font-weight="bold" dominant-baseline="hanging">Specal <tspan font-size="{}" font-weight="normal">{}</tspan></text>"#,
    l.app_name.0,
    l.app_name.1,
    layout::TITLE_SIZE,
    layout::LABEL_SIZE,
    env!("CARGO_PKG_VERSION")
  );

  if plot.width > 0.0 && plot.height > 0.0 {
    // Spectrogram and palette bar, stretched to fit.
    let spectrogram = analysis.spectrogram.to_rgba(opts.palette, opts.range);
    embed_image(&mut s, &spectrogram, &plot);
    embed_image(&mut s, &opts.palette.bar(256), &bar);

    let _ = writeln!(s, r#"<g stroke="white" stroke-width="1" fill="none">"#);
    let _ = writeln!(
      s,
      r#"<rect x="{}" y="{}" width="{}" height="{}"/>"#,
      plot.x, plot.y, plot.width, plot.height
    );
    let _ = writeln!(s, "</g>");

    let _ = writeln!(s, r#"<g font-size="{}">"#, layout::LABEL_SIZE);

    // Time axis, below the plot.
    for t in ticks::time(analysis.info.duration, plot.width, time_label_width) {
      let x = plot.x + t.pos * plot.width;
      let y = plot.bottom();
      tick_line(&mut s, x, y, x, y + layout::TICK);
      let _ = writeln!(
        s,
        r#"<text x="{x}" y="{}" text-anchor="middle" dominant-baseline="hanging">{}</text>"#,
        y + layout::GAP,
        t.label
      );
    }

    // Frequency axis, left of the plot.
    for t in ticks::frequency(analysis.info.sample_rate, plot.height, label_height) {
      let x = plot.x;
      let y = plot.bottom() - t.pos * plot.height;
      tick_line(&mut s, x, y, x - layout::TICK, y);
      let _ = writeln!(
        s,
        r#"<text x="{}" y="{y}" text-anchor="end" dominant-baseline="central">{}</text>"#,
        x - layout::GAP,
        t.label
      );
    }

    // dB scale, right of the bar.
    for t in ticks::db(opts.range, bar.height, label_height) {
      let x = bar.right();
      let y = bar.bottom() - t.pos * bar.height;
      tick_line(&mut s, x, y, x + layout::TICK, y);
      let _ = writeln!(
        s,
        r#"<text x="{}" y="{y}" dominant-baseline="central">{}</text>"#,
        x + layout::GAP,
        t.label
      );
    }

    let _ = writeln!(s, "</g>");
  }

  s.push_str("</svg>\n");
  s
}

fn embed_image(s: &mut String, img: &Rgba, at: &layout::Rect) {
  let _ = writeln!(
    s,
    r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" href="data:image/png;base64,{}"/>"#,
    at.x,
    at.y,
    at.width,
    at.height,
    BASE64.encode(encode_png(img))
  );
}

fn tick_line(s: &mut String, x1: f32, y1: f32, x2: f32, y2: f32) {
  let _ = writeln!(s, r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="white"/>"#);
}

/// Encode an [`Rgba`] image as PNG bytes.
pub fn encode_png(img: &Rgba) -> Vec<u8> {
  let buf = RgbaImage::from_raw(img.width, img.height, img.pixels.clone())
    .expect("pixel buffer matches dimensions");
  let mut out = Cursor::new(Vec::new());
  buf.write_to(&mut out, ImageFormat::Png).expect("PNG encoding to memory cannot fail");
  out.into_inner()
}

fn escape(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
