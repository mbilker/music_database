extern crate clap;
extern crate mediainfo;
extern crate serde;
extern crate serde_yaml;
extern crate walkdir;

#[macro_use]
extern crate serde_derive;

use clap::{App, SubCommand};
use mediainfo::MediaInfo;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};

mod file_scanner;

// Struct representation of the YAML configuration file
#[derive(Serialize, Deserialize, Debug)]
struct Config {
  paths: Vec<String>
}

impl Config {
  fn read_configuration() -> Result<Self, String> {
    let file = match File::open("config.yaml") {
      Ok(f) => f,
      Err(err) => return Err(err.description().to_owned()),
    };

    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();

    if let Err(err) = buf_reader.read_to_string(&mut contents) {
      return Err(err.description().to_owned());
    }

    let config = match serde_yaml::from_str(&contents) {
      Ok(c) => c,
      Err(err) => return Err(format!("failed to parse yaml config: {:?}", err)),
    };
    
    Ok(config)
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

    let config = Config::read_configuration();
    println!("Config: {:?}", config);

    if let Ok(config) = config {
      for path in config.paths {
        println!("Scanning {}", path);
        file_scanner::scan_dir(&path);
      }
    } else if let Err(err) = config {
      println!("Error reading configuration: {:?}", err);
    }
  }
}
