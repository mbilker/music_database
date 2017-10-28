use reqwest;
use serde_json;

use std::cmp::Ordering;
use std::io::Read;

static LOOKUP_URL: &'static str = "https://api.acoustid.org/v2/lookup";

static API_KEY: &'static str = "";

#[derive(Debug, Deserialize)]
struct AcoustIdArtist {
  id: String,
  name: String,
}

#[derive(Debug, Deserialize)]
struct AcoustIdRecording {
  duration: i32,
  title: String,
  id: String,
  artists: Option<Vec<AcoustIdArtist>>,
}

#[derive(Debug, Deserialize)]
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

pub fn lookup(duration: f64, fingerprint: &String) {
  let url = format!("{base}?format=json&client={apiKey}&duration={duration:.0}&fingerprint={fingerprint}&meta=recordings",
    base=LOOKUP_URL,
    apiKey=API_KEY,
    duration=duration,
    fingerprint=fingerprint
  );

  let mut resp = reqwest::get(&*url).unwrap();
  
  let mut content = String::new();
  resp.read_to_string(&mut content).unwrap();

  println!("response: {}", content);

  let v: AcoustIdResponse = serde_json::from_str(&*content).unwrap();
  println!("v: {:?}", v);
  
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
  let first_result = results.first();
  println!("top result: {:?}", first_result);
}
