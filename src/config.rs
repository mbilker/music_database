use serde_yaml;

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Read};

// Struct representation of the YAML configuration file
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
  pub api_keys: BTreeMap<String, String>,
  pub paths: Vec<String>,
}

impl Config {
  pub fn read_configuration() -> Result<Self, String> {
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
