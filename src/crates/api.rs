use async_trait::async_trait;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;

use super::{default_client, CrateError, CrateLookup};

#[derive(Debug, Clone)]
pub struct CrateApi {
    client: Client,
}

#[async_trait]
impl CrateLookup for CrateApi {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn get_latest_version(self, crate_name: String) -> Result<Version, CrateError> {
        let response = self
            .client
            .get(&format!("https://crates.io/api/v1/crates/{crate_name}"))
            .send()
            .await
            .map_err(CrateError::transport)?;

        #[derive(Deserialize)]
        struct CrateInner {
            pub max_stable_version: Version,
        }

        #[derive(Deserialize)]
        struct Crate {
            #[serde(rename = "crate")]
            pub inner: CrateInner,
        }
        let details: Crate = response.json().await?;

        Ok(details.inner.max_stable_version)
    }
}

impl Default for CrateApi {
    fn default() -> Self {
        CrateApi {
            client: default_client(),
        }
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
