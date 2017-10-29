use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdArtist {
  pub id: String,
  pub name: String,
}
    
#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdRecording {
  pub duration: Option<i32>,
  pub title: String,
  pub id: Uuid,
  pub artists: Option<Vec<AcoustIdArtist>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdResult {
  pub recordings: Option<Vec<AcoustIdRecording>>,
  pub score: f32,
  pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct AcoustIdResponse {
  pub status: String,
  pub results: Vec<AcoustIdResult>,
}
