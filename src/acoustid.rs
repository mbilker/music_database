use std::cmp::Ordering;
use std::io::Read;
use std::thread;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::Future;
use futures_cpupool::CpuPool;
use ratelimit;
use reqwest;
use serde_json;

use uuid::Uuid;

use fingerprint;

use basic_types::*;

static LOOKUP_URL: &'static str = "https://api.acoustid.org/v2/lookup";

pub struct AcoustId {
  api_key: String,
  ratelimit: Arc<Mutex<ratelimit::Handle>>,
  thread_pool: CpuPool,
}

impl AcoustId {
  pub fn new(api_key: String, thread_pool: CpuPool) -> Self {
    let mut limiter = ratelimit::Builder::new()
      .capacity(3)
      .quantum(3)
      .interval(Duration::new(1, 0)) // 3 requests every 1 second
      .build();
    let handle = limiter.make_handle();

    thread::spawn(move || limiter.run());

    let ratelimit = Arc::new(Mutex::new(handle));

    Self {
      api_key,
      ratelimit,
      thread_pool,
    }
  }

  fn handle_response(data: String) -> Result<Option<AcoustIdResult>, ProcessorError> {
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

  pub fn lookup(
    api_key: String,
    duration: f64,
    fingerprint: String,
    ratelimit: Arc<Mutex<ratelimit::Handle>>
  ) -> Result<Option<AcoustIdResult>, ProcessorError> {
    let url = format!("{base}?format=json&client={apiKey}&duration={duration:.0}&fingerprint={fingerprint}&meta=recordings",
      base=LOOKUP_URL,
      apiKey=api_key,
      duration=duration,
      fingerprint=fingerprint
    );

    let mut resp = {
      ratelimit.lock().unwrap().wait();

      try!(reqwest::get(&*url))
    };

    let mut content = String::new();
    try!(resp.read_to_string(&mut content));
    debug!("response: {}", content);

    Self::handle_response(content)
  }

  pub fn parse_file(&self, path: String) -> Box<Future<Item = Uuid, Error = ProcessorError> + Send> {
    let api_key = self.api_key.clone();
    let ratelimit = self.ratelimit.clone();

    let uuid = self.thread_pool.spawn_fn(move || {
      // Eat up fingerprinting errors, I mostly see them when a file is not easily
      // parsed like WAV files
      let (duration, fingerprint) = try!(fingerprint::get(&path));

      let result = try!(Self::lookup(api_key, duration, fingerprint, ratelimit));
      if let Some(result) = result {
       if let Some(recordings) = result.recordings {
          let first = recordings.first().unwrap();
          return Ok(first.id.clone());
        }
      }

      Err(ProcessorError::NoFingerprintMatch)
    });

    Box::new(uuid)
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
