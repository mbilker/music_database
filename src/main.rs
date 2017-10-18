extern crate clap;
extern crate mediainfo;
extern crate rayon;
extern crate serde;
extern crate serde_yaml;
extern crate walkdir;

#[macro_use]
extern crate serde_derive;

use clap::{App, SubCommand};
use rayon::prelude::*;

mod config;
mod file_scanner;
mod media_file_info;

use config::Config;
use media_file_info::MediaFileInfo;

fn scan_file(path: &String) -> Option<(&String, MediaFileInfo)> {
  if let Some(file_info) = MediaFileInfo::read_file(path) {
    Some((path, file_info))
  } else {
    None
  }
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
        .filter_map(|e| scan_file(e))
        .collect();

      for (file_name, info) in iter {
        println!("{}", file_name);
        println!("- {:?}", info);
      }
    }
  }
}
