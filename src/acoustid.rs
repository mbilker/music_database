use reqwest;
use serde_json;

use std::cmp::Ordering;
use std::io::Read;

static LOOKUP_URL: &'static str = "https://api.acoustid.org/v2/lookup";

#[derive(Clone, Debug, Deserialize)]
struct AcoustIdArtist {
  id: String,
  name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AcoustIdRecording {
  duration: Option<i32>,
  title: String,
  id: String,
  artists: Option<Vec<AcoustIdArtist>>,
}

#[derive(Clone, Debug, Deserialize)]
struct AcoustIdResult {
  recordings: Option<Vec<AcoustIdRecording>>,
  score: f32,
  id: String,
}

#[derive(Debug, Deserialize)]
struct AcoustIdResponse {
  status: String,
  results: Vec<AcoustIdResult>,
}

fn handle_response(data: &str) -> Option<AcoustIdResult> {
  let v: AcoustIdResponse = serde_json::from_str(data).unwrap();
  println!("v: {:#?}", v);

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

  let first_result = results.first().unwrap();
  Some(first_result.clone())
}

pub fn lookup(api_key: &str, duration: f64, fingerprint: &str) {
  let url = format!("{base}?format=json&client={apiKey}&duration={duration:.0}&fingerprint={fingerprint}&meta=recordings",
    base=LOOKUP_URL,
    apiKey=api_key,
    duration=duration,
    fingerprint=fingerprint
  );

  let mut resp = reqwest::get(&*url).unwrap();
  
  let mut content = String::new();
  resp.read_to_string(&mut content).unwrap();

  println!("response: {}", content);
  
  let first_result = handle_response(&*content);
  println!("top result: {:#?}", first_result);
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
