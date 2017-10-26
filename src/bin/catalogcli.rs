extern crate chromaprint;
extern crate clap;
extern crate ffmpeg;
extern crate rayon;

extern crate music_card_catalog;

use chromaprint::Chromaprint;
use clap::{App, Arg, SubCommand};
use ffmpeg::decoder;
use ffmpeg::format::Sample;
use rayon::prelude::*;

use music_card_catalog::database;
use music_card_catalog::file_scanner;
use music_card_catalog::config::Config;
use music_card_catalog::models::MediaFileInfo;

// Maximum duration global from Chromaprint's fpcalc utility
static MAX_AUDIO_DURATION: f64 = 120.0;

fn audio_decode(ictx: &mut ffmpeg::format::context::Input) -> Result<(decoder::Audio, usize, f64), ffmpeg::Error> {
  for (k, v) in ictx.metadata().iter() {
    println!("{}: {}", k, v);
  }

  let stream = ictx.streams().best(ffmpeg::media::Type::Audio).expect("could not find best audio stream");
  let index = stream.index();
  let duration = stream.duration() as f64 * f64::from(stream.time_base());
  println!("Best audio stream index: {}", index);

  let codec = stream.codec();
  println!("medium: {:?}", codec.medium());
  println!("id: {:?}", codec.id());

  let mut decoder = try!(stream.codec().decoder().audio());
  try!(decoder.set_parameters(stream.parameters()));

  Ok((decoder, index, duration))
}

// Main entrypoint for the program
fn main() {
  let matches = App::new("Music Card Catalog")
    .version("0.1.0")
    .author("Matt Bilker <me@mbilker.us>")
    .about("Gather data about my music library")
    .subcommand(SubCommand::with_name("scan")
      .about("scan music library directories")
      .author("Matt Bilker <me@mbilker.us>"))
    .subcommand(SubCommand::with_name("info")
      .about("show info about a single file")
      .author("Matt Bilker <me@mbilker.us>")
      .arg(Arg::with_name("path")
        .help("the file path")
        .index(1)
        .required(true)))
    .subcommand(SubCommand::with_name("fingerprint")
      .about("display a file's chromaprint fingerprint")
      .author("Matt Bilker <me@mbilker.us>")
      .arg(Arg::with_name("path")
        .help("the file path")
        .index(1)
        .required(true)))
    .get_matches();

  let config: Config = match Config::read_configuration() {
    Ok(res) => res,
    Err(err) => panic!("Error reading configuration: {:?}", err),
  };
  println!("Config: {:?}", config);

  if let Some(_matches) = matches.subcommand_matches("scan") {
    let conn = database::establish_connection().unwrap();
    println!("Database Connection: {:?}", conn);

    for path in config.paths {
      println!("Scanning {}", path);

      // TODO(mbilker): either figure out how to make rayon iterate over
      // filtered results using something like `for_each` or make my own
      // work queue
      //
      // `for_each` works, but the SQL connection is not `Sync + Send`
      let dir_walk = file_scanner::scan_dir(&path);
      let files: Vec<MediaFileInfo> = dir_walk.par_iter()
        .filter_map(|e| MediaFileInfo::read_file(e))
        .filter(|e| !e.is_default_values())
        .collect();

      database::insert_files(&conn, &files);
    }
  } else if let Some(matches) = matches.subcommand_matches("info") {
    let file_path = matches.value_of("path").unwrap();
    let info = MediaFileInfo::read_file(file_path);

    println!("{:?}", info);

    if let Some(info) = info {
      println!("Info for {}", file_path);
      println!("Title: {}", info.title.unwrap_or_else(|| String::new()));
      println!("Artist: {}", info.artist.unwrap_or_else(|| String::new()));
      println!("Album: {}", info.album.unwrap_or_else(|| String::new()));
      println!("Track: {}", info.track.unwrap_or_else(|| String::new()));
      println!("Track Number: {}", info.track_number);
      println!("Duration: {} ms", info.duration);
    } else {
      println!("No info could be gathered from the file");
    }
  } else if let Some(matches) = matches.subcommand_matches("fingerprint") {
    println!("Chromaprint version: {}", Chromaprint::version());

    let file_path = matches.value_of("path").unwrap();

    ffmpeg::init().unwrap();

    let mut ictx = ffmpeg::format::input(&file_path).unwrap();
    let (mut decoder, index, duration) = match audio_decode(&mut ictx) {
      Ok(res) => res,
      Err(err) => {
        println!("{:?}", err);
        panic!("error initializing audio decoder");
      },
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
    println!("samplerate: {}", samplerate);

    let channels = decoder.channels();
    println!("channels: {}", channels);

    let channel_layout = decoder.channel_layout();
    let in_format = (decoder.format(), channel_layout, samplerate);
    let out_format = (Sample::from("s16"), channel_layout, samplerate);
    let mut convert = ffmpeg::software::resampler(in_format, out_format).unwrap();

    let mut stream_size = 0;
    let stream_limit = (MAX_AUDIO_DURATION as u32) * samplerate;
    println!("stream_limit: {}", stream_limit);

    let mut chroma = Chromaprint::new();
    let start_res = chroma.start(samplerate as i32, channels as i32);
    println!("start_res: {}", start_res);

    let mut decoded = ffmpeg::frame::Audio::empty();
    for (stream, packet) in ictx.packets() {
      if stream.index() == index {
        if let Ok(true) = decoder.decode(&packet, &mut decoded) {
          let mut processed = ffmpeg::frame::Audio::empty();
          let delay = convert.run(&decoded, &mut processed).unwrap();

          //println!("packet size: {}, delay: {:?}, frame: {:?}", packet.size(), delay, decoded);
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

          if frame_size == 0 {
            if stream_done {
              break;
            } else {
              continue;
            }
          }

          if frame_size > 0 {
            let data_size = (frame_size * channels as u32) as usize;
            let data = unsafe { std::slice::from_raw_parts(processed.data(0).as_ptr() as *const u8, data_size) };
            let feed_res = chroma.feed(data);
            if !feed_res {
              panic!("feed_res not true");
            }
          }

          if stream_done {
            break;
          }
        }
      }
    }

    let finish_res = chroma.finish();
    println!("finish_res: {}", finish_res);

    let fingerprint = chroma.fingerprint();
    println!("fingerprint: {:?}", fingerprint);
    if let Some(fingerprint) = fingerprint {
      eprintln!("{}", fingerprint);
    }
  }
}
