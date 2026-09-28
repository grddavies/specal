use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use specal_core::layout::Layout;
use specal_core::{Columns, DbRange, Palette, Settings, WindowFunction, svg};

/// Render an audio file's spectrogram.
///
/// `.svg` output gets Spek-style axes, labels and a colour bar. `.png` output
/// is the bare spectrogram, one pixel per column and frequency band.
#[derive(Parser)]
#[command(version)]
struct Args {
  input: PathBuf,
  /// Output file, `.svg` or `.png`. Defaults to the input name with `.svg`.
  #[arg(short, long)]
  output: Option<PathBuf>,
  /// Image width in pixels.
  #[arg(long, default_value_t = 1200)]
  width: u32,
  /// Image height in pixels (SVG only; PNG height is the number of bands).
  #[arg(long, default_value_t = 700)]
  height: u32,
  /// Channel to analyse, starting at 1.
  #[arg(short, long, default_value_t = 1)]
  channel: usize,
  /// log2 of the FFT window length (8 to 14).
  #[arg(short = 'w', long, default_value_t = 11)]
  fft_bits: u32,
  #[arg(short = 'f', long, value_enum, default_value_t = Window::Hann)]
  window: Window,
  #[arg(short, long, value_enum, default_value_t = PaletteArg::Sox)]
  palette: PaletteArg,
  /// Lower end of the dB range.
  #[arg(long, default_value_t = -120.0, allow_negative_numbers = true)]
  lower: f32,
  /// Upper end of the dB range.
  #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
  upper: f32,
}

#[derive(Clone, Copy, ValueEnum)]
enum Window {
  Hann,
  Hamming,
  BlackmanHarris,
}

#[derive(Clone, Copy, ValueEnum)]
enum PaletteArg {
  Spectrum,
  Sox,
  Mono,
}

fn main() -> Result<()> {
  let args = Args::parse();
  if args.channel == 0 {
    bail!("channels are numbered from 1");
  }
  if args.lower >= args.upper {
    bail!("--lower must be below --upper");
  }

  let output = args.output.clone().unwrap_or_else(|| args.input.with_extension("svg"));
  let is_png = match output.extension().and_then(|e| e.to_str()) {
    Some(ext) if ext.eq_ignore_ascii_case("png") => true,
    Some(ext) if ext.eq_ignore_ascii_case("svg") => false,
    _ => bail!("output must end in .svg or .png"),
  };

  let settings = Settings {
    channel: args.channel - 1,
    fft_bits: args.fft_bits,
    window: match args.window {
      Window::Hann => WindowFunction::Hann,
      Window::Hamming => WindowFunction::Hamming,
      Window::BlackmanHarris => WindowFunction::BlackmanHarris,
    },
  };
  let palette = match args.palette {
    PaletteArg::Spectrum => Palette::Spectrum,
    PaletteArg::Sox => Palette::Sox,
    PaletteArg::Mono => Palette::Mono,
  };
  let range = DbRange { lower: args.lower, upper: args.upper };

  let columns = if is_png { args.width as usize } else { Layout::new(args.width as f32, args.height as f32).columns() };
  let analysis = specal_core::analyse(&args.input, &settings, Columns::Exact(columns))
    .with_context(|| format!("analysing {}", args.input.display()))?;

  if is_png {
    let bytes = svg::encode_png(&analysis.spectrogram.to_rgba(palette, range));
    std::fs::write(&output, bytes)
  } else {
    let title = args.input.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let opts = svg::Options { width: args.width, height: args.height, palette, range, title };
    std::fs::write(&output, svg::render(&analysis, &opts))
  }
  .with_context(|| format!("writing {}", output.display()))?;

  eprintln!("{}\n→ {}", analysis.description(), output.display());
  Ok(())
}
