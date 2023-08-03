use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use hyper::{client::HttpConnector, Body, Request};
use hyper_rustls::HttpsConnector;
use semver::Version;
use serde::Deserialize;
use tokio::sync::{mpsc, RwLock};

use super::CrateRegistry;

type HyperClient = hyper::Client<HttpsConnector<HttpConnector>>;

#[derive(Debug, Clone)]
pub struct CrateApi {
    client: HyperClient,
    crates: Arc<RwLock<HashMap<String, Option<Version>>>>,
}

#[derive(Deserialize)]
struct CrateInner {
    pub max_stable_version: Version,
}

#[derive(Deserialize)]
struct Crate {
    #[serde(rename = "crate")]
    pub inner: CrateInner,
}

#[derive(Debug)]
enum CrateError {
    Http(hyper::http::Error),
    Hyper(hyper::Error),
    Deserialization(serde_json::Error),
}

impl CrateApi {
    async fn get_latest_version(
        client: HyperClient,
        crate_name: String,
    ) -> Result<Version, CrateError> {
        let response = client
            .request(
                Request::builder()
                    .uri(&format!("https://crates.io/api/v1/crates/{crate_name}"))
                    .header(
                        "User-Agent",
                        "crates-lsp (github.com/MathiasPius/crates-lsp)",
                    )
                    .header("Accept", "application/json")
                    .body(Body::empty())
                    .map_err(CrateError::Http)?,
            )
            .await
            .map_err(CrateError::Hyper)?;

        let body = hyper::body::to_bytes(response.into_body())
            .await
            .map_err(CrateError::Hyper)?;

        let stringified = String::from_utf8_lossy(&body);

        let details: Crate =
            serde_json::from_str(&stringified).map_err(CrateError::Deserialization)?;

        Ok(details.inner.max_stable_version)
    }
}

impl Default for CrateApi {
    fn default() -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_only()
            .enable_http1()
            .build();
        let client = hyper::Client::builder().build(https);

        CrateApi {
            client,
            crates: Arc::new(RwLock::new(HashMap::default())),
        }
    }
}

#[tower_lsp::async_trait]
impl CrateRegistry for CrateApi {
    async fn fetch_versions(&self, crate_names: &[&str]) -> HashMap<String, Option<Version>> {
        let crate_names: HashSet<&str> = HashSet::from_iter(crate_names.iter().copied());

        let values = { self.crates.read().await.clone() };
        let known_crates = HashSet::from_iter(values.keys().map(|key| key.as_str()));

        let unknown_crates: Vec<_> = crate_names
            .difference(&known_crates)
            .map(ToString::to_string)
            .collect();

        if !unknown_crates.is_empty() {
            let (tx, mut rx) = mpsc::channel(crate_names.len());

            // Fetch information for unknown crates asynchronously.
            for unknown_crate in unknown_crates {
                let client = self.client.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    match CrateApi::get_latest_version(client, unknown_crate.clone()).await {
                        Ok(version) => tx.send((unknown_crate, Some(version))).await,
                        Err(_) => tx.send((unknown_crate, None)).await,
                    }
                });
            }

            // Collect all the results into a single vector.
            let mut fetched_versions = Vec::new();
            for _ in 0..crate_names.len() {
                let Some((name, version)) = rx.recv().await else {
                    break;
                };

                fetched_versions.push((name, version));
            }

            // Commit the updated versions to our crates hashmap.
            let mut lock = self.crates.write().await;
            lock.extend(fetched_versions.into_iter());

            // Clone the entire hashmap, instead of keeping the lock.
            lock.clone()
        } else {
            self.crates.read().await.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::registry::{api::CrateApi, CrateRegistry};

    #[tokio::test]
    async fn get_common_crates() {
        let api = CrateApi::default();

        let versions = api
            .fetch_versions(&["serde", "log", "tracing", "crate-does-not-exist"])
            .await;

        println!("{versions:#?}");
    }
}
