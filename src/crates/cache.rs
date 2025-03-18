use std::{collections::HashMap, path::Path, sync::Arc};

use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;

const CRATE_CACHE_DIR: &str = "./.crates-lsp/plugins/crates-lsp/crates.io";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fetch {
    pub version: Option<Version>,
    #[serde(with = "time::serde::iso8601")]
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct CrateCache {
    crates: Arc<RwLock<HashMap<String, Fetch>>>,
}

impl Default for CrateCache {
    fn default() -> Self {
        std::fs::create_dir_all(CRATE_CACHE_DIR)
            .expect("Failed to create cargo crate version cache dir.");

        std::fs::write(Path::new(CRATE_CACHE_DIR).join(".gitignore"), "*")
            .expect("failed to create crates-lsp .gitignore file.");

        CrateCache {
            crates: Arc::new(RwLock::new(HashMap::default())),
        }
    }
}

pub enum CachedVersion {
    /// Crate was found, and a latest stable version was determined.
    Known(Version),

    /// The crate name is unknown, and does not exist in cache, nor
    /// do we know if the crate might be present in an upstream registry.
    Unknown,

    /// Crate was looked up in upstream registries, and was not found.
    DoesNotExist,
}

impl From<Option<Version>> for CachedVersion {
    fn from(value: Option<Version>) -> Self {
        match value {
            Some(version) => CachedVersion::Known(version),
            None => CachedVersion::DoesNotExist,
        }
    }
}

impl CrateCache {
    pub async fn get(&self, crate_name: &str) -> CachedVersion {
        // Check the in-memory cache first.
        if let Some(cached) = self.crates.read().await.get(crate_name).cloned() {
            // Only return the cached result if it is still valid.
            if OffsetDateTime::now_utc() < cached.expires_at {
                return cached.version.into();
            }
        };

        // Attempt to load crate informtion from file cache.
        if let Ok(content) =
            std::fs::read_to_string(std::path::Path::new(CRATE_CACHE_DIR).join(crate_name))
        {
            if let Ok(fetch) = serde_json::from_str::<Fetch>(&content) {
                if OffsetDateTime::now_utc() < fetch.expires_at {
                    self.put(crate_name, fetch.version.clone(), fetch.expires_at)
                        .await;

                    return fetch.version.into();
                }
            }
        }

        CachedVersion::Unknown
    }

    pub async fn put(
        &self,
        crate_name: &str,
        version: Option<Version>,
        expires_at: OffsetDateTime,
    ) {
        let fetch = Fetch {
            version,
            expires_at,
        };

        std::fs::write(
            Path::new(CRATE_CACHE_DIR).join(crate_name),
            serde_json::to_string(&fetch).as_deref().unwrap_or("{}"),
        )
        .unwrap();

        self.crates
            .write()
            .await
            .insert(crate_name.to_string(), fetch);
    }
}
