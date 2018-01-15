use std::cell::RefCell;
use std::cmp::Ordering;
use std::thread;
use std::rc::Rc;
use std::time::Duration;

use futures::{Future, Stream};
use futures_cpupool::CpuPool;
use futures_ratelimit::RatelimitFuture;
use hyper::{Chunk, Client};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use ratelimit;
use serde_json;
use tokio_core::reactor::Handle;
use uuid::Uuid;

use fingerprint;

use basic_types::*;

static LOOKUP_URL: &'static str = "https://api.acoustid.org/v2/lookup";

pub struct AcoustId {
  api_key: String,
  client: Rc<Client<HttpsConnector<HttpConnector>>>,
  ratelimit: Rc<RefCell<ratelimit::Handle>>,

  thread_pool: CpuPool,
}

impl AcoustId {
  pub fn new(api_key: String, thread_pool: CpuPool, handle: &Handle) -> Self {
    let mut limiter = ratelimit::Builder::new()
      .capacity(3)
      .quantum(3)
      .interval(Duration::new(1, 0)) // 3 requests every 1 second
      .build();
    let limiter_handle = limiter.make_handle();

    thread::spawn(move || limiter.run());

    let client = Client::configure()
      .connector(HttpsConnector::new(4, handle).unwrap())
      .build(handle);

    Self {
      api_key,
      client: Rc::new(client),
      ratelimit: Rc::new(RefCell::new(limiter_handle)),

      thread_pool,
    }
  }

  fn handle_response(data: &Chunk) -> Result<AcoustIdResult, ProcessorError> {
    let v: AcoustIdResponse = serde_json::from_slice(data).map_err(|e| {
      ProcessorError::from(e)
    })?;
    debug!("v: {:?}", v);

    let mut results = match v.results {
      Some(v) => v,
      None => return Err(ProcessorError::NoFingerprintMatch),
    };

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

      Ok(first_result.clone())
    } else {
      Err(ProcessorError::NoFingerprintMatch)
    }
  }

  pub fn lookup(
    api_key: &str,
    duration: f64,
    fingerprint: &str,
    client: &Rc<Client<HttpsConnector<HttpConnector>>>,
    handle: &ratelimit::Handle
  ) -> impl Future<Item = AcoustIdResult, Error = ProcessorError> {
    let url = format!("{base}?format=json&client={apiKey}&duration={duration:.0}&fingerprint={fingerprint}&meta=recordings",
      base=LOOKUP_URL,
      apiKey=api_key,
      duration=duration,
      fingerprint=fingerprint
    ).parse().unwrap();

    let client = Rc::clone(client);

    RatelimitFuture::new(handle.clone())
      .map_err(|_| ProcessorError::NothingUseful)
      .and_then(move |_| {
        client.get(url)
          .map_err(ProcessorError::from)
      })
      .and_then(|res| {
        res.body()
          .concat2()
          .map_err(ProcessorError::from)
          .and_then(move |body| Self::handle_response(&body))
      })
  }

  pub fn parse_file(&self, path: String) -> impl Future<Item = Option<Uuid>, Error = ProcessorError> {
    let api_key = self.api_key.clone();
    let client = Rc::clone(&self.client);
    let ratelimit = self.ratelimit.borrow().clone();

    let path2 = path.clone();

    self.thread_pool.spawn_fn(move || {
      // Eat up fingerprinting errors, I mostly see them when a file is not easily
      // parsed like WAV files
      fingerprint::get(&path)
    }).and_then(move |(duration, fingerprint)| {
      Self::lookup(&api_key, duration, &fingerprint, &client, &ratelimit)
    }).and_then(|result| {
     if let Some(recordings) = result.recordings {
        let first = recordings.first().unwrap();
        return Ok(Some(first.id));
      }

      Ok(None)
    }).or_else(move |e| match e {
      ProcessorError::NoAudioStream => {
        error!("path: {}, weird case with no audio stream during fingerprinting (bad extension?)", path2);
        Ok(None)
      },
      ProcessorError::NoFingerprintMatch => Ok(None),
      _ => Err(e),
    })
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
