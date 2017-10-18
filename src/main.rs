extern crate clap;
extern crate mediainfo;
extern crate rayon;
extern crate serde;
extern crate serde_yaml;
extern crate walkdir;

#[macro_use]
extern crate serde_derive;

use clap::{App, SubCommand};
use mediainfo::MediaInfo;
use rayon::prelude::*;

mod config;
mod file_scanner;

use config::Config;

#[derive(Debug)]
struct MediaFileInfo {
  title: Option<String>,
  artist: Option<String>,
  album: Option<String>,
  track: Option<String>,
  track_number: u32,
  duration: u32,
}

impl MediaFileInfo {
  #[inline]
  fn is_default_values(&self) -> bool {
    self.title  == None &&
    self.artist == None &&
    self.album  == None &&
    self.track  == None &&
    self.duration == 0 &&
    self.track_number == 0
  }
}

fn get_media_file_info(path: &String) -> Option<(&String, MediaFileInfo)> {
  let mut media_info: MediaInfo = MediaInfo::new();

  // Fail quickly if the file could not be opened
  if let Err(_) = media_info.open(path) {
    println!("could not open file: {}", path);
    return None;
  }

  // Filter out any file without an audio stream
  //
  // `get_with_default_option` throws a ZeroLengthError if there is no value
  // for the parameter
  let audio_streams = media_info.get_with_default_options("AudioCount");
  if let Err(_) = audio_streams {
    return None;
  }

  // Filter out any file with no duration
  let duration = media_info.get_duration_ms().unwrap_or(0);
  if duration == 0 {
    return None;
  }

  // Store the most relevant details in a struct for easy access
  let file_info = MediaFileInfo {
    title:        media_info.get_title().ok(),
    artist:       media_info.get_performer().ok(),
    album:        media_info.get_album().ok(),
    duration:     duration,
    track:        media_info.get_track_name().ok(),
    track_number: media_info.get_track_number().unwrap_or(0),
  };

  media_info.close();

  Some((path, file_info))
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
    .get_matches();

  if let Some(matches) = matches.subcommand_matches("scan") {
    println!("matched: {:?}", matches);

    let config: Config = match Config::read_configuration() {
      Ok(res) => res,
      Err(err) => panic!("Error reading configuration: {:?}", err),
    };
    println!("Config: {:?}", config);

    for path in config.paths {
      println!("Scanning {}", path);

      // TODO(mbilker): either figure out how to make rayon iterate over
      // filtered results using something like `for_each` or make my own
      // work queue
      let dir_walk = file_scanner::scan_dir(&path);
      let iter: Vec<(&String, MediaFileInfo)> = dir_walk.par_iter()
        .filter_map(|e| get_media_file_info(e))
        .collect();

      for (file_name, info) in iter {
        println!("{}", file_name);
        println!("- {:?}", info);
      }
    }
  }
}
