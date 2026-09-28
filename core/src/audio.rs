//! Decoding with symphonia: file metadata plus a stream of samples for one channel.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::Error;

/// What we know about the audio stream before decoding it.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
  /// Human-readable codec name, e.g. "Free Lossless Audio Codec".
  pub codec: String,
  pub sample_rate: u32,
  pub channels: usize,
  /// Bits per sample, for lossless codecs that report it.
  pub bits_per_sample: Option<u32>,
  /// Average bit rate in bits per second, estimated from the file size.
  /// Only set when `bits_per_sample` is not (i.e. for lossy codecs), like Spek.
  pub bit_rate: Option<u32>,
  /// Duration in seconds.
  pub duration: f64,
  /// Number of frames (samples per channel).
  pub frames: u64,
}

impl AudioInfo {
  /// Spek-style one-line description, e.g.
  /// `"Free Lossless Audio Codec, 44100 Hz, 16 bits, channel 1 / 2, W:2048, F:Hann"`.
  pub fn description(&self, settings: &crate::Settings) -> String {
    let mut items = vec![self.codec.clone()];
    if let Some(rate) = self.bit_rate {
      items.push(format!("{} kbps", (rate + 500) / 1000));
    }
    items.push(format!("{} Hz", self.sample_rate));
    if let Some(bits) = self.bits_per_sample {
      items.push(format!("{bits} {}", if bits == 1 { "bit" } else { "bits" }));
    }
    items.push(format!("channel {} / {}", settings.channel + 1, self.channels));
    items.push(format!("W:{}", 1u32 << settings.fft_bits));
    items.push(format!("F:{}", settings.window));
    items.join(", ")
  }
}

/// An open audio file that yields decoded samples for one channel.
pub(crate) struct Decoder {
  format: Box<dyn FormatReader>,
  decoder: Box<dyn AudioDecoder>,
  track_id: u32,
  channel: usize,
  planes: Vec<Vec<f32>>,
  pub info: AudioInfo,
}

impl Decoder {
  pub fn open(path: &Path, channel: usize) -> Result<Self, Error> {
    let file = File::open(path).map_err(|source| Error::Open { path: path.to_owned(), source })?;
    let file_size = file.metadata().map(|m| m.len()).ok();
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
      hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
      .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
      .map_err(Error::Format)?;

    let track = format.default_track(TrackType::Audio).ok_or(Error::NoAudio)?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(Error::NoAudio)?;

    let sample_rate = params.sample_rate.ok_or(Error::NoSampleRate)?;
    let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(0);
    if channel >= channels {
      return Err(Error::NoChannel { requested: channel, available: channels });
    }

    let duration = track
      .time_base
      .zip(track.duration)
      .and_then(|(tb, d)| tb.calc_duration(d))
      .map(|t| t.as_secs_f64())
      .or(track.num_frames.map(|n| n as f64 / sample_rate as f64))
      .ok_or(Error::NoDuration)?;
    let frames = track.num_frames.unwrap_or((duration * sample_rate as f64).round() as u64);
    if frames == 0 {
      return Err(Error::NoDuration);
    }

    let codecs = symphonia::default::get_codecs();
    let codec = codecs
      .get_audio_decoder(params.codec)
      .map(|d| d.codec.info.long_name.to_owned())
      .unwrap_or_else(|| "Unknown codec".to_owned());

    let bits_per_sample = params.bits_per_sample.or(params.bits_per_coded_sample);
    let bit_rate = match bits_per_sample {
      Some(_) => None,
      None => file_size.map(|size| (size as f64 * 8.0 / duration) as u32),
    };

    let decoder = codecs.make_audio_decoder(params, &AudioDecoderOptions::default()).map_err(Error::NoDecoder)?;
    let track_id = track.id;

    Ok(Self {
      format,
      decoder,
      track_id,
      channel,
      planes: Vec::new(),
      info: AudioInfo { codec, sample_rate, channels, bits_per_sample, bit_rate, duration, frames },
    })
  }

  /// Decode the next packet. Returns `None` at the end of the stream.
  pub fn next_chunk(&mut self) -> Result<Option<&[f32]>, Error> {
    loop {
      let packet = match self.format.next_packet() {
        Ok(Some(packet)) => packet,
        Ok(None) => return Ok(None),
        // A chained stream changed its track list; treat it as the end, like Spek.
        Err(SymphoniaError::ResetRequired) => return Ok(None),
        Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
          return Ok(None);
        }
        Err(e) => return Err(Error::Decode(e)),
      };
      if packet.track_id != self.track_id {
        continue;
      }
      match self.decoder.decode(&packet) {
        Ok(buf) => {
          if buf.frames() == 0 {
            continue;
          }
          buf.copy_to_vecs_planar::<f32>(&mut self.planes);
          return Ok(self.planes.get(self.channel).map(Vec::as_slice));
        }
        // Skip corrupt packets rather than giving up on the whole file.
        Err(SymphoniaError::DecodeError(_) | SymphoniaError::IoError(_)) => continue,
        Err(e) => return Err(Error::Decode(e)),
      }
    }
  }
}
