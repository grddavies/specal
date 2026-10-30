<div align="center">
<h1 align="center">
  <img src="./assets/logo.svg" alt="Specal" width="128" height="128" />
  <br>
  Specal
</h1>
<p>
A Rust re-implementation of <a href="https://github.com/alexkay/spek">Spek</a>, the acoustic spectrum analyser.
</p>
</div>

## Usage

### Keymaps

| Key       | Action                                                 |
| --------- | ------------------------------------------------------ |
| `c` / `C` | Cycle audio channel                                    |
| `f` / `F` | Cycle window function (Hann, Hamming, Blackman–Harris) |
| `w` / `W` | Increment/decrement FFT window size (256 to 16384)     |
| `p` / `P` | Cycle palette (Spectrum, SoX, Mono)                    |
| `l` / `L` | Increment/decrement lower dB limit (down to -140)      |
| `u` / `U` | Increment/decrement Upper dB limit (up to 0)           |
| `Esc`     | Reset the dB limits to -120 and 0                      |

## Development

### Crates

| Crate         | Path    | What it does                                                                                                                |
| ------------- | ------- | --------------------------------------------------------------------------------------------------------------------------- |
| `specal-core` | `core/` | Decoding (symphonia), FFT (realfft), palettes, axis ticks, page layout. No GUI code. Optional `svg` feature for SVG export. |
| `specal`      | `cli/`  | Command line: audio file in, SVG (with axes) or PNG (bare spectrogram) out.                                                 |
| `specal-gui`  | `gui/`  | iced window: drop an audio file on it.                                                                                      |

### Commands

```sh
cargo run --release -p specal -- track.flac -o track.svg        # Svg spectrogram with axes
cargo run --release -p specal -- track.flac -o track.png        # bare spectrogram
cargo run --release -p specal -- --help                         # palette, window, FFT size, dB range...

cargo run --release -p specal-gui                               # then drag a file onto the window
cargo run --release -p specal-gui -- track.flac                 # or open one directly
```

### Test

```sh
cargo test --workspace
```
