# Specal

A Rust re-implementation of [Spek](https://github.com/alexkay/spek), the acoustic spectrum analyser.

## Layout

| Crate | Path | What it does |
|---|---|---|
| `specal-core` | `core/` | Decoding (symphonia), FFT (realfft), palettes, axis ticks, page layout. No GUI code. Optional `svg` feature for SVG export. |
| `specal` | `cli/` | Command line: audio file in, SVG (with axes) or PNG (bare spectrogram) out. |
| `specal-gui` | `gui/` | iced window: drop an audio file on it. |

## Usage

```sh
cargo run --release -p specal -- track.flac -o track.svg        # Spek-style image with axes
cargo run --release -p specal -- track.flac -o track.png        # bare spectrogram
cargo run --release -p specal -- --help                         # palette, window, FFT size, dB range...

cargo run --release -p specal-gui                               # then drag a file onto the window
cargo run --release -p specal-gui -- track.flac                 # or open one directly
```

## Tests

```sh
cargo test --workspace
```
