use async_trait::async_trait;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;

use super::{default_client, CrateError, CrateLookup};

#[derive(Debug, Clone)]
pub struct CrateIndex {
    client: Client,
}

#[async_trait]
impl CrateLookup for CrateIndex {
    fn client(&self) -> &Client {
        &self.client
    }

    async fn get_latest_version(self, crate_name: String) -> Result<Version, CrateError> {
        let crate_index_path = match crate_name.len() {
            0 => return Err(CrateError::InvalidCrateName(crate_name)),
            1 => format!("1/{crate_name}"),
            2 => format!("2/{crate_name}"),
            3 => format!("3/{}/{crate_name}", &crate_name[0..1]),
            _ => format!("{}/{}/{crate_name}", &crate_name[0..2], &crate_name[2..4]),
        };

        let response = self
            .client
            .get(format!("https://index.crates.io/{crate_index_path}"))
            .send()
            .await
            .map_err(CrateError::transport)?;

        let stringified = response.text().await?;

        let mut all_releases = Vec::new();
        for line in stringified.lines() {
            #[derive(Deserialize)]
            struct CrateVersion {
                pub vers: Version,
                pub yanked: bool,
            }

            let version: CrateVersion =
                serde_json::from_str(line).map_err(CrateError::Deserialization)?;

            all_releases.push(version);
        }

        let unyanked_versions: Vec<_> = all_releases
            .into_iter()
            .filter(|release| !release.yanked)
            .map(|release| release.vers)
            .collect();

        // Try to find the latest non-prerelease version first, falling back to whichever
        // latest pre-release version is available.
        unyanked_versions
            .iter()
            .filter(|version| version.pre.is_empty())
            .max()
            .or(unyanked_versions.iter().max())
            .cloned()
            .ok_or(CrateError::NoVersionsFound)
    }
}

impl Default for CrateIndex {
    fn default() -> Self {
        CrateIndex {
            client: default_client(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::crates::{cache::CrateCache, sparse::CrateIndex, CrateLookup};

    #[tokio::test]
    async fn get_common_crates() {
        let api = CrateIndex::default();

        let cache = CrateCache::new(PathBuf::from("/tmp/crates-lsp")).await;

        let versions = api
            .fetch_versions(cache, &["serde", "log", "tracing", "crate-does-not-exist"])
            .await;

        println!("{versions:#?}");
    }
}
