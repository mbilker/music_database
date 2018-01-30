use chromaprint::Chromaprint;
use ffmpeg::ChannelLayout;
use ffmpeg::decoder::Audio as AudioDecoder;
use ffmpeg::format::{self, Sample};
use ffmpeg::format::context::Input;
use ffmpeg::frame::Audio;
use ffmpeg::media::Type;
use ffmpeg::software;

use basic_types::*;

// Maximum duration global from Chromaprint's fpcalc utility
static MAX_AUDIO_DURATION: f64 = 120.0;

fn get_best_audio_stream(ictx: &Input) -> Result<(AudioDecoder, f64, usize), ProcessorError> {
  let stream = try!(ictx.streams().best(Type::Audio).ok_or(ProcessorError::NoAudioStream));
  let duration = stream.duration() as f64 * f64::from(stream.time_base());
  let index = stream.index();
  debug!("best audio stream index: {}", index);

  let codec = stream.codec();
  debug!("medium: {:?}", codec.medium());
  debug!("id: {:?}", codec.id());

  let mut decoder = try!(stream.codec().decoder().audio());
  try!(decoder.set_parameters(stream.parameters()));

  // Check for empty channel layout and set to a default one for the number
  // of channels
  if decoder.channel_layout().channels() == 0 {
    let channels = i32::from(decoder.channels());

    decoder.set_channel_layout(ChannelLayout::default(channels));
  }

  Ok((decoder, duration, index))
}

pub fn get(path: &str) -> Result<(f64, String), ProcessorError> {
  debug!("Chromaprint version: {}", Chromaprint::version());

  let mut ictx = try!(format::input(&path));
  let (mut decoder, duration, index) = try!(get_best_audio_stream(&ictx));

  let samplerate = decoder.rate();
  let channels = i32::from(decoder.channels());

  debug!("duration: {}", duration);
  debug!("bit_rate: {}", decoder.bit_rate());
  debug!("max_bit_rate: {}", decoder.max_bit_rate());
  debug!("delay: {}", decoder.delay());
  debug!("audio.rate: {}", samplerate);
  debug!("audio.channels: {}", channels);
  debug!("audio.format: {:?} (name: {})", decoder.format(), decoder.format().name());
  debug!("audio.frames: {}", decoder.frames());
  debug!("audio.align: {}", decoder.align());
  debug!("audio.channel_layout: {:?} (channels: {})", decoder.channel_layout(), decoder.channel_layout().channels());
  debug!("audio.frame_start: {:?}", decoder.frame_start());

  // Setup the converter to signed 16-bit interleaved needed for
  // accurate fingerprints for AcoustID
  let channel_layout = decoder.channel_layout();
  let in_format = (decoder.format(), channel_layout, samplerate);
  let out_format = (Sample::from("s16"), channel_layout, samplerate);
  let mut convert = try!(software::resampler(in_format, out_format));

  // Stream size limit used to count the number of samples for two minutes
  // of audio based on AcoustID's reference implementation
  let stream_limit = (MAX_AUDIO_DURATION as u32) * samplerate;
  let mut stream_size = 0;
  debug!("stream_limit: {}", stream_limit);

  // Initialize Chromaprint context
  let mut chroma = Chromaprint::new();
  if !chroma.start(samplerate as i32, channels) {
    return Err(ProcessorError::Chromaprint("failed to start chromaprint"));
  }

  // Buffer frame for the current decoded packet
  let mut decoded = Audio::empty();

  // Iterate through all the relevant packets based on the stream index
  //
  // I probably would have never figured out how to do this without the
  // reference C implementation.
  for (stream, packet) in ictx.packets() {
    // Only want packets for the audio stream
    if stream.index() != index {
      continue;
    }

    let decoder_res = try!(decoder.decode(&packet, &mut decoded));
    if decoder_res != true {
      continue;
    }

    let mut processed = Audio::empty();
    let delay = try!(convert.run(&decoded, &mut processed));

    trace!("packet size: {}, delay: {:?}, processed: {:?}", packet.size(), delay, processed);

    let mut frame_size = processed.samples() as u32;
    let remaining = stream_limit - stream_size;
    let stream_done = {
      if frame_size > remaining {
        frame_size = remaining;
        true
      } else {
        false
      }
    };
    stream_size += frame_size;

    if frame_size == 0 && !stream_done {
      continue;
    }

    if frame_size > 0 {
      // Feed chromaprint with the audio data
      //
      // There is only one plane because the audio data is now interleaved by
      // the resampler
      let data_size = (frame_size * channels as u32) as usize;
      let data = processed.data(0);
      trace!("data_size: {}, data.len(): {}", data_size, data.len());
      if !chroma.feed(&data[0..data_size]) {
        return Err(ProcessorError::Chromaprint("feed returned false"));
      }
    }

    if stream_done {
      break;
    }
  }

  let finish_res = chroma.finish();
  debug!("finish_res: {}", finish_res);

  let fingerprint = try!(chroma.fingerprint().ok_or(ProcessorError::Chromaprint("no fingerprint generated")));
  debug!("fingerprint: {}", fingerprint);

  Ok((duration, fingerprint))
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_get() {
    // TODO(mbilker): compose a few example known good fingerprints and
    // assert_eq! them to values from calling get
  }
}
