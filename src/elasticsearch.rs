use elastic::client::{AsyncClientBuilder, AsyncClient};
use elastic::client::requests::IndicesExistsRequest;
use elastic::client::responses::{AsyncResponseBuilder, CommandResponse};
use elastic::prelude::DocumentType;
use elastic::Error as ElasticError;
use futures::Future;
use futures::future;
use futures_cpupool::CpuPool;
use serde_json::Value;
use tokio_core::reactor::Handle;

use models::MediaFileInfoDocument;

static INDEX_NAME: &'static str = "music_card_catalog";

pub struct ElasticSearch {
  client: AsyncClient,
}

impl ElasticSearch {
  pub fn new(pool: CpuPool, handle: Handle) -> Self {
    let client = AsyncClientBuilder::new()
      .serde_pool(pool)
      .build(&handle.clone())
      .unwrap();

    Self {
      client,
    }
  }

  // Ensure the index exists by creating the index if it does not exist
  //
  // (Why do I have to create separate inner functions to get the type checker
  // figure out this?)
  pub fn ensure_index_exists(&self) -> impl Future<Item = (), Error = ()> + 'static {
    // Create the index
    fn create_index(client: AsyncClient) -> impl Future<Item = (), Error = ()> {
      info!("Elasticsearch index does not exist, creating index");

      client.index_create(INDEX_NAME.into())
        .body(ElasticSearch::body())
        .send()
        .map(|res| {
          info!("Index created with response: {:?}", res);
        })
        .map_err(|err| {
          error!("Index creation failed with error: {:#?}", err);
        })
    }

    // Handle other response codes when the response code is not 200 or 404
    fn handle_other_response(exists: AsyncResponseBuilder) -> impl Future<Item = (), Error = ()> {
      exists.into_response::<CommandResponse>()
        .map(|res| {
          info!("handle_other_response res: {:#?}", res);
        })
        .map_err(|err| {
          error!("handle_other_response err: {:#?}", err);
        })
    }

    // Clone of the client for capture in the closure
    let client = self.client.clone();

    // Create the request to check the existance of the index
    self.client
      .request(IndicesExistsRequest::for_index(INDEX_NAME))
      .send()
      .map_err(|err| {
        error!("ensure_index_exists err: {:#?}", err);
      })
      // TODO(mbilker): does the `and_then` get called if there is a `map_err`
      // before it?
      .and_then(|exists| -> Box<Future<Item = (), Error = ()>> {
        // Only create the index on 404 and print out details for non-200
        // response codes
        match exists.status() {
          200 => Box::new(future::ok(())),
          404 => Box::new(create_index(client)),
            _ => Box::new(handle_other_response(exists)),
        }
      })
  }

  fn body() -> Value {
    json!({
      "mappings": {
        "files": MediaFileInfoDocument::index_mapping()
      }
    })
  }

  pub fn insert_document(&self) -> impl Future<Item = (), Error = ElasticError> {
    self.client
      .document_put_mapping::<MediaFileInfoDocument>(INDEX_NAME.into())
      .send()
      .and_then(|res| {
        debug!("insert_document res: {:#?}", res);

        Ok(())
      })
  }
}
