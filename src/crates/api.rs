use async_trait::async_trait;
use hyper::{client::HttpConnector, Body, Request};
use hyper_rustls::HttpsConnector;
use semver::Version;
use serde::Deserialize;

use super::{CrateError, CrateLookup};

type HyperClient = hyper::Client<HttpsConnector<HttpConnector>>;

#[derive(Debug, Clone)]
pub struct CrateApi {
    client: HyperClient,
}

#[async_trait]
impl CrateLookup for CrateApi {
    async fn get_latest_version(self, crate_name: String) -> Result<Version, CrateError> {
        let response = self
            .client
            .request(
                Request::builder()
                    .uri(&format!("https://crates.io/api/v1/crates/{crate_name}"))
                    .header(
                        "User-Agent",
                        "crates-lsp (github.com/MathiasPius/crates-lsp)",
                    )
                    .header("Accept", "application/json")
                    .body(Body::empty())
                    .map_err(CrateError::transport)?,
            )
            .await
            .map_err(CrateError::transport)?;

        let body = hyper::body::to_bytes(response.into_body())
            .await
            .map_err(CrateError::transport)?;

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
}

impl Default for CrateApi {
    fn default() -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_only()
            .enable_http1()
            .build();
        let client = hyper::Client::builder().build(https);

        CrateApi { client }
    }
}

#[cfg(test)]
mod tests {
    use crate::crates::{api::CrateApi, cache::CrateCache, CrateLookup};

    #[tokio::test]
    async fn get_common_crates() {
        let api = CrateApi::default();

        let cache = CrateCache::default();

        let versions = api
            .fetch_versions(cache, &["serde", "log", "tracing", "crate-does-not-exist"])
            .await;

        println!("{versions:#?}");
    }
}
