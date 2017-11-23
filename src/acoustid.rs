use std::cell::RefCell;
use std::cmp::Ordering;
use std::io::Read;
use std::thread;
use std::time::Duration;

use ratelimit;
use reqwest;
use serde_json;

use uuid::Uuid;

use fingerprint;

use basic_types::*;

static LOOKUP_URL: &'static str = "https://api.acoustid.org/v2/lookup";

pub struct AcoustId {
  api_key: String,
  ratelimit: RefCell<ratelimit::Handle>,
}

impl AcoustId {
  pub fn new(api_key: String) -> Self {
    let mut limiter = ratelimit::Builder::new()
      .capacity(3)
      .quantum(3)
      .interval(Duration::new(1, 0)) // 3 requests every 1 second
      .build();
    let handle = limiter.make_handle();

    thread::spawn(move || limiter.run());

    Self {
      api_key: api_key,
      ratelimit: RefCell::new(handle),
    }
  }

  fn handle_response(&self, data: String) -> Result<Option<AcoustIdResult>, ProcessorError> {
    let v: AcoustIdResponse = try!(serde_json::from_str(&data));
    debug!("v: {:?}", v);

    let mut results: Vec<AcoustIdResult> = v.results;
    results.sort_by(|a, b| {
      if b.score > a.score {
        Ordering::Greater
      } else if b.score < a.score {
        Ordering::Less
      } else {
        Ordering::Equal
      }
    });

    if let Some(first_result) = results.first() {
      debug!("top result: {:?}", first_result);

      Ok(Some(first_result.clone()))
    } else {
      Ok(None)
    }
  }

  pub fn lookup(&self, duration: f64, fingerprint: &str) -> Result<Option<AcoustIdResult>, ProcessorError> {
    let url = format!("{base}?format=json&client={apiKey}&duration={duration:.0}&fingerprint={fingerprint}&meta=recordings",
      base=LOOKUP_URL,
      apiKey=self.api_key,
      duration=duration,
      fingerprint=fingerprint
    );

    let mut resp = {
      self.ratelimit.borrow_mut().wait();

      try!(reqwest::get(&*url))
    };

    let mut content = String::new();
    try!(resp.read_to_string(&mut content));
    debug!("response: {}", content);

    self.handle_response(content)
  }

  pub fn parse_file(&self, path: &str) -> Result<Uuid, ProcessorError> {
    // Eat up fingerprinting errors, I mostly see them when a file is not easily
    // parsed like WAV files
    let (duration, fingerprint) = try!(fingerprint::get(&path));

    let result = try!(self.lookup(duration, &fingerprint));
    if let Some(result) = result {
     if let Some(recordings) = result.recordings {
        let first = recordings.first().unwrap();
        return Ok(first.id.clone());
      }
    }

    Err(ProcessorError::NoFingerprintMatch())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_handle_response() {
    let json = r#"{
      "status": "ok",
      "results": [
        {
          "recordings": [
            {
              "title": "フラワリングナイト",
              "id": "bdf27e74-cc62-43ae-8eb8-2b40d5c421a5",
              "artists": [
                {
                  "id": "9f9a5476-22bd-48ef-8952-25cd8e3f1545",
                  "name": "TAMUSIC"
                }
              ]
            }
          ],
          "score": 0.999473,
          "id": "f2451269-9fec-4e82-aaf8-0bdf1f069ecf"
        }
      ]
    }"#;
    
    let first_result = handle_response(json).unwrap();
    assert_eq!(first_result.id, "f2451269-9fec-4e82-aaf8-0bdf1f069ecf");
  }
}
