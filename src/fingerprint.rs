use chromaprint::Chromaprint;
use ffmpeg;
use ffmpeg::frame::Audio;
use ffmpeg::format::Sample;

use std;

// Maximum duration global from Chromaprint's fpcalc utility
static MAX_AUDIO_DURATION: f64 = 120.0;

#[derive(Debug)]
pub enum FingerprintError {
  FFmpegError(ffmpeg::Error),
  ChromaprintError(String),
}

impl From<ffmpeg::Error> for FingerprintError {
  fn from(value: ffmpeg::Error) -> FingerprintError {
    FingerprintError::FFmpegError(value)
  }
}

pub fn get(path: &str) -> Result<(f64, String), FingerprintError> {
  try!(ffmpeg::init());

  let mut ictx = try!(ffmpeg::format::input(&path));

  let duration;
  let index;
  let mut decoder = {
    let stream = ictx.streams().best(ffmpeg::media::Type::Audio).expect("could not find best audio stream");

    duration = stream.duration() as f64 * f64::from(stream.time_base());
    index = stream.index();
    println!("best audio stream index: {}", index);

    let codec = stream.codec();
    println!("medium: {:?}", codec.medium());
    println!("id: {:?}", codec.id());

    let mut decoder = try!(stream.codec().decoder().audio());
    try!(decoder.set_parameters(stream.parameters()));

    decoder
  };

  println!("duration: {}", duration);
  println!("bit_rate: {}", decoder.bit_rate());
  println!("max_bit_rate: {}", decoder.max_bit_rate());
  println!("delay: {}", decoder.delay());
  println!("audio.rate: {}", decoder.rate());
  println!("audio.channels: {}", decoder.channels());
  println!("audio.format: {:?} (name: {})", decoder.format(), decoder.format().name());
  println!("audio.frames: {}", decoder.frames());
  println!("audio.align: {}", decoder.align());
  println!("audio.channel_layout: {:?}", decoder.channel_layout());
  println!("audio.frame_start: {:?}", decoder.frame_start());

  let samplerate = decoder.rate();
  let channels = decoder.channels();

  // Setup the converter to signed 16-bit interleaved needed for
  // accurate fingerprints for AcoustID
  let channel_layout = decoder.channel_layout();
  let in_format = (decoder.format(), channel_layout, samplerate);
  let out_format = (Sample::from("s16"), channel_layout, samplerate);
  let mut convert = try!(ffmpeg::software::resampler(in_format, out_format));

  // Stream size limit used to count the number of samples for two minutes
  // of audio based on AcoustID's reference implementation
  let stream_limit = (MAX_AUDIO_DURATION as u32) * samplerate;
  let mut stream_size = 0;
  println!("stream_limit: {}", stream_limit);

  // Initialize Chromaprint context
  let mut chroma = Chromaprint::new();
  if !chroma.start(samplerate as i32, channels as i32) {
    return Err(FingerprintError::ChromaprintError("failed to start chromaprint".to_owned()));
  }

  // Buffer frame for the current decoded packet
  let mut decoded = Audio::empty();

  // Iterate through all the relevant packets based on the stream index
  //
  // I probably would have never figured out how to do this without the
  // reference C implementation.
  let filtered = ictx.packets()
    .filter(|&(ref stream, _)| stream.index() == index);
  for (_stream, packet) in filtered {
    let decoder_res = try!(decoder.decode(&packet, &mut decoded));
    if decoder_res != true {
      continue;
    }

    let mut processed = Audio::empty();
    let delay = try!(convert.run(&decoded, &mut processed));

    if delay != None {
      println!("packet size: {}, delay: {:?}, frame: {:?}", packet.size(), delay, decoded);
    }

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
      println!("reached frame_size == 0 && !stream_done");
      continue;
    }

    if frame_size > 0 {
      let data_size = (frame_size * channels as u32) as usize;
      let data = unsafe { std::slice::from_raw_parts(processed.data(0).as_ptr() as *const u8, data_size) };
      let feed_res = chroma.feed(data);
      if !feed_res {
        return Err(FingerprintError::ChromaprintError("chromaprint feed returned false".to_owned()));
      }
    }

    if stream_done {
      println!("reached stream_done");
      break;
    }
  }

  let finish_res = chroma.finish();
  println!("finish_res: {}", finish_res);

  let fingerprint = chroma.fingerprint();
  if let Some(fingerprint) = fingerprint {
    Ok((duration, fingerprint))
  } else {
    Err(FingerprintError::ChromaprintError("no fingerprint generated".to_owned()))
  }
}
