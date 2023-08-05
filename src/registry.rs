use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use hyper::{client::HttpConnector, Body, Request};
use hyper_rustls::HttpsConnector;
use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::{mpsc, RwLock};

type HyperClient = hyper::Client<HttpsConnector<HttpConnector>>;

const CRATE_CACHE_DIR: &str = "./.cargo/crates-lsp-crate-cache";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fetch {
    pub version: Option<Version>,
    #[serde(with = "time::serde::iso8601")]
    pub timestamp: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct CrateApi {
    client: HyperClient,
    crates: Arc<RwLock<HashMap<String, Fetch>>>,
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

        #[derive(Deserialize)]
        struct CrateInner {
            pub max_stable_version: Version,
        }

        #[derive(Deserialize)]
        struct Crate {
            #[serde(rename = "crate")]
            pub inner: CrateInner,
        }

        let details: Crate =
            serde_json::from_str(&stringified).map_err(CrateError::Deserialization)?;

        Ok(details.inner.max_stable_version)
    }

    pub async fn fetch_versions(&self, crate_names: &[&str]) -> HashMap<String, Option<Version>> {
        let crate_names: HashSet<&str> = HashSet::from_iter(crate_names.iter().copied());

        let values = { self.crates.read().await.clone() };
        let known_crates = HashSet::from_iter(values.keys().map(|key| key.as_str()));

        let unknown_crates: Vec<_> = crate_names
            .difference(&known_crates)
            .map(ToString::to_string)
            .collect();

        if !unknown_crates.is_empty() {
            let (tx, mut rx) = mpsc::channel(crate_names.len());

            // This collects fetched version information, either from the local
            // file cache, or from the crates.io api itself.
            let mut fetched_versions = Vec::new();

            // Fetch information for unknown crates asynchronously.
            let mut dispatched_tasks = 0;
            for unknown_crate in unknown_crates {
                let client = self.client.clone();
                let tx = tx.clone();

                // Try reading from the on-disk cache first.
                if let Some(content) =
                    std::fs::read_to_string(Path::new(CRATE_CACHE_DIR).join(&unknown_crate)).ok()
                {
                    if let Ok(fetch) = serde_json::from_str::<Fetch>(&content) {
                        if (OffsetDateTime::now_utc() - fetch.timestamp).whole_days() > 0 {
                            fetched_versions.push((unknown_crate, fetch));
                            continue;
                        }
                    }
                }

                dispatched_tasks += 1;
                tokio::spawn(async move {
                    match CrateApi::get_latest_version(client, unknown_crate.clone()).await {
                        Ok(version) => tx.send((unknown_crate, Some(version))).await,
                        Err(_) => tx.send((unknown_crate, None)).await,
                    }
                });
            }

            // Collect all the results into a single vector.

            for _ in 0..dispatched_tasks {
                let Some((name, version)) = rx.recv().await else {
                    break;
                };

                let fetch = Fetch {
                    version,
                    timestamp: OffsetDateTime::now_utc(),
                };

                std::fs::write(Path::new(CRATE_CACHE_DIR).join("test"), "hello").unwrap();

                std::fs::write(
                    Path::new(CRATE_CACHE_DIR).join(&name),
                    serde_json::to_string(&fetch).as_deref().unwrap_or("{}"),
                )
                .unwrap();

                fetched_versions.push((name, fetch));
            }

            // Commit the updated versions to our crates hashmap.
            let mut crates = self.crates.write().await;
            crates.extend(fetched_versions.into_iter());

            // Clone the entire hashmap, instead of keeping the lock.
            crates
                .iter()
                .filter(|(name, _)| crate_names.contains(name.as_str()))
                .map(|(name, fetch)| (name.to_owned(), fetch.version.clone()))
                .collect()
        } else {
            self.crates
                .read()
                .await
                .iter()
                .filter(|(name, _)| crate_names.contains(name.as_str()))
                .map(|(name, fetch)| (name.to_owned(), fetch.version.clone()))
                .collect()
        }
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

        std::fs::create_dir_all(CRATE_CACHE_DIR)
            .expect("Failed to create cargo crate version cache dir.");

        CrateApi {
            client,
            crates: Arc::new(RwLock::new(HashMap::default())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::registry::CrateApi;

    #[tokio::test]
    async fn get_common_crates() {
        let api = CrateApi::default();

        let versions = api
            .fetch_versions(&["serde", "log", "tracing", "crate-does-not-exist"])
            .await;

        println!("{versions:#?}");
    }
}
