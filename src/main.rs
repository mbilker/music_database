extern crate clap;
extern crate mediainfo;
extern crate serde;
extern crate serde_yaml;
extern crate walkdir;

#[macro_use]
extern crate serde_derive;

use clap::{App, SubCommand};
use mediainfo::MediaInfo;
use std::fs::File;
use std::io::{BufReader, Read};

mod file_scanner;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
  paths: Vec<String>
}

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

    let file = File::open("config.yaml").unwrap();
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents).unwrap();
    
    let config: Config = serde_yaml::from_str(&contents).unwrap();
    println!("Config: {:?}", config);

    for path in config.paths {
      println!("Scanning {}", path);
      file_scanner::scan_dir(&path);
    }
  }
}
