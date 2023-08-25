use std::{collections::HashMap, sync::Arc};

use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;

const CRATE_CACHE_DIR: &str = "./.lapce/plugins/crates-lsp/crates.io";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fetch {
    pub version: Option<Version>,
    #[serde(with = "time::serde::iso8601")]
    pub expires_at: OffsetDateTime,
}

pub struct CrateCache {
    crates: Arc<RwLock<HashMap<String, Fetch>>>,
}

impl CrateCache {
    pub async fn get(&self, crate_name: &str) -> Option<Version> {
        // Check the in-memory cache first.
        if let Some(cached) = self.crates.read().await.get(crate_name).cloned() {
            if OffsetDateTime::now_utc() < cached.expires_at {
                return cached.version;
            }
        };

        // Attempt to load crate informtion from file cache.
        if let Ok(content) =
            std::fs::read_to_string(std::path::Path::new(CRATE_CACHE_DIR).join(&crate_name))
        {
            if let Ok(fetch) = serde_json::from_str::<Fetch>(&content) {
                if OffsetDateTime::now_utc() < fetch.expires_at {
                    self.put(crate_name, fetch.version.clone(), fetch.expires_at)
                        .await;

                    return fetch.version;
                }
            }
        }

        None
    }

    pub async fn put(
        &self,
        crate_name: &str,
        version: Option<Version>,
        expires_at: OffsetDateTime,
    ) {
        self.crates.write().await.insert(
            crate_name.to_string(),
            Fetch {
                version,
                expires_at,
            },
        );
    }
}
