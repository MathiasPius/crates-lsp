use std::{collections::HashMap, path::PathBuf, sync::Arc};

use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fetch {
    pub version: Option<Version>,
    #[serde(with = "time::serde::iso8601")]
    pub expires_at: OffsetDateTime,
}

#[derive(Default, Debug, Clone)]
pub struct CrateCache {
    directory: Arc<RwLock<PathBuf>>,
    crates: Arc<RwLock<HashMap<String, Fetch>>>,
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
    #[cfg(test)]
    pub async fn new(directory: PathBuf) -> Self {
        let cache = CrateCache::default();
        cache.change_directory(directory).await;
        cache
    }
    pub async fn change_directory(&self, directory: PathBuf) {
        std::fs::create_dir_all(&directory).unwrap_or_else(|_| {
            panic!("Failed to create crates-lsp cache directory: {directory:?}")
        });

        // This directory might be within a git repository, so make sure it is ignored.
        std::fs::write(directory.join(".gitignore"), "*")
            .expect("failed to create crates-lsp .gitignore file.");

        *self.directory.write().await = directory
    }

    pub async fn get(&self, crate_name: &str) -> CachedVersion {
        // Check the in-memory cache first.
        if let Some(cached) = self.crates.read().await.get(crate_name).cloned() {
            // Only return the cached result if it is still valid.
            if OffsetDateTime::now_utc() < cached.expires_at {
                return cached.version.into();
            }
        };

        // Attempt to load crate informtion from file cache.
        if let Ok(content) = std::fs::read_to_string(self.directory.read().await.join(crate_name)) {
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
            self.directory.read().await.join(crate_name),
            serde_json::to_string(&fetch).as_deref().unwrap_or("{}"),
        )
        .unwrap();

        self.crates
            .write()
            .await
            .insert(crate_name.to_string(), fetch);
    }
}
